//! The widget set. Every widget is a method on [`crate::Ui`] that allocates
//! a rect, hit-tests it, mutates the caller's value and draws itself.

mod basic;
mod dropdown;
mod scroll;
mod slider;
mod text_field;

pub use text_field::TextResponse;
