#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
}

struct TacticalFoliageMaterial {
    wind: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> foliage: TacticalFoliageMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var position = vertex.position;
    let anchor = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(0.0, 0.0, 0.0, 1.0));
    let bend = clamp(vertex.uv.y, 0.0, 1.0);
    let phase = anchor.x * 0.13 + anchor.z * 0.097 + globals.time * foliage.wind.w;
    let gust = sin(phase) * 0.68 + sin(phase * 2.37 + 1.4) * 0.32;
    position.x += foliage.wind.x * foliage.wind.z * gust * bend * bend;
    position.z += foliage.wind.y * foliage.wind.z * gust * bend * bend;
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
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
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let upright = 0.68 + abs(in.world_normal.y) * 0.18;
    let tip = 0.86 + in.uv.y * 0.14;
    return vec4<f32>(in.color.rgb * upright * tip, 1.0);
}
