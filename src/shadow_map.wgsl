fn mat4_from_quaternion(quat: vec4<f32>) -> mat4x4<f32> {
    let x2 = quat.x + quat.x;
    let y2 = quat.y + quat.y;
    let z2 = quat.z + quat.z;

    let xx2 = x2 * quat.x;
    let xy2 = x2 * quat.y;
    let xz2 = x2 * quat.z;

    let yy2 = y2 * quat.y;
    let yz2 = y2 * quat.z;
    let zz2 = z2 * quat.z;

    let sy2 = y2 * quat.w;
    let sz2 = z2 * quat.w;
    let sx2 = x2 * quat.w;

    return mat4x4<f32>(
        1.0 - yy2 - zz2, xy2 + sz2, xz2 - sy2, 0.0,
        xy2 - sz2, 1.0 - xx2 - zz2, yz2 + sx2, 0.0,
        xz2 + sy2, yz2 - sx2, 1.0 - xx2 - yy2, 0.0,
        0.0, 0.0, 0.0, 1.0,
    );
}

fn mat4_from_position(pos: vec4<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        pos,
    );
}

struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
}

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) clip_position: vec4<f32>,
    @location(1) texture_atlas_offset: vec2<f32>,
}

struct InstanceInput {
    @location(4) instance_position: vec4<f32>,
    @location(5) rotation_quaternion: vec4<f32>,
    @location(9) texture_atlas_offset: vec2<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_position: vec4<f32>,
}
@group(0) @binding(0)
var<uniform> camera_position: CameraUniform;

struct Light {
    position: vec3<f32>,
    color: vec3<f32>,
    is_underwater: u32,
    light_space_matrix: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> light: Light;

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var translated_instance_pos = instance.instance_position - camera_position.eye_position;
    translated_instance_pos.w = 1.0; // HACK: this feels ugly but oh well

    var translate_matrix = mat4_from_position(translated_instance_pos) * mat4_from_quaternion(instance.rotation_quaternion);

    var out: VertexOutput;
    let world_position = translate_matrix * vertex.position;
    out.tex_coord = vertex.tex_coord;
    out.clip_position = light.light_space_matrix * world_position;
    out.texture_atlas_offset = instance.texture_atlas_offset;

    // From here:
    // https://github.com/gfx-rs/wgpu/pull/71/files#diff-f91eefe904403aab76f6354857e063ff33ad277b5f046091ae1a92d9e18f8276R16-R17
    out.clip_position.z = 0.5 * (out.clip_position.z + out.clip_position.w);

    return out;
}

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(2) @binding(1)
var s_diffuse: sampler;

struct FragmentOutput {
  @builtin(frag_depth) depth: f32,
}

@fragment
fn fs_main(vertex: VertexOutput) -> FragmentOutput {
    var unit_offset: f32 = 1.0 / 32.0;
    var atlas_scaled_coords = vertex.tex_coord / 32.0;
    var offset_coords = atlas_scaled_coords + (unit_offset * vertex.texture_atlas_offset);
    var base_color = textureSample(t_diffuse, s_diffuse, offset_coords);

    var frag_out: FragmentOutput;
    frag_out.depth = select(vertex.clip_position.z, 1.1, base_color.a == 0.0);
    return frag_out;
}