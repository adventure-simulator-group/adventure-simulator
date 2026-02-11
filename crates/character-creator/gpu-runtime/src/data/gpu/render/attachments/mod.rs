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
        colors: Option<gpu_runtime_base::Value>,
        depth_stencil: Option<DepthStencilAttachment>,
    ) -> RenderAttachments {
        let mut color_attachments = Vec::new();

        if let Some(val) = colors {
            // Check for single ColorAttachment first
            if let Some((att, _)) = val.as_any() {
                if let Some(c) = att.downcast_ref::<ColorAttachment>() {
                    color_attachments.push(c.clone());
                }
                // If not ColorAttachment, check if it's a Vector
                else if let Some(vec) = val.as_vector() {
                    for vec_val in vec {
                        if let Some((v_att, _)) = vec_val.as_any() {
                            if let Some(c) = v_att.downcast_ref::<ColorAttachment>() {
                                color_attachments.push(c.clone());
                            }
                        }
                    }
                }
            }
        }

        RenderAttachments {
            colors: color_attachments,
            depth_stencil,
        }
    }

    pub fn get_colors(&self) -> gpu_runtime_base::Result<Vec<Option<gpu_runtime_base::Value>>> {
        Ok(vec![Some(gpu_runtime_base::Value::Vector(
            self.colors
                .iter()
                .map(|c| gpu_runtime_base::Value::new_any(c.clone()))
                .collect(),
        ))])
    }

    pub fn get_depth_stencil(&self) -> gpu_runtime_base::Result<Vec<Option<gpu_runtime_base::Value>>> {
        Ok(vec![if let Some(ds) = &self.depth_stencil {
            Some(gpu_runtime_base::Value::new_any(ds.clone()))
        } else {
            None
        }])
    }
}

unsafe impl Send for RenderAttachments {}
unsafe impl Sync for RenderAttachments {}
