//! Prism. See `docs/ARCHITECTURE.md`.

mod app;
mod demo;
mod translate;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    prism_core::log::init();
    let event_loop = EventLoop::new().expect("create event loop");
    // Redraw on demand: an idle editor sits at 0% CPU/GPU.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = app::App::new();
    if let Err(e) = event_loop.run_app(&mut app) {
        prism_core::log_error!("event loop: {e}");
    }
}
