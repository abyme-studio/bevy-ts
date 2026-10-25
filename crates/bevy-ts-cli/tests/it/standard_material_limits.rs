//! The `StandardMaterial` pipeline, with every texture field the engine builds set, under wgpu's
//! default limits: no render pipeline binds more than 16 sampled textures or 16 samplers to one
//! shader stage, across all its bind groups, and the opaque and prepass pipelines compile. Bevy's
//! four PBR texture features stay off for this (standard-material-fields, docs/materials.md).

use bevy::prelude::*;
use bevy::render::render_resource::{BindGroupLayoutDescriptor, BindingType, CachedPipelineState, PipelineCache, PipelineDescriptor, ShaderStages};
use bevy::render::settings::{RenderCreation, WgpuLimits, WgpuSettings};
use bevy::render::{Render, RenderApp, RenderPlugin, RenderSystems};
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::sync::{Arc, Mutex};

/// What the render world saw: each render pipeline's label and its per-stage counts.
#[derive(Default)]
struct Seen {
    /// (label, vertex textures, vertex samplers, fragment textures, fragment samplers)
    pipelines: Vec<(String, u32, u32, u32, u32)>,
    opaque_ok: bool,
    prepass_ok: bool,
    errors: Vec<String>,
}

#[derive(Resource, Clone, Default)]
struct Report(Arc<Mutex<Seen>>);

fn per_stage(layouts: &[BindGroupLayoutDescriptor], stage: ShaderStages) -> (u32, u32) {
    let (mut textures, mut samplers) = (0, 0);
    for e in layouts.iter().flat_map(|l| &l.entries).filter(|e| e.visibility.contains(stage)) {
        let n = e.count.map_or(1, |c| c.get());
        match e.ty {
            BindingType::Texture { .. } => textures += n,
            BindingType::Sampler(_) => samplers += n,
            _ => {}
        }
    }
    (textures, samplers)
}

fn watch(cache: Res<PipelineCache>, report: Res<Report>) {
    let mut seen = Seen::default();
    for p in cache.pipelines() {
        let (label, layout) = match &p.descriptor {
            PipelineDescriptor::RenderPipelineDescriptor(d) => (d.label.clone(), &d.layout),
            PipelineDescriptor::MeshPipelineDescriptor(d) => (d.label.clone(), &d.layout),
            PipelineDescriptor::ComputePipelineDescriptor(_) => continue,
        };
        let label = label.map(|l| l.to_string()).unwrap_or_default();
        let (vt, vs) = per_stage(layout, ShaderStages::VERTEX);
        let (ft, fs) = per_stage(layout, ShaderStages::FRAGMENT);
        match &p.state {
            CachedPipelineState::Ok(_) if label == "pbr_opaque_mesh_pipeline" => seen.opaque_ok = true,
            CachedPipelineState::Ok(_) if label == "pbr_prepass_pipeline" => seen.prepass_ok = true,
            CachedPipelineState::Err(e) => seen.errors.push(format!("{label}: {e}")),
            _ => {}
        }
        seen.pipelines.push((label, vt, vs, ft, fs));
    }
    *report.0.lock().unwrap() = seen;
}

fn texture(images: &mut Assets<Image>, srgb: bool) -> Handle<Image> {
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    images.add(Image::new_fill(Extent3d { width: 2, height: 2, depth_or_array_layers: 1 }, TextureDimension::D2, &[128, 128, 255, 255], format, RenderAssetUsages::default()))
}

#[test]
fn every_texture_fits_wgpus_default_limits() {
    let settings = WgpuSettings { constrained_limits: Some(WgpuLimits::default()), ..default() };
    let mut app = App::new();
    app.add_plugins(
        bevy_ts_cli::headless::default_plugins(true).set(RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(settings)), synchronous_pipeline_compilation: true, ..default() }),
    )
    .add_plugins(bevy_ts_cli::headless::HeadlessPlugin);
    let report = Report::default();
    app.sub_app_mut(RenderApp).insert_resource(report.clone()).add_systems(Render, watch.in_set(RenderSystems::Cleanup));
    app.finish();
    app.cleanup();

    let world = app.world_mut();
    let mut images = world.resource_mut::<Assets<Image>>();
    let (c, d) = (texture(&mut images, true), texture(&mut images, false));
    let material = StandardMaterial {
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
    let material = world.resource_mut::<Assets<StandardMaterial>>().add(material);
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Cuboid::default()).with_generated_tangents().unwrap());
    world.spawn((Mesh3d(mesh), MeshMaterial3d(material)));
    world.spawn((Camera3d::default(), Transform::from_xyz(-2.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((PointLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(0.0, 3.0, 0.0)));
    world.spawn((SpotLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(2.0, 3.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y)));

    for _ in 0..600 {
        app.update();
        let seen = report.0.lock().unwrap();
        assert!(seen.errors.is_empty(), "pipelines failed: {:?}", seen.errors);
        if seen.opaque_ok && seen.prepass_ok {
            break;
        }
    }
    let seen = report.0.lock().unwrap();
    assert!(seen.opaque_ok && seen.prepass_ok, "the opaque and prepass pipelines never compiled; pipelines: {:?}", seen.pipelines);
    let limits = WgpuLimits::default();
    let (tex, smp) = (limits.max_sampled_textures_per_shader_stage, limits.max_samplers_per_shader_stage);
    for (label, vt, vs, ft, fs) in &seen.pipelines {
        println!("{label}: vertex {vt} textures {vs} samplers, fragment {ft} textures {fs} samplers (limit {tex}/{smp})");
        assert!(*vt <= tex && *ft <= tex && *vs <= smp && *fs <= smp, "{label} passes the default limits");
    }
}
