//! Editor types an area can host. Phase 2 ships three: a viewport
//! placeholder, the widget gallery and Preferences. Phase 4 adds the outliner
//! and properties; Phase 5 makes the viewport real.

pub mod gallery;
pub mod prefs;
pub mod viewport;

pub use gallery::GalleryState;
pub use prefs::Prefs;

use crate::ui::Ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditorKind {
    Viewport,
    Gallery,
    Preferences,
    Empty,
}

impl EditorKind {
    pub const ALL: &'static [EditorKind] = &[EditorKind::Viewport, EditorKind::Gallery, EditorKind::Preferences, EditorKind::Empty];

    pub fn label(self) -> &'static str {
        match self {
            EditorKind::Viewport => "3D Viewport",
            EditorKind::Gallery => "Widget Gallery",
            EditorKind::Preferences => "Preferences",
            EditorKind::Empty => "Empty",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// Mutable state editors draw from and into.
pub struct EditorCtx<'a> {
    pub prefs: &'a mut Prefs,
    pub gallery: &'a mut GalleryState,
}

/// Draw the body of an editor. Returns `true` if it changed something that
/// affects other areas (theme, scale).
pub fn draw_editor(kind: EditorKind, ui: &mut Ui, ctx: &mut EditorCtx) -> bool {
    match kind {
        EditorKind::Viewport => {
            viewport::draw(ui);
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
