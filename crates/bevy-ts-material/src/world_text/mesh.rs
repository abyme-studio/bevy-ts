//! Glyph quads to one `Mesh` per label (design 7): position, a +Z normal, UVs in atlas texels,
//! the fill colour per vertex, `u32` indices. Each quad is built from its glyph and a transform,
//! so text on a path (stage 2) turns each glyph without new mesh code.

use bevy::asset::RenderAssetUsages;
use bevy::math::Affine3A;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// One glyph's quad: its rectangle in its own plane (local units, Y up), its cell in the atlas in
/// texels (`min` the top-left), its fill in linear RGBA.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    pub rect: Rect,
    pub uv: Rect,
    pub color: [f32; 4],
}

/// The quads, each placed by its transform, as one mesh drawn only by the render world.
pub fn mesh(quads: impl IntoIterator<Item = (GlyphQuad, Affine3A)>) -> Mesh {
    let (mut pos, mut normal, mut uv, mut color, mut index) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (q, t) in quads {
        let base = pos.len() as u32;
        let (r, u) = (q.rect, q.uv);
        // Bottom left, bottom right, top right, top left: counter-clockwise seen from +Z. A texel's
        // row grows downward, so the bottom corners take the cell's larger row.
        let corners = [(r.min, Vec2::new(u.min.x, u.max.y)), (Vec2::new(r.max.x, r.min.y), u.max), (r.max, Vec2::new(u.max.x, u.min.y)), (Vec2::new(r.min.x, r.max.y), u.min)];
        let n: [f32; 3] = t.transform_vector3(Vec3::Z).normalize_or(Vec3::Z).into();
        for (p, c) in corners {
            pos.push(<[f32; 3]>::from(t.transform_point3(p.extend(0.0))));
            normal.push(n);
            uv.push(<[f32; 2]>::from(c));
            color.push(q.color);
        }
        index.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normal)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(index))
}
