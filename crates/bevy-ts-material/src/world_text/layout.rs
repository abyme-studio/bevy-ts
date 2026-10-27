//! Labels to glyph quads (design 4): each label whose components changed, or whose font was still
//! loading, is laid out by Bevy's `TextPipeline` at [`RASTER`] pixels and scale 1, so it gets the
//! fonts and fallback `<text>` gets. Lengths are then scaled by `size / RASTER`, Y is flipped
//! (parley's grows down) and the box is moved so `anchor` sits at the origin. A Bevy upgrade that
//! changes the text pipeline changes this file.

use bevy::camera::primitives::Aabb;
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::system::SystemParam;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::text::{
    ComputedTextBlock, FontAtlasSet, FontCx, FontSource, FontHinting, FontSize, LayoutCx, LetterSpacing, LineBreak, LineHeight, RemSize, ScaleCx, TextBounds, TextColor, TextElement, TextError, TextFont, TextLayout,
    TextLayoutInfo, TextPipeline,
};

use super::atlas::WorldTextAtlas;
use super::material::{WorldTextMaterial, WorldTextMaterials};
use super::mesh::{GlyphQuad, mesh};
use super::sdf::SPREAD;
use super::{ComputedWorldText, LaidOut, MAX_OUTLINE, RASTER, WorldText, WorldTextStyle};

/// Bevy's text state the pipeline lays out and rasterises with.
#[derive(SystemParam)]
pub struct TextContexts<'w> {
    fonts: Res<'w, Assets<Font>>,
    font_cx: ResMut<'w, FontCx>,
    layout_cx: ResMut<'w, LayoutCx>,
    scale_cx: ResMut<'w, ScaleCx>,
    font_atlases: ResMut<'w, FontAtlasSet>,
    rem: Res<'w, RemSize>,
    server: Option<Res<'w, AssetServer>>,
}

/// Where a label's quads and material go.
#[derive(SystemParam)]
pub struct Outputs<'w> {
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<WorldTextMaterial>>,
    styles: ResMut<'w, WorldTextMaterials>,
    atlas: ResMut<'w, WorldTextAtlas>,
}

type Label<'a> = (
    Entity,
    Ref<'a, WorldText>,
    Ref<'a, WorldTextStyle>,
    Ref<'a, TextFont>,
    Ref<'a, TextColor>,
    Ref<'a, TextLayout>,
    Ref<'a, LineHeight>,
    &'a mut ComputedTextBlock,
    &'a mut ComputedWorldText,
    Option<&'a Mesh3d>,
    Option<&'a MeshMaterial3d<WorldTextMaterial>>,
);

/// The system's own: labels waiting on a font, and Bevy's pipeline and its output, reused.
#[derive(Default)]
pub struct State {
    retry: EntityHashSet,
    pipeline: TextPipeline,
    info: TextLayoutInfo,
}

/// A label laid out this frame, meshed once the frame's new glyphs are placed: its layout box and
/// each glyph's quad and raster in Bevy's atlas.
struct Laid {
    e: Entity,
    rect: Rect,
    glyphs: Vec<(Rect, AssetId<Image>, URect)>,
}

/// Lays out and meshes every label that changed, and no other.
pub fn lay_out(
    mut labels: Query<Label>,
    mut state: Local<State>,
    mut text: TextContexts,
    mut out: Outputs,
    mut laid: ResMut<LaidOut>,
    mut commands: Commands,
) {
    let State { retry, pipeline, info } = &mut *state;
    let mut count = 0;
    let mut laid_out = Vec::new();
    for (e, label, style, font, color, layout, line_height, mut block, _, _, _) in &mut labels {
        let changed = label.is_changed() || style.is_changed() || font.is_changed() || color.is_changed() || layout.is_changed() || line_height.is_changed();
        let waited = retry.remove(&e);
        if !(changed || waited) {
            continue;
        }
        count += 1;
        let size = style.size;
        if !(size.is_finite() && size > 0.0) {
            error!("world text {e}: size {size} is not positive and finite; the label keeps its last look");
            continue;
        }
        let k = RASTER / size;
        let raster_font = TextFont { font_size: FontSize::Px(RASTER), ..(*font).clone() };
        let line_height = match *line_height {
            LineHeight::Px(h) => LineHeight::Px(h * k),
            h => h,
        };
        let span = TextElement::Text { text: &label.0, font: &raster_font, color: color.0, line_height, letter_spacing: LetterSpacing::Px(style.letter_spacing * RASTER) };
        let width = style.width.filter(|_| layout.linebreak != LineBreak::NoWrap).map(|w| w * k);
        let bounds = TextBounds { width, height: None };
        let result = pipeline
            .update_buffer(&text.fonts, std::iter::once((e, 0, span)), layout.linebreak, layout.justify, bounds, 1.0, &mut block, &mut text.font_cx, &mut text.layout_cx, Vec2::splat(1000.0), *text.rem)
            .and_then(|()| pipeline.update_text_layout_info(info, &mut text.font_atlases, &mut out.images, &mut block, &mut text.scale_cx, bounds, layout.justify, FontHinting::Disabled));
        match result {
            Ok(()) => {}
            // A font still loading: again next frame, until it draws, or until its load fails.
            Err(TextError::NoSuchFont | TextError::NoSuchFontFamily(_)) => {
                let failed = match (&font.font, &text.server) {
                    (FontSource::Handle(h), Some(server)) => server.load_state(h).is_failed(),
                    _ => false,
                };
                if failed {
                    error!("world text {e}: its font failed to load; the label draws nothing until its font changes");
                } else {
                    retry.insert(e);
                }
                continue;
            }
            Err(err) => {
                error!("world text {e}: {err}; the label keeps its last look");
                continue;
            }
        }

        // The layout box, Y up, with the anchor at the origin.
        let b = block.buffer();
        let boxed = Vec2::new(width.unwrap_or(b.full_width()).max(b.full_width()), b.height());
        let origin = (style.anchor.clamp(Vec2::splat(-0.5), Vec2::splat(0.5)) + 0.5) * boxed;
        let local = |p: Vec2| Vec2::new(p.x - origin.x, boxed.y - p.y - origin.y) / k;
        let pad = Vec2::splat(SPREAD as f32);
        let mut glyphs = Vec::with_capacity(info.glyphs.len());
        for g in &info.glyphs {
            let r = g.atlas_info.rect;
            // A space has no ink: no quad.
            if r.width() < 1.0 || r.height() < 1.0 {
                continue;
            }
            let (texture, r) = (g.atlas_info.texture, r.as_urect());
            if !out.atlas.want(&out.images, texture, r) {
                continue;
            }
            let half = r.size().as_vec2() / 2.0 + pad;
            glyphs.push((Rect::from_corners(local(g.position - half), local(g.position + half)), texture, r));
        }
        let rect = Rect::from_corners(local(Vec2::ZERO), local(boxed));
        laid_out.push(Laid { e, rect, glyphs });
    }
    // This frame's new glyphs, in an order of their own: the atlas is the same whichever label
    // went first.
    out.atlas.place(&mut out.images);

    for Laid { e, rect, glyphs } in laid_out {
        let Ok((_, _, style, _, color, _, _, _, mut computed, mesh3d, material3d)) = labels.get_mut(e) else { continue };
        let fill = color.0.to_linear().to_f32_array();
        let quads: Vec<_> = glyphs
            .into_iter()
            .filter_map(|(rect, texture, r)| {
                let cell = out.atlas.cell(texture, r)?;
                let uv = Rect::from_corners(cell.min.as_vec2(), (cell.min + cell.size).as_vec2());
                Some(GlyphQuad { rect, uv, color: fill })
            })
            .collect();
        *computed = ComputedWorldText { rect, quads: quads.len() as u32, mesh: computed.mesh.take() };

        let mut ec = commands.entity(e);
        if quads.is_empty() {
            ec.remove::<(Mesh3d, MeshMaterial3d<WorldTextMaterial>, Aabb)>();
            continue;
        }
        let bounds = quads.iter().fold(rect, |a, q| a.union(q.rect));
        let m = mesh(quads.into_iter().map(|q| (q, Affine3A::IDENTITY)));
        let handle = match computed.mesh.clone() {
            Some(h) => {
                out.meshes.insert(h.id(), m).expect("a mesh this label owns");
                h
            }
            None => {
                let h = out.meshes.add(m);
                computed.mesh = Some(h.clone());
                h
            }
        };
        if mesh3d.is_none_or(|m| m.0 != handle) {
            ec.insert(Mesh3d(handle));
        }
        let outline = style.outline.map(|(w, c)| (w.clamp(0.0, MAX_OUTLINE) * RASTER, c));
        let material = out.styles.get(&mut out.materials, &out.atlas.image, style.lit, outline, SPREAD as f32);
        if material3d.is_none_or(|m| m.0 != material) {
            ec.insert(MeshMaterial3d(material));
        }
        ec.insert(Aabb::from_min_max(bounds.min.extend(0.0), bounds.max.extend(0.0)));
    }
    laid.0 = count;
    // A texture written is a new one on the GPU: the materials that sample it are prepared again.
    if std::mem::take(&mut out.atlas.dirty) {
        let handles: Vec<_> = out.styles.handles().cloned().collect();
        for h in handles {
            let _ = out.materials.get_mut(&h);
        }
    }
}
