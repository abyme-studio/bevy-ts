import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let s = textureSample(screen, screen_sampler, in.uv);
    return vec4<f32>(1.0 - s.rgb, s.a);
}
