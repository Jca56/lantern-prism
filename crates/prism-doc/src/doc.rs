//! `Doc`: every store plus the id allocator and what is active.

use std::path::PathBuf;

use prism_core::{Id, IdAllocator};
use prism_math::Mat4;
use prism_mesh::Mesh;
use prism_props::props;

use crate::blocks::{Camera, Collection, DataKind, Light, Material, MeshBlock, Object, Scene};
use crate::store::Store;

props! {
    /// Document-level values that go in the file.
    pub struct DocProps {
        pub next_id: i64 = 1 => { id: 1, flags: HIDDEN },
        pub active_scene: Id = Id::NONE => { id: 2, flags: HIDDEN },
    }
}

#[derive(Clone, Debug)]
pub struct Doc {
    pub ids: IdAllocator,
    pub scenes: Store<Scene>,
    pub collections: Store<Collection>,
    pub objects: Store<Object>,
    pub meshes: Store<MeshBlock>,
    pub materials: Store<Material>,
    pub cameras: Store<Camera>,
    pub lights: Store<Light>,
    pub active_scene: Id,
    /// Where this document was loaded from or last saved. Not in the file.
    pub path: Option<PathBuf>,
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

impl Doc {
    /// An empty document with one empty scene.
    pub fn new() -> Self {
        let mut doc = Self {
            ids: IdAllocator::new(),
            scenes: Store::new(),
            collections: Store::new(),
            objects: Store::new(),
            meshes: Store::new(),
            materials: Store::new(),
            cameras: Store::new(),
            lights: Store::new(),
            active_scene: Id::NONE,
            path: None,
        };
        doc.add_scene("Scene");
        doc
    }

    /// The document a fresh session opens: a cube, a light and a camera.
    pub fn starter() -> Self {
        let mut doc = Self::new();
        let mesh = doc.add_mesh("Cube", prism_mesh::primitives::cube(2.0));
        let cube = doc.add_object("Cube", DataKind::Mesh, mesh);
        let light = doc.add_light("Light");
        let lo = doc.add_object("Light", DataKind::Light, light);
        doc.objects.get_mut(lo).expect("just added").location = prism_math::Vec3::new(4.0, 6.0, 3.0);
        let cam = doc.add_camera("Camera");
        let co = doc.add_object("Camera", DataKind::Camera, cam);
        if let Some(o) = doc.objects.get_mut(co) {
            o.location = prism_math::Vec3::new(7.0, 5.0, 7.0);
            o.rotation = prism_math::Vec3::new(-0.45, 0.785, 0.0);
        }
        // The cube starts selected and active, so Tab, the gizmo and the
        // Edit button work from the first frame; it rests on the floor
        // rather than straddling it (Alva, 2026-09-02).
        if let Some(o) = doc.objects.get_mut(cube) {
            o.selected = true;
            o.location = prism_math::Vec3::new(0.0, 1.0, 0.0);
        }
        let scene_id = doc.active_scene;
        if let Some(s) = doc.scenes.get_mut(scene_id) {
            s.camera = co;
            s.active_object = cube;
        }
        doc
    }

    pub fn empty_scene_count(&self) -> usize {
        self.scenes.len()
    }

    // ---- creation ----------------------------------------------------------

    pub fn add_scene(&mut self, name: &str) -> Id {
        let root = self.ids.alloc();
        self.collections.insert(root, Collection { id: root, name: format!("{name} Collection"), ..Collection::default() });
        let id = self.ids.alloc();
        self.scenes.insert(id, Scene { id, name: name.to_owned(), collection: root, ..Scene::default() });
        if self.active_scene.is_none() {
            self.active_scene = id;
        }
        id
    }

    pub fn add_mesh(&mut self, name: &str, mesh: Mesh) -> Id {
        let id = self.ids.alloc();
        let mut block = MeshBlock::new(name, mesh);
        block.props.id = id;
        self.meshes.insert(id, block);
        id
    }

    pub fn add_material(&mut self, name: &str) -> Id {
        let id = self.ids.alloc();
        self.materials.insert(id, Material { id, name: name.to_owned(), ..Material::default() });
        id
    }

    pub fn add_camera(&mut self, name: &str) -> Id {
        let id = self.ids.alloc();
        self.cameras.insert(id, Camera { id, name: name.to_owned(), ..Camera::default() });
        id
    }

    pub fn add_light(&mut self, name: &str) -> Id {
        let id = self.ids.alloc();
        self.lights.insert(id, Light { id, name: name.to_owned(), ..Light::default() });
        id
    }

    /// Add an object to the active scene's root collection and make it active.
    pub fn add_object(&mut self, name: &str, kind: DataKind, data: Id) -> Id {
        let id = self.ids.alloc();
        self.objects.insert(id, Object { id, name: name.to_owned(), kind, data, ..Object::default() });
        let scene = self.active_scene;
        let root = self.scenes.get(scene).map_or(Id::NONE, |s| s.collection);
        if let Some(c) = self.collections.get_mut(root) {
            c.objects.push(id);
        }
        if let Some(s) = self.scenes.get_mut(scene) {
            s.active_object = id;
        }
        id
    }

    /// Remove an object and every reference to it. Its data stays.
    pub fn remove_object(&mut self, id: Id) -> bool {
        if self.objects.remove(id).is_none() {
            return false;
        }
        for cid in self.collections.ids().collect::<Vec<_>>() {
            if let Some(c) = self.collections.get(cid)
                && c.objects.contains(&id)
                && let Some(c) = self.collections.get_mut(cid)
            {
                c.objects.retain(|o| *o != id);
            }
        }
        for sid in self.scenes.ids().collect::<Vec<_>>() {
            if let Some(s) = self.scenes.get(sid)
                && (s.active_object == id || s.camera == id)
                && let Some(s) = self.scenes.get_mut(sid)
            {
                if s.active_object == id {
                    s.active_object = Id::NONE;
                }
                if s.camera == id {
                    s.camera = Id::NONE;
                }
            }
        }
        for oid in self.objects.ids().collect::<Vec<_>>() {
            if self.objects.get(oid).is_some_and(|o| o.parent == id)
                && let Some(o) = self.objects.get_mut(oid)
            {
                o.parent = Id::NONE;
            }
        }
        true
    }

    /// Drop mesh/camera/light blocks no object refers to. Returns how many.
    pub fn purge_orphans(&mut self) -> usize {
        let used: Vec<Id> = self.objects.iter().map(|(_, o)| o.data).collect();
        let mut n = 0;
        for id in self.meshes.ids().collect::<Vec<_>>() {
            if !used.contains(&id) {
                self.meshes.remove(id);
                n += 1;
            }
        }
        for id in self.cameras.ids().collect::<Vec<_>>() {
            if !used.contains(&id) {
                self.cameras.remove(id);
                n += 1;
            }
        }
        for id in self.lights.ids().collect::<Vec<_>>() {
            if !used.contains(&id) {
                self.lights.remove(id);
                n += 1;
            }
        }
        n
    }

    // ---- queries ----------------------------------------------------------

    pub fn scene(&self) -> Option<&Scene> {
        self.scenes.get(self.active_scene)
    }

    pub fn scene_mut(&mut self) -> Option<&mut Scene> {
        self.scenes.get_mut(self.active_scene)
    }

    pub fn active_object(&self) -> Option<&Object> {
        self.scene().and_then(|s| self.objects.get(s.active_object))
    }

    pub fn active_object_id(&self) -> Id {
        self.scene().map_or(Id::NONE, |s| s.active_object)
    }

    /// Objects of a collection and, recursively, its children.
    pub fn objects_in(&self, collection: Id, out: &mut Vec<Id>) {
        if let Some(c) = self.collections.get(collection) {
            out.extend(c.objects.iter().copied());
            for &child in &c.children {
                self.objects_in(child, out);
            }
        }
    }

    /// Every object of the active scene, in collection order.
    pub fn scene_objects(&self) -> Vec<Id> {
        let mut out = Vec::new();
        if let Some(s) = self.scene() {
            self.objects_in(s.collection, &mut out);
        }
        out
    }

    pub fn selected_objects(&self) -> Vec<Id> {
        self.scene_objects().into_iter().filter(|&id| self.objects.get(id).is_some_and(|o| o.selected)).collect()
    }

    /// World matrix, parents applied.
    pub fn object_matrix(&self, id: Id) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        let mut cur = id;
        let mut guard = 0;
        while let Some(o) = self.objects.get(cur) {
            m = o.matrix_local() * m;
            cur = o.parent;
            guard += 1;
            if cur.is_none() || guard > 64 {
                break;
            }
        }
        m
    }

    /// The mesh block behind an object, if it is a mesh object.
    pub fn object_mesh(&self, id: Id) -> Option<&MeshBlock> {
        let o = self.objects.get(id)?;
        (o.kind == DataKind::Mesh).then(|| self.meshes.get(o.data)).flatten()
    }

    pub fn object_mesh_mut(&mut self, id: Id) -> Option<&mut MeshBlock> {
        let data = self.objects.get(id).filter(|o| o.kind == DataKind::Mesh)?.data;
        self.meshes.get_mut(data)
    }

    /// Document-level props for the file.
    pub fn doc_props(&self) -> DocProps {
        DocProps { next_id: self.ids.peek().raw() as i64, active_scene: self.active_scene }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::Vec3;

    #[test]
    fn starter_document() {
        let doc = Doc::starter();
        assert_eq!(doc.scenes.len(), 1);
        assert_eq!(doc.objects.len(), 3);
        assert_eq!(doc.meshes.len(), 1);
        assert_eq!(doc.scene_objects().len(), 3);
        let cube = doc.scene_objects()[0];
        assert_eq!(doc.objects.get(cube).unwrap().name, "Cube");
        assert_eq!(doc.object_mesh(cube).unwrap().mesh.face_count(), 6);
        assert_eq!(doc.active_object().unwrap().name, "Cube", "the cube starts active, so edit mode is one Tab away");
        assert!(doc.objects.get(cube).unwrap().selected);
        assert!(doc.scene().unwrap().camera.is_some());
    }

    #[test]
    fn clone_shares_and_edit_isolates() {
        let a = Doc::starter();
        let mut b = a.clone();
        assert!(a.objects.ptr_eq(&b.objects));
        let cube = b.scene_objects()[0];
        let start = a.objects.get(cube).unwrap().location;
        b.objects.get_mut(cube).unwrap().location = Vec3::X;
        assert!(!a.objects.ptr_eq(&b.objects));
        assert!(a.meshes.ptr_eq(&b.meshes), "meshes untouched");
        assert_eq!(a.objects.get(cube).unwrap().location, start, "the original is untouched");
        assert_eq!(b.objects.get(cube).unwrap().location, Vec3::X);
    }

    #[test]
    fn remove_and_purge() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        let cam = doc.scene().unwrap().camera;
        assert!(doc.remove_object(cube));
        assert!(!doc.remove_object(cube));
        assert_eq!(doc.scene_objects().len(), 2);
        assert_eq!(doc.purge_orphans(), 1, "the cube mesh");
        assert!(doc.remove_object(cam));
        assert!(doc.scene().unwrap().camera.is_none());
        assert!(doc.scene().unwrap().active_object.is_none());
    }

    #[test]
    fn parent_matrices() {
        let mut doc = Doc::new();
        let a = doc.add_object("A", DataKind::Empty, Id::NONE);
        let b = doc.add_object("B", DataKind::Empty, Id::NONE);
        doc.objects.get_mut(a).unwrap().location = Vec3::new(1.0, 0.0, 0.0);
        let ob = doc.objects.get_mut(b).unwrap();
        ob.location = Vec3::new(0.0, 2.0, 0.0);
        ob.parent = a;
        let p = doc.object_matrix(b).transform_point(Vec3::ZERO);
        assert!(p.approx_eq(Vec3::new(1.0, 2.0, 0.0), 1e-12));
    }
}
