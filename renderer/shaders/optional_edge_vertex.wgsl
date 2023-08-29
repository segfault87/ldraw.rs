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
    @location(1) control1: vec3<f32>,
    @location(2) control2: vec3<f32>,
    @location(3) direction: vec3<f32>,
    @location(4) color: vec3<f32>,
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
    @location(0) color: vec4<f32>,
    @location(1) discardFlag: i32,
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

    let mvMatrix = projection.projectionMatrix * projection.viewMatrix * projection.modelMatrix * instanceModelMatrix;

    var c1 = mvMatrix * vec4<f32>(vertex.control1, 1.0);
    var c2 = mvMatrix * vec4<f32>(vertex.control2, 1.0);
    var p1 = mvMatrix * vec4<f32>(vertex.position, 1.0);
    var p2 = mvMatrix * vec4<f32>(vertex.position + vertex.direction, 1.0);
    c1.x /= c1.w;
    c1.y /= c1.w;
    c2.x /= c2.w;
    c2.y /= c2.w;
    p1.x /= p1.w;
    p1.y /= p1.w;
    p2.x /= p2.w;
    p2.y /= p2.w;

    let dir = p2.xy - p1.xy;
    let norm = vec2<f32>(-dir.y, dir.x);
    let c1Dir = c1.xy - p2.xy;
    let c2Dir = c2.xy - p2.xy;
    let d0 = dot(normalize(norm), normalize(c1Dir));
    let d1 = dot(normalize(norm), normalize(c2Dir));

    out.discardFlag = select(0, 1, sign(d0) != sign(d1));

    var color = instance.instanceColor;
    var edgeColor = instance.instanceEdgeColor;

    if (vertex.color.x < -1.0) {
        out.color = edgeColor;
    } else if (vertex.color.x < 0.0) {
        out.color = color;
    } else {
        out.color = vec4<f32>(vertex.color, 1.0);
    }

    var mvPosition = vec4<f32>(vertex.position, 1.0);
    mvPosition = mvMatrix * mvPosition;
    out.position = mvPosition;

    return out;
}
