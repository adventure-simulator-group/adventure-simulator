use crate::data::{
    gpu::buffer::Buffer,
    gpu::sampler::Sampler,
    matrix::{Mat2, Mat3, Mat4},
    texture::{Texture2D, Texture3D},
    transform::Transform,
    vector::{Vec2, Vec3, Vec4},
};

#[derive(Clone, Debug)]
pub enum PassParameter {
    Number(f64),
    Unsigned(u32),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    Mat2(Mat2),
    Mat3(Mat3),
    Mat4(Mat4),
    Transform(Transform),
    Texture2D(Texture2D),
    Texture3D(Texture3D),
    Sampler(Sampler),
    Buffer(Buffer),
}

impl From<Texture2D> for PassParameter {
    fn from(value: Texture2D) -> Self {
        Self::Texture2D(value)
    }
}

impl From<Texture3D> for PassParameter {
    fn from(value: Texture3D) -> Self {
        Self::Texture3D(value)
    }
}

impl From<Sampler> for PassParameter {
    fn from(value: Sampler) -> Self {
        Self::Sampler(value)
    }
}

impl From<Buffer> for PassParameter {
    fn from(value: Buffer) -> Self {
        Self::Buffer(value)
    }
}

impl From<Vec2> for PassParameter {
    fn from(value: Vec2) -> Self {
        Self::Vec2(value)
    }
}

impl From<Vec3> for PassParameter {
    fn from(value: Vec3) -> Self {
        Self::Vec3(value)
    }
}

impl From<Vec4> for PassParameter {
    fn from(value: Vec4) -> Self {
        Self::Vec4(value)
    }
}

impl From<Mat2> for PassParameter {
    fn from(value: Mat2) -> Self {
        Self::Mat2(value)
    }
}

impl From<Mat3> for PassParameter {
    fn from(value: Mat3) -> Self {
        Self::Mat3(value)
    }
}

impl From<Mat4> for PassParameter {
    fn from(value: Mat4) -> Self {
        Self::Mat4(value)
    }
}

impl From<Transform> for PassParameter {
    fn from(value: Transform) -> Self {
        Self::Transform(value)
    }
}

impl From<f32> for PassParameter {
    fn from(value: f32) -> Self {
        Self::Number(value as f64)
    }
}

impl From<f64> for PassParameter {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<u32> for PassParameter {
    fn from(value: u32) -> Self {
        Self::Unsigned(value)
    }
}

impl From<i32> for PassParameter {
    fn from(value: i32) -> Self {
        Self::Unsigned(value as u32)
    }
}
