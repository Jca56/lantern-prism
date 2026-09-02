//! Records viewport passes (picking lives in `pick.rs`). Two phases per frame:
//! [`Renderer::prepare`] writes every uniform and syncs mesh buffers, then
//! [`Renderer::record`] only reads what was prepared while recording.

use prism_core::{Id, bytes};
use prism_doc::{DataKind, Doc, ObjectMode};
use prism_math::{Color, Mat4, Rect, Vec3};
use prism_render::Gpu;

use super::mesh_cache::MeshCache;
use super::pick::PickTarget;
use super::pipelines::Pipelines;
use super::uniforms::{OBJECT_STRIDE, ObjectUniforms, VIEW_STRIDE, ViewUniforms};
use crate::camera::Camera;
use crate::request::{Shading, ViewColors, ViewportRequest};

pub(super) struct Slots {
    buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
    stride: u64,
    capacity: u32,
    used: u32,
}

impl Slots {
    fn new(gpu: &Gpu, layout: &wgpu::BindGroupLayout, stride: u64, capacity: u32, label: &str, binding_size: u64) -> Self {
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(binding_size) }),
            }],
        });
        Self { buffer, bind_group, stride, capacity, used: 0 }
    }

    pub(super) fn ensure(&mut self, gpu: &Gpu, layout: &wgpu::BindGroupLayout, needed: u32, label: &str, binding_size: u64) {
        if needed > self.capacity {
            *self = Self::new(gpu, layout, self.stride, needed.next_power_of_two(), label, binding_size);
        }
        self.used = 0;
    }

    pub(super) fn push<T: bytes::Pod>(&mut self, gpu: &Gpu, value: &T) -> u32 {
        let offset = self.used as u64 * self.stride;
        gpu.queue.write_buffer(&self.buffer, offset, bytes::bytes_of(value));
        self.used += 1;
        offset as u32
    }
}

struct ObjectDraw {
    mesh: Id,
    object_offset: u32,
    edit: bool,
}

struct ViewportDraw {
    rect: Rect,
    view_offset: u32,
    shading: Shading,
    grid: bool,
    wire_overlay: bool,
    vert_overlay: bool,
    objects: Vec<ObjectDraw>,
}

/// Everything `record` needs, produced by `prepare`.
pub struct PreparedFrame {
    viewports: Vec<ViewportDraw>,
    /// Objects drawn per viewport, in draw order, for object picking.
    pub drawn: Vec<Vec<Id>>,
}

pub struct Renderer {
    pub(super) pipes: Pipelines,
    pub(super) views: Slots,
    pub(super) objects: Slots,
    pub meshes: MeshCache,
    pub(super) pick: Option<PickTarget>,
    /// Empty flag buffers so the grid can draw with no objects in the scene.
    dummy_flags: wgpu::BindGroup,
}

fn c4(c: Color) -> [f32; 4] {
    c.to_linear().to_gpu()
}

pub(super) fn view_uniforms(camera: &Camera, rect: Rect, size: [f32; 2], colors: &ViewColors) -> ViewUniforms {
    let aspect = rect.width() / rect.height().max(1.0);
    let cam_pos = camera.position();
    ViewUniforms {
        view_proj: camera.view_proj_relative(aspect).to_gpu(),
        inv_proj: camera.projection(aspect).inverse().unwrap_or(Mat4::IDENTITY).to_gpu(),
        view_rot: Mat4::from_mat3(camera.view_rotation()).to_gpu(),
        cam_pos: [cam_pos.x as f32, cam_pos.y as f32, cam_pos.z as f32, 1.0],
        viewport: [rect.min.x as f32, rect.min.y as f32, size[0], size[1]],
        bg: c4(colors.bg),
        grid: [1.0, 10.0, (camera.distance * 40.0) as f32, camera.ortho as u32 as f32],
        grid_colors: [c4(colors.grid_minor), c4(colors.grid_major), c4(colors.axis_x), c4(colors.axis_z)],
        overlay: [c4(colors.wire), c4(colors.vertex), c4(colors.selected), c4(colors.active)],
        point: [colors.point_size as f32, 0.0, 0.0, 0.0],
    }
}

pub(super) fn object_uniforms(doc: &Doc, id: Id, cam_pos: Vec3, colors: &ViewColors, pick_id: u32, element_pick: bool) -> Option<(ObjectUniforms, Id, bool)> {
    let obj = doc.objects.get(id)?;
    if obj.kind != DataKind::Mesh || !obj.visible {
        return None;
    }
    let block = doc.meshes.get(obj.data)?;
    let world = doc.object_matrix(id);
    let model = Mat4::from_translation(-cam_pos) * world;
    let normal = world.to_mat3().inverse().map_or(Mat4::IDENTITY, |m| Mat4::from_mat3(m.transpose()));
    let color = block
        .props
        .materials
        .first()
        .and_then(|&m| doc.materials.get(m))
        .map_or(colors.default_object, |m| m.color);
    let active = doc.active_object_id() == id;
    let edit = active && obj.mode == ObjectMode::Edit;
    let flags = (obj.selected as u32) | ((active as u32) << 1);
    let u = ObjectUniforms { model: model.to_gpu(), normal: normal.to_gpu(), color: c4(color), flags: [flags, pick_id, edit as u32, element_pick as u32] };
    Some((u, obj.data, edit))
}

impl Renderer {
    pub fn new(gpu: &Gpu, color_format: wgpu::TextureFormat) -> Self {
        let pipes = Pipelines::new(gpu, color_format);
        let views = Slots::new(gpu, &pipes.view_layout, VIEW_STRIDE, 8, "prism view slots", size_of::<ViewUniforms>() as u64);
        let objects = Slots::new(gpu, &pipes.object_layout, OBJECT_STRIDE, 64, "prism object slots", size_of::<ObjectUniforms>() as u64);
        let zero = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("prism empty flags"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let dummy_flags = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("prism empty flags"),
            layout: &pipes.flags_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: zero.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: zero.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: zero.as_entire_binding() },
            ],
        });
        Self { pipes, views, objects, meshes: MeshCache::new(), pick: None, dummy_flags }
    }

    /// Write uniforms and sync buffers for every viewport of this frame.
    pub fn prepare(&mut self, gpu: &Gpu, doc: &Doc, requests: &[ViewportRequest]) -> PreparedFrame {
        let scene_objects = doc.scene_objects();
        let n_obj = (requests.len() * scene_objects.len()).max(1) as u32;
        self.views.ensure(gpu, &self.pipes.view_layout, requests.len().max(1) as u32, "prism view slots", size_of::<ViewUniforms>() as u64);
        self.objects.ensure(gpu, &self.pipes.object_layout, n_obj, "prism object slots", size_of::<ObjectUniforms>() as u64);
        self.meshes.retain(|id| doc.meshes.contains(id));

        let mut viewports = Vec::with_capacity(requests.len());
        let mut drawn = Vec::with_capacity(requests.len());
        for req in requests {
            let cam = req.state.camera;
            let size = [req.rect.width() as f32, req.rect.height() as f32];
            let view_offset = self.views.push(gpu, &view_uniforms(&cam, req.rect, size, &req.colors));
            let cam_pos = cam.position();
            let mut objects = Vec::new();
            let mut ids = Vec::new();
            for (i, &id) in scene_objects.iter().enumerate() {
                let Some((u, mesh_id, edit)) = object_uniforms(doc, id, cam_pos, &req.colors, i as u32 + 1, false) else {
                    continue;
                };
                if let Some(block) = doc.meshes.get(mesh_id) {
                    self.meshes.sync(gpu, &self.pipes.flags_layout, mesh_id, block);
                }
                let object_offset = self.objects.push(gpu, &u);
                objects.push(ObjectDraw { mesh: mesh_id, object_offset, edit });
                ids.push(id);
            }
            let any_edit = objects.iter().any(|o| o.edit);
            viewports.push(ViewportDraw {
                rect: req.rect,
                view_offset,
                shading: req.state.shading,
                grid: req.state.overlays.grid,
                wire_overlay: req.state.overlays.wire || any_edit,
                vert_overlay: req.state.overlays.verts,
                objects,
            });
            drawn.push(ids);
        }
        PreparedFrame { viewports, drawn }
    }

    /// Record every viewport into `color` (loaded, not cleared) with `depth`.
    pub fn record(&self, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView, depth: &wgpu::TextureView, frame: &PreparedFrame) {
        for vp in &frame.viewports {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("prism viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            let r = vp.rect;
            let (x, y, w, h) = (r.min.x.max(0.0) as u32, r.min.y.max(0.0) as u32, r.width().max(1.0) as u32, r.height().max(1.0) as u32);
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(0, &self.views.bind_group, &[vp.view_offset]);
            // Grid first: it also paints the background and initial depth.
            // It ignores groups 1 and 2 but the shared layout wants them bound.
            pass.set_bind_group(1, &self.objects.bind_group, &[0]);
            pass.set_bind_group(2, &self.dummy_flags, &[]);
            let _ = vp.grid;
            pass.set_pipeline(&self.pipes.grid);
            pass.draw(0..3, 0..1);

            for o in &vp.objects {
                let Some(cage) = self.meshes.get(o.mesh) else {
                    continue;
                };
                // The modifier result is the surface you see; in edit mode
                // the base mesh sits over it as a cage of edges and dots.
                let surface = self.meshes.surface(o.mesh).unwrap_or(cage);
                let wire = if o.edit { cage } else { surface };
                pass.set_bind_group(1, &self.objects.bind_group, &[o.object_offset]);
                if vp.shading == Shading::Solid && surface.tri_index_count > 0 {
                    pass.set_bind_group(2, &surface.flags_bind_group, &[]);
                    pass.set_pipeline(&self.pipes.mesh);
                    pass.set_vertex_buffer(0, surface.corner_pos.slice(..));
                    pass.set_vertex_buffer(1, surface.corner_normal.slice(..));
                    pass.set_vertex_buffer(2, surface.corner_face.slice(..));
                    pass.set_index_buffer(surface.tri_index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..surface.tri_index_count, 0, 0..1);
                }
                if (vp.wire_overlay || vp.shading == Shading::Wire) && wire.edge_vertex_count > 0 {
                    pass.set_bind_group(2, &wire.flags_bind_group, &[]);
                    pass.set_pipeline(&self.pipes.wire);
                    pass.set_vertex_buffer(0, wire.edge_pos.slice(..));
                    pass.draw(0..wire.edge_vertex_count, 0..1);
                }
                if o.edit && vp.vert_overlay && cage.vert_count > 0 {
                    pass.set_bind_group(2, &cage.flags_bind_group, &[]);
                    pass.set_pipeline(&self.pipes.points);
                    pass.set_vertex_buffer(0, cage.vert_pos.slice(..));
                    pass.draw(0..6, 0..cage.vert_count);
                }
            }
        }
    }
}
