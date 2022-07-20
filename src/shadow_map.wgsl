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
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

struct InstanceInput {
    @location(4) instance_position: vec4<f32>,
    @location(5) rotation_quaternion: vec4<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_position: vec4<f32>,
}
@group(0) @binding(0)
var<uniform> camera_position: CameraUniform;

@group(1) @binding(0)
var<uniform> light_space_matrix: mat4x4<f32>;

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
    out.clip_position = light_space_matrix * world_position;
    return out;
}

@fragment
fn fs_main(vertex: VertexOutput) {}