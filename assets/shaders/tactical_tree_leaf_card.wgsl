#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var leaf_opacity: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var leaf_opacity_sampler: sampler;

@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var front_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var front_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4)
var back_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5)
var back_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(6)
var front_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7)
var front_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(8)
var back_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9)
var back_normal_sampler: sampler;

struct TacticalTreeLeafCardMaterial {
    parameters: vec4<f32>,
    surface_parameters: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(10)
var<uniform> leaf_card: TacticalTreeLeafCardMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    var world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    ));

    let bend = smoothstep(0.03, 1.0, 1.0 - vertex.uv.y);
    let wind_direction = normalize(leaf_card.parameters.xy);
    let wind_cross = vec2<f32>(-wind_direction.y, wind_direction.x);
    let position_phase = dot(world_position.xz, wind_direction) * 0.31
        + world_position.y * 0.17;
    let time = globals.time * leaf_card.parameters.w;
    let gust = 0.7 + 0.3 * sin(time * 0.37 + position_phase * 0.63);
    let primary = sin(time + position_phase);
    let flutter = sin(time * 3.11 - position_phase * 1.73);
    let displacement = (
        wind_direction * primary * gust + wind_cross * flutter * 0.22
    ) * leaf_card.parameters.z * bend * bend;
    world_position.x += displacement.x;
    world_position.z += displacement.y;
    world_position.y -= length(displacement) * 0.16 * bend;
    world_normal = normalize(world_normal + vec3<f32>(
        displacement.x * 1.8,
        0.16,
        displacement.y * 1.8,
    ));

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = world_normal;
    out.uv = vertex.uv;
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
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let position_dx = dpdx(in.world_position.xyz);
    let position_dy = dpdy(in.world_position.xyz);
    let uv_dx = dpdx(in.uv);
    let uv_dy = dpdy(in.uv);
    let opacity = textureSample(leaf_opacity, leaf_opacity_sampler, in.uv).r;
    if leaf_card.surface_parameters.x >= 0.0
        && opacity < leaf_card.surface_parameters.x {
        discard;
    }
    var albedo = textureSample(front_albedo, front_albedo_sampler, in.uv).rgb;
    var tangent_normal = textureSample(front_normal, front_normal_sampler, in.uv).xyz * 2.0 - 1.0;
    if !is_front {
        albedo = textureSample(back_albedo, back_albedo_sampler, in.uv).rgb;
        tangent_normal = textureSample(back_normal, back_normal_sampler, in.uv).xyz * 2.0 - 1.0;
    }
    tangent_normal = normalize(vec3<f32>(
        tangent_normal.xy * leaf_card.surface_parameters.y,
        tangent_normal.z,
    ));
    let base_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let determinant = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;
    let orientation = select(-1.0, 1.0, determinant >= 0.0);
    let tangent_raw = (position_dx * uv_dy.y - position_dy * uv_dx.y) * orientation;
    let tangent = normalize(tangent_raw - base_normal * dot(base_normal, tangent_raw));
    let bitangent = normalize(cross(base_normal, tangent));
    let normal = normalize(
        tangent * tangent_normal.x
        + bitangent * tangent_normal.y
        + base_normal * tangent_normal.z
    );
    let light_direction = normalize(vec3<f32>(0.35, 0.86, 0.25));
    let facing_light = dot(normal, light_direction);
    let wrapped_light = 0.68 + 0.32 * clamp(facing_light * 0.5 + 0.5, 0.0, 1.0);
    let spatial_hue = 1.0 + 0.1
        * sin(in.world_position.x * 1.71 + in.world_position.z * 1.13);
    let interior = mix(0.58, 1.0, smoothstep(0.05, 0.72, 1.0 - in.uv.y));
    let transmitted = select(vec3<f32>(1.0), vec3<f32>(1.32, 1.16, 0.72), !is_front);
    return vec4<f32>(
        albedo * in.color.rgb * wrapped_light * spatial_hue * interior * transmitted,
        opacity,
    );
}
