import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

// An integer hash (PCG), about the cost of one of PixelPlanets' noise hashes, with no `sin`.
fn hash(x: u32) -> u32 {
    let s = x * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

// `n` chained hashes of an art pixel, 0 to 1.
fn noise(p: vec2<f32>, seed: f32, n: u32) -> f32 {
    var h = bitcast<u32>(seed) ^ (u32(p.x) * 73856093u) ^ (u32(p.y) * 19349663u);
    var acc = 0u;
    for (var i = 0u; i < n; i++) {
        h = hash(h + i);
        acc += h >> 24u;
    }
    return f32(acc) / (255.0 * f32(n));
}

// The clouds, about 17 hashes a pixel, blended.
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let me = instance(in.instance_index);
    let px = floor(in.uv * me.pixels);
    let d = distance(px + 0.5, vec2<f32>(me.pixels * 0.5)) / (me.pixels * 0.5);
    let n = noise(px, me.seed + 2.0, 17u);
    return vec4<f32>(params.tint.rgb, step(0.6, n) * step(d, 1.0));
}

