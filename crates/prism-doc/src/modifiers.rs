//! The modifier stack (Phase 8, D029): non-destructive operations a mesh goes
//! through when it is displayed. The data lives here with the mesh block and
//! saves with it; `prism-eval` does the work.

use prism_props::{Reflect, props};

props! {
    pub enum ModifierKind {
        Mirror = 0,
        Subsurf = 1 => { label: "Subdivision Surface" },
    }
}

props! {
    /// Mirror across the object's local axes, welding the seam.
    pub struct MirrorProps {
        pub x: bool = true => { id: 1 },
        pub y: bool = false => { id: 2 },
        pub z: bool = false => { id: 3 },
        /// Weld vertices on the mirror plane to their reflections.
        pub merge: bool = true => { id: 4 },
        pub merge_distance: f64 = 0.001 => { id: 5, soft: 0.0..=1.0, subtype: Distance },
    }
}

props! {
    /// Catmull-Clark subdivision. `smooth` off only splits faces, keeping the shape.
    pub struct SubsurfProps {
        pub levels: i64 = 1 => { id: 1, hard: 0..=5, soft: 0..=3 },
        pub smooth: bool = true => { id: 2 },
    }
}

#[derive(Clone, Debug)]
pub enum Modifier {
    Mirror(MirrorProps),
    Subsurf(SubsurfProps),
}

impl Modifier {
    pub fn new(kind: ModifierKind) -> Self {
        match kind {
            ModifierKind::Mirror => Modifier::Mirror(MirrorProps::default()),
            ModifierKind::Subsurf => Modifier::Subsurf(SubsurfProps::default()),
        }
    }

    pub fn kind(&self) -> ModifierKind {
        match self {
            Modifier::Mirror(_) => ModifierKind::Mirror,
            Modifier::Subsurf(_) => ModifierKind::Subsurf,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Modifier::Mirror(_) => "Mirror",
            Modifier::Subsurf(_) => "Subdivision Surface",
        }
    }

    pub fn props(&self) -> &dyn Reflect {
        match self {
            Modifier::Mirror(p) => p,
            Modifier::Subsurf(p) => p,
        }
    }

    pub fn props_mut(&mut self) -> &mut dyn Reflect {
        match self {
            Modifier::Mirror(p) => p,
            Modifier::Subsurf(p) => p,
        }
    }

    /// Stable id in files.
    pub fn kind_id(&self) -> u32 {
        self.kind() as u32
    }
}
