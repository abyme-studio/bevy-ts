//! Levels task 6.2: the seven demo steps of `openspec/changes/levels/design.md` ("First demo")
//! against `examples/arena`, in V8, headless and in-process, under both `levels.prefabEdits`
//! policies. Prints each step and the file diff every write made; `examples/arena/LEVELS_WALK.md`
//! records a release run:
//! `cargo static test --release -p bevy-ts-cli --test it levels_walk:: -- --nocapture`.

use crate::common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy_ts::plugin_api;
use bevy_ts_levels::engine::{Levels, LevelsPlugin, content_hash};
use bevy_ts_levels::{PrefabEdits, WriteResults, route, write_from};
use bevy_ts_tsx::TsxNode;
use common::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Instant;

const LEVEL: &str = "levels/arena.bsn";

struct Walk {
    app: App,
    dir: PathBuf,
}

/// The lines that differ, as a diff hunk would show them.
fn diff(a: &str, b: &str) -> Vec<String> {
    let (al, bl): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let mut start = 0;
    while start < al.len() && start < bl.len() && al[start] == bl[start] {
        start += 1;
    }
    let (mut ea, mut eb) = (al.len(), bl.len());
    while ea > start && eb > start && al[ea - 1] == bl[eb - 1] {
        ea -= 1;
        eb -= 1;
    }
    al[start..ea].iter().map(|l| format!("-{l}")).chain(bl[start..eb].iter().map(|l| format!("+{l}"))).collect()
}

fn show(diff: &[String]) {
    for l in diff {
        println!("    {l}");
    }
}

impl Walk {
    fn start(name: &str) -> Walk {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
        let mut files: Vec<(String, String)> = ["enemy.tsx", "player.tsx", "hud.tsx", "game.tsx"]
            .iter()
            .map(|f| (format!("src/{f}"), std::fs::read_to_string(root.join("src").join(f)).unwrap()))
            .collect();
        for f in ["arena.bsn", "room.bsn"] {
            files.push((format!("assets/levels/{f}"), std::fs::read_to_string(root.join("assets/levels").join(f)).unwrap()));
        }
        let mut list: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        list.push(("package.json", "{}"));
        let dir = project(name, &list);
        let mut app = app(&dir.join("src/game.tsx"));
        let assets = dir.join("assets").canonicalize().unwrap();
        app.add_plugins((AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..Default::default() }, RemotePlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .add_plugins(LevelsPlugin);
        app.world_mut().resource_mut::<Levels>().asset_root = assets;
        bevy_ts_levels::protocol::add_methods(&mut app);
        let mut w = Walk { app, dir };
        let r = reload(&mut w.app, false);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        w.settle();
        w
    }

    fn settle(&mut self) {
        settle_levels(&mut self.app);
    }

    fn file(&self) -> String {
        std::fs::read_to_string(self.dir.join("assets").join(LEVEL)).unwrap()
    }

    /// Saves the level as an agent would, and waits for the world to follow (or an error).
    fn save(&mut self, text: &str) -> f64 {
        std::fs::write(self.dir.join("assets").join(LEVEL), text).unwrap();
        let hash = content_hash(text.as_bytes());
        let errors = self.errors().len();
        let t = Instant::now();
        self.app.world().resource::<AssetServer>().reload(LEVEL);
        until(&mut self.app, &format!("{LEVEL} never reloaded"), |app| {
            let failed = logs(app).iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).count() > errors;
            app.world().resource::<Levels>().roots[LEVEL].applied.get(LEVEL) == Some(&hash) || failed
        });
        t.elapsed().as_secs_f64() * 1e3
    }

    fn errors(&self) -> Vec<String> {
        logs(&self.app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| format!("{} {}", e.location.map(|l| l.to_string()).unwrap_or_default(), e.message)).collect()
    }

    fn node(&mut self, path: &[&str]) -> Option<Entity> {
        let root = format!("level:{LEVEL}");
        let world = self.app.world_mut();
        world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.root == root && n.key_path == path).map(|(e, _)| e)
    }

    fn translation(&self, e: Entity) -> Vec3 {
        self.app.world().get::<Transform>(e).unwrap().translation
    }

    fn health(&self, e: Entity) -> (f32, f32) {
        let (id, _) = plugin_api::component_by_name(self.app.world(), "enemy::Health").unwrap();
        let p = self.app.world().get_by_id(e, id).unwrap();
        // SAFETY: Health is two f32s.
        let b = unsafe { std::slice::from_raw_parts(p.as_ptr(), 8) };
        (f32::from_le_bytes(b[0..4].try_into().unwrap()), f32::from_le_bytes(b[4..8].try_into().unwrap()))
    }

    fn set_hp(&mut self, e: Entity, hp: f32) {
        let (id, _) = plugin_api::component_by_name(self.app.world(), "enemy::Health").unwrap();
        let mut m = self.app.world_mut().get_mut_by_id(e, id).unwrap();
        // SAFETY: Health is two f32s.
        unsafe { std::ptr::copy_nonoverlapping(hp.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
    }

    /// A gizmo drag of `e`'s translation along one axis to `to`, released: the write's result.
    fn drag(&mut self, e: Entity, axis: usize, to: f32) -> (bevy_ts_levels::WriteResult, f64) {
        let before = self.translation(e);
        let mut after = before;
        after[axis] = to;
        self.app.world_mut().get_mut::<Transform>(e).unwrap().translation = after;
        let t = Instant::now();
        write_from(self.app.world_mut(), 1, e, "Transform", "translation", Some(json!([before.x, before.y, before.z])), json!([after.x, after.y, after.z])).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let r = self.app.world_mut().resource_mut::<WriteResults>().drain().pop().expect("a result");
        (r, ms)
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = *self.app.world().resource::<RemoteMethods>().get(method).unwrap();
        let RemoteMethodSystemId::Instant(id) = id else { panic!("{method} watches") };
        self.app.world_mut().run_system_with(id, Some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }
}

#[test]
fn the_levels_demo_walks_its_seven_steps() {
    // 1. `bevy-ts dev`: the same two enemies, now from arena.bsn.
    let t = Instant::now();
    let mut w = Walk::start("levels-walk");
    let load = t.elapsed().as_secs_f64() * 1e3;
    let (a, b) = (w.node(&["EnemyA"]).expect("a"), w.node(&["EnemyB"]).expect("b"));
    assert_eq!((w.translation(a).x, w.translation(b).x), (-3.0, 3.0));
    println!("1 two enemies from arena.bsn (and a guard in each of two rooms); load {load:.1} ms");

    // 2. Drag enemy b along x: one changed line; the cube does not jump on the reload.
    let before = w.file();
    let (r, ms) = w.drag(b, 0, 5.0);
    assert_eq!(r.status, "written", "{r:?}");
    let d = diff(&before, &w.file());
    assert_eq!(d.len(), 2, "{d:?}");
    w.app.update();
    assert_eq!(w.translation(b).x, 5.0);
    assert_eq!(w.node(&["EnemyB"]), Some(b));
    println!("2 drag b along x: {} ({ms:.1} ms, the reconcile included); b stays at x 5", r.status);
    show(&d);

    // 3. Drag b's eye under `override`: a Children override under #EnemyB; a's eye unchanged.
    let eye_b = w.node(&["EnemyB", "eye"]).expect("b's eye");
    let eye_a = w.node(&["EnemyA", "eye"]).expect("a's eye");
    let r = route(w.app.world(), eye_b, "Transform", "translation").expect("a bsn route");
    assert_eq!(r["policy"], "override");
    let before = w.file();
    let (res, _) = w.drag(eye_b, 1, 0.9);
    assert_eq!(res.status, "written", "{res:?}");
    let d = diff(&before, &w.file());
    assert!(d.iter().any(|l| l.contains("#eye")), "{d:?}");
    assert!((w.translation(eye_b).y - 0.9).abs() < 1e-5 && (w.translation(eye_a).y - 0.6).abs() < 1e-5);
    println!("3 drag b's eye (override): Children [ #eye … ] under #EnemyB, a's eye unchanged");
    show(&d);
    // "Apply to prefab": TSX is the next owner, so the editor writes enemy.tsx, then the override goes.
    let apply = w.rpc("bevy-ts/level_apply", json!({ "entity": eye_b.to_bits(), "component": "Transform", "field": "translation" }));
    assert_eq!(apply["status"], "tsx", "{apply}");
    let element = apply["edit"]["element"].clone();
    assert!(element["file"].as_str().unwrap().ends_with("enemy.tsx"));
    let tsx_before = std::fs::read_to_string(w.dir.join("src/enemy.tsx")).unwrap();
    let tsx_after = tsx_before.replacen("translation={[0, 0.6, 0.5]}", "translation={[0, 0.9, 0.5]}", 1);
    std::fs::write(w.dir.join("src/enemy.tsx"), &tsx_after).unwrap();
    assert!(reload(&mut w.app, false).errors.is_empty());
    w.settle();
    let before = w.file();
    let then = w.rpc("bevy-ts/level_write", apply["then"].clone());
    assert_eq!(then["status"], "written", "{then}");
    w.settle();
    let d = diff(&before, &w.file());
    assert!((w.translation(eye_a).y - 0.9).abs() < 1e-5 && (w.translation(eye_b).y - 0.9).abs() < 1e-5, "both eyes follow the prefab");
    println!("  apply to prefab: the editor rewrote {}:{} and level_write removed the override; both eyes at y 0.9", element["file"].as_str().unwrap().rsplit('/').next().unwrap(), element["line"]);
    show(&diff(&tsx_before, &tsx_after));
    show(&d);
    // Under `source`, the same drag routes to the editor (tsx): levels has no say.
    w.app.world_mut().resource_mut::<Levels>().prefab_edits = PrefabEdits::Source;
    assert!(route(w.app.world(), eye_b, "Transform", "translation").is_none());
    assert_eq!(route(w.app.world(), b, "Transform", "translation").unwrap()["policy"], "source", "the instance root stays the level's");
    w.app.world_mut().resource_mut::<Levels>().prefab_edits = PrefabEdits::Override;
    println!("  under source: b's eye routes tsx (the editor rewrites enemy.tsx); b itself still routes bsn");

    // 4. An agent edits @hp: 250.0 to 400.0 by hand: max follows, the damaged hp stays.
    w.set_hp(b, 90.0);
    let before = w.file();
    let text = before.replacen("@hp: 250.0", "@hp: 400.0", 1);
    let ms = w.save(&text);
    assert_eq!(w.health(b), (90.0, 400.0));
    println!("4 an agent sets @hp: 400.0: Health (hp, max) = {:?}; reload {ms:.1} ms", w.health(b));
    show(&diff(&before, &text));

    // 5. While b is dragged, an agent renames #EnemyA: the drag replays by name path (rebased).
    let renamed = w.file().replacen("#EnemyA", "#Grunt", 1);
    std::fs::write(w.dir.join("assets").join(LEVEL), &renamed).unwrap();
    let (r, _) = w.drag(b, 0, 6.0);
    assert_eq!(r.status, "rebased", "{r:?}");
    assert!(w.file().contains("#Grunt") && w.file().contains("x: 6.0"), "{}", w.file());
    println!("5 rename #EnemyA mid-drag: {}", r.status);
    show(&diff(&renamed, &w.file()));
    // If the agent deleted #EnemyB instead, the drag is a conflict and b goes, as the file says.
    let start = w.file();
    let from = start.find("#EnemyB").unwrap();
    let to = from + start[from..].find("--\n").unwrap() + 3;
    let deleted = format!("{}{}", &start[..from], &start[to..]);
    std::fs::write(w.dir.join("assets").join(LEVEL), &deleted).unwrap();
    let (r, _) = w.drag(b, 0, 7.0);
    assert_eq!((r.status, r.reason.as_deref()), ("conflict", Some("target removed")), "{r:?}");
    assert!(w.app.world().get_entity(b).is_err(), "b follows the disk");
    println!("  delete #EnemyB mid-drag: {} ({}); b despawned", r.status, r.reason.unwrap());

    // 6. A typo in a component name: under `bevy-ts dev` the save applies, the typo is a
    // placeholder on a, and the warning names its place.
    let a = w.node(&["Grunt"]).expect("a, renamed");
    let good = w.file();
    let typo = good.replacen("Transform { translation: Vec3 { x: -3.0", "Transfrom { translation: Vec3 { x: -3.0", 1);
    w.save(&typo);
    assert!(w.errors().iter().all(|e| !e.contains("Transfrom")), "{:?}", w.errors());
    assert_eq!(w.node(&["Grunt"]), Some(a), "the same entity");
    let m = w.app.world().get::<bevy_ts_levels::engine::MissingComponents>(a).expect("a placeholder entry").0[0].clone();
    assert_eq!(m.component, "Transfrom");
    let e = format!("{}:{} {}", m.location.file, m.location.line, m.message);
    assert!(e.starts_with("levels/arena.bsn:") && e.contains("did you mean Transform?"), "{e}");
    println!("6 a typo: {e}; a placeholder, the rest applies");
    w.save(&good);
    assert!(w.app.world().get::<bevy_ts_levels::engine::MissingComponents>(a).is_none(), "fixed, the marker goes");

    // 7. `bevy-ts levels check --upstream arena.bsn` lists E1 and E2.
    let (lines, ok) = bevy_ts_levels::check::check_source("arena.bsn", &good, true);
    assert!(ok);
    assert!(lines.iter().any(|l| l.contains(" E1 ")) && lines.iter().any(|l| l.contains(" E2 ")), "{lines:?}");
    println!("7 levels check --upstream arena.bsn:");
    for l in lines {
        println!("    {l}");
    }
}
