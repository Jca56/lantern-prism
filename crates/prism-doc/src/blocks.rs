//! The datablocks. Each is a `props!` struct with an `id` and a `name`, so
//! panels, files and undo all come from one description (D006).

use prism_core::Id;
use prism_math::{Color, Mat4, Quat, Transform, Vec3};
use prism_mesh::{EdgeH, FaceH, Mesh, VertH};
use prism_props::props;

use crate::modifiers::Modifier;

props! {
    pub enum SelectMode {
        Vertex = 0,
        Edge = 1,
        Face = 2,
    }
}

props! {
    /// Per-scene tool state.
    pub struct ToolSettings {
        pub select_mode: SelectMode = SelectMode::Vertex => { id: 1, label: "Select Mode" },
        pub merge_distance: f64 = 0.0001 => { id: 2, hard: 0.0.., soft: 0.0..=1.0, subtype: Distance },
        /// Mirror editing: when moving, the side of the selection on the far
        /// side of the pivot gets the reflected movement, so opposite faces
        /// move apart or together instead of sliding the same way.
        pub mirror: bool = false => { id: 3 },
    }
}

props! {
    /// A scene: a root collection of objects plus what is active in it.
    pub struct Scene {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Scene".into() => { id: 2 },
        /// The root collection.
        pub collection: Id = Id::NONE => { id: 3, flags: HIDDEN },
        pub active_object: Id = Id::NONE => { id: 4, flags: HIDDEN },
        pub camera: Id = Id::NONE => { id: 5, flags: HIDDEN },
        pub tool: ToolSettings = ToolSettings::default() => { id: 6, label: "Tool Settings" },
    }
}

props! {
    /// A group of objects and child collections.
    pub struct Collection {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Collection".into() => { id: 2 },
        pub objects: Vec<Id> = Vec::new() => { id: 3, flags: HIDDEN },
        pub children: Vec<Id> = Vec::new() => { id: 4, flags: HIDDEN },
        pub visible: bool = true => { id: 5 },
    }
}

props! {
    pub enum DataKind {
        Empty = 0,
        Mesh = 1,
        Camera = 2,
        Light = 3,
    }
}

props! {
    pub enum ObjectMode {
        Object = 0,
        Edit = 1,
    }
}

props! {
    /// A thing in the scene: a transform plus a reference to its data.
    pub struct Object {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Object".into() => { id: 2 },
        pub kind: DataKind = DataKind::Empty => { id: 3, flags: READ_ONLY },
        /// The mesh / camera / light block this object shows.
        pub data: Id = Id::NONE => { id: 4, flags: HIDDEN },
        pub location: Vec3 = Vec3::ZERO => { id: 5, subtype: Translation, flags: ANIMATABLE },
        /// XYZ Euler, radians in memory.
        pub rotation: Vec3 = Vec3::ZERO => { id: 6, subtype: Euler, flags: ANIMATABLE },
        pub scale: Vec3 = Vec3::ONE => { id: 7, subtype: Scale, flags: ANIMATABLE },
        pub parent: Id = Id::NONE => { id: 8, flags: HIDDEN },
        pub visible: bool = true => { id: 9 },
        pub selected: bool = false => { id: 10, flags: HIDDEN },
        pub mode: ObjectMode = ObjectMode::Object => { id: 11, flags: READ_ONLY },
    }
}

impl Object {
    pub fn transform(&self) -> Transform {
        Transform::new(self.location, Quat::from_euler_xyz(self.rotation.x, self.rotation.y, self.rotation.z), self.scale)
    }

    /// Local matrix (parent not applied).
    pub fn matrix_local(&self) -> Mat4 {
        self.transform().to_mat4()
    }
}

props! {
    /// Reflected part of a mesh block; the geometry itself is not a property.
    pub struct MeshProps {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Mesh".into() => { id: 2 },
        pub materials: Vec<Id> = Vec::new() => { id: 3, flags: HIDDEN },
    }
}

/// A mesh element reference, for the active element and selection history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elem {
    Vert(VertH),
    Edge(EdgeH),
    Face(FaceH),
}

/// Edit-mode state that lives with the mesh (and therefore undoes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditState {
    pub active: Option<Elem>,
    /// Selection order, oldest first (bridge, connect and friends need it).
    pub history: Vec<Elem>,
}

/// Mesh datablock: reflected header plus the kernel mesh.
#[derive(Clone, Debug)]
pub struct MeshBlock {
    pub props: MeshProps,
    pub mesh: Mesh,
    pub edit: EditState,
    /// Non-destructive stack applied for display (D029).
    pub modifiers: Vec<Modifier>,
    /// Bumped whenever `modifiers` changes, so the evaluated mesh cache knows.
    pub modifiers_version: u64,
}

impl MeshBlock {
    pub fn new(name: &str, mesh: Mesh) -> Self {
        Self { props: MeshProps { name: name.to_owned(), ..MeshProps::default() }, mesh, edit: EditState::default(), modifiers: Vec::new(), modifiers_version: 0 }
    }

    /// The stack changed: the evaluated mesh must be rebuilt.
    pub fn bump_modifiers(&mut self) {
        self.modifiers_version += 1;
    }

    pub fn id(&self) -> Id {
        self.props.id
    }

    pub fn name(&self) -> &str {
        &self.props.name
    }
}

props! {
    pub struct Material {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Material".into() => { id: 2 },
        pub color: Color = Color::rgb(0.8, 0.8, 0.8) => { id: 3, label: "Base Color" },
        pub roughness: f64 = 0.5 => { id: 4, hard: 0.0..=1.0, subtype: Factor },
        pub metallic: f64 = 0.0 => { id: 5, hard: 0.0..=1.0, subtype: Factor },
    }
}

props! {
    pub enum Projection {
        Perspective = 0,
        Orthographic = 1,
    }
}

props! {
    pub struct Camera {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Camera".into() => { id: 2 },
        pub projection: Projection = Projection::Perspective => { id: 3 },
        /// Vertical field of view.
        pub fov: f64 = 0.8726646259971648 => { id: 4, hard: 0.01..=3.1, subtype: Angle, label: "Field of View" },
        pub ortho_scale: f64 = 6.0 => { id: 5, hard: 0.001.., subtype: Distance },
        pub near: f64 = 0.1 => { id: 6, hard: 0.0001.., subtype: Distance, label: "Clip Start" },
    }
}

props! {
    pub enum LightKind {
        Point = 0,
        Sun = 1,
        Spot = 2,
    }
}

props! {
    pub struct Light {
        pub id: Id = Id::NONE => { id: 1, flags: HIDDEN | READ_ONLY },
        pub name: String = "Light".into() => { id: 2 },
        pub kind: LightKind = LightKind::Point => { id: 3 },
        pub color: Color = Color::WHITE => { id: 4 },
        pub power: f64 = 1000.0 => { id: 5, hard: 0.0.., soft: 0.0..=10000.0 },
    }
}
