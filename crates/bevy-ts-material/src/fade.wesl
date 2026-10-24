// `FadeMaterial` (alpha.rs): a `StandardMaterial` that draws about `fade.alpha` of its pixels,
// chosen by a 4×4 ordered (Bayer) map in screen space, as Bevy's `VisibilityRange` cross-fade
// does. The main pass is Bevy's `pbr.wesl` with the discard; the prepass, which the shadow pass
// runs, discards the same pixels, so the shadow follows `alpha`.

import bevy_pbr::render::pbr_types;

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    render::pbr_prepass_functions::prepass_sample_color_and_alpha_discard,
};
@if(PREPASS_PIPELINE && MOTION_VECTOR_PREPASS)
import bevy_pbr::render::pbr_prepass_functions::calculate_motion_vector;
@if(!PREPASS_PIPELINE)
import bevy_pbr::{
    render::{
        forward_io::{VertexOutput, FragmentOutput},
        pbr_fragment::pbr_input_from_standard_material,
        pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    },
    decal::clustered::apply_decals,
};
@if(!PREPASS_PIPELINE && VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

struct Fade {
    alpha: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> fade: Fade;

// The threshold of the pixel at `p`: (i + 0.5) / 16 for its index i in the 4×4 Bayer map, so
// `alpha` 1 keeps every pixel, 0 none, and k/16 exactly k of every 16.
fn bayer4(p: vec2<f32>) -> f32 {
    let c = vec2<u32>(floor(p)) % 4u;
    var m = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    return (f32(m[c.y * 4u + c.x]) + 0.5) / 16.0;
}

fn dither(frag_coord: vec4<f32>) {
    if bayer4(frag_coord.xy) > fade.alpha {
        discard;
    }
}

// Main pass. A blend-mode base was faded through its `base_color` alpha (alpha.rs), so it does
// not discard here.
@if(!PREPASS_PIPELINE)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    let blends = mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND
        || mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED
        || mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD
        || mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MULTIPLY;
    if !blends {
        dither(in.position);
    }

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );
    apply_decals(&pbr_input);

    var out: FragmentOutput;
    if (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// Prepass and shadow pass with outputs (a normal or motion-vector prepass, or emulated unclipped
// depth). The normal is the mesh's, without a normal map.
@if(PREPASS_PIPELINE && PREPASS_FRAGMENT)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    dither(in.position);
    prepass_sample_color_and_alpha_discard(in);

    var out: FragmentOutput;
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;
@if(NORMAL_PREPASS)
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
@if(MOTION_VECTOR_PREPASS)
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
    return out;
}

// Prepass and shadow pass, depth only. Bevy adds no fragment stage to an opaque base's depth-only
// pipeline; `Fade::specialize` adds this one.
@if(PREPASS_PIPELINE && !PREPASS_FRAGMENT)
@fragment
fn fragment(in: VertexOutput) {
    dither(in.position);
    prepass_sample_color_and_alpha_discard(in);
}
