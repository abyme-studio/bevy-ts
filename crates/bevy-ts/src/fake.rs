//! An in-process backend with no JS engine: "modules" are Rust closures queued by a test. It
//! implements the same seam as V8, so the core and feature crates test without V8.

use crate::backend::{
    CallContext, ConsoleMessage, FrameInfo, Generation, ParamFrame, RunOutput, ScriptBackend, ScriptError, ScriptKey, SystemRun,
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

/// What a fake system, observer or entry gets.
pub struct FakeCall<'a> {
    pub frame: FrameInfo,
    /// The tables of the first query param (the common case in tests).
    pub tables: Vec<FakeTable<'a>>,
    /// Every query param's tables, in param order.
    pub queries: Vec<Vec<FakeTable<'a>>>,
    /// Every one-row param (resource, `Local`, `Time`), in param order.
    pub rows: Vec<&'a mut [u8]>,
    /// An observer's event bytes and target bits.
    pub event: Option<(&'a [u8], u64)>,
    pub stop_propagation: bool,
    pub commands: FakeCommands<'a>,
    pub resolution: &'a Resolution,
    /// `(param, table, column)` triples to mark wholly changed, as `table.write(T)` does.
    pub write_marks: Vec<(u32, u32, u32)>,
    /// An entry's payload.
    pub payload: &'a [u8],
    /// An entry's return bytes.
    pub output: Vec<u8>,
    /// The call's host functions, as JS reaches them through the seam; valid for the call only.
    host: Option<*mut (dyn crate::backend::HostCalls + 'static)>,
}

impl FakeCall<'_> {
    /// Calls host function `id` with `args` as slots; returns the result slots.
    pub fn host_call(&mut self, id: u32, args: &[f64], slices: &mut [&mut [u8]]) -> Result<Vec<f64>, ScriptError> {
        let host = self.host.ok_or_else(|| ScriptError::new("fake: no host functions in this call"))?;
        // SAFETY: the pointer is the call context's host, alive for the whole call this
        // `FakeCall` exists in.
        let host = unsafe { &mut *host };
        let bytes: Vec<u8> = args.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut ret = vec![0u8; crate::host::SLOTS * 8];
        let n = host.call(id, &bytes, &mut ret, slices).map_err(ScriptError::new)?;
        Ok(ret[..n * 8].chunks(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect())
    }

    /// Calls JSON host service `name` (`assets.add`) with `args`, as the runtime's `assets` does.
    pub fn host_json(&mut self, name: &str, args: serde_json::Value) -> Result<serde_json::Value, ScriptError> {
        let host = self.host.ok_or_else(|| ScriptError::new("fake: no host functions in this call"))?;
        // SAFETY: as in `host_call`.
        let host = unsafe { &mut *host };
        let out = host.call_json(name, &args.to_string()).map_err(ScriptError::new)?;
        serde_json::from_str(&out).map_err(|e| ScriptError::new(format!("{name}: {e}")))
    }
}

pub type FakeFn = Box<dyn FnMut(&mut FakeCall) -> Result<(), ScriptError> + Send>;

/// A generation's worth of fake modules.
#[derive(Default)]
pub struct FakeProgram {
    pub manifest: Manifest,
    pub sections: Vec<Section>,
    /// One per `manifest.systems` (observers and `Startup` systems included), by index.
    pub systems: Vec<FakeFn>,
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
    /// Requests the next delivery (or load) hands core, as scripts would have made them.
    requests: Vec<crate::net::NetRequest>,
    /// The fake script side's earliest timer.
    next_timer: Option<f64>,
    /// Deliveries that called "JS": `(now_ms, events)`.
    deliveries: Vec<(f64, Vec<crate::net::NetEvent>)>,
    /// The environment each load saw.
    envs: Vec<std::collections::BTreeMap<String, String>>,
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

    /// Queues requests the next delivery or load returns, as if scripts had made them.
    pub fn request(&self, r: crate::net::NetRequest) {
        self.0.lock().unwrap().requests.push(r);
    }

    /// Sets the fake script side's earliest timer: a delivery at or after it calls "JS".
    pub fn set_timer(&self, at: Option<f64>) {
        self.0.lock().unwrap().next_timer = at;
    }

    /// Deliveries that called "JS", oldest first.
    pub fn deliveries(&self) -> Vec<(f64, Vec<crate::net::NetEvent>)> {
        self.0.lock().unwrap().deliveries.clone()
    }

    /// The environment each load saw, oldest first.
    pub fn envs(&self) -> Vec<std::collections::BTreeMap<String, String>> {
        self.0.lock().unwrap().envs.clone()
    }

    pub fn backend(&self) -> FakeBackend {
        FakeBackend(self.clone())
    }
}

pub struct FakeBackend(FakeHandle);

impl FakeBackend {
    #[allow(clippy::too_many_arguments)]
    fn call<'a>(
        &mut self,
        f: impl FnOnce(&mut State) -> Option<(*mut FakeFn, Resolution)>,
        frame: FrameInfo,
        mut queries: Vec<Vec<FakeTable<'a>>>,
        rows: Vec<&'a mut [u8]>,
        event: Option<(&'a [u8], u64)>,
        ctx_pool: &[u64],
        payload: &'a [u8],
        arena: &mut [u8],
        host: Option<&mut dyn crate::backend::HostCalls>,
    ) -> Result<(Vec<u8>, RunOutput), ScriptError> {
        // SAFETY: only the lifetime is erased; the pointer is used during this call only.
        let host = host.map(|h| unsafe {
            std::mem::transmute::<*mut dyn crate::backend::HostCalls, *mut (dyn crate::backend::HostCalls + 'static)>(h)
        });
        let (func, resolution) = {
            let mut s = self.0.0.lock().unwrap();
            s.calls += 1;
            f(&mut s).ok_or_else(|| ScriptError::new("fake: nothing to call"))?
        };
        let tables = if queries.is_empty() { Vec::new() } else { queries.remove(0) };
        let mut call = FakeCall {
            frame,
            tables,
            queries,
            rows,
            event,
            stop_propagation: false,
            commands: FakeCommands { encoder: Encoder::default(), pool: ctx_pool, used: 0 },
            resolution: &resolution,
            write_marks: Vec::new(),
            payload,
            output: Vec::new(),
            host,
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
                stop_propagation: call.stop_propagation,
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
        s.envs.push(generation.env.clone());
        let mut program = s.queue.pop_front().ok_or_else(|| vec![ScriptError::new("fake: no program queued")])?;
        if let Some(errors) = program.load_error.take() {
            s.requests.retain(|r| r.generation() != generation.number);
            return Err(errors);
        }
        let mut requests = std::mem::take(&mut s.requests);
        for r in &mut requests {
            set_generation(r, generation.number);
        }
        let loaded = LoadedGeneration { manifest: program.manifest.clone(), sections: program.sections.clone(), requests };
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
        let mut queries = Vec::new();
        let mut rows = Vec::new();
        let mut event = None;
        for p in run.params {
            match p {
                ParamFrame::Query { tables, .. } => queries.push(
                    tables
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
                                    if c.data.is_null() {
                                        return (&mut [][..], None);
                                    }
                                    // SAFETY: the seam's contract: valid for this call only, and
                                    // these slices do not outlive it.
                                    let data = unsafe { std::slice::from_raw_parts_mut(c.data, c.bytes) };
                                    let ticks = c.ticks.map(|p| unsafe { std::slice::from_raw_parts_mut(p, t.len) });
                                    (data, ticks)
                                })
                                .collect(),
                        })
                        .collect(),
                ),
                // SAFETY: as above.
                ParamFrame::Row { data, bytes, .. } => rows.push(unsafe { std::slice::from_raw_parts_mut(*data, *bytes) }),
                ParamFrame::Event { data, bytes, entity, .. } => event = Some((unsafe { std::slice::from_raw_parts(*data, *bytes) }, *entity)),
                _ => {}
            }
        }
        let frame = run.ctx.frame;
        let pool = run.ctx.pool;
        let host = run.ctx.host.as_mut().map(|h| &mut **h as &mut dyn crate::backend::HostCalls);
        let (_, out) = self.call(
            |s| {
                let (p, r) = s.loaded.get_mut(&key.generation)?;
                Some((p.systems.get_mut(key.index as usize)? as *mut FakeFn, r.clone()?))
            },
            frame,
            queries,
            rows,
            event,
            pool,
            &[],
            run.ctx.arena,
            host,
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
            Vec::new(),
            None,
            ctx.pool,
            payload,
            ctx.arena,
            ctx.host.as_mut().map(|h| &mut **h as &mut dyn crate::backend::HostCalls),
        )
    }

    fn deliver(&mut self, now_ms: f64, events: &[crate::net::NetEvent]) -> Result<crate::backend::Delivered, ScriptError> {
        let mut s = self.0.0.lock().unwrap();
        let due = s.next_timer.is_some_and(|t| now_ms >= t);
        if !events.is_empty() || due {
            s.calls += 1;
            if due {
                s.next_timer = None;
            }
            s.deliveries.push((now_ms, events.to_vec()));
        }
        Ok(crate::backend::Delivered { requests: std::mem::take(&mut s.requests), next_timer_ms: s.next_timer })
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

/// A request made during a load belongs to the loading generation.
fn set_generation(r: &mut crate::net::NetRequest, number: u64) {
    use crate::net::NetRequest;
    match r {
        NetRequest::Open { generation, .. }
        | NetRequest::Send { generation, .. }
        | NetRequest::Close { generation, .. }
        | NetRequest::Fetch { generation, .. }
        | NetRequest::Abort { generation, .. } => *generation = number,
    }
}
