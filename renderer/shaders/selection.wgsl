struct ProjectionData {
    modelMatrix: mat4x4<f32>,
    projectionMatrix: mat4x4<f32>,
    viewMatrix: mat4x4<f32>,
    normalMatrix: mat3x3<f32>,
    isOrthographic: i32,
}

@group(0) @binding(0)
var<uniform> projection: ProjectionData;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

struct InstanceInput {
    @location(10) modelMatrix0: vec4<f32>,
    @location(11) modelMatrix1: vec4<f32>,
    @location(12) modelMatrix2: vec4<f32>,
    @location(13) modelMatrix3: vec4<f32>,
    @location(14) instanceId: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) instanceId: vec4<u32>,
}

@vertex
fn vs(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    let instanceModelMatrix = mat4x4<f32>(
        instance.modelMatrix0,
        instance.modelMatrix1,
        instance.modelMatrix2,
        instance.modelMatrix3,
    );

    let instanceNormalMatrix = mat3x3<f32>(
        instanceModelMatrix[0].xyz,
        instanceModelMatrix[1].xyz,
        instanceModelMatrix[2].xyz,
    );
    
    var mvPosition = vec4<f32>(vertex.position, 1.0);
    mvPosition = projection.viewMatrix * projection.modelMatrix * instanceModelMatrix * mvPosition;

    out.instanceId = vec4<u32>(
        instance.instanceId & 0x00ff0000u >> 16u,
        instance.instanceId & 0x0000ff00u >> 8u,
        instance.instanceId & 0x000000ffu,
        instance.instanceId & 0xff000000u >> 24u
    );

    return out;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<u32> {
    return in.instanceId;
}
