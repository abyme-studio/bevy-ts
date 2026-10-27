//! One erased Rust material for every script material (design decision 1). `ScriptMaterial`
//! implements `ErasedRenderAsset` by hand, as Bevy's `manual_material.rs` does: each instance
//! carries its kind (bind group layout and shaders, shared by every instance of the kind) and its
//! uniform bytes and textures. Every kind sits behind the one `TypeId` allocator, non-bindless,
//! which takes the layout per allocation, so kinds with different layouts coexist.

use crate::layout::Bindings;
use bevy::asset::{AsAssetId, AssetEventSystems};
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d, Transparent3d};
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::{SRes, SResMut};
use bevy::material::key::{ErasedMaterialKey, ErasedMaterialPipelineKey, ErasedMeshPipelineKey};
use bevy::material::specialize::{SpecializedMeshPipelineError, UserSpecializeFn};
use bevy::material::{MaterialProperties, RenderPhaseType};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    DrawMaterial, EntitiesNeedingSpecialization, MainPassAlphaMaskDrawFunction, MainPassOpaqueDrawFunction,
    MainPassTransparentDrawFunction, MaterialFragmentShader, MaterialVertexShader, MeshPipelineKey, PreparedMaterial,
    RenderMaterialInstance, RenderMaterialInstances, base_specialize,
};
use bevy::platform::collections::hash_map::Entry;
use bevy::prelude::*;
use bevy::render::camera::{DirtySpecializationSystems, DirtySpecializations};
use bevy::image::ImageSampler;
use bevy::platform::collections::HashMap;
use bevy::render::erased_render_asset::{ErasedRenderAsset, ErasedRenderAssetDependency, ErasedRenderAssetPlugin, PrepareAssetError};
use bevy::render::material_bind_groups::{MaterialBindGroupAllocator, MaterialBindGroupAllocators, RenderMaterialBindings};
use bevy::render::render_asset::{RenderAssets, prepare_assets};
use bevy::render::render_phase::DrawFunctions;
use bevy::render::render_resource::binding_types::{sampler, storage_buffer_read_only_sized, texture_2d, uniform_buffer_sized};
use bevy::render::render_resource::{
    BindGroupBuilder, BindGroupLayoutDescriptor, BlendComponent, BlendFactor, BlendOperation, BlendState,
    RenderPipelineDescriptor, SamplerBindingType, ShaderStages, TextureSampleType, TextureViewDimension, UnpreparedBindingResource, UnpreparedBindingResources,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::sync_world::MainEntity;
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::{Extract, Render, RenderApp, RenderStartup};
use std::any::TypeId;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// What every instance of one material kind shares: its bind group layout, its shaders and its
/// alpha mode. A new schema or shader makes a new `Kind`; instances are rebuilt against it.
#[derive(Debug)]
pub struct Kind {
    /// Unique per `Kind`, and the pipeline specialization key: every kind sits behind one
    /// `TypeId`, so without it two kinds would share one specialized pipeline (task 1.1).
    pub id: u64,
    pub name: String,
    pub bindings: Bindings,
    pub layout: BindGroupLayoutDescriptor,
    pub fragment: Option<Handle<Shader>>,
    pub vertex: Option<Handle<Shader>>,
    pub alpha_mode: AlphaMode,
}

impl Kind {
    pub fn new(name: &str, bindings: Bindings, fragment: Option<Handle<Shader>>, vertex: Option<Handle<Shader>>, alpha_mode: AlphaMode) -> Self {
        let vis = ShaderStages::VERTEX_FRAGMENT;
        let mut entries = Vec::new();
        if let Some(b) = bindings.params_binding() {
            entries.push(uniform_buffer_sized(false, NonZeroU64::new(bindings.params().size as u64)).build(b, vis));
        }
        for i in 0..bindings.textures().len() {
            let b = bindings.texture_binding(i);
            entries.push(texture_2d(TextureSampleType::Float { filterable: true }).build(b, vis));
            entries.push(sampler(SamplerBindingType::Filtering).build(b + 1, vis));
        }
        if let Some(b) = bindings.instance_binding() {
            entries.push(storage_buffer_read_only_sized(false, None).build(b, vis));
        }
        let layout = BindGroupLayoutDescriptor::new(format!("script_material {name}"), &entries);
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Kind { id: NEXT.fetch_add(1, Ordering::Relaxed), name: name.into(), bindings, layout, fragment, vertex, alpha_mode }
    }
}

/// One script material instance.
#[derive(Asset, TypePath, Clone)]
pub struct ScriptMaterial {
    pub kind: Arc<Kind>,
    /// The params struct's bytes, `kind.bindings.params().size` long.
    pub data: Vec<u8>,
    /// One per texture of the kind; `None` binds Bevy's white fallback.
    pub textures: Vec<Option<Handle<Image>>>,
}

/// Draws a mesh with a script material, as `MeshMaterial3d` does with a typed one.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ScriptMaterial3d(pub Handle<ScriptMaterial>);

impl AsAssetId for ScriptMaterial3d {
    type Asset = ScriptMaterial;

    fn as_asset_id(&self) -> AssetId<ScriptMaterial> {
        self.0.id()
    }
}

/// The renderer half of script materials.
pub struct ScriptMaterialRenderPlugin;

impl Plugin for ScriptMaterialRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ScriptMaterial>()
            .add_plugins(ErasedRenderAssetPlugin::<ScriptMaterial, AfterImages>::default())
            .init_resource::<EntitiesNeedingSpecialization<ScriptMaterial>>()
            .init_resource::<ImageSamplers>()
            .add_systems(PostUpdate, (check_entities_needing_specialization, rebind_samplers).after(AssetEventSystems));
        crate::scene::plugin(app);
        crate::alpha::plugin(app);
        crate::instance::plugin(app);
        let instances = crate::instance::buffer(app.world());
        let watch = crate::wgsl::PipelineWatch::default();
        app.insert_resource(watch.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        if let Some(b) = instances {
            render_app.insert_resource(InstanceBuffer(b));
        }
        render_app
            .insert_resource(watch)
            .add_systems(Render, crate::wgsl::watch_pipelines.in_set(bevy::render::RenderSystems::Cleanup))
            .add_systems(ExtractSchedule, crate::wgsl::extract_versions)
            .add_systems(RenderStartup, init_render_resources)
            .add_systems(
            ExtractSchedule,
            (
                extract_instances,
                extract_needing_specialization.in_set(DirtySpecializationSystems::CheckForChanges),
                extract_specializations_removed.in_set(DirtySpecializationSystems::CheckForRemovals),
            ),
        );
    }
}

/// Script materials prepare after the frame's images, so a material and its image changed in one
/// frame bind the image's new sampler. Bevy's own dependencies only name erased assets.
struct AfterImages;

impl ErasedRenderAssetDependency for AfterImages {
    fn register_system(render_app: &mut SubApp, system: bevy::ecs::schedule::ScheduleConfigs<bevy::ecs::system::ScheduleSystem>) {
        render_app.add_systems(Render, system.after(prepare_assets::<GpuImage>));
    }
}

/// The sampler last seen for each image a script material holds.
#[derive(Resource, Default)]
struct ImageSamplers(HashMap<AssetId<Image>, ImageSampler>);

/// A material binds its images' samplers when it is prepared, and it is prepared only on its own
/// asset event. A re-prepared `GpuImage` makes a new sampler every time, so a changed sampler
/// (`assets.set(img, { sampler })`) touches every material holding the image, which re-prepares
/// it as the same instance. A rewrite that keeps the sampler (`setData`) touches none.
fn rebind_samplers(
    mut events: MessageReader<AssetEvent<Image>>,
    images: Res<Assets<Image>>,
    mut materials: ResMut<Assets<ScriptMaterial>>,
    mut seen: ResMut<ImageSamplers>,
) {
    for ev in events.read() {
        match *ev {
            AssetEvent::Modified { id } => {
                let Some(image) = images.get(id) else { continue };
                if seen.0.get(&id) == Some(&image.sampler) {
                    continue;
                }
                let holders: Vec<_> = materials
                    .iter()
                    .filter(|(_, m)| m.textures.iter().flatten().any(|h| h.id() == id))
                    .map(|(m, _)| m)
                    .collect();
                if holders.is_empty() {
                    continue;
                }
                seen.0.insert(id, image.sampler.clone());
                for m in holders {
                    // `into_inner` marks it modified; a bare `get_mut` does not.
                    materials.get_mut(m).map(|m| m.into_inner());
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                seen.0.remove(&id);
            }
            _ => {}
        }
    }
}

/// Render world: the storage buffer of every entity's `ScriptInstance`, bound by each kind with
/// instance params.
#[derive(Resource)]
pub struct InstanceBuffer(pub Handle<ShaderBuffer>);

fn init_render_resources(device: Res<RenderDevice>, mut allocators: ResMut<MaterialBindGroupAllocators>) {
    // Non-bindless: the layout given here is unused; each allocation brings its kind's.
    allocators.insert(
        TypeId::of::<ScriptMaterial>(),
        MaterialBindGroupAllocator::new(&device, "script_material", None, BindGroupLayoutDescriptor::new("script_material", &[]), None),
    );
}

impl ErasedRenderAsset for ScriptMaterial {
    type SourceAsset = ScriptMaterial;
    type ErasedAsset = PreparedMaterial;
    type Param = (
        SRes<DrawFunctions<Opaque3d>>,
        SRes<DrawFunctions<AlphaMask3d>>,
        SRes<DrawFunctions<Transparent3d>>,
        SResMut<MaterialBindGroupAllocators>,
        SResMut<RenderMaterialBindings>,
        SRes<RenderAssets<GpuImage>>,
        SRes<FallbackImage>,
        Option<SRes<InstanceBuffer>>,
        SRes<RenderAssets<GpuShaderBuffer>>,
    );

    fn prepare_asset(
        source: ScriptMaterial,
        asset_id: AssetId<ScriptMaterial>,
        (opaque, alpha_mask, transparent, allocators, bindings, images, fallback, instances, buffers): &mut SystemParamItem<Self::Param>,
    ) -> Result<PreparedMaterial, PrepareAssetError<ScriptMaterial>> {
        let kind = source.kind.clone();
        let mut resources = Vec::new();
        let mut data_buffer = Vec::new();
        if let Some(b) = kind.bindings.params_binding() {
            data_buffer = source.data.clone();
            resources.push((b, UnpreparedBindingResource::Data(0..data_buffer.len() as u32)));
        }
        for i in 0..kind.bindings.textures().len() {
            // The image's own sampler, as Bevy's `AsBindGroup` binds `#[sampler]`.
            let (view, sampler) = match source.textures.get(i).and_then(Option::as_ref) {
                Some(h) => match images.get(h) {
                    Some(img) => (img.texture_view.clone(), img.sampler.clone()),
                    None => return Err(PrepareAssetError::RetryNextUpdate(source)),
                },
                None => (fallback.d2.texture_view.clone(), fallback.d2.sampler.clone()),
            };
            let b = kind.bindings.texture_binding(i);
            resources.push((b, UnpreparedBindingResource::TextureView(TextureViewDimension::D2, view)));
            resources.push((b + 1, UnpreparedBindingResource::Sampler(SamplerBindingType::Filtering, sampler)));
        }
        if let Some(b) = kind.bindings.instance_binding() {
            // Bound once Bevy has made the buffer; it rebinds when the buffer grows.
            let Some(InstanceBuffer(h)) = instances.as_deref() else { return Err(PrepareAssetError::RetryNextUpdate(source)) };
            if buffers.get(h.id()).is_none() {
                return Err(PrepareAssetError::RetryNextUpdate(source));
            }
            resources.push((b, UnpreparedBindingResource::ShaderBuffer(h.clone())));
        }
        let mut unprepared = BindGroupBuilder { binding_resources: UnpreparedBindingResources(resources), data_buffer };
        let allocator = allocators.get_mut(&TypeId::of::<ScriptMaterial>()).expect("allocator inserted at render startup");
        let binding = match bindings.entry(asset_id.into()) {
            Entry::Occupied(mut e) => {
                allocator.free(*e.get());
                let b = allocator.allocate_unprepared(&mut unprepared, &kind.layout);
                *e.get_mut() = b;
                b
            }
            Entry::Vacant(e) => *e.insert(allocator.allocate_unprepared(&mut unprepared, &kind.layout)),
        };

        let render_phase_type = match kind.alpha_mode {
            AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply => RenderPhaseType::Transparent,
            AlphaMode::Mask(_) => RenderPhaseType::AlphaMask,
            AlphaMode::Opaque | AlphaMode::AlphaToCoverage => RenderPhaseType::Opaque,
        };
        let mut properties = MaterialProperties {
            alpha_mode: kind.alpha_mode,
            render_phase_type,
            material_layout: Some(kind.layout.clone()),
            mesh_pipeline_key_bits: ErasedMeshPipelineKey::new(MeshPipelineKey::empty()),
            base_specialize: Some(base_specialize),
            material_key: ErasedMaterialKey::new(kind.id),
            user_specialize: matches!(kind.alpha_mode, AlphaMode::Add).then_some(additive as UserSpecializeFn),
            ..default()
        };
        properties.add_draw_function(MainPassOpaqueDrawFunction, opaque.read().id::<DrawMaterial>());
        properties.add_draw_function(MainPassAlphaMaskDrawFunction, alpha_mask.read().id::<DrawMaterial>());
        properties.add_draw_function(MainPassTransparentDrawFunction, transparent.read().id::<DrawMaterial>());
        if let Some(s) = &kind.fragment {
            properties.add_shader(MaterialFragmentShader, s.clone());
        }
        if let Some(s) = &kind.vertex {
            properties.add_shader(MaterialVertexShader, s.clone());
        }
        Ok(PreparedMaterial { binding, properties: Arc::new(properties) })
    }

    fn unload_asset(source: AssetId<ScriptMaterial>, (.., allocators, bindings, _, _, _, _): &mut SystemParamItem<Self::Param>) {
        if let Some(b) = bindings.remove(&source.untyped())
            && let Some(a) = allocators.get_mut(&TypeId::of::<ScriptMaterial>())
        {
            a.free(b);
        }
    }
}

/// `AlphaMode::Add`: Bevy shares the premultiplied blend with `Premultiplied` and zeroes alpha in
/// its PBR shader to add. A script material's own WGSL keeps its alpha, which then covers what is
/// behind, so the blend itself adds: colour plus what is there, its alpha kept.
fn additive(
    _: &dyn std::any::Any,
    descriptor: &mut RenderPipelineDescriptor,
    _: &MeshVertexBufferLayoutRef,
    _: ErasedMaterialPipelineKey,
) -> Result<(), SpecializedMeshPipelineError> {
    let add = BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
    let keep = BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
    for target in descriptor.fragment.iter_mut().flat_map(|f| f.targets.iter_mut()).flatten() {
        target.blend = Some(BlendState { color: add, alpha: keep });
    }
    Ok(())
}

type VisibilityOrMaterialChanged = Or<(Changed<ViewVisibility>, Changed<ScriptMaterial3d>)>;
type MeshOrMaterialChanged = Or<(Changed<Mesh3d>, AssetChanged<Mesh3d>, Changed<ScriptMaterial3d>, AssetChanged<ScriptMaterial3d>)>;

fn extract_instances(
    mut instances: ResMut<RenderMaterialInstances>,
    changed: Extract<Query<(Entity, &ViewVisibility, &ScriptMaterial3d), VisibilityOrMaterialChanged>>,
    mut removed: Extract<RemovedComponents<ScriptMaterial3d>>,
) {
    let tick = instances.current_change_tick;
    for (entity, visibility, m) in &changed {
        if visibility.get() {
            instances.instances.insert(entity.into(), RenderMaterialInstance { asset_id: m.0.id().untyped(), last_change_tick: tick });
        } else {
            instances.instances.remove(&MainEntity::from(entity));
        }
    }
    for entity in removed.read() {
        instances.instances.remove(&MainEntity::from(entity));
    }
}

pub(crate) fn check_entities_needing_specialization(
    needs: Query<Entity, (MeshOrMaterialChanged, With<ScriptMaterial3d>)>,
    mut out: ResMut<EntitiesNeedingSpecialization<ScriptMaterial>>,
    mut removed_mesh: RemovedComponents<Mesh3d>,
    mut removed_material: RemovedComponents<ScriptMaterial3d>,
) {
    out.changed.clear();
    out.removed.clear();
    out.changed.extend(needs.iter());
    out.removed.extend(removed_mesh.read().chain(removed_material.read()));
}

fn extract_needing_specialization(needs: Extract<Res<EntitiesNeedingSpecialization<ScriptMaterial>>>, mut dirty: ResMut<DirtySpecializations>) {
    for e in needs.changed.iter() {
        dirty.changed_renderables.insert(MainEntity::from(*e));
    }
}

fn extract_specializations_removed(needs: Extract<Res<EntitiesNeedingSpecialization<ScriptMaterial>>>, mut dirty: ResMut<DirtySpecializations>) {
    for e in needs.removed.iter() {
        dirty.removed_renderables.insert(MainEntity::from(*e));
    }
}
