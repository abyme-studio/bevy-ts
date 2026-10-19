//! `bevy-ts types` (native decision 6): builds the engine's `App` with every plugin added and no
//! GPU (`WgpuSettings { backends: None }`, and `finish` never runs), walks the type registry and
//! writes `native.d.ts`: layout-tier components as columns, reflect-tier ones as typed values,
//! resources, and the exposed sets as the `NativeSet` union. Deterministic: sorted by type path.

use crate::cli::Engine;
use crate::headless;
use bevy::prelude::*;
use bevy::reflect::{TypeInfo, TypeRegistration, TypeRegistry};
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy_ts::BevyTsPlugin;
use bevy_ts::layout::{FieldType, NativeLayouts};
use bevy_ts::sets::TsSets;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The app `bevy-ts types` walks: the engine's plugins and the project's, no window, no device,
/// no JS engine.
fn types_app(engine: &Engine, extra: impl Plugin) -> App {
    let mut app = App::new();
    let render = RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })),
        ..default()
    };
    app.add_plugins(headless::default_plugins(true).set(render));
    let fake = bevy_ts::fake::FakeHandle::default();
    app.add_plugins(BevyTsPlugin::new(move || Box::new(fake.backend())))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    app
}

/// `bevy-ts types [--out <file>]`. Returns the exit code.
pub fn run(engine: &Engine, extra: impl Plugin, args: &[String]) -> i32 {
    let mut out = PathBuf::from(".bevy-ts/types/native.d.ts");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => match it.next() {
                Some(p) => out = PathBuf::from(p),
                None => {
                    eprintln!("bevy-ts types: --out takes a file");
                    return 2;
                }
            },
            s => {
                eprintln!("bevy-ts types: unexpected argument {s}");
                return 2;
            }
        }
    }
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| types_app(engine, extra)));
    let mut app = match built {
        Ok(app) => app,
        Err(p) => {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            eprintln!("bevy-ts types: a plugin failed to build without a GPU device, so its types are unavailable: {msg}");
            return 1;
        }
    };
    bevy_ts::layout_reflect::fill(app.world_mut());
    let text = emit(app.world());
    if let Err(e) = write(&out, &text) {
        eprintln!("bevy-ts types: {e}");
        return 1;
    }
    eprintln!("bevy-ts types: wrote {}", out.display());
    0
}

fn write(out: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if std::fs::read_to_string(out).ok().as_deref() == Some(text) {
        return Ok(());
    }
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))
}

fn kind(t: &FieldType) -> String {
    match t {
        FieldType::Enum { variants, .. } => {
            format!("enumOf({})", variants.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(", "))
        }
        FieldType::Array { of, len } => format!("array({}, {len})", kind(of)),
        other => other.ts_name(),
    }
}

/// A TS identifier for a type path's short name: `Time<()>` is `Time`, `Time<Virtual>` is
/// `TimeVirtual`.
fn ident(short: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in short.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        } else {
            upper = true;
        }
    }
    if out.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// A TS type for a reflected type, following `registry.schema`'s JSON shapes: numbers, strings,
/// `Handle<T>` as an asset path, `Entity` as an entity, options as `| null`, lists as arrays.
fn ts_type(registry: &TypeRegistry, path: &str, depth: usize) -> String {
    const NUMBERS: [&str; 14] = ["f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize"];
    if NUMBERS.contains(&path) {
        return "number".into();
    }
    match path {
        "bool" => return "boolean".into(),
        "alloc::string::String" | "&str" | "alloc::borrow::Cow<str>" | "char" => return "string".into(),
        "bevy_ecs::entity::Entity" => return "Entity".into(),
        "glam::Vec2" => return "readonly [number, number]".into(),
        "glam::Vec3" | "glam::Vec3A" => return "readonly [number, number, number]".into(),
        "glam::Vec4" | "glam::Quat" => return "readonly [number, number, number, number]".into(),
        _ => {}
    }
    if path.starts_with("bevy_asset::handle::Handle<") {
        return "string".into();
    }
    if depth > 4 {
        return "unknown".into();
    }
    let Some(reg) = registry.get_with_type_path(path) else { return "unknown".into() };
    match reg.type_info() {
        TypeInfo::Struct(s) => {
            let fields: Vec<String> =
                s.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(registry, f.type_path(), depth + 1))).collect();
            format!("{{ {} }}", fields.join("; "))
        }
        TypeInfo::TupleStruct(s) if s.field_len() == 1 => ts_type(registry, s.field_at(0).unwrap().type_path(), depth + 1),
        TypeInfo::TupleStruct(s) => {
            format!("readonly [{}]", s.iter().map(|f| ts_type(registry, f.type_path(), depth + 1)).collect::<Vec<_>>().join(", "))
        }
        TypeInfo::Tuple(t) => {
            format!("readonly [{}]", t.iter().map(|f| ts_type(registry, f.type_path(), depth + 1)).collect::<Vec<_>>().join(", "))
        }
        TypeInfo::List(l) => format!("readonly {}[]", paren(ts_type(registry, l.item_ty().path(), depth + 1))),
        TypeInfo::Array(a) => format!("readonly {}[]", paren(ts_type(registry, a.item_ty().path(), depth + 1))),
        TypeInfo::Set(s) => format!("readonly {}[]", paren(ts_type(registry, s.value_ty().path(), depth + 1))),
        TypeInfo::Map(m) => format!("Record<string, {}>", ts_type(registry, m.value_ty().path(), depth + 1)),
        TypeInfo::Enum(e) => {
            if path.starts_with("core::option::Option<") {
                let inner = e.variant("Some").and_then(|v| match v {
                    bevy::reflect::enums::VariantInfo::Tuple(t) => t.field_at(0).map(|f| f.type_path().to_string()),
                    _ => None,
                });
                return match inner {
                    Some(i) => format!("{} | null", ts_type(registry, &i, depth + 1)),
                    None => "unknown".into(),
                };
            }
            let variants: Vec<String> = e
                .iter()
                .map(|v| match v {
                    bevy::reflect::enums::VariantInfo::Unit(u) => format!("{:?}", u.name()),
                    bevy::reflect::enums::VariantInfo::Tuple(t) if t.field_len() == 1 => {
                        format!("{{ {}: {} }}", prop(t.name()), ts_type(registry, t.field_at(0).unwrap().type_path(), depth + 1))
                    }
                    bevy::reflect::enums::VariantInfo::Tuple(t) => {
                        let items: Vec<String> = t.iter().map(|f| ts_type(registry, f.type_path(), depth + 1)).collect();
                        format!("{{ {}: readonly [{}] }}", prop(t.name()), items.join(", "))
                    }
                    bevy::reflect::enums::VariantInfo::Struct(s) => {
                        let fields: Vec<String> =
                            s.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(registry, f.type_path(), depth + 1))).collect();
                        format!("{{ {}: {{ {} }} }}", prop(s.name()), fields.join("; "))
                    }
                })
                .collect();
            variants.join(" | ")
        }
        _ => "unknown".into(),
    }
}

fn paren(t: String) -> String {
    if t.contains(' ') && !t.starts_with('{') { format!("({t})") } else { t }
}

fn prop(name: &str) -> String {
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain { name.to_string() } else { format!("{name:?}") }
}

fn is_resource(reg: &TypeRegistration) -> bool {
    reg.data::<bevy::ecs::reflect::ReflectResource>().is_some()
}

/// The `.d.ts` text for a world whose native layouts are filled.
pub fn emit(world: &World) -> String {
    let registry = world.resource::<AppTypeRegistry>().read();
    let native = world.resource::<NativeLayouts>();
    let mut s = String::from(
        "// Generated by `bevy-ts types` from the engine's type registry. Do not edit.\n\
         // Layout-tier types are columns in TS systems; reflect-tier types are typed values for TSX,\n\
         // the protocol and With/Without filters.\n\n",
    );
    s.push_str("import { array, bool, Component, entity, enumOf, f32, f64, i16, i32, i8, quat, ReflectComponent, ReflectResource, Resource, u16, u32, u8, vec2, vec3, vec4 } from \"bevy-ts\";\n");
    s.push_str("import type { Entity } from \"bevy-ts\";\n\n");

    let sets: Vec<String> = world.get_resource::<TsSets>().map(|t| t.iter().map(|x| format!("{:?}", x.name)).collect()).unwrap_or_default();
    let _ = writeln!(s, "/** System sets exposed with `app.ts_set`, for `.before`/`.after(systemSet(name))`. */");
    let _ = writeln!(s, "export type NativeSet = {};\n", if sets.is_empty() { "never".to_string() } else { sets.join(" | ") });

    // Names: short idents, disambiguated by crate when two types share one.
    let mut entries: BTreeMap<String, (String, &TypeRegistration)> = BTreeMap::new();
    for reg in registry.iter().filter(|r| r.data::<bevy::ecs::reflect::ReflectComponent>().is_some()) {
        let path = reg.type_info().type_path().to_string();
        entries.insert(path.clone(), (ident(reg.type_info().type_path_table().short_path()), reg));
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (name, _) in entries.values() {
        *seen.entry(name.clone()).or_default() += 1;
    }
    let mut used = BTreeSet::new();
    for (path, (short, reg)) in &entries {
        let mut name = short.clone();
        if seen[short] > 1 {
            let krate = reg.type_info().type_path_table().crate_name().unwrap_or("");
            name = format!("{}{}", ident(krate), short);
        }
        while !used.insert(name.clone()) {
            name.push('_');
        }
        let resource = is_resource(reg);
        if let Some(n) = native.get(path).filter(|n| &n.type_path == path) {
            let fields: Vec<String> = n.layout.fields.iter().map(|f| format!("{}: {}", prop(&f.name), kind(&f.ty))).collect();
            let mut notes = Vec::new();
            if !n.writable {
                notes.push("immutable: read-only in TS".to_string());
            }
            if !n.holes.is_empty() {
                notes.push(format!("holes (never visible): {}", n.holes.join("; ")));
            }
            if !notes.is_empty() {
                let _ = writeln!(s, "/** {} */", notes.join(". "));
            }
            let base = if resource { "Resource" } else { "Component" };
            let _ = writeln!(s, "export declare class {name} extends {base}({path:?}, {{ {} }}) {{}}", fields.join(", "));
        } else {
            let ty = ts_type(&registry, path, 0);
            if let Some(r) = native.reflect_tier(path) {
                let _ = writeln!(s, "/** Reflect tier: {}. */", r.reason.replace("*/", "* /"));
            }
            let base = if resource { "ReflectResource" } else { "ReflectComponent" };
            let _ = writeln!(s, "export declare class {name} extends {base}<{ty}>({path:?}) {{}}");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idents_from_short_paths() {
        assert_eq!(ident("Time<()>"), "Time");
        assert_eq!(ident("Time<Virtual>"), "TimeVirtual");
        assert_eq!(ident("Handle<Mesh>"), "HandleMesh");
        assert_eq!(prop("0"), "\"0\"");
        assert_eq!(prop("linear"), "linear");
    }
}
