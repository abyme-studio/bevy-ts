//! TS systems as Bevy systems (lanes stage 1). Each TS system in a schedule or a slot is a
//! [`TsBevySystem`] of its own, in its stage's set, declaring exactly its access: one
//! `QueryState<FilteredEntityMut>` per TS query, `Res`/`ResMut` access per resource, its exported
//! params' access, and the runtime as non-send data (the seam stage 2 replaces with a lane). Bevy
//! orders it, detects its conflicts and ambiguities, and runs Rust systems beside it that touch
//! none of its data. Its commands, messages, state changes and triggers apply at the sync point
//! Bevy inserts after it, and the chain between a stage's TS systems keeps today's run order.
//!
//! Three kinds of param are declared wider than they read, as known limits: with the `reflect`
//! feature every TS system reads every resource that exists when it is initialized (an `assets.*`
//! host call may read any `Assets<T>`); a system with a reflect-tier query term or resource, or a
//! Rust `MessageReader`, writes the whole world (reflection and the message reader take
//! `&World`); a run condition holding a TS predicate or a Rust `onMessage` is evaluated by an
//! exclusive gate system just before what it gates, since Bevy may evaluate a condition system
//! off the main thread.
//!
//! A commit diffs the new generation's systems against the live ones by identity: a system whose
//! fingerprint, place and predecessor are unchanged keeps its Bevy system (only its key changes);
//! the rest are removed and added, one edit per touched schedule.

use crate::conditions::{Condition, TsCondition};
use crate::manifest::{ParamDecl, Stage, SystemDecl};
use crate::memory::SystemMemory;
use crate::messages::TsPayloads;
use crate::runtime::{Pending, ScriptFrame, ScriptRuntime};
use crate::sets::BevyTsSlot;
use crate::systems::{Clock, RunBytes, RunParam, SlotKey, StageTimes, TsSystem, TsSystemCell, TsSystems};
use bevy::ecs::change_detection::{CheckChangeTicks, Tick};
use bevy::ecs::component::ComponentId;
use bevy::ecs::query::FilteredAccess;
use bevy::ecs::resource::IS_RESOURCE;
use bevy::ecs::schedule::{InternedScheduleLabel, InternedSystemSet, ScheduleCleanupPolicy, SystemSet};
use bevy::ecs::system::{RunSystemError, SystemAccess, SystemIn, SystemStateFlags};
use bevy::ecs::world::DeferredWorld;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use bevy::utils::prelude::DebugName;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The set holding one TS system, by identity: the chain orders each system after its
/// predecessor's set, so an edge survives the predecessor's rebuild.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsSystemSet(pub String);

/// A group's set, for its collective `runIf`: one per group and condition, so a changed condition
/// is a new set.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsGroupSet(pub String);

/// The start of a stage: the exclusive system that runs observers Rust triggered since the last
/// stage, before the stage's first TS system.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsStageHead(pub InternedSystemSet);

/// Whether this frame runs the bevy-ts stages (pause and stepping): the run condition of every
/// stage and slot set.
pub fn stages_run(clock: Option<Res<crate::runtime::ScriptClock>>) -> bool {
    clock.is_none_or(|c| c.running_this_frame)
}

fn observers_queued(queue: Option<Res<crate::observers::ObserverQueue>>) -> bool {
    queue.is_some_and(|q| !q.0.is_empty())
}

/// Runs the observers Rust triggered since the last stage. The head of every stage.
fn stage_head(world: &mut World) {
    crate::systems::with_systems(world, |world, rt, systems| crate::systems::drain_observers(world, rt, systems));
    if crate::schedules::has_pending(world) {
        crate::schedules::run_pending(world);
    }
}

/// Configures a stage's set in `schedules`: skipped while paused, its head first.
pub(crate) fn configure_stage(schedules: &mut Schedules, label: InternedScheduleLabel, set: InternedSystemSet) {
    schedules.configure_sets(label, set.run_if(stages_run));
    schedules.add_systems(label, stage_head.in_set(set).in_set(TsStageHead(set)).run_if(observers_queued));
}

/// Configures a slot's set: skipped while paused.
pub(crate) fn configure_slot(schedules: &mut Schedules, label: InternedScheduleLabel, slot: BevyTsSlot) {
    schedules.configure_sets(label, slot.run_if(stages_run));
}

/// A TS system as a Bevy system.
pub struct TsBevySystem {
    name: DebugName,
    decl: SystemDecl,
    /// Its key in `SystemMemory`.
    memory: String,
    params: Vec<RunParam>,
    cell: Arc<TsSystemCell>,
    ports: Vec<usize>,
    /// It takes `Commands`, so the entity pool is filled for it.
    spawns: bool,
    /// Its stage, timed for the stage diagnostics; `None` in a custom schedule or a slot.
    stage: Option<(Stage, Arc<StageTimes>)>,
    last_run: Tick,
    /// The last run's output, applied at the sync point after it; `Err` drops its params' state;
    /// `Ok(None)`, a skipped run, applies nothing.
    output: Option<Result<Option<RunBytes>, ()>>,
}

impl TsBevySystem {
    fn new(sys: &mut TsSystem, stage: Option<(Stage, Arc<StageTimes>)>) -> Self {
        let params = std::mem::take(&mut sys.params);
        let ports: Vec<usize> = params.iter().filter_map(|p| if let RunParam::Port(i) = p { Some(*i) } else { None }).collect();
        let spawns = sys.decl.params_flat().any(|p| matches!(p, ParamDecl::Commands));
        TsBevySystem {
            name: DebugName::owned(sys.decl.name.clone()),
            decl: sys.decl.clone(),
            memory: sys.memory.clone(),
            params,
            cell: sys.cell.clone(),
            ports,
            spawns,
            stage,
            last_run: Tick::new(0),
            output: None,
        }
    }
}

/// Whether a run made or changed assets that wait to go in.
fn assets_queued(world: &World) -> bool {
    #[cfg(feature = "reflect")]
    return crate::reflect_asset::queued(world);
    #[cfg(not(feature = "reflect"))]
    {
        let _ = world;
        false
    }
}

/// Reads or writes of the whole world: a param bevy-ts reaches through `&World` or reflection.
fn whole_world(write: bool) -> FilteredAccess {
    let mut all = FilteredAccess::default();
    if write { all.write_all() } else { all.read_all() }
    all
}


/// The `last_run` of a system that never ran: as old as change detection reaches.
pub(crate) fn never_ran(now: Tick) -> Tick {
    Tick::new(now.get().wrapping_sub(Tick::MAX.get()))
}

fn resource(id: ComponentId, write: bool) -> FilteredAccess {
    let mut f = FilteredAccess::default();
    if write { f.add_write(id) } else { f.add_read(id) }
    f.and_with(IS_RESOURCE);
    f
}

/// Every access a TS system declares but its queries: what every TS system takes
/// (the runtime, its memory, the payloads, the frame and `Time`), then its params in order.
fn declared(world: &mut World, params: &[RunParam]) -> Vec<FilteredAccess> {
    let mut out = Vec::new();
    let mut runtime = FilteredAccess::default();
    runtime.add_write(world.components_registrator().register_non_send::<ScriptRuntime>());
    out.push(runtime);
    out.push(resource(world.register_component::<SystemMemory>(), true));
    out.push(resource(world.register_component::<TsPayloads>(), false));
    out.push(resource(world.register_component::<ScriptFrame>(), false));
    out.push(resource(world.register_component::<Time>(), false));
    out.push(resource(world.register_component::<crate::places::RunPlace>(), true));
    // Written by a system taking an exported param, read by others to name a refused call.
    let ports = params.iter().any(|p| matches!(p, RunParam::Port(_)));
    out.push(resource(world.register_component::<crate::host::HostParams>(), ports));
    // An `assets.*` host call reads the registry, the asset server and any `Assets<T>`.
    #[cfg(feature = "reflect")]
    {
        let ids: Vec<ComponentId> = world.resource_entities().iter().map(|(id, _)| id).collect();
        out.extend(ids.into_iter().map(|id| resource(id, false)));
    }
    for p in params {
        match p {
            RunParam::Res { id, write, .. } => out.push(resource(*id, *write)),
            RunParam::ReflectRes { .. } | RunParam::RustMessageReader { .. } => out.push(whole_world(true)),
            RunParam::Query(q) if q.columns.iter().any(|c| c.reflect.is_some()) => out.push(whole_world(true)),
            RunParam::State { cell, .. } => {
                out.push(resource(cell.state, false));
                out.push(resource(cell.previous, false));
            }
            RunParam::Time(clock) => {
                let id = match clock {
                    Clock::Generic => continue,
                    Clock::Real => world.register_component::<Time<bevy::time::Real>>(),
                    Clock::Virtual => world.register_component::<Time<bevy::time::Virtual>>(),
                    Clock::Fixed => world.register_component::<Time<bevy::time::Fixed>>(),
                };
                out.push(resource(id, false));
            }
            RunParam::Port(i) => {
                let Some(access) = world.resource_scope(|world, host: Mut<crate::host::HostParams>| host.access(*i, world)) else { continue };
                match access {
                    SystemAccess::Shared(set) => out.extend(set.filtered_accesses().iter().cloned()),
                    SystemAccess::Exclusive => {
                        let mut all = FilteredAccess::default();
                        all.write_all();
                        out.push(all);
                    }
                    SystemAccess::None => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// The access a TS system declares, or the two params whose access conflicts, as a Rust system
/// with the same params fails.
impl System for TsBevySystem {
    type In = ();
    type Out = ();

    fn name(&self) -> DebugName {
        self.name.clone()
    }

    fn flags(&self) -> SystemStateFlags {
        // Always deferred: any TS system may leave output (a signal set, a binding written) without
        // a deferring param, and the next TS system must see it.
        SystemStateFlags::NON_SEND | SystemStateFlags::DEFERRED
    }

    unsafe fn run_unsafe(&mut self, _input: SystemIn<'_, Self>, world: UnsafeWorldCell) -> Result<(), RunSystemError> {
        let started = self.stage.as_ref().and_then(|(_, t)| t.start());
        let this_run = world.increment_change_tick();
        let ticks = crate::host::RunTicks { last_run: self.last_run, this_run };
        // SAFETY: declared as non-send write access; the system is non-send, so on the main thread.
        let Some(mut rt) = (unsafe { world.get_non_send_mut::<ScriptRuntime>() }) else { return Ok(()) };
        // SAFETY: the caller grants the access `initialize` returned, which is everything
        // `run_body` touches: `declared`.
        let ran = unsafe { crate::systems::run_body(world, &mut rt, &mut self.params, &self.decl, &self.memory, &self.cell, ticks, None, self.spawns) };
        // A skipped run keeps its last run, as a false run condition does.
        if !matches!(ran.result, Ok(None)) {
            self.last_run = this_run;
        }
        if let Some(w) = ran.warn {
            rt.log.push(Pending::Warn(w, self.cell.location.lock().unwrap().clone()));
        }
        match ran.result {
            Ok(out) => self.output = Some(Ok(out)),
            Err(e) => {
                rt.log.push(Pending::Error(format!("system {} threw", self.decl.name), e));
                self.output = Some(Err(()));
            }
        }
        if let Some((stage, times)) = &self.stage {
            times.add(*stage, started);
        }
        Ok(())
    }

    fn apply_deferred(&mut self, world: &mut World) {
        let Some(out) = self.output.take() else { return };
        // A run that left nothing (no commands, no ports to settle, no asset made, no observer or
        // run queued) applies nothing: the runtime stays in the world.
        let left_nothing = self.ports.is_empty() && out.as_ref().is_ok_and(|o| o.as_ref().is_none_or(|o| o.bytes.is_empty() && o.pool_used == 0));
        if left_nothing
            && !assets_queued(world)
            && world.get_resource::<crate::observers::ObserverQueue>().is_none_or(|q| q.0.is_empty())
            && !crate::schedules::has_pending(world)
        {
            return;
        }
        let started = self.stage.as_ref().and_then(|(_, t)| t.start());
        let label = format!("system {}", self.decl.name);
        let ports = &self.ports;
        crate::systems::with_systems_keeping_pool(world, |world, rt, systems| {
            match out {
                Ok(Some(o)) => {
                    rt.apply_output(world, o.bytes, o.pool_used, &label);
                    crate::systems::settle_ports(world, ports, true);
                }
                Ok(None) => {}
                Err(()) => {
                    crate::systems::settle_ports(world, ports, false);
                    // Assets a throwing run made still go in: their refs were handed out.
                    #[cfg(feature = "reflect")]
                    crate::reflect_asset::flush(world);
                }
            }
            crate::systems::drain_observers(world, rt, systems);
        });
        if crate::schedules::has_pending(world) {
            crate::schedules::claim(world, &label);
            crate::schedules::run_pending(world);
        }
        if let Some((stage, times)) = &self.stage {
            times.add(*stage, started);
        }
    }

    fn queue_deferred(&mut self, _world: DeferredWorld) {}

    fn initialize(&mut self, world: &mut World) -> SystemAccess {
        let now = world.change_tick();
        self.last_run = world
            .get_resource::<SystemMemory>()
            .and_then(|m| m.0.get(&self.memory))
            .map_or_else(|| never_ran(now), |m| m.last_run);
        world.init_resource::<crate::places::RunPlace>();
        let mut access = SystemAccess::default();
        access.ensure_metadata_access();
        let name = self.decl.name.clone();
        for d in declared(world, &self.params) {
            access.ensure_filtered_access(d);
        }
        // Queries declare through Bevy's own code, as a `Query` param does. Two params of one TS
        // system may overlap: its body runs serially and the bridge aliases one column.
        if let SystemAccess::Shared(set) = &mut access {
            for p in &self.params {
                if let RunParam::Query(q) = p {
                    let mut own = bevy::ecs::query::FilteredAccessSet::new();
                    q.state.init_access(Some(&name), &mut own, world.as_unsafe_world_cell());
                    set.extend(own);
                }
            }
        }
        access
    }

    fn check_change_tick(&mut self, check: CheckChangeTicks) {
        self.last_run.check_tick(check);
    }

    fn get_last_run(&self) -> Tick {
        self.last_run
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.last_run = last_run;
    }
}

/// Where a TS system runs: its schedule, its stage or slot set, and what it runs after.
#[derive(Clone, Debug, PartialEq)]
struct Place {
    label: InternedScheduleLabel,
    set: InternedSystemSet,
    after: After,
}

#[derive(Clone, Debug, PartialEq)]
enum After {
    /// The stage's head.
    Head(InternedSystemSet),
    /// The previous TS system of its stage or slot.
    System(String),
    /// Nothing: the first system of a slot.
    Nothing,
}

/// A live Bevy system of a TS system.
struct Live {
    fingerprint: String,
    place: Place,
    cell: Arc<TsSystemCell>,
}

/// The live Bevy systems, by schedule and identity, and the group sets configured.
#[derive(Resource, Default)]
pub struct LiveSystems {
    systems: HashMap<(InternedScheduleLabel, String), Live>,
    groups: HashSet<(InternedScheduleLabel, String)>,
    /// Restarts so far: a group set's name carries it, so a restart's groups get fresh state.
    restarts: u64,
    /// Schedule edits so far, for tests: one per touched schedule per commit.
    pub edits: u64,
}

/// A TS system to place.
struct Wanted<'a> {
    sys: &'a mut TsSystem,
    place: Place,
    /// Its stage, for the stage diagnostics.
    stage: Option<Stage>,
    /// Its groups' sets.
    groups: Vec<String>,
}

/// Places a just-committed generation's scheduled systems: keeps unchanged Bevy systems, removes
/// and adds the rest, and configures new group sets; one edit per touched schedule, at this frame
/// boundary.
pub(crate) fn commit(world: &mut World) {
    let Some(mut systems) = world.remove_resource::<TsSystems>() else { return };
    let mut groups = std::mem::take(&mut systems.groups);
    let mut group_conditions: HashMap<String, (InternedScheduleLabel, crate::conditions::Condition)> = HashMap::new();
    let mut group_sets: HashMap<String, String> = HashMap::new();
    let mut wanted: Vec<Wanted> = Vec::new();
    let epoch = world.get_resource::<LiveSystems>().map_or(0, |l| l.restarts);
    let clocks = world.get_resource_or_init::<crate::systems::StageClocks>().clone();
    {
        let mut lists: Vec<(InternedScheduleLabel, InternedSystemSet, bool, Option<Stage>, &mut Vec<TsSystem>)> = Vec::new();
        for (stage, list) in systems.stages.iter_mut() {
            lists.push((crate::schedules::stage_label(*stage), crate::schedules::stage_set(*stage).intern(), true, Some(*stage), list));
        }
        for (name, list) in systems.schedules.iter_mut() {
            let key = crate::schedules::TsSchedule::new(name).0;
            lists.push((crate::schedules::label_of(world, name), crate::BevyTsSet::Schedule(key).intern(), true, None, list));
        }
        let sets = world.get_resource::<crate::sets::TsSets>().cloned().unwrap_or_default();
        for (SlotKey { set, after }, list) in systems.slots.iter_mut() {
            let Some(exposed) = sets.get(set) else { continue };
            let slot = if *after { BevyTsSlot::after(set) } else { BevyTsSlot::before(set) };
            lists.push((exposed.schedule, slot.intern(), false, None, list));
        }
        for (label, set, head, stage, list) in lists {
            let mut after = if head { After::Head(set) } else { After::Nothing };
            for sys in list.iter_mut() {
                let mut sets = Vec::new();
                for g in &sys.decl.groups {
                    let name = match group_sets.get(g) {
                        Some(n) => n.clone(),
                        None => {
                            let mut fp = String::new();
                            if let Some(c) = groups.remove(g) {
                                c.fingerprint(&mut fp);
                                group_conditions.insert(g.clone(), (label, c));
                            }
                            let n = format!("{g}#{fp}@{epoch}");
                            group_sets.insert(g.clone(), n.clone());
                            n
                        }
                    };
                    sets.push(name);
                }
                let place = Place { label, set, after: after.clone() };
                after = After::System(sys.decl.name.clone());
                wanted.push(Wanted { sys, place, stage, groups: sets });
            }
        }
    }

    let mut live = world.remove_resource::<LiveSystems>().unwrap_or_default();
    let mut edits: HashMap<InternedScheduleLabel, Edit> = HashMap::new();
    let mut keep: HashSet<(InternedScheduleLabel, String)> = HashSet::new();
    for w in wanted {
        let id = (w.place.label, w.sys.decl.name.clone());
        let fingerprint = format!("{}|{:?}", w.sys.fingerprint, w.groups);
        keep.insert(id.clone());
        if let Some(l) = live.systems.get(&id)
            && l.fingerprint == fingerprint
            && l.place == w.place
        {
            *l.cell.key.lock().unwrap() = w.sys.key();
            *l.cell.location.lock().unwrap() = w.sys.decl.location.clone();
            w.sys.cell = l.cell.clone();
            w.sys.params.clear();
            w.sys.condition = None;
            continue;
        }
        let edit = edits.entry(w.place.label).or_default();
        if live.systems.contains_key(&id) {
            edit.remove.push(id.1.clone());
        }
        let condition = w.sys.condition.take().map(|c| (format!("{} condition", w.sys.decl.name), c));
        let stage = w.stage.map(|s| (s, clocks.0.clone()));
        let system = TsBevySystem::new(w.sys, stage);
        edit.add.push(Add { system, condition, place: w.place.clone(), groups: w.groups.clone(), identity: id.1.clone() });
        live.systems.insert(id, Live { fingerprint, place: w.place, cell: w.sys.cell.clone() });
    }
    let gone: Vec<(InternedScheduleLabel, String)> = live.systems.keys().filter(|k| !keep.contains(*k)).cloned().collect();
    for id in gone {
        live.systems.remove(&id);
        edits.entry(id.0).or_default().remove.push(id.1);
    }
    let mut wanted_groups: HashSet<(InternedScheduleLabel, String)> = HashSet::new();
    for (g, (label, c)) in group_conditions {
        let name = group_sets[&g].clone();
        wanted_groups.insert((label, name.clone()));
        if live.groups.insert((label, name.clone())) {
            edits.entry(label).or_default().groups.push((name, c));
        }
    }
    // A group gone, or whose condition changed (a new set name): its gate goes too.
    let gone: Vec<(InternedScheduleLabel, String)> = live.groups.iter().filter(|k| !wanted_groups.contains(*k)).cloned().collect();
    for id in gone {
        live.groups.remove(&id);
        edits.entry(id.0).or_default().gates.push(id.1);
    }
    live.edits += edits.len() as u64;
    world.insert_resource(live);
    world.insert_resource(systems);
    for (label, edit) in edits {
        let custom = world.get_resource::<crate::schedules::MadeStages>().is_some_and(|m| m.created.contains(&label));
        crate::schedules::edit(world, label, custom, move |world| edit.apply(world, label));
    }
}

/// Removes every live TS Bevy system: a restart, whose new generation adds its own.
pub(crate) fn retire_all(world: &mut World) {
    let Some(mut live) = world.remove_resource::<LiveSystems>() else { return };
    let mut edits: HashMap<InternedScheduleLabel, Edit> = HashMap::new();
    for ((label, identity), _) in live.systems.drain() {
        edits.entry(label).or_default().remove.push(identity);
    }
    for (label, name) in live.groups.drain() {
        edits.entry(label).or_default().gates.push(name);
    }
    live.restarts += 1;
    live.edits += edits.len() as u64;
    world.insert_resource(live);
    for (label, edit) in edits {
        let custom = world.get_resource::<crate::schedules::MadeStages>().is_some_and(|m| m.created.contains(&label));
        crate::schedules::edit(world, label, custom, move |world| edit.apply(world, label));
    }
}

struct Add {
    system: TsBevySystem,
    condition: Option<(String, Condition)>,
    place: Place,
    groups: Vec<String>,
    identity: String,
}

/// The exclusive system that evaluates a condition needing the whole world (a TS predicate, a
/// Rust `onMessage`), once each time it runs, for the run condition of what it gates to read.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsGate(pub String);

/// Evaluates `condition` with the world held, its predicates run in JS, into `answer`.
fn gate(mut condition: Condition, answer: Arc<AtomicBool>) -> impl FnMut(&mut World) + Send + Sync + 'static {
    move |world: &mut World| {
        let mut pass = false;
        crate::systems::with_systems_keeping_pool(world, |world, rt, systems| {
            let predicates = &mut systems.predicates;
            pass = crate::conditions::evaluate(world, &mut condition, &mut |world, memory| crate::systems::run_predicate(world, rt, predicates, memory));
        });
        answer.store(pass, Ordering::Relaxed);
    }
}

/// A gated config's run condition: a Bevy condition system declaring what it reads, or, for a
/// condition needing the world, its gate's answer, with the gate to add before it.
enum RunIf {
    Bevy(TsCondition),
    Gated(Arc<AtomicBool>, Condition),
}

impl RunIf {
    fn of(name: String, condition: Condition) -> Self {
        if condition.needs_world() { RunIf::Gated(Arc::new(AtomicBool::new(false)), condition) } else { RunIf::Bevy(TsCondition::new(name, condition)) }
    }
}

/// One schedule's edit: systems and group gates out, then group sets and systems in.
#[derive(Default)]
struct Edit {
    remove: Vec<String>,
    /// Group sets whose gate, if any, goes.
    gates: Vec<String>,
    add: Vec<Add>,
    groups: Vec<(String, Condition)>,
}

impl Edit {
    fn apply(self, world: &mut World, label: InternedScheduleLabel) {
        // A schedule scope, not a `Schedules` one: initializing the schedule may insert resources.
        world.schedule_scope(label, |world, schedule| {
            for identity in self.remove {
                // Keeps the set, so the chain edge of the system after it holds for its rebuild.
                let _ = schedule.remove_systems_in_set(TsSystemSet(identity), world, ScheduleCleanupPolicy::RemoveSystemsOnlyAllowBreakages);
            }
            for name in self.gates {
                let _ = schedule.remove_systems_in_set(TsGate(name), world, ScheduleCleanupPolicy::RemoveSystemsOnlyAllowBreakages);
            }
            for (name, condition) in self.groups {
                match RunIf::of(format!("runIf of group {name}"), condition) {
                    RunIf::Bevy(c) => drop(schedule.configure_sets(TsGroupSet(name).run_if(c))),
                    RunIf::Gated(answer, c) => {
                        // Before every member, wherever its stage or slot runs; skipped as the stages are.
                        schedule.add_systems(gate(c, answer.clone()).in_set(TsGate(name.clone())).before(TsGroupSet(name.clone())).run_if(stages_run));
                        schedule.configure_sets(TsGroupSet(name).run_if(move || answer.load(Ordering::Relaxed)));
                    }
                }
            }
            for a in self.add {
                let identity = a.identity;
                let mut config = a.system.in_set(a.place.set).in_set(TsSystemSet(identity.clone()));
                for g in a.groups {
                    config = config.in_set(TsGroupSet(g));
                }
                let after = |config: bevy::ecs::schedule::ScheduleConfigs<_>| match &a.place.after {
                    After::Head(set) => config.after(TsStageHead(*set)),
                    After::System(prev) => config.after(TsSystemSet(prev.clone())),
                    After::Nothing => config,
                };
                match a.condition.map(|(name, c)| RunIf::of(name, c)) {
                    Some(RunIf::Bevy(c)) => drop(schedule.add_systems(after(config.run_if(c)))),
                    Some(RunIf::Gated(answer, c)) => {
                        // In the system's own set, so it goes with the system, and just before it.
                        let g = gate(c, answer.clone()).in_set(a.place.set).in_set(TsSystemSet(identity.clone())).in_set(TsGate(identity.clone()));
                        schedule.add_systems(after(g.into_configs()));
                        schedule.add_systems(after(config.after(TsGate(identity)).run_if(move || answer.load(Ordering::Relaxed))));
                    }
                    None => drop(schedule.add_systems(after(config))),
                }
            }
        });
    }
}
