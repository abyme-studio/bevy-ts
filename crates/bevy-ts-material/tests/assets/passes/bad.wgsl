import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

// Line 6 returns a u32 where a vec4 goes.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return 1u;
}
