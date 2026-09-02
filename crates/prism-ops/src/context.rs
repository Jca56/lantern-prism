//! What an operator sees: the document and the editing situation. Never the
//! UI directly; requests for it go through [`UiRequest`].

use prism_doc::Doc;
use prism_math::{Rect, Vec2};

use crate::input::Modifiers;

/// Something the operator wants the UI layer to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    Undo,
    Redo,
    /// The document was replaced wholesale (new / open): forget history.
    HistoryClear,
    /// Open a named menu at the pointer.
    Menu(String),
    /// Open the command palette.
    Palette,
    /// Ask for a path, then run `op` with its `path` property set.
    PathDialog { op: String, save: bool },
    /// Frame the scene (or the selection) in the active 3D viewport.
    ViewFrame { selected: bool },
    Quit,
}

pub struct Ctx<'a> {
    pub doc: &'a mut Doc,
    /// Pointer in physical pixels (window space).
    pub pointer: Vec2,
    /// The region the event came from.
    pub region: Rect,
    pub mods: Modifiers,
    pub requests: Vec<UiRequest>,
    /// One line for the status area.
    pub report: Option<String>,
}

impl<'a> Ctx<'a> {
    pub fn new(doc: &'a mut Doc) -> Self {
        Self { doc, pointer: Vec2::ZERO, region: Rect::ZERO, mods: Modifiers::NONE, requests: Vec::new(), report: None }
    }

    pub fn report(&mut self, msg: impl Into<String>) {
        self.report = Some(msg.into());
    }

    pub fn request(&mut self, r: UiRequest) {
        self.requests.push(r);
    }
}

/// Result of `exec`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The document changed (or the op did its job): record undo.
    Finished,
    /// Nothing happened; nothing to record.
    Cancelled,
    /// The op did not want this; let others try.
    PassThrough,
}

/// Result of `invoke` / `modal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Running,
    Finished,
    Cancelled,
    PassThrough,
}

impl From<Outcome> for Flow {
    fn from(o: Outcome) -> Flow {
        match o {
            Outcome::Finished => Flow::Finished,
            Outcome::Cancelled => Flow::Cancelled,
            Outcome::PassThrough => Flow::PassThrough,
        }
    }
}
