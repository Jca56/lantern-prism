//! Instance, adapter, device, queue.

use core::fmt;

use prism_core::{block_on, log_info};

#[derive(Debug)]
pub enum GpuError {
    NoAdapter(String),
    NoDevice(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuError::NoAdapter(e) => write!(f, "no compatible GPU adapter: {e}"),
            GpuError::NoDevice(e) => write!(f, "could not open GPU device: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Create an instance and pick a device. Pass the window surface so the
    /// adapter is guaranteed to be able to present to it; `None` is headless.
    pub fn new(compatible_surface: Option<&wgpu::Surface<'_>>) -> Result<Gpu, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        Self::with_instance(instance, compatible_surface)
    }

    pub fn with_instance(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Gpu, GpuError> {
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface,
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        log_info!("gpu: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("prism device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| GpuError::NoDevice(e.to_string()))?;

        Ok(Gpu { instance, adapter, device, queue })
    }

    pub fn create_encoder(&self, label: &str) -> wgpu::CommandEncoder {
        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) })
    }
}
