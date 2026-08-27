// Composites the reduced-resolution offscreen cloud target into the main
// view. The dome mesh keeps the ordinary depth test working, so terrain and
// trees occlude clouds exactly as they occluded the legacy in-view shells;
// the fragment itself is a single premultiplied texture fetch.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var cloud_source: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var cloud_source_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    return textureSampleLevel(cloud_source, cloud_source_sampler, uv, 0.0);
}
