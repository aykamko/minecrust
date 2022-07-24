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

fn mat4_from_quaternion(quat: vec4<f32>) -> mat4x4<f32> {
    let q_mat3 = mat3_from_quaternion(quat);

    return mat4x4<f32>(
        q_mat3[0].x, q_mat3[0].y, q_mat3[0].z, 0.0,
        q_mat3[1].x, q_mat3[1].y, q_mat3[1].z, 0.0,
        q_mat3[2].x, q_mat3[2].y, q_mat3[2].z, 0.0,
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
    light_space_matrix: mat4x4<f32>,
}
@group(2) @binding(0)
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
    out.tex_coord = vertex.tex_coord;
    out.world_position = translate_matrix * vertex.position;
    out.clip_position = camera_position.view_proj * out.world_position;
    // out.clip_position = light.light_space_matrix * out.world_position;
    out.texture_atlas_offset = instance.texture_atlas_offset;
    out.color_adjust = instance.color_adjust;

    // All faces are rotated from bottom face, so we can hardcode the normal
    var bottom_face_normal = vec3<f32>(0.0, -1.0, 0.0);
    out.world_normal = mat3_from_quaternion(instance.rotation_quaternion) * bottom_face_normal;

    out.light_space_position = light.light_space_matrix * out.world_position;

    return out;
}

@vertex
fn vs_wire_no_instancing(
    vertex: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coord = vertex.tex_coord;
    out.world_position = vertex.position;
    out.clip_position = camera_position.view_proj * out.world_position;

    out.light_space_position = light.light_space_matrix * out.world_position;

    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(0) @binding(2)
var t_shadow_map: texture_2d<f32>;
@group(0) @binding(3)
var s_shadow_map: sampler;

// https://learnopengl.com/Advanced-Lighting/Shadows/Shadow-Mapping
fn shadow_calculation(fragPosLightSpace: vec4<f32>) -> f32 {
    // perform perspective divide ([-1, 1])
    var projCoords = fragPosLightSpace.xyz / fragPosLightSpace.w;
    // transform to [0,1] range
    projCoords = projCoords * 0.5 + 0.5;
    projCoords.y = 1.0 - projCoords.y;
    // get closest depth value from light's perspective (using [0,1] range fragPosLight as coords)
    let closestDepth = textureSample(t_shadow_map, s_shadow_map, projCoords.xy).r; 
    // get depth of current fragment from light's perspective
    let currentDepth = projCoords.z;

    // Points outside of the sunlight volume should not be in shadow
    if (currentDepth > 1.0) {
        return 0.0;
    }

    // NOTE(aleks): smallest bias I can get before things get janky
    let bias = 0.001;

    // check whether current frag pos is in shadow
    return select(0.0, 1.0, currentDepth - bias > closestDepth);
}

fn shadow_calculation_pcf(fragPosLightSpace: vec4<f32>) -> f32 {
    // perform perspective divide ([-1, 1])
    var projCoords = fragPosLightSpace.xyz / fragPosLightSpace.w;
    // transform to [0,1] range
    projCoords = projCoords * 0.5 + 0.5;
    projCoords.y = 1.0 - projCoords.y;
    // get closest depth value from light's perspective (using [0,1] range fragPosLight as coords)
    let closestDepth = textureSample(t_shadow_map, s_shadow_map, projCoords.xy).r; 
    // get depth of current fragment from light's perspective
    let currentDepth = projCoords.z;

    // NOTE(aleks): smallest bias I can get before things get janky
    let bias = 0.0003;

    var pcf_shadow: f32 = 0.0;
    let texture_dims = textureDimensions(t_shadow_map, 0);
    let texel_size: vec2<f32> = vec2(1.0 / f32(texture_dims.x), 1.0 / f32(texture_dims.y));
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let offset = vec2<f32>(f32(x), f32(y));
            let sample_loc: vec2<f32> = projCoords.xy + offset * texel_size;
            let pcf_depth = textureSample(t_shadow_map, s_shadow_map, sample_loc).r; 
            pcf_shadow += select(0.0, 1.0, currentDepth - bias > pcf_depth);
        }    
    }
    pcf_shadow /= 9.0;

    // Points outside of the sunlight volume should not be in shadow
    return select(pcf_shadow, 0.0, currentDepth > 1.0);
}

let MAX_SHADOW_EDGE_DISTANCE = 16;

fn revectorize_shadow(relative_distances: vec2<f32>, shadow_val: f32) -> f32 {
    let r = relative_distances;
    let s = shadow_val;
    return select(1.0, 0.0, (r.x * r.y == 2.0 * s) || ((abs(r.x) * abs(r.y) > 0.0) && ((1.0 - s) + (2.0 * s - 1.0) + (abs(r.x) + abs(r.y)) < 0.5)));
}

fn shadow_test(shadowmap_depth: f32, real_depth: f32) -> f32 {
    let bias = 0.0003;
    return select(0.0, 1.0, real_depth - bias > shadowmap_depth);
}

fn estimate_relative_position(direction: vec2<f32>, shadow_val: f32) -> f32 {
    // let max_shadow_edge_distance = 16;
    let dir = direction;

    var T: f32 = 1.0;
    if (dir.x < 0.0 && dir.y < 0.0) {
        T = 0.0;
    }
    if (dir.x > 0.0 && dir.y > 0.0) {
        T = -2.0;
    }

    let edge_length = min(abs(dir.x) + abs(dir.y), f32(MAX_SHADOW_EDGE_DISTANCE));
    return (max(T, 2.0 * shadow_val - 1.0) * abs(max(T * dir.x, T * dir.y))) / edge_length;
}

fn normalize_distance_to_shadow_edge(relative_distance: vec4<f32>, shadow_val: f32) -> vec2<f32> {
    return vec2<f32>(
        estimate_relative_position(vec2<f32>(relative_distance.x, relative_distance.y), shadow_val),
        estimate_relative_position(vec2<f32>(relative_distance.z, relative_distance.w), shadow_val)
    );
}

fn check_discontinuity(shadowmap_coord: vec4<f32>, direction: vec2<f32>, discontinuity: vec3<f32>, texel_size: vec2<f32>) -> bool {
    var sample_loc_1 = vec2<f32>(-1.0, -1.0);
    var sample_loc_2 = vec2<f32>(-1.0, -1.0);
    var use_sample_1 = false;
    var use_sample_2 = false;

    var shadowmap_depth: f32;

    if (direction.x == 0.0) {
        if (discontinuity.r == 0.5 || discontinuity.r == 0.75) {
            // left
            sample_loc_1 = vec2<f32>(shadowmap_coord.x - texel_size.x, shadowmap_coord.y);
            use_sample_1 = true;
        }
        if (discontinuity.r == 0.75 || discontinuity.r == 0.25) {
            // right
            sample_loc_2 = vec2<f32>(shadowmap_coord.x + texel_size.x, shadowmap_coord.y);
            use_sample_2 = true;
        }
        // if (discontinuity.r == 0.75 || discontinuity.r == 0.25) {
        //     relative_coord.x = shadowmap_coord.x + texel_size.x;
        //     shadowmap_depth = textureSample(t_shadow_map, s_shadow_map, relative_coord.xy).r;
        //     let right = shadow_test(shadowmap_depth, shadowmap_coord.z);
        //     if (abs(right - discontinuity.z) == 0.0) {
        //         return true;
        //     }
        // }
    }

    if (direction.y == 0.0) {
        if (discontinuity.g == 0.5 || discontinuity.g == 0.75) {
            // bottom
            // XXX(aleks): the signs here are reversed from above... is this a bug?
            sample_loc_1 = vec2<f32>(shadowmap_coord.x, shadowmap_coord.y + texel_size.y);
            use_sample_1 = true;
        }
        if (discontinuity.g == 0.75 || discontinuity.g == 0.25) {
            // top
            sample_loc_2 = vec2<f32>(shadowmap_coord.x, shadowmap_coord.y - texel_size.y);
            use_sample_2 = true;
        }
    }

    let sample_1 = textureSample(t_shadow_map, s_shadow_map, sample_loc_1).r;
    let sample_2 = textureSample(t_shadow_map, s_shadow_map, sample_loc_2).r;

    if (!use_sample_1 || !use_sample_2) {
        return false;
    }
    var is_discontinuous = false;
    if (use_sample_1) {
       is_discontinuous = is_discontinuous || abs(sample_1 - discontinuity.z) == 0.0;
    }
    if (use_sample_2) {
       is_discontinuous = is_discontinuous || abs(sample_2 - discontinuity.z) == 0.0;
    }

    return is_discontinuous;
}

fn traverse_shadow_silhouette(initial_shadowmap_coords: vec4<f32>, discontinuity: vec3<f32>, texel_size: vec2<f32>, direction: vec2<f32>) -> f32 {
    // let max_shadow_edge_distance = 16;

    var shadow_edge_end = 0.0;
    var distance = 0.0;
    var has_discontinuity = false;

    var current_coords: vec4<f32> = initial_shadowmap_coords;
    let step = direction * texel_size;

    for (var i = 0; i < MAX_SHADOW_EDGE_DISTANCE; i++) {
        current_coords.x += step.x;
        current_coords.y += step.y;
        let real_depth = current_coords.z;
        let shadowmap_depth = textureSample(t_shadow_map, s_shadow_map, current_coords.xy).r;
        let s = shadow_test(shadowmap_depth, real_depth);

        // When the visibility of |initial_shadowmap_coords| and |current_coords| are different
        if (abs(s - discontinuity.z) == 0.0) {
            shadow_edge_end = 1.0;

            // // TODO(aleks): unpack this spaghetti
            // //We disable exiting discontinuities if the neighbour entering discontinuity is in all the directions
            // //We disable entering discontinuities if the neighbour exiting discontinuity is in all the directions
            // hasDisc = getDisc(centeredLightCoord, vec2(0.0, 0.0), inputDiscontinuity.b);
            // if(!hasDisc) foundEdgeEnd = 0.0;
            // else foundEdgeEnd = 1.0;

            break;
        } else {
            has_discontinuity = check_discontinuity(current_coords, direction, discontinuity, texel_size);
            if (!has_discontinuity) {
                break;
            }
        }

        distance += 1.0;
    }

    return mix(-distance, distance, shadow_edge_end);
}

fn compute_distance_to_shadow_edge(shadowmap_coords: vec4<f32>, discontinuity: vec3<f32>, texel_size: vec2<f32>) -> vec4<f32> {
    let left = traverse_shadow_silhouette(shadowmap_coords, discontinuity, texel_size, vec2<f32>(-1.0, 0.0));
    let right = traverse_shadow_silhouette(shadowmap_coords, discontinuity, texel_size, vec2<f32>(1.0, 0.0));
    let down = traverse_shadow_silhouette(shadowmap_coords, discontinuity, texel_size, vec2<f32>(0.0, -1.0));
    let up = traverse_shadow_silhouette(shadowmap_coords, discontinuity, texel_size, vec2<f32>(0.0, 1.0));

    return vec4<f32>(left, right, down, up);
}

fn compute_discontinuity(shadowmap_coords: vec4<f32>, texel_size: vec2<f32>) -> vec3<f32> {
    let center = shadow_test(textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy + vec2<f32>(0.0, 0.0) * texel_size).r, shadowmap_coords.z);
    let left = shadow_test(textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy + vec2<f32>(-1.0, 0.0) * texel_size).r, shadowmap_coords.z); 
    let right = shadow_test(textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy + vec2<f32>(1.0, 0.0) * texel_size).r, shadowmap_coords.z); 
    let top = shadow_test(textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy + vec2<f32>(0.0, 1.0) * texel_size).r, shadowmap_coords.z);
    let bottom = shadow_test(textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy + vec2<f32>(0.0, -1.0) * texel_size).r, shadowmap_coords.z);

    let discontinuity_directions = abs(vec4<f32>(left, right, bottom, top) - center);

    let discontinuity_compressed = (2.0 * discontinuity_directions.xz + discontinuity_directions.yw) / 4.0;
    let discontinuity_type = 1.0 - center;
    return vec3<f32>(discontinuity_compressed, discontinuity_type);
}

// https://arxiv.org/pdf/1711.07793.pdf
fn shadow_calculation_rbsm(light_space_pos: vec4<f32>) -> f32 {
    var shadowmap_coords = light_space_pos.xyz / light_space_pos.w;
    shadowmap_coords = shadowmap_coords * 0.5 + 0.5;
    shadowmap_coords.y = 1.0 - shadowmap_coords.y;

    let real_depth = shadowmap_coords.z;
    if (real_depth > 1.0) {
        // Beyond zfar for sunlight volume
        return 0.0;
    }

    let shadowmap_depth = textureSample(t_shadow_map, s_shadow_map, shadowmap_coords.xy).r; 
    let bias = 0.0003;
    let shadow_val = shadow_test(shadowmap_depth, real_depth);
    if (shadow_val == 1.0) {
        // Discard shadowed fragments from computation
        return 1.0;
    }

    let texture_dims = textureDimensions(t_shadow_map, 0);
    let texel_size: vec2<f32> = vec2(1.0 / f32(texture_dims.x), 1.0 / f32(texture_dims.y));

    let discontinuity = compute_discontinuity(shadowmap_coords, texel_size);
    // If fragment is on the shadow edge
    if (discontinuity.r > 0.0 || discontinuity.g > 0.0) {
        let relative_distance = compute_distance_to_shadow_edge(shadowmap_coords, discontinuity, texel_size);
        let normalized_relative_distance = normalize_distance_to_shadow_edge(relative_distance, shadow_val);
        return revectorize_shadow(normalized_relative_distance, shadow_val);
    }
    return shadow_val;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    var unit_offset: f32 = 1.0 / 32.0;
    var atlas_scaled_coords = vertex.tex_coord / 32.0;
    var offset_coords = atlas_scaled_coords + (unit_offset * vertex.texture_atlas_offset);
    var base_color = textureSample(t_diffuse, s_diffuse, offset_coords);

    var distance_from_camera = length(vertex.world_position);
    var zfar: f32 = 250.0;
    var z_fade_start: f32 = 230.0;
    var distance_alpha_adjust: f32 = max(0.0, distance_from_camera - z_fade_start) / (zfar - z_fade_start);

    var color = base_color * vertex.color_adjust;
    // var color = base_color * vec4(1.0, 1.0, 1.0, vertex.color_adjust.a);
    // color.a -= distance_alpha_adjust; // fog effect: fade distant vertices

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

    // let lighted_color = (ambient_color + diffuse_color) * color.xyz;

    var shadow = shadow_calculation_rbsm(vertex.light_space_position);

    let lighted_color = (ambient_color + (1.0 - shadow) * (diffuse_color + specular_color)) * color.xyz; 
    return vec4<f32>(lighted_color, color.a);
}

@fragment
fn fs_wire(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
