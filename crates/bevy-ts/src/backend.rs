//! The engine-neutral seam. The core drives a JS engine only through [`ScriptBackend`]; a
//! backend sees pointers, lengths, layouts and a command arena, never the `World`.

use crate::manifest::{LoadedGeneration, Resolution};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// One compiled module of a generation.
#[derive(Clone, Debug)]
pub struct CompiledModule {
    /// The TS source path.
    pub path: PathBuf,
    /// Compiled JS.
    pub code: String,
    /// Source map JSON, mapping `code` back to the TS source.
    pub source_map: Option<String>,
}

/// A compiled module graph, waiting to become a generation.
#[derive(Clone, Debug)]
pub struct ModuleSet {
    /// The entry module's path; it must be one of `modules`.
    pub entry: PathBuf,
    pub modules: Vec<CompiledModule>,
}

/// A module set with its generation number.
#[derive(Clone, Debug)]
pub struct Generation {
    pub number: u64,
    pub set: ModuleSet,
}

/// A location in TS source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// An error from compiling, loading, declaring or running TS, located in TS source when known.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    /// The stack mapped to TS, when the error was thrown by JS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

impl ScriptError {
    pub fn new(message: impl Into<String>) -> Self {
        ScriptError { message: message.into(), location: None, stack: None }
    }

    pub fn at(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        ScriptError { message: message.into(), location, stack: None }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(loc) => write!(f, "{loc}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Identifies a declared system or startup within a generation, by its index in the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScriptKey {
    pub generation: u64,
    pub index: u32,
}

/// Per-run scalars every call into JS carries.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub dt: f64,
    pub elapsed: f64,
    /// This run's change tick, from `World::increment_change_tick`.
    pub tick: u32,
    /// The frame counter.
    pub frame: u64,
}

/// The command arena and entity pool a call may use. Both are owned by Rust and never grow
/// during the call; overflow stays in JS and comes back in [`RunOutput::overflow`].
pub struct CallContext<'a> {
    pub frame: FrameInfo,
    /// The arena's bytes; JS writes from 0 and reports the used length.
    pub arena: &'a mut [u8],
    /// Pre-allocated entity ids (`Entity::to_bits`) `spawn` hands out, in order.
    pub pool: &'a [u64],
    /// Host functions JS may call synchronously during this call (the sixth seam operation).
    pub host: Option<&'a mut dyn HostCalls>,
}

/// Call host function: Rust functions JS calls synchronously during a system run or an entry
/// call, one per exported method. Arguments arrive as little-endian `f64` slots (`args`),
/// results go into `ret` (also slots), and slices are views over JS memory passed in place. A
/// backend makes it reachable from JS for exactly the duration of the call into JS: on V8 a fast
/// op, on QuickJS-ng a host function, on the web a wasm import.
pub trait HostCalls {
    /// Calls host function `id`; returns the result's slot count, or an error message JS throws.
    fn call(&mut self, id: u32, args: &[u8], ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String>;
}

/// One component column of one table, as raw memory valid for the length of one run.
#[derive(Clone, Copy, Debug)]
pub struct ColumnFrame {
    /// The column's data, `len × size` bytes.
    pub data: *mut u8,
    pub bytes: usize,
    /// The column's changed ticks, `len` `u32`s, for `write` components only.
    pub ticks: Option<*mut u32>,
}

/// One matched table: its row count, its entities as (index, generation) pairs, and one
/// [`ColumnFrame`] per data component of the query, in the query's order.
#[derive(Clone, Debug)]
pub struct TableFrame {
    pub len: usize,
    /// `len × 2` `u32`s.
    pub entities: *const u32,
    /// A column the table lacks (an `Option<C>` the row does not have) has a null `data`.
    pub columns: Vec<ColumnFrame>,
    /// For a query with `Changed`/`Added` filters: the rows that pass, `(ptr, count)`.
    pub rows: Option<(*const u32, usize)>,
}

/// One system param's memory for a run. Params are flattened in declaration order, bundles
/// depth-first; params with nothing to hand over (`Commands`, writers, `NextState`) are `None`.
#[derive(Clone, Debug)]
pub enum ParamFrame {
    Query { tables: Vec<TableFrame> },
    /// One row: a resource, a `Local`, or a computed view such as `Time`. `ticks` points at the
    /// row's changed tick when the param writes it.
    Row { data: *mut u8, bytes: usize, ticks: Option<*mut u32> },
    /// Unread messages, oldest first, in at most two contiguous runs of `stride`-byte records.
    Messages { parts: [(*const u8, usize); 2], stride: usize },
    State { value: u32 },
    /// An observer's trigger: the event's bytes and its current target (`u64::MAX` for none).
    Event { data: *const u8, bytes: usize, entity: u64 },
    None,
}

/// What a system run sees.
///
/// # Safety contract
/// Every pointer is valid for reads (and writes, for written columns, rows and ticks) for exactly
/// the duration of [`ScriptBackend::run_system`]. A backend must not retain any of them past the
/// call: views it hands to JS must be detached before it returns, whether `run` returned or
/// threw.
pub struct SystemRun<'a> {
    pub ctx: CallContext<'a>,
    pub params: &'a [ParamFrame],
}

/// What came back from a call into JS.
#[derive(Clone, Debug, Default)]
pub struct RunOutput {
    /// Bytes of the arena JS wrote.
    pub arena_len: usize,
    /// Command bytes that did not fit the arena, to apply after `arena[..arena_len]`.
    pub overflow: Vec<u8>,
    /// How many ids of [`CallContext::pool`] JS handed out, from the front.
    pub pool_used: usize,
    /// `(param, table, column)` triples taken through `table.write(T)`: every row is marked.
    pub write_marks: Vec<(u32, u32, u32)>,
    /// An observer called `propagate(false)`.
    pub stop_propagation: bool,
}

/// A `console.*` call or other runtime message.
#[derive(Clone, Debug)]
pub struct ConsoleMessage {
    pub level: crate::log::LogLevel,
    pub message: String,
    pub location: Option<SourceLocation>,
}

/// The JS engine behind the bridge.
pub trait ScriptBackend: 'static {
    /// Engine name and version, for `bevy-ts/hello`.
    fn engine(&self) -> String;

    /// Loads a module set as a new generation: evaluates it and collects its declarations and
    /// manifest sections. Nothing becomes live until [`ScriptBackend::commit`].
    fn load(&mut self, generation: &Generation) -> Result<LoadedGeneration, Vec<ScriptError>>;

    /// Makes a loaded generation live with the ids and layouts Rust resolved, replacing the
    /// previous one. The previous generation's systems, startups and entries are dropped.
    fn commit(&mut self, generation: u64, resolution: &Resolution) -> Result<(), Vec<ScriptError>>;

    /// Drops a loaded generation that will not be committed.
    fn discard(&mut self, generation: u64);

    /// Runs one declared system or observer: exactly one call into JS.
    fn run_system(&mut self, key: ScriptKey, run: &mut SystemRun<'_>) -> Result<RunOutput, ScriptError>;

    /// Calls a named entry of the live generation with a byte payload.
    fn call_entry(
        &mut self,
        name: &str,
        payload: &[u8],
        ctx: &mut CallContext<'_>,
    ) -> Result<(Vec<u8>, RunOutput), ScriptError>;

    /// Runs pending engine work (microtasks, timers, inspector messages) and returns
    /// `console.*` output gathered since the last pump.
    fn pump(&mut self) -> Vec<ConsoleMessage>;

    /// Replaces the engine instance with a fresh one; every loaded generation is gone.
    fn restart(&mut self);

    /// Calls made into JS so far (system runs, startups, entries, loads), for tests and stats.
    fn calls_into_js(&self) -> u64;

    /// Where a debugger can attach, when the engine serves one.
    fn inspector_address(&self) -> Option<String> {
        None
    }

    /// JS heap in use, in bytes, when the engine can tell.
    fn heap_used(&self) -> Option<usize> {
        None
    }
}
