#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
#ifdef PREPASS_FRAGMENT
#import bevy_pbr::prepass_io::FragmentOutput
#endif
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{
        STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT,
        STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT,
    },
}
#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
#import bevy_pbr::mesh_view_bindings::screen_space_ambient_occlusion_texture
#endif
#endif

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

fn displaced_world_position(vertex: Vertex) -> vec4<f32> {
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    let bend = smoothstep(0.03, 1.0, 1.0 - vertex.uv.y);
    let wind_direction = normalize(leaf_card.parameters.xy);
    let wind_cross = vec2<f32>(-wind_direction.y, wind_direction.x);
    let position_phase = dot(world_position.xz, wind_direction) * 0.31
        + world_position.y * 0.17;
    let time = leaf_card.parameters.w;
    let gust = 0.7 + 0.3 * sin(time * 0.37 + position_phase * 0.63);
    let primary = sin(time + position_phase);
    let flutter = sin(time * 3.11 - position_phase * 1.73);
    let displacement = (
        wind_direction * primary * gust + wind_cross * flutter * 0.22
    ) * leaf_card.parameters.z * bend * bend;
    world_position.x += displacement.x;
    world_position.z += displacement.y;
    world_position.y -= length(displacement) * 0.16 * bend;
    return world_position;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_position = displaced_world_position(vertex);
    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

#ifndef PREPASS_PIPELINE
    var world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    ));
    let normal_bend = smoothstep(0.03, 1.0, 1.0 - vertex.uv.y);
    world_normal = normalize(world_normal + vec3<f32>(0.0, 0.16 * normal_bend, 0.0));
    out.world_normal = world_normal;
#else ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    var world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    ));
    let normal_bend = smoothstep(0.03, 1.0, 1.0 - vertex.uv.y);
    world_normal = normalize(world_normal + vec3<f32>(0.0, 0.16 * normal_bend, 0.0));
    out.world_normal = world_normal;
#endif

    out.uv = vertex.uv;
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = world_position;
#endif
    return out;
}

fn discard_transparent_leaf(uv: vec2<f32>) -> f32 {
    let opacity = textureSample(leaf_opacity, leaf_opacity_sampler, uv).r;
    var cutoff = leaf_card.surface_parameters.x;
#ifdef PREPASS_PIPELINE
#ifndef PREPASS_FRAGMENT
    // Thin only the directional shadow silhouette at the soft alpha edge.
    // The visible leaf and depth/normal prepasses retain the calibrated mask.
    cutoff = max(cutoff, 0.48);
#endif
#endif
    if opacity < cutoff {
        discard;
    }
    return opacity;
}

#ifdef PREPASS_PIPELINE
#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    discard_transparent_leaf(in.uv);
    var out: FragmentOutput;
#ifdef NORMAL_PREPASS
    let normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    out.normal = vec4<f32>(normal * 0.5 + vec3<f32>(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = vec2<f32>(0.0);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    discard_transparent_leaf(in.uv);
    // Thin whole terminal shoots, rather than high-frequency pixels, so
    // overlapping alpha cards leave coherent sun flecks on the forest floor.
    if in.color.b < 0.42 {
        discard;
    }
}
#endif
#else
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let opacity = discard_transparent_leaf(in.uv);
    let position_dx = dpdx(in.world_position.xyz);
    let position_dy = dpdy(in.world_position.xyz);
    let uv_dx = dpdx(in.uv);
    let uv_dy = dpdy(in.uv);
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

    let spatial_hue = 1.0 + 0.08
        * sin(in.world_position.x * 1.71 + in.world_position.z * 1.13);
    let interior = mix(0.72, 1.0, smoothstep(0.05, 0.72, 1.0 - in.uv.y));
    let transmitted_tint = select(vec3<f32>(1.0), vec3<f32>(1.14, 1.08, 0.82), !is_front);
    let canopy_visibility = mix(1.0, in.color.a, leaf_card.surface_parameters.z);

    var pbr_input = pbr_input_from_vertex_output(in, is_front, true);
    pbr_input.material.flags = STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT
        | STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    pbr_input.material.base_color = vec4<f32>(
        albedo * vec3<f32>(in.color.r) * spatial_hue * interior * transmitted_tint,
        opacity,
    );
    pbr_input.material.perceptual_roughness = 0.86;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.22);
    pbr_input.material.diffuse_transmission = 0.32;
    pbr_input.material.thickness = 0.001;
    pbr_input.world_normal = base_normal;
    pbr_input.N = normal;
    pbr_input.diffuse_occlusion = vec3<f32>(canopy_visibility);
    pbr_input.specular_occlusion = canopy_visibility;
#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
    let screen_ao = textureLoad(
        screen_space_ambient_occlusion_texture,
        vec2<i32>(in.position.xy),
        0i,
    ).r;
    pbr_input.diffuse_occlusion *= screen_ao;
    pbr_input.specular_occlusion *= screen_ao;
#endif

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    // Thin leaves retain substantial green indirect/transmitted light even
    // while their direct sun is occluded by another canopy layer.
    out.color = vec4<f32>(
        out.color.rgb
            + pbr_input.material.base_color.rgb * vec3<f32>(0.16, 0.20, 0.10),
        out.color.a,
    );
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
#endif
