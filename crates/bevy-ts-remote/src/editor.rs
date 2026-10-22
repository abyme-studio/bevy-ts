//! `x-bevy-ts-editor`: whose a component is, whether the engine maintains it, whether Add
//! component may offer it, and its icon category. bevy-ts owns the lists here.

use crate::protocol::{EditorInfo, EditorInfoCategory as Category, EditorInfoOrigin as Origin};
use bevy::prelude::*;
use bevy::reflect::TypeRegistration;
use bevy_ts::components::{ComponentState, TsComponent};

/// The crates of Bevy's workspace at the pinned version (0.20). A Bevy bump that adds one fails
/// the bare-engine test until it is listed.
pub const BEVY_CRATES: &[&str] = &[
    "bevy",
    "bevy_a11y",
    "bevy_android",
    "bevy_animation",
    "bevy_anti_alias",
    "bevy_app",
    "bevy_asset",
    "bevy_audio",
    "bevy_camera",
    "bevy_camera_controller",
    "bevy_clipboard",
    "bevy_color",
    "bevy_core_pipeline",
    "bevy_curve",
    "bevy_derive",
    "bevy_dev_tools",
    "bevy_diagnostic",
    "bevy_ecs",
    "bevy_extract",
    "bevy_feathers",
    "bevy_gilrs",
    "bevy_gizmos",
    "bevy_gizmos_render",
    "bevy_gltf",
    "bevy_image",
    "bevy_input",
    "bevy_input_focus",
    "bevy_internal",
    "bevy_light",
    "bevy_log",
    "bevy_material",
    "bevy_math",
    "bevy_mesh",
    "bevy_mikktspace",
    "bevy_pbr",
    "bevy_picking",
    "bevy_platform",
    "bevy_post_process",
    "bevy_ptr",
    "bevy_reflect",
    "bevy_remote",
    "bevy_render",
    "bevy_scene",
    "bevy_shader",
    "bevy_shape",
    "bevy_solari",
    "bevy_sprite",
    "bevy_sprite_render",
    "bevy_state",
    "bevy_tasks",
    "bevy_text",
    "bevy_time",
    "bevy_transform",
    "bevy_ui",
    "bevy_ui_render",
    "bevy_ui_widgets",
    "bevy_utils",
    "bevy_window",
    "bevy_winit",
    "bevy_world_serialization",
];

/// Components the engine maintains and nobody authors, by type path: every one in Bevy 0.20's
/// default plugins that a system writes and no user sets. A path the registry does not know
/// fails the bare-engine test. Unreflected markers (the viewport's `EditorOnly`, `PreviewOnly`)
/// never reach `bevy-ts/schema` and need no entry.
pub const INTERNAL: &[&str] = &[
    // Hierarchy and bookkeeping.
    "bevy_ecs::hierarchy::Children",
    "bevy_ecs::observer::distributed_storage::ObservedBy",
    "bevy_ecs::resource::IsResource",
    "bevy_extract::sync_world::MainEntity",
    // Transform propagation.
    "bevy_transform::components::global_transform::GlobalTransform",
    "bevy_transform::components::transform::TransformTreeChanged",
    // Visibility and culling caches.
    "bevy_camera::visibility::InheritedVisibility",
    "bevy_camera::visibility::ViewVisibility",
    "bevy_camera::visibility::VisibilityClass",
    "bevy_camera::visibility::VisibleEntities",
    "bevy_camera::visibility::VisibleMeshEntities",
    "bevy_camera::visibility::CubemapVisibleEntities",
    "bevy_camera::visibility::CascadesVisibleEntities",
    "bevy_render::view::visibility::RenderShadowMapVisibleEntities",
    "bevy_camera::primitives::Aabb",
    "bevy_camera::primitives::Frustum",
    "bevy_camera::primitives::CubemapFrusta",
    "bevy_camera::primitives::CascadesFrusta",
    "bevy_light::cascade::Cascades",
    // Meshes, animation and gizmos.
    "bevy_mesh::morph::MeshMorphWeights",
    "bevy_animation::AnimatedBy",
    "bevy_sprite_render::tilemap_chunk::TilemapChunkMeshCache",
    "bevy_gizmos::transform_gizmo::TransformGizmoFocus",
    // Picking and windows.
    "bevy_picking::hover::PickingInteraction",
    "bevy_picking::pointer::PointerInteraction",
    "bevy_window::monitor::Monitor",
    "bevy_window::monitor::PrimaryMonitor",
    // UI layout and text.
    "bevy_ui::ui_node::ComputedNode",
    "bevy_ui::ui_node::ComputedUiTargetCamera",
    "bevy_ui::ui_node::ComputedUiRenderTargetInfo",
    "bevy_ui::ui_node::CalculatedClip",
    "bevy_ui::ui_transform::UiGlobalTransform",
    "bevy_ui::stack::ComputedStackIndex",
    "bevy_ui::measurement::ContentSize",
    "bevy_ui::widget::image::ImageNodeSize",
    "bevy_ui::widget::text::TextNodeFlags",
    "bevy_ui_widgets::slider::SliderDragState",
    "bevy_ui_widgets::scrollbar::ScrollbarDragState",
    "bevy_text::text::ComputedTextBlock",
    "bevy_text::pipeline::TextLayoutInfo",
    // bevy-ts's own markers.
    "bevy_ts::ScriptHidden",
    "bevy_ts::commands::TsSpawned",
    // A level's placeholder entries: `level_node.missing` draws them, not a component group.
    "bevy_ts_levels::engine::MissingComponents",
];

/// Icon categories: a key with `::` is a type path prefix and wins over a crate key.
pub const CATEGORIES: &[(&str, Category)] = &[
    ("bevy_camera::visibility::", Category::Render),
    ("bevy_camera::primitives::", Category::Render),
    ("bevy_transform", Category::Transform),
    ("bevy_light", Category::Light),
    ("bevy_camera", Category::Camera),
    ("bevy_core_pipeline", Category::Camera),
    ("bevy_anti_alias", Category::Camera),
    ("bevy_post_process", Category::Camera),
    ("bevy_audio", Category::Audio),
    ("bevy_ui", Category::Ui),
    ("bevy_ui_render", Category::Ui),
    ("bevy_ui_widgets", Category::Ui),
    ("bevy_text", Category::Ui),
    ("bevy_feathers", Category::Ui),
    ("bevy_animation", Category::Animation),
    ("bevy_render", Category::Render),
    ("bevy_pbr", Category::Render),
    ("bevy_sprite", Category::Render),
    ("bevy_sprite_render", Category::Render),
    ("bevy_mesh", Category::Render),
    ("bevy_material", Category::Render),
    ("bevy_gltf", Category::Render),
    ("bevy_rapier3d", Category::Physics),
    ("bevy_ts_rapier3d", Category::Physics),
];

/// Whose a type from `krate` is. `native` is the project's native crate name (`-` as `_`).
pub fn origin(krate: &str, native: Option<&str>) -> Origin {
    if native == Some(krate) {
        Origin::Game
    } else if BEVY_CRATES.contains(&krate) || krate == "bevy_ts" || krate.starts_with("bevy_ts_") {
        Origin::Engine
    } else {
        Origin::Plugin
    }
}

/// The icon category of a non-game type.
pub fn category(type_path: &str, krate: &str) -> Category {
    let by_path = CATEGORIES.iter().find(|(k, _)| k.contains("::") && type_path.starts_with(k));
    by_path.or_else(|| CATEGORIES.iter().find(|(k, _)| *k == krate)).map_or(Category::Other, |(_, c)| *c)
}

/// The native crate's Rust name, from `EngineIdentity`.
pub fn native_crate(world: &World) -> Option<String> {
    let id = world.get_resource::<bevy_ts::engine::EngineIdentity>()?;
    id.native.as_ref().map(|n| n.name.replace('-', "_"))
}

/// A reflected Rust component's editor info.
pub fn reflected(reg: &TypeRegistration, world: &World, native: Option<&str>) -> EditorInfo {
    let table = reg.type_info().type_path_table();
    let (path, krate) = (table.path(), table.crate_name().unwrap_or_default());
    let origin = origin(krate, native);
    let internal = origin == Origin::Engine && INTERNAL.contains(&path);
    let mutable = world
        .components()
        .get_valid_id(reg.type_id())
        .and_then(|id| world.components().get_info(id))
        .is_some_and(|i| i.mutable());
    let constructible = reg.data::<ReflectDefault>().is_some() || reg.data::<ReflectFromWorld>().is_some();
    // A resource is a component in Bevy 0.20, but never one an entity is given.
    let resource = reg.data::<ReflectResource>().is_some();
    let addable = reg.data::<ReflectComponent>().is_some() && !resource && constructible && mutable && !internal;
    let category = if origin == Origin::Game { Category::Game } else { category(path, krate) };
    EditorInfo { origin, internal, addable, category }
}

/// A TS component's editor info: the game's, addable while its live generation declares it.
pub fn ts(c: &TsComponent) -> EditorInfo {
    EditorInfo { origin: Origin::Game, internal: false, addable: c.state == ComponentState::Live, category: Category::Game }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_by_crate_not_prefix() {
        assert_eq!(origin("bevy_rapier3d", None), Origin::Plugin);
        assert_eq!(category("bevy_rapier3d::dynamics::rigid_body::RigidBody", "bevy_rapier3d"), Category::Physics);
        assert_eq!(origin("bevy_ts_rapier3d", None), Origin::Engine);
        assert_eq!(origin("bevy_ts", None), Origin::Engine);
        assert_eq!(origin("bevy_tsunami", None), Origin::Plugin);
        assert_eq!(origin("bevy_transform", None), Origin::Engine);
        assert_eq!(origin("native", Some("native")), Origin::Game);
        assert_eq!(origin("avian3d", Some("native")), Origin::Plugin);
        assert_eq!(category("avian3d::RigidBody", "avian3d"), Category::Other);
        assert_eq!(category("bevy_camera::visibility::Visibility", "bevy_camera"), Category::Render);
        assert_eq!(category("bevy_camera::camera::Camera", "bevy_camera"), Category::Camera);
    }
}
