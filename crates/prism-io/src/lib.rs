//! Prism io: exchange formats (D026, D031). OBJ for the simplest possible
//! handoff, glTF 2.0 (`.glb` and `.gltf`) for game engines and DCC tools.
//! Everything here is our own code, including the JSON reader and writer.
//! Exports write the evaluated mesh (modifiers applied), as the viewport shows it.

pub mod gltf;
pub mod json;
pub mod obj;

pub use gltf::{GltfError, GltfObject};
pub use json::{Json, JsonError};
pub use obj::{ObjError, ObjMesh};

use prism_mesh::{FaceH, Mesh, VertH};

/// Make any missing edges, then the face; `None` if the kernel refuses
/// (repeated vertices, fewer than three). Importers use it so one bad
/// polygon never fails a whole file.
pub(crate) fn add_face_lenient(mesh: &mut Mesh, verts: &[VertH]) -> Option<FaceH> {
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
    mesh.make_face(verts).ok()
}
