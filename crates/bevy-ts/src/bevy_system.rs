//! TS systems as Bevy systems (lanes stage 1). Each TS system in a schedule or a slot is a
//! [`TsBevySystem`] of its own, in its stage's set, declaring exactly its access: one
//! `QueryState<FilteredEntityMut>` per TS query, `Res`/`ResMut` access per resource, its exported
//! params' access, and the runtime as non-send data (the seam stage 2 replaces with a lane). Bevy
//! orders it, detects its conflicts and ambiguities, and runs Rust systems beside it that touch
//! none of its data. Its commands, messages, state changes and triggers apply at the sync point
//! Bevy inserts after it, and the chain between a stage's TS systems keeps today's run order.
//!
//! A commit diffs the new generation's systems against the live ones by identity: a system whose
//! fingerprint, place and predecessor are unchanged keeps its Bevy system (only its key changes);
//! the rest are removed and added, one edit per touched schedule.

use crate::backend::ScriptError;
use crate::conditions::TsCondition;
use crate::manifest::{ParamDecl, SystemDecl};
use crate::memory::SystemMemory;
use crate::messages::TsPayloads;
use crate::runtime::{Pending, ScriptFrame, ScriptRuntime};
use crate::sets::BevyTsSlot;
use crate::systems::{Clock, RunBytes, RunParam, SlotKey, TsSystem, TsSystemCell, TsSystems};
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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

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
    params: Vec<RunParam>,
    cell: Arc<TsSystemCell>,
    ports: Vec<usize>,
    /// It takes `Commands`, so the entity pool is filled for it.
    spawns: bool,
    deferred: bool,
    last_run: Tick,
    /// The last run's output, applied at the sync point after it; `Err` drops its params' state.
    output: Option<Result<RunBytes, ()>>,
}

impl TsBevySystem {
    fn new(world: &World, sys: &mut TsSystem) -> Self {
        let params = std::mem::take(&mut sys.params);
        let ports: Vec<usize> = params.iter().filter_map(|p| if let RunParam::Port(i) = p { Some(*i) } else { None }).collect();
        let host = world.get_resource::<crate::host::HostParams>();
        let spawns = sys.decl.params_flat().any(|p| matches!(p, ParamDecl::Commands));
        let deferred = sys.decl.params_flat().any(|p| matches!(p, ParamDecl::Commands | ParamDecl::MessageWriter { .. } | ParamDecl::NextState { .. }))
            || ports.iter().any(|i| host.is_some_and(|h| h.deferred(*i)));
        TsBevySystem {
            name: DebugName::owned(sys.decl.name.clone()),
            decl: sys.decl.clone(),
            params,
            cell: sys.cell.clone(),
            ports,
            spawns,
            deferred,
            last_run: Tick::new(0),
            output: None,
        }
    }
}

/// One declared access, named for a conflict error.
struct Declared {
    what: String,
    access: FilteredAccess,
    /// A TS query's, which `initialize` declares through `QueryState::init_access`.
    query: bool,
}

fn resource(id: ComponentId, write: bool) -> FilteredAccess {
    let mut f = FilteredAccess::default();
    if write { f.add_write(id) } else { f.add_read(id) }
    f.and_with(IS_RESOURCE);
    f
}

/// Every access a TS system's params declare, in param order, after what every TS system takes
/// (the runtime, its memory, the payloads, the frame and `Time`).
fn declared(world: &mut World, params: &[RunParam]) -> Vec<Declared> {
    let mut out = Vec::new();
    let mut runtime = FilteredAccess::default();
    runtime.add_write(world.components_registrator().register_non_send::<ScriptRuntime>());
    out.push(Declared { what: "the bevy-ts runtime".into(), access: runtime, query: false });
    out.push(Declared { what: "bevy-ts system memory".into(), access: resource(world.register_resource::<SystemMemory>(), true), query: false });
    out.push(Declared { what: "bevy-ts payloads".into(), access: resource(world.register_resource::<TsPayloads>(), false), query: false });
    out.push(Declared { what: "the bevy-ts frame".into(), access: resource(world.register_resource::<ScriptFrame>(), false), query: false });
    out.push(Declared { what: "Res<Time>".into(), access: resource(world.register_resource::<Time>(), false), query: false });
    if params.iter().any(|p| matches!(p, RunParam::Port(_))) {
        out.push(Declared { what: "exported params".into(), access: resource(world.register_resource::<crate::host::HostParams>(), true), query: false });
    }
    for p in params {
        match p {
            RunParam::Query(q) => {
                let cols: Vec<String> = q.columns.iter().map(|c| if c.write { format!("Mut<{}>", c.name) } else { c.name.clone() }).collect();
                out.push(Declared { what: format!("Query<[{}]>", cols.join(", ")), access: q.state.component_access().clone(), query: true });
            }
            RunParam::Res { name, id, write, .. } => {
                let what = if *write { format!("ResMut<{name}>") } else { format!("Res<{name}>") };
                out.push(Declared { what, access: resource(*id, *write) });
            }
            RunParam::Time(clock) => {
                let id = match clock {
                    Clock::Generic => continue,
                    Clock::Real => world.register_resource::<Time<bevy::time::Real>>(),
                    Clock::Virtual => world.register_resource::<Time<bevy::time::Virtual>>(),
                    Clock::Fixed => world.register_resource::<Time<bevy::time::Fixed>>(),
                };
                out.push(Declared { what: format!("Res<{}>", clock.resource()), access: resource(id, false), query: false });
            }
            RunParam::Port(i) => {
                let Some((ts, access)) = world.resource_scope(|world, host: Mut<crate::host::HostParams>| host.access(*i, world)) else { continue };
                if let SystemAccess::Shared(set) = access {
                    for f in set.filtered_accesses() {
                        out.push(Declared { what: ts.to_string(), access: f.clone(), query: false });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The access a TS system declares, or the two params whose access conflicts, as a Rust system
/// with the same params fails.
pub(crate) fn check_access(world: &mut World, decl: &SystemDecl, params: &[RunParam]) -> Result<(), ScriptError> {
    let all = declared(world, params);
    for (i, d) in all.iter().enumerate() {
        if let Some(prev) = all[..i].iter().find(|p| !p.access.is_compatible(&d.access)) {
            return Err(ScriptError::at(
                format!("system {}: {} conflicts with {}: one takes what the other writes; merge them into one param", decl.name, d.what, prev.what),
                decl.location.clone(),
            ));
        }
    }
    Ok(())
}

impl System for TsBevySystem {
    type In = ();
    type Out = ();

    fn name(&self) -> DebugName {
        self.name.clone()
    }

    fn flags(&self) -> SystemStateFlags {
        if self.deferred { SystemStateFlags::NON_SEND | SystemStateFlags::DEFERRED } else { SystemStateFlags::NON_SEND }
    }

    unsafe fn run_unsafe(&mut self, _input: SystemIn<'_, Self>, world: UnsafeWorldCell) -> Result<(), RunSystemError> {
        let this_run = world.increment_change_tick();
        let ticks = crate::host::RunTicks { last_run: self.last_run, this_run };
        self.last_run = this_run;
        // SAFETY: declared as non-send write access; the system is non-send, so on the main thread.
        let Some(mut rt) = (unsafe { world.get_non_send_mut::<ScriptRuntime>() }) else { return Ok(()) };
        // SAFETY: the caller grants the access `initialize` returned, which is everything
        // `run_body` touches: `declared`.
        let ran = unsafe { crate::systems::run_body(world, &mut rt, &mut self.params, &self.decl, &self.cell, ticks, None, self.spawns) };
        if let Some(w) = ran.warn {
            rt.log.push(Pending::Warn(w, self.decl.location.clone()));
        }
        match ran.result {
            Ok(out) => {
                if self.deferred {
                    self.output = Some(Ok(out));
                }
            }
            Err(e) => {
                rt.log.push(Pending::Error(format!("system {} threw", self.decl.name), e));
                if self.deferred {
                    self.output = Some(Err(()));
                }
            }
        }
        Ok(())
    }

    fn apply_deferred(&mut self, world: &mut World) {
        let Some(out) = self.output.take() else { return };
        let label = format!("system {}", self.decl.name);
        let ports = &self.ports;
        crate::systems::with_systems(world, |world, rt, systems| {
            match out {
                Ok(o) => {
                    rt.apply_output(world, o.bytes, o.pool_used, &label);
                    crate::systems::settle_ports(world, ports, true);
                }
                Err(()) => crate::systems::settle_ports(world, ports, false),
            }
            crate::systems::drain_observers(world, rt, systems);
        });
        if crate::schedules::has_pending(world) {
            crate::schedules::claim(world, &label);
            crate::schedules::run_pending(world);
        }
    }

    fn queue_deferred(&mut self, _world: DeferredWorld) {}

    fn initialize(&mut self, world: &mut World) -> SystemAccess {
        self.last_run = world
            .get_resource::<SystemMemory>()
            .and_then(|m| m.0.get(&self.decl.name))
            .map_or_else(|| world.change_tick().relative_to(Tick::MAX), |m| m.last_run);
        let mut access = SystemAccess::default();
        access.ensure_metadata_access();
        let name = self.decl.name.clone();
        for d in declared(world, &self.params).into_iter().filter(|d| !d.query) {
            access.ensure_filtered_access(d.access);
        }
        // Queries declare through Bevy's own code, as a `Query` param does (checked at build).
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
    /// Schedule edits so far, for tests: one per touched schedule per commit.
    pub edits: u64,
}

/// A TS system to place.
struct Wanted<'a> {
    sys: &'a mut TsSystem,
    place: Place,
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
    {
        let mut lists: Vec<(InternedScheduleLabel, InternedSystemSet, bool, &mut Vec<TsSystem>)> = Vec::new();
        for (stage, list) in systems.stages.iter_mut() {
            lists.push((crate::schedules::stage_label(*stage), crate::schedules::stage_set(*stage).intern(), true, list));
        }
        for (name, list) in systems.schedules.iter_mut() {
            let key = crate::schedules::TsSchedule::new(name).0;
            lists.push((crate::schedules::label_of(world, name), crate::BevyTsSet::Schedule(key).intern(), true, list));
        }
        let sets = world.get_resource::<crate::sets::TsSets>().cloned().unwrap_or_default();
        for (SlotKey { set, after }, list) in systems.slots.iter_mut() {
            let Some(exposed) = sets.get(set) else { continue };
            let slot = if *after { BevyTsSlot::after(set) } else { BevyTsSlot::before(set) };
            lists.push((exposed.schedule, slot.intern(), false, list));
        }
        for (label, set, head, list) in lists {
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
                            let n = format!("{g}#{fp}");
                            group_sets.insert(g.clone(), n.clone());
                            n
                        }
                    };
                    sets.push(name);
                }
                let place = Place { label, set, after: after.clone() };
                after = After::System(sys.decl.name.clone());
                wanted.push(Wanted { sys, place, groups: sets });
            }
        }
    }

    let mut live = world.remove_resource::<LiveSystems>().unwrap_or_default();
    let mut edits: BTreeMap<InternedScheduleLabel, Edit> = BTreeMap::new();
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
            w.sys.cell = l.cell.clone();
            w.sys.params.clear();
            w.sys.condition = None;
            continue;
        }
        let edit = edits.entry(w.place.label).or_default();
        if live.systems.contains_key(&id) {
            edit.remove.push(id.1.clone());
        }
        let condition = w.sys.condition.take().map(|c| TsCondition::new(format!("{} condition", w.sys.decl.name), c));
        let system = TsBevySystem::new(world, w.sys);
        edit.add.push(Add { system, condition, place: w.place.clone(), groups: w.groups.clone(), identity: id.1.clone() });
        live.systems.insert(id, Live { fingerprint, place: w.place, cell: w.sys.cell.clone() });
    }
    let gone: Vec<(InternedScheduleLabel, String)> = live.systems.keys().filter(|k| !keep.contains(*k)).cloned().collect();
    for id in gone {
        live.systems.remove(&id);
        edits.entry(id.0).or_default().remove.push(id.1);
    }
    for (g, (label, c)) in group_conditions {
        let name = group_sets[&g].clone();
        if live.groups.insert((label, name.clone())) {
            edits.entry(label).or_default().groups.push((name, TsCondition::new(format!("runIf of group {g}"), c)));
        }
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
    let mut edits: BTreeMap<InternedScheduleLabel, Edit> = BTreeMap::new();
    for ((label, identity), _) in live.systems.drain() {
        edits.entry(label).or_default().remove.push(identity);
    }
    live.edits += edits.len() as u64;
    world.insert_resource(live);
    for (label, edit) in edits {
        let custom = world.get_resource::<crate::schedules::MadeStages>().is_some_and(|m| m.created.contains(&label));
        crate::schedules::edit(world, label, custom, move |world| edit.apply(world, label));
    }
}

struct Add {
    system: TsBevySystem,
    condition: Option<TsCondition>,
    place: Place,
    groups: Vec<String>,
    identity: String,
}

/// One schedule's edit: systems out, then group sets and systems in.
#[derive(Default)]
struct Edit {
    remove: Vec<String>,
    add: Vec<Add>,
    groups: Vec<(String, TsCondition)>,
}

impl Edit {
    fn apply(self, world: &mut World, label: InternedScheduleLabel) {
        world.resource_scope(|world, mut schedules: Mut<Schedules>| {
            for identity in self.remove {
                // Keeps the set, so the chain edge of the system after it holds for its rebuild.
                let _ = schedules.remove_systems_in_set(label, TsSystemSet(identity), world, ScheduleCleanupPolicy::RemoveSystemsOnlyAllowBreakages);
            }
            for (name, condition) in self.groups {
                schedules.configure_sets(label, TsGroupSet(name).run_if(condition));
            }
            for a in self.add {
                let mut config = a.system.in_set(a.place.set).in_set(TsSystemSet(a.identity));
                for g in a.groups {
                    config = config.in_set(TsGroupSet(g));
                }
                config = match a.place.after {
                    After::Head(set) => config.after(TsStageHead(set)),
                    After::System(prev) => config.after(TsSystemSet(prev)),
                    After::Nothing => config,
                };
                match a.condition {
                    Some(c) => schedules.add_systems(label, config.run_if(c)),
                    None => schedules.add_systems(label, config),
                };
            }
        });
    }
}
