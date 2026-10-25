//! `bevy-ts/session` in an app with the V8 backend: values kept across reloads, failed ones too,
//! emptied by a restart and a Stop, shared with Rust and between modules, and its limits.

use crate::common;

use bevy::prelude::*;
use bevy_ts::SessionBytes;
use common::*;

fn session_app(dir: &std::path::Path, entry: &str) -> App {
    let mut app = app(&dir.join(entry));
    app.add_plugins(bevy_ts_cli::session::ScriptSessionPlugin);
    app
}

fn loaded(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, App) {
    let dir = project(name, files);
    let mut app = session_app(&dir, files[0].0);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    (dir, app)
}

// The first update after a load makes its stages; TS systems run from the second.

/// The messages that start with `prefix`, with it stripped.
fn tagged(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_string)).collect()
}

fn store(app: &App) -> &SessionBytes {
    app.world().resource::<SessionBytes>()
}

const PUT_64K: &str = r#"import { app, Startup, Update } from "bevy-ts";
import type { SessionBytes } from "bevy-ts/session";

let hits = 0;
export function remember(session: SessionBytes) {
  const v = new Uint8Array(65536);
  for (let i = 0; i < v.length; i++) v[i] = (i * 7) & 255;
  session.put("heimdall/memory", v);
}
export function tick() {
  console.log(`hits ${hits++}`);
}
app.addSystems(Startup, remember);
app.addSystems(Update, tick);
"#;

const READ_64K: &str = r#"import { app, Update } from "bevy-ts";
import { get, type SessionBytes } from "bevy-ts/session";

let hits = 0;
export function recall(session: SessionBytes) {
  const v = get(session, "heimdall/memory");
  let sum = 0;
  if (v) for (let i = 0; i < v.length; i++) sum = (sum * 31 + v[i]) >>> 0;
  console.log(`recall ${v?.length} ${sum} ${hits++}`);
}
app.addSystems(Update, recall);
"#;

/// The checksum `READ_64K` logs, for the pattern `PUT_64K` stores.
fn checksum() -> u32 {
    (0..65536u32).fold(0u32, |s, i| s.wrapping_mul(31).wrapping_add((i * 7) & 255))
}

#[test]
fn put_reload_get() {
    let (dir, mut app) = loaded("session-put-reload", &[("game.ts", PUT_64K)]);
    updates(&mut app, 3);
    assert_ne!(tagged(&app, "hits ").last().map(String::as_str).unwrap_or("0"), "0", "the counter counted");
    assert_eq!(store(&app).size("heimdall/memory"), Some(65536));

    write(&dir, "game.ts", READ_64K);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(tagged(&app, "recall ").first().cloned(), Some(format!("65536 {} 0", checksum())), "the same bytes, and module state started again");
}

#[test]
fn survives_a_failed_reload() {
    let (dir, mut app) = loaded("session-failed-reload", &[("game.ts", PUT_64K)]);
    updates(&mut app, 2);
    write(&dir, "game.ts", "export function broken( {\n");
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Failed);
    assert_eq!(store(&app).size("heimdall/memory"), Some(65536), "a failed reload keeps it");
    write(&dir, "game.ts", READ_64K);
    assert!(reload(&mut app, false).errors.is_empty());
    updates(&mut app, 2);
    assert_eq!(tagged(&app, "recall ").first().cloned(), Some(format!("65536 {} 0", checksum())));
}

#[test]
fn restart_and_stop_empty_it() {
    let (_dir, mut app) = loaded("session-restart", &[("game.ts", PUT_64K)]);
    updates(&mut app, 2);
    let put = |app: &mut App| app.world_mut().resource_mut::<SessionBytes>().put("left/over", &[1]).unwrap();
    put(&mut app);
    assert_eq!(store(&app).count(), 2);
    // The restart applies the generation as if first, so its `Startup` puts its key again.
    assert!(reload(&mut app, true).errors.is_empty());
    assert_eq!((store(&app).count(), store(&app).size("left/over")), (1, None), "a restart empties it");

    put(&mut app);
    bevy_ts::generation::rebuild(app.world_mut()).unwrap();
    assert_eq!((store(&app).count(), store(&app).size("left/over")), (1, None), "a Stop empties it");
}

const SHARED: &str = r#"import { app, Update } from "bevy-ts";
import { get, type SessionBytes } from "bevy-ts/session";

export function share(session: SessionBytes) {
  console.log(`seed ${Array.from(get(session, "rust/seed") ?? []).join(",")}`);
  session.put("ts/answer", new Uint8Array([4, 2]));
}
app.addSystems(Update, share);
"#;

fn seed(mut store: ResMut<SessionBytes>) {
    store.put("rust/seed", &[1, 2, 3]).unwrap();
}

#[test]
fn rust_and_ts_share_it() {
    let (_dir, mut app) = loaded("session-rust", &[("game.ts", SHARED)]);
    app.add_systems(First, seed);
    updates(&mut app, 2);
    assert_eq!(tagged(&app, "seed ").first().map(String::as_str), Some("1,2,3"));
    assert_eq!(store(&app).get("ts/answer"), Some(&[4u8, 2][..]));
}

const METHODS: &str = r#"import { app, Update } from "bevy-ts";
import { get, type SessionBytes } from "bevy-ts/session";

export function methods(session: SessionBytes) {
  session.put("a", new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]));
  const short = new Uint8Array(4).fill(99);
  console.log(`short ${session.read("a", short)} ${Array.from(short).join(",")}`);
  const long = new Uint8Array(12);
  console.log(`long ${session.read("a", long)} ${Array.from(long).join(",")}`);
  console.log(`none ${session.read("zz", long)} ${session.size("zz")} ${get(session, "zz")}`);
  const copy = get(session, "a")!;
  copy[0] = 200;
  console.log(`copy ${get(session, "a")![0]} ${session.size("a")}`);
  session.put("b", new Uint8Array([7, 8]));
  console.log(`count ${session.count()} ${session.bytes()}`);
  console.log(`delete ${session.delete("a")} ${session.delete("a")} ${session.has("a")} ${session.has("b")} ${session.count()}`);
  session.clear();
  console.log(`clear ${session.count()} ${session.bytes()}`);
}
app.addSystems(Update, methods);
"#;

#[test]
fn methods() {
    let (_dir, mut app) = loaded("session-methods", &[("game.ts", METHODS)]);
    updates(&mut app, 2);
    let one = |p: &str| tagged(&app, p).first().cloned().unwrap_or_default();
    assert_eq!(one("short "), "10 99,99,99,99", "too short: copies nothing");
    assert_eq!(one("long "), "10 0,1,2,3,4,5,6,7,8,9,0,0");
    assert_eq!(one("none "), "undefined undefined undefined");
    assert_eq!(one("copy "), "0 10", "get's array is a copy");
    assert_eq!(one("count "), "2 14");
    assert_eq!(one("delete "), "true false false true 1");
    assert_eq!(one("clear "), "0 0");
}

const LIMITS: &str = r#"import { app, Update } from "bevy-ts";
import type { SessionBytes } from "bevy-ts/session";

function attempt(what: string, f: () => void) {
  try {
    f();
    console.log(`${what} stored`);
  } catch (e) {
    console.log(`${what} ${e.message}`);
  }
}
export function limits(session: SessionBytes) {
  attempt("empty", () => session.put("", new Uint8Array(1)));
  attempt("long", () => session.put("k".repeat(1025), new Uint8Array(1)));
  attempt("max", () => session.put("k".repeat(1024), new Uint8Array(1)));
  session.put("big", new Uint8Array(60 << 20));
  const before = `${session.count()} ${session.bytes()}`;
  attempt("over", () => session.put("heimdall/more", new Uint8Array(5 << 20)));
  console.log(`after ${before} ${session.count()} ${session.bytes()}`);
  attempt("float", () => session.put("f", new Float32Array(2) as unknown as Uint8Array));
  console.log(`float has ${session.has("f")}`);
}
app.addSystems(Update, limits);
"#;

#[test]
fn limits_and_types() {
    let (_dir, mut app) = loaded("session-limits", &[("game.ts", LIMITS)]);
    updates(&mut app, 2);
    let one = |p: &str| tagged(&app, p).first().cloned().unwrap_or_default();
    let empty = one("empty ");
    assert!(empty.contains("SessionBytes.put") && empty.contains("1 to 1024 bytes"), "{empty}");
    assert!(one("long ").contains("1025 bytes"), "{}", one("long "));
    assert_eq!(one("max "), "stored");
    let over = one("over ");
    assert!(over.contains("\"heimdall/more\"") && over.contains(&format!("{}-byte value", 5 << 20)) && over.contains("64 MiB"), "{over}");
    let after: Vec<String> = one("after ").split(' ').map(str::to_string).collect();
    assert_eq!((after[0..2].join(" ")), after[2..4].join(" "), "an overflowing put changes nothing");
    let float = one("float ");
    assert!(float.contains("SessionBytes.put") && float.contains("Uint8Array") && float.contains("Float32Array"), "{float}");
    assert_eq!(one("float has "), "false");
}

const UI: &str = r#"import { app, PreUpdate } from "bevy-ts";
import type { SessionBytes } from "bevy-ts/session";

export function select(session: SessionBytes) {
  session.put("heimdall/selection", new Uint8Array([42]));
}
app.addSystems(PreUpdate, select);
"#;

const CAMERA: &str = r#"import { app, Update } from "bevy-ts";
import { get, type SessionBytes } from "bevy-ts/session";

export function follow(session: SessionBytes) {
  console.log(`selection ${get(session, "heimdall/selection")?.[0]}`);
}
app.addSystems(Update, follow);
"#;

#[test]
fn two_modules_share_a_key() {
    let files = [("game.ts", "import \"./ui.ts\";\nimport \"./camera.ts\";\n"), ("ui.ts", UI), ("camera.ts", CAMERA)];
    let (_dir, mut app) = loaded("session-two-modules", &files);
    updates(&mut app, 2);
    assert_eq!(tagged(&app, "selection ").first().map(String::as_str), Some("42"), "camera.ts reads what ui.ts put, the same frame");
}
