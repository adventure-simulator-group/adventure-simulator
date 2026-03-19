pub mod color;
pub mod depth_stencil;
pub mod ops;

pub use color::*;
pub use depth_stencil::*;
pub use ops::*;

#[derive(Clone, Debug, Default)]
pub struct RenderAttachments {
    pub colors: Vec<ColorAttachment>,
    pub depth_stencil: Option<DepthStencilAttachment>,
}

impl RenderAttachments {
    pub fn new(
        colors: Vec<ColorAttachment>,
        depth_stencil: Option<DepthStencilAttachment>,
    ) -> RenderAttachments {
        RenderAttachments {
            colors,
            depth_stencil,
        }
    }

    pub fn get_colors(&self) -> Vec<ColorAttachment> {
        self.colors.clone()
    }

    pub fn get_depth_stencil(&self) -> Option<DepthStencilAttachment> {
        self.depth_stencil.clone()
    }
}
