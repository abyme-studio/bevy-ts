//! One erased Rust material for every script 2D material (change `script-material2d`, design 1).
//! Bevy 0.20's 2D material pipeline is erased under its `Material2d` trait: specialise and queue
//! read only the shared `RenderMaterial2dInstances` and each prepared asset's `MaterialProperties`.
//! So `ScriptMaterial2d` implements `ErasedRenderAsset` by hand, as `ScriptMaterial` does for 3D,
//! returning Bevy's `PreparedMaterial2d` with its kind's layout and shaders, and the per-type
//! systems Bevy's `Material2dPlugin<M>` would add are written here for `MeshScriptMaterial2d`.

use crate::render::{AfterImages, ImageSamplers, InstanceBuffer, Kind, Textured, allocate, rebind_samplers, unprepared};
use bevy::asset::{AsAssetId, AssetEventSystems};
use bevy::core_pipeline::core_2d::{AlphaMask2d, Opaque2d, Transparent2d};
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::{SRes, SResMut};
use bevy::material::key::{ErasedMaterialKey, ErasedMeshPipelineKey};
use bevy::material::{MaterialProperties, RenderPhaseType};
use bevy::prelude::*;
use bevy::render::camera::{DirtySpecializationSystems, DirtySpecializations};
use bevy::render::erased_render_asset::{ErasedRenderAsset, ErasedRenderAssetPlugin, PrepareAssetError};
use bevy::render::material_bind_groups::{MaterialBindGroupAllocator, MaterialBindGroupAllocators, RenderMaterialBindings};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::{AddRenderCommand, DrawFunctions, SetItemPipeline};
use bevy::render::render_resource::BindGroupLayoutDescriptor;
use bevy::render::renderer::RenderDevice;
use bevy::render::storage::GpuShaderBuffer;
use bevy::render::sync_world::MainEntity;
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::{Extract, RenderApp, RenderStartup};
use bevy::sprite_render::{
    DrawMesh2d, EntitiesNeedingSpecialization, MATERIAL_2D_BIND_GROUP_INDEX, Material2dFragmentShader, Material2dVertexShader,
    Mesh2dPipelineKey, Pass2dAlphaMaskDrawFunction, Pass2dOpaqueDrawFunction, Pass2dTransparentDrawFunction, PreparedMaterial2d,
    RenderMaterial2dInstances, SetMaterial2dBindGroup, SetMesh2dBindGroup, SetMesh2dViewBindGroup, alpha_mode_pipeline_key_2d,
    base_specialize, extract_2d_meshes,
};
use std::any::TypeId;
use std::sync::Arc;

/// One script 2D material instance: the fields `ScriptMaterial` has, under its own asset type so
/// Bevy's 2D pipeline prepares it and its own allocator holds it.
#[derive(Asset, TypePath, Clone)]
pub struct ScriptMaterial2d {
    pub kind: Arc<Kind>,
    /// The params struct's bytes, `kind.bindings.params().size` long.
    pub data: Vec<u8>,
    /// One per texture of the kind; `None` binds Bevy's white fallback.
    pub textures: Vec<Option<Handle<Image>>>,
}

impl From<crate::ScriptMaterial> for ScriptMaterial2d {
    fn from(m: crate::ScriptMaterial) -> Self {
        ScriptMaterial2d { kind: m.kind, data: m.data, textures: m.textures }
    }
}

impl Textured for ScriptMaterial2d {
    fn textures(&self) -> &[Option<Handle<Image>>] {
        &self.textures
    }
}

/// Draws a `Mesh2d` with a script 2D material, as `MeshMaterial2d` does with a typed one.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MeshScriptMaterial2d(pub Handle<ScriptMaterial2d>);

impl AsAssetId for MeshScriptMaterial2d {
    type Asset = ScriptMaterial2d;

    fn as_asset_id(&self) -> AssetId<ScriptMaterial2d> {
        self.0.id()
    }
}

/// Bevy's `DrawMaterial2d`, which is not public: the same commands.
pub type DrawScriptMaterial2d = (
    SetItemPipeline,
    SetMesh2dViewBindGroup<0>,
    SetMesh2dBindGroup<1>,
    SetMaterial2dBindGroup<MATERIAL_2D_BIND_GROUP_INDEX>,
    DrawMesh2d,
);

/// The 2D half of [`crate::ScriptMaterialRenderPlugin`]. Needs Bevy's `SpriteRenderPlugin` (its
/// `Mesh2dRenderPlugin` and `Materials2dPlugin`) in an app that renders; without a renderer it
/// adds the asset and the main-world systems only.
pub(crate) fn plugin(app: &mut App) {
    app.init_asset::<ScriptMaterial2d>()
        .init_resource::<EntitiesNeedingSpecialization<ScriptMaterial2d>>()
        .init_resource::<ImageSamplers<ScriptMaterial2d>>()
        .add_systems(
            PostUpdate,
            ((mark_meshes_changed, check_entities_needing_specialization).chain(), rebind_samplers::<ScriptMaterial2d>).after(AssetEventSystems),
        );
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    // Bevy's 2D render resources: absent without `SpriteRenderPlugin`, and then nothing draws 2D.
    if !render_app.world().contains_resource::<RenderMaterial2dInstances>() {
        return;
    }
    app.add_plugins(ErasedRenderAssetPlugin::<ScriptMaterial2d, AfterImages>::default());
    let render_app = app.sub_app_mut(RenderApp);
    render_app
        .add_render_command::<Opaque2d, DrawScriptMaterial2d>()
        .add_render_command::<AlphaMask2d, DrawScriptMaterial2d>()
        .add_render_command::<Transparent2d, DrawScriptMaterial2d>()
        .add_systems(RenderStartup, init_allocator)
        .add_systems(
            ExtractSchedule,
            (
                extract_instances.before(extract_2d_meshes),
                extract_needing_specialization.in_set(DirtySpecializationSystems::CheckForChanges),
                extract_specializations_removed.in_set(DirtySpecializationSystems::CheckForRemovals),
            ),
        );
}

fn init_allocator(device: Res<RenderDevice>, mut allocators: ResMut<MaterialBindGroupAllocators>) {
    // Non-bindless: the layout given here is unused; each allocation brings its kind's.
    allocators.insert(
        TypeId::of::<ScriptMaterial2d>(),
        MaterialBindGroupAllocator::new(&device, "script_material2d", None, BindGroupLayoutDescriptor::new("script_material2d", &[]), None),
    );
}

/// The 2D phase and pipeline key bits of an alpha mode. 2D has three; `material2d()` refuses the
/// rest, so any other is drawn opaque.
pub fn phase_2d(alpha: AlphaMode) -> (RenderPhaseType, Mesh2dPipelineKey) {
    let phase = match alpha {
        AlphaMode::Blend => RenderPhaseType::Transparent,
        AlphaMode::Mask(_) => RenderPhaseType::AlphaMask,
        _ => RenderPhaseType::Opaque,
    };
    (phase, alpha_mode_pipeline_key_2d(alpha))
}

impl ErasedRenderAsset for ScriptMaterial2d {
    type SourceAsset = ScriptMaterial2d;
    type ErasedAsset = PreparedMaterial2d;
    type Param = (
        SRes<DrawFunctions<Opaque2d>>,
        SRes<DrawFunctions<AlphaMask2d>>,
        SRes<DrawFunctions<Transparent2d>>,
        SResMut<MaterialBindGroupAllocators>,
        SResMut<RenderMaterialBindings>,
        SRes<RenderAssets<GpuImage>>,
        SRes<FallbackImage>,
        Option<SRes<InstanceBuffer>>,
        SRes<RenderAssets<GpuShaderBuffer>>,
    );

    fn prepare_asset(
        source: ScriptMaterial2d,
        asset_id: AssetId<ScriptMaterial2d>,
        (opaque, alpha_mask, transparent, allocators, bindings, images, fallback, instances, buffers): &mut SystemParamItem<Self::Param>,
    ) -> Result<PreparedMaterial2d, PrepareAssetError<ScriptMaterial2d>> {
        let kind = source.kind.clone();
        let Some(mut unprepared) = unprepared(&kind, &source.data, &source.textures, images, fallback, instances.as_deref(), buffers) else {
            return Err(PrepareAssetError::RetryNextUpdate(source));
        };
        let binding = allocate(allocators, bindings, TypeId::of::<ScriptMaterial2d>(), asset_id.untyped(), &mut unprepared, &kind.layout);
        // Only the three modes 2D draws; any other was refused at `material2d()`.
        let alpha_mode = match kind.alpha_mode {
            a @ (AlphaMode::Blend | AlphaMode::Mask(_)) => a,
            _ => AlphaMode::Opaque,
        };
        let (render_phase_type, key) = phase_2d(alpha_mode);
        let mut properties = MaterialProperties {
            alpha_mode,
            render_phase_type,
            material_layout: Some(kind.layout.clone()),
            mesh_pipeline_key_bits: ErasedMeshPipelineKey::new(key),
            base_specialize: Some(base_specialize),
            material_key: ErasedMaterialKey::new(kind.id),
            ..default()
        };
        properties.add_draw_function(Pass2dOpaqueDrawFunction, opaque.read().id::<DrawScriptMaterial2d>());
        properties.add_draw_function(Pass2dAlphaMaskDrawFunction, alpha_mask.read().id::<DrawScriptMaterial2d>());
        properties.add_draw_function(Pass2dTransparentDrawFunction, transparent.read().id::<DrawScriptMaterial2d>());
        if let Some(s) = &kind.fragment {
            properties.add_shader(Material2dFragmentShader, s.clone());
        }
        if let Some(s) = &kind.vertex {
            properties.add_shader(Material2dVertexShader, s.clone());
        }
        Ok(PreparedMaterial2d { binding, properties: Arc::new(properties) })
    }

    fn unload_asset(source: AssetId<ScriptMaterial2d>, (.., allocators, bindings, _, _, _, _): &mut SystemParamItem<Self::Param>) {
        if let Some(b) = bindings.remove(&source.untyped())
            && let Some(a) = allocators.get_mut(&TypeId::of::<ScriptMaterial2d>())
        {
            a.free(b);
        }
    }
}

type VisibilityOrMaterialChanged = Or<(Changed<ViewVisibility>, Changed<MeshScriptMaterial2d>)>;
type MeshOrMaterialChanged = Or<(Changed<Mesh2d>, AssetChanged<Mesh2d>, Changed<MeshScriptMaterial2d>, AssetChanged<MeshScriptMaterial2d>)>;

/// Bevy's `extract_mesh_materials_2d`, for `MeshScriptMaterial2d`: before `extract_2d_meshes`,
/// which skips a mesh with no material instance.
fn extract_instances(
    mut instances: ResMut<RenderMaterial2dInstances>,
    changed: Extract<Query<(Entity, &ViewVisibility, &MeshScriptMaterial2d), VisibilityOrMaterialChanged>>,
    mut removed: Extract<RemovedComponents<MeshScriptMaterial2d>>,
) {
    let ours = |id: &bevy::asset::UntypedAssetId| id.type_id() == TypeId::of::<ScriptMaterial2d>();
    for (entity, visibility, m) in &changed {
        if visibility.get() {
            instances.insert(entity.into(), m.0.id().untyped());
        } else if instances.get(&MainEntity::from(entity)).is_some_and(ours) {
            instances.remove(&MainEntity::from(entity));
        }
    }
    for entity in removed.read() {
        // Re-added this frame, or now another material type's: not ours to remove.
        if !changed.contains(entity) && instances.get(&MainEntity::from(entity)).is_some_and(ours) {
            instances.remove(&MainEntity::from(entity));
        }
    }
}

/// Bevy's `mark_2d_meshes_as_changed_if_their_materials_changed`: `extract_2d_meshes` re-extracts
/// only changed meshes, and it is where a mesh takes its material's binding.
fn mark_meshes_changed(
    mut queries: ParamSet<(
        Query<&mut Mesh2d, Or<(Changed<MeshScriptMaterial2d>, AssetChanged<MeshScriptMaterial2d>)>>,
        Query<&mut Mesh2d>,
    )>,
    mut removed: RemovedComponents<MeshScriptMaterial2d>,
) {
    for mut mesh in &mut queries.p0() {
        mesh.set_changed();
    }
    let mut all = queries.p1();
    for e in removed.read() {
        if let Ok(mut mesh) = all.get_mut(e) {
            mesh.set_changed();
        }
    }
}

fn check_entities_needing_specialization(
    needs: Query<Entity, (MeshOrMaterialChanged, With<MeshScriptMaterial2d>)>,
    mut out: ResMut<EntitiesNeedingSpecialization<ScriptMaterial2d>>,
    mut removed_mesh: RemovedComponents<Mesh2d>,
    mut removed_material: RemovedComponents<MeshScriptMaterial2d>,
) {
    out.changed.clear();
    out.removed.clear();
    out.changed.extend(needs.iter());
    out.removed.extend(removed_mesh.read().chain(removed_material.read()));
}

fn extract_needing_specialization(needs: Extract<Res<EntitiesNeedingSpecialization<ScriptMaterial2d>>>, mut dirty: ResMut<DirtySpecializations>) {
    for e in needs.changed.iter() {
        dirty.changed_renderables.insert(MainEntity::from(*e));
    }
}

fn extract_specializations_removed(needs: Extract<Res<EntitiesNeedingSpecialization<ScriptMaterial2d>>>, mut dirty: ResMut<DirtySpecializations>) {
    for e in needs.removed.iter() {
        dirty.removed_renderables.insert(MainEntity::from(*e));
    }
}
