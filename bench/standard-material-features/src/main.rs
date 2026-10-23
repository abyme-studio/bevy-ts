//! The arena's scene (sun with shadows, camera, player, two enemies and their eyes, two rooms'
//! guards) plus 100 distinct textured, normal-mapped `StandardMaterial`s on cubes, drawn headless
//! offscreen. Prints the mean and p95 frame time, unsynced, and the mean frame time with each
//! frame waited out on the GPU (Bevy records no GPU timestamps on Metal, so that is the GPU's
//! share). Built with `--features textures` (the four PBR texture features) or without.

use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::renderer::RenderDevice;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use std::time::{Duration, Instant};

const SIZE: (u32, u32) = (3840, 2160);
const WARMUP: usize = 240;
const FRAMES: usize = 600;

fn main() {
    let arm = if cfg!(feature = "textures") { "on" } else { "off" };
    if std::env::args().nth(1).as_deref() == Some("limits") {
        return limits(arm);
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    app.finish();
    app.cleanup();
    scene(app.world_mut());

    for _ in 0..WARMUP {
        app.update();
    }
    let mut free = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        free.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let device = app.world().resource::<RenderDevice>().clone();
    let mut synced = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        device.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely()).unwrap();
        synced.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    free.sort_by(f64::total_cmp);
    let p95 = free[free.len() * 95 / 100];
    println!("arm={arm} frames={FRAMES} size={}x{} frame_mean_ms={:.3} frame_p95_ms={p95:.3} gpu_synced_mean_ms={:.3}", SIZE.0, SIZE.1, mean(&free), mean(&synced));
}

fn image(images: &mut Assets<Image>, rgba: [u8; 4], srgb: bool) -> Handle<Image> {
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    let size = Extent3d { width: 256, height: 256, depth_or_array_layers: 1 };
    let mut data = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let check = ((x / 32) + (y / 32)) % 2 == 0;
            data.extend(if check { rgba } else { [rgba[0] / 2, rgba[1] / 2, rgba[2], rgba[3]] });
        }
    }
    images.add(Image::new(size, TextureDimension::D2, data, format, RenderAssetUsages::default()))
}

fn scene(world: &mut World) {
    let mut target = Image::new_target_texture(SIZE.0, SIZE.1, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = RenderAssetUsages::default();
    let mut images = world.resource_mut::<Assets<Image>>();
    let target = images.add(target);
    let albedo = image(&mut images, [220, 180, 120, 255], true);
    let normal = image(&mut images, [160, 128, 230, 255], false);

    let mut meshes = world.resource_mut::<Assets<Mesh>>();
    let cube = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)).with_generated_tangents().unwrap());
    let sphere = meshes.add(Sphere::new(0.5).mesh().build());
    let eye = meshes.add(Sphere::new(0.15).mesh().build());

    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let red = materials.add(StandardMaterial { base_color: Srgba::hex("#cc3333").unwrap().into(), ..default() });
    let white = materials.add(StandardMaterial::default());
    let blue = materials.add(StandardMaterial { base_color: Srgba::hex("#3366cc").unwrap().into(), ..default() });
    let textured: Vec<_> = (0..100)
        .map(|i| {
            materials.add(StandardMaterial {
                base_color: Color::hsl(i as f32 * 3.6, 0.6, 0.6),
                base_color_texture: Some(albedo.clone()),
                normal_map_texture: Some(normal.clone()),
                perceptual_roughness: 0.3 + (i % 10) as f32 * 0.05,
                ..default()
            })
        })
        .collect();

    world.spawn((Camera3d::default(), RenderTarget::Image(target.into()), Transform::from_xyz(-6.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((Mesh3d(sphere), MeshMaterial3d(blue), Transform::default()));
    for at in [vec3(-3.0, 0.0, -4.0), vec3(3.0, 0.0, -4.0), vec3(-12.0, 0.0, -12.0), vec3(12.0, 0.0, -12.0)] {
        world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(red.clone()), Transform::from_translation(at))).with_children(|c| {
            c.spawn((Mesh3d(eye.clone()), MeshMaterial3d(white.clone()), Transform::from_xyz(0.0, 0.6, 0.5)));
        });
    }
    for (i, m) in textured.into_iter().enumerate() {
        let (x, z) = ((i % 10) as f32 - 4.5, (i / 10) as f32 - 4.5);
        world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(m), Transform::from_xyz(x * 1.2, -1.0, z * 1.2).with_scale(Vec3::splat(0.9))));
    }
}

/// `limits`: every texture field this build has, on one cube, on a device held to
/// `wgpu::Limits::default()`. Prints each render pipeline's sampled textures and samplers per
/// stage, across its bind groups, and any pipeline wgpu rejects.
fn limits(arm: &str) {
    use bevy::render::render_resource::{BindGroupLayoutDescriptor, BindingType, CachedPipelineState, PipelineCache, PipelineDescriptor, ShaderStages};
    use bevy::render::settings::{RenderCreation, WgpuLimits, WgpuSettings};
    use bevy::render::{Render, RenderApp, RenderPlugin, RenderSystems};
    use std::sync::{Arc, Mutex};

    #[derive(Resource, Clone, Default)]
    struct Seen(Arc<Mutex<Vec<String>>>);

    fn per_stage(layouts: &[BindGroupLayoutDescriptor], stage: ShaderStages) -> (u32, u32) {
        let (mut t, mut s) = (0, 0);
        for e in layouts.iter().flat_map(|l| &l.entries).filter(|e| e.visibility.contains(stage)) {
            let n = e.count.map_or(1, |c| c.get());
            match e.ty {
                BindingType::Texture { .. } => t += n,
                BindingType::Sampler(_) => s += n,
                _ => {}
            }
        }
        (t, s)
    }

    fn watch(cache: Res<PipelineCache>, seen: Res<Seen>) {
        let mut out = vec![];
        for p in cache.pipelines() {
            let (label, layout) = match &p.descriptor {
                PipelineDescriptor::RenderPipelineDescriptor(d) => (d.label.clone(), &d.layout),
                PipelineDescriptor::MeshPipelineDescriptor(d) => (d.label.clone(), &d.layout),
                PipelineDescriptor::ComputePipelineDescriptor(_) => continue,
            };
            let (ft, fs) = per_stage(layout, ShaderStages::FRAGMENT);
            let (vt, vs) = per_stage(layout, ShaderStages::VERTEX);
            let state = match &p.state {
                CachedPipelineState::Ok(_) => "ok",
                CachedPipelineState::Err(_) => "rejected",
                _ => "pending",
            };
            out.push(format!("{}: fragment {ft} textures {fs} samplers, vertex {vt} textures {vs} samplers, {state}", label.unwrap_or_default()));
        }
        *seen.0.lock().unwrap() = out;
    }

    let settings = WgpuSettings { constrained_limits: Some(WgpuLimits::default()), ..default() };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(settings)), ..default() }),
    );
    let seen = Seen::default();
    app.sub_app_mut(RenderApp).insert_resource(seen.clone()).add_systems(Render, watch.in_set(RenderSystems::Cleanup));
    app.finish();
    app.cleanup();
    let world = app.world_mut();
    let mut target = Image::new_target_texture(64, 64, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = RenderAssetUsages::default();
    let mut images = world.resource_mut::<Assets<Image>>();
    let target = images.add(target);
    let (c, d) = (image(&mut images, [200, 200, 200, 255], true), image(&mut images, [128, 128, 255, 255], false));
    #[allow(unused_mut)]
    let mut m = StandardMaterial {
        base_color_texture: Some(c.clone()),
        emissive_texture: Some(c.clone()),
        metallic_roughness_texture: Some(d.clone()),
        normal_map_texture: Some(d.clone()),
        occlusion_texture: Some(d.clone()),
        depth_map: Some(d.clone()),
        clearcoat: 1.0,
        anisotropy_strength: 0.5,
        ..default()
    };
    #[cfg(feature = "textures")]
    {
        m.specular_tint_texture = Some(c);
        m.diffuse_transmission_texture = Some(d.clone());
        m.specular_transmission_texture = Some(d.clone());
        m.thickness_texture = Some(d.clone());
        m.specular_texture = Some(d.clone());
        m.clearcoat_texture = Some(d.clone());
        m.clearcoat_roughness_texture = Some(d.clone());
        m.clearcoat_normal_texture = Some(d.clone());
        m.anisotropy_texture = Some(d);
    }
    let m = world.resource_mut::<Assets<StandardMaterial>>().add(m);
    let cube = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Cuboid::default()).with_generated_tangents().unwrap());
    world.spawn((Mesh3d(cube), MeshMaterial3d(m)));
    world.spawn((Camera3d::default(), RenderTarget::Image(target.into()), Transform::from_xyz(-2.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    for _ in 0..300 {
        app.update();
    }
    for line in seen.0.lock().unwrap().iter() {
        println!("limits arm={arm} {line}");
    }
}
