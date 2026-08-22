use crate::data::Scan;
use crate::data::gather::Gather;
use crate::data::gpu::buffer::{Buffer, BufferDefinition};
use crate::data::gpu::compute::{
    GatherDefinition, MapDefinition, ScanDefinition, StreamDefinition,
};
use crate::globals::WgpuContext;
use anyhow::Result;

use crate::data::PassParameter;
use crate::data::gpu::parameters::PassParameters;
use crate::data::gpu::resource::GpuResource;

#[derive(Clone, Debug)]
pub struct DualContouringDefinition {
    pub vertex_count_def: GatherDefinition,
    pub index_count_def: GatherDefinition,
    pub scan_def: ScanDefinition,
    pub vertex_stream_def: StreamDefinition,
    pub index_stream_def: StreamDefinition,
    pub sync_indirect_def: MapDefinition,
    pub deinterleave_pipeline: crate::data::gpu::compute::ComputePipeline,
}

impl PartialEq for DualContouringDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.vertex_count_def == other.vertex_count_def
            && self.index_count_def == other.index_count_def
            && self.scan_def == other.scan_def
            && self.vertex_stream_def == other.vertex_stream_def
            && self.index_stream_def == other.index_stream_def
            && self.sync_indirect_def == other.sync_indirect_def
    }
}

impl DualContouringDefinition {
    pub fn new(context: &WgpuContext) -> Result<Self> {
        let params_struct = r#"
struct Params {
    grid_x: u32,
    grid_y: u32,
    grid_z: u32,
    max_vertices: u32,
    max_indices: u32,
    front_mode: u32,
    threshold: f32,
    scale_x: f32,
    scale_y: f32,
    scale_z: f32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
}
"#;

        let vertex_count_wgsl = format!(
            r#"
{params_struct}
@group(0) @binding(2) var<uniform> params: Params;

fn get_sdf(x: i32, y: i32, z: i32) -> f32 {{
    let ux = clamp(u32(x), 0u, params.grid_x - 1u);
    let uy = clamp(u32(y), 0u, params.grid_y - 1u);
    let uz = clamp(u32(z), 0u, params.grid_z - 1u);
    return textureLoad(input, vec3<u32>(ux, uy, uz), 0).x;
}}

fn gather(index: u32) -> u32 {{
    let grid_xy = params.grid_x * params.grid_y;
    let z = index / grid_xy;
    let y = (index % grid_xy) / params.grid_x;
    let x = index % params.grid_x;
    let id0 = vec3<u32>(x, y, z);

    if (id0.x == 0u || id0.y == 0u || id0.z == 0u || id0.x >= params.grid_x - 2u || id0.y >= params.grid_y - 2u || id0.z >= params.grid_z - 2u) {{
        return 0u;
    }}

    let c0 = vec3<i32>(id0);
    let c1 = c0 + vec3<i32>(1, 0, 0);
    let c2 = c0 + vec3<i32>(1, 1, 0);
    let c3 = c0 + vec3<i32>(0, 1, 0);
    let c4 = c0 + vec3<i32>(0, 0, 1);
    let c5 = c0 + vec3<i32>(1, 0, 1);
    let c6 = c0 + vec3<i32>(1, 1, 1);
    let c7 = c0 + vec3<i32>(0, 1, 1);

    let v0 = get_sdf(c0.x, c0.y, c0.z);
    let v1 = get_sdf(c1.x, c1.y, c1.z);
    let v2 = get_sdf(c2.x, c2.y, c2.z);
    let v3 = get_sdf(c3.x, c3.y, c3.z);
    let v4 = get_sdf(c4.x, c4.y, c4.z);
    let v5 = get_sdf(c5.x, c5.y, c5.z);
    let v6 = get_sdf(c6.x, c6.y, c6.z);
    let v7 = get_sdf(c7.x, c7.y, c7.z);

    let s0 = v0 < params.threshold;
    let s1 = v1 < params.threshold;
    let s2 = v2 < params.threshold;
    let s3 = v3 < params.threshold;
    let s4 = v4 < params.threshold;
    let s5 = v5 < params.threshold;
    let s6 = v6 < params.threshold;
    let s7 = v7 < params.threshold;

    let active_edges = 
        (s0 != s1) || (s1 != s2) || (s2 != s3) || (s3 != s0) ||
        (s4 != s5) || (s5 != s6) || (s6 != s7) || (s7 != s4) ||
        (s0 != s4) || (s1 != s5) || (s2 != s6) || (s3 != s7);

    if (active_edges) {{
        return 1u;
    }}
    return 0u;
}}
"#,
            params_struct = params_struct
        );

        let index_count_wgsl = format!(
            r#"
{params_struct}
@group(0) @binding(2) var<uniform> params: Params;

fn get_sdf(x: i32, y: i32, z: i32) -> f32 {{
    let ux = clamp(u32(x), 0u, params.grid_x - 1u);
    let uy = clamp(u32(y), 0u, params.grid_y - 1u);
    let uz = clamp(u32(z), 0u, params.grid_z - 1u);
    return textureLoad(input, vec3<u32>(ux, uy, uz), 0).x;
}}

fn gather(index: u32) -> u32 {{
    let grid_xy = params.grid_x * params.grid_y;
    let z = index / grid_xy;
    let y = (index % grid_xy) / params.grid_x;
    let x = index % params.grid_x;
    let id0 = vec3<u32>(x, y, z);

    if (id0.x == 0u || id0.y == 0u || id0.z == 0u || id0.x >= params.grid_x - 2u || id0.y >= params.grid_y - 2u || id0.z >= params.grid_z - 2u) {{
        return 0u;
    }}

    let c0 = vec3<i32>(id0);
    let v0 = get_sdf(c0.x, c0.y, c0.z);
    let v1 = get_sdf(c0.x + 1, c0.y, c0.z);
    let v3 = get_sdf(c0.x, c0.y + 1, c0.z);
    let v4 = get_sdf(c0.x, c0.y, c0.z + 1);

    let s0 = v0 < params.threshold;
    let s1 = v1 < params.threshold;
    let s3 = v3 < params.threshold;
    let s4 = v4 < params.threshold;

    var count = 0u;
    // Edge X
    if (s0 != s1 && id0.x >= 1u && id0.x < params.grid_x - 2u && id0.y >= 2u && id0.y < params.grid_y - 2u && id0.z >= 2u && id0.z < params.grid_z - 2u) {{
        count += 6u;
    }}
    // Edge Y
    if (s0 != s3 && id0.x >= 2u && id0.x < params.grid_x - 2u && id0.y >= 1u && id0.y < params.grid_y - 2u && id0.z >= 2u && id0.z < params.grid_z - 2u) {{
        count += 6u;
    }}
    // Edge Z
    if (s0 != s4 && id0.x >= 2u && id0.x < params.grid_x - 2u && id0.y >= 2u && id0.y < params.grid_y - 2u && id0.z >= 1u && id0.z < params.grid_z - 2u) {{
        count += 6u;
    }}
    return count;
}}
"#,
            params_struct = params_struct
        );

        let vertex_count_def = GatherDefinition::new(context, vertex_count_wgsl)?;
        let index_count_def = GatherDefinition::new(context, index_count_wgsl)?;
        let scan_def = ScanDefinition::new(
            context,
            "fn scan(a: u32, b: u32) -> u32 { return a + b; }".to_string(),
        )?;

        let vertex_stream_wgsl = format!(
            r#"
struct Vertex {{
    position: vec4<f32>,
    normal: vec4<f32>,
}}
{params_struct}
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read_write> cell_vertex_indices: array<u32>;

fn get_sdf(x: i32, y: i32, z: i32) -> f32 {{
    let ux = clamp(u32(x), 0u, params.grid_x - 1u);
    let uy = clamp(u32(y), 0u, params.grid_y - 1u);
    let uz = clamp(u32(z), 0u, params.grid_z - 1u);
    return textureLoad(input, vec3<u32>(ux, uy, uz), 0).x;
}}

fn interpolate_sdf(d: vec3<f32>, v0: f32, v1: f32, v2: f32, v3: f32, v4: f32, v5: f32, v6: f32, v7: f32) -> f32 {{
    let c01 = mix(v0, v1, d.x);
    let c32 = mix(v3, v2, d.x);
    let c45 = mix(v4, v5, d.x);
    let c76 = mix(v7, v6, d.x);
    
    let c0132 = mix(c01, c32, d.y);
    let c4576 = mix(c45, c76, d.y);
    
    return mix(c0132, c4576, d.z);
}}

fn get_sdf_interpolated(p: vec3<f32>) -> f32 {{
    let base = vec3<u32>(floor(p));
    let d = p - vec3<f32>(base);
    let v0 = textureLoad(input, base, 0).x;
    let v1 = textureLoad(input, base + vec3<u32>(1u, 0u, 0u), 0).x;
    let v2 = textureLoad(input, base + vec3<u32>(1u, 1u, 0u), 0).x;
    let v3 = textureLoad(input, base + vec3<u32>(0u, 1u, 0u), 0).x;
    let v4 = textureLoad(input, base + vec3<u32>(0u, 0u, 1u), 0).x;
    let v5 = textureLoad(input, base + vec3<u32>(1u, 0u, 1u), 0).x;
    let v6 = textureLoad(input, base + vec3<u32>(1u, 1u, 1u), 0).x;
    let v7 = textureLoad(input, base + vec3<u32>(0u, 1u, 1u), 0).x;
    return interpolate_sdf(d, v0, v1, v2, v3, v4, v5, v6, v7);
}}

fn calculate_normal_at(p: vec3<f32>) -> vec3<f32> {{
    let eps = 0.01;
    let nx = get_sdf_interpolated(p + vec3<f32>(eps, 0.0, 0.0)) - get_sdf_interpolated(p - vec3<f32>(eps, 0.0, 0.0));
    let ny = get_sdf_interpolated(p + vec3<f32>(0.0, eps, 0.0)) - get_sdf_interpolated(p - vec3<f32>(0.0, eps, 0.0));
    let nz = get_sdf_interpolated(p + vec3<f32>(0.0, 0.0, eps)) - get_sdf_interpolated(p - vec3<f32>(0.0, 0.0, eps));
    return normalize(vec3<f32>(nx, ny, nz));
}}

fn stream(index: vec3<u32>, offset: u32, _res: ptr<storage, array<Vertex>, read_write>) {{
    if (index.x == 0u || index.y == 0u || index.z == 0u || index.x >= params.grid_x - 2u || index.y >= params.grid_y - 2u || index.z >= params.grid_z - 2u) {{
        return;
    }}

    let c0 = vec3<i32>(index);
    let c1 = c0 + vec3<i32>(1, 0, 0);
    let c2 = c0 + vec3<i32>(1, 1, 0);
    let c3 = c0 + vec3<i32>(0, 1, 0);
    let c4 = c0 + vec3<i32>(0, 0, 1);
    let c5 = c0 + vec3<i32>(1, 0, 1);
    let c6 = c0 + vec3<i32>(1, 1, 1);
    let c7 = c0 + vec3<i32>(0, 1, 1);

    let v0 = get_sdf(c0.x, c0.y, c0.z);
    let v1 = get_sdf(c1.x, c1.y, c1.z);
    let v2 = get_sdf(c2.x, c2.y, c2.z);
    let v3 = get_sdf(c3.x, c3.y, c3.z);
    let v4 = get_sdf(c4.x, c4.y, c4.z);
    let v5 = get_sdf(c5.x, c5.y, c5.z);
    let v6 = get_sdf(c6.x, c6.y, c6.z);
    let v7 = get_sdf(c7.x, c7.y, c7.z);

    let s0 = v0 < params.threshold;
    let s1 = v1 < params.threshold;
    let s2 = v2 < params.threshold;
    let s3 = v3 < params.threshold;
    let s4 = v4 < params.threshold;
    let s5 = v5 < params.threshold;
    let s6 = v6 < params.threshold;
    let s7 = v7 < params.threshold;

    var sum_pos = vec3<f32>(0.0);
    var count_intersections = 0.0;

    if (s0 != s1) {{
        let t = clamp((params.threshold - v0) / (v1 - v0), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c0), vec3<f32>(c1), t);
        count_intersections += 1.0;
    }}
    if (s1 != s2) {{
        let t = clamp((params.threshold - v1) / (v2 - v1), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c1), vec3<f32>(c2), t);
        count_intersections += 1.0;
    }}
    if (s3 != s2) {{
        let t = clamp((params.threshold - v3) / (v2 - v3), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c3), vec3<f32>(c2), t);
        count_intersections += 1.0;
    }}
    if (s0 != s3) {{
        let t = clamp((params.threshold - v0) / (v3 - v0), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c0), vec3<f32>(c3), t);
        count_intersections += 1.0;
    }}
    if (s4 != s5) {{
        let t = clamp((params.threshold - v4) / (v5 - v4), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c4), vec3<f32>(c5), t);
        count_intersections += 1.0;
    }}
    if (s5 != s6) {{
        let t = clamp((params.threshold - v5) / (v6 - v5), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c5), vec3<f32>(c6), t);
        count_intersections += 1.0;
    }}
    if (s7 != s6) {{
        let t = clamp((params.threshold - v7) / (v6 - v7), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c7), vec3<f32>(c6), t);
        count_intersections += 1.0;
    }}
    if (s4 != s7) {{
        let t = clamp((params.threshold - v4) / (v7 - v4), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c4), vec3<f32>(c7), t);
        count_intersections += 1.0;
    }}
    if (s0 != s4) {{
        let t = clamp((params.threshold - v0) / (v4 - v0), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c0), vec3<f32>(c4), t);
        count_intersections += 1.0;
    }}
    if (s1 != s5) {{
        let t = clamp((params.threshold - v1) / (v5 - v1), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c1), vec3<f32>(c5), t);
        count_intersections += 1.0;
    }}
    if (s2 != s6) {{
        let t = clamp((params.threshold - v2) / (v6 - v2), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c2), vec3<f32>(c6), t);
        count_intersections += 1.0;
    }}
    if (s3 != s7) {{
        let t = clamp((params.threshold - v3) / (v7 - v3), 0.0, 1.0);
        sum_pos += mix(vec3<f32>(c3), vec3<f32>(c7), t);
        count_intersections += 1.0;
    }}

    if (count_intersections == 0.0) {{
        return;
    }}

    var v_pos = sum_pos / count_intersections;
    var p_curr = v_pos;
    for (var step = 0; step < 2; step += 1) {{
        let val = get_sdf_interpolated(p_curr);
        let grad = calculate_normal_at(p_curr);
        let diff = val - params.threshold;
        p_curr = p_curr - diff * grad;
    }}
    p_curr = clamp(p_curr, vec3<f32>(c0), vec3<f32>(c0) + vec3<f32>(1.0));
    v_pos = p_curr;

    let scale = vec3<f32>(params.scale_x, params.scale_y, params.scale_z);
    let pos_scaled = v_pos * scale + vec3<f32>(params.offset_x, params.offset_y, params.offset_z);
    let normal = normalize(calculate_normal_at(v_pos) / scale);

    if (offset < params.max_vertices) {{
        output[offset] = Vertex(vec4<f32>(pos_scaled, 1.0), vec4<f32>(normal, 0.0));
    }}

    let grid_xy = params.grid_x * params.grid_y;
    let linear_index = index.z * grid_xy + index.y * params.grid_x + index.x;
    cell_vertex_indices[linear_index] = offset;
}}
"#,
            params_struct = params_struct
        );

        let vertex_stream_def = StreamDefinition::new(context, vertex_stream_wgsl)?;

        let index_stream_wgsl = format!(
            r#"
struct Vertex {{
    position: vec4<f32>,
    normal: vec4<f32>,
}}
{params_struct}
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read> cell_vertex_indices: array<u32>;
@group(0) @binding(6) var<storage, read> front_vertices: array<Vertex>;

fn write_front_quad(offset: u32, a: u32, b: u32, c: u32, d: u32, forward: bool) {{
    // SAFT-style candidate selection: for an advancing front, choose the
    // shorter of the two legal diagonals. Dual contouring keeps its historical
    // fixed diagonal when front_mode is disabled.
    let diagonal_ac = distance(front_vertices[a].position.xyz, front_vertices[c].position.xyz);
    let diagonal_bd = distance(front_vertices[b].position.xyz, front_vertices[d].position.xyz);
    let use_bd = params.front_mode != 0u && diagonal_bd < diagonal_ac;

    if (forward) {{
        if (use_bd) {{
            output[offset] = a; output[offset + 1u] = b; output[offset + 2u] = d;
            output[offset + 3u] = b; output[offset + 4u] = c; output[offset + 5u] = d;
        }} else {{
            output[offset] = a; output[offset + 1u] = b; output[offset + 2u] = c;
            output[offset + 3u] = a; output[offset + 4u] = c; output[offset + 5u] = d;
        }}
    }} else {{
        if (use_bd) {{
            output[offset] = a; output[offset + 1u] = d; output[offset + 2u] = b;
            output[offset + 3u] = b; output[offset + 4u] = d; output[offset + 5u] = c;
        }} else {{
            output[offset] = a; output[offset + 1u] = c; output[offset + 2u] = b;
            output[offset + 3u] = a; output[offset + 4u] = d; output[offset + 5u] = c;
        }}
    }}
}}

fn get_sdf(x: i32, y: i32, z: i32) -> f32 {{
    let ux = clamp(u32(x), 0u, params.grid_x - 1u);
    let uy = clamp(u32(y), 0u, params.grid_y - 1u);
    let uz = clamp(u32(z), 0u, params.grid_z - 1u);
    return textureLoad(input, vec3<u32>(ux, uy, uz), 0).x;
}}

fn stream(index: vec3<u32>, offset: u32, _res: ptr<storage, array<u32>, read_write>) {{
    if (index.x == 0u || index.y == 0u || index.z == 0u || index.x >= params.grid_x - 2u || index.y >= params.grid_y - 2u || index.z >= params.grid_z - 2u) {{
        return;
    }}

    let grid_xy = params.grid_x * params.grid_y;
    let linear_index = index.z * grid_xy + index.y * params.grid_x + index.x;

    let c0 = vec3<i32>(index);
    let v0 = get_sdf(c0.x, c0.y, c0.z);
    let v1 = get_sdf(c0.x + 1, c0.y, c0.z);
    let v3 = get_sdf(c0.x, c0.y + 1, c0.z);
    let v4 = get_sdf(c0.x, c0.y, c0.z + 1);

    let s0 = v0 < params.threshold;
    let s1 = v1 < params.threshold;
    let s3 = v3 < params.threshold;
    let s4 = v4 < params.threshold;

    var cur_offset = offset;

    // Edge X
    if (s0 != s1 && index.x >= 1u && index.x < params.grid_x - 2u && index.y >= 2u && index.y < params.grid_y - 2u && index.z >= 2u && index.z < params.grid_z - 2u) {{
        let cell0 = linear_index;
        let cell1 = (index.z - 1u) * grid_xy + index.y * params.grid_x + index.x;
        let cell2 = (index.z - 1u) * grid_xy + (index.y - 1u) * params.grid_x + index.x;
        let cell3 = index.z * grid_xy + (index.y - 1u) * params.grid_x + index.x;

        let v0_idx = cell_vertex_indices[cell0];
        let v1_idx = cell_vertex_indices[cell1];
        let v2_idx = cell_vertex_indices[cell2];
        let v3_idx = cell_vertex_indices[cell3];

        if (v0_idx != 0xFFFFFFFFu && v1_idx != 0xFFFFFFFFu && v2_idx != 0xFFFFFFFFu && v3_idx != 0xFFFFFFFFu) {{
            if (cur_offset + 6u <= params.max_indices) {{
                write_front_quad(cur_offset, v0_idx, v1_idx, v2_idx, v3_idx, v0 < v1);
            }}
            cur_offset += 6u;
        }}
    }}

    // Edge Y
    if (s0 != s3 && index.x >= 2u && index.x < params.grid_x - 2u && index.y >= 1u && index.y < params.grid_y - 2u && index.z >= 2u && index.z < params.grid_z - 2u) {{
        let cell0 = linear_index;
        let cell1 = index.z * grid_xy + index.y * params.grid_x + (index.x - 1u);
        let cell2 = (index.z - 1u) * grid_xy + index.y * params.grid_x + (index.x - 1u);
        let cell3 = (index.z - 1u) * grid_xy + index.y * params.grid_x + index.x;

        let v0_idx = cell_vertex_indices[cell0];
        let v1_idx = cell_vertex_indices[cell1];
        let v2_idx = cell_vertex_indices[cell2];
        let v3_idx = cell_vertex_indices[cell3];

        if (v0_idx != 0xFFFFFFFFu && v1_idx != 0xFFFFFFFFu && v2_idx != 0xFFFFFFFFu && v3_idx != 0xFFFFFFFFu) {{
            if (cur_offset + 6u <= params.max_indices) {{
                write_front_quad(cur_offset, v0_idx, v1_idx, v2_idx, v3_idx, v0 < v3);
            }}
            cur_offset += 6u;
        }}
    }}

    // Edge Z
    if (s0 != s4 && index.x >= 2u && index.x < params.grid_x - 2u && index.y >= 2u && index.y < params.grid_y - 2u && index.z >= 1u && index.z < params.grid_z - 2u) {{
        let cell0 = linear_index;
        let cell1 = index.z * grid_xy + (index.y - 1u) * params.grid_x + index.x;
        let cell2 = index.z * grid_xy + (index.y - 1u) * params.grid_x + (index.x - 1u);
        let cell3 = index.z * grid_xy + index.y * params.grid_x + (index.x - 1u);

        let v0_idx = cell_vertex_indices[cell0];
        let v1_idx = cell_vertex_indices[cell1];
        let v2_idx = cell_vertex_indices[cell2];
        let v3_idx = cell_vertex_indices[cell3];

        if (v0_idx != 0xFFFFFFFFu && v1_idx != 0xFFFFFFFFu && v2_idx != 0xFFFFFFFFu && v3_idx != 0xFFFFFFFFu) {{
            if (cur_offset + 6u <= params.max_indices) {{
                write_front_quad(cur_offset, v0_idx, v1_idx, v2_idx, v3_idx, v0 < v4);
            }}
            cur_offset += 6u;
        }}
    }}
}}
"#,
            params_struct = params_struct
        );

        let index_stream_def = StreamDefinition::new(context, index_stream_wgsl)?;

        let sync_indirect_wgsl = r#"
@group(0) @binding(2) var<storage, read> inclusive_offsets: array<u32>;
@group(0) @binding(3) var<storage, read_write> indirect: array<u32>;

fn map(val: u32) -> u32 {
    let last_index = arrayLength(&inclusive_offsets) - 1u;
    indirect[0] = inclusive_offsets[last_index];
    indirect[1] = 1u;
    indirect[2] = 0u;
    indirect[3] = 0u;
    indirect[4] = 0u;
    return val;
}
"#;
        let sync_indirect_def = MapDefinition::new(sync_indirect_wgsl.to_string())?;

        let deinterleave_wgsl = r#"
            struct Vertex {
                position: vec4<f32>,
                normal: vec4<f32>,
            }

            @group(0) @binding(0) var<storage, read> vertices: array<Vertex>;
            @group(0) @binding(1) var<storage, read_write> out_positions: array<f32>;
            @group(0) @binding(2) var<storage, read_write> out_normals: array<f32>;

            @compute @workgroup_size(64)
            fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
                let idx = global_id.x;
                if (idx >= arrayLength(&vertices)) {
                    return;
                }
                let v = vertices[idx];
                let pos_idx = idx * 3u;
                out_positions[pos_idx] = v.position.x;
                out_positions[pos_idx + 1u] = v.position.y;
                out_positions[pos_idx + 2u] = v.position.z;
                
                out_normals[pos_idx] = v.normal.x;
                out_normals[pos_idx + 1u] = v.normal.y;
                out_normals[pos_idx + 2u] = v.normal.z;
            }
        "#;
        let deinterleave_shader =
            crate::data::gpu::compute::ComputeShader::new(context, deinterleave_wgsl.to_string())?;
        let deinterleave_pipeline =
            crate::data::gpu::compute::ComputePipeline::new(context, deinterleave_shader)?;

        Ok(Self {
            vertex_count_def,
            index_count_def,
            scan_def,
            vertex_stream_def,
            index_stream_def,
            sync_indirect_def,
            deinterleave_pipeline,
        })
    }
}

pub struct DualContouring;

impl DualContouring {
    pub fn execute(
        context: &WgpuContext,
        definition: &DualContouringDefinition,
        sdf: &GpuResource,
        grid: (u32, u32, u32),
        threshold: f32,
        max_vertices: u32,
        max_indices: u32,
        scale: (f32, f32, f32),
        offset: (f32, f32, f32),
    ) -> Result<(Buffer, Buffer, Buffer, Buffer)> {
        Self::execute_internal(
            context,
            definition,
            sdf,
            grid,
            threshold,
            max_vertices,
            max_indices,
            scale,
            offset,
            false,
        )
    }

    pub(crate) fn execute_advancing_front(
        context: &WgpuContext,
        definition: &DualContouringDefinition,
        sdf: &GpuResource,
        grid: (u32, u32, u32),
        threshold: f32,
        max_vertices: u32,
        max_indices: u32,
        scale: (f32, f32, f32),
        offset: (f32, f32, f32),
    ) -> Result<(Buffer, Buffer, Buffer, Buffer)> {
        Self::execute_internal(
            context,
            definition,
            sdf,
            grid,
            threshold,
            max_vertices,
            max_indices,
            scale,
            offset,
            true,
        )
    }

    fn execute_internal(
        context: &WgpuContext,
        definition: &DualContouringDefinition,
        sdf: &GpuResource,
        grid: (u32, u32, u32),
        threshold: f32,
        max_vertices: u32,
        max_indices: u32,
        scale: (f32, f32, f32),
        offset: (f32, f32, f32),
        front_mode: bool,
    ) -> Result<(Buffer, Buffer, Buffer, Buffer)> {
        let grid_total = (grid.0 * grid.1 * grid.2) as u64;

        // Create uniform parameter buffer info
        let mut params = PassParameters::new();
        params.insert("grid_x", grid.0);
        params.insert("grid_y", grid.1);
        params.insert("grid_z", grid.2);
        params.insert("max_vertices", max_vertices);
        params.insert("max_indices", max_indices);
        params.insert("front_mode", u32::from(front_mode));
        params.insert("threshold", threshold);
        params.insert("scale_x", scale.0);
        params.insert("scale_y", scale.1);
        params.insert("scale_z", scale.2);
        params.insert("offset_x", offset.0);
        params.insert("offset_y", offset.1);
        params.insert("offset_z", offset.2);

        // 1. Gather Vertex Count
        let vertex_counts_buffer = Buffer::new(
            context,
            grid_total * 4,
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_vertex_counts")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        Gather::execute_with_parameters(
            context,
            &definition.vertex_count_def,
            sdf,
            &GpuResource::Buffer(vertex_counts_buffer.clone()),
            Some(params.clone()),
        )?;

        // 2. Gather Index Count
        let index_counts_buffer = Buffer::new(
            context,
            grid_total * 4,
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_index_counts")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        Gather::execute_with_parameters(
            context,
            &definition.index_count_def,
            sdf,
            &GpuResource::Buffer(index_counts_buffer.clone()),
            Some(params.clone()),
        )?;

        // 3. Scan Passes
        let vertex_inclusive_offsets =
            Scan::execute(context, &definition.scan_def, &vertex_counts_buffer)?;
        let index_inclusive_offsets =
            Scan::execute(context, &definition.scan_def, &index_counts_buffer)?;

        // 4. Sync Indirect Buffer (using index count)
        let dummy_in = Buffer::new(
            context,
            4,
            crate::data::BufferDefinition::storage()
                .with_label("dummy_in")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        let dummy_out = Buffer::new(
            context,
            4,
            crate::data::BufferDefinition::storage()
                .with_label("dummy_out")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        let output_indirect = Buffer::new(
            context,
            20, // 5 * u32 for DrawIndexedIndirect
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_indirect")
                .with_copy_src()
                .with_copy_dst()
                .with_indirect(),
        )?;
        let mut sync_params = PassParameters::new();
        sync_params.insert("inclusive_offsets", index_inclusive_offsets.clone());
        sync_params.insert("indirect", output_indirect.clone());
        crate::data::gpu::compute::Map::execute_with_parameters(
            context,
            &definition.sync_indirect_def,
            Some(&GpuResource::Buffer(dummy_in)),
            &GpuResource::Buffer(dummy_out),
            Some(sync_params),
        )?;

        // 5. Initialize cell_vertex_indices buffer to 0xFFFFFFFF
        let cell_vertex_indices = Buffer::from_slice(
            context,
            &vec![0xFFFFFFFFu32; grid_total as usize],
            BufferDefinition::storage().with_label("cell_vertex_indices"),
        )?;

        // 6. Stream Vertices
        let output_vertices = Buffer::new(
            context,
            max_vertices as u64 * 32, // vec4 pos + vec4 norm
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_output_vertices")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        let mut vertex_gen_params = params.clone();
        vertex_gen_params.insert("cell_vertex_indices", cell_vertex_indices.clone());
        crate::data::gpu::compute::Stream::execute(
            context,
            &definition.vertex_stream_def,
            sdf,
            &vertex_counts_buffer,
            &vertex_inclusive_offsets,
            &output_vertices,
            Some(vertex_gen_params),
        )?;

        // 7. Stream Indices
        let output_indices = Buffer::new(
            context,
            max_indices as u64 * 4,
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_output_indices")
                .with_copy_src()
                .with_copy_dst()
                .with_index(),
        )?;
        let mut index_gen_params = params.clone();
        index_gen_params.insert("cell_vertex_indices", cell_vertex_indices.clone());
        index_gen_params.insert("front_vertices", output_vertices.clone());
        crate::data::gpu::compute::Stream::execute(
            context,
            &definition.index_stream_def,
            sdf,
            &index_counts_buffer,
            &index_inclusive_offsets,
            &output_indices,
            Some(index_gen_params),
        )?;

        // 8. Deinterleave positions and normals
        let out_positions = Buffer::new(
            context,
            max_vertices as u64 * 12, // vec3 pos
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_positions")
                .with_copy_src()
                .with_copy_dst()
                .with_vertex(),
        )?;
        let out_normals = Buffer::new(
            context,
            max_vertices as u64 * 12, // vec3 norm
            crate::data::BufferDefinition::storage()
                .with_label("dual_contouring_normals")
                .with_copy_src()
                .with_copy_dst()
                .with_vertex(),
        )?;

        let mut deinterleave_params = PassParameters::new();
        deinterleave_params.insert("vertices", PassParameter::from(output_vertices));
        deinterleave_params.insert("out_positions", PassParameter::from(out_positions.clone()));
        deinterleave_params.insert("out_normals", PassParameter::from(out_normals.clone()));

        let workgroups_x = (max_vertices + 63) / 64;
        crate::data::gpu::compute::ComputePass::new(
            context,
            definition.deinterleave_pipeline.clone(),
            deinterleave_params,
            workgroups_x,
            1,
            1,
        )?;

        Ok((out_positions, out_normals, output_indices, output_indirect))
    }
}
