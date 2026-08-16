#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
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

struct TacticalTerrainMaterial {
    base_color: vec4<f32>,
    grass_color: vec4<f32>,
    cover: vec4<f32>,
    weather: vec4<f32>,
    far_sward: vec4<f32>,
    playable_bounds: vec4<f32>,
    detail_patch: vec4<f32>,
    soil_detail: vec4<f32>,
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
    return normalize(macro_normal - surface_gradient * terrain.soil_detail.z);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let position = in.world_position.xyz;
    let normal = normalize(in.world_normal);

    // The camera-local mesh contains signed residual height. Remove the
    // coarse surface only where that patch is guaranteed to cover it, or the
    // old surface would depth-occlude every drainage channel and wheel rut.
    // A 1.5 m overlap remains before the circular patch edge, where relief is
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
    let canopy = terrain.cover.x;
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
    let height_metres = (soil_sample.r - 0.5) * terrain.soil_detail.y;
    let soil_normal = height_perturbed_normal(position, base_normal, height_metres);
    let soil_substrate = 1.0 - step(0.5, abs(substrate_kind - 0.0));
    let mud_substrate = 1.0 - step(0.5, abs(substrate_kind - 3.0));
    let road_substrate = 1.0 - step(0.5, abs(substrate_kind - 4.0));
    let substrate_response = max(soil_substrate, max(mud_substrate * 0.52, road_substrate * 0.34));
    let upward_response = smoothstep(0.48, 0.82, normal.y);
    let detail_distance_fade = 1.0 - smoothstep(12.0, terrain.soil_detail.w, camera_distance);
    let soil_response = substrate_response * upward_response * detail_distance_fade * (1.0 - snow);
    pbr_input.N = normalize(mix(base_normal, soil_normal, soil_response));
    pbr_input.diffuse_occlusion *= mix(1.0, soil_sample.g, soil_response * 0.82);
    // Signed canopy-floor distance: 0.0 is open ground, 0.0-0.5 approaches
    // litter from outside, and 0.5-1.0 moves into its shaded interior.
    let canopy_floor = ground_sample.a;
    let tall_grass = 1.0 - step(0.5, abs(cover_kind - 1.0));
    let leaf_litter = 1.0 - step(0.5, abs(cover_kind - 2.0));

    // Ground uses only solid molded-material colors selected at hard gameplay
    // boundaries. There is no sampled albedo or normal-map surface detail.
    var color = terrain.base_color.rgb;
    if canopy >= 0.5 { color = vec3<f32>(0.065, 0.045, 0.022); }
    if cultivation >= 0.4 { color = vec3<f32>(0.17, 0.11, 0.035); }
    if wetland >= 0.4 || substrate_kind == 3.0 { color = vec3<f32>(0.18, 0.16, 0.105); }

    // Near grass-covered terrain remains visible substrate. The geometric
    // blades provide all green until their final LOD begins to disappear.
    // Canopy proximity may darken that substrate, but never paints a second
    // green sward beneath the blades.
    let shaded_substrate = select(
        vec3<f32>(0.09, 0.065, 0.027),
        vec3<f32>(0.067, 0.047, 0.021),
        canopy_floor >= 0.34,
    );
    color = select(
        color,
        shaded_substrate,
        tall_grass > 0.5 && canopy_floor >= 0.16,
    );
    if substrate_kind == 1.0 || substrate_kind == 2.0 || slope >= 0.5 {
        color = vec3<f32>(0.31, 0.30, 0.275);
    }
    // Shallow litter islands visually join the shaded sward. Only the deep,
    // connected canopy core exposes dark loam.
    let litter_color = select(
        vec3<f32>(0.070, 0.045, 0.019),
        vec3<f32>(0.052, 0.032, 0.017),
        canopy_floor >= 0.78,
    );
    color = select(color, litter_color, leaf_litter > 0.5);
    if water >= 0.5 || substrate_kind == 5.0 { color = vec3<f32>(0.09, 0.18, 0.22); }

    // Use the same true camera distance that drives mesh visibility. Horizontal
    // distance alone left an overhead camera with culled blades but no
    // aggregate sward, revealing the playable terrain as a bare square.
    // Past the geometric LOD, represent the aggregate colour and normal
    // response of a sward directly on the terrain. Frequencies stay low enough
    // to remain stable when minified in a WebGPU browser canvas.
    let sward_fade = smoothstep(terrain.far_sward.x, terrain.far_sward.y, camera_distance);
    let sward_amount = sward_fade
        * terrain.far_sward.z
        * tall_grass
        * (1.0 - water)
        * (1.0 - slope * 0.72);
    let sward_color = terrain.grass_color.rgb;
    // A stable world-space screen-door transition preserves discrete molded
    // colors while avoiding a circular hard band around the camera.
    let sward_cell = floor(position.xz * 2.0);
    let sward_dither = fract(sin(dot(sward_cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    color = select(color, sward_color, sward_dither < sward_amount);

    // The camera-local detail mesh can cross the gameplay rectangle. Keep the
    // representation distance-driven while handing its clamped playable
    // material map to the vista's aggregate sward with discrete world-space
    // coverage instead of exposing a brown rectangular patch.
    let outside_distance = max(
        abs(position.x) - terrain.playable_bounds.x,
        abs(position.z) - terrain.playable_bounds.y,
    );
    let outside_sward = smoothstep(0.0, terrain.playable_bounds.z, outside_distance);
    color = select(color, sward_color, sward_dither < outside_sward);

    let snow_mask = snow * smoothstep(0.3, 0.86, normal.y);
    color = select(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask >= 0.5);

    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    let dry_roughness = select(0.84, 0.9, terrain.detail_patch.x > 0.5);
    let base_roughness = dry_roughness + wetness * 0.07 - water * 0.19;
    pbr_input.material.perceptual_roughness = clamp(base_roughness, 0.55, 1.0);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
