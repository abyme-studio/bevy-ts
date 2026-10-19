//! An in-process backend with no JS engine: "modules" are Rust closures queued by a test. It
//! implements the same seam as V8, so the core and feature crates test without V8.

use crate::backend::{
    CallContext, ConsoleMessage, FrameInfo, Generation, RunOutput, ScriptBackend, ScriptError, ScriptKey, SystemRun,
};
use crate::commands::Encoder;
use crate::manifest::{LoadedGeneration, Manifest, Resolution, Section};
use bevy::prelude::Entity;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// One table as a fake system sees it: slices over the real column memory.
pub struct FakeTable<'a> {
    pub len: usize,
    pub entities: Vec<Entity>,
    /// Per column (read first, then write): data bytes and, for `write`, the tick slice.
    pub columns: Vec<(&'a mut [u8], Option<&'a mut [u32]>)>,
}

impl FakeTable<'_> {
    pub fn read_f32(&self, column: usize, row: usize, size: usize, offset: usize) -> f32 {
        let at = row * size + offset;
        f32::from_le_bytes(self.columns[column].0[at..at + 4].try_into().unwrap())
    }

    /// Writes an `f32` and stamps the row's tick, as a generated setter does.
    pub fn write_f32(&mut self, column: usize, row: usize, size: usize, offset: usize, v: f32, tick: u32) {
        let at = row * size + offset;
        let (data, ticks) = &mut self.columns[column];
        data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        if let Some(t) = ticks {
            t[row] = tick;
        }
    }
}

/// Commands a fake call queues, with `spawn` drawing from the Rust-filled pool.
pub struct FakeCommands<'a> {
    pub encoder: Encoder,
    pool: &'a [u64],
    used: usize,
}

impl FakeCommands<'_> {
    pub fn alloc(&mut self) -> Result<Entity, ScriptError> {
        let bits = *self.pool.get(self.used).ok_or_else(|| ScriptError::new("spawn pool exhausted"))?;
        self.used += 1;
        Ok(Entity::from_bits(bits))
    }

    pub fn spawn(&mut self, components: &[(bevy::ecs::component::ComponentId, &[u8])]) -> Result<Entity, ScriptError> {
        let e = self.alloc()?;
        self.encoder.spawn(e, components);
        Ok(e)
    }
}

/// What a fake system, startup or entry gets.
pub struct FakeCall<'a> {
    pub frame: FrameInfo,
    pub tables: Vec<FakeTable<'a>>,
    pub commands: FakeCommands<'a>,
    pub resolution: &'a Resolution,
    /// `(table, column)` pairs to mark wholly changed, as `table.write(T)` does.
    pub write_marks: Vec<(u32, u32)>,
    /// An entry's payload.
    pub payload: &'a [u8],
    /// An entry's return bytes.
    pub output: Vec<u8>,
}

pub type FakeFn = Box<dyn FnMut(&mut FakeCall) -> Result<(), ScriptError> + Send>;

/// A generation's worth of fake modules.
#[derive(Default)]
pub struct FakeProgram {
    pub manifest: Manifest,
    pub sections: Vec<Section>,
    /// One per `manifest.systems`, by index.
    pub systems: Vec<FakeFn>,
    /// One per `manifest.startups`, by index.
    pub startups: Vec<FakeFn>,
    pub entries: HashMap<String, FakeFn>,
    /// Makes the load fail, as a syntax error or a throwing module would.
    pub load_error: Option<Vec<ScriptError>>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<FakeProgram>,
    loaded: HashMap<u64, (FakeProgram, Option<Resolution>)>,
    live: Option<u64>,
    calls: u64,
    entry_calls: HashMap<String, u64>,
    console: Vec<ConsoleMessage>,
    restarts: u64,
}

/// A handle tests keep to queue programs and read call counts.
#[derive(Clone, Default)]
pub struct FakeHandle(Arc<Mutex<State>>);

impl FakeHandle {
    /// Queues the program the next `load` evaluates.
    pub fn push(&self, program: FakeProgram) {
        self.0.lock().unwrap().queue.push_back(program);
    }

    /// Every call made into "JS" so far.
    pub fn calls(&self) -> u64 {
        self.0.lock().unwrap().calls
    }

    pub fn entry_calls(&self, name: &str) -> u64 {
        self.0.lock().unwrap().entry_calls.get(name).copied().unwrap_or(0)
    }

    pub fn restarts(&self) -> u64 {
        self.0.lock().unwrap().restarts
    }

    pub fn console(&self, message: ConsoleMessage) {
        self.0.lock().unwrap().console.push(message);
    }

    pub fn backend(&self) -> FakeBackend {
        FakeBackend(self.clone())
    }
}

pub struct FakeBackend(FakeHandle);

impl FakeBackend {
    fn call(
        &mut self,
        f: impl FnOnce(&mut State) -> Option<(*mut FakeFn, Resolution)>,
        frame: FrameInfo,
        tables: Vec<FakeTable<'_>>,
        ctx_pool: &[u64],
        payload: &[u8],
        arena: &mut [u8],
    ) -> Result<(Vec<u8>, RunOutput), ScriptError> {
        let (func, resolution) = {
            let mut s = self.0.0.lock().unwrap();
            s.calls += 1;
            f(&mut s).ok_or_else(|| ScriptError::new("fake: nothing to call"))?
        };
        let mut call = FakeCall {
            frame,
            tables,
            commands: FakeCommands { encoder: Encoder::default(), pool: ctx_pool, used: 0 },
            resolution: &resolution,
            write_marks: Vec::new(),
            payload,
            output: Vec::new(),
        };
        // SAFETY: the closure lives in `State::loaded`, which is only mutated by load, commit
        // and discard, never during a call (the backend is single-threaded on the main thread).
        unsafe { (*func)(&mut call) }?;
        let bytes = call.commands.encoder.bytes;
        let n = bytes.len().min(arena.len());
        arena[..n].copy_from_slice(&bytes[..n]);
        Ok((
            call.output,
            RunOutput {
                arena_len: n,
                overflow: bytes[n..].to_vec(),
                pool_used: call.commands.used,
                write_marks: call.write_marks,
            },
        ))
    }
}

impl ScriptBackend for FakeBackend {
    fn engine(&self) -> String {
        "fake".into()
    }

    fn load(&mut self, generation: &Generation) -> Result<LoadedGeneration, Vec<ScriptError>> {
        let mut s = self.0.0.lock().unwrap();
        s.calls += 1;
        let mut program = s.queue.pop_front().ok_or_else(|| vec![ScriptError::new("fake: no program queued")])?;
        if let Some(errors) = program.load_error.take() {
            return Err(errors);
        }
        let loaded = LoadedGeneration { manifest: program.manifest.clone(), sections: program.sections.clone() };
        s.loaded.insert(generation.number, (program, None));
        Ok(loaded)
    }

    fn commit(&mut self, generation: u64, resolution: &Resolution) -> Result<(), Vec<ScriptError>> {
        let mut s = self.0.0.lock().unwrap();
        let entry = s.loaded.get_mut(&generation).ok_or_else(|| vec![ScriptError::new("fake: generation not loaded")])?;
        entry.1 = Some(resolution.clone());
        if let Some(old) = s.live.replace(generation) {
            s.loaded.remove(&old);
        }
        Ok(())
    }

    fn discard(&mut self, generation: u64) {
        self.0.0.lock().unwrap().loaded.remove(&generation);
    }

    fn run_system(&mut self, key: ScriptKey, run: &mut SystemRun<'_>) -> Result<RunOutput, ScriptError> {
        let tables = run
            .tables
            .iter()
            .map(|t| FakeTable {
                len: t.len,
                // SAFETY: `entities` holds `len × 2` u32s for the duration of the call.
                entities: unsafe { std::slice::from_raw_parts(t.entities, t.len * 2) }
                    .chunks(2)
                    .map(|p| Entity::from_bits(p[0] as u64 | (p[1] as u64) << 32))
                    .collect(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| {
                        // SAFETY: the seam's contract: valid for this call only, and these
                        // slices do not outlive it.
                        let data = unsafe { std::slice::from_raw_parts_mut(c.data, c.bytes) };
                        let ticks = c.ticks.map(|p| unsafe { std::slice::from_raw_parts_mut(p, t.len) });
                        (data, ticks)
                    })
                    .collect(),
            })
            .collect();
        let frame = run.ctx.frame;
        let pool = run.ctx.pool;
        let (_, out) = self.call(
            |s| {
                let (p, r) = s.loaded.get_mut(&key.generation)?;
                Some((p.systems.get_mut(key.index as usize)? as *mut FakeFn, r.clone()?))
            },
            frame,
            tables,
            pool,
            &[],
            run.ctx.arena,
        )?;
        Ok(out)
    }

    fn run_startup(&mut self, key: ScriptKey, ctx: &mut CallContext<'_>) -> Result<RunOutput, ScriptError> {
        let (_, out) = self.call(
            |s| {
                let (p, r) = s.loaded.get_mut(&key.generation)?;
                Some((p.startups.get_mut(key.index as usize)? as *mut FakeFn, r.clone()?))
            },
            ctx.frame,
            Vec::new(),
            ctx.pool,
            &[],
            ctx.arena,
        )?;
        Ok(out)
    }

    fn call_entry(&mut self, name: &str, payload: &[u8], ctx: &mut CallContext<'_>) -> Result<(Vec<u8>, RunOutput), ScriptError> {
        self.call(
            |s| {
                *s.entry_calls.entry(name.to_string()).or_default() += 1;
                let live = s.live?;
                let (p, r) = s.loaded.get_mut(&live)?;
                Some((p.entries.get_mut(name)? as *mut FakeFn, r.clone()?))
            },
            ctx.frame,
            Vec::new(),
            ctx.pool,
            payload,
            ctx.arena,
        )
    }

    fn pump(&mut self) -> Vec<ConsoleMessage> {
        std::mem::take(&mut self.0.0.lock().unwrap().console)
    }

    fn restart(&mut self) {
        let mut s = self.0.0.lock().unwrap();
        s.loaded.clear();
        s.live = None;
        s.restarts += 1;
    }

    fn calls_into_js(&self) -> u64 {
        self.0.calls()
    }
}
