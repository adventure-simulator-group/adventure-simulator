#import bevy_pbr::{
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> weather: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> weather_motion: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var<uniform> weather_terrain: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var weather_heightmap: texture_2d<f32>;

struct WeatherVertex {
    @location(0) position: vec3<f32>,
    @location(1) data: vec4<f32>,
    @location(2) corner: vec2<f32>,
}

struct WeatherVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) appearance: vec4<f32>,
    @location(2) variation: vec2<f32>,
}

fn decoded_height(texel: vec2<i32>) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(weather_heightmap));
    let sample = textureLoad(weather_heightmap, clamp(texel, vec2<i32>(0), dimensions - 1), 0);
    let encoded = round(sample.r * 255.0) + round(sample.g * 255.0) * 256.0;
    return mix(weather_terrain.z, weather_terrain.w, encoded / 65535.0);
}

fn terrain_height(world_xz: vec2<f32>) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(weather_heightmap));
    let uv = clamp(
        (world_xz + weather_terrain.xy) / max(weather_terrain.xy * 2.0, vec2<f32>(0.001)),
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    let grid = uv * vec2<f32>(dimensions - 1);
    let lower = vec2<i32>(floor(grid));
    let blend = fract(grid);
    let h00 = decoded_height(lower);
    let h10 = decoded_height(lower + vec2<i32>(1, 0));
    let h01 = decoded_height(lower + vec2<i32>(0, 1));
    let h11 = decoded_height(lower + vec2<i32>(1, 1));
    return mix(mix(h00, h10, blend.x), mix(h01, h11, blend.x), blend.y);
}

fn camera_anchor() -> vec2<f32> {
    return floor(view.world_position.xz / 2.0) * 2.0;
}

fn distributed_xz(data: vec4<f32>, radius: f32) -> vec2<f32> {
    let angle = data.x * 6.28318530718;
    let radial = sqrt(data.y) * radius;
    return camera_anchor() + vec2<f32>(cos(angle), sin(angle)) * radial;
}

@vertex
fn vertex(vertex: WeatherVertex) -> WeatherVertexOutput {
    var out: WeatherVertexOutput;
    out.corner = vertex.corner;
    out.variation = vec2<f32>(vertex.data.x, vertex.data.w);
    let kind = u32(weather.x + 0.5);
    let intensity = clamp(weather.y, 0.0, 1.0);
    let severe_surge = smoothstep(0.95, 1.0, intensity);
    let capacity_fraction = select(
        0.08 + intensity * 0.58 + select(0.0, severe_surge * 0.34, kind == 1u),
        0.04 + intensity * 0.43 + severe_surge * 0.40,
        kind == 3u,
    );
    if (kind != 4u && vertex.data.z > capacity_fraction) || kind == 0u {
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.appearance = vec4<f32>(0.0);
        out.variation = vec2<f32>(0.0);
        return out;
    }

    let time = globals.time + weather.w * 0.013;
    let radius = weather_motion.z;
    var centre_xz = distributed_xz(vertex.data, radius);
    var ground = terrain_height(centre_xz);
    var world_position = vec3<f32>(centre_xz.x, ground, centre_xz.y);

    if kind == 1u || kind == 2u {
        let is_snow = kind == 2u;
        let base_speed = select(18.0 + vertex.data.y * 9.0, 1.7 + vertex.data.y * 1.8, is_snow);
        let phase = fract(vertex.data.x * 0.71 + vertex.data.y * 0.37 - time * base_speed / weather_motion.w);
        let fall_age = 1.0 - phase;
        let wind_speed = mix(1.5, 11.0, weather.z);
        centre_xz += weather_motion.xy * wind_speed * fall_age;
        if is_snow {
            let flutter_phase = time * (0.75 + vertex.data.x * 1.1) + vertex.data.w * 1.73;
            let right = vec2<f32>(-weather_motion.y, weather_motion.x);
            centre_xz += right * sin(flutter_phase) * (0.25 + vertex.data.y * 0.55);
            centre_xz += weather_motion.xy * cos(flutter_phase * 0.61) * 0.18;
        }
        ground = terrain_height(centre_xz);
        world_position = vec3<f32>(centre_xz.x, ground + 0.08 + phase * weather_motion.w, centre_xz.y);

        let to_particle = normalize(world_position - view.world_position);
        if is_snow {
            let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), to_particle));
            let up = normalize(cross(to_particle, right));
            let rotation = time * (0.7 + vertex.data.y * 2.6) + vertex.data.w;
            let rotated = vec2<f32>(
                vertex.corner.x * cos(rotation) - vertex.corner.y * sin(rotation),
                vertex.corner.x * sin(rotation) + vertex.corner.y * cos(rotation),
            );
            let size = mix(0.025, 0.095, vertex.data.y * vertex.data.y);
            world_position += right * rotated.x * size + up * rotated.y * size;
            out.appearance = vec4<f32>(0.78, 0.88, 1.0, 0.54 + intensity * 0.28);
        } else {
            let velocity = normalize(vec3<f32>(
                weather_motion.x * wind_speed,
                -base_speed,
                weather_motion.y * wind_speed,
            ));
            let side = normalize(cross(to_particle, velocity));
            let width = mix(0.008, 0.018, vertex.data.y);
            let length = mix(0.28, 0.72, intensity) * mix(0.75, 1.2, vertex.data.x);
            world_position += side * vertex.corner.x * width + velocity * vertex.corner.y * length;
            // Rain is mostly transmitted background with a narrow lit filament,
            // not a solid blue rod. The fragment shader supplies the textured
            // coverage and normal-shaped highlight.
            out.appearance = vec4<f32>(0.72, 0.81, 0.88, 0.22 + intensity * 0.34);
        }
    } else if kind == 3u {
        let cycle = fract(time * (0.65 + intensity * 1.85) + vertex.data.x * 7.31 + vertex.data.y);
        let radius_metres = mix(0.025, 0.17, cycle) * mix(0.7, 1.3, vertex.data.y);
        world_position.y += 0.018;
        world_position = vec3<f32>(
            world_position.x + vertex.corner.x * radius_metres,
            world_position.y,
            world_position.z + vertex.corner.y * radius_metres,
        );
        let fade = 1.0 - smoothstep(0.18, 1.0, cycle);
        out.appearance = vec4<f32>(0.38, 0.62, 0.78, fade * (0.18 + intensity * 0.36));
    } else {
        // A handful of low-poly shells replace thousands of distant drops.
        // The non-linear spacing preserves a near curtain while adding several
        // progressively more distant layers for depth.
        let angle = vertex.data.x * 6.28318530718;
        let radial = vec2<f32>(cos(angle), sin(angle));
        let tangent = vec2<f32>(-radial.y, radial.x);
        let sheet_radius = 22.0 + pow(vertex.data.y, 1.35) * 68.0;
        let half_panel_width = sheet_radius * 0.235;
        let sheet_xz = camera_anchor() + radial * sheet_radius
            + tangent * vertex.corner.x * half_panel_width;
        world_position = vec3<f32>(
            sheet_xz.x,
            view.world_position.y + vertex.corner.y * 13.0,
            sheet_xz.y,
        );
        let sheet_strength = smoothstep(0.78, 1.0, intensity);
        let sheet_alpha = sheet_strength * (0.012 + severe_surge * 0.042)
            * mix(0.72, 1.0, vertex.data.y);
        out.appearance = vec4<f32>(0.72, 0.77, 0.80, sheet_alpha);
    }

    out.position = position_world_to_clip(world_position);
    return out;
}

@fragment
fn fragment(in: WeatherVertexOutput) -> @location(0) vec4<f32> {
    let kind = u32(weather.x + 0.5);
    var coverage = 0.0;
    var lighting = 1.0;
    if kind == 1u {
        // Two procedural texture frames reproduce the photographic streak
        // variation used by texture-based AAA rain without another texture
        // fetch or a scene-color refraction pass.
        let frame = select(0.0, 1.0, fract(in.variation.y * 0.754877666) > 0.5);
        let along = in.corner.y * 0.5 + 0.5;
        let phase = in.variation.x * 18.8495559 + frame * 1.91;
        let centre = sin(along * mix(8.0, 13.0, frame) + phase)
            * mix(0.025, 0.065, frame);
        let local_x = in.corner.x - centre;
        let end_taper = 1.0 - 0.46 * smoothstep(0.48, 1.0, abs(in.corner.y));
        let half_width = mix(0.52, 0.40, frame) * end_taper;
        let filament = 1.0 - smoothstep(half_width * 0.12, half_width, abs(local_x));
        let soft_ends = 1.0 - smoothstep(0.70, 1.0, abs(in.corner.y));
        let texture_breakup = 0.68 + 0.32
            * sin(along * mix(21.0, 31.0, frame) + phase * 1.73);
        let bead_y = in.corner.y + mix(0.34, -0.28, frame);
        let bead = (1.0 - smoothstep(0.0, 0.42, length(vec2<f32>(local_x * 1.7, bead_y))))
            * mix(0.18, 0.32, frame);
        coverage = max(filament * soft_ends * texture_breakup, bead * soft_ends);

        // Analytic tangent-space normal: a cheap equivalent of the normal map
        // Ubisoft used to keep transparent streaks responsive to lighting.
        let normal_x = clamp(local_x / max(half_width, 0.001), -1.0, 1.0);
        let normal_y = cos(along * mix(8.0, 13.0, frame) + phase) * 0.16;
        let normal_z = sqrt(max(0.0, 1.0 - normal_x * normal_x - normal_y * normal_y));
        let normal = normalize(vec3<f32>(normal_x, normal_y, normal_z));
        let key_light = normalize(vec3<f32>(-0.38, 0.62, 0.69));
        let diffuse = 0.58 + 0.42 * max(dot(normal, key_light), 0.0);
        let glint = pow(max(dot(normal, normalize(vec3<f32>(-0.18, 0.30, 0.94))), 0.0), 18.0);
        lighting = diffuse + glint * 0.42;
    } else if kind == 2u {
        let radius = length(in.corner);
        let soft_disc = 1.0 - smoothstep(0.45, 1.0, radius);
        let crystalline = 0.86 + 0.14 * cos(atan2(in.corner.y, in.corner.x) * 6.0);
        coverage = soft_disc * crystalline;
    } else if kind == 3u {
        let radius = length(in.corner);
        coverage = smoothstep(0.36, 0.68, radius) * (1.0 - smoothstep(0.70, 1.0, radius));
    } else {
        // Broad, blurred bands form an irregular falling curtain. This is
        // deliberately 2D: no 3D texture, volume integration, or blur pass.
        let phase = in.variation.y * 1.61803398875 + weather.w * 0.0017;
        let layer = fract(in.variation.y * 0.754877666);
        let falling = in.corner.y * 0.5 + 0.5
            + globals.time * mix(0.62, 0.88, layer);
        // Huge, sparse waves read as rain curtains rather than a repeating
        // screen-space texture: around fifteen times fewer bands, with lateral
        // variation stretched to roughly ten times its old scale.
        let frequency = mix(0.60, 0.87, layer);
        // Reconstruct a nearly continuous coordinate around the cylinder from
        // the panel centre and local quad coordinate. A few incommensurate
        // waves bend both edges without a noise texture or visible repetition.
        let sheet_u = fract(in.variation.x + in.corner.x * 0.03125);
        let wave_angle = sheet_u * 6.28318530718;
        let broad_bend = sin(wave_angle * 4.1 + phase) * 0.075
            + sin(wave_angle * 6.7 - phase * 0.73) * 0.032
            + sin(wave_angle * 11.3 + phase * 1.37) * 0.012;
        let raw_band_coordinate = falling * frequency + phase;
        let band_id = floor(raw_band_coordinate);
        let band_phase = fract(raw_band_coordinate + broad_bend);
        let thickness_shape = 0.5 + 0.30 * sin(wave_angle * 2.3 + band_id * 1.91)
            + 0.14 * sin(wave_angle * 5.9 - band_id * 0.83)
            + 0.06 * sin(wave_angle * 13.7 + phase);
        let leading_edge = smoothstep(
            0.012,
            mix(0.070, 0.125, clamp(thickness_shape, 0.0, 1.0)),
            band_phase,
        );
        let tail_end = mix(0.72, 0.97, clamp(thickness_shape, 0.0, 1.0));
        let long_tail = 1.0 - smoothstep(0.10, tail_end, band_phase);
        let blurred_band = leading_edge * long_tail;
        let breakup = 0.5 + 0.5 * sin(
            wave_angle * mix(0.55, 0.85, layer) + band_id * 2.17 + phase,
        );
        let broad_wave = 0.72 + 0.28 * sin(
            wave_angle * 0.23 + falling * 1.7 + phase,
        );
        let panel_edge = (1.0 - smoothstep(0.72, 1.0, abs(in.corner.x)))
            * (1.0 - smoothstep(0.78, 1.0, abs(in.corner.y)));
        let irregular_opacity = mix(0.46, 1.0, smoothstep(0.12, 0.88, breakup));
        coverage = blurred_band * irregular_opacity * broad_wave * panel_edge;
    }
    let alpha = in.appearance.a * coverage;
    if alpha < 0.006 {
        discard;
    }
    return vec4<f32>(in.appearance.rgb * lighting * alpha, alpha);
}
