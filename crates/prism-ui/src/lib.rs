//! Prism UI. Phase 1 ships only the input event vocabulary the app translates
//! into; Phase 2 adds the screen/area/region tree, widgets, theme and the
//! props-driven panels (D016, D017).

pub mod event;

pub use event::{Event, Key, Modifiers, MouseButton, WheelDelta};
