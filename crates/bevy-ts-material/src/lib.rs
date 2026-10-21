//! Script materials: a material's WGSL lives in `.wgsl` files and its parameter schema in TS
//! (`bevy-ts/material`); the prebuilt engine draws it with one erased Rust material and no Rust
//! per material.

pub mod kinds;
pub mod layout;
pub mod render;
pub mod wgsl;

pub use kinds::MaterialKinds;
pub use render::{Kind, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};

use bevy::prelude::*;
use bevy_ts::plugin_api::BevyTsAppExt;

/// The TS module `material()` comes from.
pub const MODULE: &str = "bevy-ts/material";

/// Script materials for an app with [`bevy_ts::BevyTsPlugin`]: the renderer half, the
/// `materials` manifest section, `.wgsl` saves, and `bevy-ts/material`.
pub struct ScriptMaterialPlugin;

impl Plugin for ScriptMaterialPlugin {
    fn build(&self, app: &mut App) {
        if let Err(e) = bevy_ts::modules::register(MODULE, include_str!("../ts/material.js"), include_str!("../ts/material.d.ts")) {
            panic!("{e}");
        }
        app.add_plugins(ScriptMaterialRenderPlugin)
            .init_resource::<MaterialKinds>()
            .add_ts_section(kinds::SECTION, |world, input| kinds::check(world, input.generation, input.bytes, input.location.clone()))
            .on_ts_section_applied(kinds::SECTION, |world, s| kinds::apply(world, s.generation, s.present))
            .on_ts_files_changed(kinds::files_changed)
            .add_systems(Update, kinds::report_pipeline_errors);
    }
}
