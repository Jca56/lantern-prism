//! ID picking (D022, D025): render object or element ids into an `R32Uint`
//! target the size of the viewport, read a region back synchronously and
//! decode it. A click reads a small window and takes the nearest id of the
//! wanted kind; a box select reads its whole rectangle and keeps every id.

use std::collections::BTreeSet;

use prism_core::{Id, bytes};
use prism_doc::Doc;
use prism_math::{Rect, Vec2};
use prism_render::Gpu;

use super::pipelines::{DEPTH_FORMAT, PICK_FORMAT};
use super::renderer::{Renderer, object_uniforms, view_uniforms};
use super::uniforms::{ObjectUniforms, ViewUniforms};
use crate::request::{PickMode, PickRequest, PickResult, PickSet};

pub(super) struct PickTarget {
    size: [u32; 2],
    view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    texture: wgpu::Texture,
    /// Readback buffer and its capacity; grows to the largest region read.
    staging: Option<(wgpu::Buffer, u64)>,
}

/// Side of the window read back around a click.
const PICK_WINDOW: u32 = 64;

/// What one ID pass drew: object ids are 1-based indices into `objects`;
/// element ids belong to `mesh`.
struct IdPass {
    objects: Vec<Id>,
    mesh: Option<Id>,
}

fn wanted_kind(mode: PickMode) -> u32 {
    match mode {
        PickMode::Object => 0,
        PickMode::Face => 1,
        PickMode::Edge => 2,
        PickMode::Vertex => 3,
    }
}

impl Renderer {
    pub(super) fn ensure_pick_target(&mut self, gpu: &Gpu, size: [u32; 2]) {
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
            let staging = self.pick.take().and_then(|p| p.staging);
            self.pick = Some(PickTarget { size, view: texture.create_view(&Default::default()), depth_view: depth.create_view(&Default::default()), texture, staging });
        }
    }

    /// Record the ID pass for `req` into `encoder`. Faces always draw (they
    /// occlude); only the wanted kind of finer element goes on top, since a
    /// vertex dot stamped over a face would hide it from a face click.
    fn render_ids(&mut self, gpu: &Gpu, doc: &Doc, req: &PickRequest, size: [u32; 2], encoder: &mut wgpu::CommandEncoder) -> IdPass {
        let scene_objects = doc.scene_objects();
        self.views.ensure(gpu, &self.pipes.view_layout, 1, "prism view slots", size_of::<ViewUniforms>() as u64);
        self.objects.ensure(gpu, &self.pipes.object_layout, scene_objects.len().max(1) as u32, "prism object slots", size_of::<ObjectUniforms>() as u64);
        let element_mode = req.mode != PickMode::Object;
        let local_rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(size[0] as f64, size[1] as f64));
        let view_offset = self.views.push(gpu, &view_uniforms(&req.camera, local_rect, [size[0] as f32, size[1] as f32], &req.colors));
        let cam_pos = req.camera.position();

        let mut draws: Vec<(Id, u32)> = Vec::new();
        let mut pass_info = IdPass { objects: Vec::new(), mesh: None };
        for &id in &scene_objects {
            let pick_id = pass_info.objects.len() as u32 + 1;
            let Some((u, mesh_id, edit)) = object_uniforms(doc, id, cam_pos, &req.colors, pick_id, element_mode) else {
                continue;
            };
            if element_mode && !edit {
                continue;
            }
            if edit {
                pass_info.mesh.get_or_insert(mesh_id);
            }
            if let Some(block) = doc.meshes.get(mesh_id) {
                self.meshes.sync(gpu, &self.pipes.flags_layout, mesh_id, block);
            }
            let off = self.objects.push(gpu, &u);
            draws.push((mesh_id, off));
            pass_info.objects.push(id);
        }

        self.ensure_pick_target(gpu, size);
        let target = self.pick.as_ref().expect("created");
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
        for &(mesh_id, off) in &draws {
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
        drop(pass);
        pass_info
    }

    /// Copy a `w × h` region of the pick target out, submit, and wait. The
    /// result is dense, row-major, `w * h` ids.
    fn read_ids(&mut self, gpu: &Gpu, mut encoder: wgpu::CommandEncoder, x0: u32, y0: u32, w: u32, h: u32) -> Option<Vec<u32>> {
        let target = self.pick.as_mut()?;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let pitch = (w * 4).div_ceil(align) * align;
        let needed = pitch as u64 * h as u64;
        if target.staging.as_ref().is_none_or(|(_, cap)| *cap < needed) {
            let cap = needed.max(PICK_WINDOW as u64 * align as u64).next_power_of_two();
            let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("prism pick readback"),
                size: cap,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            target.staging = Some((buffer, cap));
        }
        let (staging, _) = target.staging.as_ref()?;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target.texture, mip_level: 0, origin: wgpu::Origin3d { x: x0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: staging, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pitch), rows_per_image: None } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        gpu.queue.submit([encoder.finish()]);
        let slice = staging.slice(..needed);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        rx.recv().ok().and_then(Result::ok)?;
        let ids = {
            let data = slice.get_mapped_range();
            let mut ids: Vec<u32> = Vec::with_capacity((w * h) as usize);
            for row in 0..h as usize {
                let start = row * pitch as usize;
                let row_ids: Vec<u32> = bytes::vec_from_bytes(&data[start..start + (w * 4) as usize]);
                ids.extend_from_slice(&row_ids);
            }
            ids
        };
        staging.unmap();
        Some(ids)
    }

    fn decode(&self, pass: &IdPass, mode: PickMode, raw: u32) -> Option<PickResult> {
        let index = (raw & 0x3fff_ffff) as usize;
        let i = index.checked_sub(1)?;
        match mode {
            PickMode::Object => pass.objects.get(i).map(|&id| PickResult::Object(id)),
            _ => {
                let mesh_id = pass.mesh?;
                let g = self.meshes.get(mesh_id)?;
                Some(match mode {
                    PickMode::Vertex => PickResult::Vert(mesh_id, *g.buffers.vert_to_vert.get(i)?),
                    PickMode::Edge => PickResult::Edge(mesh_id, *g.buffers.edge_to_edge.get(i)?),
                    _ => PickResult::Face(mesh_id, *g.buffers.face_handles.get(i)?),
                })
            }
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
        let mut encoder = gpu.create_encoder("prism pick");
        let pass = self.render_ids(gpu, doc, req, size, &mut encoder);
        let half = (PICK_WINDOW / 2) as i64;
        let x0 = (local.x as i64 - half).clamp(0, size[0] as i64 - 1) as u32;
        let y0 = (local.y as i64 - half).clamp(0, size[1] as i64 - 1) as u32;
        let w = PICK_WINDOW.min(size[0] - x0);
        let h = PICK_WINDOW.min(size[1] - y0);
        let Some(ids) = self.read_ids(gpu, encoder, x0, y0, w, h) else {
            return PickResult::Nothing;
        };
        // Nearest id of the wanted kind. Faces and objects are under the
        // cursor or not; vertices and edges get the request's grab radius.
        let wanted = wanted_kind(req.mode);
        let radius = if matches!(req.mode, PickMode::Face | PickMode::Object) { 2.0 } else { req.radius };
        let mut best: Option<(f64, u32)> = None;
        for py in 0..h {
            for px in 0..w {
                let raw = ids[(py * w + px) as usize];
                if raw == 0 || (raw >> 30) != wanted {
                    continue;
                }
                let dx = (x0 + px) as f64 + 0.5 - local.x;
                let dy = (y0 + py) as f64 + 0.5 - local.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, raw));
                }
            }
        }
        best.and_then(|(_, raw)| self.decode(&pass, req.mode, raw)).unwrap_or(PickResult::Nothing)
    }

    /// Every object or element of the wanted kind with a visible pixel inside
    /// `req.region` (D025). Synchronous, like [`Self::pick`].
    pub fn pick_box(&mut self, gpu: &Gpu, doc: &Doc, req: &PickRequest) -> PickSet {
        let size = [req.rect.width().max(1.0) as u32, req.rect.height().max(1.0) as u32];
        let r = req.region.intersection(&req.rect);
        if r.is_empty() {
            return PickSet::Nothing;
        }
        let x0 = ((r.min.x - req.rect.min.x).floor().max(0.0) as u32).min(size[0] - 1);
        let y0 = ((r.min.y - req.rect.min.y).floor().max(0.0) as u32).min(size[1] - 1);
        let x1 = ((r.max.x - req.rect.min.x).ceil() as u32).clamp(x0 + 1, size[0]);
        let y1 = ((r.max.y - req.rect.min.y).ceil() as u32).clamp(y0 + 1, size[1]);
        let mut encoder = gpu.create_encoder("prism pick box");
        let pass = self.render_ids(gpu, doc, req, size, &mut encoder);
        let Some(ids) = self.read_ids(gpu, encoder, x0, y0, x1 - x0, y1 - y0) else {
            return PickSet::Nothing;
        };
        let wanted = wanted_kind(req.mode);
        let seen: BTreeSet<u32> = ids.into_iter().filter(|&raw| raw != 0 && (raw >> 30) == wanted).collect();
        let hits = seen.iter().filter_map(|&raw| self.decode(&pass, req.mode, raw));
        match req.mode {
            PickMode::Object => {
                let v: Vec<Id> = hits.filter_map(|h| if let PickResult::Object(id) = h { Some(id) } else { None }).collect();
                if v.is_empty() { PickSet::Nothing } else { PickSet::Objects(v) }
            }
            PickMode::Vertex => {
                let v: Vec<_> = hits.filter_map(|h| if let PickResult::Vert(_, x) = h { Some(x) } else { None }).collect();
                match (pass.mesh, v.is_empty()) {
                    (Some(m), false) => PickSet::Verts(m, v),
                    _ => PickSet::Nothing,
                }
            }
            PickMode::Edge => {
                let v: Vec<_> = hits.filter_map(|h| if let PickResult::Edge(_, x) = h { Some(x) } else { None }).collect();
                match (pass.mesh, v.is_empty()) {
                    (Some(m), false) => PickSet::Edges(m, v),
                    _ => PickSet::Nothing,
                }
            }
            PickMode::Face => {
                let v: Vec<_> = hits.filter_map(|h| if let PickResult::Face(_, x) = h { Some(x) } else { None }).collect();
                match (pass.mesh, v.is_empty()) {
                    (Some(m), false) => PickSet::Faces(m, v),
                    _ => PickSet::Nothing,
                }
            }
        }
    }
}
