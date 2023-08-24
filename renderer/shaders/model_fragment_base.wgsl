struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) view_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

fn linear_to_srgb(x: f32) -> f32 {
    if (x <= 0.00031308) {
        return 12.92 * x;
    } else {
        return 1.055*pow(x, (1.0 / 2.4)) - 0.055;
    }
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
