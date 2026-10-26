//! Run conditions, compiled from the manifest and evaluated in Rust with no call into JS: Bevy
//! 0.20's built-in conditions under their names, the combinators, and conditions a crate exposes
//! with `app.ts_condition`. A TS predicate is the one node that calls into JS, once per
//! evaluation. A compiled condition is a Bevy condition system ([`TsCondition`]) declaring what
//! it reads; one that calls into JS or reads a Rust message queue needs the whole world on the
//! main thread, so an exclusive gate system evaluates it and the run condition reads the gate's
//! answer ([`Condition::needs_world`]). A stateful condition keeps its tick, cursor or previous
//! value in a handle [`ConditionMemory`] keeps per gated config and position in its tree, so a
//! rebuilt condition system takes the old one's state, a reload keeps it and a restart clears
//! it. A predicate keeps its own in `SystemMemory`, under the same key.

use crate::backend::SourceLocation;
use crate::heap::{HeapBytes, HeapKind};
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
    /// A machine's `State` resource, its reader, and a value's symbol.
    InState { state: ComponentId, current: crate::states::CurrentCell, value: u32 },
    /// Bevy's `state_changed` on a machine: its `State` changed since this condition ran.
    StateChanged { state: ComponentId, slot: Slot },
    StateExists { state: ComponentId, current: crate::states::CurrentCell },
    /// Messages written since this condition last ran: its own cursor, as Bevy's `on_message`.
    OnMessage { payloads: ComponentId, index: usize, slot: Slot },
    /// `onMessage` of a Rust message, with its own cursor into Bevy's `Messages<M>`.
    OnRustMessage { reader: crate::rust_events::ReflectTsMessage, slot: Slot },
    RunOnce { slot: Slot },
    ResourceExists { id: Option<ComponentId> },
    Resource { kind: ResourceKind, id: Option<ComponentId>, slot: Slot },
    /// The expected value, its plain fields' byte ranges, and its heap fields, compared by value.
    ResourceEquals { id: Option<ComponentId>, bytes: HeapBytes, fields: Vec<(usize, usize)>, heap: Vec<(usize, HeapKind)> },
    AnyWith { query: Box<QueryState<Entity>> },
    AnyRemoved { id: ComponentId, slot: Slot },
    AnyMatch { query: Box<QueryState<Entity>>, changed: Vec<ComponentId>, added: Vec<ComponentId>, slot: Slot },
    Changed { condition: Box<Condition>, to: Option<bool>, slot: Slot },
    Not(Box<Condition>),
    /// Several `runIf` calls: every one is evaluated, as Bevy folds a system's conditions.
    All(Vec<Condition>),
    Combine { op: CombineOp, a: Box<Condition>, b: Box<Condition>, eager: bool },
    Native { system: Arc<Mutex<BoxedCondition>>, access: SystemAccess, key: String },
    /// A use of a TS predicate: `system` is its identity, `memory` this use's key in
    /// `SystemMemory`, so each use has its own ticks, cursors and `Local`s, as each of Bevy's
    /// `run_if(f)` is its own condition system. Evaluating it is one call into JS.
    Predicate { system: String, memory: String },
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
    pub predicates: &'a mut PredicateUses,
}

/// The generation's TS predicates, and the uses its conditions make of them.
#[derive(Default)]
pub struct PredicateUses {
    /// Predicates by identity.
    pub declared: HashSet<String>,
    /// Each use as (identity, memory key), in compile order.
    pub uses: Vec<(String, String)>,
}

/// A resource condition's resource: `None` when no type has the name (the condition is never
/// true), and an error, under the condition's name, when a short name names several Rust types.
fn resource_id(world: &World, name: &str, ts: &TsLookup, condition: &str) -> Result<Option<ComponentId>, String> {
    if let Some((id, ..)) = ts(name) {
        return Ok(Some(id));
    }
    if let Some(n) = world.resource::<NativeLayouts>().get(name) {
        return Ok(Some(n.id));
    }
    rust_component_by_name(world, name).map_err(|e| format!("{condition}({name}): {e}"))
}

fn component_id(world: &World, r: &ComponentRef, ts: &TsLookup) -> Result<ComponentId, String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).map(|(id, ..)| id).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => world
            .resource::<NativeLayouts>()
            .get(&r.name)
            .map(|n| n.id)
            .map_or_else(|| rust_component_by_name(world, &r.name), |id| Ok(Some(id)))?
            .ok_or_else(|| format!("no Rust component named {}", r.name)),
    }
}

/// Byte ranges `[start, end)` of a layout's plain fields, and its heap fields' offsets and kinds.
type FieldRanges = (Vec<(usize, usize)>, Vec<(usize, HeapKind)>);

/// A resource value as native bytes, and where its fields are (padding left out).
fn encode_resource(world: &World, name: &str, values: &serde_json::Map<String, serde_json::Value>, c: &Compile, condition: &str) -> Result<(HeapBytes, FieldRanges), String> {
    let ranges = |l: &ComponentLayout| {
        let plain = l.fields.iter().filter(|f| !f.ty.is_heap()).map(|f| (f.offset, f.offset + f.ty.size())).collect();
        (plain, l.fields.iter().filter_map(|f| HeapKind::of(&f.ty).map(|k| (f.offset, k))).collect())
    };
    if let Some(r) = (c.encode)(name, values) {
        let (bytes, layout) = r.map_err(|e| format!("{condition}({name}): {e}"))?;
        return Ok((bytes, ranges(&layout)));
    }
    let native = world.resource::<NativeLayouts>();
    let Some(n) = native.get(name) else {
        let why = native.reflect_tier(name).map(|t| format!(" ({})", t.reason)).unwrap_or_default();
        return Err(format!("{condition}({name}): {name} is not a resource with a native layout{why}; its bytes cannot be compared"));
    };
    let mut bytes = if n.defaults.len() == n.layout.size { n.defaults.clone() } else { vec![0; n.layout.size] };
    for (k, v) in values {
        let f = n.layout.field(k).ok_or_else(|| format!("{condition}({name}): {name} has no field {k}"))?;
        crate::value::encode_field(&format!(".{k}"), &f.ty, v, &mut bytes[f.offset..f.offset + f.ty.size()]).map_err(|e| format!("{condition}({name}): {e}"))?;
    }
    Ok((HeapBytes::plain(bytes), ranges(&n.layout)))
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
    let slot_key = &slot_key;
    let child = |i: usize| if path.is_empty() { i.to_string() } else { format!("{path}.{i}") };
    Ok(match d {
        ConditionDecl::InState { state, value } => {
            let payloads = world.resource::<TsPayloads>();
            let m = &payloads.states[payloads.state(state).ok_or_else(|| format!("no state {state}"))?];
            let (slot, value) = (m.slot, m.sym(value).ok_or_else(|| format!("{state} has no value {value}"))?);
            let crate::states::StateCell { state, current, .. } = crate::states::state_cell_of(world, slot);
            Condition::InState { state, current, value }
        }
        ConditionDecl::StateChanged { state } | ConditionDecl::StateExists { state } => {
            let payloads = world.resource::<TsPayloads>();
            let slot = payloads.states[payloads.state(state).ok_or_else(|| format!("no state {state}"))?].slot;
            let crate::states::StateCell { state, current, .. } = crate::states::state_cell_of(world, slot);
            if matches!(d, ConditionDecl::StateChanged { .. }) { Condition::StateChanged { state, slot: slot_of(world, slot_key) } } else { Condition::StateExists { state, current } }
        }
        ConditionDecl::OnMessage { message } => match world.resource::<TsPayloads>().message(message) {
            Some(index) => Condition::OnMessage { payloads: world.register_component::<TsPayloads>(), index, slot: slot_of(world, slot_key) },
            None if message.contains("::") => {
                let (_, reader) = crate::rust_events::reader(&world.resource::<AppTypeRegistry>().read(), message).map_err(|e| format!("onMessage({message}): {e}"))?;
                // A new cursor starts at the queue's end, so a restart does not replay old messages.
                let end = reader.end(world) as u64;
                let slot = slot_of(world, slot_key);
                {
                    let mut st = slot.lock().unwrap();
                    if st.messages == 0 {
                        st.messages = end;
                    }
                }
                Condition::OnRustMessage { reader, slot }
            }
            None => return Err(format!("no message {message}").into()),
        },
        ConditionDecl::RunOnce => Condition::RunOnce { slot: slot_of(world, slot_key) },
        ConditionDecl::ResourceExists { resource } => Condition::ResourceExists { id: resource_id(world, resource, c.ts, "resourceExists")? },
        ConditionDecl::ResourceAdded { resource }
        | ConditionDecl::ResourceChanged { resource }
        | ConditionDecl::ResourceExistsAndChanged { resource }
        | ConditionDecl::ResourceRemoved { resource }
        | ConditionDecl::ResourceChangedOrRemoved { resource } => {
            let (kind, condition) = match d {
                ConditionDecl::ResourceAdded { .. } => (ResourceKind::Added, "resourceAdded"),
                ConditionDecl::ResourceRemoved { .. } => (ResourceKind::Removed, "resourceRemoved"),
                ConditionDecl::ResourceChangedOrRemoved { .. } => (ResourceKind::ChangedOrRemoved, "resourceChangedOrRemoved"),
                ConditionDecl::ResourceExistsAndChanged { .. } => (ResourceKind::Changed, "resourceExistsAndChanged"),
                _ => (ResourceKind::Changed, "resourceChanged"),
            };
            Condition::Resource { kind, id: resource_id(world, resource, c.ts, condition)?, slot: slot_of(world, slot_key) }
        }
        ConditionDecl::ResourceEquals { resource, value } | ConditionDecl::ResourceExistsAndEquals { resource, value } => {
            let condition = if matches!(d, ConditionDecl::ResourceEquals { .. }) { "resourceEquals" } else { "resourceExistsAndEquals" };
            let id = resource_id(world, resource, c.ts, condition)?;
            let (bytes, (fields, heap)) = encode_resource(world, resource, value, c, condition)?;
            Condition::ResourceEquals { id, bytes, fields, heap }
        }
        ConditionDecl::AnyWithComponent { component } => {
            let id = component_id(world, component, c.ts)?;
            Condition::AnyWith { query: hidden_query(world, &[id], &[]) }
        }
        ConditionDecl::AnyComponentRemoved { component } => Condition::AnyRemoved { id: component_id(world, component, c.ts)?, slot: slot_of(world, slot_key) },
        ConditionDecl::AnyMatchFilter { filter } => {
            let (mut with, mut without, mut changed, mut added) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for f in filter {
                match f {
                    FilterDecl::With(r) => with.push(component_id(world, r, c.ts)?),
                    FilterDecl::Without(r) => without.push(component_id(world, r, c.ts)?),
                    FilterDecl::Changed(r) => changed.push(component_id(world, r, c.ts)?),
                    FilterDecl::Added(r) => added.push(component_id(world, r, c.ts)?),
                    FilterDecl::Spawned | FilterDecl::Or(_) => {
                        return Err("anyMatchFilter takes With, Without, Changed and Added; Spawned and Or need a query".to_string().into());
                    }
                }
            }
            with.extend(changed.iter().chain(&added));
            Condition::AnyMatch { query: hidden_query(world, &with, &without), changed, added, slot: slot_of(world, slot_key) }
        }
        ConditionDecl::ConditionChanged { condition } => {
            let condition = Box::new(compile(world, condition, &child(0), c)?);
            Condition::Changed { condition, to: None, slot: slot_of(world, slot_key) }
        }
        ConditionDecl::ConditionChangedTo { condition, to } => {
            let condition = Box::new(compile(world, condition, &child(0), c)?);
            Condition::Changed { condition, to: Some(*to), slot: slot_of(world, slot_key) }
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
        ConditionDecl::Predicate { system, location } => {
            if !c.predicates.declared.contains(system) {
                return Err(CompileError { message: format!("runIf({}): not a predicate this generation declares", system.rsplit('#').next().unwrap_or(system)), location: location.clone() });
            }
            let memory = format!("predicate:{slot_key}");
            c.predicates.uses.push((system.clone(), memory.clone()));
            Condition::Predicate { system: system.clone(), memory }
        }
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

/// Runs a use of a TS predicate, by its memory key, and gives its return.
pub type CallPredicate<'a> = dyn FnMut(&mut World, &str) -> bool + 'a;

/// Evaluates a condition from an exclusive system, at the world's current tick. Stateful nodes
/// advance each time they are evaluated, as Bevy's condition systems do. `predicate` runs a TS
/// predicate, the only node that calls into JS.
pub fn evaluate(world: &mut World, c: &mut Condition, predicate: &mut CallPredicate) -> bool {
    let this_run = world.change_tick();
    // SAFETY: the world is held exclusively, and nothing borrows it while a predicate runs.
    unsafe { evaluate_cell(world.as_unsafe_world_cell(), this_run, c, Some(predicate)) }
}

/// Evaluates a condition at `this_run`. With `predicate`, the caller holds the world exclusively:
/// a TS predicate runs and a Rust message queue is read. Without it, those nodes are false; a
/// [`TsCondition`] holds none ([`Condition::needs_world`]).
///
/// # Safety
/// `world` may read everything [`Condition::access`] declares, and nothing writes it meanwhile;
/// with `predicate`, `world` is the whole world, held exclusively.
pub unsafe fn evaluate_cell(world: UnsafeWorldCell, this_run: Tick, c: &mut Condition, mut predicate: Option<&mut CallPredicate>) -> bool {
    // SAFETY (each read below): declared by `access`, per the caller's contract.
    unsafe {
        match c {
            Condition::InState { current, value, .. } => current(world) == Some(*value),
            Condition::StateExists { current, .. } => current(world).is_some(),
            Condition::StateChanged { state, slot } => {
                // The `State` resource exists once the machine has a value.
                let ticks = resource_ticks(world, *state);
                let last_run = std::mem::replace(&mut slot.lock().unwrap().last_run, this_run);
                ticks.is_some_and(|(_, changed)| changed.is_newer_than(last_run, this_run))
            }
            Condition::OnMessage { index, slot, .. } => {
                let Some(q) = world.get_resource::<TsPayloads>().and_then(|p| p.messages.get(*index)) else { return false };
                let cursor = std::mem::replace(&mut slot.lock().unwrap().messages, q.next_seq);
                q.has_unread(cursor)
            }
            Condition::OnRustMessage { reader, slot } => {
                if predicate.is_none() {
                    return false;
                }
                let world = world.world();
                let end = reader.end(world) as u64;
                let cursor = std::mem::replace(&mut slot.lock().unwrap().messages, end);
                reader.unread(world, cursor as usize)
            }
            Condition::RunOnce { slot } => !std::mem::replace(&mut slot.lock().unwrap().flag, true),
            Condition::ResourceExists { id } => id.is_some_and(|id| resource_location(world, id).is_some()),
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
            Condition::ResourceEquals { id, bytes, fields, heap } => {
                let Some(ptr) = id.and_then(|id| world.get_resource_by_id(id)) else { return false };
                // The resource's layout is the one `bytes` was encoded with: `bytes.len()` bytes.
                let have = std::slice::from_raw_parts(ptr.as_ptr(), bytes.len());
                let want = bytes.bytes();
                // The resource and `bytes` hold live heap values of these kinds at these offsets.
                fields.iter().all(|&(a, b)| have[a..b] == want[a..b]) && heap.iter().all(|&(off, kind)| crate::heap::eq_at(ptr.as_ptr(), off, kind, bytes))
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
                let now = evaluate_cell(world, this_run, condition, predicate.as_deref_mut());
                let before = std::mem::replace(&mut slot.lock().unwrap().previous, now);
                before != now && to.is_none_or(|t| t == now)
            }
            Condition::Not(c) => !evaluate_cell(world, this_run, c, predicate),
            Condition::All(cs) => {
                let mut ok = true;
                for c in cs.iter_mut() {
                    ok &= evaluate_cell(world, this_run, c, predicate.as_deref_mut());
                }
                ok
            }
            Condition::Combine { op, a, b, eager } => {
                let x = evaluate_cell(world, this_run, a, predicate.as_deref_mut());
                let decided = match op {
                    CombineOp::And | CombineOp::Nand => !x,
                    CombineOp::Or | CombineOp::Nor => x,
                    CombineOp::Xor | CombineOp::Xnor => false,
                };
                if decided && !*eager {
                    return matches!(op, CombineOp::Or | CombineOp::Nand);
                }
                let y = evaluate_cell(world, this_run, b, predicate);
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
            Condition::Predicate { memory, .. } => match predicate {
                Some(p) => p(world.world_mut(), memory),
                None => false,
            },
        }
    }
}

/// Where a resource's value lives: `None` once it was removed, though its entity may stay.
///
/// # Safety
/// The caller may read the world's metadata.
unsafe fn resource_location(world: UnsafeWorldCell, id: ComponentId) -> Option<Entity> {
    let e = unsafe { world.resource_entities() }.get(id)?;
    let loc = world.entities().get_spawned(e).ok()?;
    world.archetypes()[loc.archetype_id].contains(id).then_some(e)
}

/// A resource's added and changed ticks.
///
/// # Safety
/// The caller may read `id`'s ticks.
unsafe fn resource_ticks(world: UnsafeWorldCell, id: ComponentId) -> Option<(Tick, Tick)> {
    let entity = unsafe { resource_location(world, id) }?;
    let ticks = unsafe { world.get_entity(entity).ok()?.get_change_ticks_by_id(id)? };
    Some((ticks.added, ticks.changed))
}

impl Condition {
    /// Whether evaluating it needs the whole world on the main thread: a TS predicate calls into
    /// JS, and a Rust message queue is read through reflection. Such a condition is evaluated by
    /// an exclusive gate system rather than by a [`TsCondition`].
    pub fn needs_world(&self) -> bool {
        match self {
            Condition::Predicate { .. } | Condition::OnRustMessage { .. } => true,
            Condition::Changed { condition, .. } | Condition::Not(condition) => condition.needs_world(),
            Condition::All(cs) => cs.iter().any(Condition::needs_world),
            Condition::Combine { a, b, .. } => a.needs_world() || b.needs_world(),
            _ => false,
        }
    }

    /// Adds what evaluating the condition reads to `access`.
    pub fn access(&self, access: &mut SystemAccess) {
        let read = |access: &mut SystemAccess, id: ComponentId| {
            let mut f = FilteredAccess::default();
            f.add_read(id);
            f.and_with(IS_RESOURCE);
            access.ensure_filtered_access(f);
        };
        match self {
            Condition::InState { state, .. } | Condition::StateExists { state, .. } | Condition::StateChanged { state, .. } => read(access, *state),
            Condition::OnMessage { payloads, .. } => read(access, *payloads),
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
            // Evaluated by an exclusive gate, which holds the whole world.
            Condition::OnRustMessage { .. } | Condition::Predicate { .. } => *access = SystemAccess::Exclusive,
        }
    }

    /// What the condition was built from: ids, indices and native instances. Two equal ones
    /// gate the same way, so a rebuild may keep the old condition system.
    pub fn fingerprint(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = match self {
            Condition::InState { state, value, .. } => write!(out, "in({state:?},{value})"),
            Condition::StateExists { state, .. } => write!(out, "state({state:?})"),
            Condition::StateChanged { state, .. } => write!(out, "statechanged({state:?})"),
            Condition::OnMessage { index, .. } => write!(out, "msg({index})"),
            Condition::OnRustMessage { .. } => write!(out, "rustmsg"),
            Condition::RunOnce { .. } => write!(out, "once"),
            Condition::ResourceExists { id } => write!(out, "exists({id:?})"),
            Condition::Resource { kind, id, .. } => write!(out, "res({kind:?},{id:?})"),
            Condition::ResourceEquals { id, bytes, .. } => write!(out, "eq({id:?},{:?})", bytes.bytes()),
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
            Condition::Predicate { system, memory } => write!(out, "pred({system},{memory})"),
        };
    }
}

/// A compiled condition as a Bevy condition system: the `run_if` of a TS system, or of a group's
/// set for a collective `runIf`. It declares what it reads, so it runs beside any Rust system
/// that does not write it. A condition that [needs the world](Condition::needs_world) is not one.
pub struct TsCondition {
    name: DebugName,
    condition: Condition,
    last_run: Tick,
}

impl TsCondition {
    pub fn new(name: String, condition: Condition) -> Self {
        debug_assert!(!condition.needs_world(), "{name}: a condition needing the world is evaluated by a gate");
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
        let pass = unsafe { evaluate_cell(world, this_run, &mut self.condition, None) };
        self.last_run = this_run;
        Ok(pass)
    }

    fn apply_deferred(&mut self, _world: &mut World) {}

    fn queue_deferred(&mut self, _world: DeferredWorld) {}

    fn initialize(&mut self, world: &mut World) -> SystemAccess {
        self.last_run = crate::bevy_system::never_ran(world.change_tick());
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

// SAFETY: a condition only reads the world: `evaluate_cell` writes nothing it declares, and its
// state lives in its own handles.
unsafe impl ReadOnlySystem for TsCondition {}

/// Drops native condition instances the live generation no longer uses.
pub(crate) fn prune_natives(world: &mut World, live: &HashSet<String>) {
    if let Some(mut n) = world.get_resource_mut::<NativeConditions>() {
        n.0.retain(|k, _| live.contains(k));
    }
}
