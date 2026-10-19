//! TS systems: resolving their params, ordering them, evaluating run conditions in Rust, and
//! running them over zero-copy column views with per-row change ticks.

use crate::backend::{CallContext, ColumnFrame, ParamFrame, ScriptError, ScriptKey, SystemRun, TableFrame};
use crate::layout::{ComponentLayout, FieldType, NativeLayouts};
use crate::manifest::{
    ComponentRef, ComponentSource, ConditionDecl, DataDecl, FilterDecl, Lifecycle, ParamDecl, ScheduleDecl, Stage, SystemDecl,
};
use crate::memory::SystemMemory;
use crate::messages::TsPayloads;
use crate::observers::{ObserverQueue, Trigger};
use crate::runtime::{self, ScriptRuntime};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::storage::TableId;
use bevy::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

// JS stores ticks as u32 straight into Bevy's tick column.
const _: () = assert!(size_of::<Tick>() == 4 && align_of::<Tick>() == 4);

/// The resource name `Res<Time>` takes, and its computed layout.
pub const TIME: &str = "Time";

pub fn time_layout() -> ComponentLayout {
    ComponentLayout::c_layout([("delta", &FieldType::F32), ("elapsed", &FieldType::F64), ("frame", &FieldType::U32)])
}

/// One data column of a query.
#[derive(Clone, Debug)]
pub struct Access {
    pub name: String,
    pub id: ComponentId,
    pub size: usize,
    pub write: bool,
    pub optional: bool,
}

pub struct QueryParam {
    state: QueryState<Entity>,
    /// Data components in order (`Entity` data is not a column).
    pub columns: Vec<Access>,
    changed: Vec<ComponentId>,
    added: Vec<ComponentId>,
}

/// A resolved param, flattened (bundles depth-first) in the order frames are handed over.
pub enum RunParam {
    Query(Box<QueryParam>),
    Res { name: String, id: ComponentId, size: usize, write: bool },
    Time,
    /// The nth `Local` of the system.
    Local(usize),
    MessageReader { index: usize },
    State { index: usize },
    /// An observer's trigger.
    Event,
    None,
}

/// A run condition, compiled for Rust.
#[derive(Clone, Debug)]
pub enum Condition {
    InState { index: usize, value: u32 },
    ResourceExists { id: Option<ComponentId> },
    OnMessage { index: usize, message: String },
    Not(Box<Condition>),
    All(Vec<Condition>),
}

/// What an observer listens to.
#[derive(Clone, Debug, PartialEq)]
pub struct ObserverOf(pub Trigger);

/// A live TS system or observer.
pub struct TsSystem {
    pub key: ScriptKey,
    pub decl: SystemDecl,
    pub params: Vec<RunParam>,
    pub condition: Option<Condition>,
    pub observes: Option<ObserverOf>,
    pub last_run_us: u64,
    pub last_error: Option<ScriptError>,
    warned_read_write: bool,
}

/// The live generation's systems, by schedule, in run order.
#[derive(Resource, Default)]
pub struct TsSystems {
    pub stages: BTreeMap<StageKey, Vec<TsSystem>>,
    pub startup: Vec<TsSystem>,
    pub on_enter: HashMap<(usize, u32), Vec<TsSystem>>,
    pub on_exit: HashMap<(usize, u32), Vec<TsSystem>>,
    pub observers: Vec<TsSystem>,
    pub generation: u64,
}

/// `Stage` as an ordered map key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StageKey(pub u8);

impl From<Stage> for StageKey {
    fn from(s: Stage) -> Self {
        StageKey(s as u8)
    }
}

impl TsSystems {
    pub fn iter(&self) -> impl Iterator<Item = &TsSystem> {
        self.startup
            .iter()
            .chain(self.stages.values().flatten())
            .chain(self.on_enter.values().flatten())
            .chain(self.on_exit.values().flatten())
            .chain(self.observers.iter())
    }

    /// The observers a lifecycle trigger needs Bevy to watch.
    pub fn watched_lifecycles(&self) -> Vec<(Lifecycle, ComponentId)> {
        self.observers
            .iter()
            .filter_map(|o| match &o.observes {
                Some(ObserverOf(Trigger::Lifecycle(k, id))) => Some((*k, *id)),
                _ => None,
            })
            .collect()
    }
}

/// Resolves a TS component name to its id and size in the generation being applied.
pub type TsLookup<'a> = dyn Fn(&str) -> Option<(ComponentId, usize)> + 'a;

/// Finds a registered Rust component by type path or by its short name.
pub fn rust_component_by_name(world: &World, name: &str) -> Option<ComponentId> {
    let comps = world.components();
    let mut short = None;
    for (id, info) in comps.iter_registered() {
        let full = info.name().to_string();
        if full == name {
            return Some(id);
        }
        if full.rsplit("::").next() == Some(name) {
            short = Some(id);
        }
    }
    short
}

/// Resolves a component reference as query data (TS, or native layout) or as a filter (any
/// registered component).
fn resolve_ref(world: &World, r: &ComponentRef, ts: &TsLookup, native: &NativeLayouts, data: bool) -> Result<(ComponentId, usize), String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => {
            if let Some(n) = native.get(&r.name).or_else(|| native.iter().find(|n| n.type_path == r.name)) {
                return Ok((n.id, n.layout.size));
            }
            if data {
                return Err(format!(
                    "{} is a Rust component without a native layout; it can be used in With/Without but not as query data",
                    r.name
                ));
            }
            rust_component_by_name(world, &r.name).map(|id| (id, 0)).ok_or_else(|| format!("no Rust component named {}", r.name))
        }
    }
}

/// Orders systems by registration order adjusted by `after`/`before` (on systems or sets). Errors
/// on unknown targets and cycles.
fn order(decls: &[(usize, &SystemDecl)]) -> Result<Vec<usize>, ScriptError> {
    let n = decls.len();
    let mut members: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (_, d)) in decls.iter().enumerate() {
        members.entry(d.name.as_str()).or_default().push(i);
        for s in &d.sets {
            members.entry(s.as_str()).or_default().push(i);
        }
    }
    let mut indegree = vec![0; n];
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut edge = |from: usize, to: usize, indegree: &mut Vec<usize>| {
        if from != to && edges[from].insert(to) {
            indegree[to] += 1;
        }
    };
    for (i, (_, d)) in decls.iter().enumerate() {
        for (targets, after) in [(&d.after, true), (&d.before, false)] {
            for t in targets {
                let Some(js) = members.get(t.as_str()) else {
                    return Err(ScriptError::at(
                        format!("system {} is ordered {} {t}, which is no system or set in its schedule", d.name, if after { "after" } else { "before" }),
                        d.location.clone(),
                    ));
                };
                for &j in js {
                    if after { edge(j, i, &mut indegree) } else { edge(i, j, &mut indegree) }
                }
            }
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut ready: std::collections::BTreeSet<usize> = (0..n).filter(|i| indegree[*i] == 0).collect();
    while let Some(i) = ready.pop_first() {
        out.push(i);
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                ready.insert(j);
            }
        }
    }
    if out.len() < n {
        let stuck: Vec<&str> = (0..n).filter(|i| indegree[*i] > 0).map(|i| decls[i].1.name.as_str()).collect();
        let d = decls.iter().find(|(_, d)| d.name == stuck[0]).unwrap().1;
        return Err(ScriptError::at(format!("cycle in system ordering among {}", stuck.join(", ")), d.location.clone()));
    }
    Ok(out.into_iter().map(|i| decls[i].0).collect())
}

fn compile_condition(world: &World, c: &ConditionDecl, ts: &TsLookup) -> Result<Condition, String> {
    let payloads = world.resource::<TsPayloads>();
    Ok(match c {
        ConditionDecl::InState { state, value } => {
            let index = payloads.state(state).ok_or_else(|| format!("no state {state}"))?;
            let value = payloads.states[index].index(value).ok_or_else(|| format!("{state} has no value {value}"))?;
            Condition::InState { index, value }
        }
        ConditionDecl::ResourceExists { resource } => Condition::ResourceExists { id: ts(resource).map(|(id, _)| id) },
        ConditionDecl::OnMessage { message } => {
            Condition::OnMessage { index: payloads.message(message).ok_or_else(|| format!("no message {message}"))?, message: message.clone() }
        }
        ConditionDecl::Not { condition } => Condition::Not(Box::new(compile_condition(world, condition, ts)?)),
        ConditionDecl::All { conditions } => Condition::All(conditions.iter().map(|c| compile_condition(world, c, ts)).collect::<Result<_, _>>()?),
    })
}

fn evaluate(world: &World, c: &Condition, identity: &str) -> bool {
    match c {
        Condition::InState { index, value } => world.resource::<TsPayloads>().states.get(*index).is_some_and(|s| s.current == Some(*value)),
        Condition::ResourceExists { id } => id.is_some_and(|id| world.resource_entities().get(id).is_some()),
        Condition::OnMessage { index, message } => {
            let cursor = world.resource::<SystemMemory>().0.get(identity).and_then(|m| m.cursors.get(message)).copied().unwrap_or(0);
            world.resource::<TsPayloads>().messages.get(*index).is_some_and(|q| q.has_unread(cursor))
        }
        Condition::Not(c) => !evaluate(world, c, identity),
        Condition::All(cs) => cs.iter().all(|c| evaluate(world, c, identity)),
    }
}

/// A schedule's systems, with their indices in the manifest.
type ScheduleGroup<'a> = (ScheduleDecl, Vec<(usize, &'a SystemDecl)>);

/// Builds the systems of a generation. Fails on unusable params, duplicate identities and bad
/// ordering. `TsPayloads` must already hold the generation's messages, events and states.
pub fn build(world: &mut World, generation: u64, decls: &[SystemDecl], ts: &TsLookup) -> Result<TsSystems, Vec<ScriptError>> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for d in decls {
        if d.schedule != ScheduleDecl::Observer && !seen.insert((d.name.as_str(), format!("{:?}", d.schedule))) {
            errors.push(ScriptError::at(format!("system {} is registered twice in one schedule", d.name), d.location.clone()));
        }
    }
    let mut out = TsSystems { generation, ..Default::default() };
    let mut groups: BTreeMap<String, ScheduleGroup> = BTreeMap::new();
    for (i, d) in decls.iter().enumerate() {
        if d.schedule == ScheduleDecl::Observer {
            match build_one(world, generation, i, d, ts) {
                Ok(s) => out.observers.push(s),
                Err(e) => errors.push(e),
            }
            continue;
        }
        groups.entry(format!("{:?}", d.schedule)).or_insert_with(|| (d.schedule.clone(), Vec::new())).1.push((i, d));
    }
    for (_, (schedule, list)) in groups {
        let ordered = match order(&list) {
            Ok(o) => o,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        let mut built = Vec::new();
        for index in ordered {
            match build_one(world, generation, index, &decls[index], ts) {
                Ok(s) => built.push(s),
                Err(e) => errors.push(e),
            }
        }
        let payloads = world.resource::<TsPayloads>();
        let state_key = |state: &str, value: &str| -> Result<(usize, u32), String> {
            let i = payloads.state(state).ok_or_else(|| format!("no state {state}"))?;
            Ok((i, payloads.states[i].index(value).ok_or_else(|| format!("{state} has no value {value}"))?))
        };
        match &schedule {
            ScheduleDecl::Startup => out.startup = built,
            ScheduleDecl::Stage { stage } => {
                out.stages.insert((*stage).into(), built);
            }
            ScheduleDecl::OnEnter { state, value } | ScheduleDecl::OnExit { state, value } => match state_key(state, value) {
                Ok(k) => {
                    let map = if matches!(schedule, ScheduleDecl::OnEnter { .. }) { &mut out.on_enter } else { &mut out.on_exit };
                    map.insert(k, built);
                }
                Err(e) => errors.push(ScriptError::new(e)),
            },
            ScheduleDecl::Observer => unreachable!(),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

fn build_query(world: &mut World, data: &[DataDecl], filter: &[FilterDecl], ts: &TsLookup) -> Result<QueryParam, String> {
    let native = world.resource::<NativeLayouts>().clone();
    let mut columns = Vec::new();
    for d in data {
        if let DataDecl::Component { component, mutable, optional } = d {
            let (id, size) = resolve_ref(world, component, ts, &native, true)?;
            if columns.iter().any(|c: &Access| c.id == id && c.write != *mutable) {
                // The same component read and written in one query: written wins.
            }
            columns.push(Access { name: component.name.clone(), id, size, write: *mutable, optional: *optional });
        }
    }
    let mut with = Vec::new();
    let mut without = Vec::new();
    let mut changed = Vec::new();
    let mut added = Vec::new();
    for f in filter {
        match f {
            FilterDecl::With(r) => with.push(resolve_ref(world, r, ts, &native, false)?.0),
            FilterDecl::Without(r) => without.push(resolve_ref(world, r, ts, &native, false)?.0),
            FilterDecl::Changed(r) => changed.push(resolve_ref(world, r, ts, &native, false)?.0),
            FilterDecl::Added(r) => added.push(resolve_ref(world, r, ts, &native, false)?.0),
        }
    }
    let mut b = QueryBuilder::<Entity>::new(world);
    for c in &columns {
        match (c.optional, c.write) {
            (false, true) => {
                b.mut_id(c.id);
            }
            (false, false) => {
                b.ref_id(c.id);
            }
            (true, write) => {
                let id = c.id;
                b.optional(|b| {
                    if write {
                        b.mut_id(id);
                    } else {
                        b.ref_id(id);
                    }
                });
            }
        }
    }
    for id in with.iter().chain(&changed).chain(&added) {
        b.with_id(*id);
    }
    for id in without {
        b.without_id(id);
    }
    Ok(QueryParam { state: b.build(), columns, changed, added })
}

fn flatten_params<'a>(params: &'a [ParamDecl], out: &mut Vec<&'a ParamDecl>) {
    for p in params {
        match p {
            ParamDecl::Bundle { fields } => {
                for f in fields {
                    flatten_params(std::slice::from_ref(&f.param), out);
                }
            }
            p => out.push(p),
        }
    }
}

fn build_one(world: &mut World, generation: u64, index: usize, d: &SystemDecl, ts: &TsLookup) -> Result<TsSystem, ScriptError> {
    let err = |m: String| ScriptError::at(format!("system {}: {m}", d.name), d.location.clone());
    let mut flat = Vec::new();
    flatten_params(&d.params, &mut flat);
    let mut params = Vec::new();
    let mut locals = Vec::new();
    let mut observes = None;
    for (i, p) in flat.iter().enumerate() {
        let rp = match p {
            ParamDecl::Query { data, filter } => RunParam::Query(Box::new(build_query(world, data, filter, ts).map_err(err)?)),
            ParamDecl::Res { resource } | ParamDecl::ResMut { resource } => {
                let write = matches!(p, ParamDecl::ResMut { .. });
                if resource == TIME || resource.ends_with("::Time") {
                    if write {
                        return Err(err("ResMut<Time> is not allowed; Time is read-only from TS".into()));
                    }
                    RunParam::Time
                } else {
                    let (id, size) = ts(resource).ok_or_else(|| err(format!("no resource named {resource}")))?;
                    RunParam::Res { name: resource.clone(), id, size, write }
                }
            }
            ParamDecl::Local { fields } => {
                locals.push(fields.clone());
                RunParam::Local(locals.len() - 1)
            }
            ParamDecl::Commands | ParamDecl::MessageWriter { .. } | ParamDecl::NextState { .. } => RunParam::None,
            ParamDecl::MessageReader { message } => {
                let index = world.resource::<TsPayloads>().message(message).ok_or_else(|| err(format!("no message type {message}; call app.addMessage")))?;
                RunParam::MessageReader { index }
            }
            ParamDecl::State { state } => {
                RunParam::State { index: world.resource::<TsPayloads>().state(state).ok_or_else(|| err(format!("no state {state}")))? }
            }
            ParamDecl::On { event, lifecycle, component } => {
                if i != 0 || d.schedule != ScheduleDecl::Observer {
                    return Err(err("On<...> must be the first param of an observer".into()));
                }
                let trigger = match (event, lifecycle, component) {
                    (Some(e), _, _) => Trigger::Event(world.resource::<TsPayloads>().event(e).ok_or_else(|| err(format!("no event type {e}")))?),
                    (None, Some(k), Some(c)) => {
                        let native = world.resource::<NativeLayouts>().clone();
                        Trigger::Lifecycle(*k, resolve_ref(world, c, ts, &native, false).map_err(err)?.0)
                    }
                    _ => return Err(err("On<...> names no event".into())),
                };
                observes = Some(ObserverOf(trigger));
                RunParam::Event
            }
            ParamDecl::Bundle { .. } => unreachable!("flattened"),
        };
        params.push(rp);
    }
    if d.schedule == ScheduleDecl::Observer && observes.is_none() {
        return Err(err("an observer's first param must be On<...>".into()));
    }
    let condition = d.condition.as_ref().map(|c| compile_condition(world, c, ts)).transpose().map_err(err)?;
    let local_refs: Vec<&[crate::manifest::FieldDecl]> = locals.iter().map(|l| l.as_slice()).collect();
    world.get_resource_or_init::<SystemMemory>().declare(&d.name, &local_refs);
    Ok(TsSystem {
        key: ScriptKey { generation, index: index as u32 },
        decl: d.clone(),
        params,
        condition,
        observes,
        last_run_us: 0,
        last_error: None,
        warned_read_write: false,
    })
}

/// A fast 64-bit hash of a column, for the dev-only check that read columns stay unwritten.
fn hash_bytes(b: &[u8]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    let (chunks, rest) = b.as_chunks::<8>();
    for c in chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(0x5851_F42D_4C95_7F2D);
    }
    for &x in rest {
        h = (h.rotate_left(5) ^ x as u64).wrapping_mul(0x5851_F42D_4C95_7F2D);
    }
    h
}

/// Per-runtime scratch kept between runs so little allocates per frame once warm.
#[derive(Default)]
pub(crate) struct Scratch {
    pub(crate) arena: Vec<u8>,
    entities: Vec<Vec<u32>>,
    rows: Vec<Vec<u32>>,
    time: Vec<u64>,
}

/// Runs every system of a stage. Called from the stage's exclusive system.
pub fn run_stage(world: &mut World, stage: Stage) {
    if !runtime::stage_should_run(world) {
        return;
    }
    with_systems(world, |world, rt, systems| {
        drain_observers(world, rt, systems);
        let mut list = systems.stages.remove(&stage.into()).unwrap_or_default();
        for sys in list.iter_mut() {
            run_one(world, rt, sys, None);
            drain_observers(world, rt, systems);
        }
        systems.stages.insert(stage.into(), list);
    });
}

/// `StateTransition`: applies each pending TS state value, running `OnExit(old)` then
/// `OnEnter(new)` systems.
pub fn run_state_transitions(world: &mut World) {
    if !runtime::stage_should_run(world) {
        return;
    }
    let pending: Vec<(usize, Option<u32>, u32)> = {
        let mut p = world.resource_mut::<TsPayloads>();
        p.states
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| s.next.take().filter(|n| Some(*n) != s.current).map(|n| (i, s.current.replace(n), n)))
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    with_systems(world, |world, rt, systems| {
        for (i, old, new) in pending {
            if let Some(old) = old {
                let mut list = systems.on_exit.remove(&(i, old)).unwrap_or_default();
                run_list(world, rt, systems, &mut list);
                systems.on_exit.insert((i, old), list);
            }
            let mut list = systems.on_enter.remove(&(i, new)).unwrap_or_default();
            run_list(world, rt, systems, &mut list);
            systems.on_enter.insert((i, new), list);
        }
    });
}

/// Runs a list of systems held outside `TsSystems` (startup, state transitions).
pub(crate) fn run_list(world: &mut World, rt: &mut ScriptRuntime, systems: &mut TsSystems, list: &mut [TsSystem]) {
    for sys in list.iter_mut() {
        run_one(world, rt, sys, None);
        drain_observers(world, rt, systems);
    }
}

/// Takes the runtime and the systems out of the world for `f`.
pub(crate) fn with_systems(world: &mut World, f: impl FnOnce(&mut World, &mut ScriptRuntime, &mut TsSystems)) {
    let Some(mut rt) = world.remove_non_send::<ScriptRuntime>() else { return };
    let Some(mut systems) = world.remove_resource::<TsSystems>() else {
        world.insert_non_send(rt);
        return;
    };
    f(world, &mut rt, &mut systems);
    world.insert_resource(systems);
    rt.free_pool(world);
    world.insert_non_send(rt);
}

/// Runs queued observers until the queue is empty (bounded, so a trigger loop cannot hang).
pub(crate) fn drain_observers(world: &mut World, rt: &mut ScriptRuntime, systems: &mut TsSystems) {
    for _ in 0..10_000 {
        let Some(p) = world.get_resource_mut::<ObserverQueue>().and_then(|mut q| q.0.pop_front()) else { return };
        let targets: Vec<Option<Entity>> = if p.path.is_empty() { vec![None] } else { p.path.iter().copied().map(Some).collect() };
        'path: for target in targets {
            let mut stop = false;
            for o in systems.observers.iter_mut() {
                if o.observes.as_ref().is_some_and(|ob| ob.0 == p.trigger) {
                    let bits = target.map_or(u64::MAX, Entity::to_bits);
                    stop |= run_one(world, rt, o, Some((&p.bytes, bits)));
                }
            }
            if stop {
                break 'path;
            }
        }
    }
    warn!(target: "bevy_ts", "observer queue still busy after 10,000 events; the rest wait for the next stage");
}

/// Runs one system (or one observer event). Returns whether an observer stopped propagation.
fn run_one(world: &mut World, rt: &mut ScriptRuntime, sys: &mut TsSystem, event: Option<(&[u8], u64)>) -> bool {
    let identity = sys.decl.name.clone();
    if let Some(c) = &sys.condition
        && !evaluate(world, c, &identity)
    {
        return false;
    }
    let dev = rt.dev;
    let tick = world.increment_change_tick();
    let last_run = world.resource::<SystemMemory>().0.get(&identity).map_or(Tick::new(0), |m| m.last_run);
    let frame = runtime::frame_info(world, tick.get());
    let started = Instant::now();

    let mut scratch = std::mem::take(&mut rt.scratch);
    scratch.time.clear();
    let mut frames: Vec<ParamFrame> = Vec::with_capacity(sys.params.len());
    // Written columns per (table) for summary ticks; read columns per frame for the dev guard.
    let mut written: Vec<(TableId, ComponentId)> = Vec::new();
    let mut read_cols: Vec<(usize, usize, usize, ComponentId)> = Vec::new();
    let mut written_ids: HashSet<ComponentId> = HashSet::new();
    let mut reader_cursors: Vec<(String, u64)> = Vec::new();
    let (mut ents_used, mut rows_used) = (0usize, 0usize);
    let mut failure: Option<ScriptError> = None;
    let mut memory = world.remove_resource::<SystemMemory>().unwrap_or_default();
    let mem = memory.0.entry(identity.clone()).or_default();
    {
        for p in &sys.params {
            if let RunParam::Query(q) = p {
                written_ids.extend(q.columns.iter().filter(|c| c.write).map(|c| c.id));
            }
            if let RunParam::Res { id, write: true, .. } = p {
                written_ids.insert(*id);
            }
        }
        for p in sys.params.iter_mut() {
            let frame = match p {
                RunParam::Query(q) => {
                    q.state.update_archetypes(world);
                    let tables = &world.storages().tables;
                    let mut out = Vec::new();
                    for table_id in q.state.matched_tables() {
                        let table = tables.get(table_id).expect("matched table exists");
                        let len = table.entity_count() as usize;
                        if len == 0 {
                            continue;
                        }
                        // Changed/Added: the rows whose tick is newer than this system's last run.
                        let rows = if q.changed.is_empty() && q.added.is_empty() {
                            None
                        } else {
                            if scratch.rows.len() <= rows_used {
                                scratch.rows.push(Vec::new());
                            }
                            let list = &mut scratch.rows[rows_used];
                            list.clear();
                            let changed: Vec<_> = q.changed.iter().filter_map(|id| table.get_changed_ticks_slice_for(*id)).collect();
                            let added: Vec<_> = q.added.iter().filter_map(|id| table.get_added_ticks_slice_for(*id)).collect();
                            for row in 0..len {
                                // SAFETY: exclusive stage; ticks are plain u32 cells.
                                let ok = changed.iter().chain(added.iter()).all(|t| unsafe { *t[row].get() }.is_newer_than(last_run, tick));
                                if ok {
                                    list.push(row as u32);
                                }
                            }
                            if list.is_empty() {
                                continue;
                            }
                            rows_used += 1;
                            Some((list.as_ptr(), list.len()))
                        };
                        if scratch.entities.len() <= ents_used {
                            scratch.entities.push(Vec::new());
                        }
                        let ents = &mut scratch.entities[ents_used];
                        ents_used += 1;
                        ents.clear();
                        for e in table.entities() {
                            let bits = e.to_bits();
                            ents.push(bits as u32);
                            ents.push((bits >> 32) as u32);
                        }
                        let mut columns = Vec::with_capacity(q.columns.len());
                        for (ci, c) in q.columns.iter().enumerate() {
                            let Some(column) = table.get_column(c.id) else {
                                columns.push(ColumnFrame { data: std::ptr::null_mut(), bytes: 0, ticks: None });
                                continue;
                            };
                            // SAFETY: the column holds `len × size` bytes; only the start pointer is
                            // taken. Cells are `UnsafeCell`s and the stage is exclusive.
                            let data = unsafe { column.get_data_slice::<u8>(len) }.as_ptr() as *mut u8;
                            let ticks = c.write.then(|| table.get_changed_ticks_slice_for(c.id).expect("column has ticks").as_ptr() as *mut u32);
                            if c.write {
                                written.push((table_id, c.id));
                            } else if dev && !written_ids.contains(&c.id) {
                                read_cols.push((frames.len(), out.len(), ci, c.id));
                            }
                            columns.push(ColumnFrame { data, bytes: len * c.size, ticks });
                        }
                        out.push(TableFrame { len, entities: ents.as_ptr(), columns, rows });
                    }
                    ParamFrame::Query { tables: out }
                }
                RunParam::Res { name, id, size, write } => match world.resource_entities().get(*id).and_then(|e| world.entities().get_spawned(e).ok()) {
                    Some(loc) => {
                        let table = world.storages().tables.get(loc.table_id).expect("resource table");
                        let row = loc.table_row.index();
                        let col = table.get_column(*id).expect("resource column");
                        let len = table.entity_count() as usize;
                        // SAFETY: as for query columns; `row < len`.
                        let base = unsafe { col.get_data_slice::<u8>(len) }.as_ptr() as *mut u8;
                        let data = unsafe { base.add(row * *size) };
                        let ticks = write.then(|| unsafe { (table.get_changed_ticks_slice_for(*id).unwrap().as_ptr() as *mut u32).add(row) });
                        if *write {
                            written.push((loc.table_id, *id));
                        }
                        ParamFrame::Row { data, bytes: *size, ticks }
                    }
                    None => {
                        failure = Some(ScriptError::new(format!("resource {name} does not exist; insert it with app.insertResource or guard with runIf(resourceExists({name}))")));
                        ParamFrame::None
                    }
                },
                RunParam::Time => {
                    // `{ delta: f32, elapsed: f64, frame: u32 }`: three u64 words. The frame is
                    // pointed at once `scratch.time` stops growing (below).
                    scratch.time.push((frame.dt as f32).to_bits() as u64);
                    scratch.time.push(frame.elapsed.to_bits());
                    scratch.time.push(frame.frame as u32 as u64);
                    ParamFrame::None
                }
                RunParam::Local(i) => {
                    let store = &mut mem.locals[*i];
                    let bytes = store.layout.size;
                    ParamFrame::Row { data: store.data_mut().as_mut_ptr(), bytes, ticks: None }
                }
                RunParam::MessageReader { index } => {
                    let payloads = world.resource::<TsPayloads>();
                    let q = &payloads.messages[*index];
                    let cursor = mem.cursors.get(&q.name).copied().unwrap_or(0);
                    let [a, b] = q.unread(cursor);
                    reader_cursors.push((q.name.clone(), q.next_seq));
                    ParamFrame::Messages { parts: [(a.0.as_ptr(), a.1), (b.0.as_ptr(), b.1)], stride: q.layout.size.max(1) }
                }
                RunParam::State { index } => {
                    ParamFrame::State { value: world.resource::<TsPayloads>().states[*index].current.unwrap_or(u32::MAX) }
                }
                RunParam::Event => match event {
                    Some((bytes, entity)) => ParamFrame::Event { data: bytes.as_ptr(), bytes: bytes.len(), entity },
                    None => ParamFrame::None,
                },
                RunParam::None => ParamFrame::None,
            };
            frames.push(frame);
        }
        // Time views point into `scratch.time`, which no longer grows.
        const _: () = assert!(cfg!(target_endian = "little"));
        let mut t = 0;
        for (p, f) in sys.params.iter().zip(frames.iter_mut()) {
            if matches!(p, RunParam::Time) {
                // SAFETY: three words per Time param were pushed above.
                let data = unsafe { scratch.time.as_mut_ptr().add(t * 3) } as *mut u8;
                *f = ParamFrame::Row { data, bytes: time_layout().size, ticks: None };
                t += 1;
            }
        }
    }

    let hashes: Vec<u64> = read_cols
        .iter()
        .map(|(f, t, c, _)| match &frames[*f] {
            ParamFrame::Query { tables } => {
                let cf = &tables[*t].columns[*c];
                // SAFETY: readable for the run.
                hash_bytes(unsafe { std::slice::from_raw_parts(cf.data, cf.bytes) })
            }
            _ => 0,
        })
        .collect();

    let result = match failure {
        Some(e) => Err(e),
        None => {
            rt.fill_pool(world);
            scratch.arena.resize(crate::commands::ARENA_BYTES, 0);
            let pool = std::mem::take(&mut rt.pool);
            let r = {
                let mut run = SystemRun { ctx: CallContext { frame, arena: &mut scratch.arena, pool: &pool }, params: &frames };
                rt.backend.run_system(sys.key, &mut run)
            };
            rt.pool = pool;
            r
        }
    };

    // Post-run: summary ticks, whole-table marks, the dev guard.
    {
        let tables = &world.storages().tables;
        for (table_id, id) in &written {
            if let Some(s) = tables.get(*table_id).and_then(|t| t.get_summary_tick(*id)) {
                s.set(tick);
            }
        }
        if let Ok(out) = &result {
            for &(p, t, c) in &out.write_marks {
                if let Some(ParamFrame::Query { tables }) = frames.get(p as usize)
                    && let Some(tf) = tables.get(t as usize)
                    && let Some(ticks) = tf.columns.get(c as usize).and_then(|c| c.ticks)
                {
                    // SAFETY: the tick column has `len` entries and the stage is exclusive.
                    unsafe { std::slice::from_raw_parts_mut(ticks, tf.len) }.fill(tick.get());
                }
            }
        }
        if dev && !sys.warned_read_write {
            for ((f, t, c, id), before) in read_cols.iter().zip(&hashes) {
                if let ParamFrame::Query { tables } = &frames[*f] {
                    let cf = &tables[*t].columns[*c];
                    // SAFETY: unchanged structurally since the run.
                    if hash_bytes(unsafe { std::slice::from_raw_parts(cf.data, cf.bytes) }) != *before {
                        sys.warned_read_write = true;
                        let name = sys
                            .params
                            .iter()
                            .find_map(|p| match p {
                                RunParam::Query(q) => q.columns.iter().find(|a| a.id == *id).map(|a| a.name.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        world.insert_resource(memory);
                        rt.log_warn(
                            world,
                            format!("system {} wrote to {name}, which no query of it declares Mut; declare it Mut", sys.decl.name),
                            sys.decl.location.clone(),
                        );
                        memory = world.remove_resource::<SystemMemory>().unwrap();
                        break;
                    }
                }
            }
        }
    }
    let mem = memory.0.entry(identity.clone()).or_default();
    mem.last_run = tick;
    let mut stop = false;
    if result.is_ok() {
        for (name, next) in reader_cursors {
            mem.cursors.insert(name, next);
        }
    }
    world.insert_resource(memory);
    sys.last_run_us = started.elapsed().as_micros() as u64;

    match result {
        Ok(out) => {
            sys.last_error = None;
            stop = out.stop_propagation;
            let label = format!("system {}", sys.decl.name);
            rt.apply_output(world, &scratch.arena, out, &label);
        }
        Err(e) => {
            rt.log_error(world, &format!("system {} threw", sys.decl.name), &e);
            sys.last_error = Some(e);
        }
    }
    rt.scratch = scratch;
    stop
}

/// A row of `bevy-ts/systems`.
#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    pub id: String,
    pub name: String,
    pub schedule: ScheduleDecl,
    /// The stage, for systems in one (`preUpdate`, ...); `startup`, `onEnter`, `onExit` or
    /// `observer` otherwise.
    pub stage: String,
    pub params: Vec<ParamDecl>,
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub generation: u64,
    #[serde(rename = "lastRunMicros")]
    pub last_run_us: u64,
    #[serde(rename = "lastError", skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ScriptError>,
}

impl TsSystem {
    pub fn info(&self) -> SystemInfo {
        let (read, write) = self.decl.access();
        let stage = match &self.decl.schedule {
            ScheduleDecl::Stage { stage } => stage.as_str().to_string(),
            ScheduleDecl::Startup => "startup".into(),
            ScheduleDecl::OnEnter { .. } => "onEnter".into(),
            ScheduleDecl::OnExit { .. } => "onExit".into(),
            ScheduleDecl::Observer => "observer".into(),
        };
        SystemInfo {
            id: self.decl.name.clone(),
            name: self.decl.name.rsplit('#').next().unwrap_or(&self.decl.name).to_string(),
            schedule: self.decl.schedule.clone(),
            stage,
            params: self.decl.params.clone(),
            read,
            write,
            generation: self.key.generation,
            last_run_us: self.last_run_us,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(name: &str, after: &[&str], before: &[&str], sets: &[&str]) -> SystemDecl {
        SystemDecl {
            name: name.into(),
            schedule: ScheduleDecl::Stage { stage: Stage::Update },
            params: vec![],
            after: after.iter().map(|s| s.to_string()).collect(),
            before: before.iter().map(|s| s.to_string()).collect(),
            sets: sets.iter().map(|s| s.to_string()).collect(),
            condition: None,
            location: None,
        }
    }

    #[test]
    fn registration_order_adjusted_by_after_before_and_sets() {
        let ds = [sys("a", &["c"], &[], &[]), sys("b", &[], &[], &[]), sys("c", &[], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert_eq!(order(&refs).unwrap(), vec![1, 2, 0]);
        let ds = [sys("x", &["enemies"], &[], &[]), sys("e1", &[], &[], &["enemies"]), sys("e2", &[], &["e1"], &["enemies"])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert_eq!(order(&refs).unwrap(), vec![2, 1, 0]);
    }

    #[test]
    fn cycles_and_unknown_targets_fail() {
        let ds = [sys("a", &["b"], &[], &[]), sys("b", &["a"], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs).unwrap_err().message.contains("cycle"));
        let ds = [sys("a", &["nope"], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs).unwrap_err().message.contains("nope"));
    }
}
