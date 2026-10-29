//! Script materials: a material's WGSL lives in `.wgsl` files and its parameter schema in TS
//! (`bevy-ts/material`); the prebuilt engine draws it with one erased Rust material and no Rust
//! per material.

pub mod alpha;
pub mod bitmap_text;
pub mod instance;
pub mod kinds;
pub mod layout;
pub mod owned;
pub mod pass;
pub mod pixel2d;
pub mod render;
pub mod scene;
pub mod wgsl;
pub mod world_text;

pub use bitmap_text::{BitmapText, BitmapTextPlugin, BitmapTextSystems, SpriteFont};
pub use alpha::{Fade, FadeMaterial, MaterialAlpha, MaterialAlphaTargets, standard_of};
pub use instance::ScriptInstance;
pub use kinds::MaterialKinds;
pub use render::{Kind, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
pub use pass::{CameraPasses, PassKinds, ScriptPass, ScriptPassPlugin};
pub use pixel2d::{ClipMode, PixelCamera, Pixel2dPlugin, Pixel2dSystems, ScreenSize, SpriteClip, YSort};
pub use scene::{GltfMaterialFilter, SceneMaterialSet, SceneMaterialTargets, scene_entities};
pub use world_text::{ComputedWorldText, LaidOut, WorldText, WorldTextMaterial, WorldTextPlugin, WorldTextStyle, WorldTextSystems};

use bevy::prelude::*;
use bevy_ts::plugin_api::BevyTsAppExt;

/// The TS module `material()` comes from.
pub const MODULE: &str = "bevy-ts/material";

/// Script materials and passes for an app with [`bevy_ts::BevyTsPlugin`]: the renderer halves,
/// the `materials` and `passes` manifest sections, `.wgsl` saves, and `bevy-ts/material`.
pub struct ScriptMaterialPlugin;

impl Plugin for ScriptMaterialPlugin {
    fn build(&self, app: &mut App) {
        if let Err(e) = bevy_ts::modules::register(MODULE, include_str!("../ts/material.js"), include_str!("../ts/material.d.ts")) {
            panic!("{e}");
        }
        app.add_plugins((ScriptMaterialRenderPlugin, ScriptPassPlugin))
            .init_resource::<MaterialKinds>()
            .add_ts_section(kinds::SECTION, |world, input| kinds::check(world, input.generation, input.bytes, input.location.clone()))
            .on_ts_section_applied(kinds::SECTION, |world, s| kinds::apply(world, s.generation, s.present))
            .on_ts_files_changed(kinds::files_changed)
            .add_ts_section(pass::SECTION, |world, input| pass::check(world, input.generation, input.bytes, input.location.clone()))
            .on_ts_section_applied(pass::SECTION, |world, s| pass::apply(world, s.generation, s.present))
            .on_ts_files_changed(pass::files_changed)
            .ts_hand_map::<ScriptMaterial>(owned::read, owned::write)
            .add_systems(Update, kinds::report_pipeline_errors)
            .add_systems(PostUpdate, kinds::sweep.after(bevy_ts::BevyTsSet::PostUpdate));
    }
}
