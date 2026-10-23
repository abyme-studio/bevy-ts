use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use common::*;

#[test]
fn a_ts_graph_loads_and_a_throw_is_located_in_ts() {
    let dir = project(
        "throw",
        &[
            (
                "game.ts",
                r#"import { app, Time, Transform, Update } from "bevy-ts";
import type { Mut, Query, Res } from "bevy-ts";
import { scale } from "./util.js";

type Speed = number;
const speed: Speed = scale(2);

export function boom(q: Query<[Mut<Transform>]>, time: Res<Time>): void {
  const label: string = "boom";
  if (time.elapsed_secs_f64 > 0) throw new Error(`${label} ${speed}`);
}

app.addSystems(Update, boom);
"#,
            ),
            ("util.ts", "export function scale(n: number): number {\n  return n * 3;\n}\n"),
        ],
    );
    let mut app = app(&dir.join("game.ts"));
    app.world_mut().spawn(Transform::default());
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    let e = logs(&app).into_iter().find(|e| e.message.contains("boom 6")).expect("the throw is logged");
    let loc = e.location.expect("located");
    assert!(loc.file.ends_with("game.ts"), "{loc:?}");
    assert_eq!(loc.line, 10, "{loc:?}");
    assert!(e.message.contains("game.ts:10"), "stack mapped to TS: {}", e.message);
}

#[test]
fn a_type_error_does_not_block_a_run() {
    let dir = project(
        "typeerror",
        &[("game.ts", "import { app, Startup } from \"bevy-ts\";\nconst n: number = \"text\";\nexport function s() { console.log(`n=${n}`); }\napp.addSystems(Startup, s);\n")],
    );
    let mut app = app(&dir.join("game.ts"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    assert!(messages(&app).contains(&"n=text".to_string()), "{:?}", messages(&app));
}

#[test]
fn a_bare_import_fails_naming_it_and_its_line() {
    let dir = project("bare", &[("game.ts", "import { app } from \"bevy-ts\";\nimport _ from \"lodash\";\n")]);
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("lodash"));
    assert_eq!(r.errors[0].location.as_ref().unwrap().line, 2);
}

#[test]
fn tsx_elements_carry_their_ts_source() {
    let dir = project(
        "tsx",
        &[(
            "game.tsx",
            "import { app, Startup } from \"bevy-ts\";\n\nexport function s() {\n  const el: any = <entity name=\"a\" />;\n  console.log(JSON.stringify(el.source));\n}\napp.addSystems(Startup, s);\n",
        )],
    );
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let line = messages(&app).into_iter().find(|m| m.contains("\"line\"")).expect("element source logged");
    assert!(line.contains("game.tsx"), "{line}");
    assert!(line.contains("\"line\":4"), "{line}");
    assert!(line.contains("\"column\":19"), "{line}");
}

#[test]
fn repeated_errors_are_logged_once_per_second_with_a_count() {
    let dir = project(
        "repeat",
        &[(
            "game.ts",
            "import { app, Transform, Update } from \"bevy-ts\";\nimport type { Mut, Query } from \"bevy-ts\";\nexport function again(q: Query<[Mut<Transform>]>) { throw new Error(\"again\"); }\napp.addSystems(Update, again);\n",
        )],
    );
    let mut app = app(&dir.join("game.ts"));
    app.world_mut().spawn(Transform::default());
    // The dedup window is a wall-clock second: a loaded machine can spend several on 50 frames, so
    // the lines are bounded by the seconds taken (from before the reload, which runs the first
    // frame), rounded up, not fixed at one.
    let start = std::time::Instant::now();
    reload(&mut app, false);
    updates(&mut app, 50);
    let secs = start.elapsed().as_secs_f64().ceil() as usize;
    let lines: Vec<_> = logs(&app).into_iter().filter(|e| e.message.contains("Error: again")).collect();
    assert!((1..=1 + secs).contains(&lines.len()), "{} lines in {secs} s: {:?}", lines.len(), messages(&app));
    assert!(lines.iter().map(|e| e.count).sum::<u32>() <= 50, "{lines:?}");
    let systems = app.world().resource::<bevy_ts::systems::TsSystems>();
    assert!(systems.iter().next().unwrap().last_error.is_some());
}
