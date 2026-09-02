//! The operators that ship with Prism.

pub mod extrude;
pub mod inset;
pub mod io;
pub mod mesh;
pub mod object;
pub mod select;
pub mod transform;
pub mod view3d;
pub mod wm;

use crate::registry::Registry;

pub fn register_all(r: &mut Registry) {
    wm::register(r);
    io::register(r);
    object::register(r);
    mesh::register(r);
    extrude::register(r);
    inset::register(r);
    transform::register(r);
    view3d::register(r);
}
