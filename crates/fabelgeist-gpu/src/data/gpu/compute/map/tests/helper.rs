pub use super::super::*;
pub use crate::data::gpu::compute::test_utils::*;

pub async fn test_generalized<IN, OUT, S>(
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
    let definition = MapDefinition::new(definition_code)?;
    let sig = MapDefinition::parse_signature(definition_code)?;

    let input_element_type = if sig.user_params.is_empty() {
        ResourceBaseType::F32 // Default for generator tests without input data
    } else {
        sig.user_params[0].1.clone()
    };

    run_compute_test::<IN, OUT, S, _>(
        input_data,
        expected_output,
        input_element_type,
        sig.output_element_type,
        |context, in_res, out_res| Map::execute(context, &definition, Some(&in_res), &out_res),
    )
    .await
}

#[macro_export]
macro_rules! test_map {
    ($name:ident, $definition:expr, $input:expr, $output:expr, $s:ty) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            test_generalized::<_, _, $s>($definition, &$input, &$output).await
        }
    };
}
