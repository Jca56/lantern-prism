//! The operators that ship with Prism.

pub mod mesh;
pub mod object;
pub mod select;
pub mod view3d;
pub mod wm;

use crate::registry::Registry;

pub fn register_all(r: &mut Registry) {
    wm::register(r);
    object::register(r);
    mesh::register(r);
    view3d::register(r);
}
