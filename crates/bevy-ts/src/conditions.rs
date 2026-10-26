//! Run conditions, compiled from the manifest and evaluated in Rust with no call into JS: Bevy
//! 0.20's built-in conditions under their names, the combinators, and conditions a crate exposes
//! with `app.ts_condition`. A stateful condition keeps its tick, cursor or previous value in
//! [`ConditionMemory`], per gated config and position in its tree, so a reload keeps it and a
//! restart clears it.

use crate::backend::SourceLocation;
use crate::layout::{ComponentLayout, NativeLayouts};
use crate::manifest::{CombineOp, ComponentRef, ComponentSource, ConditionDecl, FilterDecl};
use crate::messages::TsPayloads;
use crate::systems::{TsEncode, TsLookup, rust_component_by_name};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::lifecycle::RemovedComponentEntity;
use bevy::ecs::message::MessageCursor;
use bevy::ecs::schedule::BoxedCondition;
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::Location;
use std::sync::{Arc, Mutex};

/// A run condition, compiled for Rust. `slot` keys a stateful node's memory.
pub enum Condition {
    /// A TS machine's slot and a value's symbol.
    InState { slot: u8, value: u32 },
    /// Bevy's `state_changed` on a TS machine: its `State` changed since this condition ran.
    StateChanged { slot: u8, key: String },
    StateExists { slot: u8 },
    /// Messages written since this condition last ran: its own cursor, as Bevy's `on_message`.
    OnMessage { index: usize, slot: String },
    RunOnce { slot: String },
    ResourceExists { id: Option<ComponentId> },
    Resource { kind: ResourceKind, id: Option<ComponentId>, slot: String },
    ResourceEquals { id: Option<ComponentId>, bytes: Vec<u8>, fields: Vec<(usize, usize)> },
    AnyWith { query: Box<QueryState<Entity>> },
    AnyRemoved { id: ComponentId, slot: String },
    AnyMatch { query: Box<QueryState<Entity>>, changed: Vec<ComponentId>, added: Vec<ComponentId>, slot: String },
    Changed { condition: Box<Condition>, to: Option<bool>, slot: String },
    Not(Box<Condition>),
    /// Several `runIf` calls: every one is evaluated, as Bevy folds a system's conditions.
    All(Vec<Condition>),
    Combine { op: CombineOp, a: Box<Condition>, b: Box<Condition>, eager: bool },
    Native { system: Arc<Mutex<BoxedCondition>> },
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
pub struct ConditionMemory(pub HashMap<String, ConditionState>);

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
/// stay the same, so the condition's own state survives a reload.
#[derive(Resource, Default)]
pub struct NativeConditions(pub HashMap<String, Arc<Mutex<BoxedCondition>>>);

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

/// A resource condition's resource: `None` when no type has the name (the condition is never
/// true), and an error, under the condition's name, when a short name names several Rust types.
fn resource_id(world: &World, name: &str, ts: &TsLookup, condition: &str) -> Result<Option<ComponentId>, String> {
    if let Some((id, _)) = ts(name) {
        return Ok(Some(id));
    }
    if let Some(n) = world.resource::<NativeLayouts>().get(name) {
        return Ok(Some(n.id));
    }
    rust_component_by_name(world, name).map_err(|e| format!("{condition}({name}): {e}"))
}

fn component_id(world: &World, r: &ComponentRef, ts: &TsLookup) -> Result<ComponentId, String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).map(|(id, _)| id).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => world
            .resource::<NativeLayouts>()
            .get(&r.name)
            .map(|n| n.id)
            .map_or_else(|| rust_component_by_name(world, &r.name), |id| Ok(Some(id)))?
            .ok_or_else(|| format!("no Rust component named {}", r.name)),
    }
}

/// Byte ranges `[start, end)` of a layout's fields.
type FieldRanges = Vec<(usize, usize)>;

/// A resource value as native bytes, and the byte ranges of its fields (padding left out).
fn encode_resource(world: &World, name: &str, values: &serde_json::Map<String, serde_json::Value>, c: &Compile, condition: &str) -> Result<(Vec<u8>, FieldRanges), String> {
    let ranges = |l: &ComponentLayout| l.fields.iter().map(|f| (f.offset, f.offset + f.ty.size())).collect::<Vec<_>>();
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
    let slot = || slot_key.clone();
    let slot_key = &slot_key;
    let child = |i: usize| if path.is_empty() { i.to_string() } else { format!("{path}.{i}") };
    Ok(match d {
        ConditionDecl::InState { state, value } => {
            let payloads = world.resource::<TsPayloads>();
            let m = &payloads.states[payloads.state(state).ok_or_else(|| format!("no state {state}"))?];
            Condition::InState { slot: m.slot, value: m.sym(value).ok_or_else(|| format!("{state} has no value {value}"))? }
        }
        ConditionDecl::StateChanged { state } | ConditionDecl::StateExists { state } => {
            let payloads = world.resource::<TsPayloads>();
            let slot = payloads.states[payloads.state(state).ok_or_else(|| format!("no state {state}"))?].slot;
            if matches!(d, ConditionDecl::StateChanged { .. }) { Condition::StateChanged { slot, key: slot_key.clone() } } else { Condition::StateExists { slot } }
        }
        ConditionDecl::OnMessage { message } => {
            let index = world.resource::<TsPayloads>().message(message).ok_or_else(|| format!("no message {message}"))?;
            Condition::OnMessage { index, slot: slot() }
        }
        ConditionDecl::RunOnce => Condition::RunOnce { slot: slot() },
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
            Condition::Resource { kind, id: resource_id(world, resource, c.ts, condition)?, slot: slot() }
        }
        ConditionDecl::ResourceEquals { resource, value } | ConditionDecl::ResourceExistsAndEquals { resource, value } => {
            let condition = if matches!(d, ConditionDecl::ResourceEquals { .. }) { "resourceEquals" } else { "resourceExistsAndEquals" };
            let id = resource_id(world, resource, c.ts, condition)?;
            let (bytes, fields) = encode_resource(world, resource, value, c, condition)?;
            Condition::ResourceEquals { id, bytes, fields }
        }
        ConditionDecl::AnyWithComponent { component } => {
            let id = component_id(world, component, c.ts)?;
            Condition::AnyWith { query: hidden_query(world, &[id], &[]) }
        }
        ConditionDecl::AnyComponentRemoved { component } => Condition::AnyRemoved { id: component_id(world, component, c.ts)?, slot: slot() },
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
            Condition::AnyMatch { query: hidden_query(world, &with, &without), changed, added, slot: slot() }
        }
        ConditionDecl::ConditionChanged { condition } => {
            Condition::Changed { condition: Box::new(compile(world, condition, &child(0), c)?), to: None, slot: slot() }
        }
        ConditionDecl::ConditionChangedTo { condition, to } => {
            Condition::Changed { condition: Box::new(compile(world, condition, &child(0), c)?), to: Some(*to), slot: slot() }
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
            let key = format!("{}|{name}|{args}", slot());
            c.natives.insert(key.clone());
            if let Some(system) = world.get_resource::<NativeConditions>().and_then(|n| n.0.get(&key)).cloned() {
                return Ok(Condition::Native { system });
            }
            let mut system = (exposed.factory)(args).map_err(|e| at(format!("nativeCondition({name:?}): {e}")))?;
            system.initialize(world);
            let system = Arc::new(Mutex::new(system));
            world.get_resource_or_init::<NativeConditions>().0.insert(key, system.clone());
            Condition::Native { system }
        }
    })
}

fn state<'w>(world: &'w mut World, slot: &str) -> &'w mut ConditionState {
    world.get_resource_or_init::<ConditionMemory>().into_inner().0.entry(slot.to_string()).or_default()
}

/// Evaluates a condition. Stateful nodes advance each time they are evaluated, as Bevy's
/// condition systems do.
pub fn evaluate(world: &mut World, c: &mut Condition) -> bool {
    match c {
        Condition::InState { slot, value } => crate::states::current_of(world, *slot) == Some(*value),
        Condition::StateExists { slot } => crate::states::current_of(world, *slot).is_some(),
        Condition::StateChanged { slot, key } => {
            // The `State` resource exists once the machine has a value.
            let this_run = world.change_tick();
            let ticks = crate::states::state_id_of(world, *slot).and_then(|id| world.get_resource_change_ticks_by_id(id));
            let last_run = std::mem::replace(&mut state(world, key).last_run, this_run);
            ticks.is_some_and(|t| t.is_changed(last_run, this_run))
        }
        Condition::OnMessage { index, slot } => {
            let Some(next) = world.resource::<TsPayloads>().messages.get(*index).map(|q| q.next_seq) else { return false };
            let cursor = std::mem::replace(&mut state(world, slot).messages, next);
            world.resource::<TsPayloads>().messages[*index].has_unread(cursor)
        }
        Condition::RunOnce { slot } => !std::mem::replace(&mut state(world, slot).flag, true),
        Condition::ResourceExists { id } => id.is_some_and(|id| world.resource_entities().get(id).is_some()),
        Condition::Resource { kind, id, slot } => {
            let this_run = world.change_tick();
            let ticks = id.and_then(|id| world.get_resource_change_ticks_by_id(id));
            let st = state(world, slot);
            let last_run = std::mem::replace(&mut st.last_run, this_run);
            match (kind, ticks) {
                (ResourceKind::Added, t) => t.is_some_and(|t| t.is_added(last_run, this_run)),
                (ResourceKind::Changed, t) => t.is_some_and(|t| t.is_changed(last_run, this_run)),
                (ResourceKind::Removed, Some(_)) => {
                    st.flag = true;
                    false
                }
                (ResourceKind::ChangedOrRemoved, Some(t)) => {
                    st.flag = true;
                    t.is_changed(last_run, this_run)
                }
                (ResourceKind::Removed | ResourceKind::ChangedOrRemoved, None) => std::mem::replace(&mut st.flag, false),
            }
        }
        Condition::ResourceEquals { id, bytes, fields } => {
            let Some(ptr) = id.and_then(|id| world.get_resource_by_id(id)) else { return false };
            // SAFETY: the resource's layout is the one `bytes` was encoded with: `bytes.len()` bytes.
            let have = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), bytes.len()) };
            fields.iter().all(|&(a, b)| have[a..b] == bytes[a..b])
        }
        Condition::AnyWith { query } => {
            query.update_archetypes(world);
            query.iter_manual(world).next().is_some()
        }
        Condition::AnyRemoved { id, slot } => {
            let id = *id;
            let mut cursor = std::mem::take(&mut state(world, slot).removals);
            let any = world.removed_components().get(id).is_some_and(|m| cursor.read(m).count() > 0);
            state(world, slot).removals = cursor;
            any
        }
        Condition::AnyMatch { query, changed, added, slot } => {
            let this_run = world.change_tick();
            let last_run = std::mem::replace(&mut state(world, slot).last_run, this_run);
            query.update_archetypes(world);
            if changed.is_empty() && added.is_empty() {
                return query.iter_manual(world).next().is_some();
            }
            // Per entity, so sparse-set components and the query's `Without` are honoured.
            query.iter_manual(world).any(|e| {
                let Ok(row) = world.get_entity(e) else { return false };
                changed.iter().all(|id| row.get_change_ticks_by_id(*id).is_some_and(|t| t.is_changed(last_run, this_run)))
                    && added.iter().all(|id| row.get_change_ticks_by_id(*id).is_some_and(|t| t.is_added(last_run, this_run)))
            })
        }
        Condition::Changed { condition, to, slot } => {
            let now = evaluate(world, condition);
            let st = state(world, slot);
            let before = std::mem::replace(&mut st.previous, now);
            before != now && to.is_none_or(|t| t == now)
        }
        Condition::Not(c) => !evaluate(world, c),
        Condition::All(cs) => {
            let mut ok = true;
            for c in cs.iter_mut() {
                ok &= evaluate(world, c);
            }
            ok
        }
        Condition::Combine { op, a, b, eager } => {
            let x = evaluate(world, a);
            let decided = match op {
                CombineOp::And | CombineOp::Nand => !x,
                CombineOp::Or | CombineOp::Nor => x,
                CombineOp::Xor | CombineOp::Xnor => false,
            };
            if decided && !*eager {
                return matches!(op, CombineOp::Or | CombineOp::Nand);
            }
            let y = evaluate(world, b);
            match op {
                CombineOp::And => x && y,
                CombineOp::Or => x || y,
                CombineOp::Nand => !(x && y),
                CombineOp::Nor => !(x || y),
                CombineOp::Xor => x ^ y,
                CombineOp::Xnor => !(x ^ y),
            }
        }
        Condition::Native { system } => system.lock().unwrap().run_readonly((), world).unwrap_or(false),
    }
}

/// Drops native condition instances the live generation no longer uses.
pub(crate) fn prune_natives(world: &mut World, live: &HashSet<String>) {
    if let Some(mut n) = world.get_resource_mut::<NativeConditions>() {
        n.0.retain(|k, _| live.contains(k));
    }
}
