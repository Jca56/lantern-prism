//! Every render pipeline the viewport uses, built once per surface format.

use prism_render::{Gpu, shader};

use super::uniforms::{OBJECT_STRIDE, VIEW_STRIDE};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const PICK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

pub struct Pipelines {
    pub view_layout: wgpu::BindGroupLayout,
    pub object_layout: wgpu::BindGroupLayout,
    pub flags_layout: wgpu::BindGroupLayout,
    pub grid: wgpu::RenderPipeline,
    pub mesh: wgpu::RenderPipeline,
    pub wire: wgpu::RenderPipeline,
    pub points: wgpu::RenderPipeline,
    pub pick_faces: wgpu::RenderPipeline,
    pub pick_lines: wgpu::RenderPipeline,
    pub pick_points: wgpu::RenderPipeline,
}

fn uniform_layout(gpu: &Gpu, label: &str, dynamic: bool, size: u64) -> wgpu::BindGroupLayout {
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        }],
    })
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn depth(compare: wgpu::CompareFunction, write: bool, bias: i32) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: write,
        depth_compare: compare,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState { constant: bias, slope_scale: if bias != 0 { 1.0 } else { 0.0 }, clamp: 0.0 },
    }
}

const ALPHA_BLEND: wgpu::BlendState = wgpu::BlendState::ALPHA_BLENDING;

fn module(gpu: &Gpu, name: &str) -> wgpu::ShaderModule {
    let src = shader::load(name).unwrap_or_else(|e| panic!("{name}: {e}"));
    gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(src.into()) })
}

fn pos_layout(step: wgpu::VertexStepMode) -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    wgpu::VertexBufferLayout { array_stride: 12, step_mode: step, attributes: &ATTR }
}

fn normal_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![1 => Float32x3];
    wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTR }
}

fn face_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![2 => Uint32];
    wgpu::VertexBufferLayout { array_stride: 4, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTR }
}

struct Spec<'a> {
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [wgpu::VertexBufferLayout<'a>],
    topology: wgpu::PrimitiveTopology,
    target: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    depth: wgpu::DepthStencilState,
}

fn build(gpu: &Gpu, layout: &wgpu::PipelineLayout, s: Spec) -> wgpu::RenderPipeline {
    gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(s.label),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: s.module, entry_point: Some(s.vs), buffers: s.buffers, compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: s.module,
            entry_point: Some(s.fs),
            targets: &[Some(wgpu::ColorTargetState { format: s.target, blend: s.blend, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: s.topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(s.depth),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Pipelines {
    pub fn new(gpu: &Gpu, color_format: wgpu::TextureFormat) -> Self {
        let view_layout = uniform_layout(gpu, "prism view uniforms", true, size_of::<super::uniforms::ViewUniforms>() as u64);
        let object_layout = uniform_layout(gpu, "prism object uniforms", true, size_of::<super::uniforms::ObjectUniforms>() as u64);
        let flags_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("prism element flags"),
            entries: &[storage_entry(0), storage_entry(1), storage_entry(2)],
        });
        let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prism viewport layout"),
            bind_group_layouts: &[&view_layout, &object_layout, &flags_layout],
            immediate_size: 0,
        });
        let _ = (VIEW_STRIDE, OBJECT_STRIDE);

        let grid_m = module(gpu, "grid.wgsl");
        let mesh_m = module(gpu, "mesh.wgsl");
        let wire_m = module(gpu, "wire.wgsl");
        let points_m = module(gpu, "points.wgsl");
        let pick_m = module(gpu, "pick.wgsl");
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let lines = wgpu::PrimitiveTopology::LineList;
        let mesh_buffers = [pos_layout(wgpu::VertexStepMode::Vertex), normal_layout(), face_layout()];
        let line_buffers = [pos_layout(wgpu::VertexStepMode::Vertex)];
        let point_buffers = [pos_layout(wgpu::VertexStepMode::Instance)];

        let grid = build(gpu, &layout, Spec {
            label: "prism grid", module: &grid_m, vs: "vs_main", fs: "fs_main", buffers: &[], topology: tri,
            target: color_format, blend: None, depth: depth(wgpu::CompareFunction::Always, true, 0),
        });
        let mesh = build(gpu, &layout, Spec {
            label: "prism mesh solid", module: &mesh_m, vs: "vs_main", fs: "fs_main", buffers: &mesh_buffers, topology: tri,
            target: color_format, blend: None, depth: depth(wgpu::CompareFunction::Greater, true, 0),
        });
        let wire = build(gpu, &layout, Spec {
            label: "prism wire", module: &wire_m, vs: "vs_main", fs: "fs_main", buffers: &line_buffers, topology: lines,
            target: color_format, blend: Some(ALPHA_BLEND), depth: depth(wgpu::CompareFunction::GreaterEqual, false, 2),
        });
        let points = build(gpu, &layout, Spec {
            label: "prism points", module: &points_m, vs: "vs_main", fs: "fs_main", buffers: &point_buffers, topology: tri,
            target: color_format, blend: Some(ALPHA_BLEND), depth: depth(wgpu::CompareFunction::GreaterEqual, false, 4),
        });
        let pick_faces = build(gpu, &layout, Spec {
            label: "prism pick faces", module: &pick_m, vs: "vs_faces", fs: "fs_pick", buffers: &mesh_buffers, topology: tri,
            target: PICK_FORMAT, blend: None, depth: depth(wgpu::CompareFunction::Greater, true, 0),
        });
        let pick_lines = build(gpu, &layout, Spec {
            label: "prism pick lines", module: &pick_m, vs: "vs_lines", fs: "fs_pick", buffers: &line_buffers, topology: lines,
            target: PICK_FORMAT, blend: None, depth: depth(wgpu::CompareFunction::GreaterEqual, true, 2),
        });
        let pick_points = build(gpu, &layout, Spec {
            label: "prism pick points", module: &pick_m, vs: "vs_points", fs: "fs_pick", buffers: &point_buffers, topology: tri,
            target: PICK_FORMAT, blend: None, depth: depth(wgpu::CompareFunction::GreaterEqual, true, 4),
        });
        Self { view_layout, object_layout, flags_layout, grid, mesh, wire, points, pick_faces, pick_lines, pick_points }
    }
}
