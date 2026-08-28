// Instanced shrub foliage: the texture set and calibration of
// `tactical_tree_leaf_card.wgsl` on eidolon's instanced vertex path. Every
// representation - cambered near leaves and distant alpha cards alike - shades
// with the fast foliage model; the former full-PBR cambered path was pure
// per-fragment cost. The front/back normal maps are now unused (kept bound for
// now; a follow-up can drop bindings 6-9 from the material).

#import bevy_pbr::{
    mesh_view_bindings::{globals, lights, view},
    shadows,
    view_transformations::position_world_to_clip,
}

#import bevy_eidolon::render::utils
#import bevy_eidolon::render::bindings::instance_uniforms
#import bevy_eidolon::render::io_types::Vertex

@group(3) @binding(0) var leaf_opacity: texture_2d<f32>;
@group(3) @binding(1) var leaf_opacity_sampler: sampler;
@group(3) @binding(2) var front_albedo: texture_2d<f32>;
@group(3) @binding(3) var front_albedo_sampler: sampler;
@group(3) @binding(4) var back_albedo: texture_2d<f32>;
@group(3) @binding(5) var back_albedo_sampler: sampler;
@group(3) @binding(6) var front_normal: texture_2d<f32>;
@group(3) @binding(7) var front_normal_sampler: sampler;
@group(3) @binding(8) var back_normal: texture_2d<f32>;
@group(3) @binding(9) var back_normal_sampler: sampler;
@group(3) @binding(10) var leaf_arm: texture_2d<f32>;
@group(3) @binding(11) var leaf_arm_sampler: sampler;

struct TacticalShrubLeafInstancedUniform {
    // Wind direction XZ, strength; w reserved (phase comes from globals).
    parameters: vec4<f32>,
    // Opacity cutoff, normal strength, canopy AO strength, transmission.
    surface_parameters: vec4<f32>,
    // Roughness fallback, thickness, litter pigment lane, reserved.
    physical_parameters: vec4<f32>,
    // y scales the flat ambient term; every representation now shades fast.
    shading: vec4<f32>,
}

@group(3) @binding(12) var<uniform> leaf: TacticalShrubLeafInstancedUniform;

struct ShrubLeafVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
#ifdef VISIBILITY_RANGE_DITHER
    @location(0) @interpolate(flat) visibility_range_dither: i32,
#endif
    @location(1) world_position: vec4<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> ShrubLeafVertexOutput {
    var out: ShrubLeafVertexOutput;
    let batch = instance_uniforms[vertex.i_batch_id];
    let instance_matrix = utils::calc_instance_world_matrix(
        vertex.i_pos_scale,
        vertex.i_rotation,
        batch.world_from_local,
    );
    var world_position = instance_matrix * vec4<f32>(vertex.position, 1.0);

    var uv = vec2<f32>(0.0);
#ifdef VERTEX_UVS_A
    uv = vertex.uv;
#endif
    // Same sway as the tree leaf-card shader, driven by the shader clock.
    let bend = smoothstep(0.03, 1.0, 1.0 - uv.y);
    let wind_direction = normalize(leaf.parameters.xy);
    let wind_cross = vec2<f32>(-wind_direction.y, wind_direction.x);
    let position_phase = dot(world_position.xz, wind_direction) * 0.31
        + world_position.y * 0.17;
    let time = globals.time * 1.15;
    let gust = 0.7 + 0.3 * sin(time * 0.37 + position_phase * 0.63);
    let primary = sin(time + position_phase);
    let flutter = sin(time * 3.11 - position_phase * 1.73);
    let displacement = (
        wind_direction * primary * gust + wind_cross * flutter * 0.22
    ) * leaf.parameters.z * bend * bend;
    world_position.x += displacement.x;
    world_position.z += displacement.y;
    world_position.y -= length(displacement) * 0.16 * bend;

    out.world_position = world_position;
    out.clip_position = position_world_to_clip(world_position.xyz);
    out.uv = uv;

    var normal = vec3<f32>(0.0, 1.0, 0.0);
#ifdef VERTEX_NORMALS
    normal = vertex.normal;
#endif
    let rotation_cos = cos(vertex.i_rotation);
    let rotation_sin = sin(vertex.i_rotation);
    var world_normal = normalize(vec3<f32>(
        rotation_cos * normal.x + rotation_sin * normal.z,
        normal.y,
        -rotation_sin * normal.x + rotation_cos * normal.z,
    ));
    let normal_bend = smoothstep(0.03, 1.0, 1.0 - uv.y);
    out.world_normal = normalize(world_normal + vec3<f32>(0.0, 0.16 * normal_bend, 0.0));

    out.color = vec4<f32>(1.0);
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    let instance_origin = instance_matrix * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.visibility_range_dither =
        utils::get_visibility_range_dither_level(batch.visibility_range, instance_origin);
#endif
    return out;
}

@fragment
fn fragment(
    in: ShrubLeafVertexOutput,
    @builtin(front_facing) is_front: bool,
) -> @location(0) vec4<f32> {
#ifdef VISIBILITY_RANGE_DITHER
    // Complementary per-pixel partition between the cambered and card
    // representations; see tactical_grass_instanced.wgsl for the rationale.
    if in.visibility_range_dither != 0 {
        let magnitude = clamp(f32(abs(in.visibility_range_dither)) / 16.0, 0.0, 1.0);
        let pixel_hash = fract(
            52.9829189
                * fract(dot(floor(in.clip_position.xy), vec2<f32>(0.06711056, 0.00583715))),
        );
        if in.visibility_range_dither > 0 {
            if pixel_hash < magnitude {
                discard;
            }
        } else if pixel_hash >= 1.0 - magnitude {
            discard;
        }
    }
#endif
    let opacity = textureSample(leaf_opacity, leaf_opacity_sampler, in.uv).r;
    if opacity < leaf.surface_parameters.x {
        discard;
    }

    var albedo = textureSample(front_albedo, front_albedo_sampler, in.uv).rgb;
    if !is_front {
        albedo = textureSample(back_albedo, back_albedo_sampler, in.uv).rgb;
    }
    // Deterministic per-leaf pigments (berries, senescent tints) ride the
    // vertex colour lane, exactly like the tree leaf-card shader.
    albedo = mix(albedo, in.color.rgb, leaf.physical_parameters.z);
    let arm = textureSample(leaf_arm, leaf_arm_sampler, in.uv).rgb;
    let base_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let canopy_visibility = mix(1.0, in.color.a, leaf.surface_parameters.z);
    let occlusion = canopy_visibility * arm.r;

    // Every representation - including the cambered near leaves - shades with
    // the fast foliage model: flat ambient + wrapped Lambert translucency +
    // one clamped cascade fetch. No screen-space tangent frame, normal map,
    // image-based lighting, or specular. The cards already used this model and
    // the cambered leaves crossfade into it, so full PBR was pure cost.
    var lit = albedo * lights.ambient_color.rgb * leaf.shading.y * occlusion;
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), in.world_position);
    let transmission = leaf.surface_parameters.w;
    for (var light_index = 0u; light_index < lights.n_directional_lights;
        light_index += 1u)
    {
        let light = lights.directional_lights[light_index];
        let alignment = dot(base_normal, light.direction_to_light);
        let wrapped = saturate(alignment) * (1.0 - transmission)
            + saturate(-alignment) * transmission;
        let shadow = clamp(
            shadows::fetch_directional_shadow(
                light_index,
                in.world_position,
                base_normal,
                view_z,
                in.clip_position.xy,
            ),
            0.12,
            1.0,
        );
        lit += albedo * light.color.rgb * (wrapped * shadow * occlusion * 0.3183099);
    }
    return vec4<f32>(lit * view.exposure, opacity);
}
