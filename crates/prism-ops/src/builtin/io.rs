//! Import and export in exchange formats (Phase 7, D031): OBJ and glTF.

use std::path::{Path, PathBuf};

use prism_doc::DataKind;

use crate::builtin::wm::PathProps;
use crate::context::{Ctx, Outcome, UiRequest};
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

/// The document's path with `ext`, or `untitled.ext`.
fn sibling(ctx: &Ctx, ext: &str) -> String {
    ctx.doc.path.as_ref().map_or_else(|| format!("untitled.{ext}"), |p| p.with_extension(ext).display().to_string())
}

/// Import every mesh of an OBJ as a new object; the imports end up selected
/// with the last one active.
pub struct ImportObj;
impl Operator for ImportObj {
    const ID: &'static str = "wm.import_obj";
    const LABEL: &'static str = "Import OBJ…";
    const FLAGS: OpFlags = OpFlags::DEFAULT;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            let suggest = Some(sibling(ctx, "obj"));
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: false, suggest });
            return Ok(Outcome::Cancelled);
        }
        let path = Path::new(&p.path);
        let meshes = prism_io::obj::read_file(path)?;
        for id in ctx.doc.scene_objects() {
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.selected = false;
            }
        }
        let (mut verts, mut faces, mut skipped) = (0, 0, 0);
        let n = meshes.len();
        for m in meshes {
            verts += m.mesh.vert_count();
            faces += m.mesh.face_count();
            skipped += m.skipped;
            let mesh = ctx.doc.add_mesh(&m.name, m.mesh);
            let obj = ctx.doc.add_object(&m.name, DataKind::Mesh, mesh);
            if let Some(o) = ctx.doc.objects.get_mut(obj) {
                o.selected = true;
            }
        }
        let note = if skipped > 0 { format!(", {skipped} face(s) skipped") } else { String::new() };
        ctx.report(format!("Imported {n} object(s): {verts} vertices, {faces} faces{note}"));
        Ok(Outcome::Finished)
    }
}

/// Export every visible mesh object of the scene, in world space.
pub struct ExportObj;
impl Operator for ExportObj {
    const ID: &'static str = "wm.export_obj";
    const LABEL: &'static str = "Export OBJ…";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            let suggest = Some(sibling(ctx, "obj"));
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: true, suggest });
            return Ok(Outcome::Cancelled);
        }
        let path = PathBuf::from(&p.path);
        let n = prism_io::obj::write_file(ctx.doc, &path)?;
        ctx.report(format!("Exported {n} object(s) to {}", path.display()));
        Ok(Outcome::Finished)
    }
}

/// Import a `.glb` / `.gltf`: one object per node with a mesh, with its
/// transform and base colour; the imports end up selected.
pub struct ImportGltf;
impl Operator for ImportGltf {
    const ID: &'static str = "wm.import_gltf";
    const LABEL: &'static str = "Import glTF…";
    const FLAGS: OpFlags = OpFlags::DEFAULT;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            let suggest = Some(sibling(ctx, "glb"));
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: false, suggest });
            return Ok(Outcome::Cancelled);
        }
        let objects = prism_io::gltf::read_file(Path::new(&p.path))?;
        for id in ctx.doc.scene_objects() {
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.selected = false;
            }
        }
        let (mut verts, mut faces, mut skipped) = (0, 0, 0);
        let n = objects.len();
        for g in objects {
            verts += g.mesh.vert_count();
            faces += g.mesh.face_count();
            skipped += g.skipped;
            let mesh = ctx.doc.add_mesh(&g.name, g.mesh);
            if let Some(color) = g.color {
                let mat = ctx.doc.add_material(&g.name);
                if let Some(m) = ctx.doc.materials.get_mut(mat) {
                    m.color = color;
                }
                if let Some(block) = ctx.doc.meshes.get_mut(mesh) {
                    block.props.materials.push(mat);
                }
            }
            let obj = ctx.doc.add_object(&g.name, DataKind::Mesh, mesh);
            if let Some(o) = ctx.doc.objects.get_mut(obj) {
                o.location = g.location;
                o.rotation = g.rotation;
                o.scale = g.scale;
                o.selected = true;
            }
        }
        let note = if skipped > 0 { format!(", {skipped} triangle(s) skipped") } else { String::new() };
        ctx.report(format!("Imported {n} object(s): {verts} vertices, {faces} faces{note}"));
        Ok(Outcome::Finished)
    }
}

/// Export every visible mesh object as glTF: `.glb`, or `.gltf` when the
/// name says so. Modifiers are applied.
pub struct ExportGltf;
impl Operator for ExportGltf {
    const ID: &'static str = "wm.export_gltf";
    const LABEL: &'static str = "Export glTF…";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            let suggest = Some(sibling(ctx, "glb"));
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: true, suggest });
            return Ok(Outcome::Cancelled);
        }
        let path = PathBuf::from(&p.path);
        let n = prism_io::gltf::write_file(ctx.doc, &path)?;
        ctx.report(format!("Exported {n} object(s) to {}", path.display()));
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<ImportObj>();
    r.register::<ExportObj>();
    r.register::<ImportGltf>();
    r.register::<ExportGltf>();
}

#[cfg(test)]
mod tests {
    use crate::context::Ctx;
    use crate::executor::Executor;
    use prism_doc::Doc;
    use prism_props::Value;

    #[test]
    fn export_then_import_adds_objects_and_records_undo() {
        let dir = std::env::temp_dir().join(format!("prism-obj-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cube.obj");
        let mut doc = Doc::starter();
        let mut ex = Executor::with_builtins();
        let mut c = Ctx::new(&mut doc);
        ex.run_with("wm.export_obj", &[("path", Value::Str(path.display().to_string()))], &mut c).unwrap();
        assert!(ex.last_report.as_deref().unwrap().starts_with("Exported 1 object"));
        let n = doc.objects.len();
        let mut c = Ctx::new(&mut doc);
        ex.run_with("wm.import_obj", &[("path", Value::Str(path.display().to_string()))], &mut c).unwrap();
        assert_eq!(doc.objects.len(), n + 1);
        assert_eq!(doc.active_object().unwrap().name, "Cube", "named after its `o` group");
        assert_eq!(doc.selected_objects().len(), 1, "imports replace the selection");
        assert!(ex.last_report.as_deref().unwrap().contains("8 vertices, 6 faces"));
        ex.undo(&mut doc);
        assert_eq!(doc.objects.len(), n);
        // Asking without a path opens the dialog instead.
        let mut c = Ctx::new(&mut doc);
        ex.run("wm.import_obj", None, &mut c).unwrap();
        assert!(ex.requests.iter().any(|r| matches!(r, crate::context::UiRequest::PathDialog { save: false, suggest: Some(s), .. } if s.ends_with("untitled.obj"))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gltf_round_trip_keeps_transform_and_colour() {
        let dir = std::env::temp_dir().join(format!("prism-glb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tree.glb");
        let mut doc = Doc::starter();
        let mut ex = Executor::with_builtins();
        let cube = doc.scene_objects()[0];
        doc.objects.get_mut(cube).unwrap().location = prism_math::Vec3::new(3.0, 1.0, 0.0);
        ex.run_with("wm.export_gltf", &[("path", Value::Str(path.display().to_string()))], &mut Ctx::new(&mut doc)).unwrap();
        let n = doc.objects.len();
        ex.run_with("wm.import_gltf", &[("path", Value::Str(path.display().to_string()))], &mut Ctx::new(&mut doc)).unwrap();
        assert_eq!(doc.objects.len(), n + 1);
        let o = doc.active_object().unwrap();
        assert_eq!(o.name, "Cube");
        assert!((o.location - prism_math::Vec3::new(3.0, 1.0, 0.0)).length() < 1e-9);
        assert_eq!(doc.object_mesh(doc.active_object_id()).unwrap().mesh.face_count(), 12);
        assert!(ex.last_report.as_deref().unwrap().contains("8 vertices, 12 faces"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
