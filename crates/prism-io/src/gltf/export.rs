//! glTF export: one node per visible mesh object carrying its transform, one
//! mesh per object with a single triangle primitive (positions, normals,
//! indices) built from the evaluated mesh, and a base-colour material.

use std::collections::HashMap;
use std::path::Path;

use prism_core::Id;
use prism_doc::blocks::Material;
use prism_doc::{DataKind, Doc};
use prism_math::{Quat, Vec3};

use super::{GltfError, base64_encode, pack_glb};
use crate::json::Json;

const FLOAT: usize = 5126;
const UNSIGNED_INT: usize = 5125;
const ARRAY_BUFFER: usize = 34962;
const ELEMENT_ARRAY_BUFFER: usize = 34963;

/// A built glTF: the JSON document and its binary buffer.
pub struct Exported {
    pub json: Json,
    pub bin: Vec<u8>,
    pub objects: usize,
}

fn f32s(v: &[Vec3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 12);
    for p in v {
        for c in [p.x, p.y, p.z] {
            out.extend_from_slice(&(c as f32).to_le_bytes());
        }
    }
    out
}

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|i| i.to_le_bytes()).collect()
}

#[derive(Default)]
struct Buffer {
    bin: Vec<u8>,
    views: Vec<Json>,
    accessors: Vec<Json>,
}

impl Buffer {
    fn view(&mut self, data: &[u8], target: usize) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        self.views.push(Json::obj(vec![("buffer", 0usize.into()), ("byteOffset", self.bin.len().into()), ("byteLength", data.len().into()), ("target", target.into())]));
        self.bin.extend_from_slice(data);
        self.views.len() - 1
    }

    fn accessor(&mut self, view: usize, component: usize, count: usize, kind: &str, bounds: Option<(Vec3, Vec3)>) -> usize {
        let mut a = Json::obj(vec![("bufferView", view.into()), ("componentType", component.into()), ("count", count.into()), ("type", kind.into())]);
        if let Some((lo, hi)) = bounds {
            a.insert("min", Json::nums([lo.x, lo.y, lo.z]));
            a.insert("max", Json::nums([hi.x, hi.y, hi.z]));
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
}

fn bounds(pts: &[Vec3]) -> (Vec3, Vec3) {
    pts.iter().fold((Vec3::splat(f64::MAX), Vec3::splat(f64::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

fn material_json(mat: &Material) -> Json {
    let c = mat.color.to_linear(); // glTF colours are linear
    Json::obj(vec![
        ("name", mat.name.as_str().into()),
        ("pbrMetallicRoughness", Json::obj(vec![("baseColorFactor", Json::nums([c.r, c.g, c.b, c.a])), ("metallicFactor", mat.metallic.into()), ("roughnessFactor", mat.roughness.into())])),
    ])
}

fn assemble(doc: &Doc, embed: bool) -> Exported {
    let mut buf = Buffer::default();
    let mut meshes: Vec<Json> = Vec::new();
    let mut nodes: Vec<Json> = Vec::new();
    let mut materials: Vec<Json> = Vec::new();
    let mut material_index: HashMap<Id, usize> = HashMap::new();
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
        let eval = prism_eval::apply_modifiers(&block.mesh, &block.modifiers);
        let b = prism_eval::evaluate(&eval.mesh);
        if b.tri_indices.is_empty() {
            continue;
        }
        let iv = buf.view(&u32s(&b.tri_indices), ELEMENT_ARRAY_BUFFER);
        let pv = buf.view(&f32s(&b.corner_positions), ARRAY_BUFFER);
        let nv = buf.view(&f32s(&b.corner_normals), ARRAY_BUFFER);
        let ia = buf.accessor(iv, UNSIGNED_INT, b.tri_indices.len(), "SCALAR", None);
        let pa = buf.accessor(pv, FLOAT, b.corner_positions.len(), "VEC3", Some(bounds(&b.corner_positions)));
        let na = buf.accessor(nv, FLOAT, b.corner_normals.len(), "VEC3", None);
        let mut prim = Json::obj(vec![("attributes", Json::obj(vec![("POSITION", pa.into()), ("NORMAL", na.into())])), ("indices", ia.into()), ("mode", 4usize.into())]);
        if let Some(&mid) = block.props.materials.first()
            && let Some(mat) = doc.materials.get(mid)
        {
            let idx = *material_index.entry(mid).or_insert_with(|| {
                materials.push(material_json(mat));
                materials.len() - 1
            });
            prim.insert("material", idx.into());
        }
        meshes.push(Json::obj(vec![("name", block.props.name.as_str().into()), ("primitives", vec![prim].into())]));
        let (t, r, s) = (obj.location, obj.rotation, obj.scale);
        let q = Quat::from_euler_xyz(r.x, r.y, r.z);
        nodes.push(Json::obj(vec![
            ("name", obj.name.as_str().into()),
            ("mesh", (meshes.len() - 1).into()),
            ("translation", Json::nums([t.x, t.y, t.z])),
            ("rotation", Json::nums([q.x, q.y, q.z, q.w])),
            ("scale", Json::nums([s.x, s.y, s.z])),
        ]));
    }
    let objects = nodes.len();
    let roots: Vec<Json> = (0..nodes.len()).map(Json::from).collect();
    let mut buffer = Json::obj(vec![("byteLength", buf.bin.len().into())]);
    if embed {
        buffer.insert("uri", format!("data:application/octet-stream;base64,{}", base64_encode(&buf.bin)).into());
    }
    let mut json = Json::obj(vec![
        ("asset", Json::obj(vec![("version", "2.0".into()), ("generator", "Prism".into())])),
        ("scene", 0usize.into()),
        ("scenes", vec![Json::obj(vec![("nodes", roots.into())])].into()),
        ("nodes", nodes.into()),
        ("meshes", meshes.into()),
    ]);
    if !materials.is_empty() {
        json.insert("materials", materials.into());
    }
    json.insert("accessors", buf.accessors.into());
    json.insert("bufferViews", buf.views.into());
    json.insert("buffers", vec![buffer].into());
    Exported { json, bin: buf.bin, objects }
}

/// The scene as glTF JSON plus a binary buffer (for a `.glb`).
pub fn export(doc: &Doc) -> Exported {
    assemble(doc, false)
}

/// Write `.glb` (binary) or, when the name ends in `.gltf`, JSON with the
/// buffer embedded. Returns how many objects were written.
pub fn write_file(doc: &Doc, path: &Path) -> Result<usize, GltfError> {
    let as_json = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("gltf"));
    let e = assemble(doc, as_json);
    if e.objects == 0 {
        return Err(GltfError::Empty);
    }
    if as_json {
        std::fs::write(path, e.json.to_text())?;
    } else {
        std::fs::write(path, pack_glb(&e.json.to_text(), &e.bin))?;
    }
    Ok(e.objects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::import::parse;
    use prism_math::Color;

    #[test]
    fn export_then_import_round_trips_the_starter_scene() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        {
            let o = doc.objects.get_mut(cube).unwrap();
            o.location = Vec3::new(1.0, 2.0, 3.0);
            o.rotation = Vec3::new(0.0, 0.5, 0.0);
            o.scale = Vec3::new(2.0, 1.0, 1.0);
        }
        let mat = doc.add_material("Bark");
        doc.materials.get_mut(mat).unwrap().color = Color::rgb(0.4, 0.25, 0.1);
        let mesh = doc.objects.get(cube).unwrap().data;
        doc.meshes.get_mut(mesh).unwrap().props.materials.push(mat);

        let e = export(&doc);
        assert_eq!(e.objects, 1, "lights and cameras stay home");
        assert_eq!(e.json.get("asset").and_then(|a| a.get("version")).and_then(Json::as_str), Some("2.0"));
        assert_eq!(e.json.get("accessors").and_then(Json::as_arr).map(<[Json]>::len), Some(3));
        assert_eq!(e.bin.len() % 4, 0);

        let back = parse(&e.json.to_text(), Some(e.bin.clone()), None).unwrap();
        assert_eq!(back.len(), 1);
        let o = &back[0];
        assert_eq!(o.name, "Cube");
        assert_eq!((o.mesh.vert_count(), o.mesh.face_count()), (8, 12), "24 split corners weld back to 8; 6 quads arrive as 12 triangles");
        o.mesh.validate().unwrap();
        assert!((o.location - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
        assert!((o.rotation - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-9, "{:?}", o.rotation);
        assert!((o.scale - Vec3::new(2.0, 1.0, 1.0)).length() < 1e-9);
        let c = o.color.unwrap();
        assert!((c.r - 0.4).abs() < 1e-6 && (c.g - 0.25).abs() < 1e-6 && (c.b - 0.1).abs() < 1e-6, "colour survives the linear round trip: {c:?}");
    }

    #[test]
    fn glb_and_gltf_files_both_load_back() {
        let dir = std::env::temp_dir().join(format!("prism-gltf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = Doc::starter();
        for name in ["scene.glb", "scene.gltf"] {
            let path = dir.join(name);
            assert_eq!(write_file(&doc, &path).unwrap(), 1);
            let back = crate::gltf::read_file(&path).unwrap();
            assert_eq!(back[0].mesh.face_count(), 12, "{name}");
        }
        assert!(std::fs::read(dir.join("scene.glb")).unwrap().starts_with(b"glTF"));
        assert!(std::fs::read_to_string(dir.join("scene.gltf")).unwrap().contains("data:application/octet-stream;base64,"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
