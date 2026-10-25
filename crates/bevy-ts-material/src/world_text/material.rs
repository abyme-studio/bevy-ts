//! The world-text material (design 8): a `StandardMaterial` base (white, alpha-to-coverage, back
//! faces culled, unlit unless `lit`) and an extension that reads the shared SDF atlas, so 1,000
//! labels of one style draw with one instance. Instances are cached by `lit` and outline, and a
//! style no label holds is dropped when a new one is made.

use bevy::asset::uuid_handle;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, FragmentState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use std::sync::Arc;

/// What a label draws with.
pub type WorldTextMaterial = ExtendedMaterial<StandardMaterial, WorldTextExt>;

/// The SDF half of [`WorldTextMaterial`]: the atlas, and the outline. `world_text.wesl`.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WorldTextExt {
    #[uniform(100)]
    pub params: WorldTextParams,
    #[texture(101)]
    #[sampler(102)]
    pub atlas: Handle<Image>,
}

/// Distances in raster pixels: `spread` is how far the field reaches, `outline` the band's width.
#[derive(ShaderType, Reflect, Debug, Clone, Copy, PartialEq, Default)]
pub struct WorldTextParams {
    pub outline_color: Vec4,
    pub outline: f32,
    pub spread: f32,
}

pub(crate) const SHADER: Handle<Shader> = uuid_handle!("8d0f6f2e-3c1b-4c55-9a51-3b4f2a7d9e10");

impl MaterialExtension for WorldTextExt {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// A depth-only prepass may have no fragment stage, so nothing would drop the space between
    /// glyphs there: give it this one, as `Fade` does.
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if descriptor.fragment.is_none() {
            descriptor.fragment = Some(FragmentState { shader: SHADER, shader_defs: descriptor.vertex.shader_defs.clone(), ..default() });
        }
        Ok(())
    }
}

/// A style's key: `lit`, and the outline's width (raster pixels) and colour, as bits.
type Style = (bool, Option<(u32, [u32; 4])>);

/// The material instance of each style in use.
#[derive(Resource, Default)]
pub struct WorldTextMaterials(pub(crate) HashMap<Style, Handle<WorldTextMaterial>>);

impl WorldTextMaterials {
    /// The instances made so far.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn handles(&self) -> impl Iterator<Item = &Handle<WorldTextMaterial>> {
        self.0.values()
    }

    /// The instance for `lit` and `outline` (width in raster pixels, colour), made on first use.
    pub(crate) fn get(&mut self, materials: &mut Assets<WorldTextMaterial>, atlas: &Handle<Image>, lit: bool, outline: Option<(f32, Color)>, spread: f32) -> Handle<WorldTextMaterial> {
        let key = (lit, outline.map(|(w, c)| (w.to_bits(), c.to_linear().to_f32_array().map(f32::to_bits))));
        if !self.0.contains_key(&key) {
            // A new style: drop the styles no label holds any more, so a bound outline that
            // changes every frame keeps one or two instances, not one for every value it showed.
            self.0.retain(|_, h| match h {
                Handle::Strong(a) => Arc::strong_count(a) > 1,
                Handle::Uuid(..) => true,
            });
        }
        self.0
            .entry(key)
            .or_insert_with(|| {
                let (outline, outline_color) = outline.map_or((0.0, Vec4::ZERO), |(w, c)| (w, Vec4::from_array(c.to_linear().to_f32_array())));
                materials.add(WorldTextMaterial {
                    base: StandardMaterial { base_color: Color::WHITE, unlit: !lit, alpha_mode: AlphaMode::AlphaToCoverage, ..default() },
                    extension: WorldTextExt { params: WorldTextParams { outline_color, outline, spread }, atlas: atlas.clone() },
                })
            })
            .clone()
    }
}
