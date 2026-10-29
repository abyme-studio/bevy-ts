import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

// Line 6 returns a u32 where a vec4 goes.
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return 1u;
}
