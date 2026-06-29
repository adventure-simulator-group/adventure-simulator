pub type BodyParts = enumflags2::BitFlags<BodyPart>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[enumflags2::bitflags]
#[repr(u8)]
pub enum BodyPart {
    LeftArm = 1 << 0,
    RightArm = 1 << 1,
    LeftLeg = 1 << 2,
    RightLeg = 1 << 3,
    Chest = 1 << 4,
    Stomach = 1 << 5,
    Head = 1 << 6,
}

impl BodyPart {
    pub const ARMS: BodyParts = enumflags2::make_bitflags!(BodyPart::{LeftArm | RightArm});
    pub const LEGS: BodyParts = enumflags2::make_bitflags!(BodyPart::{LeftLeg | RightLeg});
    pub const LIMBS: BodyParts =
        enumflags2::make_bitflags!(BodyPart::{LeftArm | LeftLeg | RightArm | RightLeg});

    pub const UPPER_BODY: BodyParts =
        enumflags2::make_bitflags!(BodyPart::{LeftArm | RightArm | Chest | Stomach | Head});
    pub const LOWER_BODY: BodyParts = enumflags2::make_bitflags!(BodyPart::{LeftLeg | RightLeg});
    pub const FULL_BODY: BodyParts = enumflags2::make_bitflags!(BodyPart::{LeftArm | RightArm | Chest | Stomach | Head | LeftLeg | RightLeg});

    pub fn as_parts(&self) -> BodyParts {
        BodyParts::from_flag(*self)
    }
}

impl std::fmt::Display for BodyPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyPart::Head => f.write_str("Head"),
            BodyPart::Chest => f.write_str("Chest"),
            BodyPart::Stomach => f.write_str("Stomach"),
            BodyPart::LeftArm => f.write_str("Left Arm"),
            BodyPart::RightArm => f.write_str("Right Arm"),
            BodyPart::LeftLeg => f.write_str("Left Leg"),
            BodyPart::RightLeg => f.write_str("Right Leg"),
        }
    }
}

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerBody {
    fn body_part_health(&self, part: BodyPart) -> f32;
    fn body_weight(&self) -> f32;
    fn primary_side(&self) -> BodySide;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySide {
    Left,
    Right,
    Both,
}

/// Weights for each limb used in skill checks and calculations.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct LimbWeights {
    pub left_arm: f32,
    pub right_arm: f32,
    pub left_leg: f32,
    pub right_leg: f32,
}

impl LimbWeights {
    pub const fn just_left_arm() -> Self {
        Self {
            left_arm: 1.0,
            right_arm: 0.0,
            left_leg: 0.0,
            right_leg: 0.0,
        }
    }

    pub const fn just_right_arm() -> Self {
        Self {
            left_arm: 0.0,
            right_arm: 1.0,
            left_leg: 0.0,
            right_leg: 0.0,
        }
    }

    pub const fn both_arms() -> Self {
        Self {
            left_arm: 0.5,
            right_arm: 0.5,
            left_leg: 0.0,
            right_leg: 0.0,
        }
    }

    pub const fn just_left_leg() -> Self {
        Self {
            left_arm: 0.0,
            right_arm: 0.0,
            left_leg: 1.0,
            right_leg: 0.0,
        }
    }

    pub const fn just_right_leg() -> Self {
        Self {
            left_arm: 0.0,
            right_arm: 0.0,
            left_leg: 0.0,
            right_leg: 1.0,
        }
    }

    pub const fn both_legs() -> Self {
        Self {
            left_arm: 0.0,
            right_arm: 0.0,
            left_leg: 0.5,
            right_leg: 0.5,
        }
    }

    pub const fn all_equal() -> Self {
        Self {
            left_arm: 0.25,
            right_arm: 0.25,
            left_leg: 0.25,
            right_leg: 0.25,
        }
    }

    pub const fn normalize(self) -> Self {
        let total = self.left_arm + self.right_arm + self.left_leg + self.right_leg;
        Self {
            left_arm: self.left_arm / total,
            right_arm: self.right_arm / total,
            left_leg: self.left_leg / total,
            right_leg: self.right_leg / total,
        }
    }

    pub const fn arm(side: BodySide, primary: BodySide) -> Self {
        match (side, primary) {
            (BodySide::Left, _) => Self::just_left_arm(),
            (BodySide::Right, _) => Self::just_right_arm(),
            (BodySide::Both, BodySide::Left) => Self {
                left_arm: 0.75,
                right_arm: 0.25,
                left_leg: 0.0,
                right_leg: 0.0,
            },
            (BodySide::Both, BodySide::Right) => Self {
                left_arm: 0.25,
                right_arm: 0.75,
                left_leg: 0.0,
                right_leg: 0.0,
            },
            (BodySide::Both, BodySide::Both) => Self {
                left_arm: 0.50,
                right_arm: 0.50,
                left_leg: 0.0,
                right_leg: 0.0,
            },
        }
    }

    pub fn by_part(&self, part: BodyPart) -> f32 {
        match part {
            BodyPart::LeftArm => self.left_arm,
            BodyPart::RightArm => self.right_arm,
            BodyPart::LeftLeg => self.left_leg,
            BodyPart::RightLeg => self.right_leg,
            _ => 0.0,
        }
    }
}
