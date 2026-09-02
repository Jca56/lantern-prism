//! Per-mesh GPU buffers, keyed by mesh id and kept fresh by the kernel's
//! geometry and selection versions. Geometry (positions, normals, indices)
//! re-uploads only when the mesh changes; selection changes touch just the
//! packed one-byte-per-element flag buffers.

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::{Id, bytes};
use prism_doc::{Elem, MeshBlock};
use prism_eval::{MeshBuffers, evaluate};
use prism_mesh::tables::{E_HIDE, E_SELECT, F_HIDE, F_SELECT, V_HIDE, V_SELECT};
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

#[derive(Default)]
pub struct MeshCache {
    meshes: HashMap<Id, GpuMesh>,
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

fn element_flags(block: &MeshBlock, b: &MeshBuffers) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let m = &block.mesh;
    let (vsel, vhide) = (m.vert_attrs().bools(V_SELECT), m.vert_attrs().bools(V_HIDE));
    let (esel, ehide) = (m.edge_attrs().bools(E_SELECT), m.edge_attrs().bools(E_HIDE));
    let (fsel, fhide) = (m.face_attrs().bools(F_SELECT), m.face_attrs().bools(F_HIDE));
    let active = block.edit.active;
    let flag = |sel: bool, hide: bool, act: bool| -> u8 {
        (if sel { FLAG_SELECTED } else { 0 }) | (if act { FLAG_ACTIVE } else { 0 }) | (if hide { FLAG_HIDDEN } else { 0 })
    };
    let faces: Vec<u8> = b.face_handles.iter().map(|&f| flag(fsel[f.idx()], fhide[f.idx()], active == Some(Elem::Face(f)))).collect();
    let verts: Vec<u8> = b.vert_to_vert.iter().map(|&v| flag(vsel[v.idx()], vhide[v.idx()], active == Some(Elem::Vert(v)))).collect();
    let edges: Vec<u8> = b.edge_to_edge.iter().map(|&e| flag(esel[e.idx()], ehide[e.idx()], active == Some(Elem::Edge(e)))).collect();
    (pack(&faces), pack(&verts), pack(&edges))
}

impl MeshCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: Id) -> Option<&GpuMesh> {
        self.meshes.get(&id)
    }

    /// Bring `id`'s buffers up to date with `block`.
    pub fn sync(&mut self, gpu: &Gpu, flags_layout: &wgpu::BindGroupLayout, id: Id, block: &MeshBlock) -> &GpuMesh {
        let geo = block.mesh.geometry_version();
        let sel = block.mesh.selection_version();
        let stale_geo = self.meshes.get(&id).is_none_or(|g| g.geometry_version != geo);
        if stale_geo {
            let b = Arc::new(evaluate(&block.mesh));
            let edge_pos: Vec<[f32; 3]> = b.edge_indices.iter().map(|&i| b.vert_positions[i as usize].to_gpu()).collect();
            let (ff, vf, ef) = element_flags(block, &b);
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
            let g = GpuMesh {
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
            };
            self.meshes.insert(id, g);
        } else if let Some(g) = self.meshes.get_mut(&id)
            && g.selection_version != sel
        {
            let (ff, vf, ef) = element_flags(block, &g.buffers);
            gpu.queue.write_buffer(&g.face_flags, 0, bytes::slice_as_bytes(&ff));
            gpu.queue.write_buffer(&g.vert_flags, 0, bytes::slice_as_bytes(&vf));
            gpu.queue.write_buffer(&g.edge_flags, 0, bytes::slice_as_bytes(&ef));
            g.selection_version = sel;
        }
        self.meshes.get(&id).expect("just synced")
    }

    /// Drop buffers for meshes that no longer exist.
    pub fn retain(&mut self, live: impl Fn(Id) -> bool) {
        self.meshes.retain(|id, _| live(*id));
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
