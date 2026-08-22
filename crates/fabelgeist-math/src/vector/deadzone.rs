use crate::Vec2;

pub trait DeadzoneExt {
    fn deadzone(self, threshold: f32) -> Self;
}

impl DeadzoneExt for f32 {
    fn deadzone(self, threshold: f32) -> Self {
        if self.abs() < threshold {
            0.0
        } else {
            (self - self.signum() * threshold) / (1.0 - threshold)
        }
    }
}

impl DeadzoneExt for Vec2 {
    fn deadzone(self, threshold: f32) -> Self {
        let len = self.length();
        if len < threshold {
            Self::zero()
        } else {
            let scale = (len - threshold) / (1.0 - threshold);
            Self {
                x: (self.x / len) * scale,
                y: (self.y / len) * scale,
            }
        }
    }
}
