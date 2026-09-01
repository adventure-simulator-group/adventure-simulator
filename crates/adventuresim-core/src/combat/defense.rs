use super::{DODGE_OVEREXTENSION_SCALE, DefenderResponse, WEAPON_DEFENSE_REBOUND_SCALE};

impl DefenderResponse {
    pub fn factor(&self) -> f32 {
        match self {
            Self::None | Self::Block => 1.0,
            &Self::Parry {
                input_reflex,
                precision,
            } => 2.0 * input_reflex * precision.clamp(0.0, 1.0),
            &Self::Dodge { input_reflex } => 1.5 * input_reflex,
        }
    }

    pub fn is_weapon_contact(self) -> bool {
        matches!(self, Self::Block | Self::Parry { .. })
    }

    pub(super) fn rebound_scale(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Block | Self::Parry { .. } => WEAPON_DEFENSE_REBOUND_SCALE,
            Self::Dodge { .. } => DODGE_OVEREXTENSION_SCALE,
        }
    }
}
