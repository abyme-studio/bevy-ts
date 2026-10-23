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
pub fn types_app(engine: &Engine, extra: impl Plugin) -> App {
    let mut app = App::new();
    let render = RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })),
        ..default()
    };
    app.add_plugins(headless::default_plugins(true).set(render));
    let fake = bevy_ts::fake::FakeHandle::default();
    app.add_plugins(BevyTsPlugin::new(move || Box::new(fake.backend())))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_material::ScriptMaterialPlugin)
        .add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .add_plugins(crate::camera::CamerasPlugin)
        .add_plugins(crate::input::ScriptInputPlugin)
        .add_plugins(crate::rust_events::RustEventsPlugin)
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    app
}

/// `bevy-ts types [--out <file>] [--coverage <report.json>]`. `--coverage` writes the hole
/// check's report (`crate::coverage`) instead of the types. Returns the exit code.
pub fn run(engine: &Engine, extra: impl Plugin, args: &[String]) -> i32 {
    let mut out = PathBuf::from(".bevy-ts/types/native.d.ts");
    let mut bevy: Option<(PathBuf, PathBuf)> = None;
    let mut coverage: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bevy" => match (it.next(), it.next()) {
                (Some(js), Some(dts)) => bevy = Some((PathBuf::from(js), PathBuf::from(dts))),
                _ => {
                    eprintln!("bevy-ts types: --bevy takes the bevy.js and bevy.d.ts to write");
                    return 2;
                }
            },
            "--coverage" => match it.next() {
                Some(p) => coverage = Some(PathBuf::from(p)),
                None => {
                    eprintln!("bevy-ts types: --coverage takes the report file to write");
                    return 2;
                }
            },
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
    if let Some(report) = coverage {
        let json = serde_json::to_string_pretty(&crate::coverage::walk(app.world())).expect("a report");
        return match write(&report, &json) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("bevy-ts types: {e}");
                1
            }
        };
    }
    if let Some((js_out, dts_out)) = bevy {
        let (js, dts) = emit_bevy(app.world());
        return match write(&js_out, &js).and_then(|_| write(&dts_out, &dts)) {
            Ok(()) => {
                eprintln!("bevy-ts types: wrote {} and {}", js_out.display(), dts_out.display());
                0
            }
            Err(e) => {
                eprintln!("bevy-ts types: {e}");
                1
            }
        };
    }
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

/// The TS type of a reflected type's JSON shape in `bevy_ts::reflect_value` (docs/reflection.md):
/// numbers, strings, wide integers as `number | string`, `Duration` as seconds, `Handle<T>` as an
/// asset path or `null`, `Entity` as an entity, `Color` as `ColorValue`, glam types as lane tuples,
/// options as `| null`, lists and sets as arrays, maps as records or `[key, value]` pairs, enums
/// in Bevy's shape. A nested struct's fields are optional: a written value is merged over the
/// default. `unknown` only for a type with no shape (on `reflect_value::UNCOVERED`).
pub fn reflect_ts(registry: &TypeRegistry, path: &str) -> String {
    ts_type(registry, path, 0)
}

/// glam's serde shapes: vectors and quaternions as lanes, matrices and affines flat.
fn glam_ts(path: &str) -> Option<String> {
    let name = path.strip_prefix("glam::")?;
    let lanes = |n: usize, t: &str| Some(format!("readonly [{}]", vec![t; n].join(", ")));
    let (kind, n) = match name {
        "Quat" | "DQuat" => ("number", 4),
        "Mat2" | "DMat2" => ("number", 4),
        "Mat3" | "Mat3A" | "DMat3" => ("number", 9),
        "Mat4" | "DMat4" => ("number", 16),
        "Affine2" | "DAffine2" => ("number", 6),
        "Affine3" | "Affine3A" | "DAffine3" => ("number", 12),
        _ => {
            let n = name.chars().rev().find(char::is_ascii_digit).filter(|_| name.contains("Vec"))?.to_digit(10)? as usize;
            (if name.starts_with("BVec") { "boolean" } else { "number" }, n)
        }
    };
    lanes(n, kind)
}

/// A `Handle<T>`'s TS type: an asset path, or `null` for a handle with none. reflect-assets widens
/// it to `string | AssetRef<T> | AssetValue<T> | null` for a `T` with `ReflectAsset`, named by
/// [`asset_name`].
fn handle_ts(_registry: &TypeRegistry, _path: &str) -> String {
    "string | null".into()
}

/// An asset type's TS name, by `named()`'s rule over components and assets: its short name, with
/// its crate's prefixed when two share it.
#[allow(dead_code)]
fn asset_name(registry: &TypeRegistry, id: std::any::TypeId) -> Option<String> {
    let reg = registry.get(id)?;
    let short = ident(reg.type_info().type_path_table().short_path());
    let shared = registry
        .iter()
        .filter(|r| r.data::<bevy::ecs::reflect::ReflectComponent>().is_some() || r.data::<bevy::asset::ReflectAsset>().is_some())
        .filter(|r| ident(r.type_info().type_path_table().short_path()) == short)
        .count();
    Some(match shared > 1 {
        true => format!("{}{short}", ident(reg.type_info().type_path_table().crate_name().unwrap_or(""))),
        false => short,
    })
}

thread_local! {
    /// The types `ts_type` is inside: a type met again inside itself is `any`.
    static INSIDE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn ts_type(registry: &TypeRegistry, path: &str, depth: usize) -> String {
    if INSIDE.with(|s| s.borrow().iter().any(|p| p == path)) {
        // A recursive type (`FontSource::List(Vec<FontSource>)`) refers to itself.
        return "any".into();
    }
    INSIDE.with(|s| s.borrow_mut().push(path.to_string()));
    let t = ts_type_inner(registry, path, depth);
    INSIDE.with(|s| s.borrow_mut().pop());
    t
}

fn ts_type_inner(registry: &TypeRegistry, path: &str, depth: usize) -> String {
    if bevy_ts::reflect_value::uncovered(path).is_some() {
        // reflect_value reads it as absent and writes none.
        return "never".into();
    }
    const NUMBERS: [&str; 8] = ["f32", "f64", "i8", "i16", "i32", "u8", "u16", "u32"];
    if path == "bevy_camera::camera::Exposure" {
        return "{ ev100?: number }".into();
    }
    if path.starts_with("core::num::NonZero") || path == "petgraph::graph::NodeIndex" {
        return "number".into();
    }
    for range in ["core::ops::Range<", "core::ops::RangeInclusive<"] {
        if let Some(inner) = path.strip_prefix(range).and_then(|r| r.strip_suffix('>')) {
            let t = ts_type(registry, inner, depth + 1);
            return format!("{{ start: {t}; end: {t} }}");
        }
    }
    const WIDE: [&str; 6] = ["i64", "i128", "isize", "u64", "u128", "usize"];
    if NUMBERS.contains(&path) || path == "core::time::Duration" {
        return "number".into();
    }
    if WIDE.contains(&path) {
        return "number | string".into();
    }
    match path {
        "bool" => return "boolean".into(),
        "alloc::string::String" | "&str" | "alloc::borrow::Cow<str>" | "char" | "smol_str::SmolStr" | "std::path::PathBuf" | "uuid::Uuid" => {
            return "string".into();
        }
        "bevy_ecs::entity::Entity" => return "Entity".into(),
        "bevy_ecs::name::Name" => return "string".into(),
        "bevy_color::color::Color" => return "ColorValue".into(),
        _ => {}
    }
    if let Some(t) = glam_ts(path) {
        return t;
    }
    if path.starts_with("bevy_asset::handle::Handle<") {
        return handle_ts(registry, path);
    }
    if depth > 12 {
        return "unknown".into();
    }
    let Some(reg) = registry.get_with_type_path(path) else { return "unknown".into() };
    let items = |fields: &mut dyn Iterator<Item = &str>| fields.map(|f| ts_type(registry, f, depth + 1)).collect::<Vec<_>>().join(", ");
    match reg.type_info() {
        TypeInfo::Struct(s) => {
            let fields: Vec<String> =
                s.iter().map(|f| format!("{}?: {}", prop(f.name()), ts_type(registry, f.type_path(), depth + 1))).collect();
            format!("{{ {} }}", fields.join("; "))
        }
        TypeInfo::TupleStruct(s) if s.field_len() == 1 => ts_type(registry, s.field_at(0).unwrap().type_path(), depth + 1),
        TypeInfo::TupleStruct(s) => format!("readonly [{}]", items(&mut s.iter().map(|f| f.type_path()))),
        TypeInfo::Tuple(t) => format!("readonly [{}]", items(&mut t.iter().map(|f| f.type_path()))),
        TypeInfo::List(l) => format!("readonly {}[]", paren(ts_type(registry, l.item_ty().path(), depth + 1))),
        TypeInfo::Array(a) if a.capacity() <= 4 => format!("readonly [{}]", vec![ts_type(registry, a.item_ty().path(), depth + 1); a.capacity()].join(", ")),
        TypeInfo::Array(a) => format!("readonly {}[]", paren(ts_type(registry, a.item_ty().path(), depth + 1))),
        TypeInfo::Set(s) => format!("readonly {}[]", paren(ts_type(registry, s.value_ty().path(), depth + 1))),
        TypeInfo::Map(m) if bevy_ts::reflect_value::string_key(m.key_ty().path()) => {
            format!("Record<string, {}>", ts_type(registry, m.value_ty().path(), depth + 1))
        }
        TypeInfo::Map(m) => {
            format!("readonly (readonly [{}, {}])[]", ts_type(registry, m.key_ty().path(), depth + 1), ts_type(registry, m.value_ty().path(), depth + 1))
        }
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
                        format!("{{ {}: readonly [{}] }}", prop(t.name()), items(&mut t.iter().map(|f| f.type_path())))
                    }
                    bevy::reflect::enums::VariantInfo::Struct(s) => {
                        let fields: Vec<String> =
                            s.iter().map(|f| format!("{}?: {}", prop(f.name()), ts_type(registry, f.type_path(), depth + 1))).collect();
                        format!("{{ {}: {{ {} }} }}", prop(s.name()), fields.join("; "))
                    }
                })
                .collect();
            variants.join(" | ")
        }
        _ => "unknown".into(),
    }
}

/// `static readonly tsxProps` for a layout-tier class's writable holes: TSX props TS system rows
/// do not carry. `None` when it has none.
fn hole_props(registry: &TypeRegistry, n: &bevy_ts::layout::NativeComponent) -> Option<String> {
    if n.reflect_holes.is_empty() {
        return None;
    }
    let props: Vec<String> = n.reflect_holes.iter().map(|h| format!("{}?: {}", prop(&h.name), ts_type(registry, &h.type_path, 1))).collect();
    Some(format!("  static readonly tsxProps: {{ {} }};", props.join("; ")))
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

/// Inner types bevy-ts's own declarations name, so a claim of them is typed.
const CLOCKS: &[&str] = &["Time<Real>", "Time<Virtual>", "Time<Fixed>"];

/// One `stands_for` claim: the spelling, and the module and TS name of the export.
pub struct Claim<'a> {
    pub spelling: &'a str,
    pub module: &'a str,
    pub ts: &'a str,
}

/// `ResClaims` and `ResMutClaims` entries for exports with `stands_for`, so `ResMut<T>` types a
/// claimed spelling as its export. An entry needs its inner type declared in TS (`declared`): a
/// class this file declares, or a clock bevy-ts declares (`Time<Virtual>`). Any other claim is
/// listed in a comment only, so `native.d.ts` always type-checks. Empty without claims.
pub fn claims_ts(claims: &[Claim], declared: &dyn Fn(&str) -> bool) -> String {
    let mut res = String::new();
    let mut res_mut = String::new();
    let mut untyped = Vec::new();
    for c in claims {
        let (out, inner) = match c.spelling.strip_prefix("ResMut<") {
            Some(rest) => (&mut res_mut, rest),
            None => (&mut res, c.spelling.strip_prefix("Res<").unwrap_or(c.spelling)),
        };
        let inner = inner.strip_suffix('>').unwrap_or(inner);
        if declared(inner) {
            let _ = writeln!(out, "    {:?}: [{inner}, import({:?}).{}];", c.spelling, c.module, c.ts);
        } else {
            untyped.push(format!("{} ({})", c.spelling, c.ts));
        }
    }
    if res.is_empty() && res_mut.is_empty() && untyped.is_empty() {
        return String::new();
    }
    let mut s = String::from("\n/** Exports standing for a Bevy spelling (`stands_for`): `ResMut<T>` types them as the export. */\n");
    if !untyped.is_empty() {
        let _ = writeln!(s, "// Claimed, but TS declares no such type yet, so not typed here: {}.", untyped.join(", "));
    }
    if res.is_empty() && res_mut.is_empty() {
        return s;
    }
    s.push_str("declare module \"bevy-ts\" {\n");
    for (name, body) in [("ResClaims", res), ("ResMutClaims", res_mut)] {
        if !body.is_empty() {
            let _ = write!(s, "  interface {name} {{\n{body}  }}\n");
        }
    }
    s.push_str("}\n");
    s
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

/// Every reflected component (and layout-tier resource) with a TS identifier: its short name,
/// prefixed by its crate when two types share one. Sorted by type path.
fn named<'r>(registry: &'r bevy::reflect::TypeRegistry, native: &NativeLayouts) -> Vec<(String, String, &'r TypeRegistration)> {
    let mut entries: BTreeMap<String, (String, &TypeRegistration)> = BTreeMap::new();
    for reg in registry.iter().filter(|r| r.data::<bevy::ecs::reflect::ReflectComponent>().is_some()) {
        let path = reg.type_info().type_path().to_string();
        if is_resource(reg) && native.get(&path).is_none_or(|n| n.type_path != path) {
            // A resource TS cannot read in place has no TS handle yet.
            continue;
        }
        entries.insert(path, (ident(reg.type_info().type_path_table().short_path()), reg));
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (name, _) in entries.values() {
        *seen.entry(name.clone()).or_default() += 1;
    }
    let mut used = BTreeSet::new();
    let mut out = Vec::new();
    for (path, (short, reg)) in entries {
        let mut name = short.clone();
        if seen[&short] > 1 {
            let krate = reg.type_info().type_path_table().crate_name().unwrap_or("");
            name = format!("{}{}", ident(krate), short);
        }
        while !used.insert(name.clone()) {
            name.push('_');
        }
        out.push((path, name, reg));
    }
    out
}

/// A layout field's type as a JS field-type expression (`vec3`, `enumOf("A", "B")`).
fn field_js(t: &FieldType) -> String {
    match t {
        FieldType::Enum { variants, .. } => format!("enumOf({})", variants.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(", ")),
        FieldType::Array { of, len } => format!("array({}, {len})", field_js(of)),
        other => other.ts_name(),
    }
}

/// A reflect-tier component's element props as `Reflected`'s argument: its named fields, or
/// `value` for a tuple struct of one field (`<WorldAssetRoot value="..." />`); none otherwise.
fn element_props(registry: &TypeRegistry, reg: &TypeRegistration) -> String {
    let fields: Vec<String> = match reg.type_info() {
        TypeInfo::Struct(st) => st.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(registry, f.type_path(), 1))).collect(),
        TypeInfo::TupleStruct(t) if t.field_len() == 1 => vec![format!("value: {}", ts_type(registry, t.field_at(0).unwrap().type_path(), 1))],
        _ => vec![],
    };
    if fields.is_empty() { String::new() } else { format!("<{{ {} }}>", fields.join("; ")) }
}

/// `bevy-ts/bevy` for the prebuilt engine (native task 3.5): a JS module of handles and its
/// declarations, generated from the engine's registry. Layout-tier components are column
/// classes, reflect-tier ones handles for filters and TSX. `Transform` and `Time` come from
/// `bevy-ts` itself and are re-exported.
pub fn emit_bevy(world: &World) -> (String, String) {
    let registry = world.resource::<AppTypeRegistry>().read();
    let native = world.resource::<NativeLayouts>();
    let head = "// Generated by `scripts/gen-bevy-types.sh` (bevy-ts types --bevy) from the prebuilt engine's\n\
                // type registry. Do not edit.\n";
    let mut js = format!(
        "{head}// `bevy-ts/bevy`: Rust components TS can name. Column classes have a native layout read from\n\
         // reflection; the rest are handles for With/Without filters and TSX.\n\
         import {{ array, bool, entity, enumOf, f32, f64, i16, i32, i8, nativeComponent, quat, rustComponent, u16, u32, u8, vec2, vec3, vec4 }} from \"bevy-ts\";\n\n\
         export {{ Time, Transform }} from \"bevy-ts\";\n"
    );
    let mut dts = format!(
        "{head}// `bevy-ts/bevy`: Rust components TS can name.\n\
         import {{ Component, ReflectComponent }} from \"bevy-ts\";\n\
         import type {{ bool, ColorValue, Entity, f32, f64, i16, i32, i8, Quat, u16, u32, u8, Vec2, Vec3, Vec4 }} from \"bevy-ts\";\n\n\
         type Reflected<F = {{}}> = abstract new () => ReflectComponent & F;\n\n\
         export {{ Time, Transform }} from \"bevy-ts\";\n"
    );
    for (path, name, reg) in named(&registry, native) {
        if name == "Transform" || name == "Time" || is_resource(reg) {
            continue;
        }
        js.push('\n');
        dts.push('\n');
        match native.get(&path).filter(|n| n.type_path == path) {
            Some(n) => {
                let fields: Vec<String> = n.layout.fields.iter().map(|f| format!("{}: {}", prop(&f.name), field_js(&f.ty))).collect();
                let _ = writeln!(js, "export class {name} extends nativeComponent({:?}, {{ {} }}) {{}}", n.ts_name, fields.join(", "));
                if !n.holes.is_empty() || !n.writable {
                    let mut notes = Vec::new();
                    if !n.writable {
                        notes.push("Immutable: read-only in TS.".to_string());
                    }
                    if !n.holes.is_empty() {
                        notes.push(format!("Holes, never visible: {}.", n.holes.join("; ")));
                    }
                    let _ = writeln!(dts, "/** {} */", notes.join(" ").replace("*/", "* /"));
                }
                let _ = writeln!(dts, "export declare class {name} extends Component {{\n  static readonly path: {path:?};\n  static readonly native: true;");
                // An element's props where they are not the columns: a component never inserted
                // from bytes is written through reflection, holes included.
                let element = match reg.type_info() {
                    TypeInfo::Struct(st) if !n.insertable && !n.holes.is_empty() => Some(
                        st.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(&registry, f.type_path(), 1))).collect::<Vec<_>>().join("; "),
                    ),
                    _ => None,
                };
                match element {
                    Some(props) => {
                        let _ = writeln!(dts, "  /** Its props as an element. */\n  static readonly __element: {{ {props} }};");
                    }
                    // Inserted from bytes: its writable holes are TSX props beside the columns.
                    None => {
                        if let Some(h) = hole_props(&registry, n) {
                            let _ = writeln!(dts, "{h}");
                        }
                    }
                }
                for f in &n.layout.fields {
                    let ro = if f.ty.lanes().is_some() || !n.writable { "readonly " } else { "" };
                    let _ = writeln!(dts, "  declare {ro}{}: {};", prop(&f.name), field_ts(&f.ty));
                }
                dts.push_str("}\n");
            }
            None => {
                let handle = native.reflect_tier(&path).and_then(|_| {
                    let short = reg.type_info().type_path_table().short_path();
                    native.get(short).is_none().then(|| short.to_string())
                });
                let handle = match handle {
                    Some(short) if native.ambiguous(&short).is_none() && name == ident(&short) => short,
                    _ => path.clone(),
                };
                let _ = writeln!(js, "export const {name} = rustComponent({handle:?});");
                if name == "Name" {
                    let _ = writeln!(dts, "/** `Name`; as an element, `<Name value=\"...\" />` (or `<entity name=\"...\">`). */");
                    let _ = writeln!(dts, "export declare const Name: Reflected & {{ readonly __nameElement: true }};");
                } else if let TypeInfo::Enum(_) = reg.type_info() {
                    // An enum component takes its whole value as `value`.
                    let _ = writeln!(dts, "/** As an element, `<{name} value={{...}} />`: the whole value, in Bevy's shape. */");
                    let _ = writeln!(dts, "export declare const {name}: Reflected & {{ readonly tsxProps: {{ value?: {name}Value }} }};");
                    let _ = writeln!(dts, "export type {name}Value = {};", ts_type(&registry, &path, 0));
                } else {
                    let _ = writeln!(dts, "export declare const {name}: Reflected{};", element_props(&registry, reg));
                }
                let _ = writeln!(dts, "export type {name} = InstanceType<typeof {name}>;");
            }
        }
    }
    (js, dts)
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
         import type { bool, ColorValue, Entity, f32, f64, i16, i32, i8, Quat, u16, u32, u8, Vec2, Vec3, Vec4 } from \"bevy-ts\";\n\n",
    );
    let sets: Vec<String> = world.get_resource::<TsSets>().map(|t| t.iter().map(|x| format!("{:?}", x.name)).collect()).unwrap_or_default();
    let _ = writeln!(s, "/** System sets exposed with `app.ts_set`, for `.before`/`.after(systemSet(name))`. */");
    let _ = writeln!(s, "export type NativeSet = {};", if sets.is_empty() { "never".to_string() } else { sets.join(" | ") });
    let schedules: Vec<String> =
        world.get_resource::<bevy_ts::schedules::TsSchedules>().map(|t| t.iter().map(|x| format!("{:?}", x.name)).collect()).unwrap_or_default();
    let _ = writeln!(s, "/** Schedules exposed with `app.ts_schedule`, for `schedule(name)`. */");
    let _ = writeln!(s, "export type NativeSchedule = {};", if schedules.is_empty() { "never".to_string() } else { schedules.join(" | ") });
    let conditions: Vec<String> =
        world.get_resource::<bevy_ts::conditions::TsConditions>().map(|t| t.names().map(|x| format!("{x:?}")).collect()).unwrap_or_default();
    let _ = writeln!(s, "/** Conditions exposed with `app.ts_condition`, for `nativeCondition(name, args)`. */");
    let _ = writeln!(s, "export type NativeCondition = {};", if conditions.is_empty() { "never".to_string() } else { conditions.join(" | ") });

    let classes = named(&registry, native);
    if let Some(host) = world.get_resource::<bevy_ts::host::HostParams>() {
        let claims: Vec<Claim> = host.specs().filter_map(|p| Some(Claim { spelling: p.stands_for?, module: p.module, ts: p.ts })).collect();
        s.push_str(&claims_ts(&claims, &|t| CLOCKS.contains(&t) || classes.iter().any(|(_, name, _)| name == t)));
    }

    for (path, name, reg) in classes {
        let path = &path;
        let resource = is_resource(reg);
        let layout = native.get(path).filter(|n| &n.type_path == path);
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
        if let Some(h) = layout.and_then(|n| hole_props(&registry, n)) {
            let _ = writeln!(s, "{h}");
        }
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
    fn a_claim_is_typed_only_when_ts_declares_its_inner_type() {
        let claims = [
            Claim { spelling: "ResMut<Wind>", module: "bevy-ts/claims-test", ts: "WindEdit" },
            Claim { spelling: "ResMut<Time<Virtual>>", module: "bevy-ts/claims-test", ts: "VirtualTime" },
            Claim { spelling: "ResMut<NavMesh>", module: "bevy-ts/claims-test", ts: "NavEdit" },
        ];
        let out = claims_ts(&claims, &|t| t == "Wind" || CLOCKS.contains(&t));
        assert!(out.contains("not typed here: ResMut<NavMesh> (NavEdit)"), "{out}");
        assert!(!out.contains("[NavMesh"), "{out}");
        // packages/bevy-ts/test/claims.ts holds this block verbatim, and `tsc -p test` checks it.
        let block = out.split_once("declare module").map(|(_, b)| format!("declare module{b}")).unwrap();
        let fixture = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/bevy-ts/test/claims.ts")).unwrap();
        assert!(fixture.contains(&block), "packages/bevy-ts/test/claims.ts no longer holds what claims_ts emits:\n{block}");
    }

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
        bevy_ts::time_conditions::expose_all(&mut app);
        world.insert_resource(app.world().resource::<TsSets>().clone());
        world.insert_resource(app.world().resource::<bevy_ts::conditions::TsConditions>().clone());
        let out = emit(&world);
        assert!(out.contains("export type NativeSet = \"rapier:step\";"), "{out}");
        assert!(out.contains("export type NativeCondition = \"bevy:on-real-timer\" | \"bevy:on-timer\" | \"bevy:once-after-delay\" | \"bevy:once-after-real-delay\" | \"bevy:paused\" | \"bevy:repeating-after-delay\" | \"bevy:repeating-after-real-delay\";"), "{out}");
        assert!(out.contains("export declare class Transform extends Component {\n  static readonly path: \"bevy_transform::components::transform::Transform\";\n  declare readonly translation: Vec3;"), "{out}");
        assert!(out.contains("declare value: \"Inherited\" | \"Hidden\" | \"Visible\";"), "{out}");
        assert_eq!(out, emit(&world), "deterministic");
    }

    /// The spec's inventory: collections, a `Duration`, an `Option<Entity>` and wide integers.
    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Inventory {
        items: Vec<String>,
        counts: bevy::platform::collections::HashMap<String, u32>,
        by_entity: bevy::platform::collections::HashMap<Entity, f32>,
        cooldown: std::time::Duration,
        owner: Option<Entity>,
        serial: u64,
        slots: [u8; 3],
        span: std::ops::Range<f32>,
        icon: Handle<Image>,
    }

    #[test]
    fn every_kind_has_a_ts_type() {
        let mut registry = TypeRegistry::default();
        registry.register::<Inventory>();
        let t = |p: &str| reflect_ts(&registry, p);
        assert_eq!(t("alloc::vec::Vec<alloc::string::String>"), "readonly string[]");
        assert_eq!(t("core::time::Duration"), "number");
        assert_eq!(t("u64"), "number | string");
        assert_eq!(t("glam::Mat4"), format!("readonly [{}]", vec!["number"; 16].join(", ")));
        let inv = registry.get(std::any::TypeId::of::<Inventory>()).unwrap().type_info().type_path().to_string();
        let ts = t(&inv);
        for want in [
            "items?: readonly string[]",
            "counts?: Record<string, number>",
            "by_entity?: readonly (readonly [Entity, number])[]",
            "cooldown?: number",
            "owner?: Entity | null",
            "serial?: number | string",
            "slots?: readonly [number, number, number]",
            "span?: { start: number; end: number }",
            "icon?: string | null",
        ] {
            assert!(ts.contains(want), "{want} not in {ts}");
        }
        assert!(!ts.contains("unknown"), "{ts}");
    }

    #[test]
    fn the_engine_declares_cameras() {
        let mut app = App::new();
        app.add_plugins(crate::camera::CamerasPlugin);
        let out = emit_modules(app.world());
        assert!(out.contains("declare module \"bevy-ts/camera\" {"), "{out}");
        assert!(out.contains("Cameras"), "{out}");
        assert!(out.contains("worldToViewport(camera: Entity"), "{out}");
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
