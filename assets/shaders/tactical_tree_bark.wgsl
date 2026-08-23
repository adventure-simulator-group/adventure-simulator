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
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var terrain_heightmap: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var soil_height_ao: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var soil_height_ao_sampler: sampler;

struct TacticalTreeBarkExtension {
    relief: vec4<f32>,
    projection: vec4<f32>,
    lighting: vec4<f32>,
    surface: vec4<f32>,
    soil_surface: vec4<f32>,
    deposition: vec4<f32>,
    terrain_surface: vec4<f32>,
    soil_response: vec4<f32>,
    soil_optics: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> bark: TacticalTreeBarkExtension;

fn hash21(cell: vec2<f32>) -> f32 {
    let mixed = vec3<f32>(cell.x, cell.y, cell.x) * vec3<f32>(0.1031, 0.1030, 0.0973);
    let fractal = fract(mixed);
    let folded = fractal + dot(fractal, fractal.yzx + vec3<f32>(33.33));
    return fract((folded.x + folded.y) * folded.z);
}

fn hash22(cell: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(hash21(cell), hash21(cell + vec2<f32>(19.19, 47.47)));
}

fn decode_terrain_height(texel: vec4<f32>) -> f32 {
    let encoded = round(texel.r * 255.0) + round(texel.g * 255.0) * 256.0;
    let normalized = encoded / 65535.0;
    return mix(bark.terrain_surface.z, bark.terrain_surface.w, normalized);
}

fn terrain_height_at(world_xz: vec2<f32>) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(terrain_heightmap));
    let grid_maximum = vec2<f32>(dimensions - vec2<i32>(1));
    let extent = max(bark.terrain_surface.xy * 2.0, vec2<f32>(0.001));
    let normalized = clamp(
        (world_xz + bark.terrain_surface.xy) / extent,
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    let grid = normalized * grid_maximum;
    let cell = clamp(
        vec2<i32>(floor(grid)),
        vec2<i32>(0),
        dimensions - vec2<i32>(2),
    );
    let fraction = grid - vec2<f32>(cell);
    let x0y0 = decode_terrain_height(textureLoad(terrain_heightmap, cell, 0));
    let x1y0 = decode_terrain_height(
        textureLoad(terrain_heightmap, cell + vec2<i32>(1, 0), 0)
    );
    let x0y1 = decode_terrain_height(
        textureLoad(terrain_heightmap, cell + vec2<i32>(0, 1), 0)
    );
    let x1y1 = decode_terrain_height(
        textureLoad(terrain_heightmap, cell + vec2<i32>(1, 1), 0)
    );
    if fraction.x >= fraction.y {
        return x0y0
            + (x1y0 - x0y0) * fraction.x
            + (x1y1 - x1y0) * fraction.y;
    }
    return x0y0
        + (x1y1 - x0y1) * fraction.x
        + (x0y1 - x0y0) * fraction.y;
}

fn soil_surface_sample(world_position: vec3<f32>) -> vec2<f32> {
    let warp = vec2<f32>(
        sin(world_position.z * 0.29 + world_position.x * 0.11),
        sin(world_position.x * 0.23 - world_position.z * 0.17),
    ) * 0.035;
    let uv = world_position.xz * bark.soil_response.x + warp;
    return textureSample(soil_height_ao, soil_height_ao_sampler, uv).rg;
}

fn dominant_projection(position: vec3<f32>, normal: vec3<f32>) -> vec2<f32> {
    let axis = abs(normal);
    if axis.x > axis.y && axis.x > axis.z {
        return position.zy;
    }
    if axis.y > axis.z {
        return position.xz;
    }
    return position.xy;
}

fn soil_speck_distance(
    projected: vec2<f32>,
    root_height: f32,
    cell_size: f32,
    layer_offset: vec2<f32>,
    minimum_radius: f32,
    base_occupancy: f32,
) -> f32 {
    let scaled = projected / cell_size + layer_offset;
    let cell = floor(scaled);
    let within_cell = fract(scaled);
    let random = hash22(cell + layer_offset * 17.0);
    let centre = vec2<f32>(0.08) + random * 0.84;
    let delta = (within_cell - centre) * cell_size;
    let shape_random = hash22(cell + layer_offset * 31.0 + vec2<f32>(71.7, 13.1));
    let angle = shape_random.x * 6.2831853;
    let major = vec2<f32>(cos(angle), sin(angle));
    let minor = vec2<f32>(-major.y, major.x);
    let aspect = mix(0.82, 1.22, shape_random.y);
    let elliptical_distance = length(vec2<f32>(
        dot(delta, major) / aspect,
        dot(delta, minor) * aspect,
    ));
    let radius = mix(
        minimum_radius,
        cell_size * 0.38,
        hash21(cell + layer_offset * 43.0 + vec2<f32>(29.3, 83.1)),
    );

    let height_fraction = clamp(
        (root_height - bark.deposition.x)
            / max(bark.deposition.y - bark.deposition.x, 0.0001),
        0.0,
        1.0,
    );
    let occupied = hash21(cell + layer_offset * 59.0 + vec2<f32>(101.3, 59.9))
        < mix(base_occupancy, 0.025, height_fraction);
    let bounded = min(radius - elliptical_distance, bark.deposition.y - root_height);
    return select(-cell_size, bounded, occupied);
}

fn root_soil_signed_distance(
    world_position: vec3<f32>,
    macro_normal: vec3<f32>,
    root_height: f32,
) -> f32 {
    let cell_size = bark.deposition.z;
    // Keep the procedural work confined to the narrow root-contact band. The
    // coherent branch avoids evaluating two cellular fields over the many
    // square metres of clean trunk above it.
    if root_height > bark.deposition.y + cell_size {
        return -cell_size;
    }
    if root_height < bark.deposition.x - cell_size {
        return cell_size;
    }
    let projected = dominant_projection(world_position, macro_normal);
    let fine_specks = soil_speck_distance(
        projected,
        root_height,
        cell_size,
        vec2<f32>(3.17, 8.53),
        bark.deposition.w,
        0.58,
    );
    let coarse_specks = soil_speck_distance(
        projected,
        root_height,
        cell_size * 1.73,
        vec2<f32>(11.41, 2.79),
        bark.deposition.w * 1.35,
        0.34,
    );
    let visible_speck = max(fine_specks, coarse_specks);

    // A shallow continuous contact coat seats the trunk in the ground. Its
    // per-cell height variation prevents a mechanically level ring.
    let contact_variation = (hash21(floor(world_position.xz / (cell_size * 1.7))) - 0.5)
        * cell_size * 1.6;
    let contact = bark.deposition.x + contact_variation - root_height;
    return max(contact, visible_speck);
}

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
    strength: f32,
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
    return normalize(macro_normal - surface_gradient * strength);
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
        bark.relief.z,
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
    var soil_coverage = 0.0;
    var soil_response_coverage = 0.0;
    var soil_sample = vec2<f32>(0.5, 1.0);
    var soil_normal = macro_normal;
#ifdef VERTEX_COLORS
    // Trunk meshes store final metric height above their root plane in vertex
    // colour R. This includes the root flare's radial displacement because the
    // value is generated from the final surface position. Recovering the root
    // plane lets upper-trunk fragments skip terrain lookup altogether.
    let root_plane_height = in.world_position.y - in.color.r;
    // Use the maximum encoded terrain height instead of assuming a locally
    // level root plane. The root-contact field can only reach deposition.y +
    // one cell above the sampled terrain, so this is conservative even when a
    // root spans the steepest supported terrain displacement.
    let root_contact_ceiling = max(
        bark.terrain_surface.w - root_plane_height,
        0.0,
    ) + bark.deposition.y + bark.deposition.z;
    if in.color.r <= root_contact_ceiling {
        let terrain_height = terrain_height_at(in.world_position.xz);
        let terrain_clearance = in.world_position.y - terrain_height;
        let signed_distance = root_soil_signed_distance(
            in.world_position.xyz,
            macro_normal,
            terrain_clearance,
        );
        // This is material selection, not translucent blending. Only the analytic
        // coverage required to antialias the binary boundary occupies the narrow
        // interval between zero and one.
        let edge_width = max(fwidth(signed_distance), 0.0002);
        soil_coverage = smoothstep(-edge_width, edge_width, signed_distance);
        // Terrain material maps only bridge the physical contact seam. Above two
        // inches, deposited dirt remains an albedo-only treatment over bark.
        let contact_response = 1.0 - smoothstep(0.0381, 0.0508, max(terrain_clearance, 0.0));
        soil_response_coverage = soil_coverage * contact_response;
        soil_sample = soil_surface_sample(in.world_position.xyz);
        let soil_height_metres = (soil_sample.r - 0.5) * bark.soil_response.y;
        soil_normal = height_perturbed_normal(
            in.world_position.xyz,
            macro_normal,
            soil_height_metres,
            bark.soil_response.z,
        );
    }
#endif

    pbr_input.world_normal = macro_normal;
    pbr_input.N = normalize(mix(composed_normal, soil_normal, soil_response_coverage));
    let bark_directional_response = mix(1.0, directional_visibility, 0.62);
    pbr_input.material.base_color = vec4<f32>(
        mix(bark.surface.rgb, bark.soil_surface.rgb, soil_coverage)
            * mix(bark_directional_response, 1.0, soil_response_coverage),
        1.0,
    );
    let bark_roughness = clamp(
        bark.surface.w + cavity * 0.10 + micro_roughness,
        0.62,
        0.94,
    );
    pbr_input.material.perceptual_roughness = mix(
        bark_roughness,
        bark.soil_surface.w,
        soil_coverage,
    );
    pbr_input.material.reflectance = mix(
        pbr_input.material.reflectance,
        vec3<f32>(bark.soil_optics.x),
        soil_coverage,
    );
    let soil_visibility = mix(1.0, soil_sample.g, bark.soil_response.w);
    pbr_input.diffuse_occlusion = clamp(
        pbr_input.diffuse_occlusion
            * mix(ambient_visibility, soil_visibility, soil_response_coverage),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    pbr_input.specular_occlusion = clamp(
        pbr_input.specular_occlusion * mix(ambient_visibility, 1.0, soil_response_coverage),
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
