//! Window surface: configuration, resize, frame acquisition.

use prism_core::{log_info, log_warn};

use crate::gpu::Gpu;

pub struct SurfaceTarget {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl SurfaceTarget {
    /// Configure `surface` for `gpu` at `width × height` physical pixels.
    /// Prefers an sRGB format so shaders work in linear light.
    pub fn new(gpu: &Gpu, surface: wgpu::Surface<'static>, width: u32, height: u32) -> Self {
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or_else(|| caps.formats.first().copied().expect("surface has no formats"));
        let mut config = surface
            .get_default_config(&gpu.adapter, width.max(1), height.max(1))
            .expect("surface is not compatible with the adapter");
        config.format = format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&gpu.device, &config);
        log_info!("surface: {format:?} {}x{} {:?}", config.width, config.height, config.present_mode);
        Self { surface, config }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if width == self.config.width && height == self.config.height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// The next frame's texture, or `None` when this frame should be skipped
    /// (the surface was lost and has been reconfigured, or timed out).
    pub fn acquire(&mut self, gpu: &Gpu) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            Ok(frame) => Some(frame),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            Err(wgpu::SurfaceError::Timeout) => None,
            Err(e) => {
                log_warn!("surface error: {e}");
                None
            }
        }
    }
}
