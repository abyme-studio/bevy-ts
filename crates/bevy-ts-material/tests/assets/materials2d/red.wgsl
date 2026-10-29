import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
