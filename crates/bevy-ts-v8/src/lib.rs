//! The V8 backend for bevy-ts, through `deno_core`.
//!
//! The isolate lives on the main thread. Column memory crosses as external `ArrayBuffer`s over
//! Rust pointers, which needs V8's sandbox off (the `v8` crate's default). Every buffer handed to
//! JS for a call is detached when the call returns or throws.

#[cfg(feature = "v8_enable_sandbox")]
compile_error!(
    "bevy-ts-v8 cannot run with the V8 sandbox: column views are external ArrayBuffers over ECS \
     memory outside any sandbox. See openspec/changes/bridge/design.md, \"Data layout across the \
     boundary\" (V8 sandbox caveat)."
);

mod loader;

use bevy_ts::backend::{
    CallContext, ConsoleMessage, Delivered, Generation, HostCalls, ParamFrame, RunOutput, ScriptBackend, ScriptError, ScriptKey, SourceLocation,
    SystemRun,
};
use bevy_ts::net::{ConnId, Frame, NetEvent, NetRequest, OpenRequest};
use bevy_ts::log::LogLevel;
use bevy_ts::manifest::{LoadedGeneration, Manifest, Resolution, Section};
use deno_core::error::{CoreError, CoreErrorKind, JsError};
use deno_core::{JsRuntime, OpState, RuntimeOptions, op2, v8};
use loader::Loader;
use std::ffi::c_void;
use std::path::Path;
use std::rc::Rc;

pub use loader::{BUILTIN_BEVY, BUILTIN_CORE};

#[derive(Default)]
struct ConsoleBuffer(Vec<ConsoleMessage>);

#[op2(fast)]
fn op_bevyts_log(state: &mut OpState, #[smi] level: u32, #[string] message: &str, #[string] location: &str) {
    let level = match level {
        0 => LogLevel::Debug,
        1 => LogLevel::Info,
        2 => LogLevel::Warn,
        _ => LogLevel::Error,
    };
    let location = parse_location(location);
    state.borrow_mut::<ConsoleBuffer>().0.push(ConsoleMessage { level, message: message.to_string(), location });
}

/// The host functions of the call in progress (the sixth seam operation), lifetime-erased:
/// set only while a system run or entry call is inside JS.
#[derive(Default)]
struct HostSlot(Option<*mut (dyn HostCalls + 'static)>);

#[derive(Default)]
struct HostError(String);

/// Calls exported method `id`: `args` and `ret` are `f64` slots, `s0..s3` slice arguments viewed
/// in place. Returns the result's slot count, or -1 with the message in [`HostError`].
#[allow(clippy::too_many_arguments)]
#[op2(fast)]
fn op_bevyts_host(
    state: &mut OpState,
    #[smi] id: u32,
    #[buffer] args: &[u8],
    #[buffer] ret: &mut [u8],
    #[buffer] s0: &mut [u8],
    #[buffer] s1: &mut [u8],
    #[buffer] s2: &mut [u8],
    #[buffer] s3: &mut [u8],
) -> i32 {
    let Some(host) = state.borrow::<HostSlot>().0 else {
        state.borrow_mut::<HostError>().0 = "Rust params can only be called while a system runs".into();
        return -1;
    };
    let mut slices = [s0, s1, s2, s3];
    // SAFETY: the slot holds the running call's host for exactly the duration of that call.
    match unsafe { &mut *host }.call(id, args, ret, &mut slices) {
        Ok(n) => n as i32,
        Err(e) => {
            state.borrow_mut::<HostError>().0 = e;
            -1
        }
    }
}

/// Calls the bridge's JSON service `name` with `args` (a JSON array). Returns the result's JSON,
/// or `""` with the message in [`HostError`].
#[op2]
#[string]
fn op_bevyts_host_json(state: &mut OpState, #[string] name: &str, #[string] args: &str) -> String {
    let Some(host) = state.borrow::<HostSlot>().0 else {
        state.borrow_mut::<HostError>().0 = format!("{name} can only be called while a system runs, TSX renders or the script loads");
        return String::new();
    };
    // SAFETY: as in `op_bevyts_host`.
    match unsafe { &mut *host }.call_json(name, args) {
        Ok(json) => json,
        Err(e) => {
            state.borrow_mut::<HostError>().0 = e;
            String::new()
        }
    }
}

#[op2]
#[string]
fn op_bevyts_host_error(state: &mut OpState) -> String {
    std::mem::take(&mut state.borrow_mut::<HostError>().0)
}

/// Network requests scripts made since core last drained them.
#[derive(Default)]
struct Outbox(Vec<NetRequest>);

/// Real time as the last delivery gave it, and the earliest timer's deadline, in milliseconds.
struct Clock {
    now: f64,
    deadline: f64,
}

impl Default for Clock {
    fn default() -> Self {
        Clock { now: 0.0, deadline: f64::INFINITY }
    }
}

#[op2(fast)]
fn op_bevyts_now(state: &mut OpState) -> f64 {
    state.borrow::<Clock>().now
}

/// When the engine's first runtime started: `performance`'s origin, kept across reloads and
/// restarts, as an `Instant` and as Unix milliseconds.
fn origin() -> &'static (std::time::Instant, f64) {
    static ORIGIN: std::sync::OnceLock<(std::time::Instant, f64)> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(|| {
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        (std::time::Instant::now(), unix)
    })
}

/// `performance.now()`: milliseconds since [`origin`].
#[op2(fast)]
fn op_bevyts_performance_now() -> f64 {
    origin().0.elapsed().as_secs_f64() * 1000.0
}

/// `performance.timeOrigin`: the origin in Unix milliseconds.
#[op2(fast)]
fn op_bevyts_time_origin() -> f64 {
    origin().1
}

#[op2(fast)]
fn op_bevyts_timer_deadline(state: &mut OpState, ms: f64) {
    state.borrow_mut::<Clock>().deadline = ms;
}

#[op2(fast)]
fn op_bevyts_ws_open(
    state: &mut OpState,
    #[smi] id: u32,
    #[smi] generation: u32,
    #[string] url: &str,
    #[string] protocols: &str,
    #[string] headers: &str,
) {
    let protocols = serde_json::from_str(protocols).unwrap_or_default();
    let headers = serde_json::from_str(headers).unwrap_or_default();
    let request = OpenRequest { url: url.to_string(), protocols, headers };
    state.borrow_mut::<Outbox>().0.push(NetRequest::Open { id: ConnId(id as u64), generation: generation as u64, request });
}

#[op2(fast)]
fn op_bevyts_ws_send_text(state: &mut OpState, #[smi] id: u32, #[smi] generation: u32, #[string] text: &str) {
    let frame = Frame::Text(text.to_string());
    state.borrow_mut::<Outbox>().0.push(NetRequest::Send { id: ConnId(id as u64), generation: generation as u64, frame });
}

#[op2(fast)]
fn op_bevyts_ws_send_binary(state: &mut OpState, #[smi] id: u32, #[smi] generation: u32, #[buffer] bytes: &[u8]) {
    let frame = Frame::Binary(bytes.to_vec());
    state.borrow_mut::<Outbox>().0.push(NetRequest::Send { id: ConnId(id as u64), generation: generation as u64, frame });
}

/// `code` is -1 for a close without a status.
#[op2(fast)]
fn op_bevyts_ws_close(state: &mut OpState, #[smi] id: u32, #[smi] generation: u32, #[smi] code: i32, #[string] reason: &str) {
    let code = u16::try_from(code).ok();
    state.borrow_mut::<Outbox>().0.push(NetRequest::Close {
        id: ConnId(id as u64),
        generation: generation as u64,
        code,
        reason: reason.to_string(),
    });
}

deno_core::extension!(
    bevy_ts_v8,
    ops = [
        op_bevyts_log,
        op_bevyts_host,
        op_bevyts_host_json,
        op_bevyts_host_error,
        op_bevyts_now,
        op_bevyts_performance_now,
        op_bevyts_time_origin,
        op_bevyts_timer_deadline,
        op_bevyts_ws_open,
        op_bevyts_ws_send_text,
        op_bevyts_ws_send_binary,
        op_bevyts_ws_close
    ],
    state = |state| {
        state.put(ConsoleBuffer::default());
        state.put(HostSlot::default());
        state.put(HostError::default());
        state.put(Outbox::default());
        state.put(Clock::default());
        origin();
    }
);

/// `file:///a/b.ts:3:7` to a location.
fn parse_location(s: &str) -> Option<SourceLocation> {
    let mut parts = s.rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let line = parts.next()?.parse().ok()?;
    let file = bevy_ts::backend::display_file(parts.next()?);
    Some(SourceLocation { file, line, column })
}

fn is_builtin(file: &str) -> bool {
    file.starts_with("bevy-ts:") || file.starts_with("ext:")
}

/// Maps a JS error to a script error located at its first frame in user code.
fn script_error(e: &JsError) -> ScriptError {
    let mut stack = String::new();
    let mut location = None;
    for f in &e.frames {
        let Some(file) = &f.file_name else { continue };
        let (line, column) = (f.line_number.unwrap_or(0) as u32, f.column_number.unwrap_or(0) as u32);
        let shown = if is_builtin(file) { file.clone() } else { bevy_ts::backend::display_file(file) };
        let name = f.function_name.as_deref().unwrap_or("<anonymous>");
        stack.push_str(&format!("    at {name} ({shown}:{line}:{column})\n"));
        if location.is_none() && !is_builtin(file) {
            location = Some(SourceLocation { file: shown, line, column });
        }
    }
    let message = match (&e.name, &e.message) {
        (Some(n), Some(m)) => format!("{n}: {m}"),
        _ => e.exception_message.trim_start_matches("Uncaught ").to_string(),
    };
    ScriptError { message, location, stack: (!stack.is_empty()).then(|| stack.trim_end().to_string()) }
}

fn core_error(e: CoreError) -> ScriptError {
    match *e.0 {
        CoreErrorKind::Js(js) => script_error(&js),
        other => ScriptError::new(other.to_string()),
    }
}

/// A declaration error thrown with a `bevyTsLocation` (the declaring call site).
fn located(scope: &mut v8::PinScope, exception: v8::Local<v8::Value>, mut err: ScriptError) -> ScriptError {
    if let Ok(obj) = v8::Local::<v8::Object>::try_from(exception) {
        let key = v8::String::new(scope, "bevyTsLocation").unwrap();
        if let Some(loc) = obj.get(scope, key.into())
            && let Ok(loc) = deno_core::serde_v8::from_v8::<Option<JsLocation>>(scope, loc)
            && let Some(loc) = loc
        {
            err.location = Some(loc.into());
        }
    }
    err
}

#[derive(serde::Deserialize)]
struct JsLocation {
    file: String,
    line: u32,
    column: u32,
}

impl From<JsLocation> for SourceLocation {
    fn from(l: JsLocation) -> Self {
        SourceLocation { file: bevy_ts::backend::display_file(&l.file), line: l.line, column: l.column }
    }
}

struct Fns {
    begin: v8::Global<v8::Function>,
    end: v8::Global<v8::Function>,
    abort: v8::Global<v8::Function>,
    commit: v8::Global<v8::Function>,
    discard: v8::Global<v8::Function>,
    run_system: v8::Global<v8::Function>,
    call_entry: v8::Global<v8::Function>,
    deliver: v8::Global<v8::Function>,
}

/// Options for the V8 backend.
#[derive(Clone, Debug, Default)]
pub struct V8Options {
    /// Serve the Chrome DevTools Protocol on this address (`127.0.0.1:9229`).
    pub inspect: Option<std::net::SocketAddr>,
}

pub struct V8Backend {
    /// `ManuallyDrop` so `fns` (handles into the isolate) always drop first, and so a restart
    /// can drop the old isolate before creating the new one: isolates on one thread must be
    /// disposed in the reverse order they were made.
    runtime: std::mem::ManuallyDrop<JsRuntime>,
    tokio: tokio::runtime::Runtime,
    loader: Rc<Loader>,
    fns: std::mem::ManuallyDrop<Fns>,
    calls: u64,
    /// Load counter: tags module specifiers so every load is a fresh module record.
    loads: u64,
    pool_f64: Vec<f64>,
    options: V8Options,
    pub(crate) inspector: Option<inspector::InspectorServer>,
}

mod inspector;

/// A detachable `ArrayBuffer` over Rust memory. The deleter does nothing: Rust owns the memory
/// and the buffer is detached before the memory can move.
fn external<'s>(scope: &mut v8::PinScope<'s, '_>, ptr: *mut u8, len: usize) -> v8::Local<'s, v8::ArrayBuffer> {
    unsafe extern "C" fn keep(_: *mut c_void, _: usize, _: *mut c_void) {}
    // SAFETY: the pointer is valid for `len` bytes until the buffer is detached at the end of
    // the call; the deleter never frees.
    let store = unsafe { v8::ArrayBuffer::new_backing_store_from_ptr(ptr as *mut c_void, len, keep, std::ptr::null_mut()) };
    v8::ArrayBuffer::with_backing_store(scope, &store.make_shared())
}

/// An `ArrayBuffer` over `len` bytes at `ptr`, or an empty one for a null or zero-length span.
fn span<'s>(scope: &mut v8::PinScope<'s, '_>, buffers: &mut Vec<v8::Local<'s, v8::ArrayBuffer>>, ptr: *mut u8, len: usize) -> v8::Local<'s, v8::ArrayBuffer> {
    if ptr.is_null() || len == 0 {
        return v8::ArrayBuffer::new(scope, 0);
    }
    let ab = external(scope, ptr, len);
    buffers.push(ab);
    ab
}

/// One param's frame as the JS runtime reads it (see core.js `runSystem`).
fn frame_value<'s>(s: &mut v8::PinScope<'s, '_>, buffers: &mut Vec<v8::Local<'s, v8::ArrayBuffer>>, p: &ParamFrame) -> v8::Local<'s, v8::Value> {
    match p {
        ParamFrame::Query { tables } => {
            let arr = v8::Array::new(s, tables.len() as i32);
            for (i, t) in tables.iter().enumerate() {
                let ents = span(s, buffers, t.entities as *mut u8, t.len * 8);
                let ents = v8::Uint32Array::new(s, ents, 0, if ents.byte_length() == 0 { 0 } else { t.len * 2 }).unwrap();
                let cols = v8::Array::new(s, t.columns.len() as i32);
                let ticks = v8::Array::new(s, t.columns.len() as i32);
                for (c, col) in t.columns.iter().enumerate() {
                    let data: v8::Local<v8::Value> =
                        if col.data.is_null() { v8::null(s).into() } else { span(s, buffers, col.data, col.bytes).into() };
                    cols.set_index(s, c as u32, data);
                    let tv: v8::Local<v8::Value> = match col.ticks {
                        Some(p) => span(s, buffers, p as *mut u8, t.len * 4).into(),
                        None => v8::null(s).into(),
                    };
                    ticks.set_index(s, c as u32, tv);
                }
                let rows: v8::Local<v8::Value> = match t.rows {
                    Some((ptr, n)) => {
                        let ab = span(s, buffers, ptr as *mut u8, n * 4);
                        v8::Uint32Array::new(s, ab, 0, if ab.byte_length() == 0 { 0 } else { n }).unwrap().into()
                    }
                    None => v8::null(s).into(),
                };
                let len = num(s, t.len as f64);
                let tuple = v8::Array::new_with_elements(s, &[len, ents.into(), cols.into(), ticks.into(), rows]);
                arr.set_index(s, i as u32, tuple.into());
            }
            arr.into()
        }
        ParamFrame::Row { data, bytes, ticks } => {
            let d = span(s, buffers, *data, *bytes);
            let t: v8::Local<v8::Value> = match ticks {
                Some(p) => span(s, buffers, *p as *mut u8, 4).into(),
                None => v8::null(s).into(),
            };
            v8::Array::new_with_elements(s, &[d.into(), t]).into()
        }
        ParamFrame::Messages { parts, stride } => {
            let a = span(s, buffers, parts[0].0 as *mut u8, parts[0].1 * stride);
            let b = span(s, buffers, parts[1].0 as *mut u8, parts[1].1 * stride);
            let (na, nb, st) = (num(s, parts[0].1 as f64), num(s, parts[1].1 as f64), num(s, *stride as f64));
            v8::Array::new_with_elements(s, &[a.into(), na, b.into(), nb, st]).into()
        }
        ParamFrame::State { value } => num(s, if *value == u32::MAX { -1.0 } else { *value as f64 }),
        ParamFrame::Event { data, bytes, entity, original } => {
            let d = span(s, buffers, *data as *mut u8, *bytes);
            let e = num(s, *entity as f64);
            let o = num(s, *original as f64);
            v8::Array::new_with_elements(s, &[d.into(), e, o]).into()
        }
        ParamFrame::None => v8::null(s).into(),
    }
}

fn get_fn(scope: &mut v8::PinScope, host: v8::Local<v8::Object>, name: &str) -> v8::Global<v8::Function> {
    let key = v8::String::new(scope, name).unwrap();
    let f = host.get(scope, key.into()).unwrap();
    let f = v8::Local::<v8::Function>::try_from(f).unwrap_or_else(|_| panic!("bevy-ts runtime lacks {name}"));
    v8::Global::new(scope, f)
}

fn num<'s>(scope: &mut v8::PinScope<'s, '_>, n: f64) -> v8::Local<'s, v8::Value> {
    v8::Number::new(scope, n).into()
}

/// Reads `[arenaLen, overflow, poolUsed, marks]` from a run's return value.
fn read_output<'s>(scope: &mut v8::PinScope<'s, '_>, v: v8::Local<'s, v8::Value>, offset: u32) -> RunOutput {
    let arr = v8::Local::<v8::Array>::try_from(v).expect("runtime returns an array");
    let arena_len = arr.get_index(scope, offset).unwrap().number_value(scope).unwrap_or(0.0) as usize;
    let overflow = arr.get_index(scope, offset + 1).unwrap();
    let overflow = match v8::Local::<v8::Uint8Array>::try_from(overflow) {
        Ok(a) => {
            let mut out = vec![0u8; a.byte_length()];
            a.copy_contents(&mut out);
            out
        }
        Err(_) => Vec::new(),
    };
    let pool_used = arr.get_index(scope, offset + 2).unwrap().number_value(scope).unwrap_or(0.0) as usize;
    let mut write_marks = Vec::new();
    if arr.length() > 3 + offset
        && let Ok(marks) = v8::Local::<v8::Array>::try_from(arr.get_index(scope, offset + 3).unwrap())
    {
        let n = marks.length();
        let at = |scope: &mut v8::PinScope, i: u32| marks.get_index(scope, i).unwrap().number_value(scope).unwrap_or(0.0) as u32;
        let mut i = 0;
        while i + 2 < n {
            write_marks.push((at(scope, i), at(scope, i + 1), at(scope, i + 2)));
            i += 3;
        }
    }
    let stop_propagation = arr.length() > 4 + offset && arr.get_index(scope, offset + 4).unwrap().is_true();
    RunOutput { arena_len, overflow, pool_used, write_marks, stop_propagation }
}

impl V8Backend {
    pub fn new(options: V8Options) -> Self {
        let loader = Rc::new(Loader::default());
        let tokio = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        let (runtime, fns) = Self::boot(&loader, &tokio, options.inspect.is_some());
        let mut backend =
            V8Backend {
                runtime: std::mem::ManuallyDrop::new(runtime),
                tokio,
                loader,
                fns: std::mem::ManuallyDrop::new(fns),
                calls: 0, loads: 0, pool_f64: Vec::new(), options, inspector: None };
        backend.start_inspector();
        backend
    }

    fn boot(loader: &Rc<Loader>, tokio: &tokio::runtime::Runtime, inspector: bool) -> (JsRuntime, Fns) {
        // V8 posts delayed tasks through tokio; the isolate must be made inside its context.
        let _enter = tokio.enter();
        let mut runtime = JsRuntime::new(RuntimeOptions {
            module_loader: Some(loader.clone()),
            extensions: vec![bevy_ts_v8::init()],
            inspector,
            ..Default::default()
        });
        // The built-in modules are side modules evaluated once per isolate; user imports of
        // `bevy-ts` resolve to the same records.
        tokio.block_on(async {
            for spec in [loader::BUILTIN_WEB, loader::BUILTIN_CORE, loader::BUILTIN_BEVY] {
                let url = deno_core::ModuleSpecifier::parse(spec).unwrap();
                let id = runtime.load_side_es_module(&url).await.expect("bevy-ts runtime loads");
                let eval = runtime.mod_evaluate(id);
                runtime.run_event_loop(Default::default()).await.expect("bevy-ts runtime evaluates");
                eval.await.expect("bevy-ts runtime evaluates");
            }
        });
        let fns = {
            deno_core::scope!(scope, &mut runtime);
            let global = scope.get_current_context().global(scope);
            let key = v8::String::new(scope, "__bevyts").unwrap();
            let host = global.get(scope, key.into()).unwrap();
            let host = v8::Local::<v8::Object>::try_from(host).expect("bevy-ts runtime installs __bevyts");
            Fns {
                begin: get_fn(scope, host, "begin"),
                end: get_fn(scope, host, "end"),
                abort: get_fn(scope, host, "abort"),
                commit: get_fn(scope, host, "commit"),
                discard: get_fn(scope, host, "discard"),
                run_system: get_fn(scope, host, "runSystem"),
                call_entry: get_fn(scope, host, "callEntry"),
                deliver: get_fn(scope, host, "deliver"),
            }
        };
        (runtime, fns)
    }

    /// Calls a host function with arguments built in the scope, detaching `buffers` afterwards.
    fn call<R>(
        &mut self,
        f: impl FnOnce(&Fns) -> &v8::Global<v8::Function>,
        build: impl for<'s> FnOnce(&mut v8::PinScope<'s, '_>, &mut Vec<v8::Local<'s, v8::ArrayBuffer>>) -> Vec<v8::Local<'s, v8::Value>>,
        read: impl for<'s> FnOnce(&mut v8::PinScope<'s, '_>, v8::Local<'s, v8::Value>) -> R,
    ) -> Result<R, ScriptError> {
        self.calls += 1;
        let _enter = self.tokio.enter();
        let func = f(&self.fns).clone();
        deno_core::scope!(scope, &mut self.runtime);
        v8::tc_scope!(let tc, scope);
        let mut buffers = Vec::new();
        let args = build(tc, &mut buffers);
        let func = v8::Local::new(tc, &func);
        let recv = v8::undefined(tc).into();
        let result = func.call(tc, recv, &args);
        for b in &buffers {
            b.detach(None);
        }
        match result {
            Some(v) => Ok(read(tc, v)),
            None => {
                let exception = tc.exception().unwrap_or_else(|| v8::undefined(tc).into());
                let js = JsError::from_v8_exception(tc, exception);
                let err = script_error(&js);
                Err(located(tc, exception, err))
            }
        }
    }

    fn specifier(&self, path: &Path) -> String {
        format!("{}?gen={}", deno_core::url::Url::from_file_path(path).expect("absolute module path"), self.loads)
    }

    fn ctx_args<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        buffers: &mut Vec<v8::Local<'s, v8::ArrayBuffer>>,
        ctx: &mut CallContext<'_>,
        pool: &mut [f64],
    ) -> [v8::Local<'s, v8::Value>; 6] {
        let arena = external(scope, ctx.arena.as_mut_ptr(), ctx.arena.len());
        let pool_ab = external(scope, pool.as_mut_ptr() as *mut u8, pool.len() * 8);
        buffers.push(arena);
        buffers.push(pool_ab);
        [
            num(scope, ctx.frame.dt),
            num(scope, ctx.frame.elapsed),
            num(scope, ctx.frame.tick as f64),
            num(scope, ctx.frame.frame as f64),
            arena.into(),
            pool_ab.into(),
        ]
    }

    fn fill_pool(&mut self, pool: &[u64]) {
        self.pool_f64.clear();
        // Entity bits are exact in an f64 while the generation stays below 2^21.
        self.pool_f64.extend(pool.iter().map(|b| *b as f64));
    }
}

impl Drop for V8Backend {
    fn drop(&mut self) {
        self.stop_inspector();
        // SAFETY: dropped once, handles before the isolate that owns them.
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.fns);
            std::mem::ManuallyDrop::drop(&mut self.runtime);
        }
    }
}

impl ScriptBackend for V8Backend {
    fn engine(&self) -> String {
        format!("V8 {} (deno_core)", v8::VERSION_STRING)
    }

    fn load(&mut self, generation: &Generation) -> Result<LoadedGeneration, Vec<ScriptError>> {
        self.loads += 1;
        let entry = self.specifier(&generation.set.entry);
        let modules: Vec<(String, _)> = generation.set.modules.iter().map(|m| (self.specifier(&m.path), m.clone())).collect();
        self.loader.set_generation(self.loads, modules);
        self.runtime.op_state().borrow_mut().borrow_mut::<Clock>().now = generation.now_ms;
        let number = generation.number as f64;
        let observed = serde_json::to_string(&generation.observed).expect("names serialize");
        let env = serde_json::to_string(&generation.env).expect("env serializes");
        let begun = self.call(|f| &f.begin, |s, _| vec![num(s, number), v8::String::new(s, &env).unwrap().into(), v8::String::new(s, &observed).unwrap().into()], |_, _| ());
        let result = begun.and_then(|_| self.evaluate(&entry));
        if let Err(e) = result {
            let _ = self.call(|f| &f.abort, |_, _| vec![], |_, _| ());
            self.drop_requests(generation.number);
            return Err(vec![self.relocate(e)]);
        }
        match self.finish_load() {
            Ok(mut loaded) => {
                loaded.requests = self.take_requests(Some(generation.number));
                Ok(loaded)
            }
            Err(e) => {
                self.drop_requests(generation.number);
                Err(vec![e])
            }
        }
    }

    fn load_with(&mut self, generation: &Generation, host: &mut dyn HostCalls) -> Result<LoadedGeneration, Vec<ScriptError>> {
        self.set_host(Some(host));
        let r = self.load(generation);
        self.set_host(None);
        r
    }

    fn commit(&mut self, generation: u64, resolution: &Resolution) -> Result<(), Vec<ScriptError>> {
        let json = serde_json::to_string(resolution).expect("resolution serializes");
        self.call(
            |f| &f.commit,
            |s, _| vec![num(s, generation as f64), v8::String::new(s, &json).unwrap().into()],
            |_, _| (),
        )
        .map_err(|e| vec![e])
    }

    fn discard(&mut self, generation: u64) {
        let _ = self.call(|f| &f.discard, |s, _| vec![num(s, generation as f64)], |_, _| ());
        self.drop_requests(generation);
    }

    fn deliver(&mut self, now_ms: f64, events: &[NetEvent]) -> Result<Delivered, ScriptError> {
        let deadline = {
            let state = self.runtime.op_state();
            let mut state = state.borrow_mut();
            let clock = state.borrow_mut::<Clock>();
            clock.now = now_ms;
            clock.deadline
        };
        let mut result = Ok(());
        if !events.is_empty() || now_ms >= deadline {
            result = self.call(|f| &f.deliver, |s, _| vec![num(s, now_ms), event_batch(s, events)], |_, _| ());
            let _enter = self.tokio.enter();
            deno_core::scope!(scope, &mut self.runtime);
            scope.perform_microtask_checkpoint();
        }
        // On a throw the requests stay queued for the next delivery.
        result?;
        let requests = self.take_requests(None);
        let deadline = self.runtime.op_state().borrow().borrow::<Clock>().deadline;
        Ok(Delivered { requests, next_timer_ms: deadline.is_finite().then_some(deadline) })
    }

    fn run_system(&mut self, key: ScriptKey, run: &mut SystemRun<'_>) -> Result<RunOutput, ScriptError> {
        self.fill_pool(run.ctx.pool);
        let mut pool = std::mem::take(&mut self.pool_f64);
        let params = run.params;
        let ctx = &mut run.ctx;
        self.set_host(ctx.host.take());
        let result = self.call(
            |f| &f.run_system,
            |s, buffers| {
                let frames = v8::Array::new(s, params.len() as i32);
                for (i, p) in params.iter().enumerate() {
                    let v = frame_value(s, buffers, p);
                    frames.set_index(s, i as u32, v);
                }
                let [dt, el, tick, frame, arena, pool_ab] = Self::ctx_args(s, buffers, ctx, &mut pool);
                vec![num(s, key.generation as f64), num(s, key.index as f64), dt, el, tick, frame, frames.into(), arena, pool_ab]
            },
            |s, v| read_output(s, v, 0),
        );
        self.set_host(None);
        self.pool_f64 = pool;
        result.map_err(|e| self.relocate(e))
    }

    fn call_entry(&mut self, name: &str, payload: &[u8], ctx: &mut CallContext<'_>) -> Result<(Vec<u8>, RunOutput), ScriptError> {
        self.fill_pool(ctx.pool);
        let mut pool = std::mem::take(&mut self.pool_f64);
        let mut payload = payload.to_vec();
        self.set_host(ctx.host.take());
        let result = self.call(
            |f| &f.call_entry,
            |s, buffers| {
                let p = external(s, payload.as_mut_ptr(), payload.len());
                buffers.push(p);
                let [dt, el, tick, frame, arena, pool_ab] = Self::ctx_args(s, buffers, ctx, &mut pool);
                vec![v8::String::new(s, name).unwrap().into(), p.into(), dt, el, tick, frame, arena, pool_ab]
            },
            |s, v| {
                let arr = v8::Local::<v8::Array>::try_from(v).unwrap();
                let out = v8::Local::<v8::Uint8Array>::try_from(arr.get_index(s, 0).unwrap()).unwrap();
                let mut bytes = vec![0u8; out.byte_length()];
                out.copy_contents(&mut bytes);
                (bytes, read_output(s, v, 1))
            },
        );
        self.set_host(None);
        self.pool_f64 = pool;
        result.map_err(|e| self.relocate(e))
    }

    fn pump(&mut self) -> Vec<ConsoleMessage> {
        let _enter = self.tokio.enter();
        self.pump_inspector();
        let runtime = &mut self.runtime;
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let _ = self.tokio.block_on(async { runtime.poll_event_loop(&mut cx, Default::default()) });
        let state = self.runtime.op_state();
        let mut state = state.borrow_mut();
        std::mem::take(&mut state.borrow_mut::<ConsoleBuffer>().0)
    }

    fn restart(&mut self) {
        self.stop_inspector();
        // SAFETY: both are re-initialised below before any other use.
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.fns);
            std::mem::ManuallyDrop::drop(&mut self.runtime);
        }
        let (runtime, fns) = Self::boot(&self.loader, &self.tokio, self.options.inspect.is_some());
        // Overwrite without dropping the already-dropped values.
        unsafe {
            std::ptr::write(&mut self.runtime, std::mem::ManuallyDrop::new(runtime));
            std::ptr::write(&mut self.fns, std::mem::ManuallyDrop::new(fns));
        }
        self.start_inspector();
    }

    fn calls_into_js(&self) -> u64 {
        self.calls
    }

    fn heap_used(&self) -> Option<usize> {
        None
    }

    fn inspector_address(&self) -> Option<String> {
        self.inspector.as_ref().map(|i| format!("ws://{}/{}", i.address(), inspector::TARGET_ID))
    }
}


impl V8Backend {
    /// Evaluates the entry module of the generation being loaded.
    fn evaluate(&mut self, entry: &str) -> Result<(), ScriptError> {
        let url = deno_core::ModuleSpecifier::parse(entry).map_err(|e| ScriptError::new(e.to_string()))?;
        let runtime = &mut self.runtime;
        self.tokio.block_on(async {
            let id = runtime.load_side_es_module(&url).await.map_err(core_error)?;
            let eval = runtime.mod_evaluate(id);
            runtime.run_event_loop(Default::default()).await.map_err(core_error)?;
            eval.await.map_err(core_error)
        })
    }

    /// Takes the outbox's requests: those of `generation` only, or all.
    fn take_requests(&mut self, generation: Option<u64>) -> Vec<NetRequest> {
        let state = self.runtime.op_state();
        let mut state = state.borrow_mut();
        let outbox = &mut state.borrow_mut::<Outbox>().0;
        match generation {
            None => std::mem::take(outbox),
            Some(g) => {
                let (mine, rest) = std::mem::take(outbox).into_iter().partition(|r| r.generation() == g);
                *outbox = rest;
                mine
            }
        }
    }

    /// Forgets a generation's requests: it will never be live.
    fn drop_requests(&mut self, generation: u64) {
        self.runtime.op_state().borrow_mut().borrow_mut::<Outbox>().0.retain(|r| r.generation() != generation);
    }

    /// Ends a load: the manifest and sections the generation declared.
    fn finish_load(&mut self) -> Result<LoadedGeneration, ScriptError> {
        let (json, sections) = self
            .call(
                |f| &f.end,
                |_, _| vec![],
                |scope, v| {
                    let arr = v8::Local::<v8::Array>::try_from(v).unwrap();
                    let json = arr.get_index(scope, 0).unwrap().to_rust_string_lossy(scope);
                    let list = v8::Local::<v8::Array>::try_from(arr.get_index(scope, 1).unwrap()).unwrap();
                    let mut sections = Vec::new();
                    for i in 0..list.length() {
                        let item = v8::Local::<v8::Array>::try_from(list.get_index(scope, i).unwrap()).unwrap();
                        let name = item.get_index(scope, 0).unwrap().to_rust_string_lossy(scope);
                        let bytes = v8::Local::<v8::Uint8Array>::try_from(item.get_index(scope, 1).unwrap()).unwrap();
                        let mut b = vec![0u8; bytes.byte_length()];
                        bytes.copy_contents(&mut b);
                        let loc = item.get_index(scope, 2).unwrap();
                        let location = deno_core::serde_v8::from_v8::<Option<JsLocation>>(scope, loc).ok().flatten().map(Into::into);
                        sections.push(Section { name, bytes: b, location });
                    }
                    (json, sections)
                },
            )
            ?;
        let mut manifest: Manifest =
            serde_json::from_str(&json).map_err(|e| ScriptError::new(format!("bevy-ts manifest: {e}")))?;
        relocate_manifest(&mut manifest);
        Ok(LoadedGeneration { manifest, sections, requests: Vec::new() })
    }
}

impl V8Backend {
    /// Makes `host` callable from JS until the next `set_host`.
    fn set_host(&mut self, host: Option<&mut dyn HostCalls>) {
        // SAFETY: only the lifetime is erased; every call into JS that sets a host clears it
        // before returning, so the pointer is never used past the call that owns it.
        let ptr = host.map(|h| unsafe { std::mem::transmute::<*mut dyn HostCalls, *mut (dyn HostCalls + 'static)>(h) });
        self.runtime.op_state().borrow_mut().borrow_mut::<HostSlot>().0 = ptr;
    }

    /// Errors from module evaluation may be located at a declaration's call site.
    fn relocate(&self, e: ScriptError) -> ScriptError {
        e
    }

    /// The source text of a loaded module, for tests.
    pub fn module_source(&self, path: &Path) -> Option<String> {
        self.loader.source_of(path)
    }

    /// Where the inspector listens, if it does.
    pub fn inspector_address(&self) -> Option<std::net::SocketAddr> {
        self.inspector.as_ref().map(|i| i.address())
    }
}

fn relocate_manifest(m: &mut Manifest) {
    let fix = |l: &mut Option<SourceLocation>| {
        if let Some(l) = l {
            l.file = bevy_ts::backend::display_file(&l.file);
        }
    };
    m.components.iter_mut().for_each(|c| fix(&mut c.location));
    m.systems.iter_mut().for_each(|c| fix(&mut c.location));
    m.resources.iter_mut().for_each(|c| fix(&mut c.location));
    m.messages.iter_mut().chain(m.events.iter_mut()).for_each(|c| fix(&mut c.location));
    m.states.iter_mut().for_each(|c| fix(&mut c.location));
    m.entries.iter_mut().for_each(|c| fix(&mut c.location));
}

/// A factory for `BevyTsPlugin::new`.
pub fn factory(options: V8Options) -> impl Fn() -> Box<dyn ScriptBackend> + Send + Sync + 'static {
    move || Box::new(V8Backend::new(options.clone()))
}

/// One delivery's events as JS reads them (web.js `deliver`): `[0, id, protocol, extensions]`
/// open, `[1, id, text]` and `[2, id, ArrayBuffer]` messages, `[3, id]` error,
/// `[4, id, code, reason, clean]` close, `[5, id, bytes]` written.
fn event_batch<'s>(s: &mut v8::PinScope<'s, '_>, events: &[NetEvent]) -> v8::Local<'s, v8::Value> {
    let str = |s: &mut v8::PinScope<'s, '_>, t: &str| -> v8::Local<'s, v8::Value> {
        v8::String::new(s, t).map_or_else(|| v8::String::empty(s).into(), Into::into)
    };
    let arr = v8::Array::new(s, events.len() as i32);
    for (i, e) in events.iter().enumerate() {
        let id = num(s, e.id().0 as f64);
        let items: Vec<v8::Local<v8::Value>> = match e {
            NetEvent::Open { protocol, extensions, .. } => vec![num(s, 0.0), id, str(s, protocol), str(s, extensions)],
            NetEvent::Message { frame: Frame::Text(t), .. } => vec![num(s, 1.0), id, str(s, t)],
            NetEvent::Message { frame: Frame::Binary(b), .. } => {
                let store = v8::ArrayBuffer::new_backing_store_from_vec(b.clone()).make_shared();
                vec![num(s, 2.0), id, v8::ArrayBuffer::with_backing_store(s, &store).into()]
            }
            NetEvent::Error { .. } => vec![num(s, 3.0), id],
            NetEvent::Close { code, reason, clean, .. } => {
                vec![num(s, 4.0), id, num(s, *code as f64), str(s, reason), v8::Boolean::new(s, *clean).into()]
            }
            NetEvent::Written { bytes, .. } => vec![num(s, 5.0), id, num(s, *bytes as f64)],
        };
        let item = v8::Array::new_with_elements(s, &items);
        arr.set_index(s, i as u32, item.into());
    }
    arr.into()
}
