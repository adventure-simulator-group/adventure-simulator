#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}

struct TacticalFoliageMaterial {
    wind: vec4<f32>,
    interaction: vec4<f32>,
    interaction_motion: vec4<f32>,
    lod: vec4<f32>,
    shading: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> foliage: TacticalFoliageMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var position = vertex.position;
    var root_local = vec3<f32>(0.0, 0.0, 0.0);
    var blade_threshold = 0.0;
#ifdef VERTEX_UVS_B
    root_local = vec3<f32>(vertex.uv_b.x, 0.0, vertex.uv_b.y);
    blade_threshold = vertex.color.a;
#endif

    let root_world = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(root_local, 1.0),
    ).xyz;
    let distance_to_camera = distance(root_world, view.lod_view_world_position.xyz);
    let lod_amount = smoothstep(foliage.lod.x, foliage.lod.y, distance_to_camera);
    let density = mix(1.0, foliage.lod.z, lod_amount * foliage.lod.w);
    var survival = 1.0;
    if foliage.lod.w > 0.5 {
        survival = 1.0 - smoothstep(density, density + 0.035, blade_threshold);
    }
    let width_compensation = mix(
        1.0,
        min(2.25, inverseSqrt(max(density, 0.01))),
        foliage.lod.w,
    );
    let adjusted_xz = root_local.xz
        + (position.xz - root_local.xz) * width_compensation * survival;
    position = vec3<f32>(
        adjusted_xz.x,
        root_local.y + (position.y - root_local.y) * survival,
        adjusted_xz.y,
    );

    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(position, 1.0),
    );
    let bend = clamp(vertex.uv.y, 0.0, 1.0) * survival;
    let wind_direction = normalize(foliage.wind.xy);
    let wind_cross = vec2<f32>(-wind_direction.y, wind_direction.x);
    let spatial_noise = sin(root_world.x * 0.071 + root_world.z * 0.113)
        * sin(root_world.x * 0.037 - root_world.z * 0.053);
    let wave_position = dot(root_world.xz, wind_direction) * 0.22;
    let wind_time = globals.time * foliage.wind.w;
    let gust = 0.68 + 0.32 * sin(wind_time * 0.29 + spatial_noise * 3.7);
    let primary_wave = sin(wind_time + wave_position + spatial_noise * 1.9);
    let flutter = sin(wind_time * 2.73 - wave_position * 1.61 + spatial_noise * 5.3);
    let natural_lean = vec2<f32>(
        sin(root_world.x * 1.73 + root_world.z * 0.61),
        cos(root_world.z * 1.41 - root_world.x * 0.47),
    ) * 0.055 * foliage.shading.w;
    let wind_offset = (
        wind_direction * primary_wave * gust
        + wind_cross * flutter * 0.18
    ) * foliage.wind.z;
    let wind_bend = (natural_lean + wind_offset) * bend * bend;
    world_position = vec4<f32>(
        world_position.x + wind_bend.x,
        world_position.y,
        world_position.z + wind_bend.y,
        world_position.w,
    );

    if foliage.interaction.w > 0.0 && foliage.shading.w > 0.5 {
        let from_player = root_world.xz - foliage.interaction.xz;
        let player_distance = length(from_player);
        let velocity_xz = foliage.interaction_motion.xz;
        let fallback_direction = normalize(velocity_xz + vec2<f32>(0.0001, 0.0));
        let push_direction = select(
            fallback_direction,
            normalize(from_player),
            player_distance > 0.001,
        );
        let player_push = (1.0 - smoothstep(0.18, foliage.interaction.w, player_distance))
            * foliage.interaction_motion.w;
        let motion_direction = normalize(velocity_xz + push_direction * 0.15);
        let interaction_bend = (
            push_direction * 0.62 + motion_direction * min(length(velocity_xz) * 0.035, 0.28)
        ) * player_push * bend * bend;
        world_position = vec4<f32>(
            world_position.x + interaction_bend.x,
            world_position.y - player_push * 0.34 * bend * bend,
            world_position.z + interaction_bend.y,
            world_position.w,
        );
    }

    out.world_position = world_position;
    out.position = position_world_to_clip(out.world_position.xyz);
    let world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.world_normal = normalize(mix(world_normal, vec3<f32>(0.0, 1.0, 0.0), foliage.shading.z));
    out.uv = vertex.uv;
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
    out.color = vertex.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let height_fraction = clamp(in.uv.y, 0.0, 1.0);
    let root_self_shadow = mix(
        foliage.shading.x,
        1.0,
        pow(height_fraction, 0.72),
    );
    let centre_distance = abs(in.uv.x - 0.5) * 2.0;
    let centre_rib = mix(0.84, 1.0, smoothstep(0.12, 0.72, centre_distance));
    let meadow_variation = 1.0 + foliage.shading.y
        * sin(in.world_position.x * 0.083 + sin(in.world_position.z * 0.057) * 2.3);
    let light_direction = normalize(vec3<f32>(0.35, 0.86, 0.25));
    let soft_light = 0.72 + 0.28 * max(dot(normalize(in.world_normal), light_direction), 0.0);
    let color = in.color.rgb * root_self_shadow * centre_rib * meadow_variation * soft_light;
    return vec4<f32>(color, 1.0);
}
