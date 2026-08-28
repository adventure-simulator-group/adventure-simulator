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
var cloud_baked_texture_a: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7)
var cloud_sampler_a: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(8)
var cloud_baked_texture_b: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9)
var cloud_sampler_b: sampler;

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

fn cloud_coordinate(ray_direction: vec3<f32>) -> vec2<f32> {
    let azimuth = atan2(ray_direction.z, ray_direction.x) / (2.0 * 3.14159265359) + 0.5;
    let elevation = asin(clamp(ray_direction.y, 0.0, 1.0)) / (0.5 * 3.14159265359);
    return vec2<f32>(azimuth, elevation);
}

/// Move a dome ray through the representative cloud altitude before sampling
/// an endpoint. This removes known wind translation, leaving only formation
/// and erosion for the optical interpolation to dissolve.
fn wind_compensated_direction(
    ray_direction: vec3<f32>,
    displacement: vec2<f32>,
) -> vec3<f32> {
    let reference_eye = vec3<f32>(0.0, cloud_shape.w, 0.0);
    let radius = cloud_geometry.z + cloud_layer.x;
    let roots = ray_sphere_roots(reference_eye, ray_direction, radius);
    if roots.y <= 0.0 {
        return ray_direction;
    }
    let point = reference_eye + ray_direction * roots.y;
    return normalize(point + vec3<f32>(displacement.x, 0.0, displacement.y) - reference_eye);
}

fn sample_cloud_surface(ray_direction: vec3<f32>) -> vec4<f32> {
    let blend = clamp(cloud_shape.y, 0.0, 1.0);
    let interval = cloud_shape.z;
    let wind = cloud_motion.xy;
    let direction_a = wind_compensated_direction(
        ray_direction,
        -wind * (blend * interval),
    );
    let direction_b = wind_compensated_direction(
        ray_direction,
        wind * ((1.0 - blend) * interval),
    );
    let baked_a = textureSample(
        cloud_baked_texture_a,
        cloud_sampler_a,
        cloud_coordinate(direction_a),
    );
    let baked_b = textureSample(
        cloud_baked_texture_b,
        cloud_sampler_b,
        cloud_coordinate(direction_b),
    );

    // Interpolate attenuation rather than alpha/transmission directly. This
    // preserves the exponential optical response through the handoff.
    let opacity_depth_a = -log(max(1.0 - baked_a.r, 0.0001));
    let opacity_depth_b = -log(max(1.0 - baked_b.r, 0.0001));
    let transmission_depth_a = -log(max(baked_a.b, 0.0001));
    let transmission_depth_b = -log(max(baked_b.b, 0.0001));
    return vec4<f32>(
        1.0 - exp(-mix(opacity_depth_a, opacity_depth_b, blend)),
        mix(baked_a.g, baked_b.g, blend),
        exp(-mix(transmission_depth_a, transmission_depth_b, blend)),
        mix(baked_a.a, baked_b.a, blend),
    );
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
    let baked = sample_cloud_surface(ray_direction);
    // R is the complete transmittance result for this exact dome ray. The
    // bake has already integrated its slanted path through every deck.
    let ray_opacity = baked.r;
    let storminess = cloud_shape.x;
    let horizon_fade = smoothstep(0.008, 0.12, ray_direction.y);
    let opacity = ray_opacity * horizon_fade;
    if opacity <= 0.002 {
        discard;
    }

    let sun_direction = normalize(cloud_lighting.xyz);
    let forward_highlight = pow(max(dot(ray_direction, sun_direction), 0.0), 12.0);
    // The bake retains local underside density instead of averaging it away;
    // exaggerating that retained range recovers cellular interior contrast.
    let underside = mix(0.16, 1.18, baked.a);
    let clear_ambient = mix(
        vec3<f32>(0.36, 0.42, 0.49),
        vec3<f32>(0.70, 0.77, 0.85),
        0.28 + baked.g * 0.54,
    );
    let storm_ambient = mix(
        vec3<f32>(0.23, 0.25, 0.29),
        vec3<f32>(0.46, 0.49, 0.54),
        baked.g * 0.62,
    );
    let ambient = mix(clear_ambient, storm_ambient, storminess)
        * underside
        * mix(1.0, 0.62 + baked.b * 0.38, storminess);
    let direct = cloud_spectral.xyz
        * cloud_lighting.w
        * cloud_motion.z
        * cloud_motion.w
        * baked.b
        * (0.10 + forward_highlight * 0.26)
        * mix(1.0, 0.22, storminess);
    let color = ambient + direct;
    return vec4<f32>(color * opacity, opacity);
}
