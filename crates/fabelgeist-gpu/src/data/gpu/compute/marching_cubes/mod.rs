pub mod tables;

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
pub struct MarchingCubesDefinition {
    pub count_def: GatherDefinition,
    pub scan_def: ScanDefinition,
    pub stream_def: StreamDefinition,
    pub sync_indirect_def: MapDefinition,
    pub tri_table_buffer: Buffer,
    pub edge_table_buffer: Buffer,
    pub tri_count_table_buffer: Buffer,
    pub deinterleave_pipeline: crate::data::gpu::compute::ComputePipeline,
}

impl PartialEq for MarchingCubesDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.count_def == other.count_def
            && self.scan_def == other.scan_def
            && self.stream_def == other.stream_def
            && self.sync_indirect_def == other.sync_indirect_def
            && self.tri_table_buffer == other.tri_table_buffer
            && self.edge_table_buffer == other.edge_table_buffer
            && self.tri_count_table_buffer == other.tri_count_table_buffer
    }
}

impl MarchingCubesDefinition {
    pub fn new(context: &WgpuContext) -> Result<Self> {
        let tri_table_buffer = Buffer::from_slice(
            context,
            &tables::TRI_TABLE,
            BufferDefinition::storage().with_label("TRI_TABLE"),
        )?;
        let edge_table_buffer = Buffer::from_slice(
            context,
            &tables::EDGE_TABLE,
            BufferDefinition::storage().with_label("EDGE_TABLE"),
        )?;
        let tri_count_table_buffer = Buffer::from_slice(
            context,
            &tables::TRI_COUNT_TABLE,
            BufferDefinition::storage().with_label("TRI_COUNT_TABLE"),
        )?;

        let params_struct = r#"
struct Params {
    grid_x: u32,
    grid_y: u32,
    grid_z: u32,
    max_vertices: u32,
    threshold: f32,
    scale_x: f32,
    scale_y: f32,
    scale_z: f32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
}
"#;

        let count_wgsl = format!(
            r#"
{params_struct}
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> tri_count_table: array<u32>;

fn gather(index: u32) -> u32 {{
    let grid_xy = params.grid_x * params.grid_y;
    let z = index / grid_xy;
    let y = (index % grid_xy) / params.grid_x;
    let x = index % params.grid_x;
    let id0 = vec3<u32>(x, y, z);

    if (id0.x == 0u || id0.y == 0u || id0.z == 0u || id0.x >= params.grid_x - 2u || id0.y >= params.grid_y - 2u || id0.z >= params.grid_z - 2u) {{
        return 0u;
    }}

    let v0 = textureLoad(input, id0, 0).x;
    let v1 = textureLoad(input, id0 + vec3<u32>(1u, 0u, 0u), 0).x;
    let v2 = textureLoad(input, id0 + vec3<u32>(1u, 1u, 0u), 0).x;
    let v3 = textureLoad(input, id0 + vec3<u32>(0u, 1u, 0u), 0).x;
    let v4 = textureLoad(input, id0 + vec3<u32>(0u, 0u, 1u), 0).x;
    let v5 = textureLoad(input, id0 + vec3<u32>(1u, 0u, 1u), 0).x;
    let v6 = textureLoad(input, id0 + vec3<u32>(1u, 1u, 1u), 0).x;
    let v7 = textureLoad(input, id0 + vec3<u32>(0u, 1u, 1u), 0).x;

    var cubeIndex = 0u;
    if (v0 < params.threshold) {{ cubeIndex |= 1u; }}
    if (v1 < params.threshold) {{ cubeIndex |= 2u; }}
    if (v2 < params.threshold) {{ cubeIndex |= 4u; }}
    if (v3 < params.threshold) {{ cubeIndex |= 8u; }}
    if (v4 < params.threshold) {{ cubeIndex |= 16u; }}
    if (v5 < params.threshold) {{ cubeIndex |= 32u; }}
    if (v6 < params.threshold) {{ cubeIndex |= 64u; }}
    if (v7 < params.threshold) {{ cubeIndex |= 128u; }}

    return tri_count_table[cubeIndex];
}}
"#,
            params_struct = params_struct
        );

        let count_def = GatherDefinition::new(context, count_wgsl)?;
        let scan_def = ScanDefinition::new(
            context,
            "fn scan(a: u32, b: u32) -> u32 { return a + b; }".to_string(),
        )?;

        let stream_wgsl = format!(
            r#"
struct Vertex {{
    position: vec4<f32>,
    normal: vec4<f32>,
}}
{params_struct}
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read> triTable: array<i32>;
@group(0) @binding(6) var<storage, read> edgeTable: array<u32>;

fn get_sdf(x: i32, y: i32, z: i32) -> f32 {{
    let ux = clamp(u32(x), 0u, params.grid_x - 1u);
    let uy = clamp(u32(y), 0u, params.grid_y - 1u);
    let uz = clamp(u32(z), 0u, params.grid_z - 1u);
    return textureLoad(input, vec3<u32>(ux, uy, uz), 0).x;
}}

fn calculate_normal(x: u32, y: u32, z: u32) -> vec3<f32> {{
    let ix = i32(x); let iy = i32(y); let iz = i32(z);
    let nx = get_sdf(ix + 1, iy, iz) - get_sdf(ix - 1, iy, iz);
    let ny = get_sdf(ix, iy + 1, iz) - get_sdf(ix, iy - 1, iz);
    let nz = get_sdf(ix, iy, iz + 1) - get_sdf(ix, iy, iz - 1);
    return normalize(vec3<f32>(nx, ny, nz));
}}

fn interpolate(id1: vec3<u32>, id2: vec3<u32>, pos1: vec3<f32>, pos2: vec3<f32>) -> Vertex {{
    let val1 = textureLoad(input, id1, 0).x;
    let val2 = textureLoad(input, id2, 0).x;
    let t = clamp((params.threshold - val1) / (val2 - val1), 0.0, 1.0);
    let n1 = calculate_normal(id1.x, id1.y, id1.z);
    let n2 = calculate_normal(id2.x, id2.y, id2.z);
    let scale = vec3<f32>(params.scale_x, params.scale_y, params.scale_z);
    let pos = mix(pos1, pos2, t) * scale + vec3<f32>(params.offset_x, params.offset_y, params.offset_z);
    let norm = normalize(mix(n1, n2, t) / scale);
    return Vertex(vec4<f32>(pos, 1.0), vec4<f32>(norm, 0.0));
}}

fn stream(index: vec3<u32>, offset: u32, _res: ptr<storage, array<Vertex>, read_write>) {{
    if (index.x == 0u || index.y == 0u || index.z == 0u || index.x >= params.grid_x - 2u || index.y >= params.grid_y - 2u || index.z >= params.grid_z - 2u) {{
        return;
    }}

    let id0 = index;
    let id1 = id0 + vec3<u32>(1u, 0u, 0u);
    let id2 = id0 + vec3<u32>(1u, 1u, 0u);
    let id3 = id0 + vec3<u32>(0u, 1u, 0u);
    let id4 = id0 + vec3<u32>(0u, 0u, 1u);
    let id5 = id0 + vec3<u32>(1u, 0u, 1u);
    let id6 = id0 + vec3<u32>(1u, 1u, 1u);
    let id7 = id0 + vec3<u32>(0u, 1u, 1u);

    let v0 = textureLoad(input, id0, 0).x;
    let v1 = textureLoad(input, id1, 0).x;
    let v2 = textureLoad(input, id2, 0).x;
    let v3 = textureLoad(input, id3, 0).x;
    let v4 = textureLoad(input, id4, 0).x;
    let v5 = textureLoad(input, id5, 0).x;
    let v6 = textureLoad(input, id6, 0).x;
    let v7 = textureLoad(input, id7, 0).x;

    var cubeIndex = 0u;
    if (v0 < params.threshold) {{ cubeIndex |= 1u; }}
    if (v1 < params.threshold) {{ cubeIndex |= 2u; }}
    if (v2 < params.threshold) {{ cubeIndex |= 4u; }}
    if (v3 < params.threshold) {{ cubeIndex |= 8u; }}
    if (v4 < params.threshold) {{ cubeIndex |= 16u; }}
    if (v5 < params.threshold) {{ cubeIndex |= 32u; }}
    if (v6 < params.threshold) {{ cubeIndex |= 64u; }}
    if (v7 < params.threshold) {{ cubeIndex |= 128u; }}

    let edgeState = edgeTable[cubeIndex];
    if (edgeState == 0u) {{ return; }}

    let p0 = vec3<f32>(id0); let p1 = vec3<f32>(id1);
    let p2 = vec3<f32>(id2); let p3 = vec3<f32>(id3);
    let p4 = vec3<f32>(id4); let p5 = vec3<f32>(id5);
    let p6 = vec3<f32>(id6); let p7 = vec3<f32>(id7);

    var vertList: array<Vertex, 12>;
    if ((edgeState & 1u) != 0u) {{ vertList[0] = interpolate(id0, id1, p0, p1); }}
    if ((edgeState & 2u) != 0u) {{ vertList[1] = interpolate(id1, id2, p1, p2); }}
    if ((edgeState & 4u) != 0u) {{ vertList[2] = interpolate(id2, id3, p2, p3); }}
    if ((edgeState & 8u) != 0u) {{ vertList[3] = interpolate(id3, id0, p3, p0); }}
    if ((edgeState & 16u) != 0u) {{ vertList[4] = interpolate(id4, id5, p4, p5); }}
    if ((edgeState & 32u) != 0u) {{ vertList[5] = interpolate(id5, id6, p5, p6); }}
    if ((edgeState & 64u) != 0u) {{ vertList[6] = interpolate(id6, id7, p6, p7); }}
    if ((edgeState & 128u) != 0u) {{ vertList[7] = interpolate(id7, id4, p7, p4); }}
    if ((edgeState & 256u) != 0u) {{ vertList[8] = interpolate(id0, id4, p0, p4); }}
    if ((edgeState & 512u) != 0u) {{ vertList[9] = interpolate(id1, id5, p1, p5); }}
    if ((edgeState & 1024u) != 0u) {{ vertList[10] = interpolate(id2, id6, p2, p6); }}
    if ((edgeState & 2048u) != 0u) {{ vertList[11] = interpolate(id3, id7, p3, p7); }}

    var tableOffset = cubeIndex * 16u;
    for (var i = 0u; i < 16u; i += 3u) {{
        let tri0 = triTable[tableOffset + i];
        if (tri0 == -1) {{ break; }}

        let t0 = u32(tri0);
        let t1 = u32(triTable[tableOffset + i + 1u]);
        let t2 = u32(triTable[tableOffset + i + 2u]);

        if (offset + i + 3u <= params.max_vertices) {{
            output[offset + i] = vertList[t0];
            output[offset + i + 1u] = vertList[t1];
            output[offset + i + 2u] = vertList[t2];
        }}
    }}
}}
"#,
            params_struct = params_struct
        );

        let stream_def = StreamDefinition::new(context, stream_wgsl)?;

        let sync_indirect_wgsl = r#"
@group(0) @binding(2) var<storage, read> inclusive_offsets: array<u32>;
@group(0) @binding(3) var<storage, read_write> indirect: array<u32>;

fn map(val: u32) -> u32 {
    let last_index = arrayLength(&inclusive_offsets) - 1u;
    indirect[0] = inclusive_offsets[last_index];
    indirect[1] = 1u;
    indirect[2] = 0u;
    indirect[3] = 0u;
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
            count_def,
            scan_def,
            stream_def,
            sync_indirect_def,
            tri_table_buffer,
            edge_table_buffer,
            tri_count_table_buffer,
            deinterleave_pipeline,
        })
    }
}

pub struct MarchingCubes;

impl MarchingCubes {
    pub fn execute(
        context: &WgpuContext,
        definition: &MarchingCubesDefinition,
        sdf: &GpuResource,
        grid: (u32, u32, u32),
        threshold: f32,
        max_vertices: u32,
        scale: (f32, f32, f32),
        offset: (f32, f32, f32),
    ) -> Result<(Buffer, Buffer, Buffer)> {
        let grid_total = (grid.0 * grid.1 * grid.2) as u64;

        // 1. Count Pass
        let counts_buffer = Buffer::new(
            context,
            grid_total * 4,
            crate::data::BufferDefinition::storage()
                .with_label("counts_buffer")
                .with_copy_src()
                .with_copy_dst(),
        )?;
        let counts_resource = GpuResource::Buffer(counts_buffer.clone());

        let mut count_params = PassParameters::new();
        count_params.insert("grid_x", grid.0);
        count_params.insert("grid_y", grid.1);
        count_params.insert("grid_z", grid.2);
        count_params.insert("max_vertices", max_vertices);
        count_params.insert("threshold", threshold);
        count_params.insert("scale_x", scale.0);
        count_params.insert("scale_y", scale.1);
        count_params.insert("scale_z", scale.2);
        count_params.insert("offset_x", offset.0);
        count_params.insert("offset_y", offset.1);
        count_params.insert("offset_z", offset.2);
        count_params.insert("tri_count_table", definition.tri_count_table_buffer.clone());

        Gather::execute_with_parameters(
            context,
            &definition.count_def,
            sdf,
            &counts_resource,
            Some(count_params),
        )?;

        // 2. Scan Pass
        let inclusive_offsets = Scan::execute(context, &definition.scan_def, &counts_buffer)?;

        // 3. Sync Indirect Pass (Map)
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
            16,
            crate::data::BufferDefinition::storage()
                .with_label("marching_cubes_indirect")
                .with_copy_src()
                .with_copy_dst()
                .with_indirect(),
        )?;

        let mut sync_params = PassParameters::new();
        sync_params.insert("inclusive_offsets", inclusive_offsets.clone());
        sync_params.insert("indirect", output_indirect.clone());

        crate::data::gpu::compute::Map::execute_with_parameters(
            context,
            &definition.sync_indirect_def,
            Some(&GpuResource::Buffer(dummy_in)),
            &GpuResource::Buffer(dummy_out),
            Some(sync_params),
        )?;

        // 4. Generate Pass (Stream)
        let mut gen_params = PassParameters::new();
        gen_params.insert("grid_x", grid.0);
        gen_params.insert("grid_y", grid.1);
        gen_params.insert("grid_z", grid.2);
        gen_params.insert("max_vertices", max_vertices);
        gen_params.insert("threshold", threshold);
        gen_params.insert("scale_x", scale.0);
        gen_params.insert("scale_y", scale.1);
        gen_params.insert("scale_z", scale.2);
        gen_params.insert("offset_x", offset.0);
        gen_params.insert("offset_y", offset.1);
        gen_params.insert("offset_z", offset.2);
        gen_params.insert("triTable", definition.tri_table_buffer.clone());
        gen_params.insert("edgeTable", definition.edge_table_buffer.clone());

        let output_vertices = Buffer::new(
            context,
            max_vertices as u64 * 32, // vec4 pos + vec4 norm
            crate::data::BufferDefinition::storage()
                .with_label("marching_cubes_output_vertices")
                .with_copy_src()
                .with_copy_dst(),
        )?;

        crate::data::gpu::compute::Stream::execute(
            context,
            &definition.stream_def,
            sdf,
            &counts_buffer,
            &inclusive_offsets,
            &output_vertices,
            Some(gen_params),
        )?;

        // 5. Deinterleave Pass
        let out_positions = Buffer::new(
            context,
            max_vertices as u64 * 12, // vec3 pos
            crate::data::BufferDefinition::storage()
                .with_label("marching_cubes_positions")
                .with_copy_src()
                .with_copy_dst()
                .with_vertex(),
        )?;

        let out_normals = Buffer::new(
            context,
            max_vertices as u64 * 12, // vec3 norm
            crate::data::BufferDefinition::storage()
                .with_label("marching_cubes_normals")
                .with_copy_src()
                .with_copy_dst()
                .with_vertex(),
        )?;

        let mut deinterleave_params = PassParameters::new();
        deinterleave_params.insert("vertices", PassParameter::from(output_vertices));
        deinterleave_params.insert("out_positions", PassParameter::from(out_positions.clone()));
        deinterleave_params.insert("out_normals", PassParameter::from(out_normals.clone()));

        let workgroups_x = max_vertices.div_ceil(64);
        crate::data::gpu::compute::ComputePass::new(
            context,
            definition.deinterleave_pipeline.clone(),
            deinterleave_params,
            workgroups_x,
            1,
            1,
        )?;

        Ok((out_positions, out_normals, output_indirect))
    }
}
