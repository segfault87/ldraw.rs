struct ProjectionData {
    model_matrix: mat4x4<f32>,
    projection_matrix: mat4x4<f32>,
    model_view_matrix: mat4x4<f32>,
    normal_matrix: mat3x3<f32>,
    view_matrix: mat4x4<f32>,
    is_orthographic: i32,
}

@group(1) @binding(0)
var<uniform> projection: ProjectionData;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
}
struct InstanceInput {
    @location(2) model_matrix_0: vec4<f32>,
    @location(3) model_matrix_1: vec4<f32>,
    @location(4) model_matrix_2: vec4<f32>,
    @location(5) model_matrix_3: vec4<f32>,
    @location(6) color: vec4<f32>,
    @location(7) edge_color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
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

    var mv_position = vec4<f32>(vertex.position, 1.0);
    mv_position = model_matrix * mv_position;

    if (vertex.color.x < -1.0) {
        out.color = instance.edge_color;
    } else if (vertex.color.x < 0.0) {
        out.color = instance.color;
    } else {
        out.color = vec4<f32>(vertex.color, 1.0);
    }

    mv_position = projection.model_view_matrix * mv_position;
    out.position = projection.projection_matrix * mv_position;

    return out;
}
