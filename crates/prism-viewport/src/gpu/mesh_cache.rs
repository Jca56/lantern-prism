//! Per-mesh GPU buffers, keyed by mesh id and kept fresh by the kernel's
//! geometry and selection versions. Geometry (positions, normals, indices)
//! re-uploads only when the mesh changes; selection changes touch just the
//! packed one-byte-per-element flag buffers.
//!
//! A mesh with modifiers has two entries (D029): the **cage** (the base mesh,
//! edited and picked) and the **surface** (the modifier result, what solid
//! shading shows). Face flags on the surface come from the base face each
//! result face descends from, so selection tints the smooth surface.

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::{Id, bytes};
use prism_doc::{Elem, MeshBlock};
use prism_eval::{EvalMesh, MeshBuffers, apply_modifiers, evaluate};
use prism_mesh::tables::{E_HIDE, E_SELECT, F_HIDE, F_SELECT, V_HIDE, V_SELECT};
use prism_mesh::FaceH;
use prism_render::Gpu;

use super::uniforms::{FLAG_ACTIVE, FLAG_HIDDEN, FLAG_SELECTED};

pub struct GpuMesh {
    pub geometry_version: u64,
    pub selection_version: u64,
    pub buffers: Arc<MeshBuffers>,
    pub corner_pos: wgpu::Buffer,
    pub corner_normal: wgpu::Buffer,
    pub corner_face: wgpu::Buffer,
    pub tri_index: wgpu::Buffer,
    pub tri_index_count: u32,
    /// Two positions per edge.
    pub edge_pos: wgpu::Buffer,
    pub edge_vertex_count: u32,
    pub vert_pos: wgpu::Buffer,
    pub vert_count: u32,
    face_flags: wgpu::Buffer,
    vert_flags: wgpu::Buffer,
    edge_flags: wgpu::Buffer,
    pub flags_bind_group: wgpu::BindGroup,
}

/// The modifier result for one mesh and what it was built from.
struct Surface {
    modifiers_version: u64,
    face_origin: Vec<Option<FaceH>>,
    gpu: GpuMesh,
}

#[derive(Default)]
pub struct MeshCache {
    meshes: HashMap<Id, GpuMesh>,
    surfaces: HashMap<Id, Surface>,
}

fn buffer(gpu: &Gpu, label: &str, usage: wgpu::BufferUsages, data: &[u8]) -> wgpu::Buffer {
    let size = (data.len().max(4) as u64).div_ceil(4) * 4;
    let b = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    if !data.is_empty() {
        gpu.queue.write_buffer(&b, 0, data);
    }
    b
}

fn f32x3(v: &[prism_math::Vec3]) -> Vec<[f32; 3]> {
    v.iter().map(|p| p.to_gpu()).collect()
}

/// Pack one byte per element into words.
fn pack(flags: &[u8]) -> Vec<u32> {
    let mut out = vec![0u32; flags.len().div_ceil(4).max(1)];
    for (i, &f) in flags.iter().enumerate() {
        out[i / 4] |= (f as u32) << ((i % 4) * 8);
    }
    out
}

fn flag(sel: bool, hide: bool, act: bool) -> u8 {
    (if sel { FLAG_SELECTED } else { 0 }) | (if act { FLAG_ACTIVE } else { 0 }) | (if hide { FLAG_HIDDEN } else { 0 })
}

/// Flags for the base mesh's own elements.
fn element_flags(block: &MeshBlock, b: &MeshBuffers) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let m = &block.mesh;
    let (vsel, vhide) = (m.vert_attrs().bools(V_SELECT), m.vert_attrs().bools(V_HIDE));
    let (esel, ehide) = (m.edge_attrs().bools(E_SELECT), m.edge_attrs().bools(E_HIDE));
    let (fsel, fhide) = (m.face_attrs().bools(F_SELECT), m.face_attrs().bools(F_HIDE));
    let active = block.edit.active;
    let faces: Vec<u8> = b.face_handles.iter().map(|&f| flag(fsel[f.idx()], fhide[f.idx()], active == Some(Elem::Face(f)))).collect();
    let verts: Vec<u8> = b.vert_to_vert.iter().map(|&v| flag(vsel[v.idx()], vhide[v.idx()], active == Some(Elem::Vert(v)))).collect();
    let edges: Vec<u8> = b.edge_to_edge.iter().map(|&e| flag(esel[e.idx()], ehide[e.idx()], active == Some(Elem::Edge(e)))).collect();
    (pack(&faces), pack(&verts), pack(&edges))
}

/// Flags for a modifier result: faces borrow their base face's state, edges
/// and vertices carry none (the cage shows those).
fn surface_flags(block: &MeshBlock, b: &MeshBuffers, origin: &[Option<FaceH>]) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let m = &block.mesh;
    let (fsel, fhide) = (m.face_attrs().bools(F_SELECT), m.face_attrs().bools(F_HIDE));
    let active = block.edit.active;
    let faces: Vec<u8> = b
        .face_handles
        .iter()
        .map(|&f| match origin.get(f.idx()).copied().flatten() {
            Some(o) if o.idx() < fsel.len() => flag(fsel[o.idx()], fhide[o.idx()], active == Some(Elem::Face(o))),
            _ => 0,
        })
        .collect();
    (pack(&faces), pack(&vec![0u8; b.vert_to_vert.len()]), pack(&vec![0u8; b.edge_to_edge.len()]))
}

fn upload(gpu: &Gpu, flags_layout: &wgpu::BindGroupLayout, b: Arc<MeshBuffers>, (ff, vf, ef): (Vec<u32>, Vec<u32>, Vec<u32>), geo: u64, sel: u64) -> GpuMesh {
    let edge_pos: Vec<[f32; 3]> = b.edge_indices.iter().map(|&i| b.vert_positions[i as usize].to_gpu()).collect();
    let vtx = wgpu::BufferUsages::VERTEX;
    let stg = wgpu::BufferUsages::STORAGE;
    let face_flags = buffer(gpu, "face flags", stg, bytes::slice_as_bytes(&ff));
    let vert_flags = buffer(gpu, "vert flags", stg, bytes::slice_as_bytes(&vf));
    let edge_flags = buffer(gpu, "edge flags", stg, bytes::slice_as_bytes(&ef));
    let flags_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("prism element flags"),
        layout: flags_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: face_flags.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: vert_flags.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: edge_flags.as_entire_binding() },
        ],
    });
    GpuMesh {
        geometry_version: geo,
        selection_version: sel,
        corner_pos: buffer(gpu, "corner positions", vtx, bytes::slice_as_bytes(&f32x3(&b.corner_positions))),
        corner_normal: buffer(gpu, "corner normals", vtx, bytes::slice_as_bytes(&f32x3(&b.corner_normals))),
        corner_face: buffer(gpu, "corner faces", vtx, bytes::slice_as_bytes(&b.corner_face)),
        tri_index: buffer(gpu, "triangle indices", wgpu::BufferUsages::INDEX, bytes::slice_as_bytes(&b.tri_indices)),
        tri_index_count: b.tri_indices.len() as u32,
        edge_pos: buffer(gpu, "edge positions", vtx, bytes::slice_as_bytes(&edge_pos)),
        edge_vertex_count: edge_pos.len() as u32,
        vert_pos: buffer(gpu, "vertex positions", vtx, bytes::slice_as_bytes(&f32x3(&b.vert_positions))),
        vert_count: b.vert_positions.len() as u32,
        face_flags,
        vert_flags,
        edge_flags,
        flags_bind_group,
        buffers: b,
    }
}

fn write_flags(gpu: &Gpu, g: &mut GpuMesh, (ff, vf, ef): (Vec<u32>, Vec<u32>, Vec<u32>), sel: u64) {
    gpu.queue.write_buffer(&g.face_flags, 0, bytes::slice_as_bytes(&ff));
    gpu.queue.write_buffer(&g.vert_flags, 0, bytes::slice_as_bytes(&vf));
    gpu.queue.write_buffer(&g.edge_flags, 0, bytes::slice_as_bytes(&ef));
    g.selection_version = sel;
}

impl MeshCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The base mesh: what edit mode shows as a cage and picks against.
    pub fn get(&self, id: Id) -> Option<&GpuMesh> {
        self.meshes.get(&id)
    }

    /// What solid shading shows: the modifier result when there is one.
    pub fn surface(&self, id: Id) -> Option<&GpuMesh> {
        self.surfaces.get(&id).map(|s| &s.gpu).or_else(|| self.meshes.get(&id))
    }

    /// Bring `id`'s buffers up to date with `block`.
    pub fn sync(&mut self, gpu: &Gpu, flags_layout: &wgpu::BindGroupLayout, id: Id, block: &MeshBlock) -> &GpuMesh {
        let geo = block.mesh.geometry_version();
        let sel = block.mesh.selection_version();
        let stale_geo = self.meshes.get(&id).is_none_or(|g| g.geometry_version != geo);
        if stale_geo {
            let b = Arc::new(evaluate(&block.mesh));
            let flags = element_flags(block, &b);
            self.meshes.insert(id, upload(gpu, flags_layout, b, flags, geo, sel));
        } else if let Some(g) = self.meshes.get_mut(&id)
            && g.selection_version != sel
        {
            let flags = element_flags(block, &g.buffers);
            write_flags(gpu, g, flags, sel);
        }
        self.sync_surface(gpu, flags_layout, id, block, geo, sel);
        self.meshes.get(&id).expect("just synced")
    }

    fn sync_surface(&mut self, gpu: &Gpu, flags_layout: &wgpu::BindGroupLayout, id: Id, block: &MeshBlock, geo: u64, sel: u64) {
        if block.modifiers.is_empty() {
            self.surfaces.remove(&id);
            return;
        }
        let mods = block.modifiers_version;
        let stale = self.surfaces.get(&id).is_none_or(|s| s.gpu.geometry_version != geo || s.modifiers_version != mods);
        if stale {
            let EvalMesh { mesh, face_origin } = apply_modifiers(&block.mesh, &block.modifiers);
            let b = Arc::new(evaluate(&mesh));
            let flags = surface_flags(block, &b, &face_origin);
            let gpu_mesh = upload(gpu, flags_layout, b, flags, geo, sel);
            self.surfaces.insert(id, Surface { modifiers_version: mods, face_origin, gpu: gpu_mesh });
        } else if let Some(s) = self.surfaces.get_mut(&id)
            && s.gpu.selection_version != sel
        {
            let flags = surface_flags(block, &s.gpu.buffers, &s.face_origin);
            write_flags(gpu, &mut s.gpu, flags, sel);
        }
    }

    /// Drop buffers for meshes that no longer exist.
    pub fn retain(&mut self, live: impl Fn(Id) -> bool) {
        self.meshes.retain(|id, _| live(*id));
        self.surfaces.retain(|id, _| live(*id));
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing() {
        assert_eq!(pack(&[]), vec![0]);
        assert_eq!(pack(&[1, 2, 4, 7, 5]), vec![0x0704_0201, 0x05]);
    }
}
