//! Prism operators: every user action is an operator (D007). Menus, hotkeys,
//! gizmos and the command palette are four doors into one room.
//!
//! - [`input`]: the event vocabulary shared by the whole editor.
//! - [`Operator`]: the trait; [`Registry`] type-erases implementations.
//! - [`Executor`]: runs operators transactionally, records undo steps,
//!   drives modal operators, and re-runs the last one with adjusted props.
//! - [`keymap`]: data-driven key bindings resolved by context.
//! - [`builtin`]: the operators that ship.

pub mod builtin;
pub mod context;
pub mod executor;
pub mod input;
pub mod keymap;
pub mod operator;
pub mod registry;

pub use context::{Ctx, Flow, Outcome, UiRequest};
pub use executor::{Executor, RunningModal};
pub use keymap::{KeyConfig, KeyItem, KeyMap, Trigger};
pub use operator::{OpError, OpFlags, OpResult, Operator};
pub use registry::{OpInfo, Registry};
