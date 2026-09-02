//! GPU mirror of `prism_text::Atlas`. Uploads dirty rects; recreates the
//! texture when the CPU atlas grows.

use prism_text::Atlas;

use crate::gpu::Gpu;

pub struct AtlasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    generation: u64,
    /// Bumped whenever `view` changes so bind groups can be rebuilt.
    pub(crate) binding_epoch: u64,
}

impl AtlasTexture {
    pub fn new(gpu: &Gpu, atlas: &Atlas) -> Self {
        let (texture, view) = create(gpu, atlas.size());
        // Glyphs are placed 1:1 texel→pixel; AA lives in the coverage values.
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("prism atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let mut me = Self { texture, view, sampler, size: atlas.size(), generation: atlas.generation(), binding_epoch: 0 };
        me.upload_all(gpu, atlas);
        me
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// Bring the GPU copy up to date. Returns `true` if the texture was
    /// recreated (bind groups must be rebuilt).
    pub fn sync(&mut self, gpu: &Gpu, atlas: &mut Atlas) -> bool {
        let mut recreated = false;
        if atlas.generation() != self.generation || atlas.size() != self.size {
            let (t, v) = create(gpu, atlas.size());
            self.texture = t;
            self.view = v;
            self.size = atlas.size();
            self.generation = atlas.generation();
            self.binding_epoch += 1;
            self.upload_all(gpu, atlas);
            atlas.take_dirty();
            recreated = true;
        } else if let Some(d) = atlas.take_dirty() {
            let stride = atlas.size() * 4;
            let start = (d.y * stride + d.x * 4) as usize;
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: d.x, y: d.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels()[start..],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(d.h) },
                wgpu::Extent3d { width: d.w, height: d.h, depth_or_array_layers: 1 },
            );
        }
        recreated
    }

    fn upload_all(&mut self, gpu: &Gpu, atlas: &Atlas) {
        let size = atlas.size();
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.pixels(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: Some(size) },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
    }
}

fn create(gpu: &Gpu, size: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("prism glyph atlas"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Linear: texels are premultiplied linear coverage / color.
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
