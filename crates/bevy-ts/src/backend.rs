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
    /// What `process.env` shows: the same for every load of one engine run.
    pub env: std::collections::BTreeMap<String, String>,
    /// What `process.argv` shows (the engine, the entry, the game's arguments): the same for
    /// every load of one engine run.
    pub argv: Vec<String>,
    /// Real time now, in the milliseconds `deliver` takes: timers a module sets while it
    /// evaluates measure from it, in a fresh engine instance or after edit mode too.
    pub now_ms: f64,
    /// The declarations (`file#name`) that live entity observers name, so the load rebuilds them.
    pub observed: Vec<String>,
}

/// `process.argv` in Node's shape: the engine executable's absolute path, the entry's absolute
/// path, then the game's arguments. The CLI sets it once when the engine starts; without it,
/// `process.argv` is empty.
#[derive(bevy::prelude::Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptArgs(pub Vec<String>);

impl ScriptArgs {
    /// The running executable, `entry` (made absolute against the working directory), then `args`.
    pub fn new(entry: &std::path::Path, args: &[String]) -> Self {
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let entry = std::path::absolute(entry).unwrap_or_else(|_| entry.to_path_buf());
        let mut argv = vec![exe, entry.to_string_lossy().into_owned()];
        argv.extend(args.iter().cloned());
        ScriptArgs(argv)
    }
}

/// What a delivery hands back to core.
#[derive(Clone, Debug, Default)]
pub struct Delivered {
    /// Network requests scripts made since the last drain, in order.
    pub requests: Vec<crate::net::NetRequest>,
    /// The earliest timer's deadline, in the milliseconds `deliver` takes, if any.
    pub next_timer_ms: Option<f64>,
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

/// A file as errors and the protocol show it: relative to the working directory when under it,
/// never a `file://` URL (decoded) or a generation tag (`?gen=3`). Other URLs (`bevy-ts:core`)
/// are shown as they are.
pub fn display_file(file: &str) -> String {
    display_file_from(file, std::env::current_dir().ok().as_deref())
}

/// [`display_file`] for a path.
pub fn display_path(path: &std::path::Path) -> String {
    display_file(&path.to_string_lossy())
}

/// [`display_file`] against a given working directory, for callers that resolve many files.
pub fn display_file_from(file: &str, cwd: Option<&std::path::Path>) -> String {
    let file = file.split('?').next().unwrap_or(file);
    let path = match file.strip_prefix("file://") {
        Some(rest) => percent_decode(rest.strip_prefix("localhost").unwrap_or(rest)),
        None if has_scheme(file) => return file.to_string(),
        None => file.to_string(),
    };
    match cwd.and_then(|cwd| std::path::Path::new(&path).strip_prefix(cwd).ok()) {
        Some(rel) => rel.display().to_string(),
        None => path,
    }
}

/// `scheme:` before any `/`, and not a Windows drive (`C:`).
fn has_scheme(s: &str) -> bool {
    let Some(i) = s.find(':') else { return false };
    let scheme = &s[..i];
    scheme.len() > 1 && scheme.starts_with(|c: char| c.is_ascii_alphabetic()) && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).and_then(|c| hex(*c)), b.get(i + 2).and_then(|c| hex(*c))) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn files_show_decoded_and_relative() {
        let cwd = Some(Path::new("/p"));
        assert_eq!(display_file_from("file:///p/src/my%20game.ts?gen=3", cwd), "src/my game.ts");
        assert_eq!(display_file_from("/p/src/glow.wgsl", cwd), "src/glow.wgsl");
        assert_eq!(display_file_from("/elsewhere/a.ts", cwd), "/elsewhere/a.ts");
        assert_eq!(display_file_from("bevy-ts:core", cwd), "bevy-ts:core");
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

    /// Calls the bridge's own JSON service `name` (`assets.add`): `args` is a JSON array of its
    /// arguments, the result is JSON. Only the engine's world answers; a host without one refuses.
    fn call_json(&mut self, name: &str, args: &str) -> Result<String, String> {
        let _ = args;
        Err(format!("{name} needs the engine's world"))
    }
}

/// One component column of one table, as raw memory valid for the length of one run.
#[derive(Clone, Copy, Debug)]
pub struct ColumnFrame {
    /// The column's data, `len × size` bytes.
    pub data: *mut u8,
    pub bytes: usize,
    /// The column's changed ticks, `len` `u32`s, for `write` and `Ref` components only.
    pub ticks: Option<*mut u32>,
    /// The column's added ticks, `len` `u32`s, for `Ref` components only.
    pub added: Option<*mut u32>,
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
    /// For a query with row filters (`Changed`, `Added`, `Spawned`): the rows that pass,
    /// `(ptr, count)`.
    pub rows: Option<(*const u32, usize)>,
    /// One bit per `Has<C>` datum, in order: whether this table has `C`.
    pub has: u32,
}

/// One system param's memory for a run. Params are flattened in declaration order, bundles
/// depth-first; params with nothing to hand over (`Commands`, writers, `NextState`) are `None`.
#[derive(Clone, Debug)]
pub enum ParamFrame {
    /// A query's tables, and the system's last-run tick (for `Ref`).
    Query { tables: Vec<TableFrame>, last_run: u32 },
    /// Entities, `len` (index, generation) `u32` pairs: `RemovedComponents`.
    Entities { data: *const u32, len: usize },
    /// `SystemChangeTick`.
    Ticks { this_run: u32, last_run: u32 },
    /// One row: a resource, a `Local`, or a computed view such as `Time`. `ticks` points at the
    /// row's changed tick when the param writes it.
    Row { data: *mut u8, bytes: usize, ticks: Option<*mut u32> },
    /// Unread messages, oldest first, in at most two contiguous runs of `stride`-byte records, with
    /// each run's first sequence number (a heap field's place, `places.rs`).
    Messages { parts: [(*const u8, usize); 2], stride: usize, seqs: [u64; 2] },
    State { value: u32 },
    /// An observer's trigger: the event's bytes, its current target and its first target
    /// (`u64::MAX` for none).
    Event { data: *const u8, bytes: usize, entity: u64, original: u64 },
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

    /// [`ScriptBackend::load`] with `host` reachable from JS while the modules evaluate, so a scene's
    /// first render can call the bridge's JSON services (`assets`). A backend without host calls
    /// at load ignores it.
    fn load_with(&mut self, generation: &Generation, host: &mut dyn HostCalls) -> Result<LoadedGeneration, Vec<ScriptError>> {
        let _ = host;
        self.load(generation)
    }

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

    /// Delivers one frame's socket events, then fires the timers due at `now_ms` (real time, in
    /// milliseconds), in one call into JS; makes no call when `events` is empty and no timer is
    /// due. Microtasks the callbacks queue run before it returns. Returns the network requests
    /// scripts made since the last drain, wherever they made them.
    fn deliver(&mut self, now_ms: f64, events: &[crate::net::NetEvent]) -> Result<Delivered, ScriptError>;

    /// Runs pending engine work (microtasks, inspector messages) and returns
    /// `console.*` output gathered since the last pump.
    fn pump(&mut self) -> Vec<ConsoleMessage>;

    /// Replaces the engine instance with a fresh one; every loaded generation is gone. Takes the
    /// spare [`ScriptBackend::prepare_restart`] booted when one is held, else boots one; the old
    /// instance is retired for the next `prepare_restart` to drop.
    fn restart(&mut self);

    /// Drops the instance the last restart retired, then boots a spare for the next restart unless
    /// one is held: a booted instance that ran no module, the same as one booted inside the
    /// restart. Called in a frame of its own under `dev` (a shipped game never restarts).
    fn prepare_restart(&mut self) {}

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
