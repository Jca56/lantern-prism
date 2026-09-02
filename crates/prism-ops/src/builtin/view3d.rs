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

pub fn register(r: &mut Registry) {
    r.register::<FrameAll>();
    r.register::<FrameSelected>();
}
