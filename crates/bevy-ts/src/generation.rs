//! Generations: loading a compiled module set, checking it whole, and applying it at a frame
//! boundary, or keeping the previous one when anything fails.

use crate::backend::{Generation, ModuleSet, ScriptError};
use crate::commands::{self, TsSpawned};
use crate::components::{self, Migration, TsComponents};
use crate::layout::NativeLayouts;
use crate::log::{LogLevel, ScriptLog};
use crate::manifest::{LoadedGeneration, ResolvedComponent, ResolvedPayload, Resolution};
use crate::plugin_api::{RestartHandlers, SectionHandlers, SectionInput};
use crate::runtime::{RuntimeInfo, ScriptRuntime};
use crate::systems::{self, TsSystems};
use bevy::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// Compiles the module graph from disk. The dev CLI provides one; `bevy-ts/reload` uses it.
pub trait ModuleSource: Send + Sync + 'static {
    fn compile(&self) -> Result<ModuleSet, Vec<ScriptError>>;
}

/// The module source, when the app can reload.
#[derive(Resource, Clone)]
pub struct DevSource(pub std::sync::Arc<dyn ModuleSource>);

/// Something for the engine to do at the next frame boundary.
pub enum DevEvent {
    /// A file changed and compilation started.
    Compiling,
    /// A compiled set, or the errors that stopped it.
    Compiled(Result<ModuleSet, Vec<ScriptError>>, Duration),
    /// Files of the graph that are not modules (`.wgsl`) changed, and nothing else: no
    /// generation starts; [`crate::plugin_api::FileHandlers`] see the paths.
    FilesChanged(Vec<std::path::PathBuf>),
}

/// Where compiled generations arrive from the watcher thread.
#[derive(Resource)]
pub struct DevChannel(pub Mutex<Receiver<DevEvent>>);

impl DevChannel {
    pub fn new() -> (Sender<DevEvent>, Self) {
        let (tx, rx) = std::sync::mpsc::channel();
        (tx, DevChannel(Mutex::new(rx)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReloadStatus {
    Compiling,
    Applied,
    Failed,
}

/// One reload attempt's outcome.
#[derive(Clone, Debug, Serialize)]
pub struct ReloadReport {
    pub seq: u64,
    pub generation: u64,
    pub status: ReloadStatus,
    pub errors: Vec<ScriptError>,
    pub migrations: Vec<Migration>,
    #[serde(rename = "durationMs")]
    pub duration_ms: f64,
}

/// Recent reload attempts, for `bevy-ts/reload+watch`.
#[derive(Resource, Default)]
pub struct ReloadLog {
    pub reports: Vec<ReloadReport>,
    next_seq: u64,
    /// The newest save's outcome: a compile that failed, or the apply of a compiled set. A
    /// Stop's rebuild is not a save and leaves it.
    pub newest_save: Option<ReloadReport>,
}

impl ReloadLog {
    pub fn push(&mut self, mut r: ReloadReport) -> ReloadReport {
        r.seq = self.next_seq;
        self.next_seq += 1;
        if self.reports.len() == 256 {
            self.reports.remove(0);
        }
        self.reports.push(r.clone());
        r
    }

    pub fn since(&self, seq: u64) -> impl Iterator<Item = &ReloadReport> {
        self.reports.iter().filter(move |r| r.seq >= seq)
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }
}

/// Generation bookkeeping that lives with the world.
#[derive(Resource)]
pub struct Generations {
    /// The live generation, 0 before the first.
    pub live: u64,
    /// Startup names that already ran in this world.
    pub startups_run: HashSet<String>,
    /// Whether the game has started: `Startup` systems run only once it has. False under
    /// `bevy-ts dev --edit` until the first play or step ([`start_game`]).
    pub started: bool,
    /// Restarts applied in this world, a `restart` reload's or a Stop's.
    pub restarts: u64,
    /// Whether the start handlers ran for this game: once per start, before its first `Startup`.
    pub announced: bool,
}

impl Default for Generations {
    fn default() -> Self {
        Generations { live: 0, startups_run: HashSet::new(), started: true, restarts: 0, announced: false }
    }
}

/// The module set of the live generation, so a Stop rebuilds from it without compiling.
#[derive(Resource, Clone)]
pub struct LiveSet(pub ModuleSet);

/// Whether the game's `Startup` systems have run in this world.
pub fn started(world: &World) -> bool {
    world.get_resource::<Generations>().is_none_or(|g| g.started)
}

/// Starts a game that has not started: drops triggers queued before it, then runs the due
/// `Startup` systems now. Does nothing once started.
pub fn start_game(world: &mut World) {
    if started(world) {
        return;
    }
    world.resource_mut::<Generations>().started = true;
    world.resource_mut::<crate::observers::ObserverQueue>().0.clear();
    crate::states::sync(world);
    run_startups(world);
}

/// Notifies the start handlers, once per game start: before the first `Startup`, or earlier when
/// a caller starts the game in steps (a Play notifies before its `OnEnter(Play)`). Does nothing
/// once this game was announced.
pub fn announce_start(world: &mut World) {
    let Some(mut g) = world.get_resource_mut::<Generations>() else { return };
    if std::mem::replace(&mut g.announced, true) {
        return;
    }
    let handlers = world.get_resource::<crate::plugin_api::StartHandlers>().cloned().unwrap_or_default();
    for h in handlers.0.iter() {
        h(world);
    }
}

/// A JS heap above this triggers a fresh isolate on the next reload.
pub const HEAP_RESTART_BYTES: usize = 1 << 30;

/// Loads and applies a compiled module set as the next generation. `restart` despawns TS-spawned
/// entities, forgets startups, notifies plugins and uses a fresh engine instance first.
/// Never leaves a generation partly applied.
pub fn apply(world: &mut World, set: ModuleSet, restart: bool) -> ReloadReport {
    let report = apply_set(world, set, restart);
    let mut log = world.resource_mut::<ReloadLog>();
    let report = log.push(report);
    log.newest_save = Some(report.clone());
    report
}

/// [`apply`], without recording the report.
fn apply_set(world: &mut World, set: ModuleSet, restart: bool) -> ReloadReport {
    let mut a = Applying::begin(world, set, restart);
    if restart {
        a.reset(world);
        a.restart_engine();
    } else if a.rt.backend.heap_used().is_some_and(|h| h > HEAP_RESTART_BYTES) {
        crate::net::close_all(world);
        a.rt.backend.restart();
        world.insert_resource(TsSystems::default());
        a.rt.entries.clear();
    }
    a.load(world);
    a.sections(world);
    let report = a.apply(world);
    if report.status == ReloadStatus::Applied {
        run_startups(world);
    }
    report
}

/// A generation being applied, in the steps [`apply`] and [`rebuild`] take: [`Applying::begin`],
/// for a restart [`Applying::reset`] and [`Applying::restart_engine`], then [`Applying::load`],
/// [`Applying::sections`], [`Applying::apply`] and [`run_startups`]. Public so a bench can time
/// each; a caller takes them all, in that order.
pub struct Applying {
    rt: ScriptRuntime,
    started: Instant,
    number: u64,
    set: Option<ModuleSet>,
    #[cfg(feature = "reflect")]
    assets: crate::reflect_asset::Reloading,
    generation: Option<Generation>,
    loaded: Option<LoadedGeneration>,
    /// The old generation's attachments not built yet, put back if this one fails.
    unbuilt: Option<crate::observers::NewTargeted>,
    outcome: Option<Result<Applied, Vec<ScriptError>>>,
}

impl Applying {
    /// Takes the runtime out of the world for the generation after the live one.
    pub fn begin(world: &mut World, set: ModuleSet, restart: bool) -> Self {
        let started = Instant::now();
        crate::layout_reflect::fill_once(world);
        // Script-made assets' refs go stale once the next generation applies; a restart ends the
        // live one now and removes the assets too.
        #[cfg(feature = "reflect")]
        let assets = crate::reflect_asset::reloading(world, restart);
        // Flush the previous generation's console output so the log reads in order.
        crate::runtime::pump(world);
        let number = world.resource::<Generations>().live + 1;
        let rt = world.remove_non_send::<ScriptRuntime>().expect("bevy-ts runtime present");
        Applying {
            rt,
            started,
            number,
            set: Some(set),
            #[cfg(feature = "reflect")]
            assets,
            generation: None,
            loaded: None,
            unbuilt: None,
            outcome: None,
        }
    }

    /// A restart's first step: despawns TS-spawned entities and TS resources, forgets startups
    /// and per-generation memory, and runs the restart handlers.
    pub fn reset(&mut self, world: &mut World) {
        let spawned: Vec<Entity> = world.query_filtered::<Entity, With<TsSpawned>>().iter(world).collect();
        for e in spawned {
            // Children may already be gone with their parent.
            let _ = world.try_despawn(e);
        }
        let edit = world.get_resource::<State<crate::EngineMode>>().is_some_and(|s| *s.get() == crate::EngineMode::Edit);
        let mut g = world.resource_mut::<Generations>();
        g.startups_run.clear();
        if edit {
            g.started = false;
            g.announced = false;
        }
        g.restarts += 1;
        // TS resources go with their entities, so the generation inserts them again; Rust ones stay.
        let resources: Vec<Entity> = {
            let ts = world.resource::<TsComponents>();
            world.resource_entities().iter().filter(|(id, _)| ts.by_id(*id).is_some()).map(|(_, e)| e).collect()
        };
        for e in resources {
            // `IsResource` leaves first, taking the value and the cache entry: Bevy forbids
            // despawning a resource entity.
            world.entity_mut(e).remove::<bevy::ecs::resource::IsResource>();
            world.flush();
            let _ = world.try_despawn(e);
        }
        world.insert_resource(crate::memory::SystemMemory::default());
        world.insert_resource(crate::session::SessionBytes::default());
        world.insert_resource(crate::conditions::ConditionMemory::default());
        world.insert_resource(crate::conditions::NativeConditions::default());
        world.resource_mut::<crate::messages::TsPayloads>().reset();
        crate::observers::detach(world, |_| true);
        crate::states::restart(world);
        let handlers = world.resource::<RestartHandlers>().clone();
        for h in handlers.0.iter() {
            h(world);
        }
        world.insert_resource(TsSystems::default());
        self.rt.entries.clear();
    }

    /// A restart's second step: a fresh engine instance.
    pub fn restart_engine(&mut self) {
        self.rt.backend.restart();
    }

    /// Evaluates the module set and takes its manifest.
    pub fn load(&mut self, world: &mut World) {
        // Attachments made under the old generation are rebuilt from their names; a failed load keeps
        // those the old generation has not built yet, for its next drain.
        self.unbuilt = world.remove_resource::<crate::observers::NewTargeted>();
        let observed = crate::observers::observed(world);
        let env = world.get_resource::<crate::net::NetPolicy>().map(|p| p.env.clone()).unwrap_or_default();
        let argv = world.get_resource::<crate::backend::ScriptArgs>().map(|a| a.0.clone()).unwrap_or_default();
        let set = self.set.take().expect("load runs once");
        let generation = Generation { number: self.number, set, env, argv, now_ms: crate::net::now_ms(world), observed };
        let loaded = {
            let mut host = crate::host::LoadHost { world: &*world };
            self.rt.backend.load_with(&generation, &mut host)
        };
        // Assets a scene's first render made go in before its tree applies.
        #[cfg(feature = "reflect")]
        crate::reflect_asset::flush(world);
        match loaded {
            Ok(l) => self.loaded = Some(l),
            Err(e) => self.outcome = Some(Err(e)),
        }
        self.generation = Some(generation);
    }

    /// Checks the manifest whole, builds the systems, runs each plugin's section check and
    /// commits the components. Nothing after it can fail.
    pub fn sections(&mut self, world: &mut World) {
        let Some(loaded) = self.loaded.take() else { return };
        let generation = self.generation.as_ref().expect("load ran");
        self.outcome = Some(commit(world, &mut self.rt, generation, loaded));
    }

    /// Applies the sections, records the live generation, and puts the runtime back.
    pub fn apply(self, world: &mut World) -> ReloadReport {
        let Applying { rt, started, number, generation, unbuilt, outcome, .. } = self;
        #[cfg(feature = "reflect")]
        let assets = self.assets;
        if let (Some(Err(_)), Some(unbuilt)) = (&outcome, unbuilt) {
            world.insert_resource(unbuilt);
        }
        let report = match outcome.expect("sections ran") {
            Ok((migrations, sections, requests)) => {
                #[cfg(feature = "reflect")]
                crate::reflect_asset::applied(world, assets);
                world.insert_resource(LiveSet(generation.expect("load ran").set));
                world.resource_mut::<Generations>().live = number;
                world.resource_mut::<RuntimeInfo>().generation = number;
                // The previous generation's connections go; the new one's opens come after.
                crate::net::close_other_generations(world, number);
                crate::net::apply_requests(world, requests);
                let appliers = world.get_resource::<crate::plugin_api::SectionAppliers>().cloned().unwrap_or_default();
                let mut names: Vec<&String> = appliers.0.keys().collect();
                names.sort();
                for name in names {
                    let s = sections.iter().find(|s| &s.name == name);
                    let input = crate::plugin_api::AppliedSection {
                        generation: number,
                        present: s.is_some(),
                        bytes: s.map_or(&[][..], |s| &s.bytes),
                        location: s.and_then(|s| s.location.clone()),
                    };
                    for apply in &appliers.0[name] {
                        apply(world, &input);
                    }
                }
                for m in &migrations {
                    world.resource_mut::<ScriptLog>().push(LogLevel::Info, m.to_string(), None, number);
                }
                let systems = world.resource::<TsSystems>().iter().count();
                world.resource_mut::<ScriptLog>().push(
                    LogLevel::Info,
                    format!("generation {number} applied: {systems} systems in {:.1} ms", started.elapsed().as_secs_f64() * 1000.0),
                    None,
                    number,
                );
                ReloadReport {
                    seq: 0,
                    generation: number,
                    status: ReloadStatus::Applied,
                    errors: vec![],
                    migrations,
                    duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                }
            }
            Err(errors) => {
                // What the failed generation made never stays; the live one's refs still work.
                #[cfg(feature = "reflect")]
                crate::reflect_asset::failed(world, assets);
                let live = world.resource::<Generations>().live;
                let mut log = world.resource_mut::<ScriptLog>();
                for e in &errors {
                    log.push(LogLevel::Error, format!("generation {number} failed: {}", e.message), e.location.clone(), live);
                }
                ReloadReport {
                    seq: 0,
                    generation: number,
                    status: ReloadStatus::Failed,
                    errors,
                    migrations: vec![],
                    duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                }
            }
        };
        world.insert_non_send(rt);
        report
    }
}

/// Records a compile failure (nothing was loaded).
pub fn compile_failed(world: &mut World, errors: Vec<ScriptError>, took: Duration) -> ReloadReport {
    let live = world.resource::<Generations>().live;
    let mut log = world.resource_mut::<ScriptLog>();
    for e in &errors {
        log.push(LogLevel::Error, format!("compile failed: {}", e.message), e.location.clone(), live);
    }
    let mut log = world.resource_mut::<ReloadLog>();
    let report = log.push(ReloadReport {
        seq: 0,
        generation: live + 1,
        status: ReloadStatus::Failed,
        errors,
        migrations: vec![],
        duration_ms: took.as_secs_f64() * 1000.0,
    });
    log.newest_save = Some(report.clone());
    report
}

type Applied = (Vec<Migration>, Vec<crate::manifest::Section>, Vec<crate::net::NetRequest>);

/// Resources are components on their own resource entity: one declaration list for both.
fn component_decls(manifest: &crate::manifest::Manifest) -> Vec<crate::manifest::ComponentDecl> {
    let mut all = manifest.components.clone();
    all.extend(manifest.resources.iter().map(|r| crate::manifest::ComponentDecl {
        name: r.name.clone(),
        fields: r.fields.clone(),
        location: r.location.clone(),
        ..Default::default()
    }));
    all
}

fn commit(world: &mut World, rt: &mut ScriptRuntime, generation: &Generation, loaded: LoadedGeneration) -> Result<Applied, Vec<ScriptError>> {
    let number = generation.number;
    let LoadedGeneration { manifest, sections, requests } = loaded;

    let fail = |rt: &mut ScriptRuntime, errors: Vec<ScriptError>| {
        rt.backend.discard(number);
        Err(errors)
    };

    // Declarations, checked whole before anything moves.
    let decls = component_decls(&manifest);
    let mut errors = components::validate(&decls, world.resource::<NativeLayouts>());
    let handlers = world.resource::<SectionHandlers>().clone();
    let appliers = world.get_resource::<crate::plugin_api::SectionAppliers>().cloned().unwrap_or_default();
    for s in &sections {
        if !handlers.0.contains_key(&s.name) && !appliers.0.contains_key(&s.name) {
            errors.push(ScriptError::at(format!("no plugin registered manifest section {}", s.name), s.location.clone()));
        }
    }
    let kinds = world.resource::<commands::CommandKinds>().clone();
    for k in &manifest.command_kinds {
        if kinds.opcode(k).is_none() {
            errors.push(ScriptError::new(format!("no plugin registered command kind {k}")));
        }
    }
    let mut seen = HashSet::new();
    for e in &manifest.entries {
        if !seen.insert(e.name.as_str()) {
            errors.push(ScriptError::at(format!("entry {} is declared twice", e.name), e.location.clone()));
        }
    }
    let mut seen = HashSet::new();
    for p in manifest.messages.iter().chain(&manifest.events) {
        if !seen.insert(p.name.as_str()) {
            errors.push(ScriptError::at(format!("{} is declared twice", p.name), p.location.clone()));
        }
        for f in &p.fields {
            if let Err(e) = f.ty.validate() {
                errors.push(ScriptError::at(format!("{}.{}: {e}", p.name, f.name), p.location.clone()));
            }
            if f.ty.is_heap() {
                errors.push(ScriptError::at(format!("{}.{}: {}", p.name, f.name, crate::heap::REFUSED), p.location.clone()));
            }
        }
    }
    for st in &manifest.states {
        if let Some(v) = &st.initial
            && !st.values.contains(v)
        {
            errors.push(ScriptError::at(format!("{} has no value {v}", st.name), st.location.clone()));
        }
        let derived = st.source.is_some() || st.computed.is_some();
        if derived && st.initial.is_some() {
            errors.push(ScriptError::at(format!("{} is a sub or computed state: its value follows its sources, so it takes no initState", st.name), st.location.clone()));
        }
        let known = |name: &str| manifest.states.iter().find(|o| o.name == name);
        if let Some(src) = &st.source {
            match known(&src.state) {
                None => errors.push(ScriptError::at(format!("{}: its source {} is no state machine", st.name, src.state), st.location.clone())),
                Some(o) if !o.values.contains(&src.value) => {
                    errors.push(ScriptError::at(format!("{}: its source {} has no value {}", st.name, src.state, src.value), st.location.clone()))
                }
                _ => {}
            }
        }
        for name in st.computed.iter().flatten() {
            if known(name).is_none() {
                errors.push(ScriptError::at(format!("{}: its source {name} is no state machine", st.name), st.location.clone()));
            }
        }
    }
    errors.extend(crate::schedules::check_order(world, &manifest.schedule_order));
    if !errors.is_empty() {
        return fail(rt, errors);
    }

    // Payloads first (systems resolve message, event and state indices), kept aside so a failed
    // generation leaves the old ones untouched.
    // State slots are kept for the life of the app; a failed generation gives back the ones it took.
    let old_slots = world.get_resource_or_init::<crate::states::TsStates>().slots();
    let mut slot_errors = Vec::new();
    let mut assigned = std::collections::HashMap::new();
    let (depths, depth_errors) = crate::states::depths(&manifest.states);
    slot_errors.extend(depth_errors.into_iter().map(|(i, e)| ScriptError::at(e, manifest.states[i].location.clone())));
    for st in &manifest.states {
        if !slot_errors.is_empty() {
            break;
        }
        match world.resource_mut::<crate::states::TsStates>().assign(&st.name, depths[&st.name]) {
            Ok(n) => drop(assigned.insert(st.name.clone(), n)),
            Err(e) => slot_errors.push(ScriptError::at(e, st.location.clone())),
        }
    }
    if !slot_errors.is_empty() {
        world.resource_mut::<crate::states::TsStates>().restore_slots(old_slots);
        return fail(rt, slot_errors);
    }
    let old_payloads = world.remove_resource::<crate::messages::TsPayloads>().unwrap_or_default();
    let mut payloads = old_payloads.clone();
    payloads.declare(&manifest.messages, &manifest.events, &manifest.states, &mut |name| assigned[name]);
    if let Some(rust) = world.get_resource::<crate::states::RustStates>() {
        payloads.declare_rust(&rust.0.iter().map(|s| (s.path.clone(), s.values.clone())).collect::<Vec<_>>());
    }
    world.insert_resource(payloads);
    let restore = |world: &mut World, old: crate::messages::TsPayloads| {
        world.insert_resource(old);
        world.resource_mut::<crate::states::TsStates>().restore_slots(old_slots.clone());
    };

    let mut pending = components::prepare(world, &decls);
    let required_errors = pending.resolve_required(world.resource::<NativeLayouts>());
    if !required_errors.is_empty() {
        components::abandon(world, pending);
        restore(world, old_payloads);
        return fail(rt, required_errors);
    }
    let built = {
        let lookup = |name: &str| pending.lookup(name).map(|(id, l)| (id, l.size, l.has_heap()));
        let encode = |name: &str, values: &serde_json::Map<String, serde_json::Value>| {
            let (_, layout) = pending.lookup(name)?;
            Some(pending.encode(name, values).map_err(|e| e.to_string()).map(|b| (b, layout.clone())))
        };
        systems::build(world, number, &manifest.systems, &manifest.targeted, &manifest.groups, &lookup, &encode)
    };
    let systems = match built {
        Ok(s) => s,
        Err(errors) => {
            components::abandon(world, pending);
            restore(world, old_payloads);
            return fail(rt, errors);
        }
    };

    // Plugin sections, with the pending components already resolvable.
    for s in &sections {
        let Some(handler) = handlers.0.get(&s.name).cloned() else { continue };
        let input = SectionInput { generation: number, bytes: &s.bytes, location: s.location.clone(), components: &pending, sections: &sections };
        if let Err(errors) = handler(world, &input) {
            components::abandon(world, pending);
            restore(world, old_payloads);
            return fail(rt, errors);
        }
    }

    let mut resolution = Resolution { generation: number, ..Default::default() };
    if let Some(h) = world.get_resource::<crate::host::HostParams>() {
        resolution.ports = h.glue();
    }
    for (name, id, layout) in pending.iter() {
        // A heap field is never a column: no raw `q.tables` arrays for its component.
        let raw_writes = !layout.has_heap();
        resolution.components.insert(name.to_string(), ResolvedComponent { id: id.index() as u32, layout: layout.clone(), raw_writes, defaults: Vec::new(), reflected: false });
    }
    for n in world.resource::<NativeLayouts>().iter() {
        let raw_writes = n.holes.is_empty() && n.layout.fields.iter().all(|f| !f.ty.accessor_only());
        // Only a component inserted from bytes needs them: other defaults may hold pointers.
        let defaults = if n.insertable { n.defaults.clone() } else { Vec::new() };
        let r = ResolvedComponent { id: n.id.index() as u32, layout: n.layout.clone(), raw_writes, defaults, reflected: !n.insertable };
        resolution.components.insert(n.type_path.clone(), r.clone());
        resolution.components.insert(n.ts_name.clone(), r);
    }
    #[cfg(feature = "reflect")]
    {
        (resolution.reflect, resolution.holes) = crate::reflect_codec::resolve(world, &systems);
        let heap: Vec<_> = pending.iter().filter(|(.., l)| l.has_heap()).map(|(n, id, l)| (n.to_string(), id, l.clone())).collect();
        for (name, id, layout) in heap {
            let holes = crate::reflect_codec::ts_heap_holes(world, &name, id, &layout);
            resolution.holes.insert(name, holes);
        }
        resolution.rust_messages = crate::rust_events::codecs(&world.resource::<AppTypeRegistry>().read());
    }
    for clock in systems::Clock::ALL {
        let layout = systems::time_layout(clock).clone();
        resolution.components.insert(clock.resource().into(), ResolvedComponent { id: u32::MAX, layout, raw_writes: true, defaults: Vec::new(), reflected: false });
    }
    for (name, op) in kinds.all() {
        resolution.command_kinds.insert(name.to_string(), op);
    }
    {
        let p = world.resource::<crate::messages::TsPayloads>();
        for (i, m) in p.messages.iter().enumerate() {
            resolution.messages.insert(m.name.clone(), ResolvedPayload { index: i as u32, layout: m.layout.clone() });
        }
        for (i, e) in p.events.iter().enumerate() {
            resolution.events.insert(e.name.clone(), ResolvedPayload { index: i as u32, layout: e.layout.clone() });
        }
        for (i, st) in p.states.iter().enumerate() {
            resolution.states.insert(st.name.clone(), i as u32);
        }
    }
    if let Err(errors) = rt.backend.commit(number, &resolution) {
        components::abandon(world, pending);
        restore(world, old_payloads);
        return fail(rt, errors);
    }

    // Nothing can fail from here.
    let migrations = components::commit(world, pending);
    insert_resources(world, &manifest);
    let watched = systems.watched_lifecycles();
    let rust = systems.watched_rust();
    crate::conditions::prune_natives(world, &systems.natives);
    let stale: Vec<String> = generation.observed.iter().filter(|n| !systems.targeted.contains_key(*n)).cloned().collect();
    world.insert_resource(systems);
    if !stale.is_empty() {
        crate::observers::detach(world, |n| stale.iter().any(|s| s == n));
        for n in &stale {
            world.resource_mut::<ScriptLog>().push(LogLevel::Warn, format!("observe: {n} no longer exists; its entity observers are removed"), None, number);
        }
    }
    crate::sets::ensure_slots(world);
    crate::schedules::reconcile_order(world, &manifest.schedule_order);
    crate::schedules::ensure_stages(world, &manifest.init_schedules);
    crate::states::sync(world);
    crate::observers::install(world, &watched);
    crate::observers::install_rust(world, &rust);
    rt.entries = manifest.entries.iter().map(|e| e.name.clone()).collect();
    Ok((migrations, sections, requests))
}

/// Inserts each declared resource that does not exist yet, and points migrated resources'
/// `IsResource` at their new component id.
fn insert_resources(world: &mut World, manifest: &crate::manifest::Manifest) {
    use bevy::ecs::resource::IsResource;
    for r in &manifest.resources {
        let Some(c) = world.resource::<TsComponents>().live(&r.name).cloned() else { continue };
        // Only `app.insertResource` creates a resource; a declaration alone does not.
        let declared_insert = r.insert.is_some();
        if world.resource_entities().get(c.id).is_some() {
            continue;
        }
        // A migrated resource: its entity carries the new component, and `IsResource` of the old id.
        let migrated = {
            let mut q = world.query::<(Entity, &IsResource)>();
            q.iter(world).find(|(e, _)| world.get_by_id(*e, c.id).is_some()).map(|(e, _)| e)
        };
        if let Some(e) = migrated {
            world.entity_mut(e).insert(IsResource::new(c.id));
            continue;
        }
        if !declared_insert {
            continue;
        }
        let values = r.insert.clone().unwrap_or_default();
        let bytes = match crate::value::encode_component(&c.decl, &c.layout, &values) {
            Ok(b) => b,
            Err(e) => {
                world.resource_mut::<ScriptLog>().push(LogLevel::Error, format!("insertResource {}: {e}", r.name), r.location.clone(), 0);
                continue;
            }
        };
        let mut e = world.spawn(IsResource::new(c.id));
        // SAFETY: `c.id` is a TS component with `c.layout`; `bytes` is a value of it.
        unsafe { commands::insert_owned(&mut e, c.id, bytes) };
    }
}

/// Runs `PreStartup`, `Startup` and `PostStartup` systems whose identity has not run in this
/// world, in that order, once the game has started. Each system's commands apply before the next.
pub fn run_startups(world: &mut World) {
    if !started(world) {
        return;
    }
    announce_start(world);
    let Some(systems) = world.get_resource::<TsSystems>() else { return };
    let lists: Vec<(systems::ListKey, Vec<String>)> = [
        (systems::ListKey::PreStartup, &systems.pre_startup),
        (systems::ListKey::Startup, &systems.startup),
        (systems::ListKey::PostStartup, &systems.post_startup),
    ]
    .into_iter()
    .map(|(k, l)| (k, l.iter().map(|s| s.decl.name.clone()).collect()))
    .collect();
    for (key, names) in lists {
        let due: HashSet<String> = names.into_iter().filter(|n| world.resource_mut::<Generations>().startups_run.insert(n.clone())).collect();
        if !due.is_empty() {
            systems::run_startup_list(world, key, &due);
        }
    }
}

/// `First`: applies the newest compiled set from the watcher, if any.
pub fn apply_pending(world: &mut World) {
    let Some(channel) = world.get_resource::<DevChannel>() else { return };
    let events: Vec<DevEvent> = channel.0.lock().unwrap().try_iter().collect();
    for event in events {
        match event {
            DevEvent::Compiling => {
                let live = world.resource::<Generations>().live;
                world.resource_mut::<ReloadLog>().push(ReloadReport {
                    seq: 0,
                    generation: live + 1,
                    status: ReloadStatus::Compiling,
                    errors: vec![],
                    migrations: vec![],
                    duration_ms: 0.0,
                });
            }
            DevEvent::Compiled(Ok(set), _) => {
                apply(world, set, false);
            }
            DevEvent::Compiled(Err(errors), took) => {
                compile_failed(world, errors, took);
            }
            DevEvent::FilesChanged(paths) => {
                let handlers = world.get_resource::<crate::plugin_api::FileHandlers>().cloned().unwrap_or_default();
                for h in &handlers.0 {
                    h(world, &paths);
                }
            }
        }
    }
}

/// Compiles from the dev source and applies, as `bevy-ts/reload` does.
pub fn reload_now(world: &mut World, restart: bool) -> Result<ReloadReport, ScriptError> {
    let source = world
        .get_resource::<DevSource>()
        .cloned()
        .ok_or_else(|| ScriptError::new("reload is only available under bevy-ts dev"))?;
    let started = Instant::now();
    match source.0.compile() {
        Ok(set) => Ok(apply(world, set, restart)),
        Err(errors) => Ok(compile_failed(world, errors, started.elapsed())),
    }
}

/// Applies the live generation's module set again as a restart, without compiling: a Stop
/// (edit-mode). Fails when no generation was applied yet. It is not a save: `reload+watch` and
/// [`ReloadLog::newest_save`] see it only when it fails, which leaves the empty world with its
/// error on the log, as a failed first load does.
pub fn rebuild(world: &mut World) -> Result<ReloadReport, ScriptError> {
    // One copy for the attempt: a failed one keeps the live set for the next Stop.
    let set = world.get_resource::<LiveSet>().map(|l| l.0.clone()).ok_or_else(|| ScriptError::new("no generation has been applied yet"))?;
    let report = apply_set(world, set, true);
    Ok(match report.status {
        ReloadStatus::Failed => world.resource_mut::<ReloadLog>().push(report),
        _ => report,
    })
}

/// The live generation's TS components, for code that wants them all.
pub fn live_components(world: &World) -> Vec<(String, bevy::ecs::component::ComponentId)> {
    world
        .resource::<TsComponents>()
        .current()
        .filter(|c| c.state == components::ComponentState::Live)
        .map(|c| (c.decl.name.clone(), c.id))
        .collect()
}
