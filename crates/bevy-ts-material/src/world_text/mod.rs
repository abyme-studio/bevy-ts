//! World text (openspec change `world-text`): text drawn in the 3D world under a `Camera3d`. A
//! label is [`WorldText`] and [`WorldTextStyle`] with Bevy's `TextFont`, `TextColor`, `TextLayout`
//! and `LineHeight`; `<worldText>` writes exactly these. Everything after them is this module's: a
//! `PostUpdate` system lays out each changed label with Bevy's parley pipeline at one raster size,
//! turns the glyphs Bevy rasterised into distance fields in one shared atlas, and writes the
//! label's own mesh and one of a few shared materials. The label's `Mesh3d`, material and `Aabb`
//! are owned here: a script or system that writes them is not supported.

pub mod atlas;
pub mod layout;
pub mod material;
pub mod mesh;
pub mod sdf;

pub use atlas::WorldTextAtlas;
pub use material::{WorldTextExt, WorldTextMaterial, WorldTextMaterials, WorldTextParams};

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilitySystems};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::text::{LineHeight, TextColor, TextFont, TextLayout};

/// The em size, in pixels, every label is laid out and rasterised at; `size` scales it to world
/// units.
pub const RASTER: f32 = 48.0;

/// A label's text. It lies in the entity's local XY plane, reading along +X with +Y up and facing
/// +Z, placed by the entity's `Transform`; it is culled from behind and casts no shadow.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
#[require(WorldTextStyle, TextFont, TextColor, TextLayout, LineHeight, ComputedWorldText, NoAutoAabb, Transform, Visibility, NotShadowCaster)]
pub struct WorldText(pub String);

impl WorldText {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

/// How a label is sized and drawn, in world units. `TextFont`'s `font_size` is not used: `size` is
/// the em size. `LineHeight::RelativeToFont` is a multiple of `size`; `LineHeight::Px` is world units.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct WorldTextStyle {
    /// The em size in world units.
    pub size: f32,
    /// Extra space between letters, a fraction of `size`.
    pub letter_spacing: f32,
    /// The wrap width in world units; `None` does not wrap.
    pub width: Option<f32>,
    /// The point of the layout box at the entity's origin, from `[-0.5, -0.5]` (bottom left) to
    /// `[0.5, 0.5]` (top right).
    pub anchor: Vec2,
    /// A band of the colour around every glyph, its width a fraction of `size` from 0 to 0.125.
    pub outline: Option<(f32, Color)>,
    /// Shaded by the scene's lights, as a `StandardMaterial` of its colour; unlit when false.
    pub lit: bool,
}

impl Default for WorldTextStyle {
    fn default() -> Self {
        Self { size: 1.0, letter_spacing: 0.0, width: None, anchor: Vec2::ZERO, outline: None, lit: false }
    }
}

/// The widest outline, a fraction of `size`: the distance field's spread at the raster size.
pub const MAX_OUTLINE: f32 = sdf::SPREAD as f32 / RASTER;

/// What the last layout made: the layout box in the entity's local space and the number of glyph
/// quads drawn. Written by world text only.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct ComputedWorldText {
    pub rect: Rect,
    pub quads: u32,
    /// The label's own mesh, kept while its text is empty (when it has no `Mesh3d`).
    #[reflect(ignore)]
    pub(crate) mesh: Option<Handle<Mesh>>,
}

/// Labels laid out in the last `PostUpdate`: 0 on a frame where none changed.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaidOut(pub u32);

/// Where world text lays out, in `PostUpdate` before bounds are computed and before asset events
/// are sent, so the mesh and atlas it writes reach the GPU that frame. Whatever writes a label's
/// components in `PostUpdate` (TSX's bindings) runs before it, so the label draws that frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorldTextSystems;

const ATLAS_LABEL: &str = "embedded://bevy_ts_material/world_text.wesl";

/// World text for an app with Bevy's text and PBR plugins.
pub struct WorldTextPlugin;

impl Plugin for WorldTextPlugin {
    fn build(&self, app: &mut App) {
        let shader = Shader::from_wesl(include_str!("world_text.wesl"), ATLAS_LABEL);
        app.world_mut().resource_mut::<Assets<Shader>>().insert(material::SHADER.id(), shader).expect("a uuid handle");
        app.add_plugins(MaterialPlugin::<WorldTextMaterial>::default());
        let atlas = WorldTextAtlas::new(&mut app.world_mut().resource_mut::<Assets<Image>>(), atlas::START_HEIGHT, atlas::MAX_HEIGHT);
        app.register_type::<WorldText>()
            .register_type::<WorldTextStyle>()
            .register_type::<ComputedWorldText>()
            .insert_resource(atlas)
            .init_resource::<WorldTextMaterials>()
            .init_resource::<LaidOut>()
            .add_observer(on_remove)
            .configure_sets(PostUpdate, WorldTextSystems.before(VisibilitySystems::CalculateBounds).before(bevy::asset::AssetEventSystems).after(bevy::text::load_font_assets_into_font_collection))
            .add_systems(PostUpdate, layout::lay_out.in_set(WorldTextSystems));
    }
}

/// `WorldText` removed: the label's mesh, material and bounds go with it.
fn on_remove(removed: On<Remove<WorldText>>, mut commands: Commands) {
    commands.entity(removed.entity).try_remove::<(Mesh3d, MeshMaterial3d<WorldTextMaterial>, ComputedWorldText, Aabb, NoAutoAabb)>();
}
