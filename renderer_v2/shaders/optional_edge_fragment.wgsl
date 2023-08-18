struct VertexOutput {
    @location(0) color: vec4<f32>,
}

@fragment 
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
