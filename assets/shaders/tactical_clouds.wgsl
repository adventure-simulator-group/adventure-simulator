#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> cloud_lighting: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> cloud_shape: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var<uniform> cloud_layer: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var<uniform> cloud_motion: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4)
var<uniform> cloud_spectral: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5)
var<uniform> cloud_geometry: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(6)
var cloud_noise_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7)
var cloud_noise_sampler: sampler;

fn shell_center() -> vec3<f32> {
    return vec3<f32>(cloud_geometry.x, -cloud_geometry.z, cloud_geometry.y);
}

fn ray_sphere_roots(
    ray_origin: vec3<f32>,
    ray_direction: vec3<f32>,
    radius: f32,
) -> vec2<f32> {
    let relative_origin = ray_origin - shell_center();
    let projected = dot(relative_origin, ray_direction);
    let discriminant = projected * projected
        - (dot(relative_origin, relative_origin) - radius * radius);
    if discriminant < 0.0 {
        return vec2<f32>(-1.0, -1.0);
    }
    let root = sqrt(discriminant);
    return vec2<f32>(-projected - root, -projected + root);
}

fn cloud_coordinate(world_position: vec3<f32>) -> vec2<f32> {
    let kind = u32(cloud_shape.z + 0.5);
    var coordinate = (world_position.xz + cloud_motion.xy) * cloud_layer.y;
    coordinate += vec2<f32>(cloud_shape.w * 0.013, -cloud_shape.w * 0.011);
    if kind == 2u {
        coordinate = coordinate * vec2<f32>(0.32, 1.8);
    } else if kind == 5u || kind == 8u {
        coordinate *= 1.75;
    } else if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        coordinate *= 0.58;
    }
    return coordinate;
}

/// One filtered texture lookup supplies all cloud shape variation. The global
/// shell intentionally has no depth integration, empty-space search, or
/// self-shadow ray: broad weather identity matters more than local volume.
fn sample_cloud_surface(world_position: vec3<f32>) -> vec4<f32> {
    return textureSample(
        cloud_noise_texture,
        cloud_noise_sampler,
        fract(cloud_coordinate(world_position)),
    );
}

fn cloud_coverage(noise: vec4<f32>) -> f32 {
    let kind = u32(cloud_shape.z + 0.5);
    let broad = mix(noise.r, noise.g, select(0.28, 0.55, kind == 2u));
    var threshold = 0.82 - cloud_shape.x * 0.42;
    var softness = 0.15;
    if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        threshold -= 0.13;
        softness = 0.24;
    }
    if kind == 3u || kind == 10u {
        threshold += 0.06;
    }
    var body = smoothstep(threshold, threshold + softness, broad);
    if kind == 2u {
        body *= 0.48 + noise.b * 0.52;
    }
    return body;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ray_origin = view.world_position;
    let ray_direction = normalize(in.world_position.xyz - ray_origin);
    if ray_direction.y <= 0.001 {
        discard;
    }
    let surface_radius = cloud_geometry.z + cloud_layer.x;
    let surface_roots = ray_sphere_roots(ray_origin, ray_direction, surface_radius);
    if surface_roots.y <= 0.0 {
        discard;
    }
    let surface_position = ray_origin + ray_direction * surface_roots.y;
    let noise = sample_cloud_surface(surface_position);
    let body = cloud_coverage(noise);
    let horizon_fade = smoothstep(0.008, 0.12, ray_direction.y);
    let opacity = body * (0.34 + cloud_shape.y * 0.42) * horizon_fade;
    if opacity <= 0.002 {
        discard;
    }

    let kind = u32(cloud_shape.z + 0.5);
    let storminess = select(0.0, 1.0, kind == 3u || kind == 7u);
    let sun_direction = normalize(cloud_lighting.xyz);
    let forward_highlight = pow(max(dot(ray_direction, sun_direction), 0.0), 12.0);
    let underside = mix(0.52, 1.06, noise.a);
    let clear_ambient = mix(
        vec3<f32>(0.36, 0.42, 0.49),
        vec3<f32>(0.70, 0.77, 0.85),
        0.42 + noise.b * 0.34,
    );
    let storm_ambient = mix(
        vec3<f32>(0.23, 0.25, 0.29),
        vec3<f32>(0.46, 0.49, 0.54),
        noise.b * 0.45,
    );
    let ambient = mix(clear_ambient, storm_ambient, storminess) * underside;
    let direct = cloud_spectral.xyz
        * cloud_lighting.w
        * cloud_motion.z
        * cloud_motion.w
        * (0.10 + forward_highlight * 0.26)
        * mix(1.0, 0.22, storminess);
    let color = ambient + direct;
    return vec4<f32>(color * opacity, opacity);
}
