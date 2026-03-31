bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct BodyPart: u8 {
        const LeftArm = 1 << 0;
        const RightArm = 1 << 1;
        const LeftLeg = 1 << 2;
        const RightLeg = 1 << 3;
        const Chest = 1 << 4;
        const Stomach = 1 << 5;
        const Head = 1 << 6;
    }
}

pub trait PlayerBody {
    fn body_part_health(&self, part: BodyPart) -> f32;

    fn body_weight(&self) -> f32;
}
