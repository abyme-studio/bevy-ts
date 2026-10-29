import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

// Mixes the screen toward params.tint by params.amount.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(screen, screen_sampler, in.uv);
    return vec4<f32>(mix(c.rgb, c.rgb * params.tint.rgb, params.amount), c.a);
}
