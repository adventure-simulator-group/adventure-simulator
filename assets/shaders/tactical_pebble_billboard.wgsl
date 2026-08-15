#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::view,
    pbr_functions,
    view_transformations::position_world_to_clip,
}

struct TacticalPebbleBillboardMaterial {
    color: vec4<f32>,
    lighting: vec4<f32>,
    ambient: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> pebble: TacticalPebbleBillboardMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    // Distant patch meshes store each pebble centre in the unused normal
    // channel. Positions are quad offsets, allowing a whole patch to remain
    // one draw while every member independently faces the camera.
    let centre_world = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.normal, 1.0),
    ).xyz;
    let to_camera = normalize(view.world_position.xz - centre_world.xz + vec2<f32>(0.0001, 0.0));
    let right = vec3<f32>(to_camera.y, 0.0, -to_camera.x);
    let world_position = vec4<f32>(
        centre_world + right * vertex.position.x + vec3<f32>(0.0, vertex.position.y, 0.0),
        1.0,
    );

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = normalize(vec3<f32>(to_camera.x, 0.35, to_camera.y));
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

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef VISIBILITY_RANGE_DITHER
    pbr_functions::visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    let shape = in.uv * 2.0 - vec2<f32>(1.0);
    let radius_squared = dot(shape, shape);
    let edge_width = max(fwidth(radius_squared), 0.001);
    let silhouette = 1.0 - smoothstep(1.0 - edge_width, 1.0 + edge_width, radius_squared);

    // UV derivatives provide the quad's actual rasterized diameter, so each
    // physical size fades independently instead of sharing a world-distance
    // cutoff. The last stochastic samples disappear at roughly one pixel.
    let uv_per_pixel = max(length(dpdx(in.uv)), length(dpdy(in.uv)));
    let diameter_pixels = 1.0 / max(uv_per_pixel, 0.00001);
    let screen_coverage = smoothstep(0.75, 1.5, diameter_pixels);
    let coverage = silhouette * screen_coverage;
    if interleaved_gradient_noise(floor(in.position.xy)) > coverage {
        discard;
    }

    // Reconstruct a broad world-space pebble normal from the camera-facing
    // basis. Albedo remains the one uniform molded-stone colour shared with
    // the mesh LODs; only lighting creates variation across the sprite.
    let to_camera = normalize(view.world_position.xz - in.world_position.xz + vec2<f32>(0.0001, 0.0));
    let right = vec3<f32>(to_camera.y, 0.0, -to_camera.x);
    let facing = vec3<f32>(to_camera.x, 0.0, to_camera.y);
    let surface_normal = normalize(
        right * shape.x
        + vec3<f32>(0.0, shape.y, 0.0)
        + facing * sqrt(max(1.0 - radius_squared, 0.0)),
    );
    let direct = max(dot(surface_normal, normalize(pebble.lighting.xyz)), 0.0)
        * pebble.lighting.w
        * (1.0 - pebble.ambient.w);
    let irradiance = pebble.ambient.rgb * pebble.ambient.w + vec3<f32>(direct);
    return vec4<f32>(pebble.color.rgb * irradiance, 1.0);
}
