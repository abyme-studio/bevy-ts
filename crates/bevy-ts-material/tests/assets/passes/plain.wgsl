// No imports: checks with no renderer's modules loaded.
@fragment
fn fragment(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(screen, screen_sampler, uv) * params.amount;
}
