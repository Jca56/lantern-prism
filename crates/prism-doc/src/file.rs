//! The `.prism` file: a header and a sequence of tagged chunks, one per
//! datablock. Structs serialize with stable field ids (D012), so old files
//! load in new builds and unknown chunks are skipped.
//!
//! ```text
//! "PRSM" u32 format_version u32 app_version_len app_version
//! chunk := [u8; 4] tag, u64 id, u32 len, payload
//! ```

use core::fmt;
use std::path::Path;

use prism_core::Id;
use prism_props::{Reflect, serial};

use crate::blocks::{Camera, Collection, Light, Material, MeshBlock, MeshProps, Object, Scene};
use crate::doc::{Doc, DocProps};
use crate::mesh_io;

const MAGIC: &[u8; 4] = b"PRSM";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug)]
pub enum FileError {
    NotAPrismFile,
    UnsupportedVersion(u32),
    Truncated,
    Chunk(String),
    Io(std::io::Error),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::NotAPrismFile => write!(f, "not a Prism file"),
            FileError::UnsupportedVersion(v) => write!(f, "file format version {v} is newer than this build"),
            FileError::Truncated => write!(f, "file is truncated"),
            FileError::Chunk(s) => write!(f, "bad chunk: {s}"),
            FileError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        FileError::Io(e)
    }
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], id: Id, payload: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&id.raw().to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
}

fn block_chunk(out: &mut Vec<u8>, tag: &[u8; 4], id: Id, r: &dyn Reflect) {
    chunk(out, tag, id, &serial::to_bytes(r));
}

/// Serialize the document.
pub fn save(doc: &Doc) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    let app = env!("CARGO_PKG_VERSION");
    out.extend_from_slice(&(app.len() as u32).to_le_bytes());
    out.extend_from_slice(app.as_bytes());

    block_chunk(&mut out, b"DOCM", Id::NONE, &doc.doc_props());
    for (id, s) in doc.scenes.iter() {
        block_chunk(&mut out, b"SCEN", id, s);
    }
    for (id, c) in doc.collections.iter() {
        block_chunk(&mut out, b"COLL", id, c);
    }
    for (id, o) in doc.objects.iter() {
        block_chunk(&mut out, b"OBJT", id, o);
    }
    for (id, m) in doc.meshes.iter() {
        let mut payload = serial::to_bytes(&m.props);
        let props_len = payload.len() as u32;
        let mut body = Vec::new();
        mesh_io::write(&m.mesh, &mut body);
        // props_len prefix so the reader knows where the geometry starts.
        let mut full = props_len.to_le_bytes().to_vec();
        full.append(&mut payload);
        full.append(&mut body);
        chunk(&mut out, b"MESH", id, &full);
    }
    for (id, m) in doc.materials.iter() {
        block_chunk(&mut out, b"MATL", id, m);
    }
    for (id, c) in doc.cameras.iter() {
        block_chunk(&mut out, b"CAMR", id, c);
    }
    for (id, l) in doc.lights.iter() {
        block_chunk(&mut out, b"LGHT", id, l);
    }
    out
}

fn read_block<T: Reflect + Default>(payload: &[u8], tag: &str) -> Result<T, FileError> {
    let mut b = T::default();
    serial::from_bytes(&mut b, payload).map_err(|e| FileError::Chunk(format!("{tag}: {e}")))?;
    Ok(b)
}

/// Parse a document. Unknown chunk tags are skipped.
pub fn load(bytes: &[u8]) -> Result<Doc, FileError> {
    if bytes.len() < 12 || &bytes[..4] != MAGIC {
        return Err(FileError::NotAPrismFile);
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version > FORMAT_VERSION {
        return Err(FileError::UnsupportedVersion(version));
    }
    let app_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut pos = 12 + app_len;
    if pos > bytes.len() {
        return Err(FileError::Truncated);
    }

    let mut doc = Doc::new();
    // Start from nothing: `Doc::new` made a scene we do not want.
    doc.scenes = Default::default();
    doc.collections = Default::default();
    doc.active_scene = Id::NONE;
    let mut max_id = 0u64;

    while pos < bytes.len() {
        if pos + 16 > bytes.len() {
            return Err(FileError::Truncated);
        }
        let tag: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        let id = Id(u64::from_le_bytes(bytes[pos + 4..pos + 12].try_into().unwrap()));
        let len = u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().unwrap()) as usize;
        pos += 16;
        if pos + len > bytes.len() {
            return Err(FileError::Truncated);
        }
        let payload = &bytes[pos..pos + len];
        pos += len;
        max_id = max_id.max(id.raw());
        match &tag {
            b"DOCM" => {
                let p: DocProps = read_block(payload, "DOCM")?;
                doc.active_scene = p.active_scene;
                doc.ids = prism_core::IdAllocator::resume(p.next_id.max(1) as u64);
            }
            b"SCEN" => doc.scenes.insert(id, read_block::<Scene>(payload, "SCEN")?),
            b"COLL" => doc.collections.insert(id, read_block::<Collection>(payload, "COLL")?),
            b"OBJT" => doc.objects.insert(id, read_block::<Object>(payload, "OBJT")?),
            b"MATL" => doc.materials.insert(id, read_block::<Material>(payload, "MATL")?),
            b"CAMR" => doc.cameras.insert(id, read_block::<Camera>(payload, "CAMR")?),
            b"LGHT" => doc.lights.insert(id, read_block::<Light>(payload, "LGHT")?),
            b"MESH" => {
                if payload.len() < 4 {
                    return Err(FileError::Truncated);
                }
                let props_len = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
                if 4 + props_len > payload.len() {
                    return Err(FileError::Truncated);
                }
                let props: MeshProps = read_block(&payload[4..4 + props_len], "MESH")?;
                let mesh = mesh_io::read(&payload[4 + props_len..]).map_err(|e| FileError::Chunk(format!("MESH: {e}")))?;
                doc.meshes.insert(id, MeshBlock { props, mesh, edit: Default::default() });
            }
            _ => {} // unknown chunk from a newer build: skip
        }
    }
    doc.ids.reserve(Id(max_id));
    if doc.active_scene.is_none() {
        doc.active_scene = doc.scenes.ids().next().unwrap_or(Id::NONE);
    }
    Ok(doc)
}

pub fn save_file(doc: &Doc, path: &Path) -> Result<(), FileError> {
    std::fs::write(path, save(doc))?;
    Ok(())
}

pub fn load_file(path: &Path) -> Result<Doc, FileError> {
    let mut doc = load(&std::fs::read(path)?)?;
    doc.path = Some(path.to_path_buf());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::Vec3;

    #[test]
    fn roundtrip_byte_for_byte() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        doc.objects.get_mut(cube).unwrap().location = Vec3::new(1.5, -2.0, 0.25);
        doc.objects.get_mut(cube).unwrap().selected = true;
        let mat = doc.add_material("Red");
        doc.materials.get_mut(mat).unwrap().color = prism_math::Color::RED;
        doc.object_mesh_mut(cube).unwrap().props.materials.push(mat);
        let sphere = doc.add_mesh("Sphere", prism_mesh::primitives::uv_sphere(1.0, 6, 4));
        doc.add_object("Sphere", crate::blocks::DataKind::Mesh, sphere);

        let bytes = save(&doc);
        let back = load(&bytes).unwrap();
        assert_eq!(back.objects.len(), doc.objects.len());
        assert_eq!(back.meshes.len(), 2);
        assert_eq!(back.materials.len(), 1);
        assert_eq!(back.active_scene, doc.active_scene);
        assert_eq!(back.objects.get(cube).unwrap().location, Vec3::new(1.5, -2.0, 0.25));
        assert!(back.objects.get(cube).unwrap().selected);
        assert_eq!(back.object_mesh(cube).unwrap().props.materials, vec![mat]);
        assert_eq!(back.object_mesh(cube).unwrap().mesh.face_count(), 6);
        assert_eq!(back.ids.peek(), doc.ids.peek(), "id allocator resumes");
        assert_eq!(save(&back), bytes, "save → load → save is byte-identical");
    }

    #[test]
    fn errors_and_unknown_chunks() {
        assert!(matches!(load(b"nope"), Err(FileError::NotAPrismFile)));
        let mut bytes = save(&Doc::starter());
        // Append an unknown chunk: skipped.
        chunk(&mut bytes, b"ZZZZ", Id(999), &[1, 2, 3]);
        let doc = load(&bytes).unwrap();
        assert_eq!(doc.objects.len(), 3);
        // Truncate mid-chunk.
        let cut = &bytes[..bytes.len() - 2];
        assert!(matches!(load(cut), Err(FileError::Truncated)));
        // Future version refused.
        let mut future = save(&Doc::starter());
        future[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(load(&future), Err(FileError::UnsupportedVersion(99))));
    }

    #[test]
    fn files_on_disk() {
        let dir = std::env::temp_dir().join(format!("prism-doc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.prism");
        let doc = Doc::starter();
        save_file(&doc, &path).unwrap();
        let back = load_file(&path).unwrap();
        assert_eq!(back.path.as_deref(), Some(path.as_path()));
        assert_eq!(back.objects.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
