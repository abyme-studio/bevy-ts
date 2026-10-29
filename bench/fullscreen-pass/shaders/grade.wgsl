// A dusk-like grade: the screen read once, darkened and warmed by a time-driven amount.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(screen, screen_sampler, in.uv);
    let k = params.amount * (0.5 + 0.5 * sin(params.time));
    let graded = c.rgb * params.tint.rgb * (1.0 - 0.5 * k);
    return vec4<f32>(mix(c.rgb, graded, k), c.a);
}
