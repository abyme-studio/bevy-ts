//! The hole check (reflect-coverage decision 3): every reflected component, resource and asset in
//! the prebuilt engine, and in a project engine with the Rapier port, reads through
//! `reflect_value::read`, writes back equal through `write`, and has a TS type, unless
//! `reflect_value::UNCOVERED` lists it with a reason. A component TSX cannot insert fails unless
//! `reflect_value::NO_DEFAULT` lists it; `type_data` entries Bevy already reflects fail. An entry
//! no walked type uses fails too.

use crate::common;

use bevy::prelude::*;
use bevy_ts::reflect_codec::{compile_component, encode};
use bevy_ts::reflect_value::{NO_DEFAULT, UNCOVERED, default_of, read_component};
use bevy_ts_cli::cli::Engine;
use bevy_ts_cli::coverage::{Report, walk};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// Crates only the Rapier project engine links: their entries are checked by the Rapier walk.
const RAPIER: [&str; 4] = ["bevy_rapier3d::", "rapier3d::", "parry3d::", "nalgebra::"];

fn rapier_entry(path: &str) -> bool {
    RAPIER.iter().any(|c| path.starts_with(c))
}

fn gaps(report: &Report) -> Vec<String> {
    report.gaps.iter().map(|g| format!("{} [{}] in {}{}: {}", g.type_path, g.kind, g.root, g.field, g.error)).collect()
}

/// The entries of `UNCOVERED` and `NO_DEFAULT` that `which` selects and no type in `listed` matches.
fn stale(listed: &BTreeSet<String>, which: impl Fn(&str) -> bool) -> Vec<&'static str> {
    UNCOVERED
        .iter()
        .chain(NO_DEFAULT)
        .map(|(p, _)| *p)
        .filter(|p| which(p))
        .filter(|p| !listed.iter().any(|l| l == p || (p.ends_with('<') && l.starts_with(p))))
        .collect()
}

#[test]
fn prebuilt_engine_has_no_unlisted_gap() {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let mut app = bevy_ts_cli::types::types_app(&engine, |_: &mut bevy::prelude::App| {});
    bevy_ts::layout_reflect::fill(app.world_mut());
    let report = walk(app.world());
    assert!(report.met.len() > 200, "the walk met only {} types", report.met.len());
    let gaps = gaps(&report);
    assert!(gaps.is_empty(), "{} reflected types have a gap not on reflect_value::UNCOVERED or NO_DEFAULT:\n{}", gaps.len(), gaps.join("\n"));
    let stale = stale(&report.listed, |p| !rapier_entry(p));
    assert!(stale.is_empty(), "reflect_value::UNCOVERED or NO_DEFAULT names types no registered type uses: {stale:?}");
    let redundant = &app.world().resource::<bevy_ts::type_data::RedundantTypeData>().0;
    assert!(redundant.is_empty(), "bevy_ts::type_data adds type data Bevy already reflects; delete: {redundant:?}");
}

#[test]
#[ignore = "builds the rapier project engine"]
fn rapier_port_has_no_unlisted_gap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rapier");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("reflect-coverage-rapier.json");
    let _ = std::fs::remove_file(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .args(["types", "--coverage"])
        .arg(&out)
        .status()
        .expect("bevy-ts runs");
    assert!(status.success(), "bevy-ts types --coverage failed in examples/rapier");
    let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&out).expect("a report")).expect("JSON");
    assert!(report["met"].as_array().unwrap().iter().any(|t| t.as_str().is_some_and(|t| t.starts_with("bevy_rapier3d::"))), "no Rapier type met");
    let gaps: Vec<String> = report["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| format!("{} [{}] in {}{}: {}", g["type_path"].as_str().unwrap(), g["kind"].as_str().unwrap(), g["root"].as_str().unwrap(), g["field"].as_str().unwrap(), g["error"].as_str().unwrap()))
        .collect();
    assert!(gaps.is_empty(), "{} reflected types have a gap not on reflect_value::UNCOVERED or NO_DEFAULT:\n{}", gaps.len(), gaps.join("\n"));
    let listed: BTreeSet<String> = report["listed"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect();
    let stale = stale(&listed, rapier_entry);
    assert!(stale.is_empty(), "reflect_value::UNCOVERED or NO_DEFAULT names Rapier types no registered type uses: {stale:?}");
}

/// The codec check (reflect-coverage decision 7): for every reflected component and resource of
/// the prebuilt engine with a default, the bytes its codec encodes, decoded by the JS decoder
/// generated from the same ops, equal `JSON.parse` of `read_component`'s JSON.
#[test]
fn codec_output_equals_read_for_every_covered_type() {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let app = bevy_ts_cli::types::types_app(&engine, |_: &mut App| {});
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let mut cases = Vec::new();
    let mut paths: Vec<_> = registry.iter().filter(|r| r.data::<bevy::ecs::reflect::ReflectComponent>().is_some()).map(|r| (r.type_info().type_path(), r.type_id())).collect();
    paths.sort();
    for (path, id) in paths {
        let Some(value) = default_of(&registry, id) else { continue };
        // A type `read` cannot cover is the walk's business, not the codec's.
        let Ok(json) = read_component(&registry, value.as_ref()) else { continue };
        let codec = compile_component(&registry, registry.get(id).unwrap().type_info());
        let mut bytes = Vec::new();
        if let Err(e) = encode(&registry, &codec, value.as_ref(), &mut bytes) {
            panic!("{path}: read covers it but the codec fails: {e}\n  codec: {codec:?}");
        }
        cases.push(serde_json::json!({ "type": path, "codec": codec, "bytes": bytes, "json": json.to_string() }));
    }
    assert!(cases.len() > 100, "only {} cases", cases.len());
    let n = cases.len();
    let src = "import { __reflectCheck, app, text } from \"bevy-ts\";\napp.entry(\"check\", (payload) => JSON.stringify(__reflectCheck(JSON.parse(text(payload)))));\n";
    let (_, mut js) = common::load("reflect-codec-check", src);
    let out = bevy_ts::plugin_api::call_entry(js.world_mut(), "check", serde_json::to_string(&cases).unwrap().as_bytes()).expect("the check entry runs");
    let bad: Vec<serde_json::Value> = serde_json::from_slice(&out).unwrap();
    let lines: Vec<String> = bad.iter().map(|b| format!("{}:\n  codec: {}\n  read:  {}", b["type"], b["got"], b["want"])).collect();
    assert!(bad.is_empty(), "{} of {n} types decode differently from read:\n{}", bad.len(), lines.join("\n"));
}
