mod common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::plugin_api::{self, BevyTsAppExt};
use common::*;
use std::sync::{Arc, Mutex};

#[test]
fn sections_entries_and_command_kinds_from_js() {
    let mut app = app();
    let sections = Arc::new(Mutex::new(Vec::new()));
    let s = sections.clone();
    app.add_ts_section("test/scene", move |_, input| {
        s.lock().unwrap().push((String::from_utf8_lossy(input.bytes).to_string(), input.location.clone()));
        Ok(())
    });
    app.add_ts_command_kind("test/name", |world, bytes| {
        let (bits, name) = bytes.split_at(8);
        let e = Entity::from_bits(u64::from_le_bytes(bits.try_into().unwrap()));
        world.get_entity_mut(e).map_err(|e| e.to_string())?.insert(Name::new(String::from_utf8_lossy(name).to_string()));
        Ok(())
    });
    load(
        &mut app,
        &[(
            "plugin.js",
            r#"
            import { app, section, text } from "bevy-ts";
            section("test/scene", { root: "cube" });
            app.entry("test/render", (payload, ctx) => {
              const e = ctx.commands.spawn();
              const bytes = new Uint8Array(8 + payload.length);
              const dv = new DataView(bytes.buffer);
              dv.setUint32(0, e % 4294967296, true);
              dv.setUint32(4, Math.floor(e / 4294967296), true);
              bytes.set(payload, 8);
              ctx.commands.custom("test/name", bytes);
              return `spawned ${text(payload)}`;
            });
            app.entry("test/throws", (_, ctx) => {
              ctx.commands.spawn();
              throw new Error("entry failed");
            });
            app.entry("test/unknown", (_, ctx) => {
              ctx.commands.custom("test/nope", "x");
            });
            "#,
        )],
    );
    let got = sections.lock().unwrap().clone();
    assert_eq!(got[0].0, r#"{"root":"cube"}"#);
    assert_eq!(got[0].1.as_ref().unwrap().line, 3, "{got:?}");

    let calls = |app: &App| app.world().non_send::<bevy_ts::runtime::ScriptRuntime>().backend.calls_into_js();
    let before = calls(&app);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(calls(&app), before, "no entry call unless the plugin makes one");

    let out = plugin_api::call_entry(app.world_mut(), "test/render", b"bolt").unwrap();
    assert_eq!(out, b"spawned bolt");
    let names: Vec<String> = app.world_mut().query::<&Name>().iter(app.world()).map(|n| n.to_string()).collect();
    assert_eq!(names, vec!["bolt"]);

    let n = app.world().entities().count_spawned();
    let e = plugin_api::call_entry(app.world_mut(), "test/throws", b"").unwrap_err();
    assert!(e.message.contains("entry failed"), "{e:?}");
    assert!(e.location.as_ref().unwrap().file.ends_with("plugin.js"), "{e:?}");
    assert_eq!(app.world().entities().count_spawned(), n);

    let e = plugin_api::call_entry(app.world_mut(), "test/unknown", b"").unwrap_err();
    assert!(e.message.contains("unregistered command kind test/nope"), "{e:?}");
}

#[test]
fn a_section_nobody_registered_fails_the_load() {
    let mut app = app();
    let r = bevy_ts::generation::apply(
        app.world_mut(),
        set(&[("s.js", "import { section } from \"bevy-ts\";\nsection(\"nobody/owns\", 1);\n")]),
        false,
    );
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("nobody/owns"));
}
