//! Prism UI (D016, D017).
//!
//! Two layers:
//! - **Retained**: [`Screen`] tiles the window into areas, each hosting one
//!   editor. It is plain data (saved with the file later) and it changes only
//!   when the user splits, joins or drags a separator.
//! - **Immediate**: inside every region the widgets are re-declared on each
//!   rebuild through a [`Ui`] context, which lays them out, routes input and
//!   emits draw commands. Per-widget persistent state (caret, scroll offset,
//!   open popup) lives in [`UiState`], keyed by stable [`WidgetId`]s.
//!
//! A rebuild happens whenever an input event arrives; an idle editor rebuilds
//! nothing and draws nothing.

pub mod editors;
pub mod event;
pub mod id;
pub mod panel;
pub mod popups;
pub mod screen;
pub mod shell;
pub mod state;
pub mod theme;
pub mod titlebar;
pub mod ui;
pub mod widgets;

pub use event::{Event, Key, Modifiers, MouseButton, WheelDelta};
pub use id::WidgetId;
pub use screen::{Area, AreaId, Axis, Screen};
pub use shell::{Shell, ShellOutput, WindowState};
pub use titlebar::{ResizeEdge, WindowCommand};
pub use state::{CursorIcon, UiState};
pub use theme::{Metrics, Theme};
pub use ui::{Response, Sense, Ui};
