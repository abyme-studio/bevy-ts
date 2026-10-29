// shapes::Sea and shapes::Land, one layer each of a pixel-art planet: a disc of `params.color`
// where a hashed noise of the art pixel reaches `params.cutoff` (0 draws the whole disc). The art
// pixel is `floor(in.uv * pixels)`, `pixels` the quad's size in world units, so on a quad placed
// on whole units each art pixel is one whole block of screen pixels at any whole zoom.
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

fn hash(x: u32) -> u32 {
    let s = x * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let me = instance(in.instance_index);
    let px = floor(in.uv * me.pixels);
    let d = distance(px + 0.5, vec2<f32>(me.pixels * 0.5)) / (me.pixels * 0.5);
    // Coarse noise: 4x4 art-pixel cells, so land comes in continents rather than speckle.
    let cell = vec2<u32>(floor(px / 4.0));
    let n = f32(hash((cell.x * 73856093u) ^ (cell.y * 19349663u) ^ u32(me.seed)) >> 24u) / 255.0;
    let shade = 0.85 + 0.15 * f32(hash(u32(px.x) * 31u + u32(px.y)) >> 31u);
    let on = step(d, 1.0) * step(params.cutoff, n);
    return vec4<f32>(params.color.rgb * shade, on);
}
