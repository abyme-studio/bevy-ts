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
    let modules = emit_modules(app.world());
    let modules_out = out.with_file_name(format!("{}-modules.d.ts", out.file_stem().and_then(|s| s.to_str()).unwrap_or("native").trim_end_matches(".d")));
    if let Err(e) = write(&out, &text).and_then(|_| if modules.is_empty() { Ok(()) } else { write(&modules_out, &modules) }) {
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

/// A layout-tier field's declared TS type, as TS components write theirs (ts-components).
fn field_ts(t: &FieldType) -> String {
    match t {
        FieldType::Vec2 => "Vec2".into(),
        FieldType::Vec3 => "Vec3".into(),
        FieldType::Vec4 => "Vec4".into(),
        FieldType::Quat => "Quat".into(),
        FieldType::Entity => "Entity".into(),
        FieldType::Enum { variants, .. } => variants.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(" | "),
        FieldType::Array { of, len } => format!("readonly [{}]", vec![field_ts(of); *len as usize].join(", ")),
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
        "bevy_ecs::name::Name" => return "string".into(),
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
    if t.contains(' ') { format!("({t})") } else { t }
}

fn prop(name: &str) -> String {
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain { name.to_string() } else { format!("{name:?}") }
}

fn is_resource(reg: &TypeRegistration) -> bool {
    reg.data::<bevy::ecs::reflect::ReflectResource>().is_some()
}

/// Ambient declarations of the built-in modules ports ship (`bevy-ts/rapier`) and the params
/// they export: a script file with no top-level imports, so `declare module` declares rather
/// than augments. Empty when no module is registered.
pub fn emit_modules(world: &World) -> String {
    let mut s = String::new();
    let mut modules: BTreeMap<String, String> = bevy_ts::modules::all().into_iter().map(|(k, m)| (k, m.dts)).collect();
    if let Some(host) = world.get_resource::<bevy_ts::host::HostParams>() {
        for (module, decls) in host.declarations() {
            modules.entry(module.to_string()).or_default().push_str(&decls);
        }
    }
    for (module, decls) in modules {
        let _ = writeln!(s, "declare module {module:?} {{");
        if !decls.contains("import type") {
            s.push_str("  import type { Entity } from \"bevy-ts\";\n");
        }
        for line in decls.lines() {
            // Already ambient inside `declare module`.
            let _ = writeln!(s, "  {}", line.replacen("export declare ", "export ", 1));
        }
        s.push_str("}\n\n");
    }
    if !s.is_empty() {
        s.insert_str(0, "// Generated by `bevy-ts types`: the TS modules Rust crates ship. Do not edit.\n\n");
    }
    s
}

/// The `.d.ts` text for a world whose native layouts are filled: one class per reflected
/// component and layout-tier resource, its fields read from the registry, `static path` its type
/// path.
pub fn emit(world: &World) -> String {
    let registry = world.resource::<AppTypeRegistry>().read();
    let native = world.resource::<NativeLayouts>();
    let mut s = String::from(
        "// Generated by `bevy-ts types` from the engine's type registry. Do not edit.\n\
         // Component and Resource classes are columns in TS systems; ReflectComponent classes are\n\
         // typed values for TSX, the protocol and With/Without filters.\n\n\
         import { Component, ReflectComponent, Resource } from \"bevy-ts\";\n\
         import type { bool, Entity, f32, f64, i16, i32, i8, Quat, u16, u32, u8, Vec2, Vec3, Vec4 } from \"bevy-ts\";\n\n",
    );
    let sets: Vec<String> = world.get_resource::<TsSets>().map(|t| t.iter().map(|x| format!("{:?}", x.name)).collect()).unwrap_or_default();
    let _ = writeln!(s, "/** System sets exposed with `app.ts_set`, for `.before`/`.after(systemSet(name))`. */");
    let _ = writeln!(s, "export type NativeSet = {};", if sets.is_empty() { "never".to_string() } else { sets.join(" | ") });

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
        let resource = is_resource(reg);
        let layout = native.get(path).filter(|n| &n.type_path == path);
        if resource && layout.is_none() {
            // A resource TS cannot read in place has no TS handle yet.
            continue;
        }
        let mut name = short.clone();
        if seen[short] > 1 {
            let krate = reg.type_info().type_path_table().crate_name().unwrap_or("");
            name = format!("{}{}", ident(krate), short);
        }
        while !used.insert(name.clone()) {
            name.push('_');
        }
        s.push('\n');
        let mut fields = Vec::new();
        let base = if let Some(n) = layout {
            let mut notes = Vec::new();
            if !n.writable {
                notes.push("Immutable: read-only in TS.".to_string());
            }
            if !n.holes.is_empty() {
                notes.push(format!("Holes, never visible: {}.", n.holes.join("; ")));
            }
            if !notes.is_empty() {
                let _ = writeln!(s, "/** {} */", notes.join(" ").replace("*/", "* /"));
            }
            for f in &n.layout.fields {
                let ro = if f.ty.lanes().is_some() || !n.writable { "readonly " } else { "" };
                fields.push(format!("declare {ro}{}: {};", prop(&f.name), field_ts(&f.ty)));
            }
            if resource { "Resource" } else { "Component" }
        } else {
            if let Some(r) = native.reflect_tier(path) {
                let _ = writeln!(s, "/** Reflect tier: {}. */", r.reason.replace("*/", "* /"));
            }
            if let TypeInfo::Struct(st) = reg.type_info() {
                for f in st.iter() {
                    fields.push(format!("declare {}: {};", prop(f.name()), ts_type(&registry, f.type_path(), 1)));
                }
            } else {
                fields.push(format!("declare value: {};", ts_type(&registry, path, 0)));
            }
            "ReflectComponent"
        };
        let _ = writeln!(s, "export declare class {name} extends {base} {{");
        let _ = writeln!(s, "  static readonly path: {path:?};");
        for f in fields {
            let _ = writeln!(s, "  {f}");
        }
        s.push_str("}\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_with_paths_fields_and_sets() {
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        registry.write().register::<Transform>();
        registry.write().register::<Visibility>();
        world.insert_resource(registry);
        bevy_ts::layout_reflect::fill(&mut world);
        let mut app = App::new();
        bevy_ts::sets::expose(&mut app, "rapier:step", FixedUpdate, bevy::transform::TransformSystems::Propagate);
        world.insert_resource(app.world().resource::<TsSets>().clone());
        let out = emit(&world);
        assert!(out.contains("export type NativeSet = \"rapier:step\";"), "{out}");
        assert!(out.contains("export declare class Transform extends Component {\n  static readonly path: \"bevy_transform::components::transform::Transform\";\n  declare readonly translation: Vec3;"), "{out}");
        assert!(out.contains("declare value: \"Inherited\" | \"Hidden\" | \"Visible\";"), "{out}");
        assert_eq!(out, emit(&world), "deterministic");
    }

    #[test]
    fn idents_from_short_paths() {
        assert_eq!(ident("Time<()>"), "Time");
        assert_eq!(ident("Time<Virtual>"), "TimeVirtual");
        assert_eq!(ident("Handle<Mesh>"), "HandleMesh");
        assert_eq!(prop("0"), "\"0\"");
        assert_eq!(prop("linear"), "linear");
    }
}
