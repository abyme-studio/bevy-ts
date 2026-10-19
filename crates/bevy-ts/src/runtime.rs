//! The script runtime: the backend as non-send data on the main thread, the entity pool, the
//! command arena, and the helpers every call into JS shares.

use crate::backend::{CallContext, FrameInfo, RunOutput, ScriptBackend, ScriptError, SourceLocation};
use crate::commands::{self, CommandKinds, NativeRecipes, POOL_SIZE};
use crate::components::TsComponents;
use crate::layout::NativeLayouts;
use crate::log::{LogLevel, ScriptLog};
use crate::systems::Scratch;
use bevy::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;

/// The JS engine and what lives beside it. Non-send: the isolate is pinned to the main thread.
pub struct ScriptRuntime {
    pub backend: Box<dyn ScriptBackend>,
    /// Dev mode: hot reload, the read-column check, the inspector.
    pub dev: bool,
    /// Entity ids pre-allocated for `spawn`, `Entity::to_bits`.
    pub(crate) pool: Vec<u64>,
    pub(crate) scratch: Scratch,
    /// Live generation entry names.
    pub(crate) entries: HashSet<String>,
}

impl ScriptRuntime {
    pub fn new(backend: Box<dyn ScriptBackend>, dev: bool) -> Self {
        ScriptRuntime { backend, dev, pool: Vec::new(), scratch: Scratch::default(), entries: HashSet::new() }
    }

    /// Tops the pool up to [`POOL_SIZE`] ids from `EntityAllocator::alloc`.
    pub(crate) fn fill_pool(&mut self, world: &World) {
        let alloc = world.entity_allocator();
        while self.pool.len() < POOL_SIZE {
            self.pool.push(alloc.alloc().to_bits());
        }
    }

    /// Frees pool ids nobody spawned; called after each stage.
    pub(crate) fn free_pool(&mut self, world: &mut World) {
        let freed: Vec<Entity> = self.pool.drain(..).map(Entity::from_bits).collect();
        world.entity_allocator_mut().free_many(&freed);
    }

    /// Decodes and applies a call's commands, all or nothing. Spawned pool ids leave the pool
    /// only when their commands apply.
    pub(crate) fn apply_output(&mut self, world: &mut World, arena: &[u8], out: RunOutput, label: &str) {
        let mut bytes = arena[..out.arena_len].to_vec();
        bytes.extend_from_slice(&out.overflow);
        if bytes.is_empty() {
            self.pool.drain(..out.pool_used.min(self.pool.len()));
            return;
        }
        let sizes = world.resource::<TsComponents>().writable_sizes(world.resource::<NativeLayouts>());
        let kinds = world.resource::<CommandKinds>().clone();
        let recipes = world.resource::<NativeRecipes>().clone();
        match commands::decode(&bytes, &sizes, &kinds) {
            Ok(ops) => {
                self.pool.drain(..out.pool_used.min(self.pool.len()));
                for w in commands::apply(world, ops, &kinds, &recipes) {
                    self.log_warn(world, format!("{label}: {w}"), None);
                }
            }
            Err(e) => {
                self.log_error(world, &format!("{label}: commands discarded"), &ScriptError::new(e));
            }
        }
    }

    pub(crate) fn log_error(&self, world: &mut World, context: &str, e: &ScriptError) {
        let generation = current_generation(world);
        world.resource_mut::<ScriptLog>().error(context, e, generation);
    }

    pub(crate) fn log_warn(&self, world: &mut World, message: String, location: Option<SourceLocation>) {
        let generation = current_generation(world);
        world.resource_mut::<ScriptLog>().push_deduped(LogLevel::Warn, message, location, generation);
    }

    /// Calls a named entry of the live generation from an exclusive system: the payload goes in,
    /// the entry's bytes come back, and its commands are applied like a system's. On a throw the
    /// commands are discarded and the error, mapped to TS, is returned and logged.
    pub fn call_entry(world: &mut World, name: &str, payload: &[u8]) -> Result<Vec<u8>, ScriptError> {
        let mut rt = world
            .remove_non_send::<ScriptRuntime>()
            .ok_or_else(|| ScriptError::new("bevy-ts runtime is busy or missing; call entries from an exclusive system outside a JS call"))?;
        if !rt.entries.contains(name) {
            world.insert_non_send(rt);
            return Err(ScriptError::new(format!("no entry named {name} in the live generation")));
        }
        let tick = world.increment_change_tick();
        let frame = frame_info(world, tick.get());
        rt.fill_pool(world);
        let mut scratch = std::mem::take(&mut rt.scratch);
        scratch.arena_mut().resize(commands::ARENA_BYTES, 0);
        let result = {
            let pool = std::mem::take(&mut rt.pool);
            let mut ctx = CallContext { frame, arena: scratch.arena_mut(), pool: &pool };
            let r = rt.backend.call_entry(name, payload, &mut ctx);
            rt.pool = pool;
            r
        };
        let out = match result {
            Ok((bytes, out)) => {
                rt.apply_output(world, scratch.arena_mut(), out, &format!("entry {name}"));
                Ok(bytes)
            }
            Err(e) => {
                rt.log_error(world, &format!("entry {name} threw"), &e);
                Err(e)
            }
        };
        rt.scratch = scratch;
        rt.free_pool(world);
        world.insert_non_send(rt);
        out
    }
}

/// Facts about the runtime other crates read (the protocol's handshake, plugins).
#[derive(Resource, Clone, Debug, Default)]
pub struct RuntimeInfo {
    /// The live generation, 0 before the first applies.
    pub generation: u64,
    pub entry: Option<PathBuf>,
    pub dev: bool,
    pub inspector: Option<String>,
    pub engine: String,
}

pub fn current_generation(world: &World) -> u64 {
    world.get_resource::<RuntimeInfo>().map_or(0, |i| i.generation)
}

/// A frame counter bevy-ts keeps itself, advanced in `First`.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ScriptFrame(pub u64);

/// Pause and single-frame stepping for the bevy-ts stages and `Time<Virtual>`.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ScriptClock {
    pub paused: bool,
    /// Frames still to run while paused.
    pub step_frames: u32,
    /// Whether this frame runs the stages (set in `First`).
    pub running_this_frame: bool,
}

pub(crate) fn stage_should_run(world: &World) -> bool {
    world.get_resource::<ScriptClock>().is_none_or(|c| c.running_this_frame)
}

/// `First`, before time advances: decides whether this frame runs, and pauses or unpauses
/// `Time<Virtual>` so a stepped frame advances it by exactly one frame.
pub fn advance_clock(mut clock: ResMut<ScriptClock>, mut frame: ResMut<ScriptFrame>, time: Option<ResMut<Time<Virtual>>>) {
    frame.0 += 1;
    let run = if !clock.paused {
        true
    } else if clock.step_frames > 0 {
        clock.step_frames -= 1;
        true
    } else {
        false
    };
    clock.running_this_frame = run;
    if let Some(mut time) = time {
        if run && time.is_paused() {
            time.unpause();
        } else if !run && !time.is_paused() {
            time.pause();
        }
    }
}

pub(crate) fn frame_info(world: &World, tick: u32) -> FrameInfo {
    let (dt, elapsed) = world.get_resource::<Time>().map_or((0.0, 0.0), |t| (t.delta_secs_f64(), t.elapsed_secs_f64()));
    let frame = world.get_resource::<ScriptFrame>().map_or(0, |f| f.0);
    FrameInfo { dt, elapsed, tick, frame }
}

/// `First`: pumps the backend (microtasks, timers, inspector) and logs `console.*` output.
pub fn pump(world: &mut World) {
    let Some(mut rt) = world.remove_non_send::<ScriptRuntime>() else { return };
    let messages = rt.backend.pump();
    let generation = current_generation(world);
    let mut log = world.resource_mut::<ScriptLog>();
    for m in messages {
        log.push(m.level, m.message, m.location, generation);
    }
    world.insert_non_send(rt);
}

impl Scratch {
    pub(crate) fn arena_mut(&mut self) -> &mut Vec<u8> {
        &mut self.arena
    }
}
