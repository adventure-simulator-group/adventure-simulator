mod helper;
use helper::*;

use crate::test_map;

test_map!(
    f32_to_vec2,
    "fn map(value: f32) -> vec2<f32> { return vec2<f32>(value, value * 2.0); }",
    [1.0f32, 2.0f32, 3.0f32, 4.0f32],
    [
        vec2(1.0f32, 2.0f32),
        vec2(2.0f32, 4.0f32),
        vec2(3.0f32, 6.0f32),
        vec2(4.0f32, 8.0f32)
    ],
    f32
);

test_map!(
    f32_to_vec4,
    "fn map(value: f32) -> vec4<f32> { return vec4<f32>(value, value * 2.0, value * 3.0, value * 4.0); }",
    [1.0f32, 2.0f32],
    [
        vec4(1.0f32, 2.0f32, 3.0f32, 4.0f32),
        vec4(2.0f32, 4.0f32, 6.0f32, 8.0f32)
    ],
    f32
);

test_map!(
    i32_to_vec2,
    "fn map(value: i32) -> vec2<i32> { return vec2<i32>(value, value * 2); }",
    [1i32, 2i32, 3i32, 4i32],
    [
        vec2(1i32, 2i32),
        vec2(2i32, 4i32),
        vec2(3i32, 6i32),
        vec2(4i32, 8i32)
    ],
    i32
);

test_map!(
    i32_to_vec4,
    "fn map(value: i32) -> vec4<i32> { return vec4<i32>(value, value * 2, value * 3, value * 4); }",
    [1i32, 2i32],
    [vec4(1i32, 2i32, 3i32, 4i32), vec4(2i32, 4i32, 6i32, 8i32)],
    i32
);

test_map!(
    u32_to_vec2,
    "fn map(value: u32) -> vec2<u32> { return vec2<u32>(value, value * 2u); }",
    [1u32, 2u32, 3u32, 4u32],
    [
        vec2(1u32, 2u32),
        vec2(2u32, 4u32),
        vec2(3u32, 6u32),
        vec2(4u32, 8u32)
    ],
    u32
);

test_map!(
    u32_to_vec4,
    "fn map(value: u32) -> vec4<u32> { return vec4<u32>(value, value * 2u, value * 3u, value * 4u); }",
    [1u32, 2u32],
    [vec4(1u32, 2u32, 3u32, 4u32), vec4(2u32, 4u32, 6u32, 8u32)],
    u32
);

test_map!(
    generate_from_index,
    "fn map(index: u32) -> u32 { return index * 2; }",
    [0u32, 0u32, 0u32, 0u32],
    [0u32, 2u32, 4u32, 6u32],
    u32
);

test_map!(
    subtract_from_index,
    "fn map(index: u32, value: u32) -> u32 { return value - index; }",
    [1u32, 2u32, 3u32, 4u32],
    [1u32, 1u32, 1u32, 1u32],
    u32
);

test_map!(
    f32_to_f32,
    "fn map(value: f32) -> f32 { return value * 2.0; }",
    [1.0f32, 2.0f32, 3.0f32, 4.0f32],
    [2.0f32, 4.0f32, 6.0f32, 8.0f32],
    f32
);

test_map!(
    i32_to_i32,
    "fn map(value: i32) -> i32 { return value * 2; }",
    [1i32, 2i32, 3i32, 4i32],
    [2i32, 4i32, 6i32, 8i32],
    i32
);

test_map!(
    u32_to_u32,
    "fn map(value: u32) -> u32 { return value + 10u; }",
    [1u32, 2u32, 3u32, 4u32],
    [11u32, 12u32, 13u32, 14u32],
    u32
);

test_map!(
    f32_to_u32,
    "fn map(value: f32) -> u32 { return u32(value); }",
    [1.5f32, 2.5f32, 3.5f32, 4.5f32],
    [1u32, 2u32, 3u32, 4u32],
    u32
);

test_map!(
    vec2_to_f32,
    "fn map(v: vec2<f32>) -> f32 { return v.x + v.y; }",
    [vec2(1.0f32, 2.0f32), vec2(3.0f32, 4.0f32)],
    [3.0f32, 7.0f32],
    f32
);

test_map!(
    vec4_to_f32,
    "fn map(v: vec4<f32>) -> f32 { return v.x + v.y + v.z + v.w; }",
    [
        vec4(1.0f32, 2.0f32, 3.0f32, 4.0f32),
        vec4(5.0f32, 6.0f32, 7.0f32, 8.0f32)
    ],
    [10.0f32, 26.0f32],
    f32
);

test_map!(
    vec2_to_i32,
    "fn map(v: vec2<i32>) -> i32 { return v.x + v.y; }",
    [vec2(1i32, 2i32), vec2(3i32, 4i32)],
    [3i32, 7i32],
    i32
);

test_map!(
    vec4_to_i32,
    "fn map(v: vec4<i32>) -> i32 { return v.x + v.y + v.z + v.w; }",
    [vec4(1i32, 2i32, 3i32, 4i32), vec4(5i32, 6i32, 7i32, 8i32)],
    [10i32, 26i32],
    i32
);

test_map!(
    vec2_to_u32,
    "fn map(v: vec2<u32>) -> u32 { return v.x + v.y; }",
    [vec2(1u32, 2u32), vec2(3u32, 4u32)],
    [3u32, 7u32],
    u32
);

test_map!(
    vec4_to_u32,
    "fn map(v: vec4<u32>) -> u32 { return v.x + v.y + v.z + v.w; }",
    [vec4(1u32, 2u32, 3u32, 4u32), vec4(5u32, 6u32, 7u32, 8u32)],
    [10u32, 26u32],
    u32
);

test_map!(
    generate_from_size,
    "fn map(size: u32) -> u32 { return size; }",
    [0u32, 0u32, 0u32, 0u32],
    [4u32, 4u32, 4u32, 4u32],
    u32
);

test_map!(
    vec2_size,
    "fn map(size: vec2<u32>) -> u32 { return size.x + size.y; }",
    [0u32, 0u32, 0u32, 0u32],
    [5u32, 5u32, 5u32, 5u32],
    u32
);

test_map!(
    vec3_size,
    "fn map(size: vec3<u32>) -> u32 { return size.x + size.y + size.z; }",
    [0u32, 0u32, 0u32, 0u32],
    [6u32, 6u32, 6u32, 6u32],
    u32
);

#[tokio::test]
async fn test_generator() -> Result<()> {
    let context = WgpuContext::new().await.expect("Failed to init WGPU");
    let code = "fn map(index: u32) -> u32 { return index * 3u; }";
    let definition = MapDefinition::new(code.to_string())?;

    let output_buffer = Buffer::new(
        &context,
        16,
        BufferDefinition::storage().with_copy_src().with_copy_dst(),
    )?;

    Map::execute(
        &context,
        &definition,
        None,
        &GpuResource::Buffer(output_buffer.clone()),
    )?;

    let result = output_buffer.read::<u32>(&context).await?;
    assert_eq!(result, vec![0u32, 3u32, 6u32, 9u32]);
    Ok(())
}

#[tokio::test]
async fn test_extra_parameters() -> Result<()> {
    let context = WgpuContext::new().await.expect("Failed to init WGPU");
    let code =
        "fn map(val: f32, factor: f32, offset: f32) -> f32 { return val * factor + offset; }";
    let definition = MapDefinition::new(code.to_string())?;

    let input_data = vec![1.0f32, 2.0, 3.0, 4.0];
    let input_buffer = Buffer::new(
        &context,
        16,
        BufferDefinition::storage().with_copy_src().with_copy_dst(),
    )?;
    input_buffer.write(&context, &input_data)?;

    let output_buffer = Buffer::new(
        &context,
        16,
        BufferDefinition::storage().with_copy_src().with_copy_dst(),
    )?;

    let mut parameters = crate::data::gpu::parameters::PassParameters::new();
    parameters.insert("factor", 10.0f32);
    parameters.insert("offset", 5.0f32);

    Map::execute_with_parameters(
        &context,
        &definition,
        Some(&GpuResource::Buffer(input_buffer)),
        &GpuResource::Buffer(output_buffer.clone()),
        Some(parameters),
    )?;

    let result = output_buffer.read::<f32>(&context).await?;
    assert!((result[0] - 15.0).abs() < 0.001);
    assert!((result[1] - 25.0).abs() < 0.001);
    assert!((result[2] - 35.0).abs() < 0.001);
    assert!((result[3] - 45.0).abs() < 0.001);
    Ok(())
}

#[tokio::test]
async fn test_generator_with_parameters() -> Result<()> {
    let context = WgpuContext::new().await.expect("Failed to init WGPU");
    // 'time' should be a uniform because we don't pass an input resource
    let code = "fn map(index: u32, time: f32) -> f32 { return f32(index) + time; }";
    let definition = MapDefinition::new(code.to_string())?;

    let output_buffer = Buffer::new(
        &context,
        16,
        BufferDefinition::storage().with_copy_src().with_copy_dst(),
    )?;

    let mut parameters = crate::data::gpu::parameters::PassParameters::new();
    parameters.insert("time", 100.0f32);

    Map::execute_with_parameters(
        &context,
        &definition,
        None,
        &GpuResource::Buffer(output_buffer.clone()),
        Some(parameters),
    )?;

    let result = output_buffer.read::<f32>(&context).await?;
    assert_eq!(result, vec![100.0f32, 101.0, 102.0, 103.0]);
    Ok(())
}
