//! Task 8.2: the eight demo steps of `openspec/changes/archive/2026-10-21-tsx/design.md` ("First demo") against
//! `examples/arena/src`, in V8, headless and in-process: reload latency (compile and apply, no
//! watcher) and tsx host calls per frame. The enemies are placed by `assets/levels/arena.bsn`
//! (levels task 6.1), so their edits are level edits. Prints one line per step; `examples/arena/WALK.md`
//! records a release run:
//! `cargo test --release -p bevy-ts-cli --test it arena_walk:: -- --nocapture`.

use crate::common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy_ts::generation::{ReloadReport, ReloadStatus};
use bevy_ts::plugin_api;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Walk {
    app: App,
    dir: PathBuf,
}

impl Walk {
    fn start() -> Walk {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/src");
        let files: Vec<(String, String)> = ["enemy.tsx", "player.tsx", "hud.tsx", "game.tsx"]
            .iter()
            .map(|f| (format!("src/{f}"), std::fs::read_to_string(src.join(f)).unwrap()))
            .collect();
        let levels = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/assets/levels");
        let mut files = files;
        for f in ["arena.bsn", "room.bsn"] {
            files.push((format!("assets/levels/{f}"), std::fs::read_to_string(levels.join(f)).unwrap()));
        }
        let mut list: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        list.push(("package.json", "{}"));
        let dir = project("arena-walk", &list);
        let mut app = app(&dir.join("src/game.tsx"));
        let assets = dir.join("assets").canonicalize().unwrap();
        app.add_plugins((AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..Default::default() }, RemotePlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .add_plugins(bevy_ts_levels::engine::LevelsPlugin);
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().asset_root = assets;
        bevy_ts_tsx::protocol::add_methods(&mut app);
        Walk { app, dir }
    }

    fn reload(&mut self, restart: bool) -> (ReloadReport, f64) {
        let t = Instant::now();
        let r = reload(&mut self.app, restart);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        self.settle();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        (r, ms)
    }

    /// Frames until the level has loaded and reconciled.
    fn settle(&mut self) {
        for _ in 0..500 {
            let levels = self.app.world().resource::<bevy_ts_levels::engine::Levels>();
            if !levels.roots.is_empty() && levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
                return;
            }
            self.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the level never loaded: {:?}", messages(&self.app));
    }

    /// Edits the level file and reloads it as the watcher would; returns the time to the
    /// reconciled world.
    fn edit_level(&mut self, from: &str, to: &str) -> f64 {
        let p = self.dir.join("assets/levels/arena.bsn");
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains(from), "arena.bsn has no {from:?}");
        let next = s.replacen(from, to, 1);
        std::fs::write(&p, &next).unwrap();
        let hash = bevy_ts_levels::engine::content_hash(next.as_bytes());
        let t = Instant::now();
        self.app.world().resource::<AssetServer>().reload("levels/arena.bsn");
        for _ in 0..500 {
            self.app.update();
            let levels = self.app.world().resource::<bevy_ts_levels::engine::Levels>();
            if levels.roots["levels/arena.bsn"].applied.get("levels/arena.bsn") == Some(&hash) {
                return t.elapsed().as_secs_f64() * 1e3;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("arena.bsn never reloaded: {:?}", messages(&self.app));
    }

    fn enemy(&mut self, name: &str) -> Option<Entity> {
        self.node("level:levels/arena.bsn", &[name])
    }

    fn edit(&mut self, file: &str, from: &str, to: &str) -> f64 {
        let p = self.dir.join("src").join(file);
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains(from), "{file} has no {from:?}");
        std::fs::write(&p, s.replacen(from, to, 1)).unwrap();
        self.reload(false).1
    }

    fn node(&mut self, root: &str, path: &[&str]) -> Option<Entity> {
        let world = self.app.world_mut();
        world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.root == root && n.key_path == path).map(|(e, _)| e)
    }

    fn arena(&mut self, key: &str) -> Option<Entity> {
        self.node("arena", &[key])
    }

    fn text(&mut self) -> String {
        let hud = self.node("arena", &["Hud#0", "hp"]).or_else(|| {
            let world = self.app.world_mut();
            world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path.last().is_some_and(|k| k == "hp")).map(|(e, _)| e)
        });
        self.app.world().get::<Text>(hud.expect("the HUD's hp text")).unwrap().0.clone()
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

    fn calls(&self) -> u64 {
        self.app.world().resource::<TsxTree>().reactive.calls
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = *self.app.world().resource::<RemoteMethods>().get(method).unwrap();
        let RemoteMethodSystemId::Instant(id) = id else { panic!("{method} watches") };
        self.app.world_mut().run_system_with(id, Some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn material(&self, e: Entity) -> AssetId<StandardMaterial> {
        self.app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.id()
    }
}

fn line(step: u32, what: &str) {
    println!("{step} {what}");
}

#[test]
fn the_arena_demo_walks_its_eight_steps() {
    let mut w = Walk::start();

    // 1. Two red cubes from arena.bsn; the HUD reads HP 100 / 100.
    let (_, ms) = w.reload(false);
    updates(&mut w.app, 2);
    let (a, b) = (w.enemy("EnemyA").expect("enemy a"), w.enemy("EnemyB").expect("enemy b"));
    assert!(w.app.world().get::<Mesh3d>(a).is_some() && w.app.world().get::<Mesh3d>(b).is_some());
    assert_eq!(w.text(), "HP 100 / 100");
    line(1, &format!("two enemies from arena.bsn and a player mounted, HUD \"{}\"; first load {ms:.1} ms", w.text()));

    // 2. Damage: only the HUD text changes, with no host call on frames the player's Health
    // did not change.
    let text_id = w.app.world_mut().register_component::<Text>();
    let texts: Vec<Entity> = {
        let world = w.app.world_mut();
        world.query_filtered::<Entity, With<Text>>().iter(world).collect()
    };
    let ticks = |w: &Walk| texts.iter().map(|e| plugin_api::change_ticks(w.app.world(), *e, text_id).unwrap().changed).collect::<Vec<_>>();
    let before_ticks = ticks(&w);
    let before = w.calls();
    let mut per_frame = Vec::new();
    for _ in 0..60 {
        let c = w.calls();
        w.app.update();
        per_frame.push(w.calls() - c);
    }
    let player = w.arena("PlayerUnit#0").expect("the player");
    let hp = w.health(player).0;
    let frames_with_calls = per_frame.iter().filter(|c| **c > 0).count();
    assert_eq!(hp, 80.0, "two enemies within reach, 5 each, twice in 60 frames");
    assert_eq!(w.text(), "HP 80 / 100");
    assert_eq!(frames_with_calls, 2, "{per_frame:?}");
    let moved = before_ticks.iter().zip(ticks(&w)).filter(|(x, y)| *x != y).count();
    assert_eq!(moved, 1, "only the hp text was written");
    line(2, &format!("60 frames of damage: HUD \"{}\"; {} host calls, on the 2 frames the player was hit (none on the other 58); 1 Text written", w.text(), w.calls() - before));

    // 3. b's @hp: 250 -> 400 in arena.bsn: max follows, hp (initial) stays, no cube jumps.
    let tb = *w.app.world().get::<Transform>(b).unwrap();
    let ms = w.edit_level("@hp: 250.0", "@hp: 400.0");
    assert_eq!(w.health(b), (250.0, 400.0));
    // The reload runs frames, so bob has moved y; x and z are where they were.
    let now = w.app.world().get::<Transform>(b).unwrap().translation;
    assert_eq!((now.x, now.z), (tb.translation.x, tb.translation.z));
    assert_eq!(w.enemy("EnemyB"), Some(b));
    line(3, &format!("b @hp: 250.0 -> 400.0 in arena.bsn: Health (hp, max) = {:?}, Transform untouched; reload {ms:.1} ms", w.health(b)));

    // 4. Recolour Enemy: both enemies' material changes, positions (moved by bob) kept.
    updates(&mut w.app, 5);
    let (ta, tb) = (*w.app.world().get::<Transform>(a).unwrap(), *w.app.world().get::<Transform>(b).unwrap());
    let (ma, mb) = (w.material(a), w.material(b));
    let ms = w.edit("enemy.tsx", "#cc3333", "#33cc33");
    assert!(w.material(a) != ma && w.material(b) != mb && w.material(a) == w.material(b));
    // The level re-expands at the frame after the generation, so bob has run once more: x and
    // z are the level's, and y is bob's, never reset to the prefab's 0.
    let (na, nb) = (w.app.world().get::<Transform>(a).unwrap().translation, w.app.world().get::<Transform>(b).unwrap().translation);
    assert_eq!((na.x, na.z, nb.x, nb.z), (ta.translation.x, ta.translation.z, tb.translation.x, tb.translation.z));
    assert!(na.y != 0.0 && nb.y != 0.0);
    line(4, &format!("recolour Enemy: both enemies share the new material; bobbed y kept (a.y {:.2}); reload {ms:.1} ms", ta.translation.y));

    // 5. A system kills a; an unrelated save leaves it dead; a restart brings it back.
    w.set_hp(a, 0.0);
    w.app.update();
    assert!(w.app.world().get_entity(a).is_err(), "cull despawned a");
    let ms = w.edit("game.tsx", "// The arena:", "// The arena (edited):");
    assert!(w.enemy("EnemyA").is_none(), "not respawned");
    let tree = w.rpc("bevy-ts/tsx_tree", json!(null));
    let tomb = tree["roots"].as_array().unwrap().iter().flat_map(|r| r["nodes"].as_array().unwrap().iter()).find(|n| n["keyPath"] == json!(["EnemyA"])).unwrap().clone();
    assert_eq!(tomb["tombstoned"], true);
    let (_, restart_ms) = w.reload(true);
    w.app.update();
    let a2 = w.enemy("EnemyA").expect("back after restart");
    assert_eq!(w.health(a2), (100.0, 100.0));
    line(5, &format!("cull despawned a; an unrelated save left it tombstoned ({ms:.1} ms); restart: true brought it back at hp 100 ({restart_ms:.1} ms)"));

    // 6. Add enemy c to arena.bsn: one instance (the enemy and its eye) spawns; a and b keep
    // their entities.
    let (a, b) = (a2, w.enemy("EnemyB").unwrap());
    let ms = w.edit_level("--\n// Two copies of one room", "--\n#EnemyC\n@enemy::Enemy\nTransform { translation: Vec3 { z: -8.0 } }\n--\n// Two copies of one room");
    let last = w.app.world().resource::<TsxTree>().last.clone();
    assert_eq!((last.spawned, last.despawned), (2, 0));
    assert_eq!((w.enemy("EnemyA"), w.enemy("EnemyB")), (Some(a), Some(b)));
    assert!(w.enemy("EnemyC").is_some());
    line(6, &format!("add #EnemyC to arena.bsn: {} spawned (enemy and eye), a and b unchanged; reload {ms:.1} ms", last.spawned));

    // 7. Click Spawn: an enemy mounts from the handler through the tsx op stream.
    w.app.update();
    let spawn = w.node("arena", &["Hud#0", "spawn"]).expect("the Spawn button");
    {
        use bevy::camera::NormalizedRenderTarget;
        use bevy::picking::backend::HitData;
        use bevy::picking::events::{Pointer, PointerClick};
        use bevy::picking::pointer::{Location, PointerButton, PointerId};
        let pointer = Pointer::new(PointerId::Mouse, Location { target: NormalizedRenderTarget::None { width: 1, height: 1 }, position: Vec2::new(20.0, 40.0) });
        let hit = HitData::new(Entity::PLACEHOLDER, 0.0, None, None);
        w.app.world_mut().trigger(PointerClick { entity: spawn, pointer, button: PointerButton::Primary, hit, duration: std::time::Duration::ZERO, count: 1 });
    }
    let c = w.calls();
    w.app.update();
    let runtime: Vec<Vec<String>> = {
        let world = w.app.world_mut();
        world.query::<&TsxNode>().iter(world).filter(|n| n.root == "$runtime" && n.key_path.len() == 1).map(|n| n.key_path.clone()).collect()
    };
    assert_eq!(runtime.len(), 1, "{runtime:?}");
    line(7, &format!("click Spawn: {:?} mounted under the runtime root in {} host call", runtime[0], w.calls() - c));

    // 8. tsx_node for b: its chain ends at arena.bsn's `@enemy::Enemy` entry; hp initial, max controlled.
    let n = w.rpc("bevy-ts/tsx_node", json!({ "entity": b.to_bits() }));
    let chain: Vec<String> = n["source"].as_array().unwrap().iter().map(|s| format!("{}:{}", s["file"].as_str().unwrap().rsplit('/').next().unwrap(), s["line"])).collect();
    let level_line = std::fs::read_to_string(w.dir.join("assets/levels/arena.bsn")).unwrap().lines().position(|l| l.contains("@enemy::Enemy { @hp")).unwrap() + 1;
    assert_eq!(chain.last().unwrap(), &format!("arena.bsn:{level_line}"));
    let h = &n["components"]["enemy::Health"]["fields"];
    assert_eq!((h["hp"]["provenance"].as_str(), h["max"]["provenance"].as_str()), (Some("initial"), Some("controlled")));
    line(8, &format!("tsx_node b: source {chain:?}; Health.hp {}, Health.max {}", h["hp"], h["max"]));
}
