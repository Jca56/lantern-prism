//! Records viewport passes and performs picks. Two phases per frame:
//! [`Renderer::prepare`] writes every uniform and syncs mesh buffers, then
//! [`Renderer::record`] only reads what was prepared while recording.

use prism_core::{Id, bytes};
use prism_doc::{DataKind, Doc, ObjectMode};
use prism_math::{Color, Mat4, Rect, Vec2, Vec3};
use prism_render::Gpu;

use super::mesh_cache::MeshCache;
use super::pipelines::{DEPTH_FORMAT, PICK_FORMAT, Pipelines};
use super::uniforms::{OBJECT_STRIDE, ObjectUniforms, VIEW_STRIDE, ViewUniforms};
use crate::camera::Camera;
use crate::request::{PickMode, PickRequest, PickResult, Shading, ViewColors, ViewportRequest};

struct Slots {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
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

    fn ensure(&mut self, gpu: &Gpu, layout: &wgpu::BindGroupLayout, needed: u32, label: &str, binding_size: u64) {
        if needed > self.capacity {
            *self = Self::new(gpu, layout, self.stride, needed.next_power_of_two(), label, binding_size);
        }
        self.used = 0;
    }

    fn push<T: bytes::Pod>(&mut self, gpu: &Gpu, value: &T) -> u32 {
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

struct PickTarget {
    size: [u32; 2],
    view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    texture: wgpu::Texture,
    staging: wgpu::Buffer,
}

const PICK_WINDOW: u32 = 64;

pub struct Renderer {
    pipes: Pipelines,
    views: Slots,
    objects: Slots,
    pub meshes: MeshCache,
    pick: Option<PickTarget>,
    /// Empty flag buffers so the grid can draw with no objects in the scene.
    dummy_flags: wgpu::BindGroup,
}

fn c4(c: Color) -> [f32; 4] {
    c.to_linear().to_gpu()
}

fn view_uniforms(camera: &Camera, rect: Rect, size: [f32; 2], colors: &ViewColors) -> ViewUniforms {
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

fn object_uniforms(doc: &Doc, id: Id, cam_pos: Vec3, colors: &ViewColors, pick_id: u32, element_pick: bool) -> Option<(ObjectUniforms, Id, bool)> {
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
                let Some(g) = self.meshes.get(o.mesh) else {
                    continue;
                };
                pass.set_bind_group(1, &self.objects.bind_group, &[o.object_offset]);
                pass.set_bind_group(2, &g.flags_bind_group, &[]);
                if vp.shading == Shading::Solid && g.tri_index_count > 0 {
                    pass.set_pipeline(&self.pipes.mesh);
                    pass.set_vertex_buffer(0, g.corner_pos.slice(..));
                    pass.set_vertex_buffer(1, g.corner_normal.slice(..));
                    pass.set_vertex_buffer(2, g.corner_face.slice(..));
                    pass.set_index_buffer(g.tri_index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..g.tri_index_count, 0, 0..1);
                }
                if (vp.wire_overlay || vp.shading == Shading::Wire) && g.edge_vertex_count > 0 {
                    pass.set_pipeline(&self.pipes.wire);
                    pass.set_vertex_buffer(0, g.edge_pos.slice(..));
                    pass.draw(0..g.edge_vertex_count, 0..1);
                }
                if o.edit && vp.vert_overlay && g.vert_count > 0 {
                    pass.set_pipeline(&self.pipes.points);
                    pass.set_vertex_buffer(0, g.vert_pos.slice(..));
                    pass.draw(0..6, 0..g.vert_count);
                }
            }
        }
    }

    fn ensure_pick_target(&mut self, gpu: &Gpu, size: [u32; 2]) {
        if self.pick.as_ref().is_none_or(|p| p.size != size) {
            let tex = |format, usage| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("prism pick"),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let texture = tex(PICK_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
            let depth = tex(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("prism pick readback"),
                size: (PICK_WINDOW * PICK_WINDOW * 4) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.pick = Some(PickTarget {
                size,
                view: texture.create_view(&Default::default()),
                depth_view: depth.create_view(&Default::default()),
                texture,
                staging,
            });
        }
    }

    /// Render ids for one viewport and read back a window around the cursor.
    /// Synchronous: waits for the GPU. Only runs on a click. Needs no
    /// swapchain frame: it draws into its own target.
    pub fn pick(&mut self, gpu: &Gpu, doc: &Doc, req: &PickRequest) -> PickResult {
        let size = [req.rect.width().max(1.0) as u32, req.rect.height().max(1.0) as u32];
        let local = req.pos - req.rect.min;
        if local.x < 0.0 || local.y < 0.0 || local.x >= size[0] as f64 || local.y >= size[1] as f64 {
            return PickResult::Nothing;
        }
        let scene_objects = doc.scene_objects();
        self.views.ensure(gpu, &self.pipes.view_layout, 1, "prism view slots", size_of::<ViewUniforms>() as u64);
        self.objects.ensure(gpu, &self.pipes.object_layout, scene_objects.len().max(1) as u32, "prism object slots", size_of::<ObjectUniforms>() as u64);
        let element_mode = req.mode != PickMode::Object;
        let local_rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(size[0] as f64, size[1] as f64));
        let view_offset = self.views.push(gpu, &view_uniforms(&req.camera, local_rect, [size[0] as f32, size[1] as f32], &req.colors));
        let cam_pos = req.camera.position();

        let mut draws: Vec<(Id, u32, bool)> = Vec::new();
        let mut ids: Vec<Id> = Vec::new();
        for (i, &id) in scene_objects.iter().enumerate() {
            let Some((u, mesh_id, edit)) = object_uniforms(doc, id, cam_pos, &req.colors, i as u32 + 1, element_mode) else {
                continue;
            };
            if element_mode && !edit {
                continue;
            }
            if let Some(block) = doc.meshes.get(mesh_id) {
                self.meshes.sync(gpu, &self.pipes.flags_layout, mesh_id, block);
            }
            let off = self.objects.push(gpu, &u);
            draws.push((mesh_id, off, edit));
            ids.push(id);
        }

        self.ensure_pick_target(gpu, size);
        let target = self.pick.as_ref().expect("created");
        let mut encoder = gpu.create_encoder("prism pick");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("prism pick pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.views.bind_group, &[view_offset]);
            for &(mesh_id, off, _) in &draws {
                let Some(g) = self.meshes.get(mesh_id) else {
                    continue;
                };
                pass.set_bind_group(1, &self.objects.bind_group, &[off]);
                pass.set_bind_group(2, &g.flags_bind_group, &[]);
                if g.tri_index_count > 0 {
                    pass.set_pipeline(&self.pipes.pick_faces);
                    pass.set_vertex_buffer(0, g.corner_pos.slice(..));
                    pass.set_vertex_buffer(1, g.corner_normal.slice(..));
                    pass.set_vertex_buffer(2, g.corner_face.slice(..));
                    pass.set_index_buffer(g.tri_index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..g.tri_index_count, 0, 0..1);
                }
                // Faces always draw (they occlude); only the wanted kind of
                // finer element goes on top. Drawing the others too would
                // stamp their ids over the faces (a vertex dot is ~15 px) and
                // a face click near a corner would find nothing.
                if req.mode == PickMode::Edge && g.edge_vertex_count > 0 {
                    pass.set_pipeline(&self.pipes.pick_lines);
                    pass.set_vertex_buffer(0, g.edge_pos.slice(..));
                    pass.draw(0..g.edge_vertex_count, 0..1);
                }
                if req.mode == PickMode::Vertex && g.vert_count > 0 {
                    pass.set_pipeline(&self.pipes.pick_points);
                    pass.set_vertex_buffer(0, g.vert_pos.slice(..));
                    pass.draw(0..6, 0..g.vert_count);
                }
            }
        }
        // Copy a window around the cursor.
        let half = (PICK_WINDOW / 2) as i64;
        let cx = local.x as i64;
        let cy = local.y as i64;
        let x0 = (cx - half).clamp(0, size[0] as i64 - 1) as u32;
        let y0 = (cy - half).clamp(0, size[1] as i64 - 1) as u32;
        let w = PICK_WINDOW.min(size[0] - x0);
        let h = PICK_WINDOW.min(size[1] - y0);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target.texture, mip_level: 0, origin: wgpu::Origin3d { x: x0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &target.staging,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(PICK_WINDOW * 4), rows_per_image: None },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        gpu.queue.submit([encoder.finish()]);
        let slice = target.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        if rx.recv().ok().and_then(Result::ok).is_none() {
            return PickResult::Nothing;
        }
        let ids_px: Vec<u32> = {
            let data = slice.get_mapped_range();
            bytes::vec_from_bytes(&data)
        };
        target.staging.unmap();

        // Search the window for the best candidate of the wanted kind.
        let wanted_kind: u32 = match req.mode {
            PickMode::Object => 0,
            PickMode::Face => 1,
            PickMode::Edge => 2,
            PickMode::Vertex => 3,
        };
        let radius = if req.mode == PickMode::Face || req.mode == PickMode::Object { 2.0 } else { req.radius };
        let mut best: Option<(f64, u32)> = None;
        for py in 0..h {
            for px in 0..w {
                let id = ids_px[(py * PICK_WINDOW + px) as usize];
                if id == 0 || (id >> 30) != wanted_kind {
                    continue;
                }
                let dx = (x0 + px) as f64 + 0.5 - local.x;
                let dy = (y0 + py) as f64 + 0.5 - local.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, id & 0x3fff_ffff));
                }
            }
        }
        let Some((_, index)) = best else {
            return PickResult::Nothing;
        };
        match req.mode {
            PickMode::Object => ids.get(index as usize - 1).map_or(PickResult::Nothing, |&id| PickResult::Object(id)),
            _ => {
                let Some(&(mesh_id, _, _)) = draws.first() else {
                    return PickResult::Nothing;
                };
                let Some(g) = self.meshes.get(mesh_id) else {
                    return PickResult::Nothing;
                };
                let i = index as usize - 1;
                match req.mode {
                    PickMode::Vertex => g.buffers.vert_to_vert.get(i).map_or(PickResult::Nothing, |&v| PickResult::Vert(mesh_id, v)),
                    PickMode::Edge => g.buffers.edge_to_edge.get(i).map_or(PickResult::Nothing, |&e| PickResult::Edge(mesh_id, e)),
                    _ => g.buffers.face_handles.get(i).map_or(PickResult::Nothing, |&f| PickResult::Face(mesh_id, f)),
                }
            }
        }
    }
}
