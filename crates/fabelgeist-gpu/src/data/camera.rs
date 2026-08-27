use crate::data::gpu::texture::{Texture2d, TextureFormat};
use crate::data::view::View;
use crate::globals::WgpuContext;
use anyhow::Result;
use fabelgeist_math::{Mat4, Transform, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub view: View,
    pub color: Texture2d,
    pub depth: Texture2d,
    pub depth_testing: bool,
}

impl Camera {
    pub fn pack(view: View, color: Texture2d, depth: Texture2d) -> Self {
        Self {
            view,
            color,
            depth,
            depth_testing: true,
        }
    }

    pub fn unpack(self) -> (View, Texture2d, Texture2d) {
        (self.view, self.color, self.depth)
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn color(&self) -> &Texture2d {
        &self.color
    }

    pub fn depth(&self) -> &Texture2d {
        &self.depth
    }

    pub fn depth_testing(&self) -> bool {
        self.depth_testing
    }

    pub fn new(
        context: &WgpuContext,
        resolution: Vec2,
        view: Option<View>,
        color_format: Option<TextureFormat>,
    ) -> Result<Self> {
        let view = view.unwrap_or_else(|| {
            View::new(
                Mat4::perspective(45.0, resolution.x / resolution.y, 0.1, 1000.0),
                Transform::identity(),
            )
        });
        let color = Texture2d::create(
            context,
            resolution,
            color_format.unwrap_or(TextureFormat::Rgba8Unorm),
        )?;
        color.clear_raw(context, fabelgeist_math::Vec4::new(0.0, 0.0, 0.0, 0.0))?;
        let depth = Texture2d::create(context, resolution, TextureFormat::Depth32Float)?;
        depth.clear_raw(context, fabelgeist_math::Vec4::new(1.0, 0.0, 0.0, 0.0))?;
        Ok(Self {
            view,
            color,
            depth,
            depth_testing: true,
        })
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.view.transform = transform;
        self
    }

    pub fn with_depth_testing(mut self, enabled: bool) -> Self {
        self.depth_testing = enabled;
        self
    }
}
