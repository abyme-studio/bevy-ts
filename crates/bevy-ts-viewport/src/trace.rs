//! A per-frame timeline for pacing work: with `BEVY_TS_VIEWPORT_TRACE=<file>`, each tick, wake,
//! update, slot pick, submit and GPU completion is a line `<ns> <event> <a> <b>` in the file, the
//! time on `now_ns`'s clock. Off, each call is one relaxed load.

use crate::shared::now_ns;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

fn file() -> Option<&'static Mutex<File>> {
    FILE.get_or_init(|| std::env::var_os("BEVY_TS_VIEWPORT_TRACE").and_then(|p| File::create(p).ok()).map(Mutex::new)).as_ref()
}

/// Records `event` with two numbers now.
pub(crate) fn mark(event: &str, a: u64, b: u64) {
    if let Some(f) = file() {
        let line = format!("{} {event} {a} {b}\n", now_ns());
        let _ = f.lock().unwrap().write_all(line.as_bytes());
    }
}
