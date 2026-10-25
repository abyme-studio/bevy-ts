//! Reflected values from TSX and script commands (reflect-enums-color): enums in Bevy's shape at any depth,
//! a written value merged over the default, writable layout-tier holes as TSX props, and every
//! colour form through every caller. Headless; the rendered checks are in `reflect_render.rs`.

use crate::common;

use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
use bevy::light::DirectionalLight;
use bevy::pbr::ScreenSpaceTransmission;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_material::{ScriptMaterial, ScriptMaterial3d, ScriptMaterialPlugin};
use common::*;
use std::path::{Path, PathBuf};

fn reflect_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>().init_asset::<Shader>().init_asset::<Image>();
    app.add_plugins(ScriptMaterialPlugin);
    app.register_type::<Projection>().register_type::<DirectionalLight>().register_type::<ScreenSpaceTransmission>();
    app
}

fn scene(imports: &str, body: &str) -> String {
    format!("import {{ app, hex, hsl, linear, srgb }} from \"bevy-ts\";\nimport {{ {imports} }} from \"bevy-ts/bevy\";\n\napp.scene(\"s\", () => (\n  <entity name=\"it\">\n    {body}\n  </entity>\n));\n")
}

fn setup(name: &str, src: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = reflect_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    (dir, app)
}

fn errors(name: &str, src: &str) -> String {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = reflect_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed, "expected a failed load");
    r.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
}

fn save(app: &mut App, dir: &Path, src: &str) {
    write(dir, "game.tsx", src);
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
}

fn the<C: Component + Clone>(app: &mut App) -> C {
    let world = app.world_mut();
    world.query::<&C>().single(world).expect("one").clone()
}

fn ortho(p: &Projection) -> OrthographicProjection {
    match p {
        Projection::Orthographic(o) => o.clone(),
        other => panic!("not orthographic: {other:?}"),
    }
}

fn written(app: &App) -> usize {
    app.world().resource::<bevy_ts_tsx::TsxTree>().last.written
}

const ORTHO: &str = "<Projection value={{ Orthographic: { scale: 2, scaling_mode: { FixedVertical: { viewport_height: 12 } } } }} />";

#[test]
fn an_orthographic_projection_in_tsx_and_its_reloads() {
    let (dir, mut app) = setup("reflect-ortho", &scene("Projection", ORTHO));
    let p = the::<Projection>(&mut app);
    let o = ortho(&p);
    assert_eq!(o.scale, 2.0);
    assert!(matches!(o.scaling_mode, ScalingMode::FixedVertical { viewport_height: 12.0 }));
    let mut sized = p.clone();
    sized.update(800.0, 600.0);
    assert_eq!(sized.get_clip_from_view().row(3), Vec4::new(0.0, 0.0, 0.0, 1.0), "orthographic clip-from-view");

    // A unit variant, nested: only scaling_mode changes.
    save(&mut app, &dir, &scene("Projection", "<Projection value={{ Orthographic: { scale: 2, scaling_mode: \"WindowSize\" } }} />"));
    let o = ortho(&the::<Projection>(&mut app));
    assert!(matches!(o.scaling_mode, ScalingMode::WindowSize));
    assert_eq!(o.scale, 2.0);
    assert_eq!(written(&app), 1);

    // Keys left out take the default, never the previous value.
    save(&mut app, &dir, &scene("Projection", "<Projection value={{ Orthographic: {} }} />"));
    let o = ortho(&the::<Projection>(&mut app));
    assert_eq!(o.scale, OrthographicProjection::default_3d().scale);
}

#[test]
fn a_bad_variant_or_key_fails_the_load_naming_it() {
    let e = errors("reflect-variant", &scene("Projection", "<Projection value={{ Isometric: {} } as any} />"));
    assert!(e.contains("Projection") && e.contains("Perspective, Orthographic, Custom"), "{e}");
    let e = errors("reflect-typo", &scene("Projection", "<Projection value={{ Orthographic: { scael: 2 } } as any} />"));
    assert!(e.contains("scael") && e.contains("OrthographicProjection"), "{e}");
}

const SUN: &str = "<DirectionalLight illuminance={8000} color={hex(\"#ffd7a8\")} />";

#[test]
fn a_light_colour_is_a_prop_written_through_reflection() {
    let probe = "\nimport { Update } from \"bevy-ts\";\nimport type { Query } from \"bevy-ts\";\nexport function look(q: Query<[DirectionalLight]>) {\n  q.each((l) => console.log(`light ${l.illuminance} color=${(l as any).color}`));\n}\napp.addSystems(Update, look);\n";
    let (dir, mut app) = setup("reflect-sun", &format!("{}{probe}", scene("DirectionalLight", SUN)));
    let l = the::<DirectionalLight>(&mut app);
    assert_eq!(l.illuminance, 8000.0, "written from bytes");
    assert_eq!(l.color, Color::from(Srgba::hex("#ffd7a8").unwrap()), "written through reflection");
    updates(&mut app, 3);
    assert!(messages(&app).iter().any(|m| m == "light 8000 color=undefined"), "a TS row has no color: {:?}", messages(&app));

    save(&mut app, &dir, &format!("{}{probe}", scene("DirectionalLight", "<DirectionalLight illuminance={8000} color={srgb(1, 0.9, 0.8)} />")));
    let l = the::<DirectionalLight>(&mut app);
    assert_eq!(l.color, Color::srgb(1.0, 0.9, 0.8));
    assert_eq!(l.illuminance, 8000.0);
    assert_eq!(written(&app), 1, "only color is written");
}

/// The prebuilt engine has Bevy's soft shadows (`pcss`): a sun's `soft_shadow_size` is a TSX prop,
/// an `Option` written through reflection, `null` turning them off again.
#[test]
#[cfg(feature = "pcss")]
fn a_sun_takes_a_soft_shadow_size() {
    let (dir, mut app) = setup("reflect-soft-shadow", &scene("DirectionalLight", "<DirectionalLight shadow_maps_enabled soft_shadow_size={0.5} />"));
    assert_eq!(the::<DirectionalLight>(&mut app).soft_shadow_size, Some(0.5));
    save(&mut app, &dir, &scene("DirectionalLight", "<DirectionalLight shadow_maps_enabled soft_shadow_size={null} />"));
    assert_eq!(the::<DirectionalLight>(&mut app).soft_shadow_size, None);
}

#[test]
fn an_opaque_hole_loads_and_a_bound_one_fails_naming_it() {
    // An opaque hole with reflected serde loads (reflect-coverage): `steps` is a `usize`.
    let (_, mut app) = setup("reflect-opaque", &scene("ScreenSpaceTransmission", "<ScreenSpaceTransmission steps={4} />"));
    assert_eq!(the::<ScreenSpaceTransmission>(&mut app).steps, 4);
    let e = errors("reflect-bound", &scene("DirectionalLight", "<DirectionalLight color={() => \"#fff\"} />"));
    assert!(e.contains("color") && e.contains("not a binding"), "{e}");
    let e = errors("reflect-colour", &scene("DirectionalLight", "<DirectionalLight color={{ Srgba: { red: 1 } } as any} />"));
    assert!(e.contains("green and blue"), "{e}");
}

/// The linear RGBA of every `StandardMaterial` and `ScriptMaterial` colour in the world, by entity name.
fn colours(app: &mut App) -> Vec<(String, [f32; 4])> {
    use bevy::color::ColorToComponents;
    let world = app.world_mut();
    let mut out = Vec::new();
    let standard: Vec<(String, Handle<StandardMaterial>)> =
        world.query::<(&Name, &MeshMaterial3d<StandardMaterial>)>().iter(world).map(|(n, m)| (n.to_string(), m.0.clone())).collect();
    for (n, h) in standard {
        out.push((n, world.resource::<Assets<StandardMaterial>>().get(&h).unwrap().base_color.to_linear().to_f32_array()));
    }
    let script: Vec<(String, Handle<ScriptMaterial>)> = world.query::<(&Name, &ScriptMaterial3d)>().iter(world).map(|(n, m)| (n.to_string(), m.0.clone())).collect();
    for (n, h) in script {
        let d = &world.resource::<Assets<ScriptMaterial>>().get(&h).unwrap().data;
        let c: Vec<f32> = (0..4).map(|i| f32::from_le_bytes(d[4 * i..4 * i + 4].try_into().unwrap())).collect();
        out.push((n, [c[0], c[1], c[2], c[3]]));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_colour_form_through_every_caller_gives_one_colour() {
    let src = r##"import { app, hsl, linear, Startup, Transform } from "bevy-ts";
import type { Commands } from "bevy-ts";
import { color, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color }, fragment: glow });

export function cube(commands: Commands) {
  commands.spawn(Transform.of({}), <entity name="command"><mesh shape="cuboid" /><material color={hsl(30, 1, 0.5)} /></entity>);
}
app.addSystems(Startup, cube);

app.scene("s", () => (
  <>
    <entity name="hex"><mesh shape="cuboid" color="#ff8000" /></entity>
    <entity name="material"><mesh shape="cuboid" /><material color={linear(1, 0.2158605, 0)} /></entity>
    <entity name="mesh"><mesh shape="cuboid" color={[1, 0.5019608, 0]} /></entity>
    <entity name="script"><mesh shape="cuboid" /><material kind={Glow} color={{ Hsla: { hue: 30, saturation: 1, lightness: 0.5 } }} /></entity>
  </>
));
"##;
    let wgsl = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color;\n}\n";
    let dir = project("reflect-colours", &[("game.tsx", src), ("glow.wgsl", wgsl)]);
    let mut app = reflect_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let all = colours(&mut app);
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["command", "hex", "material", "mesh", "script"]);
    let want = all[0].1;
    for (n, c) in &all {
        assert!(c.iter().zip(want).all(|(a, b)| (a - b).abs() < 5e-3), "{n}: {c:?} != hex {want:?}");
    }
}

/// A component of one field per collection and scalar kind (reflect-coverage task 1.7).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
struct Kit {
    items: Vec<String>,
    counts: bevy::platform::collections::HashMap<String, u32>,
    by_entity: bevy::platform::collections::HashMap<Entity, f32>,
    tags: bevy::platform::collections::HashSet<u8>,
    cooldown: std::time::Duration,
    owner: Option<Entity>,
    big: u64,
    pair: (u8, bool),
    grid: [i32; 3],
    at: IVec2,
    spin: Quat,
    look: Option<Dir3>,
    mesh: Handle<Mesh>,
}

#[test]
fn every_kind_is_a_tsx_prop_and_a_list_is_replaced() {
    let src = |items: &str| {
        format!(
            "import {{ app, rustComponent }} from \"bevy-ts\";\nconst Kit = rustComponent(\"it::reflect_values::Kit\");\n\napp.scene(\"s\", () => (\n  <entity name=\"it\">\n    \
             <Kit items={{{items}}} counts={{{{ a: 1, b: 2 }}}} by_entity={{[[4294967300, 0.5]]}} tags={{[3, 1]}} cooldown={{1.5}} owner={{null}} \
             big={{\"1152921504606846976\"}} pair={{[7, true]}} grid={{[1, 2, 3]}} at={{[-3, 8]}} spin={{[0, 0, 0, 1]}} look={{[0, 1, 0]}} mesh={{null}} />\n  </entity>\n));\n"
        )
    };
    let dir = project("reflect-kit", &[("game.tsx", &src("[\"a\", \"b\"]"))]);
    let mut app = reflect_app(&dir.join("game.tsx"));
    app.register_type::<Kit>();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let kit = the::<Kit>(&mut app);
    assert_eq!(kit.items, vec!["a".to_string(), "b".to_string()]);
    assert_eq!((kit.counts["a"], kit.counts["b"]), (1, 2));
    assert_eq!(kit.by_entity.get(&Entity::from_bits(4294967300)), Some(&0.5));
    assert_eq!(kit.tags, [1, 3].into_iter().collect());
    assert_eq!(kit.cooldown, std::time::Duration::from_millis(1500));
    assert_eq!((kit.owner, kit.big, kit.pair, kit.grid, kit.at), (None, 1 << 60, (7, true), [1, 2, 3], IVec2::new(-3, 8)));
    assert_eq!((kit.spin, kit.look), (Quat::IDENTITY, Some(Dir3::Y)));
    save(&mut app, &dir, &src("[\"c\"]"));
    assert_eq!(the::<Kit>(&mut app).items, vec!["c".to_string()], "a list is replaced, not merged");
}

/// A string at a `Handle<T>` loads through the AssetServer, at any depth (an `Option` here), from
/// TSX and from `.of`. `register_asset_reflect` is what `ImagePlugin` does for a real app.
#[test]
fn an_asset_path_at_a_handle_loads_it_from_tsx_and_of() {
    use bevy::asset::AssetApp;
    use bevy::light::Skybox;
    let check = |app: &mut App| {
        let world = app.world_mut();
        let found: Vec<Skybox> = world.query::<&Skybox>().iter(world).cloned().collect();
        assert_eq!(found.len(), 1, "one Skybox: {:?}", messages(app));
        let sky = found.into_iter().next().unwrap();
        let image = sky.image.expect("an image handle, not None");
        assert_ne!(image, Handle::<Image>::default(), "a loaded handle, not the default");
        assert_eq!(image.path().map(|p| p.to_string()).as_deref(), Some("sky.ktx2"));
        assert_eq!(sky.brightness, 1000.0);
    };
    let tsx = scene("Skybox", "<Skybox image=\"sky.ktx2\" brightness={1000} />");
    let dir = project("reflect-skybox", &[("game.tsx", &tsx)]);
    let mut app = reflect_app(&dir.join("game.tsx"));
    app.register_asset_reflect::<Image>().register_type::<Skybox>();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    check(&mut app);

    let of = "import { app, Startup } from \"bevy-ts\";\nimport type { Commands } from \"bevy-ts\";\nimport { Skybox } from \"bevy-ts/bevy\";\nexport function setup(commands: Commands) {\n  commands.spawn(Skybox.of({ image: \"sky.ktx2\", brightness: 1000 }));\n}\napp.addSystems(Startup, setup);\n";
    let dir = project("reflect-skybox-of", &[("game.ts", of)]);
    let mut app = reflect_app(&dir.join("game.ts"));
    app.register_asset_reflect::<Image>().register_type::<Skybox>();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    check(&mut app);
}
