//! Mesh ↔ bytes. Writes live elements compactly (attribute layers by name,
//! edges as vertex pairs, faces as vertex rings) and rebuilds through the
//! euler operators on load, which also compacts the mesh. Output is
//! deterministic, so save → load → save is byte-identical.

use core::fmt;

use prism_core::Id;
use prism_math::{Color, Vec2, Vec3, Vec4};
use prism_mesh::{AttrData, AttrFlags, AttrKind, AttributeSet, EdgeH, FaceH, LoopH, Mesh, VertH, names};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshIoError {
    Eof,
    BadKind(u8),
    BadUtf8,
    BadTopology(String),
}

impl fmt::Display for MeshIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshIoError::Eof => write!(f, "mesh data truncated"),
            MeshIoError::BadKind(k) => write!(f, "unknown attribute kind {k}"),
            MeshIoError::BadUtf8 => write!(f, "attribute name is not UTF-8"),
            MeshIoError::BadTopology(s) => write!(f, "mesh topology rejected: {s}"),
        }
    }
}

impl std::error::Error for MeshIoError {}

struct W<'a>(&'a mut Vec<u8>);

impl W<'_> {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct R<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], MeshIoError> {
        if self.pos + n > self.data.len() {
            return Err(MeshIoError::Eof);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, MeshIoError> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, MeshIoError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, MeshIoError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String, MeshIoError> {
        let n = self.u32()? as usize;
        core::str::from_utf8(self.take(n)?).map(str::to_owned).map_err(|_| MeshIoError::BadUtf8)
    }
}

fn kind_tag(k: AttrKind) -> u8 {
    match k {
        AttrKind::Bool => 0,
        AttrKind::F64 => 1,
        AttrKind::I32 => 2,
        AttrKind::U32 => 3,
        AttrKind::Vec2 => 4,
        AttrKind::Vec3 => 5,
        AttrKind::Vec4 => 6,
        AttrKind::Color => 7,
    }
}

fn tag_kind(t: u8) -> Result<AttrKind, MeshIoError> {
    Ok(match t {
        0 => AttrKind::Bool,
        1 => AttrKind::F64,
        2 => AttrKind::I32,
        3 => AttrKind::U32,
        4 => AttrKind::Vec2,
        5 => AttrKind::Vec3,
        6 => AttrKind::Vec4,
        7 => AttrKind::Color,
        other => return Err(MeshIoError::BadKind(other)),
    })
}

/// Write every non-temporary layer of `attrs` for the rows in `rows`.
fn write_layers(w: &mut W, attrs: &AttributeSet, rows: &[usize]) {
    let layers: Vec<_> = attrs.layers().iter().filter(|l| !l.flags.contains(AttrFlags::TEMPORARY)).collect();
    w.u32(layers.len() as u32);
    for l in layers {
        w.str(&l.name);
        w.u8(kind_tag(l.data.kind()));
        w.u32(l.flags.0);
        for &i in rows {
            match &l.data {
                AttrData::Bool(v) => w.u8(v[i] as u8),
                AttrData::F64(v) => w.f64(v[i]),
                AttrData::I32(v) => w.u32(v[i] as u32),
                AttrData::U32(v) => w.u32(v[i]),
                AttrData::Vec2(v) => {
                    w.f64(v[i].x);
                    w.f64(v[i].y);
                }
                AttrData::Vec3(v) => {
                    w.f64(v[i].x);
                    w.f64(v[i].y);
                    w.f64(v[i].z);
                }
                AttrData::Vec4(v) => {
                    w.f64(v[i].x);
                    w.f64(v[i].y);
                    w.f64(v[i].z);
                    w.f64(v[i].w);
                }
                AttrData::Color(v) => {
                    w.f64(v[i].r);
                    w.f64(v[i].g);
                    w.f64(v[i].b);
                    w.f64(v[i].a);
                }
            }
        }
    }
}

/// Read layers written by `write_layers` into `attrs` at `rows` (adding
/// missing layers; `position` already exists).
fn read_layers(r: &mut R, attrs: &mut AttributeSet, rows: &[usize]) -> Result<(), MeshIoError> {
    let n = r.u32()?;
    for _ in 0..n {
        let name = r.str()?;
        let kind = tag_kind(r.u8()?)?;
        let flags = AttrFlags(r.u32()?);
        let idx = match attrs.index(&name) {
            Some(i) if attrs.layer(i).data.kind() == kind => Some(i),
            Some(_) => None, // type changed: read and drop
            None => attrs.add(&name, kind, flags).ok(),
        };
        for &row in rows {
            let value = match kind {
                AttrKind::Bool => prism_mesh::AttrValue::Bool(r.u8()? != 0),
                AttrKind::F64 => prism_mesh::AttrValue::F64(r.f64()?),
                AttrKind::I32 => prism_mesh::AttrValue::I32(r.u32()? as i32),
                AttrKind::U32 => prism_mesh::AttrValue::U32(r.u32()?),
                AttrKind::Vec2 => prism_mesh::AttrValue::Vec2(Vec2::new(r.f64()?, r.f64()?)),
                AttrKind::Vec3 => prism_mesh::AttrValue::Vec3(Vec3::new(r.f64()?, r.f64()?, r.f64()?)),
                AttrKind::Vec4 => prism_mesh::AttrValue::Vec4(Vec4::new(r.f64()?, r.f64()?, r.f64()?, r.f64()?)),
                AttrKind::Color => prism_mesh::AttrValue::Color(Color::rgba(r.f64()?, r.f64()?, r.f64()?, r.f64()?)),
            };
            if let Some(i) = idx {
                attrs.layer_mut(i).data.set(row, value);
            }
        }
    }
    Ok(())
}

/// Serialize `mesh`.
pub fn write(mesh: &Mesh, out: &mut Vec<u8>) {
    let mut w = W(out);
    let verts: Vec<VertH> = mesh.verts().collect();
    let edges: Vec<EdgeH> = mesh.edges().collect();
    let faces: Vec<FaceH> = mesh.faces().collect();
    let mut vert_index = vec![u32::MAX; mesh.positions().len()];
    for (i, v) in verts.iter().enumerate() {
        vert_index[v.idx()] = i as u32;
    }
    // Vertices: positions are a layer like any other.
    w.u32(verts.len() as u32);
    write_layers(&mut w, mesh.vert_attrs(), &verts.iter().map(|v| v.idx()).collect::<Vec<_>>());
    // Edges.
    w.u32(edges.len() as u32);
    for &e in &edges {
        let [a, b] = mesh.edge_verts(e);
        w.u32(vert_index[a.idx()]);
        w.u32(vert_index[b.idx()]);
    }
    write_layers(&mut w, mesh.edge_attrs(), &edges.iter().map(|e| e.idx()).collect::<Vec<_>>());
    // Faces as vertex rings; loops follow in corner order.
    w.u32(faces.len() as u32);
    let mut loops: Vec<LoopH> = Vec::with_capacity(mesh.loop_count());
    for &f in &faces {
        w.u32(mesh.face_len(f) as u32);
        for l in mesh.loops_of_face(f) {
            w.u32(vert_index[mesh.loop_vert(l).idx()]);
            loops.push(l);
        }
    }
    write_layers(&mut w, mesh.face_attrs(), &faces.iter().map(|f| f.idx()).collect::<Vec<_>>());
    write_layers(&mut w, mesh.loop_attrs(), &loops.iter().map(|l| l.idx()).collect::<Vec<_>>());
}

/// Rebuild a mesh written by [`write`].
pub fn read(data: &[u8]) -> Result<Mesh, MeshIoError> {
    let mut r = R { data, pos: 0 };
    let mut mesh = Mesh::new();
    let nv = r.u32()? as usize;
    let verts: Vec<VertH> = (0..nv).map(|_| mesh.make_vert(Vec3::ZERO)).collect();
    let rows: Vec<usize> = verts.iter().map(|v| v.idx()).collect();
    read_layers(&mut r, mesh.vert_attrs_mut(), &rows)?;
    let vert = |i: u32| verts.get(i as usize).copied().ok_or_else(|| MeshIoError::BadTopology(format!("vertex {i} out of range")));

    let ne = r.u32()? as usize;
    let mut edges = Vec::with_capacity(ne);
    for _ in 0..ne {
        let (a, b) = (vert(r.u32()?)?, vert(r.u32()?)?);
        let e = mesh.make_edge(a, b).map_err(|e| MeshIoError::BadTopology(e.to_string()))?;
        edges.push(e);
    }
    let rows: Vec<usize> = edges.iter().map(|e| e.idx()).collect();
    read_layers(&mut r, mesh.edge_attrs_mut(), &rows)?;

    let nf = r.u32()? as usize;
    let mut faces = Vec::with_capacity(nf);
    let mut ring = Vec::new();
    for _ in 0..nf {
        let len = r.u32()? as usize;
        ring.clear();
        for _ in 0..len {
            ring.push(vert(r.u32()?)?);
        }
        let f = mesh.make_face(&ring).map_err(|e| MeshIoError::BadTopology(e.to_string()))?;
        faces.push(f);
    }
    let rows: Vec<usize> = faces.iter().map(|f| f.idx()).collect();
    read_layers(&mut r, mesh.face_attrs_mut(), &rows)?;
    let loop_rows: Vec<usize> = faces.iter().flat_map(|&f| mesh.loops_of_face(f).map(|l| l.idx()).collect::<Vec<_>>()).collect();
    read_layers(&mut r, mesh.loop_attrs_mut(), &loop_rows)?;
    Ok(mesh)
}

/// Keep the unused-import lint honest about what the reader needs.
#[allow(dead_code)]
fn _uses(_: Id, _: &str) -> &str {
    names::UV
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_mesh::primitives;
    use prism_mesh::tables::{E_SHARP, F_SMOOTH};

    #[test]
    fn roundtrip_is_byte_stable() {
        let mut m = primitives::uv_sphere(1.0, 8, 5);
        m.face_attrs_mut().bools_mut(F_SMOOTH).set(3, true);
        m.edge_attrs_mut().bools_mut(E_SHARP).set(7, true);
        let uv = m.loop_attrs_mut().add(names::UV, AttrKind::Vec2, AttrFlags::INTERPOLATE).unwrap();
        m.loop_attrs_mut().vec2s_mut(uv).set(5, Vec2::new(0.25, 0.75));
        // Make the mesh non-compact so the writer has to skip dead slots.
        let f = m.faces().nth(2).unwrap();
        m.kill_face(f).unwrap();
        let mut bytes = Vec::new();
        write(&m, &mut bytes);
        let back = read(&bytes).unwrap();
        back.validate().unwrap();
        assert_eq!((back.vert_count(), back.edge_count(), back.face_count(), back.loop_count()),
                   (m.vert_count(), m.edge_count(), m.face_count(), m.loop_count()));
        let mut again = Vec::new();
        write(&back, &mut again);
        assert_eq!(bytes, again, "second save is byte-identical");
        assert!(back.face_attrs().bools(F_SMOOTH).iter().any(|&b| b));
        assert!(back.edge_attrs().bools(E_SHARP).iter().any(|&b| b));
        let uv2 = back.loop_attrs().index(names::UV).unwrap();
        assert!(back.loop_attrs().vec2s(uv2).iter().any(|v| *v == Vec2::new(0.25, 0.75)));
        for (a, b) in m.verts().zip(back.verts()) {
            assert!(m.position(a).approx_eq(back.position(b), 0.0));
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(read(&[1, 0, 0]), Err(MeshIoError::Eof)));
        let mut bytes = Vec::new();
        write(&primitives::plane(1.0), &mut bytes);
        bytes.truncate(bytes.len() - 5);
        assert!(read(&bytes).is_err());
    }
}
