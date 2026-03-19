use anyhow::{Result, anyhow};
use std::sync::Arc;
use wgpu::{Adapter, Device, Instance, Queue};
pub mod blitter;
pub use blitter::Blitter;

#[derive(Clone)]
pub struct WgpuContext {
    pub instance: Arc<Instance>,
    pub adapter: Arc<Adapter>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    #[cfg(target_arch = "wasm32")]
    pub canvas: Option<web_sys::OffscreenCanvas>,
    pub surface: Option<Arc<wgpu::Surface<'static>>>,
    pub blitter: Option<Arc<Blitter>>,
    pub blit_lock: Arc<async_lock::Mutex<()>>,
}

impl WgpuContext {
    pub async fn new() -> Result<Self> {
        // Platform specific initialization
        #[cfg(target_arch = "wasm32")]
        let (canvas, surface, instance) = {
            let canvas = web_sys::OffscreenCanvas::new(1, 1)
                .map_err(|_| anyhow!("Failed to create OffscreenCanvas"))?;
            let desc = wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            };
            let instance = wgpu::util::new_instance_with_webgpu_detection(&desc).await;
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
                .map_err(|_| anyhow!("Failed to create surface"))?;
            (Some(canvas), Some(Arc::new(surface)), instance)
        };

        #[cfg(not(target_arch = "wasm32"))]
        let (surface, instance): (Option<Arc<wgpu::Surface<'static>>>, Instance) = {
            // For native headless, we don't have a surface or canvas (unless we create a window, but we want headless)
            #[cfg(windows)]
            let instance = Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::DX12,
                ..Default::default()
            });

            #[cfg(not(windows))]
            let instance = Instance::default();

            (None, instance)
        };

        let compatible_surface = surface
            .as_ref()
            .map(|s: &Arc<wgpu::Surface<'static>>| s.as_ref());

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| anyhow!("Failed to find an appropriate adapter"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: {
                    let mut features = wgpu::Features::empty();
                    if adapter
                        .features()
                        .contains(wgpu::Features::FLOAT32_FILTERABLE)
                    {
                        features |= wgpu::Features::FLOAT32_FILTERABLE;
                    }
                    features
                },
                required_limits: {
                    let mut limits = if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
                    } else {
                        wgpu::Limits::default()
                    };
                    let adapter_limits = adapter.limits();
                    limits.max_buffer_size = adapter_limits.max_buffer_size;
                    limits.max_storage_buffer_binding_size =
                        adapter_limits.max_storage_buffer_binding_size;
                    limits
                },
                memory_hints: wgpu::MemoryHints::Performance,
                trace: Default::default(),
            })
            .await
            .map_err(|_| anyhow!("Failed to create device"))?;

        #[cfg(target_arch = "wasm32")]
        if let (Some(canvas), Some(surface)) = (&canvas, &surface) {
            let width = canvas.width();
            let height = canvas.height();
            let config = surface.get_default_config(&adapter, width, height).unwrap();
            surface.configure(&device, &config);
        }

        // Blit pipeline (used by Texture3D widget)
        // We use Rgba8UnormSrgb because DisplayTexture3DWidget creates its temporary 2D texture in this format
        let blitter = Some(Arc::new(Blitter::new(
            &device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )));

        Ok(WgpuContext {
            instance: Arc::new(instance),
            adapter: Arc::new(adapter),
            device: Arc::new(device),
            queue: Arc::new(queue),
            #[cfg(target_arch = "wasm32")]
            canvas,
            surface,
            blitter,
            blit_lock: Arc::new(async_lock::Mutex::new(())),
        })
    }
}
