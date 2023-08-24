struct VertexOutput {
    @location(0) color: vec4<f32>,
    @location(1) discard_flag: i32,
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.discard_flag == 1) {
        discard;
    }

    return in.color;
}
