// Instanced shrub wood. The legacy per-entity renderer used a plain
// StandardMaterial, so this shades with the same fast model as the distant
// grass tiers: flat ambient + Lambert + one clamped cascade fetch. Thin
// twigs at 0-52 m gain nothing from image-based lighting.

#import bevy_pbr::{
    mesh_view_bindings::{lights, view},
    shadows,
    view_transformations::position_world_to_clip,
}

#import bevy_eidolon::render::utils
#import bevy_eidolon::render::bindings::instance_uniforms
#import bevy_eidolon::render::io_types::Vertex

// Linear bark pigment; w is perceptual roughness (unused by the fast model
// but kept for parity with the source material).
@group(3) @binding(0) var<uniform> bark_color: vec4<f32>;

struct ShrubVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
#ifdef VISIBILITY_RANGE_DITHER
    @location(0) @interpolate(flat) visibility_range_dither: i32,
#endif
    @location(1) world_position: vec4<f32>,
    @location(2) world_normal: vec3<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> ShrubVertexOutput {
    var out: ShrubVertexOutput;
    let batch = instance_uniforms[vertex.i_batch_id];
    let instance_matrix = utils::calc_instance_world_matrix(
        vertex.i_pos_scale,
        vertex.i_rotation,
        batch.world_from_local,
    );
    let world_position = instance_matrix * vec4<f32>(vertex.position, 1.0);
    out.world_position = world_position;
    out.clip_position = position_world_to_clip(world_position.xyz);

    var normal = vec3<f32>(0.0, 1.0, 0.0);
#ifdef VERTEX_NORMALS
    normal = vertex.normal;
#endif
    // Yaw-only instance rotation; uniform scale leaves direction unchanged.
    let rotation_cos = cos(vertex.i_rotation);
    let rotation_sin = sin(vertex.i_rotation);
    out.world_normal = normalize(vec3<f32>(
        rotation_cos * normal.x + rotation_sin * normal.z,
        normal.y,
        -rotation_sin * normal.x + rotation_cos * normal.z,
    ));

#ifdef VISIBILITY_RANGE_DITHER
    let instance_origin = instance_matrix * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.visibility_range_dither =
        utils::get_visibility_range_dither_level(batch.visibility_range, instance_origin);
#endif
    return out;
}

@fragment
fn fragment(
    in: ShrubVertexOutput,
    @builtin(front_facing) is_front: bool,
) -> @location(0) vec4<f32> {
#ifdef VISIBILITY_RANGE_DITHER
    // Complementary per-pixel partition; see tactical_grass_instanced.wgsl.
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
    let normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    // The wood mesh packs metric root height into the vertex-colour lane (see
    // wood_mesh.rs) - the tree bark shader decodes it as a soil-deposition
    // mask, it is NOT pigment. Its red channel carries height above the
    // tree-scale root plane (~8-10 m), so multiplying it into the bark colour
    // scaled the red channel past one and blew the wood out to coral. The
    // shrub fast path uses the flat bark pigment directly.
    let albedo = bark_color.rgb;
    var lit = albedo * lights.ambient_color.rgb;
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), in.world_position);
    for (var light_index = 0u; light_index < lights.n_directional_lights;
        light_index += 1u)
    {
        let light = lights.directional_lights[light_index];
        let alignment = saturate(dot(normal, light.direction_to_light));
        let shadow = clamp(
            shadows::fetch_directional_shadow(
                light_index,
                in.world_position,
                normal,
                view_z,
                in.clip_position.xy,
            ),
            0.12,
            1.0,
        );
        lit += albedo * light.color.rgb * (alignment * shadow * 0.3183099);
    }
    return vec4<f32>(lit * view.exposure, 1.0);
}
