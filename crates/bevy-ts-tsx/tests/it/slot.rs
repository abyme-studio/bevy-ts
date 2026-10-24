//! A `<For>` slot's host-call output applied in Rust, with a fake `tsx` entry in place of the JSX
//! runtime: what one call's mounts, unmounts and orders cost and write.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{ComponentDecl, EntryDecl, FieldDecl, Manifest, Section};
use bevy_ts::log::ScriptLog;
use bevy_ts_tsx::tree::ReconcileStats;
use bevy_ts_tsx::{TsxNode, TsxPlugin, TsxTree};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The slot's id, as the generation's description gives it.
const SLOT: u32 = 1;

/// Host-call responses the fake `tsx` entry returns, one per call; `{}` when none is queued.
type Replies = Arc<Mutex<VecDeque<Value>>>;

fn app() -> (App, Replies) {
    let fake = FakeHandle::default();
    let h = fake.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend()))).add_plugins(TsxPlugin);
    let replies: Replies = Arc::default();
    let r = replies.clone();
    // `<entity name="list"><entity key="head" /><For ... /><entity key="tail" /></entity>`
    let section = json!({ "roots": [{ "name": "ui", "nodes": [{
        "key": "list", "type": "entity",
        "children": [
            { "key": "head", "type": "entity" },
            { "key": "for#0", "type": "For", "slot": SLOT },
            { "key": "tail", "type": "entity" }
        ]
    }] }] });
    let health = ComponentDecl {
        name: "game::Health".into(),
        fields: vec![FieldDecl { name: "hp".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() }],
        ..Default::default()
    };
    let mut p = FakeProgram {
        manifest: Manifest { components: vec![health], ..Default::default() },
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(&section).unwrap(), location: None }],
        ..Default::default()
    };
    p.manifest.entries.push(EntryDecl { name: "tsx".into(), location: None });
    p.entries.insert(
        "tsx".into(),
        Box::new(move |call| {
            let reply = r.lock().unwrap().pop_front().unwrap_or_else(|| json!({}));
            call.output = serde_json::to_vec(&reply).unwrap();
            Ok(())
        }),
    );
    fake.push(p);
    let report = generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false);
    assert_eq!(report.status, ReloadStatus::Applied, "{:?}", report.errors);
    app.update();
    (app, replies)
}

/// One item's content: a row entity with a child.
fn row(child: bool) -> Value {
    let children = if child { vec![json!({ "key": "child", "type": "entity" })] } else { vec![] };
    json!([{ "key": "row", "type": "entity", "components": [{ "type": "game::Health", "kind": "ts", "props": { "hp": 1 } }], "children": children }])
}

/// Runs one host call returning `reply`; returns the reconciles it made.
fn call(app: &mut App, replies: &Replies, reply: Value) -> u64 {
    replies.lock().unwrap().push_back(reply);
    let before = app.world().resource::<TsxTree>().reconciles;
    app.world_mut().resource_mut::<TsxTree>().reactive.due = true;
    app.update();
    assert!(replies.lock().unwrap().is_empty(), "the call was made");
    app.world().resource::<TsxTree>().reconciles - before
}

fn mount(app: &mut App, replies: &Replies, keys: impl IntoIterator<Item = String>) -> u64 {
    let mounts: Vec<Value> = keys.into_iter().map(|k| json!([SLOT, k, row(true)])).collect();
    call(app, replies, json!({ "mounts": mounts }))
}

fn last(app: &App) -> ReconcileStats {
    app.world().resource::<TsxTree>().last.clone()
}

/// The entity of the node at `path` under the list's slot (`["a", "row"]`).
fn node(app: &mut App, path: &[&str]) -> Option<Entity> {
    let mut want: Vec<String> = vec!["list".into(), "for#0".into()];
    want.extend(path.iter().map(|s| s.to_string()));
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == want).map(|(e, _)| e)
}

fn list(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["list"]).map(|(e, _)| e).unwrap()
}

fn children(app: &mut App) -> Vec<Entity> {
    let l = list(app);
    app.world().get::<Children>(l).map(|c| c.to_vec()).unwrap_or_default()
}

#[test]
fn many_groups_in_one_call_are_one_reconcile() {
    let (mut app, replies) = app();
    assert_eq!(mount(&mut app, &replies, (0..1000).map(|i| format!("k{i}"))), 1);
    assert_eq!(last(&app).spawned, 2000);
    let kids = children(&mut app);
    assert_eq!(kids.len(), 1002, "head, the 1,000 rows and tail");
    let tail = node_at(&mut app, &["list", "tail"]);
    assert_eq!(kids.last(), Some(&tail), "rows mount before a sibling described after the slot");
}

#[test]
fn what_a_slot_reconcile_checks_does_not_grow_with_the_slot() {
    for existing in [10, 1000] {
        let (mut app, replies) = app();
        mount(&mut app, &replies, (0..existing).map(|i| format!("k{i}")));
        // One more group: nothing of it was mounted, so nothing is checked for despawn.
        assert_eq!(mount(&mut app, &replies, ["new".to_string()]), 1);
        assert_eq!(last(&app), ReconcileStats { spawned: 2, ..Default::default() }, "{existing} groups");
        // Three groups out: their six nodes are checked, and only they go.
        let unmounts: Vec<Value> = ["k0", "k1", "k2"].iter().map(|k| json!([SLOT, k])).collect();
        assert_eq!(call(&mut app, &replies, json!({ "unmounts": unmounts })), 1);
        assert_eq!(last(&app), ReconcileStats { despawned: 3, checked: 6, ..Default::default() }, "{existing} groups");
        assert!(node(&mut app, &["k0", "row"]).is_none() && node(&mut app, &["k0", "row", "child"]).is_none());
        assert!(node(&mut app, &["k3", "row", "child"]).is_some());
    }
}

#[test]
fn a_reversed_list_keeps_its_entities_and_sorts_children() {
    let (mut app, replies) = app();
    mount(&mut app, &replies, ["a", "b", "c"].map(String::from));
    let (head, tail) = (node_at(&mut app, &["list", "head"]), node_at(&mut app, &["list", "tail"]));
    let [a, b, c] = ["a", "b", "c"].map(|k| node(&mut app, &[k, "row"]).unwrap());
    assert_eq!(children(&mut app), vec![head, a, b, c, tail]);
    assert_eq!(call(&mut app, &replies, json!({ "orders": [[SLOT, ["c", "b", "a"]]] })), 1);
    assert_eq!(last(&app), ReconcileStats::default(), "nothing spawned, despawned or written");
    assert_eq!(children(&mut app), vec![head, c, b, a, tail], "head and tail stay where they were");
    // An order with a mount and an unmount in the same call.
    call(&mut app, &replies, json!({ "unmounts": [[SLOT, "b"]], "mounts": [[SLOT, "d", row(false)]], "orders": [[SLOT, ["d", "a", "c"]]] }));
    let d = node(&mut app, &["d", "row"]).unwrap();
    assert_eq!(children(&mut app), vec![head, d, a, c, tail]);
}

fn node_at(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap()
}

#[test]
fn a_remount_that_drops_a_child_despawns_only_it() {
    let (mut app, replies) = app();
    mount(&mut app, &replies, ["a", "b"].map(String::from));
    let (a, b) = (node(&mut app, &["a", "row"]).unwrap(), node(&mut app, &["b", "row", "child"]).unwrap());
    call(&mut app, &replies, json!({ "mounts": [[SLOT, "a", row(false)]] }));
    assert_eq!(last(&app), ReconcileStats { despawned: 1, checked: 2, ..Default::default() });
    assert_eq!(node(&mut app, &["a", "row"]), Some(a), "the row keeps its entity");
    assert!(node(&mut app, &["a", "row", "child"]).is_none());
    assert_eq!(node(&mut app, &["b", "row", "child"]), Some(b));
}

#[test]
fn an_unmount_and_a_mount_of_one_group_in_a_call_respawn_it() {
    let (mut app, replies) = app();
    mount(&mut app, &replies, ["a".to_string()]);
    let a = node(&mut app, &["a", "row"]).unwrap();
    call(&mut app, &replies, json!({ "unmounts": [[SLOT, "a"]], "mounts": [[SLOT, "a", row(true)]] }));
    let again = node(&mut app, &["a", "row"]).unwrap();
    assert_ne!(again, a, "a Show toggled off and on in one batch mounts fresh entities");
    assert!(app.world().get_entity(a).is_err());
}

#[test]
fn a_bad_group_among_good_ones_logs_and_the_rest_mount() {
    let (mut app, replies) = app();
    let bad = json!([{ "key": "row", "type": "entity", "components": [{ "type": "game::Health", "kind": "ts", "props": { "mx": 1 } }] }]);
    call(&mut app, &replies, json!({ "mounts": [[SLOT, "a", row(true)], [SLOT, "bad", bad], [SLOT, "c", row(true)]] }));
    assert!(node(&mut app, &["a", "row"]).is_some() && node(&mut app, &["c", "row"]).is_some());
    assert!(node(&mut app, &["bad", "row"]).is_none());
    let log: Vec<String> = app.world().resource::<ScriptLog>().since(0).map(|e| e.message.clone()).collect();
    assert!(log.iter().any(|m| m.contains("mx")), "{log:?}");
}
