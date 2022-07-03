struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
}

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
    @location(1) texture_atlas_offset: vec2<f32>,
    @location(2) color_adjust: vec4<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) texture_atlas_offset: vec2<f32>,
    @location(10) color_adjust: vec4<f32>,
}

@group(1) @binding(0)
var<uniform> camera_position: CameraUniform;

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
    @builtin(vertex_index) vertex_idx: u32,
) -> VertexOutput {
    var model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    var out: VertexOutput;
    out.tex_coord = model.tex_coord;
    out.position = camera_position.view_proj * model_matrix * model.position;
    out.texture_atlas_offset = instance.texture_atlas_offset;
    out.color_adjust = instance.color_adjust;
    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    var unit_offset: f32 = 1.0 / 32.0;
    var atlas_scaled_coords = vertex.tex_coord / 32.0;
    var offset_coords = atlas_scaled_coords + (unit_offset * vertex.texture_atlas_offset);

    return textureSample(t_diffuse, s_diffuse, offset_coords) * vertex.color_adjust;
}

@fragment
fn fs_wire(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}