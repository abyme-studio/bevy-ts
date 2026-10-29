import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

// The frame fields as colour: red is texel / 4, green size.x / 1000, blue view.x / 100.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(params.texel / 4.0, params.size.x / 1000.0, params.view.x / 100.0, 1.0 + params.time * 0.0);
}
