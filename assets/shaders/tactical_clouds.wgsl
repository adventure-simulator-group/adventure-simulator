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

fn hash13(position: vec3<f32>) -> f32 {
    var p = fract(position * 0.1031);
    p += dot(p, p.yzx + vec3<f32>(33.33));
    return fract((p.x + p.y) * p.z);
}

fn value_noise_3d(position: vec3<f32>) -> f32 {
    let cell = floor(position);
    let local = fract(position);
    let blend = local * local * (vec3<f32>(3.0) - 2.0 * local);
    let n000 = hash13(cell + vec3<f32>(0.0, 0.0, 0.0));
    let n100 = hash13(cell + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash13(cell + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash13(cell + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash13(cell + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash13(cell + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash13(cell + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash13(cell + vec3<f32>(1.0, 1.0, 1.0));
    let z0 = mix(mix(n000, n100, blend.x), mix(n010, n110, blend.x), blend.y);
    let z1 = mix(mix(n001, n101, blend.x), mix(n011, n111, blend.x), blend.y);
    return mix(z0, z1, blend.z);
}

fn fbm(position: vec3<f32>) -> f32 {
    let first = value_noise_3d(position);
    let second = value_noise_3d(position * 2.03 + vec3<f32>(17.1, 3.7, 11.9));
    let third = value_noise_3d(position * 4.01 + vec3<f32>(7.3, 19.1, 2.9));
    return first * 0.57 + second * 0.29 + third * 0.14;
}

fn cloud_profile(height: f32, family: f32, noise_value: f32) -> f32 {
    if family < 0.5 {
        let bottom = smoothstep(0.0, 0.08, height);
        let top = 1.0 - smoothstep(0.58 + noise_value * 0.2, 1.0, height);
        return bottom * top;
    }
    if family < 1.5 {
        return smoothstep(0.0, 0.13, height) * (1.0 - smoothstep(0.72, 1.0, height));
    }
    if family < 2.5 {
        let ribbon = 1.0 - smoothstep(0.08, 0.34, abs(height - 0.52));
        return ribbon * (0.55 + noise_value * 0.45);
    }
    let bottom = smoothstep(0.0, 0.035, height);
    let top = 1.0 - smoothstep(0.68 + noise_value * 0.12, 1.0, height);
    return bottom * top;
}

fn sample_density(world_position: vec3<f32>) -> f32 {
    let height = (world_position.y - cloud_layer.x) / cloud_layer.y;
    if height <= 0.0 || height >= 1.0 {
        return 0.0;
    }
    let wind_position = world_position.xz + cloud_motion.xy;
    let seed = cloud_shape.w;
    var coordinate = vec3<f32>(
        wind_position.x * cloud_layer.z + seed * 0.013,
        height * 1.8 + seed * 0.007,
        wind_position.y * cloud_layer.z - seed * 0.011,
    );
    if cloud_shape.z > 1.5 && cloud_shape.z < 2.5 {
        coordinate.x *= 0.32;
        coordinate.z *= 1.8;
    }
    let broad = fbm(coordinate * vec3<f32>(0.62, 0.45, 0.62));
    let detail = fbm(coordinate * 2.15 + vec3<f32>(9.7, 1.3, 4.1));
    let profile = cloud_profile(height, cloud_shape.z, broad);
    let threshold = 0.82 - cloud_shape.x * 0.38;
    let body = smoothstep(threshold, threshold + 0.16, broad * 0.72 + detail * 0.28);
    let edge_erosion = mix(0.72, 1.0, smoothstep(0.2, 0.8, detail));
    return body * profile * edge_erosion * cloud_shape.y;
}

fn henyey_greenstein(cosine: f32, eccentricity: f32) -> f32 {
    let g2 = eccentricity * eccentricity;
    let denominator = pow(max(1.0 + g2 - 2.0 * eccentricity * cosine, 0.04), 1.5);
    return (1.0 - g2) / denominator;
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
    if ray_direction.y <= 0.008 {
        discard;
    }
    let layer_top = cloud_layer.x + cloud_layer.y;
    var trace_start = max((cloud_layer.x - ray_origin.y) / ray_direction.y, 0.0);
    var trace_end = (layer_top - ray_origin.y) / ray_direction.y;
    trace_end = min(trace_end, cloud_layer.w);
    if trace_end <= trace_start {
        discard;
    }
    let step_length = (trace_end - trace_start) / 12.0;
    var distance = trace_start + step_length * 0.5;
    var optical_depth = 0.0;
    let sun_direction = normalize(cloud_lighting.xyz);
    let forward_phase = min(henyey_greenstein(dot(ray_direction, sun_direction), 0.55), 4.0);

    for (var step = 0u; step < 12u; step += 1u) {
        let position = ray_origin + ray_direction * distance;
        let density = sample_density(position);
        if density > 0.002 {
            optical_depth += density * step_length * 0.00165;
            if optical_depth > 5.0 {
                break;
            }
        }
        distance += step_length;
    }

    let opacity = 1.0 - exp(-min(optical_depth, 8.0));
    if opacity < 0.002 {
        discard;
    }
    let storminess = smoothstep(2.5, 3.0, cloud_shape.z);
    let underside = 1.0 - smoothstep(0.04, 0.55, ray_direction.y);
    let body_color = mix(
        vec3<f32>(0.78, 0.83, 0.89),
        vec3<f32>(0.22, 0.25, 0.30),
        clamp(underside * (0.5 + opacity * 0.42) + storminess * 0.28, 0.0, 0.9),
    );
    let silver_lining = pow(1.0 - opacity, 2.2)
        * forward_phase
        * cloud_motion.z
        * cloud_motion.w;
    let sun_color = vec3<f32>(1.0, 0.88, 0.68);
    let cloud_color = body_color + sun_color * silver_lining * 0.42;
    // Premultiplied output preserves soft cloud edges over the atmosphere and
    // avoids the dark fringe produced by straight-alpha filtering.
    return vec4<f32>(cloud_color * opacity * cloud_lighting.w, opacity);
}
