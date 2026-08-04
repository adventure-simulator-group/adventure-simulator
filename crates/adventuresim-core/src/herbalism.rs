//! Shared physical-preparation and tincture timing constants.

pub const BASE_CUT_MINUTES: u32 = 10;
pub const BASE_GRIND_MINUTES: u32 = 20;
pub const CHECK_TIME_REDUCTION_PER_RANK: f32 = 0.06;
pub const GRINDING_TOOL_TIME_FACTOR: f32 = 0.50;
pub const POPPY_TINCTURE_MATURATION_MINUTES: u64 = 60_480;
pub const POPPY_TINCTURE_HERB_GRAMS: u32 = 50;
pub const POPPY_TINCTURE_SPIRIT_ML: u32 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalPreparation {
    Cut,
    Ground,
}

pub fn physical_preparation_minutes(
    preparation: PhysicalPreparation,
    governing_check: f32,
    has_grinding_tool: bool,
) -> u32 {
    let base = match preparation {
        PhysicalPreparation::Cut => BASE_CUT_MINUTES,
        PhysicalPreparation::Ground => BASE_GRIND_MINUTES,
    };
    let check = if governing_check.is_finite() {
        governing_check.clamp(0.0, 5.0)
    } else {
        0.0
    };
    let tool = if preparation == PhysicalPreparation::Ground && has_grinding_tool {
        GRINDING_TOOL_TIME_FACTOR
    } else {
        1.0
    };
    ((base as f32) * (1.0 - CHECK_TIME_REDUCTION_PER_RANK * check) * tool)
        .ceil()
        .max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grinding_tool_halves_time_and_skill_uses_canonical_check() {
        assert_eq!(
            physical_preparation_minutes(PhysicalPreparation::Ground, 0.0, false),
            20
        );
        assert_eq!(
            physical_preparation_minutes(PhysicalPreparation::Ground, 0.0, true),
            10
        );
        assert!(
            physical_preparation_minutes(PhysicalPreparation::Cut, 5.0, false) < BASE_CUT_MINUTES
        );
    }
}
