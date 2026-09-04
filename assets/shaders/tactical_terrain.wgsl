#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::{view, lights},
    shadows,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#endif

struct TacticalTerrainMaterial {
    base_color: vec4<f32>,
    grass_color: vec4<f32>,
    cover: vec4<f32>,
    weather: vec4<f32>,
    far_sward: vec4<f32>,
    lod_sward: vec4<f32>,
    playable_bounds: vec4<f32>,
    detail_patch: vec4<f32>,
    soil_detail: vec4<f32>,
    litter_detail: vec4<f32>,
    cliff_palette_a: vec4<f32>,
    cliff_palette_b: vec4<f32>,
    cliff_surface: vec4<f32>,
    cliff_structure_a: vec4<f32>,
    cliff_structure_b: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> terrain: TacticalTerrainMaterial;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var ground_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var ground_map_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var soil_height_ao: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var soil_height_ao_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var litter_surface: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106)
var litter_surface_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107)
var litter_normal_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108)
var litter_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109)
var blood_mask: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110)
var blood_mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(111)
var cliff_height: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(112)
var cliff_height_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(113)
var cliff_arm: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114)
var cliff_arm_sampler: sampler;

fn cliff_triplanar_weights(normal: vec3<f32>) -> vec3<f32> {
    let sharpened = pow(abs(normal), vec3<f32>(4.0));
    return sharpened / max(dot(sharpened, vec3<f32>(1.0)), 0.00001);
}

fn cliff_axis_uvs(world_position: vec3<f32>) -> mat3x2<f32> {
    return mat3x2<f32>(
        world_position.zy * terrain.cliff_surface.x,
        world_position.xz * terrain.cliff_surface.x,
        world_position.xy * terrain.cliff_surface.x,
    );
}

fn cliff_triplanar_height(uvs: mat3x2<f32>, weights: vec3<f32>) -> f32 {
    return textureSample(cliff_height, cliff_height_sampler, uvs[0]).r * weights.x
        + textureSample(cliff_height, cliff_height_sampler, uvs[1]).r * weights.y
        + textureSample(cliff_height, cliff_height_sampler, uvs[2]).r * weights.z;
}

fn cliff_triplanar_arm(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec3<f32> {
    return textureSample(cliff_arm, cliff_arm_sampler, uvs[0]).rgb * weights.x
        + textureSample(cliff_arm, cliff_arm_sampler, uvs[1]).rgb * weights.y
        + textureSample(cliff_arm, cliff_arm_sampler, uvs[2]).rgb * weights.z;
}

// Bedding and foliation are evaluated once in continuous scene space. The
// three axis UVs above contribute only non-structural grain and microrelief.
fn geological_scalar(world_position: vec3<f32>) -> f32 {
    let structural_normal = normalize(terrain.cliff_structure_a.xyz);
    let broad_warp = (
        sin(dot(world_position, vec3<f32>(0.071, 0.019, 0.043)))
        + sin(dot(world_position, vec3<f32>(-0.037, 0.013, 0.061))) * 0.5
    ) * terrain.cliff_structure_b.z;
    let spacing_variation = 1.0 + terrain.cliff_structure_b.y
        * sin(dot(world_position, vec3<f32>(0.017, 0.011, -0.023)));
    return (dot(world_position, structural_normal) + broad_warp)
        / max(terrain.cliff_structure_b.x * spacing_variation, 0.08);
}

fn geological_palette_selector(world_position: vec3<f32>) -> f32 {
    let mode = terrain.cliff_structure_a.w;
    if mode < 0.5 {
        let massive = sin(dot(world_position, vec3<f32>(0.41, 0.29, 0.37)))
            + 0.45 * sin(dot(world_position, vec3<f32>(0.13, -0.19, 0.17)));
        return step(0.18, massive);
    }
    let scalar = geological_scalar(world_position);
    let primary = fract(scalar);
    let edge_width = max(fwidth(scalar) * 1.25, 0.012);
    var layer = smoothstep(0.36 - edge_width, 0.36 + edge_width, primary)
        * (1.0 - smoothstep(0.82 - edge_width, 0.82 + edge_width, primary));
    if mode < 1.5 && terrain.cliff_structure_b.w > 0.0 {
        let cross_scalar = dot(world_position, vec3<f32>(0.31, 0.91, -0.27))
            / max(terrain.cliff_structure_b.x * 0.42, 0.08);
        layer = mix(layer, step(0.54, fract(cross_scalar)), terrain.cliff_structure_b.w * 0.34);
    }
    return layer;
}

fn height_perturbed_normal(
    world_position: vec3<f32>,
    macro_normal: vec3<f32>,
    height_metres: f32,
    normal_strength: f32,
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
    return normalize(macro_normal - surface_gradient * normal_strength);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let position = in.world_position.xyz;
    let normal = normalize(in.world_normal);

    // The camera-local mesh contains signed residual height. Remove the
    // coarse surface only where that patch is guaranteed to cover it, or the
    // old surface would depth-occlude every drainage channel and wheel rut.
    // A 2 m overlap remains before the circular patch edge, where relief is
    // already morphed almost completely back to the authoritative surface.
    if terrain.detail_patch.x > 0.5
        && distance(position.xz, view.lod_view_world_position.xz) < terrain.detail_patch.y {
        discard;
    }
    // Preserve the geometry-derived direction but modestly expand its lateral
    // components on the refined mesh. Solid molded colors otherwise let the
    // bright environment-light floor wash out centimetre-scale facets.
    let readable_detail_normal = normalize(vec3<f32>(normal.x * 1.45, normal.y, normal.z * 1.45));
    let base_normal = select(readable_detail_normal, normal, terrain.detail_patch.x > 0.5);
    pbr_input.N = base_normal;
    let wetland = terrain.cover.y;
    let cultivation = terrain.cover.z;
    let water = terrain.cover.w;
    let wetness = terrain.weather.x;
    let snow = terrain.weather.y;
    let hilly = terrain.weather.z;
    let slope = smoothstep(0.16, 0.68, 1.0 - abs(normal.y));
    let ground_sample = textureSample(ground_map, ground_map_sampler, in.uv);
    let cover_kind = round(ground_sample.r * 255.0);
    let substrate_kind = round(ground_sample.g * 255.0);
    let camera_distance = distance(position, view.lod_view_world_position.xyz);
    // One repeatable packed sample supplies physical height and AO. World-XZ
    // mapping is sufficient for walkable soil; its influence fades on steep
    // faces where planar projection would stretch.
    let soil_warp = vec2<f32>(
        sin(position.z * 0.29 + position.x * 0.11),
        sin(position.x * 0.23 - position.z * 0.17),
    ) * 0.035;
    let soil_uv = position.xz * terrain.soil_detail.x + soil_warp;
    let soil_sample = textureSample(soil_height_ao, soil_height_ao_sampler, soil_uv).rg;
    let litter_warp = vec2<f32>(
        sin(position.z * 0.17 - position.x * 0.07),
        sin(position.x * 0.13 + position.z * 0.09),
    ) * 0.021;
    let litter_uv = position.xz * terrain.litter_detail.x + litter_warp;
    // R is also a true parallax height field. One bounded relief lookup shifts
    // the shared normal/surface samples at grazing angles, while an overhead
    // view remains exactly on the unshifted world-XZ projection.
    let litter_base_sample = textureSample(
        litter_surface,
        litter_surface_sampler,
        litter_uv,
    );
    let view_to_camera = normalize(view.lod_view_world_position.xyz - position);
    let parallax_direction = view_to_camera.xz / max(abs(view_to_camera.y), 0.24);
    let parallax_height_metres = (litter_base_sample.r - 0.48) * terrain.litter_detail.y;
    let parallax_offset = clamp(
        parallax_direction * parallax_height_metres * terrain.litter_detail.x,
        vec2<f32>(-0.035),
        vec2<f32>(0.035),
    );
    let litter_parallax_uv = litter_uv - parallax_offset;
    // RGBA carries normalized height, broad/local AO, a stable muted-palette
    // selector, and physical litter coverage. The separate RG normal map is
    // generated from this exact height field, so color, relief, and parallax
    // retain one leaf silhouette through their complete mip chains.
    let litter_sample = textureSample(
        litter_surface,
        litter_surface_sampler,
        litter_parallax_uv,
    );
    let litter_normal_xz = textureSample(
        litter_normal_map,
        litter_normal_sampler,
        litter_parallax_uv,
    ).rg * 2.0 - 1.0;
    let height_metres = (soil_sample.r - 0.5) * terrain.soil_detail.y;
    let soil_normal = height_perturbed_normal(
        position,
        base_normal,
        height_metres,
        terrain.soil_detail.z,
    );
    let soil_substrate = 1.0 - step(0.5, abs(substrate_kind - 0.0));
    let mud_substrate = 1.0 - step(0.5, abs(substrate_kind - 3.0));
    let road_substrate = 1.0 - step(0.5, abs(substrate_kind - 4.0));
    let substrate_response = max(soil_substrate, max(mud_substrate * 0.52, road_substrate * 0.34));
    let upward_response = smoothstep(0.48, 0.82, normal.y);
    let detail_distance_fade = 1.0 - smoothstep(12.0, terrain.soil_detail.w, camera_distance);
    let soil_response = substrate_response * upward_response * detail_distance_fade * (1.0 - snow);
    pbr_input.N = normalize(mix(base_normal, soil_normal, soil_response));
    pbr_input.diffuse_occlusion *= mix(1.0, soil_sample.g, soil_response * 0.82);
    let tall_grass = 1.0 - step(0.5, abs(cover_kind - 1.0));
    let leaf_litter = 1.0 - step(0.5, abs(cover_kind - 2.0));
    // Ground-map alpha is a signed canopy-floor distance: the lower half is
    // the exterior approach and the upper half is depth inside leaf litter.
    // Keep categorical authority while allowing a broad, organic material
    // transition beyond the last litter cell.
    let canopy_floor = smoothstep(0.14, 0.72, ground_sample.a);
    let litter_region = max(leaf_litter * 0.88, canopy_floor)
        * soil_substrate
        * upward_response
        * (1.0 - snow);
    let litter_distance_fade = 1.0 - smoothstep(20.0, terrain.litter_detail.w, camera_distance);
    let litter_normal_y = sqrt(max(0.0, 1.0 - dot(litter_normal_xz, litter_normal_xz)));
    let litter_mapped_normal = normalize(
        base_normal * litter_normal_y
        + vec3<f32>(litter_normal_xz.x, 0.0, litter_normal_xz.y)
    );
    let litter_relief = litter_region * litter_sample.a * litter_distance_fade;
    pbr_input.N = normalize(mix(
        pbr_input.N,
        litter_mapped_normal,
        litter_relief * terrain.litter_detail.z,
    ));
    pbr_input.diffuse_occlusion *= mix(1.0, litter_sample.g, litter_relief * 0.88);

    // Ordinary soil keeps one solid molded-material albedo. Canopy floor adds
    // a constrained procedural palette: dark shaded soil remains visible in
    // the packed coverage gaps while overlapping litter supplies the mass.
    var color = terrain.base_color.rgb;
    if cultivation >= 0.4 { color = vec3<f32>(0.17, 0.11, 0.035); }
    if wetland >= 0.4 || substrate_kind == 3.0 { color = vec3<f32>(0.18, 0.16, 0.105); }

    // Near grass-covered terrain retains the scene substrate albedo. Geometric
    // blades provide its green cover, while canopy lighting supplies shade.
    if substrate_kind == 1.0 || substrate_kind == 2.0 || slope >= 0.5 {
        color = vec3<f32>(0.31, 0.30, 0.275);
    }
    if water >= 0.5 || substrate_kind == 5.0 { color = vec3<f32>(0.09, 0.18, 0.22); }

    // The implicit patch alone enables this response. Steep faces and
    // downward-facing overhangs keep geological identity, while horizontal
    // top/collar regions return to the ordinary ground material at the rim.
    let cliff_side = smoothstep(0.16, 0.58, 1.0 - abs(normal.y));
    let cliff_underside = 1.0 - smoothstep(-0.62, -0.12, normal.y);
    let cliff_response = terrain.cliff_palette_a.w * max(cliff_side, cliff_underside);
    let cliff_weights = cliff_triplanar_weights(base_normal);
    let cliff_uvs = cliff_axis_uvs(position);
    let cliff_height_sample = cliff_triplanar_height(cliff_uvs, cliff_weights);
    let cliff_arm_sample = cliff_triplanar_arm(cliff_uvs, cliff_weights);
    let cliff_height_metres = (cliff_height_sample - 0.5) * terrain.cliff_surface.y;
    let cliff_normal = height_perturbed_normal(position, base_normal, cliff_height_metres, 1.0);
    let cliff_selector = geological_palette_selector(position);
    let cliff_color = mix(terrain.cliff_palette_a.rgb, terrain.cliff_palette_b.rgb, cliff_selector);
    color = mix(color, cliff_color, cliff_response);
    pbr_input.N = normalize(mix(pbr_input.N, cliff_normal, cliff_response));
    pbr_input.diffuse_occlusion *= mix(1.0, clamp(cliff_arm_sample.r, 0.48, 1.0), cliff_response);

    let shaded_soil = terrain.base_color.rgb * vec3<f32>(0.38, 0.40, 0.37);
    // Match the dark/medium/pale bands of the physical dry-oak cards. Stronger
    // separation from the soil and a narrow generated coverage rim keep these
    // leaves recognizable instead of reducing the terrain layer to mottling.
    let litter_dark = vec3<f32>(0.016, 0.010, 0.006);
    let litter_mid = vec3<f32>(0.035, 0.021, 0.011);
    let litter_pale = vec3<f32>(0.062, 0.036, 0.018);
    let litter_color = mix(
        mix(litter_dark, litter_mid, smoothstep(0.05, 0.58, litter_sample.b)),
        litter_pale,
        smoothstep(0.62, 0.96, litter_sample.b),
    );
    color = mix(color, shaded_soil, litter_region * 0.76);
    color = mix(color, litter_color, litter_region * litter_sample.a * 0.92);

    // Use the same true camera distance that drives mesh visibility. Horizontal
    // distance alone left an overhead camera with culled blades but no
    // aggregate sward, revealing the playable terrain as a bare square.
    // Past the geometric LOD, represent the aggregate colour and normal
    // response of a sward directly on the terrain. Frequencies stay low enough
    // to remain stable when minified in a WebGPU browser canvas.
    // Start the dithered ground fill while Near exchanges its full root set
    // for Far's stable sparse subset. The retained blades preserve their width;
    // this field replaces only the coverage that left with the other roots.
    // The terminal fade then completes the same field as Vista blades disappear.
    let near_to_far_sward = smoothstep(
        terrain.lod_sward.x,
        terrain.lod_sward.y,
        camera_distance,
    ) * terrain.lod_sward.z;
    let terminal_sward = smoothstep(terrain.far_sward.x, terrain.far_sward.y, camera_distance);
    let sward_coverage = mix(near_to_far_sward, 1.0, terminal_sward);
    let sward_amount = sward_coverage
        * terrain.far_sward.z
        * tall_grass
        * (1.0 - water)
        * (1.0 - slope * 0.72)
        * (1.0 - cliff_response);
    let sward_color = terrain.grass_color.rgb;
    // Retain low-amplitude world-space variation without hard-selecting one
    // flat terminal color. Continuous optical coverage avoids a visible ring
    // as geometry hands the sward to the terrain.
    let sward_cell = floor(position.xz * 2.0);
    let sward_dither = fract(sin(dot(sward_cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let sward_target = sward_color * mix(0.92, 1.08, sward_dither);
    color = mix(color, sward_target, sward_amount);

    // The camera-local detail mesh can cross the gameplay rectangle. Keep the
    // representation distance-driven while handing its clamped playable
    // material map to the vista's aggregate sward with discrete world-space
    // coverage instead of exposing a brown rectangular patch.
    let outside_distance = max(
        abs(position.x) - terrain.playable_bounds.x,
        abs(position.z) - terrain.playable_bounds.y,
    );
    let outside_sward = smoothstep(0.0, terrain.playable_bounds.z, outside_distance);
    color = mix(color, sward_target, outside_sward);

    let snow_mask = snow * smoothstep(0.3, 0.86, normal.y);
    // Accumulation is continuous across slope and coverage. The old binary
    // threshold exposed the terrain lattice as large square white pixels and
    // erased all readable relief at the snow line. Reuse the packed physical
    // height field at low strength so covered ground remains molded rather
    // than becoming a perfectly flat color plate.
    color = mix(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask);
    pbr_input.N = normalize(mix(
        pbr_input.N,
        soil_normal,
        snow_mask * detail_distance_fade * upward_response * 0.24,
    ));

    let blood = textureSample(blood_mask, blood_mask_sampler, in.uv).r;
    let dried_blood = vec3<f32>(0.035, 0.0003, 0.0007);
    pbr_input.material.base_color = vec4<f32>(mix(color, dried_blood, blood), 1.0);
    let dry_roughness = select(0.84, 0.9, terrain.detail_patch.x > 0.5);
    // A continuous film darkens porous ground and narrows its highlights.
    // Snow remains a rough dielectric unless the underlying surface is water.
    let litter_roughness = litter_region * litter_sample.a * 0.055;
    let ground_roughness = dry_roughness + litter_roughness
        - wetness * 0.22 + snow_mask * 0.08 - water * 0.19;
    let cliff_roughness = mix(
        terrain.cliff_surface.z,
        terrain.cliff_surface.w,
        clamp(cliff_arm_sample.g, 0.0, 1.0),
    ) - wetness * 0.16;
    let base_roughness = mix(ground_roughness, cliff_roughness, cliff_response);
    pbr_input.material.perceptual_roughness = mix(
        clamp(base_roughness, 0.55, 1.0),
        0.86,
        blood,
    );
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    // Matte soil (roughness 0.55-1.0, no metal) gains nothing from full PBR's
    // image-based lighting and specular. Shade it with the same fast model as
    // the foliage: flat ambient scaled by the accumulated soil/litter AO, plus
    // one clamped cascade fetch per directional light. Fog is negligible here
    // (linear start 30 km; the vista terrain is a separate material), and the
    // camera's ACES pass still tonemaps this output, matching the foliage.
    var out: FragmentOutput;
    let albedo = pbr_input.material.base_color.rgb;
    let N = pbr_input.N;
    let ambient_occlusion = pbr_input.diffuse_occlusion;
    var lit = albedo * lights.ambient_color.rgb * ambient_occlusion;
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), in.world_position);
    for (var light_index = 0u; light_index < lights.n_directional_lights; light_index += 1u) {
        let light = lights.directional_lights[light_index];
        let n_dot_l = saturate(dot(N, light.direction_to_light));
        let shadow = clamp(
            shadows::fetch_directional_shadow(
                light_index,
                in.world_position,
                N,
                view_z,
                in.position.xy,
            ),
            0.12,
            1.0,
        );
        lit += albedo * light.color.rgb * (n_dot_l * shadow * 0.3183099);
    }
    out.color = vec4<f32>(lit * view.exposure, 1.0);
    return out;
#endif
}
