//! TS components: registration as dynamic ECS components with native layouts, identity across
//! generations, migration by field name, and undeclared components.

use crate::backend::ScriptError;
use crate::commands::insert_bytes;
use crate::layout::{ComponentLayout, FieldType, NativeLayouts};
use crate::manifest::ComponentDecl;
use crate::value::{self, ValueError};
use bevy::ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

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
    pub fn encode(&self, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<u8>, ValueError> {
        let c = self.live(name).ok_or_else(|| ValueError {
            path: String::new(),
            expected: "a live TS component".into(),
            message: format!("no live component named {name}"),
        })?;
        value::encode_component(&c.decl, &c.layout, values)
    }

    /// Component ids TS may write with their sizes: live TS components and native layouts.
    pub fn writable_sizes(&self, native: &NativeLayouts) -> HashMap<ComponentId, usize> {
        let mut m: HashMap<_, _> = self
            .current()
            .filter(|c| c.state == ComponentState::Live)
            .map(|c| (c.id, c.layout.size))
            .collect();
        m.extend(native.iter().map(|n| (n.id, n.layout.size)));
        m
    }
}

/// Validates a generation's component declarations without touching the world.
pub fn validate(decls: &[ComponentDecl], native: &NativeLayouts) -> Vec<ScriptError> {
    let mut errors = Vec::new();
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
        if d.fields.is_empty() {
            errors.push(ScriptError::at(format!("component {} has no fields", d.name), at.clone()));
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
    errors
}

fn register(world: &mut World, decl: &ComponentDecl, layout: &ComponentLayout) -> ComponentId {
    // SAFETY: TS components are plain bytes: no drop, any bit pattern is a valid value for every
    // field type's storage, and the data is Send + Sync.
    let descriptor = unsafe {
        ComponentDescriptor::new_with_layout(
            decl.name.clone(),
            StorageType::Table,
            layout.std_layout(),
            None,
            true,
            true,
            ComponentCloneBehavior::Default,
            None,
        )
    };
    world.register_component_with_descriptor(descriptor)
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

fn default_bytes(decl: &ComponentDecl, layout: &ComponentLayout) -> Vec<u8> {
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
    pub fn encode(&self, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<u8>, ValueError> {
        let (d, l, ..) = self.items.iter().find(|(d, ..)| d.name == name).ok_or_else(|| ValueError {
            path: String::new(),
            expected: "a component of this generation".into(),
            message: format!("no component named {name}"),
        })?;
        value::encode_component(d, l, values)
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
    let mut items = Vec::new();
    for decl in decls {
        let layout = decl.layout();
        let current = registry.current.get(&decl.name).copied();
        let planned = match current {
            Some(i) if registry.entries[i].layout.same_shape(&layout) => Planned::Reuse(i, registry.entries[i].id),
            from => {
                let spare = registry.spares.iter().position(|(n, l, _)| n == &decl.name && l.same_shape(&layout));
                let id = match spare {
                    Some(s) => registry.spares.swap_remove(s).2,
                    None => register(world, decl, &layout),
                };
                Planned::New { id, from }
            }
        };
        let id = match planned {
            Planned::Reuse(_, id) | Planned::New { id, .. } => id,
        };
        items.push((decl.clone(), layout, planned, id));
    }
    world.insert_resource(registry);
    PendingComponents { items }
}

/// Gives back ids a failed generation registered, so the next attempt reuses them.
pub fn abandon(world: &mut World, pending: PendingComponents) {
    let mut registry = world.resource_mut::<TsComponents>();
    for (d, l, p, _) in pending.items {
        if let Planned::New { id, .. } = p {
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

    for (decl, layout, planned, _) in pending.items {
        match planned {
            Planned::Reuse(i, _) => {
                let c = &mut registry.entries[i];
                c.decl = decl;
                c.state = ComponentState::Live;
            }
            Planned::New { id, from } => {
                if let Some(i) = from {
                    let old = registry.entries[i].clone();
                    let mut m = plan(&old, &decl, &layout);
                    let defaults = default_bytes(&decl, &layout);
                    let entities = entities_with(world, old.id);
                    for e in &entities {
                        let mut bytes = defaults.clone();
                        let src = world.get_by_id(*e, old.id).expect("queried entity has the component");
                        // SAFETY: the column holds `old.layout.size` bytes for this entity.
                        let src = unsafe { std::slice::from_raw_parts(src.as_ptr(), old.layout.size) };
                        for name in &m.kept {
                            let (o, n) = (old.layout.field(name).unwrap(), layout.field(name).unwrap());
                            let w = n.ty.size();
                            bytes[n.offset..n.offset + w].copy_from_slice(&src[o.offset..o.offset + w]);
                        }
                        let mut em = world.entity_mut(*e);
                        // SAFETY: `id` was registered with `layout`, `bytes` has its size.
                        unsafe { insert_bytes(&mut em, id, &bytes) };
                        em.remove_by_id(old.id);
                    }
                    m.entities = entities.len();
                    registry.entries[i].state = ComponentState::Retired;
                    migrations.push(m);
                }
                let name = decl.name.clone();
                registry.entries.push(TsComponent { id, decl, layout, state: ComponentState::Live });
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
    migrations
}

/// [`prepare`] then [`commit`].
pub fn apply(world: &mut World, decls: &[ComponentDecl]) -> Vec<Migration> {
    let pending = prepare(world, decls);
    commit(world, pending)
}

/// Builds a zeroed or defaulted value: used by `spawn` when JS passes a component type alone.
pub fn defaults_of(c: &TsComponent) -> Vec<u8> {
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
        }
    }

    fn read(world: &World, e: Entity, c: &TsComponent) -> serde_json::Value {
        let p = world.get_by_id(e, c.id).unwrap();
        let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), c.layout.size) };
        value::decode_component(&c.layout, bytes)
    }

    fn spawn_with(world: &mut World, c: &TsComponent, v: serde_json::Value) -> Entity {
        let bytes = value::encode_component(&c.decl, &c.layout, v.as_object().unwrap()).unwrap();
        let mut e = world.spawn_empty();
        unsafe { insert_bytes(&mut e, c.id, &bytes) };
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
