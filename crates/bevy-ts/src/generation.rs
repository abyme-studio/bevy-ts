//! Generations: loading a compiled module set, checking it whole, and applying it at a frame
//! boundary, or keeping the previous one when anything fails.

use crate::backend::{CallContext, Generation, ModuleSet, ScriptError, ScriptKey};
use crate::commands::{self, TsSpawned};
use crate::components::{self, Migration, TsComponents};
use crate::layout::NativeLayouts;
use crate::log::{LogLevel, ScriptLog};
use crate::manifest::{LoadedGeneration, ResolvedComponent, Resolution, StartupDecl};
use crate::plugin_api::{RestartHandlers, SectionHandlers, SectionInput};
use crate::runtime::{RuntimeInfo, ScriptRuntime, frame_info};
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
#[derive(Resource, Default)]
pub struct Generations {
    /// The live generation, 0 before the first.
    pub live: u64,
    /// Startup names that already ran in this world.
    pub startups_run: HashSet<String>,
}

/// A JS heap above this triggers a fresh isolate on the next reload.
pub const HEAP_RESTART_BYTES: usize = 1 << 30;

/// Loads and applies a module set as the next generation. `restart` despawns TS-spawned
/// entities, forgets startups, notifies plugins and uses a fresh engine instance first.
/// Never leaves a generation partly applied.
pub fn apply(world: &mut World, set: ModuleSet, restart: bool) -> ReloadReport {
    let started = Instant::now();
    let number = world.resource::<Generations>().live + 1;
    let mut rt = world.remove_non_send::<ScriptRuntime>().expect("bevy-ts runtime present");

    if restart {
        let spawned: Vec<Entity> = world.query_filtered::<Entity, With<TsSpawned>>().iter(world).collect();
        for e in spawned {
            // Children may already be gone with their parent.
            let _ = world.try_despawn(e);
        }
        world.resource_mut::<Generations>().startups_run.clear();
        let handlers = world.resource::<RestartHandlers>().clone();
        for h in handlers.0.iter() {
            h(world);
        }
        world.insert_resource(TsSystems::default());
        rt.entries.clear();
        rt.backend.restart();
    } else if rt.backend.heap_used().is_some_and(|h| h > HEAP_RESTART_BYTES) {
        rt.backend.restart();
        world.insert_resource(TsSystems::default());
        rt.entries.clear();
    }

    let result = load_and_apply(world, &mut rt, number, set);
    let report = match result {
        Ok((migrations, startups)) => {
            world.resource_mut::<Generations>().live = number;
            world.resource_mut::<RuntimeInfo>().generation = number;
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
            run_startups(world, &mut rt, startups);
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
    world.resource_mut::<ReloadLog>().push(report)
}

/// Records a compile failure (nothing was loaded).
pub fn compile_failed(world: &mut World, errors: Vec<ScriptError>, took: Duration) -> ReloadReport {
    let live = world.resource::<Generations>().live;
    let mut log = world.resource_mut::<ScriptLog>();
    for e in &errors {
        log.push(LogLevel::Error, format!("compile failed: {}", e.message), e.location.clone(), live);
    }
    world.resource_mut::<ReloadLog>().push(ReloadReport {
        seq: 0,
        generation: live + 1,
        status: ReloadStatus::Failed,
        errors,
        migrations: vec![],
        duration_ms: took.as_secs_f64() * 1000.0,
    })
}

type Applied = (Vec<Migration>, Vec<(ScriptKey, StartupDecl)>);

fn load_and_apply(world: &mut World, rt: &mut ScriptRuntime, number: u64, set: ModuleSet) -> Result<Applied, Vec<ScriptError>> {
    let LoadedGeneration { manifest, sections } = rt.backend.load(&Generation { number, set })?;

    let fail = |rt: &mut ScriptRuntime, errors: Vec<ScriptError>| {
        rt.backend.discard(number);
        Err(errors)
    };

    // Declarations, checked whole before anything moves.
    let mut errors = components::validate(&manifest.components, world.resource::<NativeLayouts>());
    let handlers = world.resource::<SectionHandlers>().clone();
    for s in &sections {
        if !handlers.0.contains_key(&s.name) {
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
    for s in &manifest.startups {
        if !seen.insert(s.name.as_str()) {
            errors.push(ScriptError::at(format!("startup {} is declared twice", s.name), s.location.clone()));
        }
    }
    if !errors.is_empty() {
        return fail(rt, errors);
    }

    let pending = components::prepare(world, &manifest.components);
    let built = {
        let lookup = |name: &str| pending.lookup(name).map(|(id, l)| (id, l.size));
        systems::build(world, number, &manifest.systems, &lookup)
    };
    let systems = match built {
        Ok(s) => s,
        Err(errors) => {
            components::abandon(world, pending);
            return fail(rt, errors);
        }
    };

    // Plugin sections, with the pending components already resolvable.
    for s in &sections {
        let handler = handlers.0[&s.name].clone();
        let input = SectionInput { generation: number, bytes: &s.bytes, location: s.location.clone(), components: &pending };
        if let Err(errors) = handler(world, &input) {
            components::abandon(world, pending);
            return fail(rt, errors);
        }
    }

    let mut resolution = Resolution { generation: number, ..Default::default() };
    for (name, id, layout) in pending.iter() {
        resolution.components.insert(name.to_string(), ResolvedComponent { id: id.index() as u32, layout: layout.clone() });
    }
    for n in world.resource::<NativeLayouts>().iter() {
        resolution.components.insert(n.ts_name.clone(), ResolvedComponent { id: n.id.index() as u32, layout: n.layout.clone() });
    }
    for (name, op) in kinds.all() {
        resolution.command_kinds.insert(name.to_string(), op);
    }
    if let Err(errors) = rt.backend.commit(number, &resolution) {
        components::abandon(world, pending);
        return fail(rt, errors);
    }

    // Nothing can fail from here.
    let migrations = components::commit(world, pending);
    world.insert_resource(systems);
    rt.entries = manifest.entries.iter().map(|e| e.name.clone()).collect();
    let startups = manifest
        .startups
        .iter()
        .enumerate()
        .map(|(i, s)| (ScriptKey { generation: number, index: i as u32 }, s.clone()))
        .collect();
    Ok((migrations, startups))
}

fn run_startups(world: &mut World, rt: &mut ScriptRuntime, startups: Vec<(ScriptKey, StartupDecl)>) {
    for (key, decl) in startups {
        if !world.resource_mut::<Generations>().startups_run.insert(decl.name.clone()) {
            continue;
        }
        let tick = world.increment_change_tick();
        let frame = frame_info(world, tick.get());
        rt.fill_pool(world);
        let mut scratch = std::mem::take(&mut rt.scratch);
        scratch.arena_mut().resize(commands::ARENA_BYTES, 0);
        let pool = std::mem::take(&mut rt.pool);
        let result = {
            let mut ctx = CallContext { frame, arena: scratch.arena_mut(), pool: &pool };
            rt.backend.run_startup(key, &mut ctx)
        };
        rt.pool = pool;
        match result {
            Ok(out) => rt.apply_output(world, scratch.arena_mut(), out, &format!("startup {}", decl.name)),
            Err(e) => rt.log_error(world, &format!("startup {} threw", decl.name), &e),
        }
        rt.scratch = scratch;
    }
    rt.free_pool(world);
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

/// The live generation's TS components, for code that wants them all.
pub fn live_components(world: &World) -> Vec<(String, bevy::ecs::component::ComponentId)> {
    world
        .resource::<TsComponents>()
        .current()
        .filter(|c| c.state == components::ComponentState::Live)
        .map(|c| (c.decl.name.clone(), c.id))
        .collect()
}
