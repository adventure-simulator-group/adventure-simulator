// Fast-shaded loose-stone pebble meshes (hero and near LODs).
//
// The pebble patch meshes previously rendered through a full StandardMaterial
// PBR path: image-based lighting, specular, and Fresnel evaluated per fragment
// for 3-8 cm rocks that carpet the foreground. This material keeps the shared
// patch meshes and their automatic batching but swaps the fragment path for the
// same cheap model the distant grass tiers use - flat ambient plus a Lambert
// term with one clamped cascade fetch. Directional diffuse matches the PBR
// tier's Lambert/pi normalisation exactly (no diffuse transmission on solid
// rock), so the swap is energy-neutral on the lit face; `surface.w` scales the
// flat ambient in place of the skipped sky IBL.

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::{lights, view},
    pbr_functions,
    shadows,
    view_transformations::position_world_to_clip,
}

struct TacticalPebbleMaterial {
    // Linear albedo in rgb, flat-ambient scale in w.
    surface: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> pebble: TacticalPebbleMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    ));
    out.uv = vertex.uv;
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
#ifdef VISIBILITY_RANGE_DITHER
    pbr_functions::visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    let normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let albedo = pebble.surface.rgb;

    var lit = albedo * lights.ambient_color.rgb * pebble.surface.w;
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), in.world_position);
    for (var light_index = 0u; light_index < lights.n_directional_lights; light_index += 1u) {
        let light = lights.directional_lights[light_index];
        let n_dot_l = saturate(dot(normal, light.direction_to_light));
        let shadow = clamp(
            shadows::fetch_directional_shadow(
                light_index,
                in.world_position,
                normal,
                view_z,
                in.position.xy,
            ),
            0.12,
            1.0,
        );
        lit += albedo * light.color.rgb * (n_dot_l * shadow * 0.3183099);
    }

    return vec4<f32>(lit * view.exposure, 1.0);
}
