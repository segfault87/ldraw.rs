struct ProjectionData {
    model_matrix: mat4x4<f32>,
    projection_matrix: mat4x4<f32>,
    model_view_matrix: mat4x4<f32>,
    normal_matrix: mat3x3<f32>,
    view_matrix: mat4x4<f32>,
    is_orthographic: i32,
}

struct ColorUniforms {
    color: vec4<f32>,
    edge_color: vec4<f32>,
    use_instance_colors: i32,
}

@group(0) @binding(0)
var<uniform> projection: ProjectionData;

@group(1) @binding(0)
var<uniform> color_uniforms: ColorUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}
struct InstanceInput {
    @location(10) model_matrix_0: vec4<f32>,
    @location(11) model_matrix_1: vec4<f32>,
    @location(12) model_matrix_2: vec4<f32>,
    @location(13) model_matrix_3: vec4<f32>,
    @location(14) color: vec4<f32>,
    @location(15) edge_color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) view_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
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

    let model_normal_matrix = mat3x3<f32>(
        model_matrix[0].xyz,
        model_matrix[1].xyz,
        model_matrix[2].xyz,
    );
    var transformed_normal = vertex.normal;
    transformed_normal /= vec3<f32>(
        dot(model_normal_matrix[0], model_normal_matrix[0]),
        dot(model_normal_matrix[1], model_normal_matrix[1]),
        dot(model_normal_matrix[2], model_normal_matrix[2])
    );
    transformed_normal = model_normal_matrix * transformed_normal;

    mv_position = projection.model_view_matrix * mv_position;

    out.color = select(color_uniforms.color, instance.color, color_uniforms.use_instance_colors == 1);
    out.normal = normalize(projection.normal_matrix * transformed_normal);
    out.normal.y *= -1.0;
    out.view_position = -mv_position.xyz;
    out.position = projection.projection_matrix * mv_position;

    return out;
}
