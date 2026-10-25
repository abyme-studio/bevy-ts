// `WorldTextMaterial` (world_text/material.rs): a label's glyphs from the shared distance-field
// atlas. The distance is antialiased by its screen-space derivative, so an edge stays sharp close up
// and steady far away; an outline is a second threshold on the same distance. The fill is the
// vertex colour. Lit labels run Bevy's PBR lighting on the result, unlit ones return it. The
// prepass drops the same pixels the main pass does, so the space between glyphs hides nothing.

import bevy_pbr::render::pbr_types;

@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{VertexOutput, FragmentOutput};
@if(PREPASS_PIPELINE && MOTION_VECTOR_PREPASS)
import bevy_pbr::render::pbr_prepass_functions::calculate_motion_vector;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
};

struct WorldTextParams {
    outline_color: vec4<f32>,
    outline: f32,
    spread: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> params: WorldTextParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var atlas: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var atlas_sampler: sampler;

// The signed distance at `uv` (atlas texels) in raster pixels, positive inside the glyph.
fn signed_distance(uv: vec2<f32>) -> f32 {
    let t = textureSample(atlas, atlas_sampler, uv / vec2<f32>(textureDimensions(atlas))).r;
    return (t - 0.5) * 2.0 * params.spread;
}

// How much of this pixel lies inside the edge at `d`: a one-pixel ramp in screen space.
fn coverage(d: f32, w: f32) -> f32 {
    return clamp(d / w + 0.5, 0.0, 1.0);
}

// The fill's and the outline's coverage at `o` pixels from the centre of the pixel at `uv`.
fn tap(uv: vec2<f32>, o: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, w: f32) -> vec2<f32> {
    let d = signed_distance(uv + o.x * dx + o.y * dy);
    return vec2(coverage(d, w), coverage(d + params.outline, w));
}

// Main pass.
@if(!PREPASS_PIPELINE)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let fill = pbr_input.material.base_color;
    // Four taps on a rotated grid, each ramping over half a pixel: far away, where a stroke is
    // thinner than a pixel, one tap at the centre misjudges how much of the pixel it covers, and
    // the label shimmers as it moves.
    let w = max(fwidth(signed_distance(in.uv)), 1e-4) * 0.5;
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let c = 0.25 * (tap(in.uv, vec2(0.125, 0.375), dx, dy, w) + tap(in.uv, vec2(-0.375, 0.125), dx, dy, w)
        + tap(in.uv, vec2(0.375, -0.125), dx, dy, w) + tap(in.uv, vec2(-0.125, -0.375), dx, dy, w));
    let inner = c.x;
    let outer = c.y;
    var rgb = fill.rgb;
    var a = inner * fill.a;
    if params.outline > 0.0 {
        rgb = mix(params.outline_color.rgb, fill.rgb, inner);
        a = outer * mix(params.outline_color.a, fill.a, inner);
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, vec4(rgb, a));

    var out: FragmentOutput;
    if (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// Below half coverage of the outer edge: dropped, as the main pass drops it without MSAA.
@if(PREPASS_PIPELINE)
fn drop_outside(in: VertexOutput) {
@if(VERTEX_UVS_A)
    if signed_distance(in.uv) + params.outline < 0.0 {
        discard;
    }
}

// Prepass with outputs (a normal or motion-vector prepass, or emulated unclipped depth).
@if(PREPASS_PIPELINE && PREPASS_FRAGMENT)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    drop_outside(in);
    var out: FragmentOutput;
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;
@if(NORMAL_PREPASS)
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
@if(MOTION_VECTOR_PREPASS)
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
    return out;
}

// Prepass, depth only. `WorldTextExt::specialize` adds this stage when Bevy gives none.
@if(PREPASS_PIPELINE && !PREPASS_FRAGMENT)
@fragment
fn fragment(in: VertexOutput) {
    drop_outside(in);
}
