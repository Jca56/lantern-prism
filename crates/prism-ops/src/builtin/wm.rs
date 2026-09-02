//! Window-manager level: undo/redo, files, menus, the palette, quitting.

use prism_doc::Doc;
use prism_props::props;

use crate::context::{Ctx, Outcome, UiRequest};
use crate::operator::{OpError, OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct Empty {}
}

props! {
    pub struct PathProps {
        /// Empty means "ask" (save as / open) or "the document's path" (save).
        pub path: String = String::new() => { id: 1, subtype: FilePath },
    }
}

props! {
    pub struct MenuProps {
        pub menu: String = String::new() => { id: 1 },
    }
}

pub struct Undo;
impl Operator for Undo {
    const ID: &'static str = "ed.undo";
    const LABEL: &'static str = "Undo";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::Undo);
        Ok(Outcome::Finished)
    }
}

pub struct Redo;
impl Operator for Redo {
    const ID: &'static str = "ed.redo";
    const LABEL: &'static str = "Redo";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::Redo);
        Ok(Outcome::Finished)
    }
}

pub struct Save;
impl Operator for Save {
    const ID: &'static str = "wm.save";
    const LABEL: &'static str = "Save";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        let path = if p.path.is_empty() {
            match &ctx.doc.path {
                Some(existing) => existing.clone(),
                None => {
                    ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: true, suggest: None });
                    return Ok(Outcome::Cancelled);
                }
            }
        } else {
            std::path::PathBuf::from(&p.path)
        };
        prism_doc::save_file(ctx.doc, &path)?;
        ctx.doc.path = Some(path.clone());
        ctx.report(format!("Saved {}", path.display()));
        Ok(Outcome::Finished)
    }
}

pub struct SaveAs;
impl Operator for SaveAs {
    const ID: &'static str = "wm.save_as";
    const LABEL: &'static str = "Save As…";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: true, suggest: None });
            return Ok(Outcome::Cancelled);
        }
        Save::exec(ctx, p)
    }
}

pub struct Open;
impl Operator for Open {
    const ID: &'static str = "wm.open";
    const LABEL: &'static str = "Open…";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = PathProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &PathProps) -> OpResult<Outcome> {
        if p.path.is_empty() {
            ctx.request(UiRequest::PathDialog { op: Self::ID.into(), save: false, suggest: None });
            return Ok(Outcome::Cancelled);
        }
        let path = std::path::Path::new(&p.path);
        let doc = prism_doc::load_file(path)?;
        *ctx.doc = doc;
        ctx.request(UiRequest::HistoryClear);
        ctx.report(format!("Opened {}", path.display()));
        Ok(Outcome::Finished)
    }
}

pub struct New;
impl Operator for New {
    const ID: &'static str = "wm.new";
    const LABEL: &'static str = "New";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        *ctx.doc = Doc::starter();
        ctx.request(UiRequest::HistoryClear);
        Ok(Outcome::Finished)
    }
}

pub struct Quit;
impl Operator for Quit {
    const ID: &'static str = "wm.quit";
    const LABEL: &'static str = "Quit";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::Quit);
        Ok(Outcome::Finished)
    }
}

pub struct CallMenu;
impl Operator for CallMenu {
    const ID: &'static str = "wm.call_menu";
    const LABEL: &'static str = "Open Menu";
    const FLAGS: OpFlags = OpFlags::NONE;
    type Props = MenuProps;
    type Modal = ();
    fn exec(ctx: &mut Ctx, p: &MenuProps) -> OpResult<Outcome> {
        if p.menu.is_empty() {
            return Err(OpError::Failed("no menu named".into()));
        }
        ctx.request(UiRequest::Menu(p.menu.clone()));
        Ok(Outcome::Finished)
    }
}

pub struct Palette;
impl Operator for Palette {
    const ID: &'static str = "wm.palette";
    const LABEL: &'static str = "Command Palette";
    const FLAGS: OpFlags = OpFlags::REGISTER;
    type Props = Empty;
    type Modal = ();
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        ctx.request(UiRequest::Palette);
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<Undo>();
    r.register::<Redo>();
    r.register::<Save>();
    r.register::<SaveAs>();
    r.register::<Open>();
    r.register::<New>();
    r.register::<Quit>();
    r.register::<CallMenu>();
    r.register::<Palette>();
}
