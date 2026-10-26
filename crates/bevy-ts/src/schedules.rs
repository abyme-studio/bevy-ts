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

/// The set a stage's TS systems run in.
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

/// Stages and custom schedules made so far: each is a set configured in its schedule, with its
/// head, kept for the life of the app (an emptied one holds no TS system).
#[derive(Resource, Default, Debug)]
pub struct MadeStages {
    pub stages: BTreeSet<Stage>,
    /// Custom schedules, by manifest name, with a bevy-ts stage in them.
    pub schedules: BTreeSet<String>,
    /// Custom schedules bevy-ts created in `Schedules`.
    pub created: HashSet<InternedScheduleLabel>,
}

/// A schedule edit that waits because its schedule is running (a generation applied in `First`
/// makes the `First` stage): applied in `Last`, or in the next `First`.
pub struct DeferredEdit {
    pub label: InternedScheduleLabel,
    pub edit: Box<dyn FnOnce(&mut World) + Send + Sync>,
}

#[derive(Resource, Default)]
pub struct DeferredEdits(pub Vec<DeferredEdit>);

/// Applies schedule edits that waited for their schedule to finish, once it has.
pub(crate) fn apply_deferred_edits(world: &mut World) {
    if world.get_resource::<DeferredEdits>().is_none_or(|e| e.0.is_empty()) {
        return;
    }
    let all = std::mem::take(&mut world.resource_mut::<DeferredEdits>().0);
    let (ready, waiting): (Vec<_>, Vec<_>) = all.into_iter().partition(|e| world.resource::<Schedules>().contains(e.label));
    world.resource_mut::<DeferredEdits>().0.extend(waiting);
    for e in ready {
        (e.edit)(world);
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
pub(crate) fn edit(world: &mut World, label: InternedScheduleLabel, custom: bool, f: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let present = world.resource::<Schedules>().contains(label);
    let made = world.get_resource_or_init::<MadeStages>().created.contains(&label);
    let running = if custom { made } else { main_schedule(label) };
    if present || !running {
        if custom {
            world.resource_mut::<MadeStages>().created.insert(label);
        }
        if !present {
            world.resource_mut::<Schedules>().insert(Schedule::new(label));
        }
        f(world);
    } else {
        world.get_resource_or_init::<DeferredEdits>().0.push(DeferredEdit { label, edit: Box::new(f) });
    }
}

/// Configures a stage's set in `schedules`, where Bevy's own schedules place it.
pub(crate) fn configure_stage(schedules: &mut Schedules, stage: Stage) {
    let label = stage_label(stage);
    let set = stage_set(stage);
    match stage {
        Stage::First => drop(schedules.configure_sets(label, set.after(crate::runtime::pump))),
        Stage::RunFixedMainLoop => drop(schedules.configure_sets(label, set.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop))),
        Stage::PostUpdate => drop(schedules.configure_sets(label, set.before(bevy::transform::TransformSystems::Propagate))),
        _ => {}
    }
    crate::bevy_system::configure_stage(schedules, label, set.intern());
}

/// Configures a stage's set in its schedule.
fn make_stage(world: &mut World, stage: Stage) {
    edit(world, stage_label(stage), false, move |world| configure_stage(&mut world.resource_mut::<Schedules>(), stage));
}

/// Configures a custom schedule's stage, creating the schedule if absent.
fn make_schedule_stage(world: &mut World, name: &str) {
    let label = label_of(world, name);
    let custom = world.get_resource::<TsSchedules>().is_none_or(|s| s.get(name).is_none());
    let key: &'static str = TsSchedule::new(name).0;
    edit(world, label, custom, move |world| {
        crate::bevy_system::configure_stage(&mut world.resource_mut::<Schedules>(), label, BevyTsSet::Schedule(key).intern());
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

/// Configures the stages a just-committed generation names for the first time: one schedule edit
/// each, at this frame boundary. Later generations reuse them.
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

/// Bevy's three schedule orders, as label lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Orders {
    pub main: Vec<InternedScheduleLabel>,
    pub startup: Vec<InternedScheduleLabel>,
    pub fixed: Vec<InternedScheduleLabel>,
}

impl Orders {
    fn list(&mut self, order: ScheduleOrder) -> &mut Vec<InternedScheduleLabel> {
        match order {
            ScheduleOrder::Main => &mut self.main,
            ScheduleOrder::Startup => &mut self.startup,
            ScheduleOrder::Fixed => &mut self.fixed,
        }
    }

    fn without(mut self, labels: &[InternedScheduleLabel]) -> Self {
        for l in [&mut self.main, &mut self.startup, &mut self.fixed] {
            l.retain(|x| !labels.contains(x));
        }
        self
    }
}

/// The schedule orders bevy-ts places TS labels in. `Main` holds `MainScheduleOrder` out of the
/// world for the whole frame, so a generation applied inside a frame (the watcher's, in `First`;
/// BRP's, in `RemoteLast`) cannot edit it: its orders wait in `pending` for
/// [`apply_pending_order`], which runs in `Main` after `run_main`.
#[derive(Resource, Default, Debug)]
pub struct OrderState {
    /// The orders without bevy-ts's labels, as last seen.
    base: Option<Orders>,
    /// The labels bevy-ts placed.
    placed: Vec<InternedScheduleLabel>,
    /// A committed generation's orders and labels, waiting for `MainScheduleOrder`.
    pending: Option<(Orders, Vec<InternedScheduleLabel>)>,
}

/// The orders, when `Main` is not holding them.
fn read_orders(world: &World) -> Option<Orders> {
    let main = world.get_resource::<MainScheduleOrder>()?;
    let fixed = world.get_resource::<FixedMainScheduleOrder>().map(|f| f.labels.clone()).unwrap_or_default();
    Some(Orders { main: main.labels.clone(), startup: main.startup_labels.clone(), fixed })
}

/// The orders without bevy-ts's labels: read now when it can, else as last seen.
fn base(world: &World) -> Orders {
    let state = world.get_resource::<OrderState>();
    let placed = state.map(|s| s.placed.clone()).unwrap_or_default();
    match read_orders(world) {
        Some(o) => o.without(&placed),
        None => state.and_then(|s| s.base.clone()).unwrap_or_default(),
    }
}

/// A schedule order's name in TS (`Update`, `game::Waves`, `net:tick`) as a label.
fn resolve(world: &World, name: &str) -> InternedScheduleLabel {
    builtin(name).unwrap_or_else(|| label_of(world, name))
}

/// The orders a generation's declarations make: bevy-ts's labels from the last generation
/// removed, then each declared label inserted at its anchor, an anchor that is itself declared
/// placed first. Fails naming every label it cannot place.
fn plan(world: &World, decls: &[ScheduleOrderDecl]) -> Result<Orders, Vec<ScriptError>> {
    let mut orders = base(world);
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    let mut todo = Vec::new();
    for d in decls {
        if seen.insert(&d.label) {
            todo.push(d);
        } else {
            errors.push(ScriptError::at(format!("schedule {} is inserted into a schedule order twice", d.label), d.location.clone()));
        }
    }
    loop {
        let before = todo.len();
        todo.retain(|d| {
            let label = resolve(world, &d.label);
            let anchor = resolve(world, d.after.as_deref().or(d.before.as_deref()).unwrap_or(""));
            let list = orders.list(d.order);
            let Some(i) = list.iter().position(|x| *x == anchor) else { return true };
            list.insert(if d.after.is_some() { i + 1 } else { i }, label);
            false
        });
        if todo.is_empty() || todo.len() == before {
            break;
        }
    }
    for d in todo {
        let which = match d.order {
            ScheduleOrder::Main => "MainScheduleOrder",
            ScheduleOrder::Fixed => "FixedMainScheduleOrder",
            ScheduleOrder::Startup => "the startup schedules",
        };
        let anchor = d.after.as_deref().or(d.before.as_deref()).unwrap_or("");
        errors.push(ScriptError::at(format!("schedule {} is inserted relative to {anchor}, which is not in {which}", d.label), d.location.clone()));
    }
    if errors.is_empty() { Ok(orders) } else { Err(errors) }
}

/// Checks a generation's schedule order before it commits.
pub(crate) fn check_order(world: &World, decls: &[ScheduleOrderDecl]) -> Vec<ScriptError> {
    if decls.is_empty() {
        return Vec::new();
    }
    plan(world, decls).err().unwrap_or_default()
}

fn write_orders(main: &mut MainScheduleOrder, fixed: Option<&mut FixedMainScheduleOrder>, orders: Orders) {
    if main.labels != orders.main {
        main.labels = orders.main;
    }
    if main.startup_labels != orders.startup {
        main.startup_labels = orders.startup;
    }
    if let Some(f) = fixed
        && f.labels != orders.fixed
    {
        f.labels = orders.fixed;
    }
}

/// Places a committed generation's labels in the schedule orders, now or, while `Main` holds
/// them, once it returns. A label the generation no longer inserts leaves its order.
pub(crate) fn reconcile_order(world: &mut World, decls: &[ScheduleOrderDecl]) {
    let previous = world.get_resource_or_init::<OrderState>().placed.clone();
    if decls.is_empty() && previous.is_empty() {
        return;
    }
    // Checked before the commit: it places every label.
    let Ok(orders) = plan(world, decls) else { return };
    for d in decls {
        init_schedule(world, &d.label);
    }
    let placed: Vec<InternedScheduleLabel> = decls.iter().map(|d| resolve(world, &d.label)).collect();
    let base = orders.clone().without(&placed);
    world.resource_mut::<OrderState>().base = Some(base);
    if world.contains_resource::<MainScheduleOrder>() {
        world.resource_mut::<OrderState>().placed = placed;
        world.resource_scope(|world, mut main: Mut<MainScheduleOrder>| {
            write_orders(&mut main, world.get_resource_mut::<FixedMainScheduleOrder>().as_deref_mut(), orders);
        });
    } else {
        world.resource_mut::<OrderState>().pending = Some((orders, placed));
    }
}

/// `Main`, after `run_main`: applies the orders a generation committed during the frame, and
/// keeps the last-seen orders current.
pub(crate) fn apply_pending_order(mut state: ResMut<OrderState>, main: Option<ResMut<MainScheduleOrder>>, fixed: Option<ResMut<FixedMainScheduleOrder>>) {
    let Some(mut main) = main else { return };
    let mut fixed = fixed;
    if let Some((orders, placed)) = state.pending.take() {
        state.placed = placed;
        write_orders(&mut main, fixed.as_deref_mut(), orders);
    }
    if main.is_changed() || fixed.as_ref().is_some_and(|f| f.is_changed()) {
        let orders = Orders {
            main: main.labels.clone(),
            startup: main.startup_labels.clone(),
            fixed: fixed.as_ref().map(|f| f.labels.clone()).unwrap_or_default(),
        };
        let placed = state.placed.clone();
        state.base = Some(orders.without(&placed));
    }
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
                        log_error(world, format!("runSchedule({name}) from {caller}: {e}; it is running already (a schedule cannot run inside itself) or was never created (app.initSchedule)"));
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

pub(crate) fn log_error(world: &mut World, message: String) {
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
