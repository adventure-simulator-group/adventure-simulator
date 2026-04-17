use super::*;

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
    
    let mut scratch_a = None;
    let mut scratch_b = None;
    
    let in_buf = Buffer::from_slice(&context, input_data, BufferDefinition::storage().with_copy_src())?;
    
    let result_buf = Reduce::execute(
        &context,
        &definition,
        &in_buf,
        &mut scratch_a,
        &mut scratch_b
    )?;
    
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
    ).await
}

#[tokio::test]
async fn sum_i32() -> Result<()> {
    test_generalized_reduce::<i32, i32>(
        "fn reduce(a: i32, b: i32) -> i32 { return a + b; }",
        &(0..64).map(|i| i as i32).collect::<Vec<_>>(),
        &[2016],
    ).await
}
