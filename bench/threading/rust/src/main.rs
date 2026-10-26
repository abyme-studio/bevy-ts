//! Isolate startup, memory per isolate, a thread hop into another isolate, and one column
//! loop split across isolates on threads over the same Rust memory (no copy).
use deno_core::{JsRuntime, RuntimeOptions, v8};
use std::ffi::c_void;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap() / 1024.0
}

/// A synthetic generation: `n` small functions, about what a mid-sized game's modules compile to.
fn bundle(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!("globalThis.f{i} = function(a, b) {{ let x = a * {i} + b; for (let k = 0; k < 3; k++) x += k; return x; }};\n"));
    }
    s
}

const SYSTEM: &str = "(function(pos, vel, dt) { for (let i = 0; i < pos.length; i++) pos[i] += vel[i] * dt; return pos.length; })";
const ECHO: &str = "(function(bytes) { return bytes[0] + bytes.length; })";

fn func(rt: &mut JsRuntime, code: &'static str) -> v8::Global<v8::Function> {
    let g = rt.execute_script("bench", code).unwrap();
    deno_core::scope!(scope, rt);
    let l = v8::Local::new(scope, &g);
    let f = v8::Local::<v8::Function>::try_from(l).unwrap();
    v8::Global::new(scope, f)
}

unsafe extern "C" fn keep(_: *mut c_void, _: usize, _: *mut c_void) {}

fn f32_view<'s>(scope: &mut v8::PinScope<'s, '_>, ptr: *mut f32, len: usize, abs: &mut Vec<v8::Local<'s, v8::ArrayBuffer>>) -> v8::Local<'s, v8::Value> {
    let store = unsafe { v8::ArrayBuffer::new_backing_store_from_ptr(ptr as *mut c_void, len * 4, keep, std::ptr::null_mut()) };
    let ab = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
    abs.push(ab);
    v8::Float32Array::new(scope, ab, 0, len).unwrap().into()
}

fn run_system(rt: &mut JsRuntime, f: &v8::Global<v8::Function>, pos: *mut f32, vel: *mut f32, len: usize) {
    deno_core::scope!(scope, rt);
    let mut abs = Vec::new();
    let p = f32_view(scope, pos, len, &mut abs);
    let v = f32_view(scope, vel, len, &mut abs);
    let dt = v8::Number::new(scope, 0.016).into();
    let f = v8::Local::new(scope, f);
    let recv = v8::undefined(scope).into();
    f.call(scope, recv, &[p, v, dt]).unwrap();
    for ab in abs {
        ab.detach(None);
    }
}

fn echo(rt: &mut JsRuntime, f: &v8::Global<v8::Function>, bytes: &mut [u8]) -> f64 {
    deno_core::scope!(scope, rt);
    let store = unsafe { v8::ArrayBuffer::new_backing_store_from_ptr(bytes.as_mut_ptr() as *mut c_void, bytes.len(), keep, std::ptr::null_mut()) };
    let ab = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
    let arr = v8::Uint8Array::new(scope, ab, 0, bytes.len()).unwrap().into();
    let f = v8::Local::new(scope, f);
    let recv = v8::undefined(scope).into();
    let r = f.call(scope, recv, &[arr]).unwrap().number_value(scope).unwrap();
    ab.detach(None);
    r
}

#[derive(Clone, Copy)]
struct Span(*mut f32, *mut f32, usize);
unsafe impl Send for Span {}

enum Job {
    Echo(Vec<u8>),
    System(Span),
}

/// An isolate on a thread of its own, taking jobs over a channel.
struct Worker {
    tx: mpsc::Sender<Job>,
    rx: mpsc::Receiver<f64>,
}

fn worker(bundle_src: Option<String>) -> Worker {
    let (tx, jobs) = mpsc::channel::<Job>();
    let (done, rx) = mpsc::channel::<f64>();
    std::thread::spawn(move || {
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        if let Some(src) = bundle_src {
            rt.execute_script("bundle", src).unwrap();
        }
        let sys = func(&mut rt, SYSTEM);
        let ech = func(&mut rt, ECHO);
        done.send(0.0).unwrap();
        for job in jobs {
            let r = match job {
                Job::Echo(mut b) => echo(&mut rt, &ech, &mut b),
                Job::System(Span(p, v, n)) => {
                    run_system(&mut rt, &sys, p, v, n);
                    n as f64
                }
            };
            done.send(r).unwrap();
        }
    });
    let w = Worker { tx, rx };
    w.rx.recv().unwrap();
    w
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let base = rss_mb();
    // 1. Startup.
    let t = Instant::now();
    let first = JsRuntime::new(RuntimeOptions::default());
    let first_ms = t.elapsed().as_secs_f64() * 1e3;
    drop(first);
    let mut fresh = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let rt = JsRuntime::new(RuntimeOptions::default());
        fresh.push(t.elapsed().as_secs_f64() * 1e3);
        drop(rt);
    }
    let src = bundle(5000);
    let mut loaded = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        rt.execute_script("bundle", src.clone()).unwrap();
        loaded.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("first JsRuntime (V8 platform init): {first_ms:.2} ms");
    println!("JsRuntime::new, median of 20: {:.2} ms", median(fresh));
    println!("JsRuntime::new + {} KB of JS (5000 fns), median of 10: {:.2} ms", src.len() / 1024, median(loaded));

    // 2. Memory per isolate: 16 workers, each holding an isolate that loaded the bundle.
    let before = rss_mb();
    let workers: Vec<Worker> = (0..16).map(|_| worker(Some(src.clone()))).collect();
    let after = rss_mb();
    println!("RSS at start {base:.1} MB; 16 isolates with the bundle add {:.1} MB, {:.1} MB each", after - before, (after - before) / 16.0);

    // 3. A thread hop: call into another thread's isolate and wait, against a call on this thread.
    let mut local = JsRuntime::new(RuntimeOptions::default());
    let ech = func(&mut local, ECHO);
    for size in [64usize, 65536] {
        let mut buf = vec![1u8; size];
        let n = 20_000;
        let t = Instant::now();
        for _ in 0..n {
            echo(&mut local, &ech, &mut buf);
        }
        let same = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        let w = &workers[0];
        let t = Instant::now();
        for _ in 0..n {
            w.tx.send(Job::Echo(buf.clone())).unwrap();
            w.rx.recv().unwrap();
        }
        let hop = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        println!("call with {size} B: same thread {same:.2} us; other thread's isolate, copy + round trip {hop:.2} us");
    }

    // 4. One system over 4M rows: one isolate, against the rows split across 2, 4, 8 isolates.
    let n = 4 << 20;
    let mut pos = vec![0f32; n];
    let mut vel = vec![1f32; n];
    let frames = 40;
    for k in [1usize, 2, 4, 8] {
        let ws = &workers[..k];
        let chunk = n / k;
        let mut times = Vec::new();
        for frame in 0..frames + 5 {
            let t = Instant::now();
            for (i, w) in ws.iter().enumerate() {
                let p = unsafe { pos.as_mut_ptr().add(i * chunk) };
                let v = unsafe { vel.as_mut_ptr().add(i * chunk) };
                w.tx.send(Job::System(Span(p, v, chunk))).unwrap();
            }
            for w in ws {
                w.rx.recv().unwrap();
            }
            if frame >= 5 {
                times.push(t.elapsed().as_secs_f64() * 1e3);
            }
        }
        println!("pos += vel * dt over {n} rows, {k} isolate(s): {:.2} ms per frame (median)", median(times));
    }
    assert!(pos[0] > 0.0 && pos[n - 1] == pos[0]);
    std::thread::sleep(Duration::from_millis(10));
}
