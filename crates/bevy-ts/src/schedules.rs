//! Which Bevy schedules TS systems run in: a bevy-ts stage per main and fixed schedule, made the
//! first time a generation names it; custom schedules (a TS `ScheduleLabel` is a
//! [`TsSchedule`], a crate exposes its own with `app.ts_schedule`) and their place in Bevy's
//! schedule orders; and the schedules and one-shot systems TS commands and [`RunTsSystem`] run.

use crate::BevyTsSet;
use crate::backend::ScriptError;
use crate::manifest::{ScheduleOrder, ScheduleOrderDecl, Stage};
use crate::systems::TsSystems;
use bevy::app::{FixedMainScheduleOrder, MainScheduleOrder, RunFixedMainLoopSystems};
use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::{InternedScheduleLabel, ScheduleLabel};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::panic::Location;
use std::sync::Mutex;

/// A TS `ScheduleLabel` in Rust: `TsSchedule::new("game::Waves")`. A Rust plugin adds systems to
/// it or runs it like any label.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TsSchedule(pub &'static str);

static LABELS: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

impl TsSchedule {
    /// The label for a TS path. Each path is leaked once for the life of the process; labels are
    /// few (one per declared `ScheduleLabel`).
    pub fn new(path: &str) -> Self {
        let mut labels = LABELS.lock().unwrap();
        if let Some(p) = labels.get(path) {
            return TsSchedule(p);
        }
        let p: &'static str = Box::leak(path.to_string().into_boxed_str());
        labels.insert(p);
        TsSchedule(p)
    }
}

/// One exposed schedule.
#[derive(Clone, Debug)]
pub struct ExposedSchedule {
    pub name: String,
    pub label: InternedScheduleLabel,
    pub location: &'static Location<'static>,
}

/// Schedules crates exposed with `app.ts_schedule`, by name.
#[derive(Resource, Default, Clone, Debug)]
pub struct TsSchedules {
    by_name: BTreeMap<String, ExposedSchedule>,
    duplicates: Vec<(ExposedSchedule, ExposedSchedule)>,
}

impl TsSchedules {
    pub fn get(&self, name: &str) -> Option<&ExposedSchedule> {
        self.by_name.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ExposedSchedule> {
        self.by_name.values()
    }

    pub fn check(&self) -> Result<(), String> {
        if self.duplicates.is_empty() {
            return Ok(());
        }
        let lines: Vec<String> =
            self.duplicates.iter().map(|(a, b)| format!("schedule {:?} is exposed twice: at {} and at {}", a.name, a.location, b.location)).collect();
        Err(format!("bevy-ts: {}", lines.join("; ")))
    }
}

/// Exposes `label` to TS as `name` (`net:tick`). A duplicate fails the app at startup, naming
/// both registrations.
#[track_caller]
pub fn expose(app: &mut App, name: &str, label: impl ScheduleLabel) {
    assert!(crate::sets::valid_name(name), "bevy-ts: exposed schedule name {name:?} must be <crate>:<name>, such as \"net:tick\"");
    let entry = ExposedSchedule { name: name.to_string(), label: label.intern(), location: Location::caller() };
    let mut s = app.world_mut().get_resource_or_init::<TsSchedules>();
    if let Some(prev) = s.by_name.get(name).cloned() {
        s.duplicates.push((prev, entry));
    } else {
        s.by_name.insert(name.to_string(), entry);
    }
}

/// The Bevy label a manifest schedule name means: an exposed one (`net:tick`), or a TS label.
pub fn label_of(world: &World, name: &str) -> InternedScheduleLabel {
    match world.get_resource::<TsSchedules>().and_then(|s| s.get(name)) {
        Some(e) => e.label,
        None => TsSchedule::new(name).intern(),
    }
}

/// A built-in schedule by the name TS gives it (`Update`), for schedule orders.
fn builtin(name: &str) -> Option<InternedScheduleLabel> {
    use bevy::app::*;
    Some(match name {
        "PreStartup" => PreStartup.intern(),
        "Startup" => Startup.intern(),
        "PostStartup" => PostStartup.intern(),
        "First" => First.intern(),
        "PreUpdate" => PreUpdate.intern(),
        "RunFixedMainLoop" => RunFixedMainLoop.intern(),
        "Update" => Update.intern(),
        "SpawnScene" => SpawnScene.intern(),
        "PostUpdate" => PostUpdate.intern(),
        "Last" => Last.intern(),
        "FixedFirst" => FixedFirst.intern(),
        "FixedPreUpdate" => FixedPreUpdate.intern(),
        "FixedUpdate" => FixedUpdate.intern(),
        "FixedPostUpdate" => FixedPostUpdate.intern(),
        "FixedLast" => FixedLast.intern(),
        _ => return None,
    })
}

/// The Bevy schedule a stage runs in.
pub fn stage_label(stage: Stage) -> InternedScheduleLabel {
    use bevy::app::*;
    match stage {
        Stage::First => First.intern(),
        Stage::PreUpdate => PreUpdate.intern(),
        Stage::RunFixedMainLoop => RunFixedMainLoop.intern(),
        Stage::Update => Update.intern(),
        Stage::SpawnScene => SpawnScene.intern(),
        Stage::PostUpdate => PostUpdate.intern(),
        Stage::Last => Last.intern(),
        Stage::FixedFirst => FixedFirst.intern(),
        Stage::FixedPreUpdate => FixedPreUpdate.intern(),
        Stage::FixedUpdate => FixedUpdate.intern(),
        Stage::FixedPostUpdate => FixedPostUpdate.intern(),
        Stage::FixedLast => FixedLast.intern(),
    }
}

/// The set a stage's exclusive system runs in.
pub fn stage_set(stage: Stage) -> BevyTsSet {
    match stage {
        Stage::First => BevyTsSet::First,
        Stage::PreUpdate => BevyTsSet::PreUpdate,
        Stage::RunFixedMainLoop => BevyTsSet::RunFixedMainLoop,
        Stage::Update => BevyTsSet::Update,
        Stage::SpawnScene => BevyTsSet::SpawnScene,
        Stage::PostUpdate => BevyTsSet::PostUpdate,
        Stage::Last => BevyTsSet::Last,
        Stage::FixedFirst => BevyTsSet::FixedFirst,
        Stage::FixedPreUpdate => BevyTsSet::FixedPreUpdate,
        Stage::FixedUpdate => BevyTsSet::FixedUpdate,
        Stage::FixedPostUpdate => BevyTsSet::FixedPostUpdate,
        Stage::FixedLast => BevyTsSet::FixedLast,
    }
}

/// Stages and custom schedules made so far. Each is one exclusive system, kept for the life of
/// the app (an emptied one is skipped).
#[derive(Resource, Default, Debug)]
pub struct MadeStages {
    pub stages: BTreeSet<Stage>,
    /// Custom schedules, by manifest name, with a bevy-ts stage in them.
    pub schedules: BTreeSet<String>,
    /// Custom schedules bevy-ts created in `Schedules`.
    pub created: HashSet<InternedScheduleLabel>,
    /// Labels bevy-ts placed in a schedule order, and where.
    pub ordered: BTreeMap<String, ScheduleOrderDecl>,
}

/// A schedule edit that waits because its schedule is running (a generation applied in `First`
/// makes the `First` stage): applied in `Last`, or in the next `First`.
pub struct DeferredEdit {
    pub label: InternedScheduleLabel,
    pub edit: Box<dyn FnOnce(&mut Schedules) + Send + Sync>,
}

#[derive(Resource, Default)]
pub struct DeferredEdits(pub Vec<DeferredEdit>);

/// Applies schedule edits that waited for their schedule to finish, once it has.
pub(crate) fn apply_deferred_edits(mut edits: ResMut<DeferredEdits>, mut schedules: ResMut<Schedules>) {
    if edits.0.is_empty() {
        return;
    }
    let (ready, waiting): (Vec<_>, Vec<_>) = edits.0.drain(..).partition(|e| schedules.contains(e.label));
    edits.0 = waiting;
    for e in ready {
        (e.edit)(&mut schedules);
    }
}

/// The main schedules, which always exist: one missing from `Schedules` is running.
fn main_schedule(label: InternedScheduleLabel) -> bool {
    use bevy::app::*;
    [First.intern(), PreUpdate.intern(), RunFixedMainLoop.intern(), Update.intern(), SpawnScene.intern(), PostUpdate.intern(), Last.intern()].contains(&label)
}

/// Edits `label`'s schedule now, or once it stops running. A missing main schedule is running,
/// as is a missing custom one bevy-ts created; a missing fixed schedule never runs while a
/// generation applies (in `First`, or outside any schedule), so it is created.
fn edit(world: &mut World, label: InternedScheduleLabel, custom: bool, f: impl FnOnce(&mut Schedules) + Send + Sync + 'static) {
    let present = world.resource::<Schedules>().contains(label);
    let made = world.get_resource_or_init::<MadeStages>().created.contains(&label);
    let running = if custom { made } else { main_schedule(label) };
    if present || !running {
        if custom {
            world.resource_mut::<MadeStages>().created.insert(label);
        }
        f(&mut world.resource_mut::<Schedules>());
    } else {
        world.get_resource_or_init::<DeferredEdits>().0.push(DeferredEdit { label, edit: Box::new(f) });
    }
}

/// Adds a stage's exclusive system to its schedule.
fn make_stage(world: &mut World, stage: Stage) {
    let label = stage_label(stage);
    let set = stage_set(stage);
    edit(world, label, false, move |schedules| {
        let run = move |world: &mut World| crate::systems::run_stage(world, stage);
        match stage {
            Stage::First => schedules.add_systems(label, run.in_set(set).after(crate::runtime::pump)),
            Stage::RunFixedMainLoop => schedules.add_systems(label, run.in_set(set).in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)),
            _ => schedules.add_systems(label, run.in_set(set)),
        };
    });
}

/// Adds a custom schedule's stage, creating the schedule if absent.
fn make_schedule_stage(world: &mut World, name: &str) {
    let label = label_of(world, name);
    let custom = world.get_resource::<TsSchedules>().is_none_or(|s| s.get(name).is_none());
    let key: &'static str = TsSchedule::new(name).0;
    edit(world, label, custom, move |schedules| {
        schedules.add_systems(label, (move |world: &mut World| crate::systems::run_custom_stage(world, key)).in_set(BevyTsSet::Schedule(key)));
    });
}

/// Creates a custom schedule if absent (`app.initSchedule`).
fn init_schedule(world: &mut World, name: &str) {
    let label = label_of(world, name);
    if world.resource::<Schedules>().contains(label) || world.get_resource_or_init::<MadeStages>().created.contains(&label) {
        return;
    }
    world.resource_mut::<MadeStages>().created.insert(label);
    world.resource_mut::<Schedules>().insert(Schedule::new(label));
}

/// Makes the stages a just-committed generation names for the first time: one schedule edit
/// each, at this frame boundary. Later generations reuse them without touching schedules.
pub(crate) fn ensure_stages(world: &mut World, init: &[String]) {
    let (stages, schedules): (Vec<Stage>, Vec<String>) = match world.get_resource::<TsSystems>() {
        Some(s) => (
            s.stages.iter().filter(|(k, l)| !k.eager() && !l.is_empty()).map(|(k, _)| *k).collect(),
            s.schedules.iter().filter(|(_, l)| !l.is_empty()).map(|(k, _)| k.clone()).collect(),
        ),
        None => return,
    };
    for name in init {
        init_schedule(world, name);
    }
    for stage in stages {
        if world.get_resource_or_init::<MadeStages>().stages.insert(stage) {
            make_stage(world, stage);
        }
    }
    for name in schedules {
        if world.get_resource_or_init::<MadeStages>().schedules.insert(name.clone()) {
            make_schedule_stage(world, &name);
        }
    }
}

/// Checks a generation's schedule order before it commits: every label is inserted relative to
/// a schedule that is in that order.
pub(crate) fn check_order(world: &World, decls: &[ScheduleOrderDecl]) -> Vec<ScriptError> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for d in decls {
        let anchor = d.after.as_deref().or(d.before.as_deref()).unwrap_or("");
        if !seen.insert(&d.label) {
            errors.push(ScriptError::at(format!("schedule {} is inserted into a schedule order twice", d.label), d.location.clone()));
            continue;
        }
        let labels = order_labels(world, d.order);
        let pending: Vec<&String> = decls.iter().filter(|o| o.order == d.order && o.label != d.label).map(|o| &o.label).collect();
        let known = resolve(world, anchor).is_some_and(|l| labels.contains(&l)) || pending.iter().any(|p| p.as_str() == anchor);
        if !known {
            let which = match d.order {
                ScheduleOrder::Main => "MainScheduleOrder",
                ScheduleOrder::Fixed => "FixedMainScheduleOrder",
                ScheduleOrder::Startup => "the startup schedules",
            };
            errors.push(ScriptError::at(format!("schedule {} is inserted relative to {anchor}, which is not in {which}", d.label), d.location.clone()));
        }
    }
    errors
}

/// A schedule order's name in TS (`Update`, `game::Waves`, `net:tick`) as a label.
fn resolve(world: &World, name: &str) -> Option<InternedScheduleLabel> {
    builtin(name).or_else(|| Some(label_of(world, name)))
}

fn order_labels(world: &World, order: ScheduleOrder) -> Vec<InternedScheduleLabel> {
    match order {
        ScheduleOrder::Main => world.get_resource::<MainScheduleOrder>().map(|o| o.labels.clone()).unwrap_or_default(),
        ScheduleOrder::Startup => world.get_resource::<MainScheduleOrder>().map(|o| o.startup_labels.clone()).unwrap_or_default(),
        ScheduleOrder::Fixed => world.get_resource::<FixedMainScheduleOrder>().map(|o| o.labels.clone()).unwrap_or_default(),
    }
}

fn order_mut(world: &mut World, order: ScheduleOrder, f: impl FnOnce(&mut Vec<InternedScheduleLabel>)) {
    match order {
        ScheduleOrder::Main => f(&mut world.get_resource_or_init::<MainScheduleOrder>().labels),
        ScheduleOrder::Startup => f(&mut world.get_resource_or_init::<MainScheduleOrder>().startup_labels),
        ScheduleOrder::Fixed => f(&mut world.get_resource_or_init::<FixedMainScheduleOrder>().labels),
    }
}

/// Reconciles schedule orders with a committed generation: each declared label is removed and
/// inserted at its declared place; a label a later generation no longer inserts leaves its order.
pub(crate) fn reconcile_order(world: &mut World, decls: &[ScheduleOrderDecl]) {
    let previous = std::mem::take(&mut world.get_resource_or_init::<MadeStages>().ordered);
    for (name, d) in &previous {
        if decls.iter().all(|x| &x.label != name) {
            let label = label_of(world, name);
            order_mut(world, d.order, |l| l.retain(|x| *x != label));
        }
    }
    // Declared order: an anchor that is itself declared is placed first.
    let mut todo: Vec<&ScheduleOrderDecl> = decls.iter().collect();
    for _ in 0..=decls.len() {
        let mut rest = Vec::new();
        for d in todo {
            if previous.get(&d.label) == Some(d) && order_labels(world, d.order).contains(&label_of(world, &d.label)) {
                continue;
            }
            let label = label_of(world, &d.label);
            init_schedule(world, &d.label);
            let anchor = d.after.as_deref().or(d.before.as_deref()).and_then(|a| resolve(world, a));
            let Some(anchor) = anchor else { continue };
            order_mut(world, d.order, |l| l.retain(|x| *x != label));
            let mut placed = false;
            order_mut(world, d.order, |l| {
                if let Some(i) = l.iter().position(|x| *x == anchor) {
                    l.insert(if d.after.is_some() { i + 1 } else { i }, label);
                    placed = true;
                }
            });
            if !placed {
                rest.push(d);
            }
        }
        if rest.is_empty() {
            break;
        }
        todo = rest;
    }
    world.resource_mut::<MadeStages>().ordered = decls.iter().map(|d| (d.label.clone(), d.clone())).collect();
}

/// What a `runSchedule` or `runSystem` names.
#[derive(Clone, Debug)]
pub enum RunTarget {
    Schedule(String),
    System { identity: String, input: Option<Vec<u8>> },
}

/// A run queued by commands: TS's `commands.runSchedule`/`runSystem`, or [`RunTsSystem`].
#[derive(Clone, Debug)]
pub struct PendingRun {
    pub target: RunTarget,
    /// Who queued it, for errors; filled by the stage that ran the caller.
    pub caller: Option<String>,
}

/// Runs queued by applied commands, run before the next system of the stage.
#[derive(Resource, Default, Debug)]
pub struct PendingRuns(pub VecDeque<PendingRun>);

pub(crate) fn has_pending(world: &World) -> bool {
    world.get_resource::<PendingRuns>().is_some_and(|p| !p.0.is_empty())
}

/// Names the caller of runs queued without one.
pub(crate) fn claim(world: &mut World, caller: &str) {
    if let Some(mut p) = world.get_resource_mut::<PendingRuns>() {
        for r in p.0.iter_mut().filter(|r| r.caller.is_none()) {
            r.caller = Some(caller.to_string());
        }
    }
}

/// How deep runs may nest (a one-shot that runs itself recurses, as in Bevy).
const MAX_DEPTH: u32 = 64;

/// Runs every queued run, depth first: what a run queues runs before the runs queued beside it,
/// as Bevy applies a system's commands when it returns. Needs the runtime and `TsSystems` in the
/// world.
pub(crate) fn run_pending(world: &mut World) {
    run_pending_at(world, 0);
}

fn run_pending_at(world: &mut World, depth: u32) {
    loop {
        let Some(run) = world.get_resource_mut::<PendingRuns>().and_then(|mut p| p.0.pop_front()) else { return };
        let rest = std::mem::take(&mut world.resource_mut::<PendingRuns>().0);
        let caller = run.caller.clone().unwrap_or_else(|| "a Rust command".into());
        if depth >= MAX_DEPTH {
            log_error(world, format!("{} from {caller} nests {MAX_DEPTH} runs deep; skipped", describe(&run.target)));
        } else {
            match run.target {
                RunTarget::Schedule(name) => {
                    let label = label_of(world, &name);
                    if let Err(e) = world.try_run_schedule(label) {
                        log_error(world, format!("runSchedule({name}) from {caller}: {e}; a schedule cannot run inside itself"));
                    }
                }
                RunTarget::System { identity, input } => crate::systems::run_one_shot(world, &identity, input.as_deref(), &caller),
            }
            claim(world, &caller);
            run_pending_at(world, depth + 1);
        }
        let mut p = world.resource_mut::<PendingRuns>();
        let queued = std::mem::replace(&mut p.0, rest);
        p.0.extend(queued);
    }
}

fn describe(t: &RunTarget) -> String {
    match t {
        RunTarget::Schedule(n) => format!("runSchedule({n})"),
        RunTarget::System { identity, .. } => format!("runSystem({identity})"),
    }
}

fn log_error(world: &mut World, message: String) {
    let generation = crate::runtime::current_generation(world);
    world.resource_mut::<crate::log::ScriptLog>().push(crate::log::LogLevel::Error, message, None, generation);
}

/// Runs a TS one-shot system by identity (`game::waves#spawnWave`), as Bevy's
/// `Commands::run_system`: `commands.queue(RunTsSystem::new(id))`.
#[derive(Clone, Debug)]
pub struct RunTsSystem {
    pub identity: String,
    /// The `In<E>` input: the event's bytes in its native layout.
    pub input: Option<Vec<u8>>,
}

impl RunTsSystem {
    pub fn new(identity: impl Into<String>) -> Self {
        RunTsSystem { identity: identity.into(), input: None }
    }

    /// With an `In<E>` input, the event's bytes in its layout.
    pub fn with(identity: impl Into<String>, bytes: Vec<u8>) -> Self {
        RunTsSystem { identity: identity.into(), input: Some(bytes) }
    }
}

impl Command for RunTsSystem {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.get_resource_or_init::<PendingRuns>().0.push_back(PendingRun { target: RunTarget::System { identity: self.identity, input: self.input }, caller: None });
        // Run now unless a TS call is under way; then the stage runs it before its next system.
        if world.contains_non_send::<crate::runtime::ScriptRuntime>() && world.contains_resource::<TsSystems>() {
            run_pending(world);
        }
    }
}

/// bevy-ts's anchors for every bridge stage and the fixed main loop. A stage's anchor names its
/// set even before the stage is made.
pub(crate) fn expose_anchors(app: &mut App) {
    for stage in Stage::ALL {
        let label: Interned<dyn ScheduleLabel> = stage_label(stage);
        crate::sets::expose_interned(app, &stage.anchor(), label, stage_set(stage).intern());
    }
    crate::sets::expose(app, "bevy:fixed-main-loop", bevy::app::RunFixedMainLoop, RunFixedMainLoopSystems::FixedMainLoop);
}
