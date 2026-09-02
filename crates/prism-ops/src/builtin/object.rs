//! Object-mode operators.

use prism_core::Id;
use prism_doc::{DataKind, ObjectMode};
use prism_math::Vec3;
use prism_mesh::primitives;
use prism_mesh::tables::F_SMOOTH;
use prism_props::props;

use crate::context::{Ctx, Outcome};
use crate::operator::{OpError, OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub enum PrimKind {
        Plane = 0,
        Cube = 1,
        UvSphere = 2 => { label: "UV Sphere" },
        Cylinder = 3,
        Grid = 4,
        Circle = 5,
    }
}

props! {
    /// Add a primitive mesh object at a location.
    pub struct AddPrimitiveProps {
        pub kind: PrimKind = PrimKind::Cube => { id: 1 },
        pub size: f64 = 2.0 => { id: 2, hard: 0.001.., soft: 0.1..=10.0, subtype: Distance },
        pub segments: i64 = 16 => { id: 3, hard: 3..=256 },
        pub rings: i64 = 8 => { id: 4, hard: 2..=128 },
        pub location: Vec3 = Vec3::ZERO => { id: 5, subtype: Translation },
    }
}

pub struct AddPrimitive;
impl Operator for AddPrimitive {
    const ID: &'static str = "object.add_primitive";
    const LABEL: &'static str = "Add Primitive";
    type Props = AddPrimitiveProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &AddPrimitiveProps) -> OpResult<Outcome> {
        let (name, mesh) = match p.kind {
            PrimKind::Plane => ("Plane", primitives::plane(p.size)),
            PrimKind::Cube => ("Cube", primitives::cube(p.size)),
            PrimKind::UvSphere => ("Sphere", primitives::uv_sphere(p.size * 0.5, p.segments as usize, p.rings as usize)),
            PrimKind::Cylinder => ("Cylinder", primitives::cylinder(p.size * 0.5, p.size, p.segments as usize, true)),
            PrimKind::Grid => ("Grid", primitives::grid(p.size, p.size, p.segments as usize, p.segments as usize)),
            PrimKind::Circle => ("Circle", primitives::circle(p.size * 0.5, p.segments as usize, false)),
        };
        for id in ctx.doc.scene_objects() {
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.selected = false;
            }
        }
        let mesh_id = ctx.doc.add_mesh(name, mesh);
        let obj = ctx.doc.add_object(name, DataKind::Mesh, mesh_id);
        if let Some(o) = ctx.doc.objects.get_mut(obj) {
            o.location = p.location;
            o.selected = true;
        }
        ctx.report(format!("Added {name}"));
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct Empty {}
}

pub struct Delete;
impl Operator for Delete {
    const ID: &'static str = "object.delete";
    const LABEL: &'static str = "Delete";
    type Props = Empty;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        !ctx.doc.selected_objects().is_empty()
    }
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        let sel = ctx.doc.selected_objects();
        if sel.is_empty() {
            return Ok(Outcome::Cancelled);
        }
        for id in &sel {
            ctx.doc.remove_object(*id);
        }
        ctx.doc.purge_orphans();
        ctx.report(format!("Deleted {} object(s)", sel.len()));
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct SelectProps {
        pub id: Id = Id::NONE => { id: 1 },
        pub extend: bool = false => { id: 2 },
        pub toggle: bool = false => { id: 3 },
    }
}

pub struct Select;
impl Operator for Select {
    const ID: &'static str = "object.select";
    const LABEL: &'static str = "Select Object";
    const FLAGS: OpFlags = OpFlags::UNDO;
    type Props = SelectProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &SelectProps) -> OpResult<Outcome> {
        if !ctx.doc.objects.contains(p.id) {
            return Err(OpError::Failed("no such object".into()));
        }
        if !p.extend && !p.toggle {
            for id in ctx.doc.scene_objects() {
                if let Some(o) = ctx.doc.objects.get_mut(id) {
                    o.selected = false;
                }
            }
        }
        let now = {
            let o = ctx.doc.objects.get_mut(p.id).expect("checked");
            o.selected = if p.toggle { !o.selected } else { true };
            o.selected
        };
        if let Some(s) = ctx.doc.scene_mut() {
            if now {
                s.active_object = p.id;
            } else if s.active_object == p.id {
                s.active_object = Id::NONE;
            }
        }
        Ok(Outcome::Finished)
    }
}

props! {
    pub enum SelectAction {
        Toggle = 0,
        Select = 1,
        Deselect = 2,
        Invert = 3,
    }
}

props! {
    pub struct SelectAllProps {
        pub action: SelectAction = SelectAction::Toggle => { id: 1 },
    }
}

pub struct SelectAll;
impl Operator for SelectAll {
    const ID: &'static str = "object.select_all";
    const LABEL: &'static str = "Select All";
    type Props = SelectAllProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &SelectAllProps) -> OpResult<Outcome> {
        let ids = ctx.doc.scene_objects();
        let any = ids.iter().any(|&id| ctx.doc.objects.get(id).is_some_and(|o| o.selected));
        for id in ids {
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.selected = match p.action {
                    SelectAction::Toggle => !any,
                    SelectAction::Select => true,
                    SelectAction::Deselect => false,
                    SelectAction::Invert => !o.selected,
                };
            }
        }
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct DuplicateProps {
        /// Share the mesh data instead of copying it.
        pub linked: bool = false => { id: 1 },
    }
}

pub struct Duplicate;
impl Operator for Duplicate {
    const ID: &'static str = "object.duplicate";
    const LABEL: &'static str = "Duplicate";
    type Props = DuplicateProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        !ctx.doc.selected_objects().is_empty()
    }
    fn exec(ctx: &mut Ctx, p: &DuplicateProps) -> OpResult<Outcome> {
        let sel = ctx.doc.selected_objects();
        if sel.is_empty() {
            return Ok(Outcome::Cancelled);
        }
        let mut new_ids = Vec::new();
        for id in sel {
            let Some(src) = ctx.doc.objects.get(id).cloned() else {
                continue;
            };
            let data = if src.kind == DataKind::Mesh && !p.linked {
                match ctx.doc.meshes.get(src.data).cloned() {
                    Some(block) => ctx.doc.add_mesh(&block.props.name, block.mesh),
                    None => src.data,
                }
            } else {
                src.data
            };
            let nid = ctx.doc.add_object(&format!("{}.001", src.name), src.kind, data);
            if let Some(o) = ctx.doc.objects.get_mut(nid) {
                o.location = src.location;
                o.rotation = src.rotation;
                o.scale = src.scale;
                o.parent = src.parent;
                o.selected = true;
            }
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.selected = false;
            }
            new_ids.push(nid);
        }
        ctx.report(format!("Duplicated {} object(s)", new_ids.len()));
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct TranslateProps {
        pub delta: Vec3 = Vec3::ZERO => { id: 1, subtype: Translation },
    }
}

pub struct Translate;
impl Operator for Translate {
    const ID: &'static str = "object.translate";
    const LABEL: &'static str = "Move";
    type Props = TranslateProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        !ctx.doc.selected_objects().is_empty()
    }
    fn exec(ctx: &mut Ctx, p: &TranslateProps) -> OpResult<Outcome> {
        for id in ctx.doc.selected_objects() {
            if let Some(o) = ctx.doc.objects.get_mut(id) {
                o.location += p.delta;
            }
        }
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct RenameProps {
        pub name: String = String::new() => { id: 1 },
    }
}

pub struct Rename;
impl Operator for Rename {
    const ID: &'static str = "object.rename";
    const LABEL: &'static str = "Rename";
    type Props = RenameProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        ctx.doc.active_object().is_some()
    }
    fn exec(ctx: &mut Ctx, p: &RenameProps) -> OpResult<Outcome> {
        if p.name.trim().is_empty() {
            return Ok(Outcome::Cancelled);
        }
        let id = ctx.doc.active_object_id();
        match ctx.doc.objects.get_mut(id) {
            Some(o) => {
                o.name = p.name.trim().to_owned();
                Ok(Outcome::Finished)
            }
            None => Ok(Outcome::Cancelled),
        }
    }
}

props! {
    pub struct ModeSetProps {
        pub mode: ObjectMode = ObjectMode::Edit => { id: 1 },
        /// Leave the mode again if already in it.
        pub toggle: bool = false => { id: 2 },
    }
}

pub struct ModeSet;
impl Operator for ModeSet {
    const ID: &'static str = "object.mode_set";
    const LABEL: &'static str = "Set Mode";
    type Props = ModeSetProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        ctx.doc.active_object().is_some_and(|o| o.kind == DataKind::Mesh)
    }
    fn exec(ctx: &mut Ctx, p: &ModeSetProps) -> OpResult<Outcome> {
        let id = ctx.doc.active_object_id();
        let Some(o) = ctx.doc.objects.get_mut(id) else {
            return Ok(Outcome::Cancelled);
        };
        let target = if p.toggle && o.mode == p.mode {
            match p.mode {
                ObjectMode::Edit => ObjectMode::Object,
                ObjectMode::Object => ObjectMode::Edit,
            }
        } else {
            p.mode
        };
        if o.mode == target {
            return Ok(Outcome::Cancelled);
        }
        o.mode = target;
        ctx.report(format!("{} mode", target.label()));
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct ShadeProps {
        pub smooth: bool = true => { id: 1 },
    }
}

pub struct Shade;
impl Operator for Shade {
    const ID: &'static str = "object.shade";
    const LABEL: &'static str = "Shade Smooth / Flat";
    type Props = ShadeProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        ctx.doc.selected_objects().iter().any(|&id| ctx.doc.object_mesh(id).is_some())
    }
    fn exec(ctx: &mut Ctx, p: &ShadeProps) -> OpResult<Outcome> {
        for id in ctx.doc.selected_objects() {
            if let Some(block) = ctx.doc.object_mesh_mut(id) {
                let faces: Vec<_> = block.mesh.faces().collect();
                let smooth = block.mesh.face_attrs_mut().bools_mut(F_SMOOTH);
                for f in faces {
                    smooth.set(f.idx(), p.smooth);
                }
            }
        }
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<AddPrimitive>();
    r.register::<Delete>();
    r.register::<Select>();
    r.register::<SelectAll>();
    r.register::<Duplicate>();
    r.register::<Translate>();
    r.register::<Rename>();
    r.register::<ModeSet>();
    r.register::<Shade>();
}
