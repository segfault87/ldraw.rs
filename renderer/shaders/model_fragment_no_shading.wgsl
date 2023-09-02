struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) viewPosition: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
