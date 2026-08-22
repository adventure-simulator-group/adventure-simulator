use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Vec2 {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn ones() -> Self {
        Self { x: 1.0, y: 1.0 }
    }

    pub fn from_scalar(s: f32) -> Self {
        Self { x: s, y: s }
    }

    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    pub fn break_(self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self::new(self.x / len, self.y / len)
        } else {
            self
        }
    }

    /// Add two Vec2 values.
    pub fn add(a: Self, b: Self) -> Self {
        Self::new(a.x + b.x, a.y + b.y)
    }

    /// Subtract two Vec2 values.
    pub fn sub(a: Self, b: Self) -> Self {
        Self::new(a.x - b.x, a.y - b.y)
    }

    /// Multiply a Vec2 by a scalar.
    pub fn mul(value: Self, scalar: f32) -> Self {
        Self::new(value.x * scalar, value.y * scalar)
    }

    /// Negate a Vec2.
    pub fn neg(value: Self) -> Self {
        Self::new(-value.x, -value.y)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl std::fmt::Display for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}
