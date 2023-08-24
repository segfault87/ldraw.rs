struct ProjectionData {
    model_matrix: mat4x4<f32>,
    projection_matrix: mat4x4<f32>,
    model_view_matrix: mat4x4<f32>,
    normal_matrix: mat3x3<f32>,
    view_matrix: mat4x4<f32>,
    is_orthographic: i32,
}

@group(0) @binding(0)
var<uniform> projection: ProjectionData;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) control_1: vec3<f32>,
    @location(2) control_2: vec3<f32>,
    @location(3) direction: vec3<f32>,
    @location(4) color: vec3<f32>,
}
struct InstanceInput {
    @location(10) model_matrix_0: vec4<f32>,
    @location(11) model_matrix_1: vec4<f32>,
    @location(12) model_matrix_2: vec4<f32>,
    @location(13) model_matrix_3: vec4<f32>,
    @location(14) instance_color: vec4<f32>,
    @location(15) instance_edge_color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) discard_flag: i32,
}

@vertex
fn vs(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    let mvp = projection.projection_matrix * projection.model_view_matrix * model_matrix;

    var c1 = mvp * vec4<f32>(vertex.control_1, 1.0);
    var c2 = mvp * vec4<f32>(vertex.control_2, 1.0);
    var p1 = mvp * vec4<f32>(vertex.position, 1.0);
    var p2 = mvp * vec4<f32>(vertex.position + vertex.direction, 1.0);
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
    let c1_dir = c1.xy - p2.xy;
    let c2_dir = c2.xy - p2.xy;
    let d0 = dot(normalize(norm), normalize(c1_dir));
    let d1 = dot(normalize(norm), normalize(c2_dir));

    out.discard_flag = select(0, 1, sign(d0) != sign(d1));

    var mv_position = vec4<f32>(vertex.position, 1.0);
    mv_position = model_matrix * mv_position;

    var color = instance.instance_color;
    var edge_color = instance.instance_edge_color;

    if (vertex.color.x < -1.0) {
        out.color = edge_color;
    } else if (vertex.color.x < 0.0) {
        out.color = color;
    } else {
        out.color = vec4<f32>(vertex.color, 1.0);
    }

    mv_position = projection.model_view_matrix * mv_position;
    out.position = projection.projection_matrix * mv_position;

    return out;
}
