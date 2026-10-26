//! TS components: registration as dynamic ECS components with native layouts, identity across
//! generations, migration by field name, and undeclared components.

use crate::backend::ScriptError;
use crate::commands::{Writable, insert_owned};
use crate::heap::{HeapBytes, HeapFill, HeapKind};
use crate::layout::{ComponentLayout, FieldType, NativeLayouts};
use crate::manifest::{ComponentDecl, RelationshipRole};
use crate::relationships::{self, Role, TsHooks};
use crate::value::{self, ValueError};
use bevy::ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Where a TS component stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentState {
    /// Declared by the live generation.
    Live,
    /// No longer declared; its data stays on its entities.
    Undeclared,
    /// Replaced by a new layout; hidden and never matched again.
    Retired,
}

/// One registered TS component id.
#[derive(Clone, Debug)]
pub struct TsComponent {
    pub id: ComponentId,
    pub decl: ComponentDecl,
    pub layout: ComponentLayout,
    pub state: ComponentState,
    /// For a component with a heap field: what a command builds its values from.
    pub fill: Option<Arc<HeapFill>>,
}

/// A migration performed while applying a generation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Migration {
    pub component: String,
    pub entities: usize,
    /// Fields copied from the old layout.
    pub kept: Vec<String>,
    /// Fields new in this layout, filled with their default.
    pub added: Vec<String>,
    /// Fields whose type changed, reset to their default.
    pub reset: Vec<String>,
    /// Fields that no longer exist.
    pub removed: Vec<String>,
}

impl std::fmt::Display for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} migrated on {} entities", self.component, self.entities)?;
        for (label, list) in [("added", &self.added), ("reset", &self.reset), ("removed", &self.removed)] {
            if !list.is_empty() {
                write!(f, "; {label} {}", list.join(", "))?;
            }
        }
        Ok(())
    }
}

/// Every TS component id ever registered in this world.
#[derive(Resource, Default, Debug)]
pub struct TsComponents {
    entries: Vec<TsComponent>,
    /// Name to the index of its current (live or undeclared) entry.
    current: HashMap<String, usize>,
    /// Ids registered by generations that failed, kept for the next attempt with that layout.
    spares: Vec<(String, ComponentLayout, ComponentId)>,
}

impl TsComponents {
    /// The current entry for a name: live or undeclared, never retired.
    pub fn get(&self, name: &str) -> Option<&TsComponent> {
        self.current.get(name).map(|i| &self.entries[*i])
    }

    /// A live component by name.
    pub fn live(&self, name: &str) -> Option<&TsComponent> {
        self.get(name).filter(|c| c.state == ComponentState::Live)
    }

    pub fn by_id(&self, id: ComponentId) -> Option<&TsComponent> {
        self.entries.iter().find(|c| c.id == id)
    }

    /// Live and undeclared components.
    pub fn current(&self) -> impl Iterator<Item = &TsComponent> {
        self.current.values().map(|i| &self.entries[*i])
    }

    pub fn all(&self) -> &[TsComponent] {
        &self.entries
    }

    /// Encodes a field map into a live component's native bytes, defaults filled in.
    pub fn encode(&self, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<HeapBytes, ValueError> {
        let c = self.live(name).ok_or_else(|| ValueError {
            path: String::new(),
            expected: "a live TS component".into(),
            message: format!("no live component named {name}"),
        })?;
        refuse_target(&c.decl)?;
        value::encode_component(&c.decl, &c.layout, values)
    }

    /// Component ids TS may write with their sizes: live TS components and native layouts.
    pub fn writable_sizes(&self, native: &NativeLayouts) -> HashMap<ComponentId, Writable> {
        let mut m: HashMap<_, _> = self
            .current()
            .filter(|c| c.state == ComponentState::Live && !c.decl.is_target())
            .map(|c| (c.id, Writable { size: c.layout.size, heap: c.fill.clone() }))
            .collect();
        m.extend(native.iter().filter(|n| n.insertable).map(|n| (n.id, Writable { size: n.layout.size, heap: None })));
        m
    }
}

/// What each live TS component requires (`static require`): the parts its [`on_add`] inserts
/// when the entity lacks them, each a value with the given fields and the rest defaults, copied
/// deep per insert.
#[derive(Resource, Default, Debug)]
pub struct Required(HashMap<ComponentId, RequiredParts>);

/// A component's required parts: each id with its value.
type RequiredParts = Arc<[(ComponentId, HeapBytes)]>;

impl Required {
    /// The ids `id` requires directly.
    pub fn of(&self, id: ComponentId) -> impl Iterator<Item = ComponentId> + '_ {
        self.0.get(&id).into_iter().flat_map(|parts| parts.iter().map(|(r, _)| *r))
    }
}

/// The required-components part of every TS component's `on_add`, which
/// [`relationships::dispatch_on_add`] calls (the slot is registered by id in [`register`]). It inserts,
/// through the world's commands, each part the component requires that the entity lacks, so
/// they land at the next flush, before the next system. Each insert runs its own `on_add`, so
/// requirements apply transitively; a part the entity has keeps its value.
pub fn on_add(mut world: DeferredWorld, ctx: HookContext) {
    let Some(parts) = world.get_resource::<Required>().and_then(|r| r.0.get(&ctx.component_id)).cloned() else { return };
    let e = ctx.entity;
    world.commands().queue(move |world: &mut World| {
        let Ok(mut em) = world.get_entity_mut(e) else { return };
        for (id, value) in parts.iter() {
            if !em.contains_id(*id) {
                // SAFETY: `value` was encoded for `id`: a TS component or an insertable native
                // layout. Each insert takes its own copy of the heap values.
                unsafe { insert_owned(&mut em, *id, value.duplicate()) };
            }
        }
    });
}

/// A cycle among the declarations' `require`s, as the names around it.
fn require_cycle(decls: &[ComponentDecl]) -> Option<Vec<String>> {
    let by_name: HashMap<&str, &ComponentDecl> = decls.iter().map(|d| (d.name.as_str(), d)).collect();
    // 0: unseen, 1: on the path, 2: done.
    let mut mark: HashMap<&str, u8> = HashMap::new();
    fn visit<'a>(n: &'a str, by_name: &HashMap<&str, &'a ComponentDecl>, mark: &mut HashMap<&'a str, u8>, path: &mut Vec<&'a str>) -> Option<Vec<String>> {
        match mark.get(n) {
            Some(2) => return None,
            Some(1) => {
                let from = path.iter().position(|p| *p == n).unwrap_or(0);
                return Some(path[from..].iter().chain(std::iter::once(&n)).map(|s| s.to_string()).collect());
            }
            _ => {}
        }
        let d = by_name.get(n)?;
        mark.insert(n, 1);
        path.push(n);
        for r in &d.require {
            if let Some(c) = visit(r.name.as_str(), by_name, mark, path) {
                return Some(c);
            }
        }
        path.pop();
        mark.insert(n, 2);
        None
    }
    for d in decls {
        if let Some(c) = visit(d.name.as_str(), &by_name, &mut mark, &mut Vec::new()) {
            return Some(c);
        }
    }
    None
}

/// A relationship target is derived from its sources: no path builds one from values.
fn refuse_target(d: &ComponentDecl) -> Result<(), ValueError> {
    match d.is_target() {
        true => Err(ValueError {
            path: String::new(),
            expected: "a component that can be inserted".into(),
            message: format!("{} is a relationship target: its list is kept from its sources, never inserted", d.name),
        }),
        false => Ok(()),
    }
}

/// Validates a generation's component declarations without touching the world.
pub fn validate(decls: &[ComponentDecl], native: &NativeLayouts) -> Vec<ScriptError> {
    let mut errors = Vec::new();
    if let Some(cycle) = require_cycle(decls) {
        let first = decls.iter().find(|d| d.name == cycle[0]).and_then(|d| d.location.clone());
        errors.push(ScriptError::at(format!("static require makes a cycle: {}", cycle.join(" requires ")), first));
    }
    let mut seen = HashMap::new();
    for d in decls {
        let at = d.location.clone();
        if d.name.is_empty() {
            errors.push(ScriptError::at("a component needs a name", at.clone()));
        }
        if native.get(&d.name).is_some() {
            errors.push(ScriptError::at(format!("{} is a Rust component name; pick a path such as game::{}", d.name, d.name), at.clone()));
        }
        if seen.insert(d.name.as_str(), ()).is_some() {
            errors.push(ScriptError::at(format!("component {} is declared twice", d.name), at.clone()));
        }
        let mut field_names = HashMap::new();
        for f in &d.fields {
            if field_names.insert(f.name.as_str(), ()).is_some() {
                errors.push(ScriptError::at(format!("{}.{} is declared twice", d.name, f.name), at.clone()));
            }
            if let Err(e) = f.ty.validate() {
                errors.push(ScriptError::at(format!("{}.{}: {e}", d.name, f.name), at.clone()));
            }
            if let Some(default) = &f.default {
                let mut scratch = vec![0u8; f.ty.size()];
                if let Err(e) = value::encode_field(&format!(".{}", f.name), &f.ty, default, &mut scratch) {
                    errors.push(ScriptError::at(format!("{}: default {e}", d.name), at.clone()));
                }
            }
        }
    }
    errors.extend(validate_pairs(decls));
    errors
}

/// Relationship pairs: a source's first field is `target: Entity`, a target has no fields and
/// names a source, and each source has exactly one target (custom-relationships decision 3).
fn validate_pairs(decls: &[ComponentDecl]) -> Vec<ScriptError> {
    let mut errors = Vec::new();
    let by_name: HashMap<&str, &ComponentDecl> = decls.iter().map(|d| (d.name.as_str(), d)).collect();
    let mut targets: HashMap<&str, Vec<&ComponentDecl>> = HashMap::new();
    for d in decls {
        let at = d.location.clone();
        let Some(r) = &d.relationship else { continue };
        match r.role {
            RelationshipRole::Source => {
                if !d.fields.first().is_some_and(|f| f.name == "target" && f.ty == FieldType::Entity) {
                    errors.push(ScriptError::at(format!("{}: a Relationship's first field is target: Entity", d.name), at));
                }
            }
            RelationshipRole::Target => {
                if let Some(f) = d.fields.first() {
                    errors.push(ScriptError::at(format!("{}: a RelationshipTarget has no fields; {} is one", d.name, f.name), at.clone()));
                }
                match r.counterpart.as_deref() {
                    Some(s) if by_name.get(s).is_some_and(|s| s.is_source()) => targets.entry(s).or_default().push(d),
                    Some(s) => errors.push(ScriptError::at(format!("{}: RelationshipTarget({s}) names a class that is not a Relationship", d.name), at)),
                    None => errors.push(ScriptError::at(format!("{}: RelationshipTarget(...) names no source", d.name), at)),
                }
            }
        }
    }
    for d in decls.iter().filter(|d| d.is_source()) {
        match targets.get(d.name.as_str()).map(Vec::as_slice) {
            None | Some([]) => errors.push(ScriptError::at(format!("{}: no RelationshipTarget({}) names this Relationship; declare one", d.name, d.name), d.location.clone())),
            Some([_]) => {}
            Some(ts) => {
                let names: Vec<&str> = ts.iter().map(|t| t.name.as_str()).collect();
                errors.push(ScriptError::at(format!("{}: two targets name it ({}); a Relationship has one RelationshipTarget", d.name, names.join(", ")), ts[1].location.clone()));
            }
        }
    }
    errors
}

/// How a declaration registers: plain, or one half of a relationship pair.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Plain,
    /// `linked_spawn` is its target's.
    Source { linked_spawn: bool },
    /// `source` is the source's id, registered first.
    Target { source: ComponentId, allow_self_referential: bool },
}

fn register(world: &mut World, decl: &ComponentDecl, layout: &ComponentLayout, kind: Kind) -> ComponentId {
    let rel = decl.relationship.as_ref();
    let allow_self = rel.is_some_and(|r| r.allow_self_referential);
    let offset = layout.field("target").map_or(0, |f| f.offset);
    let descriptor = match kind {
        // A target is Rust-owned: a `Vec<Entity>` Bevy writes through its hooks.
        // SAFETY: the layout and drop fn are `Vec<Entity>`'s, and every value is built as one.
        Kind::Target { source, allow_self_referential } => unsafe {
            ComponentDescriptor::new_with_layout(
                decl.name.clone(),
                StorageType::Table,
                std::alloc::Layout::new::<Vec<Entity>>(),
                Some(drop_list),
                true,
                true,
                ComponentCloneBehavior::Custom(relationships::clone_target),
                Some(relationships::target_accessor(source, rel.is_some_and(|r| r.linked_spawn), allow_self_referential)),
            )
        },
        _ => {
            // A heap field's value is freed by the drop fn and deep-copied by the clone fn, both
            // reading the value's header; any other component is plain bytes. A source's clone
            // maps its `target` too.
            let source = matches!(kind, Kind::Source { .. });
            let (drop, clone): (Option<for<'a> unsafe fn(bevy::ptr::OwningPtr<'a>)>, _) = match (layout.has_heap(), source) {
                (true, false) => (Some(crate::heap::drop_component), ComponentCloneBehavior::Custom(crate::heap::clone_component)),
                (true, true) => (Some(crate::heap::drop_component), ComponentCloneBehavior::Custom(relationships::clone_source_heap)),
                (false, false) => (None, ComponentCloneBehavior::Custom(clone_bytes)),
                (false, true) => (None, ComponentCloneBehavior::Custom(relationships::clone_source)),
            };
            let accessor = match kind {
                Kind::Source { linked_spawn } => Some(relationships::source_accessor(offset, linked_spawn, allow_self)),
                _ => None,
            };
            // SAFETY: a plain TS component is bytes: any bit pattern is a valid value for every
            // field type's storage. One with a heap field is only ever built as a whole
            // `HeapBytes` value, which the drop fn frees. Both are Send + Sync. A source is
            // immutable, as `ChildOf` is: its hooks keep the target's list.
            unsafe { ComponentDescriptor::new_with_layout(decl.name.clone(), StorageType::Table, layout.std_layout(), drop, !source, true, clone, accessor) }
        }
    };
    let id = world.register_component_with_descriptor(descriptor);
    let hooks = world.register_component_hooks_by_id(id).expect("a TS component was just registered");
    hooks.on_add(relationships::dispatch_on_add);
    match kind {
        Kind::Plain => {}
        Kind::Source { .. } => {
            hooks.on_insert(relationships::dispatch_on_insert).on_discard(relationships::dispatch_on_discard);
        }
        Kind::Target { .. } => {
            hooks.on_discard(relationships::dispatch_on_discard);
            if rel.is_some_and(|r| r.linked_spawn) {
                hooks.on_despawn(relationships::dispatch_on_despawn);
            }
        }
    }
    let role = match kind {
        Kind::Plain => None,
        Kind::Source { linked_spawn } => Some(Role::Source { target: None, offset, linked_spawn, allow_self_referential: allow_self }),
        Kind::Target { source, .. } => Some(Role::Target { source, linked_spawn: rel.is_some_and(|r| r.linked_spawn) }),
    };
    if let Some(role) = role {
        let mut roles = world.get_resource_or_init::<TsHooks>();
        roles.set_role(id, role);
        if let Role::Target { source, .. } = role
            && let Some(Role::Source { offset, linked_spawn, allow_self_referential, .. }) = roles.role(source)
        {
            roles.set_role(source, Role::Source { target: Some(id), offset, linked_spawn, allow_self_referential });
        }
    }
    id
}

/// A target's drop fn: its `Vec<Entity>`.
///
/// # Safety
/// `p` owns a target's value.
unsafe fn drop_list(p: bevy::ptr::OwningPtr<'_>) {
    // SAFETY: the caller's contract.
    unsafe { p.drop_as::<Vec<Entity>>() };
}

/// Clones a TS component by copying its bytes (`cloneAndSpawn`): it is plain data.
fn clone_bytes(source: &bevy::ecs::entity::SourceComponent, ctx: &mut bevy::ecs::entity::ComponentCloneCtx) {
    // SAFETY: TS components are plain bytes with no drop, so a copy owns nothing twice.
    unsafe { ctx.write_target_component_ptr(source.ptr()) };
}

fn entities_with(world: &mut World, id: ComponentId) -> Vec<Entity> {
    let mut q = QueryBuilder::<Entity>::new(world).with_id(id).build();
    q.iter(world).collect()
}

/// Plans one component's migration from `old` to `new`.
fn plan(old: &TsComponent, decl: &ComponentDecl, layout: &ComponentLayout) -> Migration {
    let mut m = Migration { component: decl.name.clone(), entities: 0, kept: vec![], added: vec![], reset: vec![], removed: vec![] };
    for f in &layout.fields {
        match old.layout.field(&f.name) {
            Some(o) if o.ty == f.ty => m.kept.push(f.name.clone()),
            Some(_) => m.reset.push(f.name.clone()),
            None => m.added.push(f.name.clone()),
        }
    }
    for f in &old.layout.fields {
        if layout.field(&f.name).is_none() {
            m.removed.push(f.name.clone());
        }
    }
    m
}

fn default_bytes(decl: &ComponentDecl, layout: &ComponentLayout) -> HeapBytes {
    value::encode_component(decl, layout, &serde_json::Map::new()).expect("defaults were validated")
}

/// One declaration of a generation, matched against what the world already has.
#[derive(Clone, Debug)]
enum Planned {
    /// Same name and layout as the current entry at this index.
    Reuse(usize, ComponentId),
    /// A fresh id; `from` is the current entry to migrate from, if any.
    New { id: ComponentId, from: Option<usize> },
}

/// A generation's components, registered but not yet live: ids for new layouts exist, no data
/// has moved. Plugins look components up through it while the generation is checked.
#[derive(Clone, Debug, Default)]
pub struct PendingComponents {
    items: Vec<(ComponentDecl, ComponentLayout, Planned, ComponentId)>,
    /// Their `require`s, resolved by [`PendingComponents::resolve_required`].
    required: HashMap<ComponentId, RequiredParts>,
}

impl PendingComponents {
    /// A component of the pending generation by declared name.
    pub fn lookup(&self, name: &str) -> Option<(ComponentId, &ComponentLayout)> {
        self.items.iter().find(|(d, ..)| d.name == name).map(|(_, l, _, id)| (*id, l))
    }

    /// A pending component's declaration (fields, defaults, hints).
    pub fn decl(&self, name: &str) -> Option<&ComponentDecl> {
        self.items.iter().find(|(d, ..)| d.name == name).map(|(d, ..)| d)
    }

    /// Encodes a field map into a pending component's native bytes, defaults filled in.
    pub fn encode(&self, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<HeapBytes, ValueError> {
        let (d, l, ..) = self.items.iter().find(|(d, ..)| d.name == name).ok_or_else(|| ValueError {
            path: String::new(),
            expected: "a component of this generation".into(),
            message: format!("no component named {name}"),
        })?;
        refuse_target(d)?;
        value::encode_component(d, l, values)
    }

    /// Resolves each declaration's `require` to ids and bytes: a component of this generation,
    /// or a Rust component with an insertable native layout, with the given fields.
    pub fn resolve_required(&mut self, native: &NativeLayouts) -> Vec<ScriptError> {
        let mut errors = Vec::new();
        let mut out = HashMap::new();
        for (d, _, _, id) in &self.items {
            let mut parts = Vec::new();
            for r in &d.require {
                let part = if let Some((rid, _)) = self.lookup(&r.name) {
                    self.encode(&r.name, &r.values).map(|b| (rid, b)).map_err(|e| e.to_string())
                } else if let Some(n) = native.get(&r.name).filter(|n| n.insertable && !n.resource) {
                    native_bytes(n, &r.values).map(|b| (n.id, HeapBytes::plain(b)))
                } else if native.get(&r.name).is_some() || native.reflect_tier(&r.name).is_some() {
                    Err(format!("{} cannot be inserted from bytes", r.name))
                } else {
                    Err(format!("no component named {}", r.name))
                };
                match part {
                    Ok(p) => parts.push(p),
                    Err(why) => errors.push(ScriptError::at(format!("{} static require: {why}", d.name), d.location.clone())),
                }
            }
            if !parts.is_empty() {
                out.insert(*id, Arc::from(parts));
            }
        }
        self.required = out;
        errors
    }

    /// Every pending component: name, id, layout.
    pub fn iter(&self) -> impl Iterator<Item = (&str, ComponentId, &ComponentLayout)> {
        self.items.iter().map(|(d, l, _, id)| (d.name.as_str(), *id, l))
    }
}

/// Registers ids for a generation's declarations without moving data: unchanged layouts reuse
/// their id, changed or new ones get a fresh id (or a spare left by a failed generation).
/// Declarations must have passed [`validate`].
pub fn prepare(world: &mut World, decls: &[ComponentDecl]) -> PendingComponents {
    let mut registry = world.remove_resource::<TsComponents>().unwrap_or_default();
    let layouts: Vec<ComponentLayout> = decls.iter().map(ComponentDecl::layout).collect();
    // The entry each declaration may keep: same layout and same relationship role.
    let mut keep: Vec<Option<usize>> = decls
        .iter()
        .zip(&layouts)
        .map(|(d, l)| registry.current.get(&d.name).copied().filter(|&i| registry.entries[i].layout.same_shape(l) && registry.entries[i].decl.relationship == d.relationship))
        .collect();
    // A pair's ids name each other through their accessors, so if either half needs a new id
    // both get one (custom-relationships decision 8).
    let index: HashMap<&str, usize> = decls.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();
    let pair_of = |t: &ComponentDecl| t.relationship.as_ref().filter(|_| t.is_target()).and_then(|r| r.counterpart.as_deref()).and_then(|s| index.get(s).copied());
    for (ti, t) in decls.iter().enumerate() {
        if let Some(si) = pair_of(t)
            && (keep[ti].is_none() || keep[si].is_none())
        {
            keep[ti] = None;
            keep[si] = None;
        }
    }
    let mut planned: Vec<Option<Planned>> = vec![None; decls.len()];
    // Targets last: each names its source's id.
    let order = (0..decls.len()).filter(|&i| !decls[i].is_target()).chain((0..decls.len()).filter(|&i| decls[i].is_target()));
    for i in order.collect::<Vec<_>>() {
        let (decl, layout) = (&decls[i], &layouts[i]);
        if let Some(k) = keep[i] {
            planned[i] = Some(Planned::Reuse(k, registry.entries[k].id));
            continue;
        }
        let from = registry.current.get(&decl.name).copied();
        let kind = match decl.relationship.as_ref().map(|r| r.role) {
            None => Kind::Plain,
            Some(RelationshipRole::Source) => {
                let linked_spawn = decls.iter().find(|t| pair_of(t) == Some(i)).and_then(|t| t.relationship.as_ref()).is_some_and(|r| r.linked_spawn);
                Kind::Source { linked_spawn }
            }
            Some(RelationshipRole::Target) => {
                let si = pair_of(decl).expect("a validated target names a declared source");
                let source = match planned[si] {
                    Some(Planned::Reuse(_, id) | Planned::New { id, .. }) => id,
                    None => unreachable!("sources are planned before targets"),
                };
                Kind::Target { source, allow_self_referential: decls[si].relationship.as_ref().is_some_and(|r| r.allow_self_referential) }
            }
        };
        // A spare is plain: a pair's ids are always fresh, as each names the other.
        let spare = match kind {
            Kind::Plain => registry.spares.iter().position(|(n, l, _)| n == &decl.name && l.same_shape(layout)),
            _ => None,
        };
        let id = match spare {
            Some(s) => registry.spares.swap_remove(s).2,
            None => register(world, decl, layout, kind),
        };
        planned[i] = Some(Planned::New { id, from });
    }
    let items = decls
        .iter()
        .zip(layouts)
        .zip(planned)
        .map(|((decl, layout), planned)| {
            let planned = planned.expect("every declaration is planned");
            let id = match planned {
                Planned::Reuse(_, id) | Planned::New { id, .. } => id,
            };
            (decl.clone(), layout, planned, id)
        })
        .collect();
    world.insert_resource(registry);
    PendingComponents { items, required: HashMap::new() }
}

/// A native layout's default bytes with the given fields written over them.
fn native_bytes(n: &crate::layout::NativeComponent, values: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<u8>, String> {
    let mut out = n.defaults.clone();
    for (k, v) in values {
        let f = n.layout.field(k).ok_or_else(|| format!("{} has no field {k}", n.ts_name))?;
        value::encode_field(&format!(".{k}"), &f.ty, v, &mut out[f.offset..f.offset + f.ty.size()]).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Gives back ids a failed generation registered, so the next attempt reuses them.
pub fn abandon(world: &mut World, pending: PendingComponents) {
    let mut registry = world.resource_mut::<TsComponents>();
    for (d, l, p, _) in pending.items {
        if let Planned::New { id, .. } = p
            && d.relationship.is_none()
        {
            registry.spares.push((d.name, l, id));
        }
    }
}

/// Makes a prepared generation's components live: migrates changed layouts by field name,
/// retires the old ids, and marks components the generation no longer declares undeclared.
pub fn commit(world: &mut World, pending: PendingComponents) -> Vec<Migration> {
    let mut registry = world.remove_resource::<TsComponents>().unwrap_or_default();
    let mut migrations = Vec::new();
    let declared: HashMap<String, ()> = pending.items.iter().map(|(d, ..)| (d.name.clone(), ())).collect();
    world.insert_resource(Required(pending.required));
    // A pair with a new id is rebuilt, not migrated: its hooks, old and new, are off until then
    // (custom-relationships decision 8).
    let mut rebuild = Vec::new();
    {
        let mut hooks = world.get_resource_or_init::<TsHooks>();
        for (decl, _, planned, id) in &pending.items {
            let Planned::New { from, .. } = planned else { continue };
            let old = from.map(|i| registry.entries[i].id);
            if decl.relationship.is_some() || old.is_some_and(|o| hooks.role(o).is_some()) {
                hooks.suspend(old.into_iter().chain([*id]));
            }
            if let Some(Role::Target { source, .. }) = hooks.role(*id) {
                rebuild.push((source, *id));
            }
        }
    }

    for (decl, layout, planned, _) in pending.items {
        match planned {
            Planned::Reuse(i, _) => {
                let c = &mut registry.entries[i];
                c.fill = HeapFill::of(&decl, &layout).map(Arc::new);
                c.decl = decl;
                c.state = ComponentState::Live;
            }
            Planned::New { id, from } => {
                if let Some(i) = from.filter(|_| decl.is_target()) {
                    // A target is derived: the old lists go (their drop runs) and are rebuilt.
                    let old = registry.entries[i].id;
                    for e in entities_with(world, old) {
                        world.entity_mut(e).remove_by_id(old);
                    }
                    registry.entries[i].state = ComponentState::Retired;
                } else if let Some(i) = from {
                    let old = registry.entries[i].clone();
                    let mut m = plan(&old, &decl, &layout);
                    let entities = entities_with(world, old.id);
                    for e in &entities {
                        // Built per entity: a default string is its own value on each.
                        let mut bytes = default_bytes(&decl, &layout);
                        let mut em = world.entity_mut(*e);
                        // SAFETY: the old value is removed below; a kept heap value moves out of it,
                        // which an immutable source allows as its removal does.
                        let mut src = unsafe { em.get_mut_assume_mutable_by_id(old.id) }.expect("queried entity has the component");
                        let src = src.bypass_change_detection().as_ptr();
                        for name in &m.kept {
                            let (o, n) = (old.layout.field(name).unwrap(), layout.field(name).unwrap());
                            match HeapKind::of(&n.ty) {
                                // A kept heap value moves: the old slot is left empty, so the old
                                // component's drop frees nothing the new one holds.
                                // SAFETY: the old column's slot holds a live value of `kind`.
                                Some(kind) => bytes.set(n.offset, unsafe { crate::heap::take_value(src, o.offset, kind) }),
                                None => {
                                    let w = n.ty.size();
                                    // SAFETY: the column holds `old.layout.size` bytes for this entity.
                                    let from = unsafe { std::slice::from_raw_parts(src.add(o.offset), w) };
                                    bytes.field_mut(n.offset, w).copy_from_slice(from);
                                }
                            }
                        }
                        // SAFETY: `id` was registered with `layout`, `bytes` is a value of it.
                        unsafe { insert_owned(&mut em, id, bytes) };
                        em.remove_by_id(old.id);
                    }
                    m.entities = entities.len();
                    registry.entries[i].state = ComponentState::Retired;
                    migrations.push(m);
                }
                let name = decl.name.clone();
                let fill = HeapFill::of(&decl, &layout).map(Arc::new);
                registry.entries.push(TsComponent { id, decl, layout, state: ComponentState::Live, fill });
                let new_index = registry.entries.len() - 1;
                registry.current.insert(name, new_index);
            }
        }
    }
    for i in registry.current.values() {
        let c = &mut registry.entries[*i];
        if !declared.contains_key(&c.decl.name) {
            c.state = ComponentState::Undeclared;
        }
    }
    world.insert_resource(registry);
    for (source, target) in rebuild {
        relationships::rebuild(world, source, target);
    }
    world.resource_mut::<TsHooks>().resume();
    migrations
}

/// [`prepare`] then [`commit`].
pub fn apply(world: &mut World, decls: &[ComponentDecl]) -> Vec<Migration> {
    let pending = prepare(world, decls);
    commit(world, pending)
}

/// Builds a zeroed or defaulted value: used by `spawn` when JS passes a component type alone.
pub fn defaults_of(c: &TsComponent) -> HeapBytes {
    default_bytes(&c.decl, &c.layout)
}

/// Field type of a component path, for errors.
pub fn field_type(c: &TsComponent, field: &str) -> Option<FieldType> {
    c.layout.field(field).map(|f| f.ty.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::FieldDecl;
    use serde_json::json;

    fn decl(fields: &[(&str, FieldType, Option<serde_json::Value>)]) -> ComponentDecl {
        ComponentDecl {
            name: "game::Mover".into(),
            fields: fields
                .iter()
                .map(|(n, t, d)| FieldDecl { name: n.to_string(), ty: t.clone(), default: d.clone(), hints: Default::default() })
                .collect(),
            location: None,
            ..Default::default()
        }
    }

    fn read(world: &World, e: Entity, c: &TsComponent) -> serde_json::Value {
        let p = world.get_by_id(e, c.id).unwrap();
        let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), c.layout.size) };
        unsafe { value::decode_component(&c.layout, bytes) }
    }

    fn spawn_with(world: &mut World, c: &TsComponent, v: serde_json::Value) -> Entity {
        let bytes = value::encode_component(&c.decl, &c.layout, v.as_object().unwrap()).unwrap();
        let mut e = world.spawn_empty();
        unsafe { insert_owned(&mut e, c.id, bytes) };
        e.id()
    }

    #[test]
    fn same_layout_keeps_id_and_rust_queries_read_it() {
        let mut world = World::new();
        let d = decl(&[("speed", FieldType::F32, Some(json!(1))), ("amplitude", FieldType::F32, Some(json!(2)))]);
        apply(&mut world, std::slice::from_ref(&d));
        let c = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        let e = spawn_with(&mut world, &c, json!({"speed": 3}));
        apply(&mut world, &[d]);
        let c2 = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        assert_eq!(c.id, c2.id);
        // A Rust query over the dynamic id reads the declared layout.
        let mut q = QueryBuilder::<(Entity, bevy::ecs::world::FilteredEntityRef)>::new(&mut world).ref_id(c.id).build();
        let rows: Vec<_> = q
            .iter(&world)
            .map(|(e, r)| {
                let p = r.get_by_id(c.id).unwrap();
                (e, f32::from_le_bytes(unsafe { std::slice::from_raw_parts(p.as_ptr(), 4) }.try_into().unwrap()))
            })
            .collect();
        assert_eq!(rows, vec![(e, 3.0)]);
    }

    #[test]
    fn adding_a_field_migrates_by_name() {
        let mut world = World::new();
        apply(&mut world, &[decl(&[("speed", FieldType::F32, None), ("amplitude", FieldType::F32, None)])]);
        let c = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        let e = spawn_with(&mut world, &c, json!({"speed": 3, "amplitude": 4}));
        let m = apply(
            &mut world,
            &[decl(&[
                ("speed", FieldType::F32, None),
                ("amplitude", FieldType::F32, None),
                ("phase", FieldType::F32, Some(json!(0.5))),
            ])],
        );
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].entities, 1);
        assert_eq!(m[0].added, vec!["phase"]);
        let c2 = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        assert_ne!(c.id, c2.id);
        assert_eq!(read(&world, e, &c2), json!({"speed": 3.0, "amplitude": 4.0, "phase": 0.5}));
        assert!(world.get_by_id(e, c.id).is_none());
        assert_eq!(world.resource::<TsComponents>().by_id(c.id).unwrap().state, ComponentState::Retired);
    }

    #[test]
    fn retyping_a_field_resets_it() {
        let mut world = World::new();
        apply(&mut world, &[decl(&[("speed", FieldType::F32, None), ("amplitude", FieldType::F32, None)])]);
        let c = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        let e = spawn_with(&mut world, &c, json!({"speed": 3, "amplitude": 4}));
        let m = apply(&mut world, &[decl(&[("speed", FieldType::I32, Some(json!(7))), ("amplitude", FieldType::F32, None)])]);
        assert_eq!(m[0].reset, vec!["speed"]);
        let c2 = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        assert_eq!(read(&world, e, &c2), json!({"speed": 7, "amplitude": 4.0}));
    }

    #[test]
    fn undeclared_keeps_data_and_revives() {
        let mut world = World::new();
        let d = decl(&[("speed", FieldType::F32, None)]);
        apply(&mut world, std::slice::from_ref(&d));
        let c = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        let e = spawn_with(&mut world, &c, json!({"speed": 9}));
        apply(&mut world, &[]);
        assert_eq!(world.resource::<TsComponents>().get("game::Mover").unwrap().state, ComponentState::Undeclared);
        assert!(world.resource::<TsComponents>().live("game::Mover").is_none());
        apply(&mut world, &[d]);
        let c2 = world.resource::<TsComponents>().live("game::Mover").unwrap().clone();
        assert_eq!(c.id, c2.id);
        assert_eq!(read(&world, e, &c2), json!({"speed": 9.0}));
    }

    #[test]
    fn validation() {
        let native = NativeLayouts::default();
        let d = decl(&[("speed", FieldType::F32, None)]);
        assert_eq!(validate(&[d.clone(), d.clone()], &native).len(), 1);
        let bad = decl(&[("speed", FieldType::F32, Some(json!("fast")))]);
        assert!(validate(&[bad], &native)[0].message.contains(".speed"));
    }
}
