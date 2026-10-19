mod common;

use bevy::prelude::*;
use common::*;

#[test]
fn a_retained_view_reads_length_zero_after_the_run() {
    let mut app = app();
    app.world_mut().spawn(Transform::from_xyz(1.0, 2.0, 3.0));
    load(
        &mut app,
        &[(
            "views.js",
            r#"
            import { app } from "bevy-ts";
            import { Transform } from "bevy-ts/bevy";
            let kept = null;
            let keptTicks = null;
            app.system("keep", {
              write: [Transform],
              run(q) {
                if (kept) console.log(`after: ${kept.length} ${keptTicks.length} ${kept.buffer.byteLength}`);
                const t = q.tables[0].split(Transform);
                kept = t.f32;
                keptTicks = t.ticks;
                console.log(`during: ${kept.length} y=${kept[Transform.offsets.translation / 4 + 1]}`);
              },
            });
            "#,
        )],
    );
    // Console output is pumped in the next frame's `First`.
    for _ in 0..3 {
        app.update();
    }
    let m = messages(&app);
    assert!(m.iter().any(|l| l.starts_with("during: ") && l.ends_with("y=2")), "{m:?}");
    assert!(m.iter().any(|l| l == "after: 0 0 0"), "{m:?}");
}

#[test]
fn the_sandbox_feature_fails_the_build() {
    // The feature only switches on a compile_error!; check it is there and says why. Building it
    // for real from inside `cargo test` would wait on cargo's own build lock.
    let src = include_str!("../src/lib.rs");
    let gate = src.find("#[cfg(feature = \"v8_enable_sandbox\")]\ncompile_error!(").expect("sandbox gate");
    assert!(src[gate..].contains("design.md"));
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("v8_enable_sandbox = []"));
}
