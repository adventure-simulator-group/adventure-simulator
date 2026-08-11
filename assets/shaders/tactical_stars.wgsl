#import bevy_pbr::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> equatorial_to_world: mat4x4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> star_settings: vec4<f32>;

struct StarVertex {
    @location(0) position: vec3<f32>,
    @location(1) direction: vec3<f32>,
    @location(2) color_magnitude: vec4<f32>,
    @location(3) corner: vec2<f32>,
}

struct StarVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color_magnitude: vec4<f32>,
    @location(1) corner: vec2<f32>,
}

@vertex
fn vertex(vertex: StarVertex) -> StarVertexOutput {
    var out: StarVertexOutput;
    let direction = normalize((equatorial_to_world * vec4<f32>(vertex.direction, 0.0)).xyz);
    if direction.y <= -0.012 || star_settings.x <= 0.0001 {
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color_magnitude = vec4<f32>(0.0);
        out.corner = vertex.corner;
        return out;
    }

    let world_position = view.world_position + direction * star_settings.w;
    var clip = position_world_to_clip(world_position);
    let bright_radius = clamp((2.5 - vertex.color_magnitude.w) * 0.34, 0.0, 1.8);
    // Clip-space expansion by viewport dimensions makes this an exact
    // physical-pixel radius. Higher-resolution displays resolve a denser,
    // finer field instead of magnifying a fixed angular star texture.
    let radius_pixels = (0.85 + bright_radius) * star_settings.y;
    let pixel_offset = vertex.corner * radius_pixels * 2.0 / view.viewport.zw * clip.w;
    clip = vec4<f32>(clip.xy + pixel_offset, clip.zw);
    out.position = clip;
    out.color_magnitude = vertex.color_magnitude;
    out.corner = vertex.corner;
    return out;
}

@fragment
fn fragment(in: StarVertexOutput) -> @location(0) vec4<f32> {
    let radius_squared = dot(in.corner, in.corner);
    if radius_squared > 1.0 {
        discard;
    }
    let profile = exp(-radius_squared * 3.8);
    let flux = pow(10.0, -0.4 * in.color_magnitude.w);
    let radiance = star_settings.x * star_settings.z * flux * profile;
    return vec4<f32>(in.color_magnitude.rgb * radiance, 1.0);
}
