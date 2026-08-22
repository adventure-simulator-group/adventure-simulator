use super::*;
use crate::prelude::*;

pub async fn test_generalized_stream<IN, OUT>(
    definition_code: &str,
    input_data: &[IN],
    counts: &[u32],
    offsets: &[u32],
    expected_output: &[OUT],
) -> Result<()>
where
    IN: bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    OUT:
        bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
{
    let context = WgpuContext::new().await.unwrap();
    let definition = StreamDefinition::new(&context, definition_code.to_string())?;

    // We'll use Buffer for stream components for now
    let in_buf = Buffer::from_slice(&context, input_data, BufferDefinition::storage())?;
    let counts_buf = Buffer::from_slice(&context, counts, BufferDefinition::storage())?;
    let offsets_buf = Buffer::from_slice(&context, offsets, BufferDefinition::storage())?;
    let out_buf = Buffer::new(
        &context,
        std::mem::size_of_val(expected_output) as u64,
        BufferDefinition::storage().with_copy_src(),
    )?;

    Stream::execute(
        &context,
        &definition,
        &GpuResource::Buffer(in_buf),
        &counts_buf,
        &offsets_buf,
        &out_buf,
        None,
    )?;

    let result: Vec<OUT> = out_buf.read(&context).await?;
    assert_eq!(result, expected_output);

    Ok(())
}

#[tokio::test]
async fn simple_stream_f32() -> Result<()> {
    // Each input element produces 1 output element
    test_generalized_stream::<f32, f32>(
        "fn stream(in: f32, offset: u32) { output[offset] = in + 1.0; }",
        &[1.0, 2.0, 3.0],
        &[1, 1, 1],
        &[1, 2, 3],
        &[2.0, 3.0, 4.0],
    )
    .await
}
