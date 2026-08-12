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
    cover: vec4<f32>,
    weather: vec4<f32>,
    variation: vec4<f32>,
    far_sward: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> terrain: TacticalTerrainMaterial;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var ground_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var ground_map_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var dirt_diffuse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var dirt_diffuse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var dirt_normal_gl: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106)
var dirt_normal_gl_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107)
var dirt_arm: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108)
var dirt_arm_sampler: sampler;

fn detail_noise(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let weights = abs(normal) / max(dot(abs(normal), vec3<f32>(1.0)), 0.001);
    let seed = terrain.variation.x * 31.0;
    let yz = sin(position.y * 1.17 + position.z * 1.89 + seed)
        * cos(position.y * 2.41 - position.z * 0.73 - seed * 0.31);
    let xz = sin(position.x * 1.33 - position.z * 1.71 + seed * 1.37)
        * cos(position.x * 2.03 + position.z * 0.91 + seed * 0.53);
    let xy = sin(position.x * 1.61 + position.y * 1.09 - seed * 0.71)
        * cos(position.x * 0.67 - position.y * 2.23 + seed * 0.83);
    let fine = dot(vec3<f32>(yz, xz, xy), weights);
    let broad = sin(position.x * 0.17 + position.z * 0.11 + seed)
        * cos(position.z * 0.19 - position.x * 0.07 - seed);
    // Keep high-frequency triplanar detail subordinate to the band-limited
    // macro variation so distant/overhead views do not resolve into moire.
    return fine * 0.12 + broad * 0.88;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let position = in.world_position.xyz;
    let normal = normalize(in.world_normal);
    let detail = detail_noise(position, normal);
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
    let tall_grass = 1.0 - step(0.5, abs(cover_kind - 1.0));
    let leaf_litter = 1.0 - step(0.5, abs(cover_kind - 2.0));

    var color = terrain.base_color.rgb;
    let forest_floor = vec3<f32>(0.105, 0.175, 0.072) * (0.91 + detail * 0.09);
    let mud = vec3<f32>(0.18, 0.16, 0.105) * (0.94 + detail * 0.06);
    let tilled = vec3<f32>(0.33, 0.285, 0.105) * (0.92 + detail * 0.08);
    let stone = vec3<f32>(0.31, 0.30, 0.275) * (0.9 + detail * 0.1);
    let leaf_floor = vec3<f32>(0.255, 0.16, 0.065) * (0.9 + detail * 0.1);
    color = mix(color, forest_floor, canopy * 0.74);
    color = mix(color, mud, max(wetland, wetness) * 0.68);
    color = mix(color, tilled, cultivation * 0.72);
    color = mix(color, stone, slope * (0.48 + hilly * 0.42));
    color = mix(color, leaf_floor, leaf_litter * 0.88);
    color = mix(color, vec3<f32>(0.09, 0.18, 0.22), water * 0.78);
    color *= 0.94 + detail * terrain.variation.y;

    // The scan is authored at two metres and repeats in world space. Restrict
    // it to upward-facing, near/mid-field fragments: vertical rock faces have
    // their own geological material and distant terrain needs stable macro
    // colour instead of subpixel texture reads.
    let dirt_uv = position.xz * 0.5 + vec2<f32>(
        terrain.variation.x * 7.13,
        terrain.variation.x * 11.47,
    );
    let dirt_color = textureSample(dirt_diffuse, dirt_diffuse_sampler, dirt_uv).rgb;
    let dirt_normal = textureSample(dirt_normal_gl, dirt_normal_gl_sampler, dirt_uv).xyz * 2.0 - 1.0;
    let dirt_arm_sample = textureSample(dirt_arm, dirt_arm_sampler, dirt_uv).rgb;
    let camera_distance = distance(position.xz, view.lod_view_world_position.xz);
    let texture_distance_fade = 1.0 - smoothstep(42.0, 96.0, camera_distance);
    let soil_surface = (1.0 - water) * (1.0 - snow) * smoothstep(0.46, 0.82, normal.y);
    let dirt_amount = texture_distance_fade * soil_surface;
    let dirt_luminance = dot(dirt_color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let neutral_dirt = dirt_color / max(dirt_luminance, 0.08);
    color *= mix(vec3<f32>(1.0), neutral_dirt, dirt_amount * 0.48);

    // Past the geometric LOD, represent the aggregate colour and normal
    // response of a sward directly on the terrain. Frequencies stay low enough
    // to remain stable when minified in a WebGPU browser canvas.
    let sward_fade = smoothstep(terrain.far_sward.x, terrain.far_sward.y, camera_distance);
    let sward_amount = sward_fade
        * terrain.far_sward.z
        * tall_grass
        * (1.0 - water)
        * (1.0 - slope * 0.72);
    let sward_pattern = sin(position.x * 0.43 + position.z * 0.19 + terrain.variation.x * 17.0)
        * cos(position.z * 0.37 - position.x * 0.13 - terrain.variation.x * 11.0);
    let sward_color = color * vec3<f32>(0.82, 1.035, 0.72)
        * (0.97 + sward_pattern * 0.035);
    color = mix(color, sward_color, sward_amount * 0.44);

    let snow_mask = snow * smoothstep(0.3, 0.86, normal.y) * (0.91 + detail * 0.09);
    color = mix(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask);

    let micro = (terrain.variation.z + sward_amount * 0.012) * (1.0 - snow_mask);
    let procedural_normal = normalize(pbr_input.N + vec3<f32>(
        cos(position.x * 2.11 + position.z * 0.47),
        0.0,
        sin(position.z * 1.93 - position.x * 0.41),
    ) * micro);
    let tangent_x = normalize(vec3<f32>(1.0, -normal.x / max(normal.y, 0.18), 0.0));
    let tangent_z = normalize(vec3<f32>(0.0, -normal.z / max(normal.y, 0.18), 1.0));
    let mapped_dirt_normal = normalize(
        tangent_x * dirt_normal.x + tangent_z * dirt_normal.y + normal * dirt_normal.z,
    );
    let composed_normal = normalize(mix(procedural_normal, mapped_dirt_normal, dirt_amount * 0.54));
    pbr_input.world_normal = composed_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    let base_roughness = 0.9 + wetness * 0.07 - water * 0.19;
    pbr_input.material.perceptual_roughness = clamp(
        mix(base_roughness, dirt_arm_sample.g, dirt_amount * 0.72),
        0.55,
        1.0,
    );
    pbr_input.diffuse_occlusion *= vec3<f32>(mix(1.0, dirt_arm_sample.r, dirt_amount * 0.5));
    pbr_input.specular_occlusion *= mix(1.0, dirt_arm_sample.r, dirt_amount * 0.5);
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
