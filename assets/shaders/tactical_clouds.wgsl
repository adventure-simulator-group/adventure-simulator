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
/// Solar chroma derived from altitude; kept separate from scalar illuminance.
@group(#{MATERIAL_BIND_GROUP}) @binding(4)
var<uniform> cloud_spectral: vec4<f32>;
/// Fixed scene anchor X/Z, shell curvature radius, aerial extinction.
@group(#{MATERIAL_BIND_GROUP}) @binding(5)
var<uniform> cloud_geometry: vec4<f32>;

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
    let fourth = value_noise_3d(position * 8.07 + vec3<f32>(13.7, 5.3, 23.9));
    return first * 0.52 + second * 0.27 + third * 0.14 + fourth * 0.07;
}

/// The coarse ray march only needs to decide whether a large interval could
/// contain cloud. Two broad octaves preserve the major coverage cells while
/// avoiding the detailed density path's warp and secondary erosion noise.
fn fbm_coarse(position: vec3<f32>) -> f32 {
    let first = value_noise_3d(position);
    let second = value_noise_3d(position * 2.03 + vec3<f32>(17.1, 3.7, 11.9));
    return first * 0.66 + second * 0.34;
}

fn cloud_profile(height: f32, family: f32, noise_value: f32) -> f32 {
    let kind = u32(family + 0.5);
    if kind == 0u {
        let bottom = smoothstep(0.0, 0.08, height);
        let top = 1.0 - smoothstep(0.58 + noise_value * 0.2, 1.0, height);
        return bottom * top;
    }
    if kind == 1u {
        return smoothstep(0.0, 0.13, height) * (1.0 - smoothstep(0.72, 1.0, height));
    }
    if kind == 2u {
        let ribbon = 1.0 - smoothstep(0.08, 0.34, abs(height - 0.52));
        return ribbon * (0.55 + noise_value * 0.45);
    }
    if kind == 3u {
        let tower = smoothstep(0.0, 0.035, height)
            * (1.0 - smoothstep(0.78 + noise_value * 0.12, 1.0, height));
        let anvil = smoothstep(0.68, 0.78, height) * (1.0 - smoothstep(0.9, 1.0, height));
        return max(tower, anvil * 0.85);
    }
    if kind == 4u {
        return smoothstep(0.0, 0.06, height) * (1.0 - smoothstep(0.82, 1.0, height));
    }
    if kind == 5u {
        let middle = 1.0 - smoothstep(0.18, 0.46, abs(height - 0.5));
        return middle * (0.7 + noise_value * 0.3);
    }
    if kind == 6u {
        return smoothstep(0.0, 0.12, height) * (1.0 - smoothstep(0.82, 1.0, height));
    }
    if kind == 7u {
        return smoothstep(0.0, 0.04, height) * (1.0 - smoothstep(0.9, 1.0, height));
    }
    if kind == 8u {
        let beads = 1.0 - smoothstep(0.12, 0.34, abs(height - 0.52));
        return beads * (0.65 + noise_value * 0.35);
    }
    if kind == 9u {
        return 1.0 - smoothstep(0.24, 0.48, abs(height - 0.5));
    }
    let bottom = smoothstep(0.0, 0.05, height);
    let top = 1.0 - smoothstep(0.72 + noise_value * 0.18, 1.0, height);
    return bottom * top;
}

fn shell_center() -> vec3<f32> {
    return vec3<f32>(
        cloud_geometry.x,
        -cloud_geometry.z,
        cloud_geometry.y,
    );
}

fn altitude_in_shell(world_position: vec3<f32>) -> f32 {
    return length(world_position - shell_center()) - cloud_geometry.z;
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

fn sample_density(world_position: vec3<f32>) -> f32 {
    let height = (altitude_in_shell(world_position) - cloud_layer.x) / cloud_layer.y;
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
    let kind = u32(cloud_shape.z + 0.5);
    if kind == 2u {
        coordinate.x *= 0.32;
        coordinate.z *= 1.8;
    } else if kind == 5u || kind == 8u {
        coordinate.x *= 1.75;
        coordinate.z *= 1.75;
    } else if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        coordinate.x *= 0.58;
        coordinate.z *= 0.58;
    }
    let warp = vec3<f32>(
        value_noise_3d(coordinate * 0.37 + vec3<f32>(2.1, 7.3, 11.7)) - 0.5,
        0.0,
        value_noise_3d(coordinate * 0.37 + vec3<f32>(17.9, 3.1, 5.7)) - 0.5,
    );
    coordinate += warp * 0.85;
    let broad = fbm(coordinate * vec3<f32>(0.82, 0.62, 0.82));
    let detail = fbm(coordinate * 3.35 + vec3<f32>(9.7, 1.3, 4.1));
    let profile = cloud_profile(height, cloud_shape.z, broad);
    var threshold = 0.78 - cloud_shape.x * 0.34;
    if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        threshold -= 0.08;
    }
    if kind == 0u {
        threshold += height * 0.07;
    } else if kind == 3u {
        // Convective cores narrow with altitude. The upper-deck relaxation
        // leaves room for a broader anvil without extending every low-level
        // cell into an implausible full-height column.
        threshold += height * 0.24 - smoothstep(0.68, 0.82, height) * 0.11;
    } else if kind == 10u {
        threshold += height * 0.14;
    }
    var body = 0.0;
    if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        // Layer clouds keep a continuous body while fine noise modulates their
        // optical depth. This prevents overcast from becoming a featureless
        // alpha sheet at high coverage.
        body = smoothstep(threshold, threshold + 0.17, broad)
            * mix(0.58, 1.08, detail);
    } else {
        // Erode cellular cloud boundaries with higher-frequency detail while
        // leaving their dense cores intact.
        let eroded = broad - (1.0 - detail) * 0.18;
        body = smoothstep(threshold, threshold + 0.14, eroded);
    }
    return clamp(body * profile * cloud_shape.y, 0.0, 1.35);
}

/// A deliberately conservative occupancy estimate for the empty-space search.
/// It can enter fine marching early, but must not reject small cellular cloud
/// features solely because detail erosion or domain warping changes their edge.
fn sample_density_coarse(world_position: vec3<f32>) -> f32 {
    let height = (altitude_in_shell(world_position) - cloud_layer.x) / cloud_layer.y;
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
    let kind = u32(cloud_shape.z + 0.5);
    if kind == 2u {
        coordinate.x *= 0.32;
        coordinate.z *= 1.8;
    } else if kind == 5u || kind == 8u {
        coordinate.x *= 1.75;
        coordinate.z *= 1.75;
    } else if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        coordinate.x *= 0.58;
        coordinate.z *= 0.58;
    }

    let broad = fbm_coarse(coordinate * vec3<f32>(0.82, 0.62, 0.82));
    let profile = cloud_profile(height, cloud_shape.z, broad);
    var threshold = 0.78 - cloud_shape.x * 0.34;
    if kind == 4u || kind == 6u || kind == 7u || kind == 9u {
        threshold -= 0.08;
    }
    if kind == 0u {
        threshold += height * 0.07;
    } else if kind == 3u {
        threshold += height * 0.24 - smoothstep(0.68, 0.82, height) * 0.11;
    } else if kind == 10u {
        threshold += height * 0.14;
    }

    // The relaxed threshold covers the detailed path's erosion and warp. A
    // false positive merely costs a few fine steps; a false negative would
    // systematically erase thin cirrus and small cumulus fragments.
    let conservative_threshold = threshold - 0.22;
    let body = smoothstep(
        conservative_threshold - 0.08,
        conservative_threshold + 0.12,
        broad,
    );
    return clamp(body * profile * cloud_shape.y, 0.0, 1.35);
}

fn sunlight_transmittance(position: vec3<f32>, sun_direction: vec3<f32>) -> f32 {
    var optical_depth = 0.0;
    var distance = 420.0;
    // Two exponentially-spaced probes retain the broad self-shadow gradient
    // while avoiding a third detailed density evaluation for every refresh.
    for (var step = 0u; step < 2u; step += 1u) {
        let sample_position = position + sun_direction * distance;
        optical_depth += sample_density(sample_position) * distance * 0.00052;
        distance *= 1.72;
    }
    return exp(-min(optical_depth, 6.0));
}

// Keep all march budgets explicit and low enough to stay within WebGPU's
// predictable fixed-loop path. Fine steps are half a coarse interval, so the
// 48-step cap can still traverse a complete 24-interval shell once occupied.
const CLOUD_COARSE_INTERVALS = 24.0;
const CLOUD_FINE_STEP_SCALE = 0.5;
const CLOUD_MAX_MARCH_STEPS = 48u;
const SUNLIGHT_TRANSMITTANCE_INTERVAL = 4u;

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
    if ray_direction.y < -0.001 {
        discard;
    }

    let centre = shell_center();
    let origin_radius = length(ray_origin - centre);
    let inner_radius = cloud_geometry.z + cloud_layer.x;
    let outer_radius = inner_radius + cloud_layer.y;
    let outer_roots = ray_sphere_roots(ray_origin, ray_direction, outer_radius);
    if outer_roots.y <= 0.0 {
        discard;
    }

    var trace_start = 0.0;
    var trace_end = outer_roots.y;
    if origin_radius < inner_radius {
        // The ordinary grounded-camera case: begin where the ray exits the
        // empty space below the cloud shell.
        let inner_roots = ray_sphere_roots(ray_origin, ray_direction, inner_radius);
        trace_start = max(inner_roots.y, 0.0);
    } else if origin_radius > outer_radius {
        // Retain a well-defined interval for diagnostic cameras outside the
        // shell, even though tactical play normally remains below it.
        trace_start = max(outer_roots.x, 0.0);
        let inner_roots = ray_sphere_roots(ray_origin, ray_direction, inner_radius);
        if inner_roots.x > trace_start {
            trace_end = inner_roots.x;
        }
    }
    trace_end = min(trace_end, cloud_layer.w);
    if trace_end <= trace_start {
        discard;
    }

    // Empty air is searched in large steps. Once density is found, backtrack
    // and integrate in half-sized steps until the ray is clear again.
    // Pixel-stable jitter prevents the curved shell from resolving into
    // coherent marching bands without requiring more samples everywhere.
    let coarse_step = (trace_end - trace_start) / CLOUD_COARSE_INTERVALS;
    let fine_step = coarse_step * CLOUD_FINE_STEP_SCALE;
    let ray_jitter = hash13(vec3<f32>(floor(in.position.xy), cloud_shape.w));
    var step_length = coarse_step;
    var distance = trace_start + coarse_step * ray_jitter;
    var fine_marching = false;
    var fine_empty_steps = 0u;
    // Detailed self-shadowing is materially lower frequency than the fine
    // integration samples. Start each occupied segment with a fresh result,
    // then reuse it for three immediately following occupied samples.
    var occupied_fine_steps = 0u;
    var sun_visibility = 1.0;
    var transmittance = 1.0;
    var visible_opacity = 0.0;
    var radiance = vec3<f32>(0.0);
    let sun_direction = normalize(cloud_lighting.xyz);
    let forward_phase = min(henyey_greenstein(dot(ray_direction, sun_direction), 0.55), 4.0);
    let kind = u32(cloud_shape.z + 0.5);
    let storminess = select(0.0, 1.0, kind == 3u || kind == 7u);
    let sun_color = cloud_spectral.xyz;
    let horizon_haze = 1.0 - smoothstep(0.02, 0.22, ray_direction.y);
    let aerial_extinction = cloud_geometry.w * mix(0.65, 4.5, horizon_haze);

    for (var step = 0u; step < CLOUD_MAX_MARCH_STEPS; step += 1u) {
        if distance >= trace_end {
            break;
        }
        let position = ray_origin + ray_direction * distance;
        let distance_fade = 1.0 - smoothstep(cloud_layer.w * 0.68, cloud_layer.w, distance);
        var density: f32;
        var density_threshold: f32;
        if fine_marching {
            density = sample_density(position);
            density_threshold = 0.002;
        } else {
            density = sample_density_coarse(position);
            density_threshold = 0.0004;
        }
        density *= distance_fade;
        if density > density_threshold {
            if !fine_marching {
                fine_marching = true;
                fine_empty_steps = 0u;
                step_length = fine_step;
                distance = max(distance - coarse_step, trace_start) + fine_step * ray_jitter;
                continue;
            }
            // Deep precipitating clouds extinguish the warm atmospheric
            // aureole behind them. Without this storm-specific optical-depth
            // boost, a physically neutral core could still appear olive from
            // the background leaking through its alpha.
            let extinction_scale = mix(1.0, 1.65, storminess);
            let sample_opacity = 1.0
                - exp(-density * step_length * 0.00142 * extinction_scale);
            let height = clamp(
                (altitude_in_shell(position) - cloud_layer.x) / cloud_layer.y,
                0.0,
                1.0,
            );
            if occupied_fine_steps % SUNLIGHT_TRANSMITTANCE_INTERVAL == 0u {
                sun_visibility = sunlight_transmittance(position, sun_direction);
            }
            occupied_fine_steps += 1u;
            let powder = 1.0 - exp(-density * 2.4);
            let clear_ambient = mix(
                vec3<f32>(0.30, 0.36, 0.43),
                vec3<f32>(0.68, 0.75, 0.83),
                0.28 + height * 0.52,
            );
            // Multiple scattering in a deep storm core trends toward a
            // neutral gray-blue. It should not inherit either golden direct
            // sunlight or a saturated sky tint after many scattering events.
            let neutral_storm_ambient = mix(
                vec3<f32>(0.31, 0.33, 0.36),
                vec3<f32>(0.53, 0.55, 0.58),
                0.22 + height * 0.46,
            );
            let ambient_color = mix(clear_ambient, neutral_storm_ambient, storminess * 0.88)
                * mix(0.52, 1.0, sqrt(sun_visibility))
                * (1.0 - density * (0.12 + storminess * 0.16));
            let sheet_cloud = kind == 4u || kind == 6u || kind == 7u || kind == 9u;
            let underside_variation = value_noise_3d(vec3<f32>(
                position.x * 0.00034 + cloud_shape.w * 0.017,
                cloud_shape.w * 0.009,
                position.z * 0.00034 - cloud_shape.w * 0.013,
            ));
            let ambient_variation = select(1.0, mix(0.46, 1.04, underside_variation), sheet_cloud);
            let phase_light = 0.14 + 0.12 * forward_phase;
            let storm_direct = select(1.0, 0.08 + sun_visibility * 0.22, storminess > 0.5);
            let direct_light = sun_color
                * cloud_motion.z
                * cloud_motion.w
                * sun_visibility
                * phase_light
                * (0.45 + powder * 0.55)
                * storm_direct;
            let sample_color = ambient_color * ambient_variation + direct_light;
            let weight = transmittance * sample_opacity;
            let aerial_transmission = exp(-distance * aerial_extinction);
            radiance += sample_color * weight * aerial_transmission;
            visible_opacity += weight * aerial_transmission;
            transmittance *= 1.0 - sample_opacity;
            fine_empty_steps = 0u;
            if transmittance < 0.012 {
                break;
            }
        } else if fine_marching {
            fine_empty_steps += 1u;
            if fine_empty_steps >= 3u {
                fine_marching = false;
                fine_empty_steps = 0u;
                occupied_fine_steps = 0u;
                step_length = coarse_step;
            }
        }
        distance += step_length;
    }

    let cloud_opacity = 1.0 - transmittance;
    let opacity = clamp(visible_opacity, 0.0, 1.0);
    if opacity < 0.002 {
        discard;
    }
    let silver_lining = pow(1.0 - cloud_opacity, 2.2)
        * forward_phase
        * cloud_motion.z
        * cloud_motion.w
        * (1.0 - storminess * 0.55);
    radiance += sun_color * silver_lining * opacity * 0.18;
    // Premultiplied output preserves soft cloud edges over the atmosphere and
    // avoids the dark fringe produced by straight-alpha filtering.
    return vec4<f32>(radiance * cloud_lighting.w, opacity);
}
