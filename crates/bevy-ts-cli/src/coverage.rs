//! The hole check (reflect-coverage decision 3): every component, resource and asset in a world's
//! type registry, walked through every field type. For each type it takes a value (its default,
//! else one shaped from its type info), and checks that `reflect_value::read` gives JSON, that
//! writing that JSON over a fresh value gives an equal one, and that its TS type is not `unknown`.
//! A type on `reflect_value::UNCOVERED` is not checked and not walked into; a hand-mapped asset
//! (`reflect_asset::HAND_MAPS`) is checked by its entry's round trip and not walked into. `tests/it/reflect_coverage.rs`
//! runs it on the prebuilt engine, and `bevy-ts types --coverage <file>` on a project engine (the
//! Rapier port's).

use crate::types::reflect_ts;
use bevy::asset::ReflectAsset;
use bevy::ecs::reflect::{ReflectComponent, ReflectResource};
use bevy::prelude::*;
use bevy::reflect::enums::VariantInfo;
use bevy::reflect::{PartialReflect, TypeInfo, TypeRegistry};
use bevy_ts::reflect_asset;
use bevy_ts::reflect_value::{self, uncovered};
use serde::Serialize;
use serde_json::json;
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};

/// One type the reader, writer or types cannot cover.
#[derive(Serialize, Debug, Clone)]
pub struct Gap {
    /// The type that fails.
    pub type_path: String,
    /// What fails: `read`, `round trip`, `TS type`, `no value`, `not registered`.
    pub kind: String,
    pub error: String,
    /// Where the walk first met it: the component, resource or asset, and the field path in it.
    pub root: String,
    pub field: String,
}

#[derive(Serialize, Debug, Default)]
pub struct Report {
    pub gaps: Vec<Gap>,
    /// Every type path the walk met, uncovered ones included.
    pub met: BTreeSet<String>,
    /// The `UNCOVERED` entries the walk met.
    pub listed: BTreeSet<String>,
}

/// Walks every reflected component, resource and asset in `world`'s registry.
pub fn walk(world: &World) -> Report {
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut roots: Vec<(String, TypeId)> = registry
        .iter()
        .filter(|r| r.data::<ReflectComponent>().is_some() || r.data::<ReflectResource>().is_some() || r.data::<ReflectAsset>().is_some())
        .map(|r| (r.type_info().type_path().to_string(), r.type_id()))
        .collect();
    roots.sort();
    let mut w = Walk { registry: &registry, done: BTreeMap::new(), report: Report::default() };
    for (path, id) in roots {
        w.visit(id, &path, &path, "");
    }
    w.report
}

struct Walk<'r> {
    registry: &'r TypeRegistry,
    /// Each type visited: whether it, or a type inside it, has a gap not on the list.
    done: BTreeMap<TypeId, bool>,
    report: Report,
}

/// Types the reader and writer take whole, so the walk does not look inside them.
fn leaf(path: &str) -> bool {
    path.starts_with("glam::")
        || path.starts_with("bevy_asset::handle::Handle<")
        || path.starts_with("core::ops::Range")
        || matches!(path, "bevy_color::color::Color" | "bevy_camera::camera::Exposure" | "core::time::Duration" | "bevy_ecs::entity::Entity" | "bevy_ecs::name::Name")
}

/// The type ids of what a type holds.
fn children(info: &TypeInfo) -> Vec<(String, TypeId, String)> {
    let mut out = Vec::new();
    match info {
        TypeInfo::Struct(s) => out.extend(s.iter().map(|f| (format!(".{}", f.name()), f.type_id(), f.type_path().to_string()))),
        TypeInfo::TupleStruct(s) => out.extend(s.iter().enumerate().map(|(i, f)| (format!(".{i}"), f.type_id(), f.type_path().to_string()))),
        TypeInfo::Tuple(t) => out.extend(t.iter().enumerate().map(|(i, f)| (format!(".{i}"), f.type_id(), f.type_path().to_string()))),
        TypeInfo::List(l) => out.push(("[]".into(), l.item_ty().id(), l.item_ty().path().to_string())),
        TypeInfo::Array(a) => out.push(("[]".into(), a.item_ty().id(), a.item_ty().path().to_string())),
        TypeInfo::Set(s) => out.push(("[]".into(), s.value_ty().id(), s.value_ty().path().to_string())),
        TypeInfo::Map(m) => {
            out.push(("{key}".into(), m.key_ty().id(), m.key_ty().path().to_string()));
            out.push(("{value}".into(), m.value_ty().id(), m.value_ty().path().to_string()));
        }
        TypeInfo::Enum(e) => {
            for v in e.iter() {
                match v {
                    VariantInfo::Unit(_) => {}
                    VariantInfo::Tuple(t) => out.extend(t.iter().enumerate().map(|(i, f)| (format!("::{}.{i}", v.name()), f.type_id(), f.type_path().to_string()))),
                    VariantInfo::Struct(s) => out.extend(s.iter().map(|f| (format!("::{}.{}", v.name(), f.name()), f.type_id(), f.type_path().to_string()))),
                }
            }
        }
        _ => {}
    }
    out
}

/// A value of the type: `reflect_value::blank`, else, for an opaque type with no default, the first
/// small JSON value it takes.
fn sample(registry: &TypeRegistry, id: TypeId) -> Option<Box<dyn PartialReflect>> {
    if let Some(v) = reflect_value::blank(registry, id) {
        return Some(v);
    }
    let candidates = [json!(1), json!(0), json!(""), json!(false), json!({ "start": 0, "end": 1 }), json!([]), json!({})];
    candidates.iter().find_map(|c| reflect_value::value_of(registry, id, c).ok())
}

/// `read`, then `write` over a second value, then the two compared.
fn round_trip(registry: &TypeRegistry, id: TypeId) -> Result<(), (&'static str, String)> {
    let value = sample(registry, id).ok_or(("no value", "no default and no value can be built from its type info".to_string()))?;
    let json = reflect_value::read(registry, value.as_ref()).map_err(|e| ("read", e))?;
    let mut back = sample(registry, id).expect("a second value");
    reflect_value::write(registry, None, back.as_mut(), None, &json).map_err(|e| ("round trip", format!("writing {json}: {e}")))?;
    same(registry, back.as_ref(), value.as_ref(), &json)?;
    // A new value from the JSON alone, as a list item, map entry or new variant's field is made.
    let made = reflect_value::value_of(registry, id, &json).map_err(|e| ("new value", format!("making one from {json}: {e}")))?;
    same(registry, made.as_ref(), value.as_ref(), &json).map_err(|(_, e)| ("new value", e))
}

/// `got` equals `want`; a type that cannot compare is taken as equal when it reads the same.
fn same(registry: &TypeRegistry, got: &dyn PartialReflect, want: &dyn PartialReflect, json: &serde_json::Value) -> Result<(), (&'static str, String)> {
    match got.reflect_partial_eq(want) {
        Some(true) => Ok(()),
        None if reflect_value::read(registry, got).ok().as_ref() == Some(json) => Ok(()),
        _ => Err(("round trip", format!("{json} wrote back as {}", reflect_value::read(registry, got).unwrap_or_default()))),
    }
}

impl Walk<'_> {
    fn gap(&mut self, type_path: &str, kind: &str, error: String, root: &str, field: &str) {
        self.report.gaps.push(Gap { type_path: type_path.into(), kind: kind.into(), error, root: root.into(), field: field.into() });
    }

    /// Visits a type; true when it or a type inside it has a gap not on the list.
    fn visit(&mut self, id: TypeId, path: &str, root: &str, field: &str) -> bool {
        if let Some(&bad) = self.done.get(&id) {
            return bad;
        }
        self.done.insert(id, false);
        self.report.met.insert(path.to_string());
        if uncovered(path).is_some() {
            self.report.listed.insert(path.to_string());
            return false;
        }
        let Some(reg) = self.registry.get(id) else {
            self.gap(path, "not registered", "the type is not in the registry".into(), root, field);
            self.done.insert(id, true);
            return true;
        };
        // A hand-mapped asset is read and written by its entry, not by reflection.
        if let Some(trip) = reflect_asset::hand_round_trip(self.registry, id) {
            let bad = trip.is_err();
            if let Err(e) = trip {
                self.gap(path, "round trip", e, root, field);
            }
            self.done.insert(id, bad);
            return bad;
        }
        let mut inner_bad = false;
        if !leaf(path) {
            for (step, child, child_path) in children(reg.type_info()) {
                inner_bad |= self.visit(child, &child_path, root, &format!("{field}{step}"));
            }
        }
        if inner_bad {
            // The gap inside is reported; this type fails because of it.
            self.done.insert(id, true);
            return true;
        }
        let registry = self.registry;
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| round_trip(registry, id)))
            .unwrap_or_else(|p| Err(("round trip", format!("panicked: {}", panic_text(&p)))));
        let mut bad = false;
        if let Err((kind, e)) = checked {
            self.gap(path, kind, e, root, field);
            bad = true;
        }
        let ts = reflect_ts(registry, path);
        if ts.contains("unknown") {
            self.gap(path, "TS type", ts, root, field);
            bad = true;
        }
        self.done.insert(id, bad);
        bad
    }
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}
