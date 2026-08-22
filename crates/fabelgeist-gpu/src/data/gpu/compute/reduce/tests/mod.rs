use super::*;
use crate::data::gpu::compute::test_utils::*;
use crate::prelude::*;

pub async fn test_generalized_reduce<T, S>(
    definition_code: &str,
    input_data: &[T],
    expected_output: &[T],
) -> Result<()>
where
    T: bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    S: bytemuck::Pod + std::fmt::Debug + Default + Copy + PartialEq,
{
    let context = WgpuContext::new().await.unwrap();
    let definition = ReduceDefinition::new(&context, definition_code.to_string())?;

    let mut scratchpad = ReduceScratchpad::default();

    let in_buf = Buffer::from_slice(
        &context,
        input_data,
        BufferDefinition::storage().with_copy_src(),
    )?;
    let in_res = crate::data::gpu::resource::GpuResource::Buffer(in_buf);

    let result_buf = Reduce::execute(&context, &definition, &in_res, &mut scratchpad)?;

    let result: Vec<T> = result_buf.read(&context).await?;

    assert_eq!(result, expected_output);

    Ok(())
}

#[tokio::test]
async fn sum_f32() -> Result<()> {
    test_generalized_reduce::<f32, f32>(
        "fn reduce(a: f32, b: f32) -> f32 { return a + b; }",
        &(0..128).map(|i| i as f32).collect::<Vec<_>>(),
        &[8128.0],
    )
    .await
}

#[tokio::test]
async fn min_f32() -> Result<()> {
    let context = WgpuContext::new().await.unwrap();
    let mut scratchpad = ReduceScratchpad::default();
    let input_data: Vec<f32> = vec![10.0, 2.0, 50.0, -1.0, 20.0];
    let in_buf = Buffer::from_slice(
        &context,
        &input_data,
        BufferDefinition::storage().with_copy_src(),
    )?;
    let in_res = crate::data::gpu::resource::GpuResource::Buffer(in_buf);

    let result_buf = Min::execute(&context, &in_res, None, &mut scratchpad)?;
    let result: Vec<f32> = result_buf.read(&context).await?;
    assert_eq!(result[0], -1.0);
    Ok(())
}

#[tokio::test]
async fn max_f32() -> Result<()> {
    let context = WgpuContext::new().await.unwrap();
    let mut scratchpad = ReduceScratchpad::default();
    let input_data: Vec<f32> = vec![10.0, 2.0, 50.0, -1.0, 20.0];
    let in_buf = Buffer::from_slice(
        &context,
        &input_data,
        BufferDefinition::storage().with_copy_src(),
    )?;
    let in_res = crate::data::gpu::resource::GpuResource::Buffer(in_buf);

    let result_buf = Max::execute(&context, &in_res, None, &mut scratchpad)?;
    let result: Vec<f32> = result_buf.read(&context).await?;
    assert_eq!(result[0], 50.0);
    Ok(())
}

#[tokio::test]
async fn min_custom_struct() -> Result<()> {
    let context = WgpuContext::new().await.unwrap();

    #[repr(C)]
    #[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, PartialEq)]
    struct Item {
        val: f32,
        id: u32,
    }

    let input_data = vec![
        Item { val: 10.0, id: 0 },
        Item { val: 2.0, id: 1 },
        Item { val: 50.0, id: 2 },
        Item { val: -1.0, id: 3 },
        Item { val: 20.0, id: 4 },
    ];

    let mut scratchpad = ReduceScratchpad::default();
    let in_buf = Buffer::from_slice(
        &context,
        &input_data,
        BufferDefinition::storage().with_copy_src(),
    )?;
    let in_res = crate::data::gpu::resource::GpuResource::Buffer(in_buf);

    let custom_code = r#"
        struct Item {
            val: f32,
            id: u32,
        }
        fn min(a: Item, b: Item) -> bool {
            return a.val < b.val;
        }
    "#;

    let result_buf = Min::execute(&context, &in_res, Some(custom_code), &mut scratchpad)?;
    let result: Vec<Item> = result_buf.read(&context).await?;
    assert_eq!(result[0].val, -1.0);
    assert_eq!(result[0].id, 3);
    Ok(())
}

#[tokio::test]
async fn min_Texture2d() -> Result<()> {
    let context = WgpuContext::new().await.unwrap();
    let mut scratchpad = crate::data::gpu::compute::reduce::ReduceScratchpad::default();

    let width = 10;
    let height = 10;
    let mut input_data = vec![1.0f32; (width * height) as usize];
    input_data[42] = -5.0; // The minimum value

    let texture = crate::data::gpu::texture::Texture2d::create(
        &context,
        crate::data::Vec2::new(width as f32, height as f32),
        crate::data::gpu::texture::TextureFormat::R32Float,
    )?;
    texture.write(&context, &input_data)?;

    let in_res = crate::data::gpu::resource::GpuResource::Texture2d(texture);

    let result_buf = Min::execute(&context, &in_res, None, &mut scratchpad)?;
    let result: Vec<f32> = result_buf.read(&context).await?;

    assert_eq!(result[0], -5.0);
    Ok(())
}

#[tokio::test]
async fn min_to_number() -> Result<()> {
    let context = WgpuContext::new().await.unwrap();
    let mut scratchpad = ReduceScratchpad::default();
    let input_data: Vec<f32> = vec![10.0, 2.0, 50.0, -1.0, 20.0];
    let in_buf = Buffer::from_slice(
        &context,
        &input_data,
        BufferDefinition::storage().with_copy_src(),
    )?;
    let in_res = crate::data::gpu::resource::GpuResource::Buffer(in_buf);

    let result = Min::execute_to_number(&context, &in_res, &mut scratchpad).await?;
    assert_eq!(result, -1.0);
    Ok(())
}
