import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return textureSample(screen, screen_sampler, in.uv);
}
