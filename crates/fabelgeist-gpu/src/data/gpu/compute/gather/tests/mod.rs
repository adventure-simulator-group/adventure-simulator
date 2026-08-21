use super::*;
use crate::data::gpu::compute::test_utils::*;
use crate::prelude::*;

pub async fn test_generalized_gather<IN, OUT, S>(
    definition_code: &str,
    input_data: &[IN],
    expected_output: &[OUT],
) -> Result<()>
where
    IN: bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    OUT:
        bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    S: bytemuck::Pod + std::fmt::Debug + Default + Copy + PartialEq,
{
    let context = WgpuContext::new().await.unwrap();
    let definition = GatherDefinition::new(&context, definition_code.to_string())?;
    let sig = GatherDefinition::parse_signature(definition_code)?;

    run_compute_test::<IN, OUT, S, _>(
        input_data,
        expected_output,
        sig.input_element_type,
        sig.output_element_type,
        |context, in_res, out_res| Gather::execute(context, &definition, &in_res, &out_res),
    )
    .await
}

#[tokio::test]
async fn identity_gather_f32() -> Result<()> {
    test_generalized_gather::<f32, f32, f32>(
        "fn gather(index: u32) -> f32 { return 1.0; }", // simplified test: return constant for now to verify infra
        &[0.0; 10],
        &[1.0; 10],
    )
    .await
}

#[tokio::test]
async fn index_gather_f32() -> Result<()> {
    // Identity gather reading from the index itself
    test_generalized_gather::<f32, f32, f32>(
        "fn gather(index: u32) -> f32 { return f32(index); }",
        &[0.0; 10],
        &(0..10).map(|i| i as f32).collect::<Vec<_>>(),
    )
    .await
}
