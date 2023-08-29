struct VertexOutput {
    @location(0) color: vec4<f32>,
    @location(1) discardFlag: i32,
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.discardFlag == 1) {
        discard;
    }

    return in.color;
}
