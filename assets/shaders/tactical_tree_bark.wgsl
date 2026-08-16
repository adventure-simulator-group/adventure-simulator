#import bevy_pbr::{
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var bark_height_ao: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var bark_height_ao_sampler: sampler;

struct TacticalTreeBarkExtension {
    relief: vec4<f32>,
    projection: vec4<f32>,
    lighting: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> bark: TacticalTreeBarkExtension;

fn triplanar_weights(normal: vec3<f32>) -> vec3<f32> {
    let weighted = pow(abs(normal), vec3<f32>(bark.projection.x));
    return weighted / max(dot(weighted, vec3<f32>(1.0)), 0.0001);
}

fn branch_texture_coordinates(branch_uv: vec2<f32>) -> vec2<f32> {
    // Sweep U is measured in one-metre circumference tiles; sweep V is
    // measured in two-metre growth-axis tiles.
    return vec2<f32>(branch_uv.x, branch_uv.y * 2.0) * bark.relief.x;
}

fn macro_coordinate_warp(position: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(
        sin(dot(position, vec3<f32>(0.173, 0.071, -0.119)) + 0.41),
        sin(dot(position, vec3<f32>(-0.089, 0.137, 0.191)) + 1.73),
    ) * 0.075;
}

fn axis_uvs(position: vec3<f32>, branch_coordinates: vec2<f32>) -> mat3x2<f32> {
    let point = position * bark.relief.x;
    let alignment = bark.projection.y;
    let growth_x = mix(point.y, branch_coordinates.y, alignment);
    let growth_z = mix(point.y, branch_coordinates.y, alignment);
    let branch_top = mix(point.xz, branch_coordinates, vec2<f32>(alignment));
    return mat3x2<f32>(
        vec2<f32>(point.z, growth_x) + vec2<f32>(0.173, 0.311),
        branch_top + vec2<f32>(0.419, 0.071),
        vec2<f32>(point.x, growth_z) + vec2<f32>(0.619, 0.233),
    );
}

fn cotangent_frame(
    normal: vec3<f32>,
    position: vec3<f32>,
    coordinates: vec2<f32>,
) -> mat3x3<f32> {
    let position_dx = dpdx(position);
    let position_dy = dpdy(position);
    let coordinate_dx = dpdx(coordinates);
    let coordinate_dy = dpdy(coordinates);
    let perpendicular_x = cross(position_dy, normal);
    let perpendicular_y = cross(normal, position_dx);
    let tangent = perpendicular_x * coordinate_dx.x + perpendicular_y * coordinate_dy.x;
    let bitangent = perpendicular_x * coordinate_dx.y + perpendicular_y * coordinate_dy.y;
    let inverse_scale = inverseSqrt(max(dot(tangent, tangent), dot(bitangent, bitangent)));
    return mat3x3<f32>(tangent * inverse_scale, bitangent * inverse_scale, normal);
}

fn parallax_branch_coordinates(
    position: vec3<f32>,
    normal: vec3<f32>,
    branch_coordinates: vec2<f32>,
    view_direction: vec3<f32>,
    camera_distance: f32,
) -> vec3<f32> {
    let fade = 1.0 - smoothstep(bark.projection.w * 0.45, bark.projection.w, camera_distance);
    let coordinate_dx = dpdx(branch_coordinates);
    let coordinate_dy = dpdy(branch_coordinates);
    if bark.relief.y <= 0.0001 || fade <= 0.001 {
        return vec3<f32>(branch_coordinates, 0.0);
    }
    let frame = cotangent_frame(normal, position, branch_coordinates);
    let tangent_view = transpose(frame) * view_direction;
    let layer_count = 6.0;
    let layer_depth = 1.0 / layer_count;
    let texture_depth = bark.relief.y * bark.relief.x * bark.projection.z * fade;
    let ray_step = tangent_view.xy / max(abs(tangent_view.z), 0.28)
        * texture_depth / layer_count;
    var coordinates = branch_coordinates;
    var travelled = 0.0;
    for (var layer = 0; layer < 6; layer += 1) {
        let height = textureSampleGrad(
            bark_height_ao,
            bark_height_ao_sampler,
            coordinates,
            coordinate_dx,
            coordinate_dy,
        ).r;
        if travelled < 1.0 - height {
            coordinates -= ray_step;
            travelled += layer_depth;
        }
    }
    return vec3<f32>(coordinates, travelled * fade);
}

fn directional_horizon_visibility(
    position: vec3<f32>,
    normal: vec3<f32>,
    branch_coordinates: vec2<f32>,
    centre_height_metres: f32,
    camera_distance: f32,
) -> f32 {
    let fade = 1.0 - smoothstep(bark.projection.w * 0.45, bark.projection.w, camera_distance);
    let coordinate_dx = dpdx(branch_coordinates);
    let coordinate_dy = dpdy(branch_coordinates);
    if bark.relief.y <= 0.0001 || fade <= 0.001 || bark.lighting.w <= 0.001 {
        return 1.0;
    }
    let frame = cotangent_frame(normal, position, branch_coordinates);
    let tangent_light = transpose(frame) * normalize(bark.lighting.xyz);
    let lateral_length = length(tangent_light.xy);
    if tangent_light.z <= 0.02 || lateral_length <= 0.02 {
        return 1.0;
    }
    let lateral_direction = tangent_light.xy / lateral_length;
    let light_slope = tangent_light.z / lateral_length;
    var blockage = 0.0;
    for (var horizon_step = 1; horizon_step <= 3; horizon_step += 1) {
        let step_metres = f32(horizon_step) * 0.012;
        let coordinates = branch_coordinates
            + lateral_direction * step_metres * bark.relief.x;
        let neighbor = textureSampleGrad(
            bark_height_ao,
            bark_height_ao_sampler,
            coordinates,
            coordinate_dx,
            coordinate_dy,
        ).r;
        let neighbor_height_metres = (neighbor - 0.5) * bark.relief.y;
        let ray_height_metres = centre_height_metres + step_metres * light_slope;
        blockage = max(
            blockage,
            smoothstep(ray_height_metres + 0.001, ray_height_metres + 0.004, neighbor_height_metres),
        );
    }
    return 1.0 - blockage * bark.lighting.w * fade * 0.58;
}

fn triplanar_height_ao(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec2<f32> {
    let x_sample = textureSample(bark_height_ao, bark_height_ao_sampler, uvs[0]).rg;
    let y_sample = textureSample(bark_height_ao, bark_height_ao_sampler, uvs[1]).rg;
    let z_sample = textureSample(bark_height_ao, bark_height_ao_sampler, uvs[2]).rg;
    return x_sample * weights.x + y_sample * weights.y + z_sample * weights.z;
}

fn height_perturbed_normal(
    world_position: vec3<f32>,
    macro_normal: vec3<f32>,
    height_metres: f32,
) -> vec3<f32> {
    let position_dx = dpdx(world_position);
    let position_dy = dpdy(world_position);
    let height_dx = dpdx(height_metres);
    let height_dy = dpdy(height_metres);
    let reciprocal_x = cross(position_dy, macro_normal);
    let reciprocal_y = cross(macro_normal, position_dx);
    let determinant = dot(position_dx, reciprocal_x);
    let safe_determinant = select(
        -max(abs(determinant), 0.000001),
        max(abs(determinant), 0.000001),
        determinant >= 0.0,
    );
    let surface_gradient = (
        reciprocal_x * height_dx + reciprocal_y * height_dy
    ) / safe_determinant;
    return normalize(macro_normal - surface_gradient * bark.relief.z);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let macro_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let weights = triplanar_weights(macro_normal);
    let branch_coordinates = branch_texture_coordinates(in.uv)
        + macro_coordinate_warp(in.world_position.xyz);
    let view_direction = normalize(view.world_position - in.world_position.xyz);
    let camera_distance = distance(view.lod_view_world_position.xyz, in.world_position.xyz);
    let parallax = parallax_branch_coordinates(
        in.world_position.xyz,
        macro_normal,
        branch_coordinates,
        view_direction,
        camera_distance,
    );
    let sample = triplanar_height_ao(
        axis_uvs(in.world_position.xyz, parallax.xy),
        weights,
    );
    let height_metres = (sample.r - 0.5) * bark.relief.y;
    let composed_normal = height_perturbed_normal(
        in.world_position.xyz,
        macro_normal,
        height_metres,
    );
    let parallax_shadow = 1.0 - parallax.z * 0.22;
    let directional_visibility = directional_horizon_visibility(
        in.world_position.xyz,
        macro_normal,
        parallax.xy,
        height_metres,
        camera_distance,
    );
    let ambient_visibility = mix(1.0, sample.g, bark.relief.w)
        * parallax_shadow
        * directional_visibility;
    let cavity = 1.0 - sample.g;
    let micro_roughness = 0.025 * sin(dot(in.world_position.xyz, vec3<f32>(37.0, 53.0, 29.0)));

    pbr_input.world_normal = macro_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(
        pbr_input.material.base_color.rgb * mix(1.0, directional_visibility, 0.62),
        pbr_input.material.base_color.a,
    );
    pbr_input.material.perceptual_roughness = clamp(
        pbr_input.material.perceptual_roughness + cavity * 0.10 + micro_roughness,
        0.62,
        0.94,
    );
    pbr_input.diffuse_occlusion = clamp(
        pbr_input.diffuse_occlusion * ambient_visibility,
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    pbr_input.specular_occlusion = clamp(
        pbr_input.specular_occlusion * ambient_visibility,
        0.0,
        1.0,
    );
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
