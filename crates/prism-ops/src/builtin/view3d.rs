//! Viewport operators. The camera lives in the UI, so these only ask.

use prism_props::props;

use crate::context::{Ctx, Outcome, UiRequest};
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct Empty {}
}

pub struct FrameAll;
impl Operator for FrameAll {
    const ID: &'static str = "view3d.frame_all";
    const LABEL: &'static str = "Frame All";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::ViewFrame { selected: false });
        Ok(Outcome::Finished)
    }
}

pub struct FrameSelected;
impl Operator for FrameSelected {
    const ID: &'static str = "view3d.frame_selected";
    const LABEL: &'static str = "Frame Selected";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::ViewFrame { selected: true });
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct ShadingProps {
        pub wire: bool = false => { id: 1 },
    }
}

pub struct Shading;
impl Operator for Shading {
    const ID: &'static str = "view3d.shading";
    const LABEL: &'static str = "Viewport Shading";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = ShadingProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &ShadingProps) -> OpResult<Outcome> {
        ctx.request(UiRequest::ViewShading { wire: p.wire });
        Ok(Outcome::Finished)
    }
}

pub struct ToggleGrid;
impl Operator for ToggleGrid {
    const ID: &'static str = "view3d.toggle_grid";
    const LABEL: &'static str = "Toggle Grid";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::ViewToggleGrid);
        Ok(Outcome::Finished)
    }
}

props! {
    pub enum GizmoKind {
        Move = 0,
        Rotate = 1,
        Scale = 2,
    }
}

props! {
    pub struct GizmoProps {
        pub mode: GizmoKind = GizmoKind::Move => { id: 1 },
    }
}

/// Show one transform gizmo in the active viewport (D024).
pub struct Gizmo;
impl Operator for Gizmo {
    const ID: &'static str = "view3d.gizmo";
    const LABEL: &'static str = "Gizmo";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = GizmoProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &GizmoProps) -> OpResult<Outcome> {
        ctx.request(UiRequest::GizmoSet(p.mode as usize));
        Ok(Outcome::Finished)
    }
}

/// Next gizmo: Move → Rotate → Scale → Move. Bound to R.
pub struct GizmoCycle;
impl Operator for GizmoCycle {
    const ID: &'static str = "view3d.gizmo_cycle";
    const LABEL: &'static str = "Cycle Gizmo";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::GizmoCycle);
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<FrameAll>();
    r.register::<FrameSelected>();
    r.register::<Shading>();
    r.register::<ToggleGrid>();
    r.register::<Gizmo>();
    r.register::<GizmoCycle>();
}
