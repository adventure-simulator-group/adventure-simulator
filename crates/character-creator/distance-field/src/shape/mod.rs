use bevy_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SdfShape {
    Sphere(SdfSphere),
    Box(SdfBox),
}

impl From<SdfSphere> for SdfShape {
    fn from(sphere: SdfSphere) -> Self {
        Self::Sphere(sphere)
    }
}

impl From<SdfBox> for SdfShape {
    fn from(box_: SdfBox) -> Self {
        Self::Box(box_)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfSphere {
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfBox {
    pub size: Vec3,
}