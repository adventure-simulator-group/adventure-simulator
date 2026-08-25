// Centered incapacitation gauge drawn entirely in the fragment shader, as an
// ordinary (custom-shaded) Bevy UI node instead of an EGUI immediate-mode
// overlay. Segment order and colors mirror `incapacitation_wheel_segments`
// in `ui.rs` and the `--incap-*` custom properties in the strategic web
// app's `strategic.css`.
#import bevy_ui::ui_vertex_output::UiVertexOutput

// xyz = linear color, w = visible fraction of the ring circumference.
@group(1) @binding(0)
var<uniform> segments: array<vec4<f32>, 9>;
// x = inner radius, y = outer radius, z = edge softness, w = 1.0 when fully
// incapacitated (draws the alarm glow), all radii in UV units where 1.0 is
// the distance from the node center to its edge.
@group(1) @binding(1)
var<uniform> geometry: vec4<f32>;

const PI: f32 = 3.14159265358979323846;
const TAU: f32 = 6.28318530717958647692;
// Matches the reticle wheel's former `Color32::from_rgba_unmultiplied(0xc8, 0x47, 0x47, 70)`.
const HALO_COLOR: vec4<f32> = vec4<f32>(0.7843137, 0.2784314, 0.2784314, 0.27450981);
const HALO_PAD: f32 = 0.14;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let centered = in.uv - vec2<f32>(0.5, 0.5);
    let dist = length(centered) * 2.0;

    let inner = geometry.x;
    let outer = geometry.y;
    let edge = max(geometry.z, 0.0005);
    let incapacitated = geometry.w > 0.5;

    let ring_mask = smoothstep(inner - edge, inner + edge, dist)
        - smoothstep(outer - edge, outer + edge, dist);

    if ring_mask > 0.0 {
        // 0 turns at 12 o'clock, increasing clockwise, matching the old wheel.
        var theta = atan2(centered.y, centered.x) + PI * 0.5;
        if theta < 0.0 {
            theta += TAU;
        }
        let t = theta / TAU;

        var cumulative = 0.0;
        for (var i = 0u; i < 9u; i += 1u) {
            let amount = segments[i].w;
            let next = cumulative + amount;
            if amount > 0.0 && t >= cumulative && t < next {
                return vec4<f32>(segments[i].rgb, ring_mask);
            }
            cumulative = next;
        }
    }

    // Soft alarm glow around the ring once fully incapacitated. Nothing
    // above ever returned for this pixel, so at 0% incapacitation (every
    // segment amount is 0, and this flag is unset) every pixel falls
    // through to `discard`, which is what makes the wheel disappear
    // entirely rather than needing separate visibility bookkeeping.
    if incapacitated {
        let halo_mask = smoothstep(inner - HALO_PAD - edge, inner - HALO_PAD + edge, dist)
            - smoothstep(outer + HALO_PAD - edge, outer + HALO_PAD + edge, dist);
        if halo_mask > 0.0 {
            return vec4<f32>(HALO_COLOR.rgb, HALO_COLOR.a * halo_mask);
        }
    }

    discard;
}
