//! The modifier stack (Phase 8, D029): add, remove and apply modifiers on the
//! active object's mesh.

use prism_doc::{DataKind, MeshBlock, Modifier, ModifierKind, ObjectMode};
use prism_props::props;

use crate::context::{Ctx, Outcome};
use crate::operator::{OpError, OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct ModifierAddProps {
        pub kind: ModifierKind = ModifierKind::Mirror => { id: 1 },
    }
}

props! {
    pub struct ModifierIndexProps {
        /// Position in the stack, top first.
        pub index: i64 = 0 => { id: 1, hard: 0..=64 },
    }
}

fn has_mesh(ctx: &Ctx) -> bool {
    ctx.doc.active_object().is_some_and(|o| o.kind == DataKind::Mesh) && ctx.doc.object_mesh(ctx.doc.active_object_id()).is_some()
}

fn active_block<'c>(ctx: &'c mut Ctx<'_>) -> Option<&'c mut MeshBlock> {
    let id = ctx.doc.active_object_id();
    ctx.doc.object_mesh_mut(id)
}

pub struct ModifierAdd;
impl Operator for ModifierAdd {
    const ID: &'static str = "object.modifier_add";
    const LABEL: &'static str = "Add Modifier";
    const FLAGS: OpFlags = OpFlags::DEFAULT;
    type Props = ModifierAddProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        has_mesh(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &ModifierAddProps) -> OpResult<Outcome> {
        let Some(block) = active_block(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = Modifier::new(p.kind);
        let label = m.label();
        block.modifiers.push(m);
        block.bump_modifiers();
        ctx.report(format!("Added {label}"));
        Ok(Outcome::Finished)
    }
}

pub struct ModifierRemove;
impl Operator for ModifierRemove {
    const ID: &'static str = "object.modifier_remove";
    const LABEL: &'static str = "Remove Modifier";
    const FLAGS: OpFlags = OpFlags::UNDO;
    type Props = ModifierIndexProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        has_mesh(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &ModifierIndexProps) -> OpResult<Outcome> {
        let Some(block) = active_block(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let i = p.index.max(0) as usize;
        if i >= block.modifiers.len() {
            return Err(OpError::Failed("no such modifier".into()));
        }
        let m = block.modifiers.remove(i);
        block.bump_modifiers();
        ctx.report(format!("Removed {}", m.label()));
        Ok(Outcome::Finished)
    }
}

/// Bake the stack up to and including `index` into the mesh itself.
pub struct ModifierApply;
impl Operator for ModifierApply {
    const ID: &'static str = "object.modifier_apply";
    const LABEL: &'static str = "Apply Modifier";
    const FLAGS: OpFlags = OpFlags::UNDO;
    type Props = ModifierIndexProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        has_mesh(ctx) && ctx.doc.active_object().is_some_and(|o| o.mode == ObjectMode::Object)
    }
    fn exec(ctx: &mut Ctx, p: &ModifierIndexProps) -> OpResult<Outcome> {
        let Some(block) = active_block(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let n = p.index.max(0) as usize + 1;
        if n > block.modifiers.len() {
            return Err(OpError::Failed("no such modifier".into()));
        }
        let eval = prism_eval::apply_modifiers(&block.mesh, &block.modifiers[..n]);
        block.mesh = eval.mesh;
        block.edit = Default::default();
        block.modifiers.drain(..n);
        block.bump_modifiers();
        ctx.report(format!("Applied {n} modifier(s)"));
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<ModifierAdd>();
    r.register::<ModifierRemove>();
    r.register::<ModifierApply>();
}

#[cfg(test)]
mod tests {
    use crate::context::Ctx;
    use crate::executor::Executor;
    use prism_doc::Doc;
    use prism_props::Value;

    #[test]
    fn add_remove_apply_round_trip() {
        let mut doc = Doc::starter();
        let mut ex = Executor::with_builtins();
        let cube = doc.scene_objects()[0];
        let mesh = doc.objects.get(cube).unwrap().data;
        let steps = ex.history.len();
        ex.run_with("object.modifier_add", &[("kind", Value::Enum(0))], &mut Ctx::new(&mut doc)).unwrap();
        ex.run_with("object.modifier_add", &[("kind", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        let block = doc.meshes.get(mesh).unwrap();
        assert_eq!(block.modifiers.iter().map(|m| m.label()).collect::<Vec<_>>(), vec!["Mirror", "Subdivision Surface"]);
        assert_eq!(block.modifiers_version, 2);
        assert_eq!(ex.history.len(), steps + 2);

        ex.run_with("object.modifier_remove", &[("index", Value::I64(0))], &mut Ctx::new(&mut doc)).unwrap();
        assert_eq!(doc.meshes.get(mesh).unwrap().modifiers.len(), 1);
        assert!(ex.run_with("object.modifier_remove", &[("index", Value::I64(5))], &mut Ctx::new(&mut doc)).is_err());

        // Apply the subdivision: the base mesh becomes 24 quads, stack empty.
        ex.run_with("object.modifier_apply", &[("index", Value::I64(0))], &mut Ctx::new(&mut doc)).unwrap();
        let block = doc.meshes.get(mesh).unwrap();
        assert_eq!(block.mesh.face_count(), 24);
        assert!(block.modifiers.is_empty());
        ex.undo(&mut doc);
        let block = doc.meshes.get(mesh).unwrap();
        assert_eq!((block.mesh.face_count(), block.modifiers.len()), (6, 1), "undo restores geometry and stack together");
    }
}
