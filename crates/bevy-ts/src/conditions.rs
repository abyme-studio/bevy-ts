//! Run conditions, compiled from the manifest and evaluated in Rust with no call into JS: Bevy
//! 0.20's built-in conditions under their names, the combinators, and conditions a crate exposes
//! with `app.ts_condition`. A compiled condition is a Bevy condition system ([`TsCondition`])
//! declaring what it reads. A stateful condition keeps its tick, cursor or previous value in a
//! handle [`ConditionMemory`] keeps per gated config and position in its tree, so a rebuilt
//! condition system takes the old one's state, a reload keeps it and a restart clears it.

use crate::backend::SourceLocation;
use crate::layout::{ComponentLayout, NativeLayouts};
use crate::manifest::{CombineOp, ComponentRef, ComponentSource, ConditionDecl, FilterDecl};
use crate::messages::TsPayloads;
use crate::systems::{TsEncode, TsLookup, rust_component_by_name};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::lifecycle::RemovedComponentEntity;
use bevy::ecs::message::MessageCursor;
use bevy::ecs::query::FilteredAccess;
use bevy::ecs::resource::IS_RESOURCE;
use bevy::ecs::schedule::BoxedCondition;
use bevy::ecs::system::{ReadOnlySystem, RunSystemError, SystemAccess, SystemIn, SystemStateFlags};
use bevy::ecs::world::DeferredWorld;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use bevy::utils::prelude::DebugName;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::Location;
use std::sync::{Arc, Mutex};

/// A stateful node's memory, shared with [`ConditionMemory`] under its slot key.
pub type Slot = Arc<Mutex<ConditionState>>;

/// A run condition, compiled for Rust. `slot` is a stateful node's memory.
pub enum Condition {
    InState { payloads: ComponentId, index: usize, value: u32 },
    /// Messages written since this condition last ran: its own cursor, as Bevy's `on_message`.
    OnMessage { payloads: ComponentId, index: usize, slot: Slot },
    RunOnce { slot: Slot },
    ResourceExists { id: Option<ComponentId> },
    Resource { kind: ResourceKind, id: Option<ComponentId>, slot: Slot },
    ResourceEquals { id: Option<ComponentId>, bytes: Vec<u8>, fields: Vec<(usize, usize)> },
    AnyWith { query: Box<QueryState<Entity>> },
    AnyRemoved { id: ComponentId, slot: Slot },
    AnyMatch { query: Box<QueryState<Entity>>, changed: Vec<ComponentId>, added: Vec<ComponentId>, slot: Slot },
    Changed { condition: Box<Condition>, to: Option<bool>, slot: Slot },
    Not(Box<Condition>),
    /// Several `runIf` calls: every one is evaluated, as Bevy folds a system's conditions.
    All(Vec<Condition>),
    Combine { op: CombineOp, a: Box<Condition>, b: Box<Condition>, eager: bool },
    Native { system: Arc<Mutex<BoxedCondition>>, access: SystemAccess, key: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Added,
    Changed,
    Removed,
    ChangedOrRemoved,
}

/// One stateful condition's memory.
#[derive(Clone, Debug, Default)]
pub struct ConditionState {
    /// The condition's last run, for `Added`/`Changed`.
    pub last_run: Tick,
    /// `runOnce`: it has run; the `Removed` forms: the resource existed.
    pub flag: bool,
    /// `conditionChanged`: the inner condition's previous value.
    pub previous: bool,
    pub removals: MessageCursor<RemovedComponentEntity>,
    /// `onMessage`: the next message sequence number it has not seen.
    pub messages: u64,
}

/// Every stateful condition's memory, by `<config>@<path>`. Survives reloads; a restart clears it.
#[derive(Resource, Default, Debug)]
pub struct ConditionMemory(pub HashMap<String, Slot>);

/// The memory under `key`, made on first use.
fn slot_of(world: &mut World, key: &str) -> Slot {
    world.get_resource_or_init::<ConditionMemory>().0.entry(key.to_string()).or_default().clone()
}

/// A `nativeCondition` call's arguments, as JSON.
pub type ConditionArgs = serde_json::Value;

/// Builds a Rust condition from a call's JSON arguments.
pub type ConditionFactory = Arc<dyn Fn(&serde_json::Value) -> Result<BoxedCondition, String> + Send + Sync>;

/// One exposed condition.
#[derive(Clone)]
pub struct ExposedCondition {
    pub name: String,
    pub factory: ConditionFactory,
    pub location: &'static Location<'static>,
}

/// Conditions crates exposed with `app.ts_condition`, by name.
#[derive(Resource, Default, Clone)]
pub struct TsConditions {
    by_name: BTreeMap<String, ExposedCondition>,
    duplicates: Vec<(String, &'static Location<'static>, &'static Location<'static>)>,
}

impl TsConditions {
    pub fn get(&self, name: &str) -> Option<&ExposedCondition> {
        self.by_name.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.duplicates.is_empty() {
            return Ok(());
        }
        let lines: Vec<String> = self.duplicates.iter().map(|(n, a, b)| format!("condition {n:?} is exposed twice: at {a} and at {b}")).collect();
        Err(format!("bevy-ts: {}", lines.join("; ")))
    }
}

/// Exposes a Rust condition to TS as `name` (`time:on-timer`). A duplicate fails the app at
/// startup, naming both registrations.
#[track_caller]
pub fn expose(app: &mut App, name: &str, factory: impl Fn(&serde_json::Value) -> Result<BoxedCondition, String> + Send + Sync + 'static) {
    assert!(crate::sets::valid_name(name), "bevy-ts: exposed condition name {name:?} must be <crate>:<name>, such as \"time:on-timer\"");
    let location = Location::caller();
    let mut c = app.world_mut().get_resource_or_init::<TsConditions>();
    if let Some(prev) = c.by_name.get(name) {
        let prev = prev.location;
        c.duplicates.push((name.to_string(), prev, location));
    } else {
        c.by_name.insert(name.to_string(), ExposedCondition { name: name.to_string(), factory: Arc::new(factory), location });
    }
}

/// Instances of exposed conditions, kept across generations while a use's name and arguments
/// stay the same, so the condition's own state survives a reload; with the access each declared.
#[derive(Resource, Default)]
pub struct NativeConditions(pub HashMap<String, (Arc<Mutex<BoxedCondition>>, SystemAccess)>);

/// A compile error, with the location of the call that caused it when there is one.
pub struct CompileError {
    pub message: String,
    pub location: Option<SourceLocation>,
}

impl From<String> for CompileError {
    fn from(message: String) -> Self {
        CompileError { message, location: None }
    }
}

/// What compiling a condition reads.
pub struct Compile<'a> {
    pub ts: &'a TsLookup<'a>,
    pub encode: &'a TsEncode<'a>,
    /// The gated config: a system's identity or a group's id.
    pub config: &'a str,
    /// Native condition instances this generation uses, by key.
    pub natives: &'a mut HashSet<String>,
}

fn resource_id(world: &World, name: &str, ts: &TsLookup) -> Option<ComponentId> {
    ts(name).map(|(id, _)| id).or_else(|| world.resource::<NativeLayouts>().get(name).map(|n| n.id)).or_else(|| rust_component_by_name(world, name))
}

fn component_id(world: &World, r: &ComponentRef, ts: &TsLookup) -> Result<ComponentId, String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).map(|(id, _)| id).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => world
            .resource::<NativeLayouts>()
            .get(&r.name)
            .map(|n| n.id)
            .or_else(|| rust_component_by_name(world, &r.name))
            .ok_or_else(|| format!("no Rust component named {}", r.name)),
    }
}

/// Byte ranges `[start, end)` of a layout's fields.
type FieldRanges = Vec<(usize, usize)>;

/// A resource value as native bytes, and the byte ranges of its fields (padding left out).
fn encode_resource(world: &World, name: &str, values: &serde_json::Map<String, serde_json::Value>, c: &Compile) -> Result<(Vec<u8>, FieldRanges), String> {
    let ranges = |l: &ComponentLayout| l.fields.iter().map(|f| (f.offset, f.offset + f.ty.size())).collect::<Vec<_>>();
    if let Some(r) = (c.encode)(name, values) {
        let (bytes, layout) = r.map_err(|e| format!("resourceEquals({name}): {e}"))?;
        return Ok((bytes, ranges(&layout)));
    }
    let native = world.resource::<NativeLayouts>();
    let Some(n) = native.get(name) else {
        let why = native.reflect_tier(name).map(|t| format!(" ({})", t.reason)).unwrap_or_default();
        return Err(format!("resourceEquals({name}): {name} is not a resource with a native layout{why}; its bytes cannot be compared"));
    };
    let mut bytes = if n.defaults.len() == n.layout.size { n.defaults.clone() } else { vec![0; n.layout.size] };
    for (k, v) in values {
        let f = n.layout.field(k).ok_or_else(|| format!("resourceEquals({name}): {name} has no field {k}"))?;
        crate::value::encode_field(&format!(".{k}"), &f.ty, v, &mut bytes[f.offset..f.offset + f.ty.size()]).map_err(|e| format!("resourceEquals({name}): {e}"))?;
    }
    Ok((bytes, ranges(&n.layout)))
}

fn hidden_query(world: &mut World, with: &[ComponentId], without: &[ComponentId]) -> Box<QueryState<Entity>> {
    let mut b = QueryBuilder::<Entity>::new(world);
    for id in with {
        b.with_id(*id);
    }
    for id in without {
        b.without_id(*id);
    }
    b.without::<crate::ScriptHidden>();
    Box::new(b.build())
}

/// Compiles a condition tree. `path` is the node's position, part of its memory key.
pub fn compile(world: &mut World, d: &ConditionDecl, path: &str, c: &mut Compile) -> Result<Condition, CompileError> {
    let slot_key = format!("{}@{path}", c.config);
    let child = |i: usize| if path.is_empty() { i.to_string() } else { format!("{path}.{i}") };
    Ok(match d {
        ConditionDecl::InState { state, value } => {
            let payloads = world.resource::<TsPayloads>();
            let index = payloads.state(state).ok_or_else(|| format!("no state {state}"))?;
            let value = payloads.states[index].index(value).ok_or_else(|| format!("{state} has no value {value}"))?;
            Condition::InState { payloads: world.register_resource::<TsPayloads>(), index, value }
        }
        ConditionDecl::OnMessage { message } => {
            let index = world.resource::<TsPayloads>().message(message).ok_or_else(|| format!("no message {message}"))?;
            Condition::OnMessage { payloads: world.register_resource::<TsPayloads>(), index, slot: slot_of(world, &slot_key) }
        }
        ConditionDecl::RunOnce => Condition::RunOnce { slot: slot_of(world, &slot_key) },
        ConditionDecl::ResourceExists { resource } => Condition::ResourceExists { id: resource_id(world, resource, c.ts) },
        ConditionDecl::ResourceAdded { resource }
        | ConditionDecl::ResourceChanged { resource }
        | ConditionDecl::ResourceExistsAndChanged { resource }
        | ConditionDecl::ResourceRemoved { resource }
        | ConditionDecl::ResourceChangedOrRemoved { resource } => {
            let kind = match d {
                ConditionDecl::ResourceAdded { .. } => ResourceKind::Added,
                ConditionDecl::ResourceRemoved { .. } => ResourceKind::Removed,
                ConditionDecl::ResourceChangedOrRemoved { .. } => ResourceKind::ChangedOrRemoved,
                _ => ResourceKind::Changed,
            };
            Condition::Resource { kind, id: resource_id(world, resource, c.ts), slot: slot_of(world, &slot_key) }
        }
        ConditionDecl::ResourceEquals { resource, value } | ConditionDecl::ResourceExistsAndEquals { resource, value } => {
            let (bytes, fields) = encode_resource(world, resource, value, c)?;
            Condition::ResourceEquals { id: resource_id(world, resource, c.ts), bytes, fields }
        }
        ConditionDecl::AnyWithComponent { component } => {
            let id = component_id(world, component, c.ts)?;
            Condition::AnyWith { query: hidden_query(world, &[id], &[]) }
        }
        ConditionDecl::AnyComponentRemoved { component } => Condition::AnyRemoved { id: component_id(world, component, c.ts)?, slot: slot_of(world, &slot_key) },
        ConditionDecl::AnyMatchFilter { filter } => {
            let (mut with, mut without, mut changed, mut added) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for f in filter {
                match f {
                    FilterDecl::With(r) => with.push(component_id(world, r, c.ts)?),
                    FilterDecl::Without(r) => without.push(component_id(world, r, c.ts)?),
                    FilterDecl::Changed(r) => changed.push(component_id(world, r, c.ts)?),
                    FilterDecl::Added(r) => added.push(component_id(world, r, c.ts)?),
                }
            }
            with.extend(changed.iter().chain(&added));
            Condition::AnyMatch { query: hidden_query(world, &with, &without), changed, added, slot: slot_of(world, &slot_key) }
        }
        ConditionDecl::ConditionChanged { condition } => {
            let condition = Box::new(compile(world, condition, &child(0), c)?);
            Condition::Changed { condition, to: None, slot: slot_of(world, &slot_key) }
        }
        ConditionDecl::ConditionChangedTo { condition, to } => {
            let condition = Box::new(compile(world, condition, &child(0), c)?);
            Condition::Changed { condition, to: Some(*to), slot: slot_of(world, &slot_key) }
        }
        ConditionDecl::Not { condition } => Condition::Not(Box::new(compile(world, condition, &child(0), c)?)),
        ConditionDecl::All { conditions } => {
            Condition::All(conditions.iter().enumerate().map(|(i, x)| compile(world, x, &child(i), c)).collect::<Result<_, _>>()?)
        }
        ConditionDecl::Combine { op, a, b, eager } => Condition::Combine {
            op: *op,
            a: Box::new(compile(world, a, &child(0), c)?),
            b: Box::new(compile(world, b, &child(1), c)?),
            eager: *eager,
        },
        ConditionDecl::Native { name, args, location } => {
            let at = |message: String| CompileError { message, location: location.clone() };
            let Some(exposed) = world.get_resource::<TsConditions>().and_then(|t| t.get(name)).cloned() else {
                return Err(at(format!("nativeCondition({name:?}): no loaded crate exposes a condition named {name}")));
            };
            let key = format!("{slot_key}|{name}|{args}");
            c.natives.insert(key.clone());
            if let Some((system, access)) = world.get_resource::<NativeConditions>().and_then(|n| n.0.get(&key)).cloned() {
                return Ok(Condition::Native { system, access, key });
            }
            let mut system = (exposed.factory)(args).map_err(|e| at(format!("nativeCondition({name:?}): {e}")))?;
            let access = system.initialize(world);
            if access.is_exclusive() || system.has_deferred() {
                return Err(at(format!("nativeCondition({name:?}): a run condition may only read the world")));
            }
            let system = Arc::new(Mutex::new(system));
            world.get_resource_or_init::<NativeConditions>().0.insert(key.clone(), (system.clone(), access.clone()));
            Condition::Native { system, access, key }
        }
    })
}

/// Evaluates a condition at `this_run`. Stateful nodes advance each time they are evaluated, as
/// Bevy's condition systems do.
///
/// # Safety
/// `world` may read everything [`Condition::access`] declares, and nothing writes it meanwhile.
pub unsafe fn evaluate(world: UnsafeWorldCell, this_run: Tick, c: &mut Condition) -> bool {
    // SAFETY (each read below): declared by `access`, per the caller's contract.
    unsafe {
        match c {
            Condition::InState { index, value, .. } => world.get_resource::<TsPayloads>().and_then(|p| p.states.get(*index)).is_some_and(|s| s.current == Some(*value)),
            Condition::OnMessage { index, slot, .. } => {
                let Some(q) = world.get_resource::<TsPayloads>().and_then(|p| p.messages.get(*index)) else { return false };
                let cursor = std::mem::replace(&mut slot.lock().unwrap().messages, q.next_seq);
                q.has_unread(cursor)
            }
            Condition::RunOnce { slot } => !std::mem::replace(&mut slot.lock().unwrap().flag, true),
            Condition::ResourceExists { id } => id.is_some_and(|id| world.resource_entities().get(id).is_some()),
            Condition::Resource { kind, id, slot } => {
                let ticks = id.and_then(|id| resource_ticks(world, id));
                let mut st = slot.lock().unwrap();
                let last_run = std::mem::replace(&mut st.last_run, this_run);
                match (*kind, ticks) {
                    (ResourceKind::Added, t) => t.is_some_and(|(added, _)| added.is_newer_than(last_run, this_run)),
                    (ResourceKind::Changed, t) => t.is_some_and(|(_, changed)| changed.is_newer_than(last_run, this_run)),
                    (ResourceKind::Removed, Some(_)) => {
                        st.flag = true;
                        false
                    }
                    (ResourceKind::ChangedOrRemoved, Some((_, changed))) => {
                        st.flag = true;
                        changed.is_newer_than(last_run, this_run)
                    }
                    (ResourceKind::Removed | ResourceKind::ChangedOrRemoved, None) => std::mem::replace(&mut st.flag, false),
                }
            }
            Condition::ResourceEquals { id, bytes, fields } => {
                let Some(ptr) = id.and_then(|id| world.get_resource_by_id(id)) else { return false };
                // The resource's layout is the one `bytes` was encoded with: `bytes.len()` bytes.
                let have = std::slice::from_raw_parts(ptr.as_ptr(), bytes.len());
                fields.iter().all(|&(a, b)| have[a..b] == bytes[a..b])
            }
            Condition::AnyWith { query } => {
                query.update_archetypes_unsafe_world_cell(world);
                query.query_unchecked_manual_with_ticks(world, this_run, this_run).iter().next().is_some()
            }
            Condition::AnyRemoved { id, slot } => {
                let mut st = slot.lock().unwrap();
                world.removed_components().get(*id).is_some_and(|m| st.removals.read(m).count() > 0)
            }
            Condition::AnyMatch { query, changed, added, slot } => {
                let last_run = std::mem::replace(&mut slot.lock().unwrap().last_run, this_run);
                query.update_archetypes_unsafe_world_cell(world);
                let q = query.query_unchecked_manual_with_ticks(world, last_run, this_run);
                let mut rows = q.iter();
                if changed.is_empty() && added.is_empty() {
                    return rows.next().is_some();
                }
                // Per entity, so sparse-set components and the query's `Without` are honoured.
                rows.any(|e| {
                    let Ok(row) = world.get_entity(e) else { return false };
                    changed.iter().all(|id| row.get_change_ticks_by_id(*id).is_some_and(|t| t.is_changed(last_run, this_run)))
                        && added.iter().all(|id| row.get_change_ticks_by_id(*id).is_some_and(|t| t.is_added(last_run, this_run)))
                })
            }
            Condition::Changed { condition, to, slot } => {
                let now = evaluate(world, this_run, condition);
                let before = std::mem::replace(&mut slot.lock().unwrap().previous, now);
                before != now && to.is_none_or(|t| t == now)
            }
            Condition::Not(c) => !evaluate(world, this_run, c),
            Condition::All(cs) => {
                let mut ok = true;
                for c in cs.iter_mut() {
                    ok &= evaluate(world, this_run, c);
                }
                ok
            }
            Condition::Combine { op, a, b, eager } => {
                let x = evaluate(world, this_run, a);
                let decided = match op {
                    CombineOp::And | CombineOp::Nand => !x,
                    CombineOp::Or | CombineOp::Nor => x,
                    CombineOp::Xor | CombineOp::Xnor => false,
                };
                if decided && !*eager {
                    return matches!(op, CombineOp::Or | CombineOp::Nand);
                }
                let y = evaluate(world, this_run, b);
                match op {
                    CombineOp::And => x && y,
                    CombineOp::Or => x || y,
                    CombineOp::Nand => !(x && y),
                    CombineOp::Nor => !(x || y),
                    CombineOp::Xor => x ^ y,
                    CombineOp::Xnor => !(x ^ y),
                }
            }
            Condition::Native { system, .. } => system.lock().unwrap().run_unsafe((), world).unwrap_or(false),
        }
    }
}

/// A resource's added and changed ticks.
///
/// # Safety
/// The caller may read `id`'s ticks.
unsafe fn resource_ticks(world: UnsafeWorldCell, id: ComponentId) -> Option<(Tick, Tick)> {
    let entity = unsafe { world.resource_entities() }.get(id)?;
    let ticks = unsafe { world.get_entity(entity).ok()?.get_change_ticks_by_id(id)? };
    Some((ticks.added, ticks.changed))
}

/// Evaluates a condition from an exclusive system, at the world's current tick.
pub fn evaluate_now(world: &mut World, c: &mut Condition) -> bool {
    let this_run = world.change_tick();
    // SAFETY: the world is held exclusively.
    unsafe { evaluate(world.as_unsafe_world_cell(), this_run, c) }
}

impl Condition {
    /// Adds what evaluating the condition reads to `access`.
    pub fn access(&self, access: &mut SystemAccess) {
        let read = |access: &mut SystemAccess, id: ComponentId| {
            let mut f = FilteredAccess::default();
            f.add_read(id);
            f.and_with(IS_RESOURCE);
            access.ensure_filtered_access(f);
        };
        match self {
            Condition::InState { payloads, .. } | Condition::OnMessage { payloads, .. } => read(access, *payloads),
            Condition::RunOnce { .. } | Condition::ResourceExists { .. } | Condition::AnyRemoved { .. } => access.ensure_metadata_access(),
            Condition::Resource { id, .. } | Condition::ResourceEquals { id, .. } => match id {
                Some(id) => read(access, *id),
                None => access.ensure_metadata_access(),
            },
            Condition::AnyWith { query } => access.ensure_filtered_access(query.component_access().clone()),
            Condition::AnyMatch { query, changed, added, .. } => {
                let mut f = query.component_access().clone();
                for id in changed.iter().chain(added) {
                    f.add_read(*id);
                }
                access.ensure_filtered_access(f);
            }
            Condition::Changed { condition, .. } | Condition::Not(condition) => condition.access(access),
            Condition::All(cs) => cs.iter().for_each(|c| c.access(access)),
            Condition::Combine { a, b, .. } => {
                a.access(access);
                b.access(access);
            }
            Condition::Native { access: a, .. } => access.extend(a.clone()),
        }
    }

    /// What the condition was built from: ids, indices and native instances. Two equal ones
    /// gate the same way, so a rebuild may keep the old condition system.
    pub fn fingerprint(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = match self {
            Condition::InState { index, value, .. } => write!(out, "in({index},{value})"),
            Condition::OnMessage { index, .. } => write!(out, "msg({index})"),
            Condition::RunOnce { .. } => write!(out, "once"),
            Condition::ResourceExists { id } => write!(out, "exists({id:?})"),
            Condition::Resource { kind, id, .. } => write!(out, "res({kind:?},{id:?})"),
            Condition::ResourceEquals { id, bytes, .. } => write!(out, "eq({id:?},{bytes:?})"),
            Condition::AnyWith { query } => write!(out, "with({:?})", query.component_access()),
            Condition::AnyRemoved { id, .. } => write!(out, "removed({id:?})"),
            Condition::AnyMatch { query, changed, added, .. } => write!(out, "match({:?},{changed:?},{added:?})", query.component_access()),
            Condition::Changed { condition, to, .. } => {
                let _ = write!(out, "changed({to:?},");
                condition.fingerprint(out);
                write!(out, ")")
            }
            Condition::Not(c) => {
                out.push('!');
                c.fingerprint(out);
                Ok(())
            }
            Condition::All(cs) => {
                out.push('[');
                for c in cs {
                    c.fingerprint(out);
                    out.push(',');
                }
                write!(out, "]")
            }
            Condition::Combine { op, a, b, eager } => {
                let _ = write!(out, "{op:?}{eager}(");
                a.fingerprint(out);
                out.push(',');
                b.fingerprint(out);
                write!(out, ")")
            }
            Condition::Native { key, .. } => write!(out, "native({key})"),
        };
    }
}

/// A compiled condition as a Bevy condition system: the `run_if` of a TS system, or of a group's
/// set for a collective `runIf`. It declares what it reads, so it runs beside any Rust system
/// that does not write it.
pub struct TsCondition {
    name: DebugName,
    condition: Condition,
    last_run: Tick,
}

impl TsCondition {
    pub fn new(name: String, condition: Condition) -> Self {
        TsCondition { name: DebugName::owned(name), condition, last_run: Tick::new(0) }
    }
}

impl System for TsCondition {
    type In = ();
    type Out = bool;

    fn name(&self) -> DebugName {
        self.name.clone()
    }

    fn flags(&self) -> SystemStateFlags {
        SystemStateFlags::empty()
    }

    unsafe fn run_unsafe(&mut self, _input: SystemIn<'_, Self>, world: UnsafeWorldCell) -> Result<bool, RunSystemError> {
        let this_run = world.increment_change_tick();
        // SAFETY: the caller grants the access `initialize` returned, which is `Condition::access`.
        let pass = unsafe { evaluate(world, this_run, &mut self.condition) };
        self.last_run = this_run;
        Ok(pass)
    }


    fn apply_deferred(&mut self, _world: &mut World) {}

    fn queue_deferred(&mut self, _world: DeferredWorld) {}

    fn initialize(&mut self, world: &mut World) -> SystemAccess {
        self.last_run = world.change_tick().relative_to(Tick::MAX);
        let mut access = SystemAccess::default();
        self.condition.access(&mut access);
        access
    }

    fn check_change_tick(&mut self, check: bevy::ecs::change_detection::CheckChangeTicks) {
        self.last_run.check_tick(check);
    }

    fn get_last_run(&self) -> Tick {
        self.last_run
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.last_run = last_run;
    }
}

// SAFETY: a condition only reads the world: `evaluate` writes nothing it declares, and its state
// lives in its own handles.
unsafe impl ReadOnlySystem for TsCondition {}

/// Drops native condition instances the live generation no longer uses.
pub(crate) fn prune_natives(world: &mut World, live: &HashSet<String>) {
    if let Some(mut n) = world.get_resource_mut::<NativeConditions>() {
        n.0.retain(|k, _| live.contains(k));
    }
}
