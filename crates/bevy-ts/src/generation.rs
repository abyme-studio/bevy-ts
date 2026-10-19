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
    crate::layout_reflect::fill_once(world);
    // Flush the previous generation's console output so the log reads in order.
    crate::runtime::pump(world);
    let number = world.resource::<Generations>().live + 1;
    let mut rt = world.remove_non_send::<ScriptRuntime>().expect("bevy-ts runtime present");

    if restart {
        let spawned: Vec<Entity> = world.query_filtered::<Entity, With<TsSpawned>>().iter(world).collect();
        for e in spawned {
            // Children may already be gone with their parent.
            let _ = world.try_despawn(e);
        }
        world.resource_mut::<Generations>().startups_run.clear();
        world.insert_resource(crate::memory::SystemMemory::default());
        world.resource_mut::<crate::messages::TsPayloads>().reset();
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
        Ok((migrations, sections)) => {
            world.resource_mut::<Generations>().live = number;
            world.resource_mut::<RuntimeInfo>().generation = number;
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
            run_startups(world, &mut rt);
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

type Applied = (Vec<Migration>, Vec<crate::manifest::Section>);

/// Resources are components on their own resource entity: one declaration list for both.
fn component_decls(manifest: &crate::manifest::Manifest) -> Vec<crate::manifest::ComponentDecl> {
    let mut all = manifest.components.clone();
    all.extend(manifest.resources.iter().map(|r| crate::manifest::ComponentDecl {
        name: r.name.clone(),
        fields: r.fields.clone(),
        location: r.location.clone(),
    }));
    all
}

fn load_and_apply(world: &mut World, rt: &mut ScriptRuntime, number: u64, set: ModuleSet) -> Result<Applied, Vec<ScriptError>> {
    let LoadedGeneration { manifest, sections } = rt.backend.load(&Generation { number, set })?;

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
        }
    }
    for st in &manifest.states {
        if let Some(v) = &st.initial
            && !st.values.contains(v)
        {
            errors.push(ScriptError::at(format!("{} has no value {v}", st.name), st.location.clone()));
        }
    }
    if !errors.is_empty() {
        return fail(rt, errors);
    }

    // Payloads first (systems resolve message, event and state indices), kept aside so a failed
    // generation leaves the old ones untouched.
    let old_payloads = world.remove_resource::<crate::messages::TsPayloads>().unwrap_or_default();
    let mut payloads = old_payloads.clone();
    payloads.declare(&manifest.messages, &manifest.events, &manifest.states);
    world.insert_resource(payloads);
    let restore = |world: &mut World, old: crate::messages::TsPayloads| {
        world.insert_resource(old);
    };

    let pending = components::prepare(world, &decls);
    let built = {
        let lookup = |name: &str| pending.lookup(name).map(|(id, l)| (id, l.size));
        systems::build(world, number, &manifest.systems, &lookup)
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
        let input = SectionInput { generation: number, bytes: &s.bytes, location: s.location.clone(), components: &pending };
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
        resolution.components.insert(name.to_string(), ResolvedComponent { id: id.index() as u32, layout: layout.clone() });
    }
    for n in world.resource::<NativeLayouts>().iter() {
        let r = ResolvedComponent { id: n.id.index() as u32, layout: n.layout.clone() };
        resolution.components.insert(n.type_path.clone(), r.clone());
        resolution.components.insert(n.ts_name.clone(), r);
    }
    resolution.components.insert(systems::TIME.into(), ResolvedComponent { id: u32::MAX, layout: systems::time_layout() });
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
    world.insert_resource(systems);
    crate::sets::ensure_slots(world);
    crate::observers::install(world, &watched);
    rt.entries = manifest.entries.iter().map(|e| e.name.clone()).collect();
    Ok((migrations, sections))
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
        // SAFETY: `c.id` is a TS component with `c.layout`; `bytes` has its size.
        unsafe { commands::insert_bytes(&mut e, c.id, &bytes) };
    }
}

/// Runs `Startup` systems whose identity has not run in this world.
fn run_startups(world: &mut World, rt: &mut ScriptRuntime) {
    let Some(mut systems) = world.remove_resource::<TsSystems>() else { return };
    let mut list = std::mem::take(&mut systems.startup);
    let mut due = Vec::new();
    let mut rest = Vec::new();
    for s in list.drain(..) {
        if world.resource_mut::<Generations>().startups_run.insert(s.decl.name.clone()) {
            due.push(s);
        } else {
            rest.push(s);
        }
    }
    systems::run_list(world, rt, &mut systems, &mut due);
    rest.extend(due);
    systems.startup = rest;
    world.insert_resource(systems);
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
