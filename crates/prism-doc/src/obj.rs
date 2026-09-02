//! Wavefront OBJ, the lingua franca of mesh exchange (Phase 7). Own parser.
//!
//! Import reads `v` and `f` (any polygon size; `v`, `v/vt`, `v//vn` and
//! `v/vt/vn` index forms; negative indices count back from the end) and
//! starts a new mesh at every `o`. Anything else (`vn`, `vt`, `g`, `s`,
//! materials) is skipped. Export writes every visible mesh object in world
//! space as its own `o` group. Coordinates pass through untouched: Prism and
//! the common OBJ convention are both Y-up.

use core::fmt;
use std::collections::HashMap;
use std::path::Path;

use prism_math::Vec3;
use prism_mesh::{Mesh, VertH};

use crate::blocks::DataKind;
use crate::doc::Doc;

#[derive(Debug)]
pub enum ObjError {
    Io(std::io::Error),
    Parse { line: usize, msg: String },
    /// The file had no faces at all.
    Empty,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjError::Io(e) => write!(f, "{e}"),
            ObjError::Parse { line, msg } => write!(f, "line {line}: {msg}"),
            ObjError::Empty => write!(f, "no faces in file"),
        }
    }
}

impl std::error::Error for ObjError {}

impl From<std::io::Error> for ObjError {
    fn from(e: std::io::Error) -> Self {
        ObjError::Io(e)
    }
}

/// One mesh read from an OBJ, named after its `o` group (or the file).
pub struct ObjMesh {
    pub name: String,
    pub mesh: Mesh,
    /// Faces the kernel refused (degenerate, repeated vertices).
    pub skipped: usize,
}

/// A group being built: the mesh plus which global OBJ vertices it has so far.
struct Group {
    name: String,
    mesh: Mesh,
    verts: HashMap<usize, VertH>,
    skipped: usize,
}

impl Group {
    fn new(name: &str) -> Self {
        Self { name: name.to_owned(), mesh: Mesh::new(), verts: HashMap::new(), skipped: 0 }
    }

    fn vert(&mut self, index: usize, positions: &[Vec3]) -> VertH {
        *self.verts.entry(index).or_insert_with(|| self.mesh.make_vert(positions[index]))
    }

    fn finish(self) -> Option<ObjMesh> {
        (self.mesh.face_count() > 0).then_some(ObjMesh { name: self.name, mesh: self.mesh, skipped: self.skipped })
    }
}

/// Resolve one `f` index token (`7`, `7/2`, `7//3`, `-1/-1/-1`) to a
/// zero-based position index.
fn face_index(token: &str, count: usize, line: usize) -> Result<usize, ObjError> {
    let first = token.split('/').next().unwrap_or("");
    let i: i64 = first.parse().map_err(|_| ObjError::Parse { line, msg: format!("bad face index `{token}`") })?;
    let idx = if i < 0 { count as i64 + i } else { i - 1 };
    if idx < 0 || idx as usize >= count {
        return Err(ObjError::Parse { line, msg: format!("face index {i} out of range (have {count} vertices)") });
    }
    Ok(idx as usize)
}

/// Make any missing edges, then the face; `None` if the kernel refuses.
fn try_add_face(mesh: &mut Mesh, verts: &[VertH]) -> Option<()> {
    let n = verts.len();
    if n < 3 || (0..n).any(|i| verts[i + 1..].contains(&verts[i])) {
        return None;
    }
    for i in 0..n {
        let (a, b) = (verts[i], verts[(i + 1) % n]);
        if mesh.edge_between(a, b).is_none() {
            mesh.make_edge(a, b).ok()?;
        }
    }
    mesh.make_face(verts).ok().map(|_| ())
}

/// Parse OBJ text. `default_name` names a mesh with no `o` line.
pub fn parse(text: &str, default_name: &str) -> Result<Vec<ObjMesh>, ObjError> {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut out: Vec<ObjMesh> = Vec::new();
    let mut group = Group::new(default_name);
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let content = raw.split('#').next().unwrap_or("").trim();
        let mut words = content.split_whitespace();
        let Some(key) = words.next() else {
            continue;
        };
        match key {
            "v" => {
                let mut xyz = [0.0f64; 3];
                for (k, slot) in xyz.iter_mut().enumerate() {
                    let w = words.next().ok_or_else(|| ObjError::Parse { line, msg: "vertex needs x y z".into() })?;
                    *slot = w.parse().map_err(|_| ObjError::Parse { line, msg: format!("bad coordinate {} `{w}`", ["x", "y", "z"][k]) })?;
                }
                positions.push(Vec3::new(xyz[0], xyz[1], xyz[2]));
            }
            "f" => {
                let mut verts: Vec<VertH> = Vec::new();
                for token in words {
                    let idx = face_index(token, positions.len(), line)?;
                    verts.push(group.vert(idx, &positions));
                }
                if try_add_face(&mut group.mesh, &verts).is_none() {
                    group.skipped += 1;
                }
            }
            "o" => {
                let name = words.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() { default_name.to_owned() } else { name };
                if group.mesh.face_count() == 0 {
                    group.name = name;
                } else {
                    let done = std::mem::replace(&mut group, Group::new(&name));
                    out.extend(done.finish());
                }
            }
            _ => {}
        }
    }
    out.extend(group.finish());
    if out.is_empty() {
        return Err(ObjError::Empty);
    }
    Ok(out)
}

pub fn read_file(path: &Path) -> Result<Vec<ObjMesh>, ObjError> {
    let text = std::fs::read_to_string(path)?;
    let name = path.file_stem().map_or_else(|| "Imported".to_owned(), |s| s.to_string_lossy().into_owned());
    parse(&text, &name)
}

fn num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

/// Every visible mesh object of the active scene, in world space. Returns
/// the text and how many objects it holds.
pub fn write(doc: &Doc) -> (String, usize) {
    let mut out = String::from("# Prism OBJ export\n");
    let mut base = 1usize; // OBJ indices are global and one-based
    let mut count = 0;
    for id in doc.scene_objects() {
        let Some(obj) = doc.objects.get(id) else {
            continue;
        };
        if obj.kind != DataKind::Mesh || !obj.visible {
            continue;
        }
        let Some(block) = doc.meshes.get(obj.data) else {
            continue;
        };
        let m = &block.mesh;
        let world = doc.object_matrix(id);
        out.push_str(&format!("o {}\n", obj.name.replace(char::is_whitespace, "_")));
        let mut index: HashMap<VertH, usize> = HashMap::with_capacity(m.vert_count());
        for v in m.verts() {
            let p = world.transform_point(m.position(v));
            out.push_str(&format!("v {} {} {}\n", num(p.x), num(p.y), num(p.z)));
            index.insert(v, base + index.len());
        }
        for f in m.faces() {
            out.push('f');
            for v in m.verts_of_face(f) {
                out.push(' ');
                out.push_str(&index[&v].to_string());
            }
            out.push('\n');
        }
        base += index.len();
        count += 1;
    }
    (out, count)
}

/// Write the scene to `path`. Returns how many objects were written.
pub fn write_file(doc: &Doc, path: &Path) -> Result<usize, ObjError> {
    let (text, count) = write(doc);
    std::fs::write(path, text)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = "# two boxes\no Quad\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\no Tri\nv 2 0 0\nv 3 0 0\nv 2.5 1 0\nf -3/1/1 -2/2/1 -1//1\nf 5 5 6\n";

    #[test]
    fn parses_groups_index_forms_and_skips_junk() {
        let meshes = parse(TWO, "file").unwrap();
        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].name, "Quad");
        assert_eq!((meshes[0].mesh.vert_count(), meshes[0].mesh.face_count()), (4, 1));
        assert_eq!(meshes[1].name, "Tri");
        assert_eq!((meshes[1].mesh.vert_count(), meshes[1].mesh.face_count()), (3, 1), "each group only holds the vertices it uses");
        assert_eq!(meshes[1].skipped, 1, "a face with a repeated vertex is refused, not fatal");
        let tri = &meshes[1].mesh;
        let ys: Vec<f64> = tri.verts().map(|v| tri.position(v).y).collect();
        assert!(ys.contains(&1.0));
        assert!(matches!(parse("v 0 0 0\nf 1 2 3\n", "x"), Err(ObjError::Parse { line: 2, .. })));
        assert!(matches!(parse("v 0 0 0\n", "x"), Err(ObjError::Empty)));
        assert!(matches!(parse("v 0 zero 0\n", "x"), Err(ObjError::Parse { line: 1, .. })));
    }

    #[test]
    fn export_round_trips_the_starter_cube_in_world_space() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        doc.objects.get_mut(cube).unwrap().location = Vec3::new(10.0, 0.0, 0.0);
        let (text, count) = write(&doc);
        assert_eq!(count, 1, "only the mesh is exported");
        assert!(text.contains("o Cube\n"));
        assert_eq!(text.matches("\nv ").count(), 8);
        assert_eq!(text.matches("\nf ").count(), 6);
        let back = parse(&text, "x").unwrap();
        let m = &back[0].mesh;
        assert_eq!((m.vert_count(), m.edge_count(), m.face_count()), (8, 12, 6));
        let min_x = m.verts().map(|v| m.position(v).x).fold(f64::MAX, f64::min);
        assert!((min_x - 9.0).abs() < 1e-9, "world transform baked in: {min_x}");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(2.0), "2");
    }
}
