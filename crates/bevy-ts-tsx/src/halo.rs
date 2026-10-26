//! `textHalo` on `<text>`: the text's laid-out glyphs drawn again in the halo's colour at 8 offsets
//! around them (16 above 2 px), under the glyphs, as Bevy draws `TextShadow` at one offset
//! (`bevy_ui_render`'s `extract_text_shadows`). No extra layout, entity or shaping.
//!
//! Bevy extracts UI by retention: only nodes in its own change filter are extracted again, and that
//! filter cannot name `TextHalo`. So a change to `TextHalo` marks the text's `TextColor` changed,
//! which is in the filter and is not a layout input.

use bevy::camera::visibility::InheritedVisibility;
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::sync_world::MainEntity;
use bevy::render::{Extract, ExtractSchedule, RenderApp};
use bevy::text::{ComputedTextBlock, EditableText, PositionedGlyph, TextLayoutInfo};
use bevy::ui::{CalculatedClip, ComputedStackIndex, ComputedUiTargetCamera, UiGlobalTransform};
use bevy::ui_render::{ExtractedGlyph, ExtractedUiItem, ExtractedUiNode, ExtractedUiNodes, RenderUiSystems, UiCameraMap, stack_z_offsets};

/// A halo around a `<text>`'s glyphs: `width` logical pixels, in `color`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct TextHalo {
    pub width: f32,
    pub color: Color,
}

impl Default for TextHalo {
    fn default() -> Self {
        TextHalo { width: 1.0, color: Color::BLACK }
    }
}

/// Under the text (`TEXT`, 0.06), over a selection (`TEXT_SELECTION`, 0.055).
const HALO_Z: f32 = 0.058;
const _: () = assert!(HALO_Z > stack_z_offsets::TEXT_SELECTION && HALO_Z < stack_z_offsets::TEXT);

pub(crate) fn plugin(app: &mut App) {
    // In `Last`, after every write of the frame and before extraction.
    app.register_type::<TextHalo>().add_systems(Last, mark_changed);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.add_systems(ExtractSchedule, extract_text_halos.in_set(RenderUiSystems::ExtractTextShadows));
    }
}

/// A changed or removed halo marks its text's `TextColor` changed, so Bevy extracts it again.
fn mark_changed(mut changed: Query<&mut TextColor, (With<TextHalo>, Changed<TextHalo>)>, mut removed: RemovedComponents<TextHalo>, mut colors: Query<&mut TextColor, Without<TextHalo>>) {
    for mut c in &mut changed {
        c.set_changed();
    }
    for e in removed.read() {
        if let Ok(mut c) = colors.get_mut(e) {
            c.set_changed();
        }
    }
}

/// The unit offsets a halo's glyphs are drawn at: every 45°, or every 22.5° above 2 px, where 8
/// leave gaps at the diagonals.
fn offsets(width: f32) -> impl Iterator<Item = Vec2> {
    let n = if width > 2.0 { 16 } else { 8 };
    (0..n).map(move |i| Vec2::from_angle(i as f32 * std::f32::consts::TAU / n as f32))
}

/// Bevy's `calculate_text_scroll_clip` (crate-private in `bevy_ui_render` 0.20).
fn text_clip(editable: Option<&EditableText>, clip: Option<&CalculatedClip>, node: &ComputedNode, transform: &UiGlobalTransform) -> Option<CalculatedClip> {
    if editable.is_some() {
        Some(clip.cloned().unwrap_or_default().with_rect(node.content_box(), transform))
    } else {
        clip.cloned()
    }
}

/// Every text with a halo that Bevy extracts again this frame: its glyphs at each offset. Text
/// without a halo is never visited.
#[allow(clippy::type_complexity)]
fn extract_text_halos(
    mut commands: Commands,
    extracted: ResMut<ExtractedUiNodes>,
    halos: Extract<
        Query<(
            Entity,
            &ComputedNode,
            &ComputedStackIndex,
            &UiGlobalTransform,
            &ComputedUiTargetCamera,
            &InheritedVisibility,
            Option<&CalculatedClip>,
            &TextLayoutInfo,
            &TextHalo,
            Option<&EditableText>,
        ), With<ComputedTextBlock>>,
    >,
    camera_map: Extract<UiCameraMap>,
) {
    let extracted = extracted.into_inner();
    let mut camera_mapper = camera_map.get_mapper();
    let mut glyphs = vec![];
    for (entity, node, stack, transform, target, visible, clip, layout, halo, editable) in &halos {
        if !extracted.changed.contains(&MainEntity::from(entity)) || !visible.get() || node.is_empty() {
            continue;
        }
        let Some(camera) = camera_mapper.map(target) else {
            continue;
        };
        let clip = text_clip(editable, clip, node, transform);
        let scroll = editable.map_or(Vec2::ZERO, |t| t.viewport.offset);
        let color: LinearRgba = halo.color.into();
        for offset in offsets(halo.width) {
            let at = Affine2::from(*transform) * Affine2::from_translation(node.content_box().min + offset * halo.width / node.inverse_scale_factor() - scroll);
            for (i, PositionedGlyph { position, atlas_info, section_index, .. }) in layout.glyphs.iter().enumerate() {
                glyphs.push(ExtractedGlyph { color, translation: *position, rect: atlas_info.rect });
                // One extracted node per run of glyphs on one atlas texture, as Bevy's shadow.
                if layout.glyphs.get(i + 1).is_none_or(|g| g.section_index != *section_index || g.atlas_info.texture != atlas_info.texture) {
                    extracted.uinodes.entry(MainEntity::from(entity)).or_insert_with(|| (camera, Default::default())).1.insert(
                        commands.spawn_empty().id(),
                        ExtractedUiNode {
                            transform: at,
                            z_order: stack.0 as f32 + HALO_Z,
                            image: atlas_info.texture,
                            clip: clip.clone(),
                            item: ExtractedUiItem::Glyphs { glyphs: std::mem::take(&mut glyphs) },
                        },
                    );
                }
            }
        }
    }
}
