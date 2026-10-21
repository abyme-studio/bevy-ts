// game::Glow's fragment shader. bevy-ts appends `params` (a GlowParams with color and pulse)
// from the schema in glow.tsx, so this file does not declare it.
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::globals;

const FLOOR: f32 = 0.25;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let wave = 0.5 + 0.5 * sin(globals.time * params.pulse);
    let glow = FLOOR + (1.0 - FLOOR) * wave;
    return vec4<f32>(params.color.rgb * glow, params.color.a);
}
