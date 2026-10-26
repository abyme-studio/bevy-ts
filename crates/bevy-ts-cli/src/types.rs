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
    crate::audio::asset_half(&mut app);
    let fake = bevy_ts::fake::FakeHandle::default();
    app.add_plugins(BevyTsPlugin::new(move || Box::new(fake.backend())))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_material::ScriptMaterialPlugin)
        .add_plugins(bevy_ts_material::WorldTextPlugin)
        .add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .add_plugins(crate::camera::CamerasPlugin)
        .add_plugins(crate::gizmos::GizmosPlugin)
        .add_plugins(crate::input::ScriptInputPlugin)
        .add_plugins(crate::audio::ScriptAudioPlugin)
        .add_plugins(crate::rust_events::RustEventsPlugin)
        .add_plugins(crate::diagnostics::ScriptDiagnosticsPlugin)
        .add_plugins(crate::session::ScriptSessionPlugin)
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

/// A `Handle<T>`'s TS type: an asset path, or `null` for a handle with none; for a `T` with
/// `ReflectAsset`, also a ref from `assets.add`, a value `T.of(...)`, and `T.load(...)` for a `T`
/// with loader settings (reflect-assets), `T` named by [`asset_name`]; for a hand-mapped `T` with
/// no `Reflect` (`GizmoAsset`, whose class another module declares), a ref or value of any asset
/// with `T`'s type path (fields `{}`, not `unknown`, which the coverage walk reads as a gap).
fn handle_ts(registry: &TypeRegistry, path: &str) -> String {
    let id = registry.get_with_type_path(path).and_then(|r| r.data::<bevy::asset::ReflectHandle>()).map(|h| h.asset_type_id());
    let asset = id
        .filter(|id| registry.get_type_data::<bevy::asset::ReflectAsset>(*id).is_some())
        .and_then(|id| Some((asset_name(registry, id)?, registry.get(id)?.type_info().type_path())));
    let hand = id.filter(|id| bevy_ts::reflect_asset::is_hand_mapped(*id)).and_then(|_| path.strip_prefix("bevy_asset::handle::Handle<")?.strip_suffix('>'));
    match (asset, hand) {
        (Some((name, p)), _) if bevy_ts::reflect_asset::loadable(p) => format!("string | AssetRef<{name}> | AssetValue<{name}> | AssetLoad<{name}> | null"),
        (Some((name, _)), _) => format!("string | AssetRef<{name}> | AssetValue<{name}> | null"),
        (None, Some(t)) => format!("string | AssetRef<AssetFields<{t:?}, {{}}>> | AssetValue<AssetFields<{t:?}, {{}}>> | null"),
        (None, None) => "string | null".into(),
    }
}

/// Every reflected asset type with its TS name by [`asset_name`], sorted by type path.
fn assets_named(registry: &TypeRegistry) -> Vec<(String, String, &TypeRegistration)> {
    let mut out: Vec<_> = registry
        .iter()
        .filter(|r| r.data::<bevy::asset::ReflectAsset>().is_some())
        .filter_map(|r| Some((r.type_info().type_path().to_string(), asset_name(registry, r.type_id())?, r)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A hand-mapped asset's fields (`app.ts_hand_map`): the JSON its entry reads and writes.
enum HandFields {
    /// Each field by the reflected type it is written as (`bevy_ts::reflect_asset`'s `Image`).
    Reflected(&'static [(&'static str, &'static str)]),
    /// The fields as TS, for a builder TS holds as JSON (change `animation-control`).
    Ts(&'static str),
}

const HAND_FIELDS: &[(&str, HandFields)] = &[
    ("bevy_image::image::Image", HandFields::Reflected(&[("sampler", "bevy_image::image::ImageSampler")])),
    (
        "bevy_animation::graph::AnimationGraph",
        HandFields::Ts(
            "graph: { nodes: { node_type: \"Blend\" | \"Add\" | { Clip: string | AssetRef<AnimationClip> | AssetValue<AnimationClip> }; mask: number; weight: number }[]; \
             node_holes: never[]; edge_property: \"directed\"; edges: [number, number, null][] }; root: number; \
             mask_groups: [string | { readonly names: readonly string[] }, number][]",
        ),
    ),
    (
        "bevy_animation::AnimationClip",
        HandFields::Ts("curves: [string | { readonly names: readonly string[] }, { property: { component: string; field: string }; curve: unknown }][] | null; duration?: number"),
    ),
];

/// The settings `T.load(path, settings)` takes, by the reflected type each is written as;
/// `bevy_ts::reflect_asset`'s loader entries.
const LOAD_SETTINGS: &[(&str, &[(&str, &str)])] = &[("bevy_image::image::Image", &[("sampler", "bevy_image::image::ImageSampler"), ("is_srgb", "bool")])];

/// A `bevy-ts/bevy` class written by hand in place of the generated one (change
/// `animation-control`): a builder TS holds as JSON for a hand-mapped asset
/// (`app.ts_hand_map`), or statics on a component. Its JS and d.ts replace the generated
/// lines; an asset's fields are in `HAND_FIELDS`.
struct Hand {
    path: &'static str,
    js: &'static str,
    dts: &'static str,
}

const HAND: &[Hand] = &[
    Hand {
        path: "bevy_animation::AnimationTargetId",
        js: include_str!("hand_ts/animation_target_id.js"),
        dts: include_str!("hand_ts/animation_target_id.d.ts"),
    },
    Hand {
        path: "bevy_animation::graph::AnimationGraph",
        js: include_str!("hand_ts/animation_graph.js"),
        dts: include_str!("hand_ts/animation_graph.d.ts"),
    },
    Hand {
        path: "bevy_animation::AnimationClip",
        js: include_str!("hand_ts/animation_clip.js"),
        dts: include_str!("hand_ts/animation_clip.d.ts"),
    },
];

fn hand(path: &str) -> Option<&'static Hand> {
    HAND.iter().find(|h| h.path == path)
}

/// An asset's fields as `bevy-ts/bevy` types them: as the writer takes them, tagged with its type
/// path so refs of two assets never type as each other.
fn asset_fields(registry: &TypeRegistry, path: &str, reg: &TypeRegistration) -> String {
    let hand = HAND_FIELDS.iter().find(|(p, _)| *p == path).map(|(_, f)| f);
    let fields: Vec<String> = match (hand, reg.type_info()) {
        (Some(HandFields::Ts(fields)), _) => vec![fields.to_string()],
        (Some(HandFields::Reflected(fields)), _) => fields.iter().map(|(f, ty)| format!("{}: {}", prop(f), ts_type(registry, ty, 1))).collect(),
        (None, TypeInfo::Struct(st)) if bevy_ts::reflect_value::uncovered(path).is_none() => {
            st.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(registry, f.type_path(), 1))).collect()
        }
        _ => vec![],
    };
    format!("AssetFields<{path:?}, {{ {} }}>", fields.join("; "))
}

/// An asset type's TS name, by `named()`'s rule over components and assets: its short name, with
/// its crate's prefixed when two share it.
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

pub(crate) fn ts_type(registry: &TypeRegistry, path: &str, depth: usize) -> String {
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

/// `static readonly tsxProps` for a layout-tier class's writable holes: TSX props, and keys `.of`
/// takes for commands, that TS system rows do not carry. `None` when it has none.
fn hole_props(registry: &TypeRegistry, n: &bevy_ts::layout::NativeComponent) -> Option<String> {
    if n.reflect_holes.is_empty() {
        return None;
    }
    let props: Vec<String> = n.reflect_holes.iter().map(|h| format!("{}?: {}", prop(&h.name), ts_type(registry, &h.type_path, 1))).collect();
    Some(format!("  static readonly tsxProps: {{ {} }};", props.join("; ")))
}

/// A layout-tier class's holes as members (`color: ColorValue`), as TS systems and `ref.get` read
/// them: every field that is not a column and not of an uncovered type. `None` without any.
fn hole_members(registry: &TypeRegistry, reg: &TypeRegistration, n: &bevy_ts::layout::NativeComponent) -> Option<String> {
    let fields: Vec<(String, &str)> = match reg.type_info() {
        TypeInfo::Struct(st) => st.iter().map(|f| (f.name().to_string(), f.type_path())).collect(),
        TypeInfo::TupleStruct(t) if t.field_len() > 1 => t.iter().map(|f| (f.index().to_string(), f.type_path())).collect(),
        _ => vec![],
    };
    let members: Vec<String> = fields
        .into_iter()
        .filter(|(f, path)| !n.layout.fields.iter().any(|c| &c.name == f) && bevy_ts::reflect_value::uncovered(path).is_none())
        .map(|(f, path)| format!("{}: {}", prop(&f), ts_type(registry, path, 1)))
        .collect();
    (!members.is_empty()).then(|| members.join("; "))
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
/// claimed spelling as its export. An entry needs its inner type declared in TS (`declared`, which
/// gives the TS type to name): a class this file declares, a clock bevy-ts declares
/// (`Time<Virtual>`), or a class a built-in module declares (`import("bevy-ts/gizmos").GizmoConfigStore`).
/// Any other claim is listed in a comment only, so `native.d.ts` always type-checks. Empty without
/// claims.
pub fn claims_ts(claims: &[Claim], declared: &dyn Fn(&str) -> Option<String>) -> String {
    let mut res = String::new();
    let mut res_mut = String::new();
    let mut untyped = Vec::new();
    for c in claims {
        let (out, inner) = match c.spelling.strip_prefix("ResMut<") {
            Some(rest) => (&mut res_mut, rest),
            None => (&mut res, c.spelling.strip_prefix("Res<").unwrap_or(c.spelling)),
        };
        let inner = inner.strip_suffix('>').unwrap_or(inner);
        if let Some(t) = declared(inner) {
            let _ = writeln!(out, "    {:?}: [{t}, import({:?}).{}];", c.spelling, c.module, c.ts);
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
    if let Some(types) = crate::gizmos::module_types(world) {
        modules.entry(bevy_ts_native::gizmos::MODULE.to_string()).or_default().push_str(&types);
    }
    for (module, decls) in modules {
        let _ = writeln!(s, "declare module {module:?} {{");
        // The module's own declarations and its params' each import what they name from
        // `bevy-ts`; one import of them all, as two imports of a name collide.
        let (imports, rest) = bevy_ts_imports(&decls);
        if !imports.is_empty() {
            let _ = writeln!(s, "  import type {{ {} }} from \"bevy-ts\";", imports.into_iter().collect::<Vec<_>>().join(", "));
        }
        for line in rest {
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

/// Bevy generics registered at their default type argument, named as Bevy code writes them:
/// `AudioPlayer`, not `AudioPlayerAudioSource`.
const DEFAULT_ARGUMENTS: &[(&str, &str)] = &[("AudioPlayer<AudioSource>", "AudioPlayer")];

/// The names `decls` imports from `bevy-ts` with `import type { .. } from "bevy-ts";` lines, and
/// its other lines.
fn bevy_ts_imports(decls: &str) -> (BTreeSet<&str>, Vec<&str>) {
    let mut names = BTreeSet::new();
    let mut rest = Vec::new();
    for line in decls.lines() {
        let list = line.trim().strip_prefix("import type {").and_then(|l| l.strip_suffix("} from \"bevy-ts\";"));
        match list {
            Some(list) => names.extend(list.split(',').map(str::trim).filter(|n| !n.is_empty())),
            None => rest.push(line),
        }
    }
    (names, rest)
}

/// The types params stand for (`stands_for`'s `Res<GizmoConfigStore>`): `GizmoConfigStore`.
fn claimed(world: &World) -> BTreeSet<String> {
    let Some(host) = world.get_resource::<bevy_ts::host::HostParams>() else { return BTreeSet::new() };
    host.specs()
        .filter_map(|p| p.stands_for)
        .map(|s| {
            let inner = s.strip_prefix("ResMut<").or_else(|| s.strip_prefix("Res<")).unwrap_or(s);
            inner.strip_suffix('>').unwrap_or(inner).to_string()
        })
        .collect()
}

/// Every reflected component (and resource of either tier) with a TS identifier: its short name,
/// prefixed by its crate when two types share one. Sorted by type path. A reflect-tier resource a
/// param stands for (`claimed`) is the param's, so it has none.
fn named<'r>(registry: &'r bevy::reflect::TypeRegistry, native: &NativeLayouts, claimed: &BTreeSet<String>) -> Vec<(String, String, &'r TypeRegistration)> {
    let mut entries: BTreeMap<String, (String, &TypeRegistration)> = BTreeMap::new();
    for reg in registry.iter().filter(|r| r.data::<bevy::ecs::reflect::ReflectComponent>().is_some()) {
        let path = reg.type_info().type_path().to_string();
        let short = reg.type_info().type_path_table().short_path();
        if is_resource(reg)
            && native.get(&path).is_none_or(|n| n.type_path != path)
            && (native.reflect_tier(&path).is_none() || claimed.contains(short) || claimed.contains(&path))
        {
            // A resource in neither tier, or one a param stands for, has no TS handle.
            continue;
        }
        entries.insert(path, (ident(DEFAULT_ARGUMENTS.iter().find(|(full, _)| *full == short).map_or(short, |(_, bare)| bare)), reg));
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

/// A reflect-tier component's element props as `ReflectClass`'s second argument: its named fields, or
/// `value` for a tuple struct of one field (`<WorldAssetRoot value="..." />`); none otherwise.
fn element_props(registry: &TypeRegistry, reg: &TypeRegistration) -> String {
    let fields: Vec<String> = match reg.type_info() {
        TypeInfo::Struct(st) => st.iter().map(|f| format!("{}: {}", prop(f.name()), ts_type(registry, f.type_path(), 1))).collect(),
        TypeInfo::TupleStruct(t) if t.field_len() == 1 => vec![format!("value: {}", ts_type(registry, t.field_at(0).unwrap().type_path(), 1))],
        _ => vec![],
    };
    if fields.is_empty() { "{}".into() } else { format!("{{ {} }}", fields.join("; ")) }
}

/// `bevy-ts/bevy` for the prebuilt engine (native task 3.5): a JS module of handles and its
/// declarations, generated from the engine's registry. Layout-tier components are column
/// classes and layout-tier resources `Resource` classes; reflect-tier components are handles for
/// filters and TSX, and reflect-tier resources handles for `Res` and `ResMut`. `Transform` and `Time` come from `bevy-ts` itself and are re-exported.
pub fn emit_bevy(world: &World) -> (String, String) {
    let registry = world.resource::<AppTypeRegistry>().read();
    let native = world.resource::<NativeLayouts>();
    let head = "// Generated by `scripts/gen-bevy-types.sh` (bevy-ts types --bevy) from the prebuilt engine's\n\
                // type registry. Do not edit.\n";
    let mut js = format!(
        "{head}// `bevy-ts/bevy`: Rust components and resources TS can name. Column classes have a native\n\
         // layout read from reflection; the rest are handles for With/Without filters and TSX.\n\
         import {{ array, asset, bool, entity, enumOf, f32, f64, i16, i32, i8, nativeComponent, nativeResource, quat, reflectClass, rustAsset, rustEntityEvent, rustEvent, rustGeneric, rustMessage, u16, u32, u8, vec2, vec3, vec4 }} from \"bevy-ts\";\n\n\
         export {{ Time, Transform }} from \"bevy-ts\";\n"
    );
    let mut dts = format!(
        "{head}// `bevy-ts/bevy`: Rust components and resources TS can name.\n\
         import {{ Component, EntityEvent, Event, Message, Resource }} from \"bevy-ts\";\n\
         import type {{ AssetClass, AssetFields, AssetLoad, AssetProps, AssetRef, AssetType, AssetValue, bool, ColorValue, Entity, f32, f64, Holes, i16, i32, i8, Loadable, Option, Quat, ReflectClass, ReflectValue, RustClass, RustEnum, RustGeneric, RustStruct, u16, u32, u8, Vec2, Vec3, Vec4 }} from \"bevy-ts\";\n\n\
         export {{ Time, Transform }} from \"bevy-ts\";\n"
    );
    let components = named(&registry, native, &claimed(world));
    let mut taken: BTreeSet<String> = components.iter().map(|(_, n, _)| n.clone()).collect();
    taken.extend(["Time".to_string(), "Transform".to_string()]);
    for (path, name, reg) in components {
        // `Time` and its clocks are `bevy-ts`'s own (script-time).
        if name == "Transform" || name == "Time" || path.starts_with("bevy_time::time::Time<") {
            continue;
        }
        let (factory, base) = if is_resource(reg) { ("nativeResource", "Resource") } else { ("nativeComponent", "Component") };
        js.push('\n');
        dts.push('\n');
        if let Some(h) = hand(&path) {
            js.push_str(h.js);
            dts.push_str(h.dts);
            continue;
        }
        match native.get(&path).filter(|n| n.type_path == path) {
            Some(n) => {
                let fields: Vec<String> = n.layout.fields.iter().map(|f| format!("{}: {}", prop(&f.name), field_js(&f.ty))).collect();
                let _ = writeln!(js, "export class {name} extends {factory}({:?}, {{ {} }}) {{}}", n.ts_name, fields.join(", "));
                if !n.holes.is_empty() || !n.writable {
                    let mut notes = Vec::new();
                    if !n.writable {
                        notes.push("Immutable: read-only in TS.".to_string());
                    }
                    if !n.holes.is_empty() {
                        notes.push(format!("Holes, not columns: {}.", n.holes.join("; ")));
                    }
                    let _ = writeln!(dts, "/** {} */", notes.join(" ").replace("*/", "* /"));
                }
                let _ = writeln!(dts, "export declare class {name} extends {base} {{\n  static readonly path: {path:?};\n  static readonly native: true;");
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
                // Its holes, on its rows in TS systems and in `ref.get`.
                if let Some(h) = hole_members(&registry, reg, n) {
                    let _ = writeln!(dts, "export interface {name} extends Holes<{{ {h} }}> {{}}");
                }
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
                let _ = writeln!(js, "export const {name} = reflectClass({handle:?});");
                if name == "Name" {
                    let _ = writeln!(dts, "/** `Name`; as an element, `<Name value=\"...\" />` (or `<entity name=\"...\">`); `Name.of(\"...\")` in commands. */");
                    let _ = writeln!(dts, "export declare const Name: ReflectClass<string, ReflectValue<string>> & {{ readonly __nameElement: true }};");
                } else if let TypeInfo::Enum(_) = reg.type_info() {
                    // An enum component takes its whole value as `value`.
                    let _ = writeln!(dts, "/** As an element, `<{name} value={{...}} />`: the whole value, in Bevy's shape; `{name}.of(...)` in commands. */");
                    let _ = writeln!(dts, "export declare const {name}: ReflectClass<{name}Value, ReflectValue<{name}Value>> & {{ readonly tsxProps: {{ value?: {name}Value }} }};");
                    let _ = writeln!(dts, "export type {name}Value = {};", ts_type(&registry, &path, 0));
                } else if is_resource(reg) {
                    let _ = writeln!(dts, "/** `Res<{name}>` and `ResMut<{name}>` in systems, read and written through reflection. */");
                    let _ = writeln!(dts, "export declare const {name}: ReflectClass<{}, {}>;", ts_type(&registry, &path, 0), element_props(&registry, reg));
                } else {
                    let _ = writeln!(dts, "/** `{name}.of(...)` in commands: the value written over its default. */");
                    let _ = writeln!(dts, "export declare const {name}: ReflectClass<{}, {}>;", ts_type(&registry, &path, 0), element_props(&registry, reg));
                }
                let _ = writeln!(dts, "export type {name} = InstanceType<typeof {name}>;");
            }
        }
    }
    // Asset types (reflect-assets): `assets.add(T, ...)`, `T.of(...)`, and the `T` of `Handle<T>`.
    let mut assets = BTreeMap::new();
    for (path, name, reg) in assets_named(&registry) {
        taken.insert(name.clone());
        assets.insert(path.clone(), name.clone());
        let default = reg.data::<ReflectDefault>().is_some();
        js.push('\n');
        dts.push('\n');
        let settings = LOAD_SETTINGS.iter().find(|(p, _)| *p == path).map(|(_, s)| *s);
        let opts = match (default, settings.is_some()) {
            (true, false) => "",
            (true, true) => ", { load: true }",
            (false, false) => ", { of: false }",
            (false, true) => ", { of: false, load: true }",
        };
        if let Some(h) = hand(&path) {
            js.push_str(h.js);
            let _ = writeln!(dts, "export type {name} = {};", asset_fields(&registry, &path, reg));
            dts.push_str(h.dts);
            continue;
        }
        let _ = writeln!(js, "export const {name} = asset({path:?}{opts});");
        let _ = writeln!(dts, "export type {name} = {};", asset_fields(&registry, &path, reg));
        let class = if default { "AssetClass" } else { "AssetType" };
        match settings {
            Some(s) => {
                let fields: Vec<String> = s.iter().map(|(k, ty)| format!("{}?: {}", prop(k), ts_type(&registry, ty, 1))).collect();
                let _ = writeln!(dts, "export type {name}LoadSettings = {{ {} }};", fields.join("; "));
                let _ = writeln!(dts, "export declare const {name}: {class}<{name}> & Loadable<{name}, {name}LoadSettings>;");
            }
            None => _ = writeln!(dts, "export declare const {name}: {class}<{name}>;"),
        }
    }
    emit_rust_events(&registry, &mut taken, assets, &mut js, &mut dts);
    (js, dts)
}

/// A Rust message or event TS can name (rust-events): its registration and what it is.
struct RustData<'r> {
    path: String,
    reg: &'r TypeRegistration,
    message: bool,
    /// `Some(entity)` for an event: whether it is an `EntityEvent`.
    event: Option<bool>,
}

impl RustData<'_> {
    /// Its `bevy-ts` factory call.
    fn factory(&self) -> String {
        match self.event {
            None => format!("rustMessage({:?})", self.path),
            Some(entity) => {
                let f = if entity { "rustEntityEvent" } else { "rustEvent" };
                if self.message { format!("{f}({:?}, true)", self.path) } else { format!("{f}({:?})", self.path) }
            }
        }
    }

    fn kind(&self) -> &'static str {
        match self.event {
            None => "message",
            Some(true) => "entityEvent",
            Some(false) => "event",
        }
    }

    fn base(&self) -> &'static str {
        match self.event {
            None => "Message",
            Some(true) => "EntityEvent",
            Some(false) => "Event",
        }
    }

    /// Its instance type, for a generic's instance or an enum: `RustEnum<..>` or `RustStruct<..>`.
    fn instance_ts(&self, registry: &TypeRegistry) -> String {
        let value = ts_type(registry, &self.path, 1);
        match self.reg.type_info() {
            TypeInfo::Struct(_) => format!("RustStruct<{:?}, {value}>", self.kind()),
            _ => format!("RustEnum<{:?}, {value}>", self.kind()),
        }
    }
}

/// `On<E>`'s own members: a Rust event field by these names is read as `on.event.<name>`.
const ON_OWN: [&str; 5] = ["entity", "original", "propagate", "toJSON", "event"];

/// Every type with `ReflectTsMessage` or `ReflectTsEvent` as a class: a struct as a class with its
/// fields, an enum as its value's type, a generic type (`AssetEvent<A>`) as one name whose
/// instances the reader picks by type argument. An asset type a generic is instanced on gets a
/// marker to spell it (`AssetEvent<Image>`).
/// `assets` are the asset classes already emitted, by type path: a generic's argument names them.
fn emit_rust_events(registry: &TypeRegistry, taken: &mut BTreeSet<String>, assets: BTreeMap<String, String>, js: &mut String, dts: &mut String) {
    use bevy_ts::rust_events::{ReflectTsEvent, ReflectTsMessage};
    let mut all: Vec<RustData> = registry
        .iter()
        .filter_map(|reg| {
            let message = reg.data::<ReflectTsMessage>().is_some();
            let event = reg.data::<ReflectTsEvent>().map(|e| e.entity);
            (message || event.is_some()).then(|| RustData { path: reg.type_info().type_path().to_string(), reg, message, event })
        })
        .collect();
    all.sort_by(|a, b| a.path.cmp(&b.path));
    // Generic instances by their type's path without arguments.
    let mut generics: BTreeMap<String, Vec<&RustData>> = BTreeMap::new();
    let mut plain = Vec::new();
    for d in &all {
        match d.path.split_once('<') {
            Some((base, _)) => generics.entry(base.to_string()).or_default().push(d),
            None => plain.push(d),
        }
    }
    let unique = |taken: &mut BTreeSet<String>, short: &str, krate: &str| {
        let mut name = ident(short);
        if taken.contains(&name) {
            name = format!("{}{name}", ident(krate));
        }
        while !taken.insert(name.clone()) {
            name.push('_');
        }
        name
    };
    js.push_str("\n// Rust messages and events, by type path (rust-events).\n");
    dts.push_str("\n// Rust messages and events, by type path (rust-events). Values are in reflect-coverage's shape.\n");
    for d in plain {
        let table = d.reg.type_info().type_path_table();
        let name = unique(taken, table.short_path(), table.crate_name().unwrap_or(""));
        let _ = writeln!(js, "export const {name} = {};", d.factory());
        match d.reg.type_info() {
            TypeInfo::Struct(st) => {
                let _ = writeln!(dts, "export declare class {name} extends {} {{\n  static readonly path: {:?};\n  static readonly native: true;", d.base(), d.path);
                for f in st.iter() {
                    // An entity event's `entity` is its target, `On<E>.entity` at the first hop.
                    if d.event.is_some() && ON_OWN.contains(&f.name()) && !(d.event == Some(true) && f.name() == "entity") {
                        let _ = writeln!(dts, "  /** In an observer, read as `on.event.{0}`: `On<E>` has its own `{0}`. */", f.name());
                    }
                    let _ = writeln!(dts, "  declare readonly {}: {};", prop(f.name()), ts_type(registry, f.type_path(), 1));
                }
                dts.push_str("}\n");
            }
            _ => {
                let _ = writeln!(dts, "/** Its value is Bevy's enum, as `of(value)` takes it and a reader hands it. */");
                let _ = writeln!(dts, "export declare const {name}: RustClass<{name}, {name}Value> & {{ readonly path: {:?} }};", d.path);
                let _ = writeln!(dts, "export type {name} = RustEnum<{:?}, {name}Value>;", d.kind());
                let _ = writeln!(dts, "export type {name}Value = {};", ts_type(registry, &d.path, 0));
            }
        }
    }
    // Asset classes and markers by type path; a marker is made once, for an asset type with no
    // class (`Gltf`, `Shader`, `Font`), whichever generic names it first.
    let mut markers = assets;
    for (base, instances) in generics {
        let short = base.rsplit("::").next().unwrap_or(&base);
        let krate = base.split("::").next().unwrap_or("");
        let name = unique(taken, short, krate);
        let mut js_items = Vec::new();
        let mut dts_items = Vec::new();
        let mut args = Vec::new();
        let mut types = Vec::new();
        for d in instances {
            // The argument as TS spells it, an asset type by its marker.
            let short_path = d.reg.type_info().type_path_table().short_path();
            let mut arg = short_path.split_once('<').map_or("", |(_, a)| a).strip_suffix('>').unwrap_or("").to_string();
            let generic_args: Vec<_> = d.reg.type_info().generics().iter().collect();
            // `AssetEvent`'s argument is an asset whether or not its type registers `ReflectAsset`
            // (`Gltf`, `Shader` and `Font` do not).
            if let [g] = generic_args.as_slice()
                && (base == "bevy_asset::event::AssetEvent" || registry.get(g.type_id()).is_some_and(|r| r.data::<bevy::asset::ReflectAsset>().is_some()))
            {
                let t = g.type_path_table();
                arg = match markers.get(t.path()) {
                    Some(marker) => marker.clone(),
                    None => {
                        let marker = unique(taken, t.short_path(), t.crate_name().unwrap_or(""));
                        let _ = writeln!(js, "export const {marker} = rustAsset({:?});", t.path());
                        let _ = writeln!(dts, "/** The asset type `{}`, for a generic's argument (`AssetEvent<{marker}>`). */", t.path());
                        let _ = writeln!(dts, "export declare abstract class {marker} {{\n  static readonly path: {:?};\n  private readonly __asset: {:?};\n}}", t.path(), t.path());
                        markers.insert(t.path().to_string(), marker.clone());
                        marker
                    }
                };
            }
            js_items.push(format!("{arg:?}: {}", d.factory()));
            let ty = d.instance_ts(registry);
            dts_items.push(format!("  readonly {:?}: RustClass<{ty}, unknown> & {{ readonly path: {:?} }};", arg, d.path));
            args.push(arg);
            types.push(ty);
        }
        let _ = writeln!(js, "export const {name} = rustGeneric({name:?}, {{ {} }});", js_items.join(", "));
        let _ = writeln!(dts, "/** Generic: each registered instance by its type argument. */");
        let _ = writeln!(dts, "export declare const {name}: RustGeneric<{{\n{}\n}}>;", dts_items.join("\n"));
        // Each argument picks its instance's value; instances of one shape (`AssetEvent<A>`) share it.
        let shapes: BTreeSet<&String> = types.iter().collect();
        let value = if shapes.len() == 1 {
            types[0].clone()
        } else {
            args.iter().zip(&types).rev().fold("never".to_string(), |rest, (a, t)| format!("A extends {a} ? {t} : {rest}"))
        };
        let _ = writeln!(dts, "export type {name}<A extends {}> = {value};", args.join(" | "));
    }
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
         import type { AssetFields, AssetLoad, AssetRef, AssetValue, bool, ColorValue, Entity, f32, f64, Holes, i16, i32, i8, Quat, u16, u32, u8, Vec2, Vec3, Vec4 } from \"bevy-ts\";\n\n",
    );
    // The `T` of every `Handle<T>` a field names; the asset classes are `bevy-ts/bevy`'s.
    for (path, name, reg) in assets_named(&registry) {
        let _ = writeln!(s, "export type {name} = {};", asset_fields(&registry, &path, reg));
    }
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

    let classes = named(&registry, native, &claimed(world));
    if let Some(host) = world.get_resource::<bevy_ts::host::HostParams>() {
        let claims: Vec<Claim> = host.specs().filter_map(|p| Some(Claim { spelling: p.stands_for?, module: p.module, ts: p.ts })).collect();
        let modules = bevy_ts::modules::all();
        let declared = |t: &str| {
            if CLOCKS.contains(&t) || classes.iter().any(|(_, name, _)| name == t) {
                return Some(t.to_string());
            }
            modules.iter().find(|(_, m)| m.dts.contains(&format!("export declare class {t} "))).map(|(spec, _)| format!("import({spec:?}).{t}"))
        };
        s.push_str(&claims_ts(&claims, &declared));
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
                notes.push(format!("Holes, not columns: {}.", n.holes.join("; ")));
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
        if let Some(h) = layout.and_then(|n| hole_members(&registry, reg, n)) {
            let _ = writeln!(s, "export interface {name} extends Holes<{{ {h} }}> {{}}");
        }
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
        let out = claims_ts(&claims, &|t| (t == "Wind" || CLOCKS.contains(&t)).then(|| t.to_string()));
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
        assert!(out.contains("viewportToWorld(camera: Entity, point: "), "{out}");
        assert!(out.contains("export interface MeshRayCast {"), "{out}");
        assert!(out.contains("export interface Ray3d {"), "{out}");
        assert!(out.contains("export interface RayCastSettings {"), "{out}");
        assert!(out.contains("export interface RayMeshHit {"), "{out}");
        assert!(out.contains("export type RayCastVisibility = \"VisibleInView\" | \"Visible\" | \"Any\";"), "{out}");
        assert_eq!(out.matches("export interface Ray3d {").count(), 1, "declared once in the module: {out}");
    }

    /// `bevy-ts/input`'s own declarations import `Condition`, its params name `Entity`: one
    /// import of both, so `Gamepads` and `InputFocus` resolve `Entity` (alpha.23 lacked it).
    #[test]
    fn a_module_imports_what_its_declarations_and_params_name_once() {
        let mut app = App::new();
        app.add_plugins(crate::input::ScriptInputPlugin);
        let out = emit_modules(app.world());
        let block = out.split("declare module \"bevy-ts/input\" {").nth(1).unwrap().split("\n}\n").next().unwrap();
        assert!(block.contains("  import type { Condition, Entity } from \"bevy-ts\";"), "{block}");
        assert_eq!(block.matches("import type").count(), 1, "{block}");
        assert!(block.contains("pressed(pad: Entity, button: number): boolean;"), "{block}");
    }

    #[test]
    fn the_engine_declares_animations() {
        use bevy_ts_native::BevyTsNativeExt;
        let mut app = App::new();
        app.ts_param::<bevy_ts_animation::Animations<'static, 'static>>();
        let out = emit_modules(app.world());
        assert!(out.contains("declare module \"bevy-ts/animation\" {"), "{out}");
        assert!(out.contains("status(root: Entity)"), "{out}");
        assert!(out.contains("seekTime: number"), "{out}");
        assert!(out.contains("seek(root: Entity, secs: number): boolean"), "{out}");
    }

    #[test]
    fn the_engine_declares_diagnostics() {
        let mut app = App::new();
        app.add_plugins(crate::diagnostics::ScriptDiagnosticsPlugin);
        let out = emit_modules(app.world());
        assert!(out.contains("declare module \"bevy-ts/diagnostics\" {"), "{out}");
        assert!(out.contains("export class Diagnostic {"), "{out}");
        assert!(out.contains("value(path: string)"), "{out}");
        assert!(out.contains("addMeasurement(path: string, value: number)"), "{out}");
        assert!(out.contains("registerDiagnostic(d: import(\"bevy-ts/diagnostics\").Diagnostic): App;"), "{out}");
    }

    #[test]
    fn the_engine_declares_session() {
        let mut app = App::new();
        app.add_plugins(crate::session::ScriptSessionPlugin);
        let out = emit_modules(app.world());
        assert!(out.contains("declare module \"bevy-ts/session\" {"), "{out}");
        assert!(out.contains("put(key: string, value: Uint8Array)"), "{out}");
        assert!(out.contains("read(key: string, out: Uint8Array): number | undefined"), "{out}");
        assert!(out.contains("get(session: SessionBytes, key: string): Uint8Array | undefined"), "{out}");
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
