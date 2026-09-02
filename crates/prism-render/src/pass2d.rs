//! The 2D pass: uploads a `DrawList` and draws it in one call.

use prism_core::bytes;
use prism_core::impl_pod;
use prism_math::Color;
use prism_text::Atlas;

use crate::atlas_gpu::AtlasTexture;
use crate::draw2d::{DrawList, Vertex2d};
use crate::gpu::Gpu;
use crate::shader;

#[derive(Clone, Copy)]
#[repr(C)]
struct Uniforms {
    screen: [f32; 2],
    atlas: [f32; 2],
}
impl_pod!(Uniforms);

const INITIAL_VERTICES: usize = 6 * 1024;

pub struct Pass2d {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    bound_epoch: u64,
    uniform: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    staging: Vec<Vertex2d>,
    atlas: AtlasTexture,
}

impl Pass2d {
    /// `format` is the render target format (normally the surface's).
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, atlas: &Atlas) -> Self {
        let source = shader::load("ui.wgsl").expect("ui.wgsl preprocesses");
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("prism ui shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("prism 2d uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("prism 2d bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let atlas_tex = AtlasTexture::new(gpu, atlas);
        let bind_group = make_bind_group(gpu, &bind_group_layout, &uniform, &atlas_tex);

        let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prism 2d pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let attrs = wgpu::vertex_attr_array![
            0 => Float32x2, // pos
            1 => Float32x2, // uv
            2 => Float32x4, // color
            3 => Float32x4, // rect
            4 => Float32x4, // params
            5 => Float32x4, // clip
        ];
        let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prism 2d pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex2d>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_capacity = INITIAL_VERTICES;
        let vertex_buffer = make_vertex_buffer(gpu, vertex_capacity);
        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            bound_epoch: atlas_tex.binding_epoch,
            uniform,
            vertex_buffer,
            vertex_capacity,
            staging: Vec::with_capacity(vertex_capacity),
            atlas: atlas_tex,
        }
    }

    /// Draw `list` into `view` (`size` pixels). `clear` paints the background
    /// first; `None` composites over whatever is already there.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [u32; 2],
        list: &DrawList,
        atlas: &mut Atlas,
        clear: Option<Color>,
    ) {
        self.atlas.sync(gpu, atlas);
        if self.atlas.binding_epoch != self.bound_epoch {
            self.bind_group = make_bind_group(gpu, &self.bind_group_layout, &self.uniform, &self.atlas);
            self.bound_epoch = self.atlas.binding_epoch;
        }

        self.staging.clear();
        self.staging.extend(list.vertices().copied());
        if self.staging.len() > self.vertex_capacity {
            self.vertex_capacity = self.staging.len().next_power_of_two();
            self.vertex_buffer = make_vertex_buffer(gpu, self.vertex_capacity);
        }
        if !self.staging.is_empty() {
            gpu.queue.write_buffer(&self.vertex_buffer, 0, bytes::slice_as_bytes(&self.staging));
        }
        let u = Uniforms {
            screen: [size[0] as f32, size[1] as f32],
            atlas: [self.atlas.size() as f32, self.atlas.size() as f32],
        };
        gpu.queue.write_buffer(&self.uniform, 0, bytes::bytes_of(&u));

        let load = match clear {
            Some(c) => {
                let l = c.to_linear();
                wgpu::LoadOp::Clear(wgpu::Color { r: l.r, g: l.g, b: l.b, a: l.a })
            }
            None => wgpu::LoadOp::Load,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("prism 2d pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        if !self.staging.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.staging.len() as u32, 0..1);
        }
    }
}

fn make_vertex_buffer(gpu: &Gpu, capacity: usize) -> wgpu::Buffer {
    gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("prism 2d vertices"),
        size: (capacity * size_of::<Vertex2d>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_bind_group(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    atlas: &AtlasTexture,
) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("prism 2d bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(atlas.view()) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(atlas.sampler()) },
        ],
    })
}
