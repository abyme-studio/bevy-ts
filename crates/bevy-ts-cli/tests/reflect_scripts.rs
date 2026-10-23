//! Scripts write every reflect kind (reflect-coverage stage 2): reflect-tier values and layout
//! values with holes through `commands.spawn`, `Transform.lookingAt` against Bevy's `looking_at`,
//! and the removed built-in recipes naming their replacements. Headless.

mod common;

use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
use bevy::light::DirectionalLight;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use common::*;
use std::path::Path;

fn scripts_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>().init_asset::<Image>();
    app.register_type::<Projection>().register_type::<DirectionalLight>().register_type::<Camera3d>().register_type::<Camera>();
    app
}

/// A project whose `Startup` system runs `body` with `commands`.
fn run(name: &str, body: &str) -> App {
    let src = format!(
        "import {{ app, hex, srgb, Startup, Transform }} from \"bevy-ts\";\nimport type {{ Commands }} from \"bevy-ts\";\nimport {{ Camera3d, DirectionalLight, Projection }} from \"bevy-ts/bevy\";\nvoid hex; void srgb; void Camera3d; void DirectionalLight; void Projection;\nexport function setup(commands: Commands) {{\n  {body}\n}}\napp.addSystems(Startup, setup);\n"
    );
    let dir = project(name, &[("game.ts", &src)]);
    let mut app = scripts_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    app
}

fn single<C: Component + Clone>(app: &mut App) -> (Entity, C) {
    let world = app.world_mut();
    let (e, c) = world.query::<(Entity, &C)>().single(world).expect("one");
    (e, c.clone())
}

#[test]
fn an_orthographic_camera_from_ts() {
    let mut app = run(
        "scripts-ortho",
        "commands.spawn(Camera3d, Transform.lookingAt([10, 10, 10], [0, 0, 0]), Projection.of({ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }));",
    );
    let (e, p) = single::<Projection>(&mut app);
    let Projection::Orthographic(o) = p else { panic!("not orthographic: {p:?}") };
    assert!(matches!(o.scaling_mode, ScalingMode::FixedVertical { viewport_height: 12.0 }));
    let d = OrthographicProjection::default_3d();
    assert_eq!((o.scale, o.near, o.far), (d.scale, d.near, d.far), "every other field at its default");
    assert!(app.world().get::<Camera3d>(e).is_some());
    let t = *app.world().get::<Transform>(e).unwrap();
    let want = Transform::from_xyz(10.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    assert!(t.translation.abs_diff_eq(want.translation, 1e-6) && t.rotation.abs_diff_eq(want.rotation, 1e-6), "{t:?} != {want:?}");
}

#[test]
fn a_warm_sun_from_ts() {
    let mut app = run("scripts-sun", "commands.spawn(DirectionalLight.of({ illuminance: 8000, color: srgb(1, 0.85, 0.7) }));");
    let (_, light) = single::<DirectionalLight>(&mut app);
    assert_eq!(light.illuminance, 8000.0);
    assert_eq!(light.color, Color::srgb(1.0, 0.85, 0.7));
    assert_eq!(light.shadow_maps_enabled, DirectionalLight::default().shadow_maps_enabled, "fields left out keep the default");
}

#[test]
fn looking_at_matches_bevy() {
    for (eye, target, up) in [([-3.0, 3.0, 6.0], [0.0, 0.0, 0.0], None), ([0.0, 5.0, 0.0], [0.0, 0.0, 0.0], Some([0.0, 0.0, -1.0])), ([1.0, -2.0, 0.5], [4.0, 1.0, -3.0], Some([0.0, 1.0, 0.0]))] {
        let up_js = up.map_or(String::new(), |u: [f32; 3]| format!(", [{}, {}, {}]", u[0], u[1], u[2]));
        let mut app = run(
            "scripts-look",
            &format!("commands.spawn(Transform.lookingAt([{}, {}, {}], [{}, {}, {}]{up_js}));", eye[0], eye[1], eye[2], target[0], target[1], target[2]),
        );
        let world = app.world_mut();
        let t = *world.query_filtered::<&Transform, With<bevy_ts::commands::TsSpawned>>().single(world).unwrap();
        let up = Vec3::from(up.unwrap_or([0.0, 1.0, 0.0]));
        let want = Transform::from_translation(Vec3::from(eye)).looking_at(Vec3::from(target), up);
        for (a, b) in t.rotation.to_array().iter().zip(want.rotation.to_array()) {
            assert!((a - b).abs() <= 1e-6, "{eye:?} → {target:?}: {:?} != {:?}", t.rotation, want.rotation);
        }
    }
}

#[test]
fn a_bad_value_fails_its_command_naming_type_and_field() {
    let app = run("scripts-bad", "commands.spawn(DirectionalLight.of({ colour: \"#fff\" } as any));\n  commands.spawn(Projection.of({ Isometric: {} } as any));\n  commands.spawn(Transform.of({}));");
    let m = messages(&app);
    assert!(m.iter().any(|m| m.contains("DirectionalLight") && m.contains("colour")), "{m:?}");
    assert!(m.iter().any(|m| m.contains("Projection") && m.contains("Perspective, Orthographic, Custom")), "{m:?}");
}

#[test]
fn a_removed_recipe_names_its_replacement() {
    let app = run("scripts-removed", "commands.spawnNative(\"camera3d\", {});");
    let m = messages(&app);
    assert!(m.iter().any(|m| m.contains("removed") && m.contains("commands.spawn(Camera3d, Transform.lookingAt(")), "{m:?}");
}
