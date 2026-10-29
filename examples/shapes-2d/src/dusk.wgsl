// shapes::Dusk: the image darkened and warmed toward `tint` by `amount`, 0 to 1. Reads each
// pixel whole (the screen sampler is nearest), so the pixel art stays crisp.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(screen, screen_sampler, in.uv);
    let graded = c.rgb * params.tint.rgb * (1.0 - 0.5 * params.amount);
    return vec4<f32>(mix(c.rgb, graded, params.amount), c.a);
}
