//! The spare engine instance a restart takes (stop-rebuild-split, decision 3): booted in a frame
//! of its own under `dev`, holding nothing of the instance it replaces.

use crate::common;

use bevy_ts::generation::{self, ReloadStatus};
use common::*;

/// Counts its evaluations in a global: a fresh instance logs 1.
const MARK: &str = r#"globalThis.mark = (globalThis.mark ?? 0) + 1; console.log("mark " + globalThis.mark);"#;

fn marks(app: &bevy::prelude::App) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.starts_with("mark ")).collect()
}

fn restart(app: &mut bevy::prelude::App) {
    let r = generation::apply(app.world_mut(), set(&[("mark.js", MARK)]), true);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

#[test]
fn a_global_set_before_a_restart_is_gone_after() {
    let mut app = app();
    load(&mut app, &[("mark.js", MARK)]);
    // The spare boots in this frame.
    updates(&mut app, 2);
    restart(&mut app);
    updates(&mut app, 2);
    assert_eq!(marks(&app), ["mark 1", "mark 1"], "the restart's instance never saw the global");
}

#[test]
fn two_restarts_with_no_frame_between_each_get_a_fresh_instance() {
    let mut app = app();
    load(&mut app, &[("mark.js", MARK)]);
    updates(&mut app, 1);
    // The first takes the spare, the second boots its own.
    restart(&mut app);
    restart(&mut app);
    updates(&mut app, 3);
    // The retired instances are dropped and a new spare taken.
    restart(&mut app);
    updates(&mut app, 2);
    assert_eq!(marks(&app), ["mark 1"; 4]);
}
