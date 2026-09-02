//! Editor types an area can host.

pub mod gallery;
pub mod outliner;
pub mod prefs;
pub mod properties;
pub mod viewport;

pub use gallery::GalleryState;
pub use outliner::OutlinerState;
pub use prefs::Prefs;
pub use properties::PropertiesState;

use prism_doc::{Doc, UndoStep};
use prism_math::Vec2;
use prism_ops::{Ctx, Executor, OpResult, Outcome, UiRequest};
use prism_props::Value;
use prism_viewport::{PickRequest, ViewportRequest, ViewportState};

use crate::ui::Ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditorKind {
    Viewport,
    Outliner,
    Properties,
    Preferences,
    Gallery,
    Empty,
}

impl EditorKind {
    pub const ALL: &'static [EditorKind] = &[
        EditorKind::Viewport,
        EditorKind::Outliner,
        EditorKind::Properties,
        EditorKind::Preferences,
        EditorKind::Gallery,
        EditorKind::Empty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EditorKind::Viewport => "3D Viewport",
            EditorKind::Outliner => "Outliner",
            EditorKind::Properties => "Properties",
            EditorKind::Preferences => "Preferences",
            EditorKind::Gallery => "Widget Gallery",
            EditorKind::Empty => "Empty",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Keymap context name for this editor.
    pub fn keymap_context(self) -> &'static str {
        match self {
            EditorKind::Outliner => prism_ops::keymap::CTX_OUTLINER,
            _ => "editor",
        }
    }
}

/// Everything an editor may read and change.
pub struct EditorCtx<'a> {
    pub doc: &'a mut Doc,
    pub exec: &'a mut Executor,
    pub prefs: &'a mut Prefs,
    pub gallery: &'a mut GalleryState,
    pub outliner: &'a mut OutlinerState,
    pub properties: &'a mut PropertiesState,
    /// Requests for the shell (menus, palette, quit) gathered this frame.
    pub requests: &'a mut Vec<UiRequest>,
    pub pointer: Vec2,
    /// Which area is being drawn, and its viewport state.
    pub area: usize,
    pub viewport: &'a mut ViewportState,
    /// 3D viewports to render this frame, and clicks to resolve after.
    pub viewports: &'a mut Vec<ViewportRequest>,
    pub picks: &'a mut Vec<PickRequest>,
    /// Set by an editor to open a context menu at the pointer.
    pub context_menu: &'a mut Option<crate::context_menu::MenuContext>,
}

/// Run one operator from the UI, collecting its requests.
pub fn run_op(doc: &mut Doc, exec: &mut Executor, pointer: Vec2, op: &str, overrides: &[(String, Value)], requests: &mut Vec<UiRequest>) -> OpResult<Outcome> {
    let mut ctx = Ctx::new(doc);
    ctx.pointer = pointer;
    let ov: Vec<(&str, Value)> = overrides.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let r = exec.run_with(op, &ov, &mut ctx);
    requests.append(&mut exec.requests);
    if r.is_ok() && (op == "wm.save" || op == "wm.save_as") {
        exec.mark_saved();
    }
    r
}

impl EditorCtx<'_> {
    /// Run an operator with overrides; requests are collected for the shell.
    pub fn run(&mut self, op: &str, overrides: &[(&str, Value)]) -> OpResult<Outcome> {
        let mut ctx = Ctx::new(self.doc);
        ctx.pointer = self.pointer;
        let r = self.exec.run_with(op, overrides, &mut ctx);
        self.requests.append(&mut self.exec.requests);
        if r.is_ok() && (op == "wm.save" || op == "wm.save_as") {
            self.exec.mark_saved();
        }
        r
    }

    /// Record a direct property edit as an undo step. While the pointer is
    /// held (a drag), consecutive edits with the same label coalesce.
    pub fn record_edit(&mut self, before: Doc, label: &str, dragging: bool) {
        record_edit(self.exec, self.doc, before, label, dragging);
    }
}

/// Record a direct property edit as an undo step (see `EditorCtx::record_edit`).
pub fn record_edit(exec: &mut Executor, doc: &Doc, before: Doc, label: &str, dragging: bool) {
    if dragging
        && let Some(last) = exec.history.last_mut()
        && last.op_id == "ui.edit"
        && last.label == label
    {
        last.after = doc.clone();
        return;
    }
    exec.history.push(UndoStep { before, after: doc.clone(), label: label.to_owned(), op_id: "ui.edit".to_owned(), props: None });
}

/// Editor-specific controls in the area header, after the editor dropdown.
pub fn draw_editor_header(kind: EditorKind, ui: &mut Ui, ctx: &mut EditorCtx) {
    if kind == EditorKind::Viewport {
        viewport::header(ui, ctx);
    }
}

/// Draw the body of an editor. Returns `true` if it changed something that
/// affects other areas (theme, scale).
pub fn draw_editor(kind: EditorKind, ui: &mut Ui, ctx: &mut EditorCtx) -> bool {
    match kind {
        EditorKind::Viewport => {
            viewport::draw(ui, ctx);
            false
        }
        EditorKind::Outliner => {
            outliner::draw(ui, ctx);
            false
        }
        EditorKind::Properties => {
            properties::draw(ui, ctx);
            false
        }
        EditorKind::Gallery => {
            gallery::draw(ui, ctx.gallery);
            false
        }
        EditorKind::Preferences => prefs::draw(ui, ctx.prefs),
        EditorKind::Empty => false,
    }
}
