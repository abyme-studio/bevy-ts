//! Script 2D materials drawn (change `script-material2d`): a `Mesh2d` under a `Camera2d`, drawn by
//! Bevy's 2D material pipeline with a kind's WGSL, into an offscreen image.

use crate::alpha_render::{GPU, app_with, until_drawn};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_ts_material::layout::{Base, Bindings, ParamType, Space};
use bevy_ts_material::{Kind, MeshScriptMaterial2d, ScriptMaterial2d};
use serde_json::json;
use std::sync::Arc;

const W: u32 = 320;
const H: u32 = 180;

const HEAD: &str = "import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;\n\n";

/// A 2D kind of `params` and `instance` params, drawing `body` (after the `VertexOutput` import).
fn kind(world: &mut World, name: &str, params: &[(&str, ParamType)], instance: &[(&str, ParamType)], body: &str, alpha: AlphaMode) -> Arc<Kind> {
    let bindings = Bindings::new(name, params.iter().map(|(n, t)| (*n, t)), &[])
        .unwrap()
        .with_instance(instance.iter().map(|(n, t)| (*n, t)))
        .unwrap()
        .in_space(Space::D2);
    let src = bevy_ts_material::wgsl::combine(&format!("{HEAD}{body}"), &bindings, name);
    let path = format!("bevy_ts_material/m2d/{}.wesl", name.replace("::", "_").to_lowercase());
    let shader = world.resource_mut::<Assets<Shader>>().add(Shader::from_wesl(src, path));
    Arc::new(Kind::new(name, bindings, Some(shader), None, alpha))
}

fn material(kind: &Arc<Kind>, values: serde_json::Value) -> ScriptMaterial2d {
    let mut data = kind.bindings.params().defaults();
    for (k, v) in values.as_object().unwrap() {
        kind.bindings.params().encode(k, v, &mut data).unwrap();
    }
    ScriptMaterial2d { kind: kind.clone(), data, textures: vec![None; kind.bindings.textures().len()] }
}

/// A `Camera2d` drawing into a new image over white.
fn camera(world: &mut World) -> (Handle<Image>, Entity) {
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cam = world
        .spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() }, Tonemapping::None, Msaa::Off, RenderTarget::Image(target.clone().into())))
        .id();
    (target, cam)
}

fn px(img: &Image, x: u32, y: u32) -> Srgba {
    img.get_color_at(x, y).unwrap().to_srgba()
}

fn is(c: Srgba, want: Srgba) -> bool {
    (c.red - want.red).abs() < 0.02 && (c.green - want.green).abs() < 0.02 && (c.blue - want.blue).abs() < 0.02
}

/// How many pixels of `(x0..x1, y0..y1)` are `want`.
fn count(img: &Image, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>, want: Srgba) -> usize {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y))).filter(|(x, y)| is(px(img, *x, *y), want)).count()
}

const COLOR: &str = "@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> { return params.color; }\n";

#[test]
fn a_red_kind_draws_a_red_rectangle() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p);
    let world = app.world_mut();
    let (target, _) = camera(world);
    let red = kind(world, "m2d::Red", &[("color", ParamType::Base(Base::Color))], &[], COLOR, AlphaMode::Opaque);
    let m = world.resource_mut::<Assets<ScriptMaterial2d>>().add(material(&red, json!({ "color": "#ff0000" })));
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(64.0, 64.0));
    world.spawn((Mesh2d(quad), MeshScriptMaterial2d(m), Transform::default()));
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let (n, c) = (count(img, W / 2 - 32..W / 2 + 32, H / 2 - 32..H / 2 + 32, Srgba::RED), px(img, 10, 10));
        if n == 64 * 64 && is(c, Srgba::WHITE) { Ok(()) } else { Err(format!("{n} red pixels of {}, corner {c:?}", 64 * 64)) }
    });
    // A new shader at the kind's handle redraws, with no new kind or asset.
    let handle = red.fragment.clone().unwrap();
    let src = bevy_ts_material::wgsl::combine(
        &format!("{HEAD}@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {{ return vec4<f32>(0.0, 1.0, 0.0, 1.0); }}\n"),
        &red.bindings,
        "m2d::Red",
    );
    app.world_mut().resource_mut::<Assets<Shader>>().insert(handle.id(), Shader::from_wesl(src, "bevy_ts_material/m2d/m2d_red.wesl")).unwrap();
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let c = px(img, W / 2, H / 2);
        if is(c, Srgba::GREEN) { Ok(()) } else { Err(format!("the centre is {c:?}, not green")) }
    });
}

#[test]
fn an_instance_param_reads_per_quad() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p);
    let world = app.world_mut();
    let (target, _) = camera(world);
    let body = "@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return vec4<f32>(params.sea.rgb * instance(in.instance_index).seed / 10.0, 1.0);\n}\n";
    let k = kind(world, "m2d::Seed", &[("sea", ParamType::Base(Base::Color))], &[("seed", ParamType::Base(Base::F32))], body, AlphaMode::Opaque);
    let m = world.resource_mut::<Assets<ScriptMaterial2d>>().add(material(&k, json!({ "sea": "#ffffff" })));
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(40.0, 40.0));
    for (x, seed) in [(-60.0, 5), (60.0, 10)] {
        let e = world.spawn((Mesh2d(quad.clone()), MeshScriptMaterial2d(m.clone()), Transform::from_xyz(x, 0.0, 0.0))).id();
        bevy_ts_material::instance::set(world, e, "m2d::Seed", &k.bindings, json!({ "seed": seed }).as_object().unwrap(), None);
    }
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let lin = |x: u32| img.get_color_at(x, H / 2).unwrap().to_linear().red;
        let (a, b) = (lin(W / 2 - 60), lin(W / 2 + 60));
        if (b - 1.0).abs() < 0.02 && (a - 0.5).abs() < 0.02 { Ok(()) } else { Err(format!("linear red {a} and {b}, not 0.5 and 1")) }
    });
}

/// A checker of `cell`'s whole parts, red and blue.
fn checker(cell: &str) -> String {
    format!("@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {{\n    let c = {cell};\n    let odd = (i32(c.x) + i32(c.y)) & 1;\n    return select(vec4<f32>(1.0, 0.0, 0.0, 1.0), vec4<f32>(0.0, 0.0, 1.0, 1.0), odd == 1);\n}}\n")
}

/// The runs of one colour along a line of `pixels`, red or blue, past white: their lengths.
fn runs(pixels: impl Iterator<Item = Srgba>) -> Vec<u32> {
    let mut out: Vec<(Srgba, u32)> = Vec::new();
    for p in pixels.filter(|p| !is(*p, Srgba::WHITE)) {
        match out.last_mut() {
            Some((c, n)) if is(*c, p) => *n += 1,
            _ => out.push((p, 1)),
        }
    }
    out.into_iter().map(|(_, n)| n).collect()
}

/// Under `PixelCamera` zoom 3, a 16×16 quad at `x` with a checker of `cell`: each run of a row and
/// a column through the quad's middle.
fn art_runs(cell: &str, name: &str, x: f32) -> (Vec<u32>, Vec<u32>) {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p.add(bevy_ts_material::Pixel2dPlugin));
    let world = app.world_mut();
    let (target, cam) = camera(world);
    world.entity_mut(cam).insert(bevy_ts_material::PixelCamera { zoom: 3.0, ease: 0.0 });
    let k = kind(world, name, &[("c", ParamType::Base(Base::F32))], &[], &checker(cell), AlphaMode::Opaque);
    let m = world.resource_mut::<Assets<ScriptMaterial2d>>().add(material(&k, json!({})));
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(16.0, 16.0));
    world.spawn((Mesh2d(quad), MeshScriptMaterial2d(m), Transform::from_xyz(x, 0.0, 0.0)));
    let mut out = None;
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let row = runs((0..W).map(|x| px(img, x, H / 2)));
        let first = (0..W).find(|x| !is(px(img, *x, H / 2), Srgba::WHITE)).ok_or("nothing drawn")?;
        let col = runs((0..H).map(|y| px(img, first + 4, y)));
        if row.iter().sum::<u32>() < 40 {
            return Err(format!("{row:?}: not drawn yet"));
        }
        out = Some((row, col));
        Ok(())
    });
    out.unwrap()
}

#[test]
fn world_space_art_pixels_are_whole_blocks_at_a_fractional_position() {
    // 16 units from x 2.3: the edge units are cut, every unit inside is 3 pixels.
    let (row, col) = art_runs("floor(in.world_position.xy)", "m2d::World", 10.3);
    for line in [&row, &col] {
        let inner = &line[1..line.len() - 1];
        assert!(!inner.is_empty() && inner.iter().all(|n| *n == 3), "{line:?}");
    }
}

#[test]
fn quad_space_art_pixels_are_whole_blocks_on_whole_units() {
    let (row, col) = art_runs("floor(in.uv * 16.0)", "m2d::Quad", 10.0);
    assert_eq!(row, vec![3; 16], "48 pixels in whole blocks");
    assert_eq!(col, vec![3; 16], "48 pixels in whole blocks");
}

#[test]
fn the_lower_of_two_y_sorted_quads_draws_in_front() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p.add(bevy_ts_material::Pixel2dPlugin));
    let world = app.world_mut();
    let (target, _) = camera(world);
    let k = kind(world, "m2d::Sorted", &[("color", ParamType::Base(Base::Color))], &[], COLOR, AlphaMode::Opaque);
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(40.0, 40.0));
    let mut spawn = |c: &str, y: f32| {
        let m = world.resource_mut::<Assets<ScriptMaterial2d>>().add(material(&k, json!({ "color": c })));
        world.spawn((Mesh2d(quad.clone()), MeshScriptMaterial2d(m), bevy_ts_material::YSort { layer: 1.0, offset: 0.0 }, Transform::from_xyz(0.0, y, 0.0))).id()
    };
    // Spawned blue last, so without y-sort the tie at z 0 would not decide for red.
    let _red = spawn("#ff0000", -10.0);
    let blue = spawn("#0000ff", 0.0);
    // Inside both quads, whichever is in front.
    let overlap = |img: &Image| px(img, W / 2, H / 2 - 5);
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let c = overlap(img);
        if is(c, Srgba::RED) { Ok(()) } else { Err(format!("the overlap is {c:?}, not red")) }
    });
    app.world_mut().get_mut::<Transform>(blue).unwrap().translation.y = -20.0;
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let c = overlap(img);
        if is(c, Srgba::BLUE) { Ok(()) } else { Err(format!("the overlap is {c:?}, not blue")) }
    });
}

#[test]
fn a_replaced_shader_draws_within_frames_and_a_bad_one_keeps_the_last() {
    use bevy_ts::log::{LogLevel, ScriptLog};
    use bevy_ts_material::{MaterialKinds, kinds};
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p);
    app.init_resource::<ScriptLog>().init_resource::<MaterialKinds>();
    let file = |f: &str| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/materials2d").join(f);
    let load = |app: &mut App, generation: u64, fragment: &str| {
        let item = json!({
            "name": "game::K", "params": [], "textures": [], "instance": [], "fragment": file(fragment), "alphaMode": "opaque", "space": "2d",
            "location": { "file": "game.tsx", "line": 3, "column": 14 },
        });
        kinds::check(app.world_mut(), generation, &serde_json::to_vec(&json!({ "items": [item] })).unwrap(), None).unwrap();
        kinds::apply(app.world_mut(), generation, true);
    };
    let (target, _) = camera(app.world_mut());
    load(&mut app, 1, "red.wgsl");
    let world = app.world_mut();
    let e = world.spawn_empty().id();
    let m = bevy_ts_material::instance::apply2d(world, e, "game::K", &serde_json::Map::new(), None).unwrap();
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(64.0, 64.0));
    world.entity_mut(e).insert((Mesh2d(quad), MeshScriptMaterial2d(m), Transform::default()));
    let centre = |app: &mut App, want: Srgba, what: &str| {
        until_drawn(app, &target, |_| Ok(()), |_, img| {
            let c = px(img, W / 2, H / 2);
            if is(c, want) { Ok(()) } else { Err(format!("{what}: the centre is {c:?}")) }
        });
    };
    centre(&mut app, Srgba::RED, "red.wgsl");
    load(&mut app, 2, "blue.wgsl");
    centre(&mut app, Srgba::BLUE, "blue.wgsl in red's place");
    load(&mut app, 3, "bad.wgsl");
    let log = app.world().resource::<ScriptLog>();
    let err = log.since(0).filter(|e| e.level == LogLevel::Error).last().expect("an error");
    let at = err.location.clone().expect("a location");
    assert!(err.message.contains("material game::K") && err.message.contains("the previous material keeps drawing"), "{}", err.message);
    assert!(at.file.ends_with("bad.wgsl") && at.line == 6, "{at:?}: {}", err.message);
    assert!(app.world().resource::<MaterialKinds>().live["game::K"].wanted.is_some());
    centre(&mut app, Srgba::BLUE, "blue.wgsl kept");
}
