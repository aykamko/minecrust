struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
}

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) clip_position: vec4<f32>,
    @location(1) texture_atlas_offset: vec2<f32>,
    @location(2) color_adjust: vec4<f32>,
    @location(3) world_position: vec4<f32>,
    @location(6) world_normal: vec3<f32>,
    @location(7) light_space_position: vec4<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_position: vec4<f32>,
}

struct InstanceInput {
    @location(4) instance_position: vec4<f32>,
    @location(5) rotation_quaternion: vec4<f32>,
    @location(9) texture_atlas_offset: vec2<f32>,
    @location(10) color_adjust: vec4<f32>,
}

@group(1) @binding(0)
var<uniform> camera_position: CameraUniform;

struct Light {
    position: vec3<f32>,
    color: vec3<f32>,
}
@group(2) @binding(0)
var<uniform> light: Light;

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

fn mat3_from_quaternion(quat: vec4<f32>) -> mat3x3<f32> {
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

    return mat3x3<f32>(
        1.0 - yy2 - zz2, xy2 + sz2, xz2 - sy2,
        xy2 - sz2, 1.0 - xx2 - zz2, yz2 + sx2,
        xz2 + sy2, yz2 - sx2, 1.0 - xx2 - yy2,
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

@group(3) @binding(0)
var<uniform> light_space_matrix: mat4x4<f32>;

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    // All faces are rotated from bottom face, so we can hardcode the normal
    var bottom_face_normal = vec3<f32>(0.0, -1.0, 0.0);

    var translated_instance_pos = instance.instance_position - camera_position.eye_position;
    translated_instance_pos.w = 1.0; // HACK: this feels ugly but oh well

    var translate_matrix = mat4_from_position(translated_instance_pos) * mat4_from_quaternion(instance.rotation_quaternion);

    var out: VertexOutput;
    out.tex_coord = vertex.tex_coord;
    out.world_position = translate_matrix * vertex.position;
    out.clip_position = camera_position.view_proj * out.world_position;
    out.texture_atlas_offset = instance.texture_atlas_offset;
    out.color_adjust = instance.color_adjust;
    out.world_normal = mat3_from_quaternion(instance.rotation_quaternion) * bottom_face_normal;
    out.light_space_position = out.world_position * light_space_matrix;
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

    var distance_from_camera = length(vertex.world_position);

    var zfar: f32 = 250.0;
    var z_fade_start: f32 = 230.0;
    var distance_alpha_adjust: f32 = max(0.0, distance_from_camera - z_fade_start) / (zfar - z_fade_start);

    //var color = textureSample(t_diffuse, s_diffuse, offset_coords) * vertex.color_adjust;
    var color = textureSample(t_diffuse, s_diffuse, offset_coords) * vec4(1.0, 1.0, 1.0, vertex.color_adjust.a);
    color.a -= distance_alpha_adjust; // fog effect: fade distant vertices

    // We don't need (or want) much ambient light, so 0.1 is fine
    let ambient_strength = 0.3;
    let ambient_color = light.color * ambient_strength;

    let light_dir = normalize(light.position - vertex.world_position.xyz);
    let diffuse_strength = max(dot(vertex.world_normal, light_dir), 0.0);
    let diffuse_color = light.color * diffuse_strength;

    let view_dir = normalize(vec3<f32>(0.0, 0.0, 0.0) - vertex.world_position.xyz);
    let half_dir = normalize(view_dir + light_dir);
    let specular_strength = pow(max(dot(vertex.world_normal, half_dir), 0.0), 32.0);
    let specular_color = light.color * specular_strength;

    let lighted_color = (ambient_color + diffuse_color) * color.xyz;
    return vec4<f32>(lighted_color, color.a);
}

@fragment
fn fs_wire(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
