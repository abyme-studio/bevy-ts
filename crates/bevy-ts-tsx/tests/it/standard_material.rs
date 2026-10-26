//! `<material>` without `kind`: every `StandardMaterial` field, textures by asset path in their
//! colour space, and vertex tangents on a `<mesh>` whose material needs them.

use bevy::image::{CompressedImageFormats, ImageLoader, ImagePlugin};
use bevy::math::Affine2;
use bevy::mesh::UvChannel;
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::ParallaxMappingMethod;
use bevy::prelude::*;
use bevy::reflect::structs::Struct;
use bevy::render::render_resource::{Face, TextureFormat};
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadReport, ReloadStatus};
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts::manifest::{Manifest, Section};
use bevy_ts_tsx::intrinsics::IntrinsicAssets;
use bevy_ts_tsx::{TsxNode, TsxPlugin};
use serde_json::{Value, json};
use std::path::PathBuf;

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    let assets = format!("{}/tests/assets", env!("CARGO_MANIFEST_DIR"));
    app.add_plugins((MinimalPlugins, AssetPlugin { file_path: assets, ..default() }, ImagePlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin)
        // The render plugin registers it, with the GPU's compressed formats.
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()));
    (app, handle)
}

fn at(line: u32) -> Value {
    json!({ "file": "game.tsx", "line": line, "column": 3 })
}

/// An entity `key` with `<mesh shape>` (if any) and `<material {props}>` (if any).
fn entity(key: &str, mesh: Option<&str>, material: Option<Value>) -> Value {
    let mut components = vec![];
    if let Some(shape) = mesh {
        components.push(json!({ "type": "mesh", "kind": "intrinsic", "props": { "shape": shape }, "source": at(4) }));
    }
    if let Some(props) = material {
        components.push(json!({ "type": "material", "kind": "intrinsic", "props": props, "source": at(5) }));
    }
    json!({ "key": key, "type": "entity", "source": [at(3)], "components": components, "children": [] })
}

fn scene(nodes: Vec<Value>) -> Value {
    json!({ "roots": [{ "name": "s", "location": at(1), "nodes": nodes }] })
}

fn load(app: &mut App, fake: &FakeHandle, section: &Value) -> ReloadReport {
    fake.push(FakeProgram {
        manifest: Manifest::default(),
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(section).unwrap(), location: None }],
        ..Default::default()
    });
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false)
}

fn applied(r: &ReloadReport) {
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

fn by_key(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    let found = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path.last().map(String::as_str) == Some(key)).map(|(e, _)| e);
    found.unwrap_or_else(|| panic!("no entity {key}"))
}

fn material(app: &App, e: Entity) -> StandardMaterial {
    let h = &app.world().get::<MeshMaterial3d<StandardMaterial>>(e).expect("a standard material").0;
    app.world().resource::<Assets<StandardMaterial>>().get(h).unwrap().clone()
}

fn mesh_handle(app: &App, e: Entity) -> Handle<Mesh> {
    app.world().get::<Mesh3d>(e).expect("a mesh").0.clone()
}

fn has_tangents(app: &App, e: Entity) -> bool {
    app.world().resource::<Assets<Mesh>>().get(&mesh_handle(app, e)).unwrap().contains_attribute(Mesh::ATTRIBUTE_TANGENT)
}

fn errors(app: &App) -> Vec<(String, Option<u32>)> {
    let log = app.world().resource::<ScriptLog>();
    log.since(0).filter(|e| e.level == LogLevel::Error).map(|e| (e.message.clone(), e.location.as_ref().map(|l| l.line))).collect()
}

fn srgb(hex: &str) -> Color {
    Srgba::hex(hex).unwrap().into()
}

type Check = fn(&StandardMaterial) -> bool;

/// One case per prop that is not a texture: the prop, its value, and the field it sets. The
/// table in design.md, as code.
fn field_cases() -> Vec<(&'static str, Value, Check)> {
    vec![
        ("color", json!("#ff0000"), |m| m.base_color == srgb("#ff0000")),
        ("baseColorChannel", json!("uv1"), |m| m.base_color_channel == UvChannel::Uv1),
        ("emissive", json!("#00ff00"), |m| m.emissive == srgb("#00ff00").to_linear()),
        ("emissiveExposureWeight", json!(0.25), |m| m.emissive_exposure_weight == 0.25),
        ("emissiveChannel", json!("uv1"), |m| m.emissive_channel == UvChannel::Uv1),
        ("roughness", json!(0.125), |m| m.perceptual_roughness == 0.125),
        ("metallic", json!(0.75), |m| m.metallic == 0.75),
        ("metallicRoughnessChannel", json!("uv1"), |m| m.metallic_roughness_channel == UvChannel::Uv1),
        ("reflectance", json!(0.25), |m| m.reflectance == 0.25),
        ("specularTint", json!("#336699"), |m| m.specular_tint == srgb("#336699")),
        ("diffuseTransmission", json!(0.5), |m| m.diffuse_transmission == 0.5),
        ("specularTransmission", json!(0.9), |m| m.specular_transmission == 0.9),
        ("thickness", json!(0.2), |m| m.thickness == 0.2),
        ("ior", json!(1.33), |m| m.ior == 1.33),
        ("attenuationDistance", json!(3.0), |m| m.attenuation_distance == 3.0),
        ("attenuationColor", json!("#88ccff"), |m| m.attenuation_color == srgb("#88ccff")),
        ("normalMapChannel", json!("uv1"), |m| m.normal_map_channel == UvChannel::Uv1),
        ("flipNormalMapY", json!(true), |m| m.flip_normal_map_y),
        ("occlusionChannel", json!("uv1"), |m| m.occlusion_channel == UvChannel::Uv1),
        ("clearcoat", json!(1.0), |m| m.clearcoat == 1.0),
        ("clearcoatPerceptualRoughness", json!(0.3), |m| m.clearcoat_perceptual_roughness == 0.3),
        ("anisotropyStrength", json!(0.6), |m| m.anisotropy_strength == 0.6),
        ("anisotropyRotation", json!(1.5), |m| m.anisotropy_rotation == 1.5),
        ("doubleSided", json!(true), |m| m.double_sided),
        ("cullMode", Value::Null, |m| m.cull_mode.is_none()),
        ("unlit", json!(true), |m| m.unlit),
        ("fogEnabled", json!(false), |m| !m.fog_enabled),
        ("depthBias", json!(2.0), |m| m.depth_bias == 2.0),
        ("parallaxDepthScale", json!(0.05), |m| m.parallax_depth_scale == 0.05),
        ("parallaxMappingMethod", json!({ "relief": { "maxSteps": 8 } }), |m| m.parallax_mapping_method == ParallaxMappingMethod::Relief { max_steps: 8 }),
        ("maxParallaxLayerCount", json!(32.0), |m| m.max_parallax_layer_count == 32.0),
        ("lightmapExposure", json!(2.0), |m| m.lightmap_exposure == 2.0),
        ("opaqueRenderMethod", json!("deferred"), |m| m.opaque_render_method == OpaqueRendererMethod::Deferred),
        ("deferredLightingPassId", json!(7), |m| m.deferred_lighting_pass_id == 7),
        ("uvTransform", json!({ "scale": [0.5, 0.5], "translation": [0.5, 0] }), |m| {
            m.uv_transform == Affine2::from_scale_angle_translation(vec2(0.5, 0.5), 0.0, vec2(0.5, 0.0))
        }),
    ]
}

/// Every texture prop: its field, and whether it loads as sRGB colour (else linear data).
type Texture = fn(&StandardMaterial) -> Option<Handle<Image>>;

fn texture_cases() -> Vec<(&'static str, Texture, bool)> {
    vec![
        ("baseColorTexture", |m| m.base_color_texture.clone(), true),
        ("emissiveTexture", |m| m.emissive_texture.clone(), true),
        ("metallicRoughnessTexture", |m| m.metallic_roughness_texture.clone(), false),
        ("normalMapTexture", |m| m.normal_map_texture.clone(), false),
        ("occlusionTexture", |m| m.occlusion_texture.clone(), false),
        ("depthMap", |m| m.depth_map.clone(), false),
    ]
}

#[test]
fn every_prop_sets_its_field() {
    let (mut app, fake) = app();
    let cases = field_cases();
    let nodes = cases.iter().map(|(prop, v, _)| entity(prop, Some("cuboid"), Some(json!({ *prop: v })))).collect();
    applied(&load(&mut app, &fake, &scene(nodes)));
    for (prop, v, check) in cases {
        let e = by_key(&mut app, prop);
        let m = material(&app, e);
        assert!(check(&m), "{prop}={v} did not set its field");
    }
    // An unset prop leaves Bevy's default.
    let e = by_key(&mut app, "metallic");
    let m = material(&app, e);
    assert_eq!(m.cull_mode, Some(Face::Back));
    assert_eq!(m.base_color, StandardMaterial::default().base_color);
}

#[test]
fn a_cull_mode_of_front_culls_front_faces() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![entity("a", None, Some(json!({ "cullMode": "front", "doubleSided": true })))])));
    let e = by_key(&mut app, "a");
    let m = material(&app, e);
    assert_eq!((m.cull_mode, m.double_sided), (Some(Face::Front), true));
}

#[test]
fn every_texture_loads_its_path_in_its_colour_space() {
    for (prop, field, srgb) in texture_cases() {
        // One app each: a path keeps the colour space of its first load.
        let (mut app, fake) = app();
        applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(json!({ prop: "textures/checker.png" })))])));
        let e = by_key(&mut app, "a");
        let h = field(&material(&app, e)).unwrap_or_else(|| panic!("{prop} set no texture"));
        let server = app.world().resource::<AssetServer>().clone();
        assert_eq!(server.get_path(h.id()).unwrap().to_string(), "textures/checker.png", "{prop}");
        for _ in 0..200 {
            app.update();
            if app.world().resource::<Assets<Image>>().contains(h.id()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let image = app.world().resource::<Assets<Image>>().get(h.id()).unwrap_or_else(|| panic!("{prop}: never loaded: {:?}", server.load_state(h.id())));
        let want = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
        assert_eq!(image.texture_descriptor.format, want, "{prop}");
    }
}

#[test]
fn the_table_covers_standard_material() {
    let props: Vec<String> = field_cases().iter().map(|c| c.0.to_string()).chain(texture_cases().iter().map(|c| c.0.to_string())).collect();
    // Fields named otherwise, or owned elsewhere.
    let named = |f: &str| match f {
        "base_color" => Some("color".to_string()),
        "perceptual_roughness" => Some("roughness".to_string()),
        "alpha_mode" => None,
        f => Some(f.split('_').enumerate().map(|(i, w)| if i == 0 { w.to_string() } else { w[..1].to_uppercase() + &w[1..] }).collect()),
    };
    let m = StandardMaterial::default();
    let mut missing = vec![];
    for i in 0..m.field_len() {
        let f = m.name_at(i).unwrap();
        if let Some(p) = named(f)
            && !props.contains(&p)
        {
            missing.push(format!("{f} ({p})"));
        }
    }
    assert!(missing.is_empty(), "StandardMaterial fields with no <material> prop: {missing:?}");
    // Bevy does not reflect `cull_mode`; `cullMode` sets it.
    assert!(props.contains(&"cullMode".to_string()));
    assert_eq!(props.len(), m.field_len() - 1 + 1, "one prop per reflected field but alpha_mode, and cullMode");
}

fn load_error(props: Value) -> String {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(props))]));
    assert_eq!(r.status, ReloadStatus::Failed, "loaded");
    assert_eq!(r.errors[0].location.as_ref().map(|l| l.line), Some(5), "at the <material>: {:?}", r.errors);
    r.errors[0].message.clone()
}

#[test]
fn bad_values_fail_at_the_element() {
    let m = load_error(json!({ "cullMode": "both" }));
    assert!(m.contains("\"back\"") && m.contains("\"front\"") && m.contains("null"), "{m}");
    // JSON has no NaN; a non-finite number reaches `check` from Rust callers.
    let p: bevy_ts_tsx::intrinsics::MaterialProps = serde_json::from_value(json!({ "ior": 1.5 })).unwrap();
    let nan = bevy_ts_tsx::intrinsics::MaterialProps { ior: Some(f32::NAN), ..p };
    assert!(nan.check().unwrap_err().contains("ior"));
    let m = load_error(json!({ "uvTransform": { "scale": [1] } }));
    assert!(m.contains("uvTransform") || m.contains("array"), "{m}");
    let m = load_error(json!({ "uvTransform": { "skew": 1 } }));
    assert!(m.contains("skew"), "{m}");
    let m = load_error(json!({ "baseColorTexture": "" }));
    assert!(m.contains("baseColorTexture is an empty path"), "{m}");
    let m = load_error(json!({ "baseColorChannel": "uv2" }));
    assert!(m.contains("uv2"), "{m}");
    let m = load_error(json!({ "parallaxMappingMethod": { "relief": { "steps": 3 } } }));
    assert!(m.contains("steps"), "{m}");
    let m = load_error(json!({ "deferredLightingPassId": 300 }));
    assert!(m.contains("300"), "{m}");
    let m = load_error(json!({ "specularTint": "teal-ish" }));
    assert!(m.contains("teal-ish"), "{m}");
}

#[test]
fn a_texture_with_no_asset_server_fails_at_the_element() {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    let r = load(&mut app, &handle, &scene(vec![entity("a", Some("cuboid"), Some(json!({ "normalMapTexture": "t/n.png" })))]));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("normalMapTexture needs an AssetServer"), "{:?}", r.errors);
    assert_eq!(r.errors[0].location.as_ref().map(|l| l.line), Some(5));
}

#[test]
fn a_mounted_texture_with_no_asset_server_fails_the_mount() {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    let op = json!({ "nodes": [entity("a", Some("cuboid"), Some(json!({ "baseColorTexture": "t/crate.png" })))] });
    let e = bevy_ts_tsx::mount::mount(app.world_mut(), &serde_json::to_vec(&op).unwrap()).unwrap_err();
    assert!(e.contains("baseColorTexture needs an AssetServer"), "{e}");
    assert!(app.world_mut().query::<&MeshMaterial3d<StandardMaterial>>().iter(app.world()).next().is_none(), "nothing mounted");
}

#[test]
fn equal_props_share_one_handle() {
    let (mut app, fake) = app();
    let props = json!({ "color": "#fff", "baseColorTexture": "textures/checker.png", "doubleSided": true });
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(props.clone())), entity("b", Some("cuboid"), Some(props))])));
    let (a, b) = (by_key(&mut app, "a"), by_key(&mut app, "b"));
    let get = |app: &App, e| app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.clone();
    assert_eq!(get(&app, a), get(&app, b));
    assert_eq!(app.world().resource::<IntrinsicAssets>().built, 2, "one mesh, one material");
}

#[test]
fn a_missing_texture_file_reports_its_path() {
    let (mut app, fake) = app();
    let load_value = json!({ "normalMapTexture": { "$load": "textures/nope_n.png" } });
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(json!({ "baseColorTexture": "textures/nope.png" }))), entity("b", Some("cuboid"), Some(load_value))])));
    for _ in 0..200 {
        app.update();
        if errors(&app).len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let errs = errors(&app);
    assert!(errs.iter().any(|(m, line)| m.contains("textures/nope.png") && *line == Some(5)), "{errs:?}");
    assert!(errs.iter().any(|(m, line)| m.contains("textures/nope_n.png") && line.is_some()), "an Image.load value's path is reported too: {errs:?}");
}

const NORMAL: &str = "textures/checker.png";

#[test]
fn a_plain_mesh_has_no_tangents_and_a_normal_mapped_one_has_them() {
    let (mut app, fake) = app();
    applied(&load(
        &mut app,
        &fake,
        &scene(vec![
            entity("plain", Some("cuboid"), Some(json!({ "color": "#f00" }))),
            entity("bumpy", Some("cuboid"), Some(json!({ "normalMapTexture": NORMAL }))),
            entity("brushed", Some("plane"), Some(json!({ "anisotropyStrength": 0.5 }))),
            entity("still", Some("plane"), Some(json!({ "anisotropyStrength": 0 }))),
        ]),
    ));
    for (key, want) in [("plain", false), ("bumpy", true), ("brushed", true), ("still", false)] {
        let e = by_key(&mut app, key);
        assert_eq!(has_tangents(&app, e), want, "{key}");
    }
}

#[test]
fn two_normal_mapped_cuboids_share_one_tangent_mesh_beside_a_plain_one() {
    let (mut app, fake) = app();
    let bumpy = json!({ "normalMapTexture": NORMAL });
    applied(&load(
        &mut app,
        &fake,
        &scene(vec![
            entity("a", Some("cuboid"), Some(bumpy.clone())),
            entity("b", Some("cuboid"), Some(bumpy)),
            entity("c", Some("cuboid"), Some(json!({ "color": "#fff" }))),
        ]),
    ));
    let (a, b, c) = (by_key(&mut app, "a"), by_key(&mut app, "b"), by_key(&mut app, "c"));
    assert_eq!(mesh_handle(&app, a), mesh_handle(&app, b));
    assert_ne!(mesh_handle(&app, a), mesh_handle(&app, c));
    // Two cuboid meshes (plain, tangents) and two materials.
    assert_eq!(app.world().resource::<IntrinsicAssets>().built, 4);
}

#[test]
fn removing_the_normal_map_restores_the_plain_mesh() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(json!({ "normalMapTexture": NORMAL })))])));
    let e = by_key(&mut app, "a");
    assert!(has_tangents(&app, e));
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(json!({ "color": "#fff" })))])));
    assert!(!has_tangents(&app, e));
    // A material removed altogether leaves the plain mesh too.
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), Some(json!({ "normalMapTexture": NORMAL })))])));
    assert!(has_tangents(&app, e));
    applied(&load(&mut app, &fake, &scene(vec![entity("a", Some("cuboid"), None)])));
    assert!(!has_tangents(&app, e));
}

#[test]
fn a_mesh_inserted_after_its_material_and_a_material_after_its_mesh_both_fit() {
    let (mut app, fake) = app();
    let bumpy = json!({ "normalMapTexture": NORMAL });
    applied(&load(&mut app, &fake, &scene(vec![entity("late-mesh", None, Some(bumpy.clone())), entity("late-material", Some("cuboid"), None)])));
    applied(&load(&mut app, &fake, &scene(vec![entity("late-mesh", Some("cuboid"), Some(bumpy.clone())), entity("late-material", Some("cuboid"), Some(bumpy))])));
    for key in ["late-mesh", "late-material"] {
        let e = by_key(&mut app, key);
        assert!(has_tangents(&app, e), "{key}");
    }
}
