//! Retained gizmos (design 10): `GizmoAsset` in `bevy-ts/gizmos` records its drawing calls in JS
//! (`host::recorder_glue` over `Gizmos`' table); `assets.add(GizmoAsset, g)` and `assets.set`
//! reach the hand map here, which replays the record onto a fresh `GizmoAsset` through
//! [`draw`]'s entries. A read gives back the record the script last wrote.

use bevy::gizmos::GizmoAsset;
use bevy::gizmos::config::DefaultGizmoConfigGroup;
use bevy::prelude::*;
use bevy_ts::host::{self, SlotReader};
use bevy_ts::reflect_asset::Slots;
use serde_json::Value;

use super::draw::{Entry, Setting};
use super::primitives::Registry;
use super::TsGizmos;

/// `GizmoAsset`'s type path, as `assets.add` names it.
pub const TYPE_PATH: &str = "bevy_gizmos::GizmoAsset";

/// Hand-maps `GizmoAsset` for scripts.
pub(crate) fn register(app: &mut App) {
    bevy_ts::plugin_api::BevyTsAppExt::ts_hand_map::<GizmoAsset>(app, read, write);
}

/// The record the script last wrote, or `{}` for an asset Rust made.
fn read(_: &Slots, source: Option<&Value>, _: &GizmoAsset) -> Result<Value, String> {
    Ok(source.cloned().unwrap_or_else(|| Value::Object(Default::default())))
}

/// A fresh `GizmoAsset` from `{ calls }`: an `add`, or a `set`, which replaces the calls whole.
fn write(slots: &Slots, _: Option<&GizmoAsset>, json: &Value) -> Result<GizmoAsset, String> {
    replay(Registry::Read(slots.registry), json)
}

/// One recorded drawing call, its settings after it.
struct Call {
    entry: Entry,
    args: Vec<u8>,
    slices: Vec<Vec<u8>>,
    settings: Vec<(Setting, Vec<u8>)>,
}

/// Draws `{ calls: [[name, slot words, ...point words], ...] }` onto a new `GizmoAsset`.
pub fn replay(registry: Registry, json: &Value) -> Result<GizmoAsset, String> {
    let mut asset = GizmoAsset::new();
    let Some(calls) = json.get("calls") else {
        return match json.as_object() {
            Some(o) if o.is_empty() => Ok(asset),
            _ => Err(format!("a GizmoAsset is written from a GizmoAsset or {{ calls }}, not {json}")),
        };
    };
    let calls = calls.as_array().ok_or_else(|| format!("GizmoAsset calls is an array, not {calls}"))?;
    let mut open: Option<Call> = None;
    for (n, call) in calls.iter().enumerate() {
        let parts = call.as_array().filter(|p| p.len() >= 2).ok_or_else(|| format!("GizmoAsset call {n} is [name, slots, ...], not {call}"))?;
        let name = parts[0].as_str().ok_or_else(|| format!("GizmoAsset call {n}: its name is a string, not {}", parts[0]))?;
        let words = |v: &Value| bytes(v).map_err(|e| format!("GizmoAsset.{name} (call {n}): {e}"));
        if let Some(&entry) = Entry::ALL.iter().find(|e| e.ts() == name) {
            if let Some(c) = open.take() {
                draw(&mut asset, registry, c)?;
            }
            let slices: Vec<Vec<u8>> = parts[2..].iter().map(words).collect::<Result<_, _>>()?;
            let points = entry.args().iter().filter(|a| a.slice().is_some()).count();
            if slices.len() != points {
                return Err(format!("GizmoAsset.{name} (call {n}): {} point lists, not {points}", slices.len()));
            }
            open = Some(Call { entry, args: words(&parts[1])?, slices, settings: Vec::new() });
        } else if let Some(&setting) = Setting::ALL.iter().find(|s| s.ts() == name) {
            let c = open.as_mut().filter(|c| c.entry.builder().is_some()).ok_or_else(|| format!("GizmoAsset.{name} (call {n}): no builder before it"))?;
            c.settings.push((setting, words(&parts[1])?));
        } else {
            return Err(format!("GizmoAsset call {n}: no drawing method {name}"));
        }
    }
    if let Some(c) = open {
        draw(&mut asset, registry, c)?;
    }
    Ok(asset)
}

fn draw(asset: &mut GizmoAsset, registry: Registry, c: Call) -> Result<(), String> {
    let slices: Vec<&[u8]> = c.slices.iter().map(Vec::as_slice).collect();
    c.entry.draw(&mut **asset, registry, &mut SlotReader::new(&c.args), &slices, &c.settings).map_err(|e| format!("GizmoAsset.{}: {e}", c.entry.ts()))
}

/// A list of `u32` words as their little-endian bytes.
fn bytes(v: &Value) -> Result<Vec<u8>, String> {
    let words = v.as_array().ok_or_else(|| format!("expected a list of u32 words, not {v}"))?;
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        let w = w.as_u64().filter(|w| *w <= u32::MAX as u64).ok_or_else(|| format!("{w} is not a u32 word"))?;
        out.extend_from_slice(&(w as u32).to_le_bytes());
    }
    Ok(out)
}

/// `GizmoAsset`'s class for `bevy-ts/gizmos`: the recorder over the default group's export.
pub fn module_js() -> String {
    let spec = <TsGizmos<'static, 'static, DefaultGizmoConfigGroup> as crate::ExportParam>::spec();
    format!("export const GizmoAsset = __hostAsset({TYPE_PATH:?}, {:?});\n", host::recorder_glue(&spec, "GizmoAsset"))
}

/// `GizmoAsset`'s declarations for `bevy-ts/gizmos`; the builders are `Gizmos`'.
pub fn declaration() -> String {
    let spec = <TsGizmos<'static, 'static, DefaultGizmoConfigGroup> as crate::ExportParam>::spec();
    let mut s = format!(
        "/** A `GizmoAsset`'s value: its recorded calls, replayed in Rust. */\nexport type GizmoAssetValue = AssetFields<{TYPE_PATH:?}, {{ readonly calls: readonly unknown[] }}>;\n"
    );
    s.push_str("/** Bevy's `GizmoAsset`: `Gizmos`' drawing methods, recorded; `assets.add(GizmoAsset, g)` draws them once into an asset a `<Gizmo handle={ref} />` shows. */\n");
    s.push_str(&host::recorder_declaration(&spec, "GizmoAsset"));
    s.push_str("export declare const GizmoAsset: AssetClass<GizmoAssetValue> & { new (): GizmoAsset };\n");
    s
}
