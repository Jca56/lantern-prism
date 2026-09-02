//! glTF import: walk the scene's nodes, bake each node's world transform onto
//! a new object, rebuild its mesh from the triangle primitives, weld the
//! seams glTF splits, and carry the base colour over as a material.

use std::path::{Path, PathBuf};

use prism_math::{Color, Mat3, Mat4, Quat, Vec3, Vec4};
use prism_mesh::{Mesh, VertH};

use super::{GltfError, base64_decode, unpack_glb};
use crate::json::Json;

/// One object read from a glTF: an editable mesh in its own space and the
/// world transform its node had.
pub struct GltfObject {
    pub name: String,
    pub mesh: Mesh,
    pub location: Vec3,
    /// XYZ Euler, radians.
    pub rotation: Vec3,
    pub scale: Vec3,
    /// The material's base colour, sRGB.
    pub color: Option<Color>,
    /// Triangles the kernel refused (degenerate).
    pub skipped: usize,
}

/// Read a `.glb` or `.gltf` (with embedded or sibling buffers).
pub fn read_file(path: &Path) -> Result<Vec<GltfObject>, GltfError> {
    let bytes = std::fs::read(path)?;
    let (json, bin) = if bytes.starts_with(b"glTF") { unpack_glb(&bytes)? } else { (String::from_utf8_lossy(&bytes).into_owned(), None) };
    parse(&json, bin, path.parent())
}

/// Parse glTF JSON. `glb_bin` is the GLB's binary chunk; `base_dir` resolves
/// buffer files a `.gltf` points at.
pub fn parse(json: &str, glb_bin: Option<Vec<u8>>, base_dir: Option<&Path>) -> Result<Vec<GltfObject>, GltfError> {
    let doc = Json::parse(json)?;
    let gltf = Gltf::load(&doc, glb_bin, base_dir)?;
    let nodes = doc.get("nodes").and_then(Json::as_arr).unwrap_or(&[]);
    // Roots: the default scene's nodes, else every node no one parents.
    let roots: Vec<usize> = match doc.get("scenes").and_then(Json::as_arr) {
        Some(scenes) if !scenes.is_empty() => {
            let s = doc.get("scene").and_then(Json::as_usize).unwrap_or(0).min(scenes.len() - 1);
            scenes[s].get("nodes").and_then(Json::as_arr).map(|a| a.iter().filter_map(Json::as_usize).collect()).unwrap_or_default()
        }
        _ => {
            let mut is_child = vec![false; nodes.len()];
            for n in nodes {
                for c in n.get("children").and_then(Json::as_arr).unwrap_or(&[]) {
                    if let Some(i) = c.as_usize()
                        && i < nodes.len()
                    {
                        is_child[i] = true;
                    }
                }
            }
            (0..nodes.len()).filter(|&i| !is_child[i]).collect()
        }
    };
    let mut out = Vec::new();
    for r in roots {
        gltf.walk(r, Mat4::IDENTITY, 0, &mut out)?;
    }
    if out.is_empty() {
        return Err(GltfError::Empty);
    }
    Ok(out)
}

struct Gltf<'a> {
    doc: &'a Json,
    buffers: Vec<Vec<u8>>,
}

fn nums(j: Option<&Json>, n: usize) -> Option<Vec<f64>> {
    let a = j?.as_arr()?;
    (a.len() >= n).then(|| a.iter().take(n).map(|v| v.as_f64().unwrap_or(0.0)).collect())
}

fn node_local(node: &Json) -> Mat4 {
    if let Some(m) = nums(node.get("matrix"), 16) {
        let col = |i: usize| Vec4::new(m[i], m[i + 1], m[i + 2], m[i + 3]);
        return Mat4::from_cols(col(0), col(4), col(8), col(12));
    }
    let t = nums(node.get("translation"), 3).map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
    let r = nums(node.get("rotation"), 4).map_or(Quat::IDENTITY, |v| Quat::new(v[0], v[1], v[2], v[3]).normalize());
    let s = nums(node.get("scale"), 3).map_or(Vec3::ONE, |v| Vec3::new(v[0], v[1], v[2]));
    Mat4::from_translation_rotation_scale(t, r, s)
}

/// Translation, XYZ Euler rotation and scale of an affine matrix. Shear is
/// dropped; a mirrored matrix comes back with a negative X scale.
fn decompose(m: Mat4) -> (Vec3, Vec3, Vec3) {
    let t = m.translation();
    let m3 = m.to_mat3();
    let (c0, c1, c2) = (m3.col(0), m3.col(1), m3.col(2));
    let mut s = Vec3::new(c0.length(), c1.length(), c2.length());
    if m3.determinant() < 0.0 {
        s.x = -s.x;
    }
    let unit = |c: Vec3, len: f64| if len.abs() < 1e-12 { c } else { c / len };
    let rot = Mat3::from_cols(unit(c0, s.x), unit(c1, s.y), unit(c2, s.z));
    let (x, y, z) = Quat::from_mat3(&rot).to_euler_xyz();
    (t, Vec3::new(x, y, z), s)
}

fn read_component(ctype: usize, b: &[u8], normalized: bool) -> f64 {
    match ctype {
        5120 => {
            let v = b[0] as i8 as f64;
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = b[0] as f64;
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let v = i16::from_le_bytes([b[0], b[1]]) as f64;
            if normalized { (v / 32767.0).max(-1.0) } else { v }
        }
        5123 => {
            let v = u16::from_le_bytes([b[0], b[1]]) as f64;
            if normalized { v / 65535.0 } else { v }
        }
        5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
        _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
    }
}

impl<'a> Gltf<'a> {
    fn load(doc: &'a Json, glb_bin: Option<Vec<u8>>, base_dir: Option<&Path>) -> Result<Self, GltfError> {
        let mut buffers = Vec::new();
        let mut glb_bin = glb_bin;
        for (i, b) in doc.get("buffers").and_then(Json::as_arr).unwrap_or(&[]).iter().enumerate() {
            let data = match b.get("uri").and_then(Json::as_str) {
                Some(uri) if uri.starts_with("data:") => {
                    let (_, b64) = uri.split_once(',').ok_or_else(|| GltfError::Missing("data URI has no payload".into()))?;
                    base64_decode(b64).ok_or_else(|| GltfError::Glb("bad base64 in data URI".into()))?
                }
                Some(uri) => std::fs::read(base_dir.map(|d| d.join(uri)).unwrap_or_else(|| PathBuf::from(uri)))?,
                None if i == 0 => glb_bin.take().ok_or_else(|| GltfError::Missing("buffer 0 has no data".into()))?,
                None => return Err(GltfError::Missing(format!("buffer {i} has no uri"))),
            };
            buffers.push(data);
        }
        Ok(Self { doc, buffers })
    }

    fn item(&self, key: &str, i: usize) -> Result<&'a Json, GltfError> {
        self.doc.get(key).and_then(|a| a.at(i)).ok_or_else(|| GltfError::Missing(format!("{key}[{i}]")))
    }

    /// Every element of an accessor as f64 components, and how many
    /// components make one element.
    fn accessor(&self, index: usize) -> Result<(Vec<f64>, usize), GltfError> {
        let acc = self.item("accessors", index)?;
        if acc.get("sparse").is_some() {
            return Err(GltfError::Unsupported("sparse accessor".into()));
        }
        let count = acc.get("count").and_then(Json::as_usize).ok_or_else(|| GltfError::Missing("accessor count".into()))?;
        let ctype = acc.get("componentType").and_then(Json::as_usize).ok_or_else(|| GltfError::Missing("accessor componentType".into()))?;
        let comps = match acc.get("type").and_then(Json::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            other => return Err(GltfError::Unsupported(format!("accessor type {other:?}"))),
        };
        let csize = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(GltfError::Unsupported(format!("component type {ctype}"))),
        };
        let normalized = acc.get("normalized").and_then(Json::as_bool).unwrap_or(false);
        let Some(view_i) = acc.get("bufferView").and_then(Json::as_usize) else {
            return Ok((vec![0.0; count * comps], comps)); // no view: all zeros, per spec
        };
        let view = self.item("bufferViews", view_i)?;
        let buffer = self.buffers.get(view.get("buffer").and_then(Json::as_usize).unwrap_or(0)).ok_or_else(|| GltfError::Missing("buffer".into()))?;
        let base = view.get("byteOffset").and_then(Json::as_usize).unwrap_or(0) + acc.get("byteOffset").and_then(Json::as_usize).unwrap_or(0);
        let elem = comps * csize;
        let stride = view.get("byteStride").and_then(Json::as_usize).filter(|&s| s > 0).unwrap_or(elem);
        let mut out = Vec::with_capacity(count * comps);
        for e in 0..count {
            let start = base + e * stride;
            for c in 0..comps {
                let at = start + c * csize;
                let bytes = buffer.get(at..at + csize).ok_or_else(|| GltfError::Missing("accessor data runs past its buffer".into()))?;
                out.push(read_component(ctype, bytes, normalized));
            }
        }
        Ok((out, comps))
    }

    fn walk(&self, index: usize, parent: Mat4, depth: usize, out: &mut Vec<GltfObject>) -> Result<(), GltfError> {
        if depth > 64 {
            return Ok(()); // a cycle; stop rather than spin
        }
        let node = self.item("nodes", index)?;
        let world = parent * node_local(node);
        if let Some(mi) = node.get("mesh").and_then(Json::as_usize)
            && let Some(o) = self.object(node, mi, world, out.len())?
        {
            out.push(o);
        }
        for c in node.get("children").and_then(Json::as_arr).unwrap_or(&[]) {
            if let Some(ci) = c.as_usize() {
                self.walk(ci, world, depth + 1, out)?;
            }
        }
        Ok(())
    }

    fn object(&self, node: &Json, mesh_i: usize, world: Mat4, n: usize) -> Result<Option<GltfObject>, GltfError> {
        let gmesh = self.item("meshes", mesh_i)?;
        let mut mesh = Mesh::new();
        let mut skipped = 0;
        let mut color = None;
        let mut extent = 0.0f64;
        for prim in gmesh.get("primitives").and_then(Json::as_arr).unwrap_or(&[]) {
            let mode = prim.get("mode").and_then(Json::as_usize).unwrap_or(4);
            if !matches!(mode, 4..=6) {
                continue; // points and lines are not mesh surfaces
            }
            let Some(pos_i) = prim.get("attributes").and_then(|a| a.get("POSITION")).and_then(Json::as_usize) else {
                continue;
            };
            let (pos, comps) = self.accessor(pos_i)?;
            if comps != 3 {
                return Err(GltfError::Unsupported("POSITION is not VEC3".into()));
            }
            let verts: Vec<VertH> = pos
                .chunks(3)
                .map(|p| {
                    extent = extent.max(p[0].abs()).max(p[1].abs()).max(p[2].abs());
                    mesh.make_vert(Vec3::new(p[0], p[1], p[2]))
                })
                .collect();
            let indices: Vec<usize> = match prim.get("indices").and_then(Json::as_usize) {
                Some(i) => self.accessor(i)?.0.into_iter().map(|v| v as usize).collect(),
                None => (0..verts.len()).collect(),
            };
            let tri = |a: usize, b: usize, c: usize| -> Option<[VertH; 3]> { Some([*verts.get(a)?, *verts.get(b)?, *verts.get(c)?]) };
            let tris: Vec<[VertH; 3]> = match mode {
                4 => indices.chunks_exact(3).filter_map(|t| tri(t[0], t[1], t[2])).collect(),
                5 => (2..indices.len()).filter_map(|i| if i % 2 == 0 { tri(indices[i - 2], indices[i - 1], indices[i]) } else { tri(indices[i - 1], indices[i - 2], indices[i]) }).collect(),
                _ => (2..indices.len()).filter_map(|i| tri(indices[0], indices[i - 1], indices[i])).collect(),
            };
            for t in tris {
                if crate::add_face_lenient(&mut mesh, &t).is_none() {
                    skipped += 1;
                }
            }
            if color.is_none()
                && let Some(mi) = prim.get("material").and_then(Json::as_usize)
                && let Ok(mat) = self.item("materials", mi)
                && let Some(f) = mat.get("pbrMetallicRoughness").and_then(|p| p.get("baseColorFactor")).and_then(Json::as_arr)
                && f.len() >= 3
            {
                let g = |i: usize| f.get(i).and_then(Json::as_f64).unwrap_or(1.0);
                color = Some(Color::rgba(g(0), g(1), g(2), g(3)).to_srgb());
            }
        }
        if mesh.face_count() == 0 {
            return Ok(None);
        }
        // glTF splits vertices along normal and UV seams; weld them back so
        // the mesh edits as one surface.
        mesh.merge_by_distance(1e-6 * extent.max(1.0));
        let (location, rotation, scale) = decompose(world);
        let name = node
            .get("name")
            .and_then(Json::as_str)
            .or_else(|| gmesh.get("name").and_then(Json::as_str))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Imported {}", n + 1));
        Ok(Some(GltfObject { name, mesh, location, rotation, scale, color, skipped }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::base64_encode;

    /// A hand-made glTF: a parent node offset by (1,0,0) holding a child at
    /// (0,2,0) whose mesh is a two-triangle strip, positions as u8 bytes.
    #[test]
    fn hierarchy_strips_and_data_uris() {
        let mut bin = Vec::new();
        for p in [[0u8, 0, 0], [2, 0, 0], [0, 2, 0], [2, 2, 0]] {
            bin.extend_from_slice(&p);
            bin.push(0); // pad each VEC3 of bytes to 4 for alignment
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"translation":[1,0,0],"children":[1]}},{{"name":"Strip","mesh":0,"translation":[0,2,0],"scale":[1,1,2]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"mode":5,"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}}}}],
            "accessors":[{{"bufferView":0,"componentType":5121,"count":4,"type":"VEC3"}}],
            "bufferViews":[{{"buffer":0,"byteLength":16,"byteStride":4}}],
            "buffers":[{{"byteLength":16,"uri":"data:application/octet-stream;base64,{}"}}]}}"#,
            base64_encode(&bin)
        );
        let objs = parse(&json, None, None).unwrap();
        assert_eq!(objs.len(), 1);
        let o = &objs[0];
        assert_eq!(o.name, "Strip");
        assert_eq!((o.mesh.vert_count(), o.mesh.face_count()), (4, 2), "a strip of two triangles");
        assert!((o.location - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-9, "parent and child transforms compose: {:?}", o.location);
        assert!((o.scale - Vec3::new(1.0, 1.0, 2.0)).length() < 1e-9);
        assert!(o.rotation.length() < 1e-9);
        assert_eq!(o.color.map(|c| c.r > 0.99 && c.g < 0.01), Some(true), "red base colour");
        assert!(matches!(parse("{}", None, None), Err(GltfError::Empty)));
        assert!(parse("{", None, None).is_err());
    }
}
