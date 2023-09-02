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
    @location(14) instanceColor: vec4<f32>,
    @location(15) instanceEdgeColor: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) viewPosition: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
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
    var transformedNormal = vertex.normal;
    transformedNormal /= vec3<f32>(
        dot(instanceNormalMatrix[0], instanceNormalMatrix[0]),
        dot(instanceNormalMatrix[1], instanceNormalMatrix[1]),
        dot(instanceNormalMatrix[2], instanceNormalMatrix[2])
    );
    transformedNormal = instanceNormalMatrix * transformedNormal;

    var mvPosition = vec4<f32>(vertex.position, 1.0);
    mvPosition = projection.viewMatrix * projection.modelMatrix * instanceModelMatrix * mvPosition;

    if (vertex.color.x < -1.0) {
        out.color = instance.instanceEdgeColor;
    } else if (vertex.color.x < 0.0) {
        out.color = instance.instanceColor;
    } else {
        out.color = vertex.color;
    }
    out.normal = normalize(projection.normalMatrix * transformedNormal);
    out.normal.y *= -1.0;
    out.viewPosition = -mvPosition.xyz;
    out.position = projection.projectionMatrix * mvPosition;

    return out;
}
