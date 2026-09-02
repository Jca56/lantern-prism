//! The winit application: window, GPU wiring, event translation, and the
//! rebuild → draw → present cycle. Nothing here knows what a widget is.

use std::sync::Arc;

use prism_core::{log_error, log_info, log_trace};
use prism_doc::Doc;
use prism_ops::Executor;
use prism_math::{Rect, Vec2};
use prism_render::wgpu;
use prism_render::{DrawList, Gpu, Pass2d, RenderGraph, SurfaceTarget, TexturePool};
use prism_text::TextEngine;
use prism_ui::{CursorIcon, Event, Modifiers, ResizeEdge, Shell, WindowCommand, WindowState};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// A popup closing or a value committing may ask for one more rebuild; this
/// caps how many happen back to back before we present.
const MAX_REBUILDS: usize = 4;

struct Gfx {
    window: Arc<Window>,
    gpu: Gpu,
    surface: SurfaceTarget,
    pass2d: Pass2d,
    pool: TexturePool,
    cursor: CursorIcon,
}

pub struct App {
    gfx: Option<Gfx>,
    text: TextEngine,
    draw: DrawList,
    shell: Shell,
    doc: Doc,
    exec: Executor,
    /// Events since the last rebuild, in order.
    events: Vec<Event>,
    mods: Modifiers,
    pointer: Vec2,
    scale: f64,
    /// Something happened; rebuild before the loop goes back to sleep.
    dirty: bool,
    focused: bool,
    quit: bool,
}

impl App {
    pub fn new() -> Self {
        let t = std::time::Instant::now();
        let text = TextEngine::new("Inter", "JetBrains Mono");
        log_info!("fonts: {} faces in {:.0} ms", text.face_count(), t.elapsed().as_secs_f64() * 1000.0);
        Self {
            gfx: None,
            text,
            draw: DrawList::new(),
            shell: Shell::new(),
            doc: Doc::starter(),
            exec: Executor::with_builtins(),
            events: Vec::new(),
            mods: Modifiers::NONE,
            pointer: Vec2::ZERO,
            scale: 1.0,
            dirty: true,
            focused: true,
            quit: false,
        }
    }

    fn init_gfx(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("Prism")
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(1600.0, 1000.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.scale = window.scale_factor();
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(Arc::clone(&window)).expect("create surface");
        let gpu = match Gpu::with_instance(instance, Some(&surface)) {
            Ok(g) => g,
            Err(e) => {
                log_error!("{e}");
                event_loop.exit();
                return;
            }
        };
        let surface = SurfaceTarget::new(&gpu, surface, size.width, size.height);
        let pass2d = Pass2d::new(&gpu, surface.format(), self.text.atlas());
        log_info!("window: {}x{} @ {:.2}x", size.width, size.height, self.scale);
        self.gfx = Some(Gfx { window, gpu, surface, pass2d, pool: TexturePool::new(), cursor: CursorIcon::Default });
    }

    /// Rebuild the UI from the pending events (possibly more than once),
    /// then draw and present.
    fn render(&mut self) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        let size = gfx.surface.size();
        let window_rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(size[0] as f64, size[1] as f64));
        let events = std::mem::take(&mut self.events);

        let ws = WindowState { maximized: gfx.window.is_maximized(), focused: self.focused };
        let mut out = None;
        let mut evs: &[Event] = &events;
        let mut command = None;
        for _ in 0..MAX_REBUILDS {
            self.draw.clear();
            let o = self.shell.frame(evs, window_rect, self.scale, ws, &mut self.doc, &mut self.exec, &mut self.text, &mut self.draw);
            let again = o.rebuild_again;
            command = command.or(o.window_command);
            if o.quit {
                self.quit = true;
            }
            out = Some(o);
            evs = &[];
            if !again {
                break;
            }
        }
        let out = out.expect("at least one rebuild");

        if out.cursor != gfx.cursor {
            gfx.cursor = out.cursor;
            gfx.window.set_cursor(cursor_icon(out.cursor));
        }
        match command {
            Some(WindowCommand::Drag) => {
                let _ = gfx.window.drag_window();
            }
            Some(WindowCommand::Minimize) => gfx.window.set_minimized(true),
            Some(WindowCommand::ToggleMaximize) => gfx.window.set_maximized(!gfx.window.is_maximized()),
            Some(WindowCommand::Close) => self.quit = true,
            Some(WindowCommand::Resize(edge)) => {
                let _ = gfx.window.drag_resize_window(resize_direction(edge));
            }
            None => {}
        }

        let Some(frame) = gfx.surface.acquire(&gfx.gpu) else {
            gfx.window.request_redraw();
            return;
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gfx.gpu.create_encoder("prism frame");

        let (pass2d, draw, text) = (&mut gfx.pass2d, &self.draw, &mut self.text);
        let mut graph = RenderGraph::new();
        let backbuffer = graph.import(&view);
        graph.add_node("ui", &[], &[backbuffer], move |gpu, enc, views| {
            pass2d.draw(gpu, enc, views.get(backbuffer), size, draw, text.atlas_mut(), Some(out.clear));
        });
        graph.execute(&gfx.gpu, &mut gfx.pool, &mut encoder);
        gfx.pool.end_frame();

        gfx.gpu.queue.submit([encoder.finish()]);
        gfx.window.pre_present_notify();
        frame.present();
        log_trace!("frame: {} vertices", self.draw.vertex_count());
    }
}

fn cursor_icon(c: CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
    match c {
        CursorIcon::Default => W::Default,
        CursorIcon::Pointer => W::Pointer,
        CursorIcon::Text => W::Text,
        CursorIcon::EwResize => W::EwResize,
        CursorIcon::NsResize => W::NsResize,
        CursorIcon::NeswResize => W::NeswResize,
        CursorIcon::NwseResize => W::NwseResize,
        CursorIcon::Grabbing => W::Grabbing,
    }
}

fn resize_direction(e: ResizeEdge) -> winit::window::ResizeDirection {
    use winit::window::ResizeDirection as R;
    match e {
        ResizeEdge::North => R::North,
        ResizeEdge::South => R::South,
        ResizeEdge::East => R::East,
        ResizeEdge::West => R::West,
        ResizeEdge::NorthEast => R::NorthEast,
        ResizeEdge::NorthWest => R::NorthWest,
        ResizeEdge::SouthEast => R::SouthEast,
        ResizeEdge::SouthWest => R::SouthWest,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            self.init_gfx(event_loop);
            self.dirty = true;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(ev) = crate::translate::window_event(&event, &mut self.mods, &mut self.pointer) {
            self.events.push(ev);
            if let Some(text) = crate::translate::key_text(&event) {
                self.events.push(text);
            }
            self.dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.surface.resize(&g.gpu, size.width, size.height);
                    g.pool.trim();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.scale = scale_factor,
            WindowEvent::Focused(f) => self.focused = f,
            WindowEvent::RedrawRequested => {
                self.render();
                if self.quit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.dirty && let Some(g) = &self.gfx {
            self.dirty = false;
            g.window.request_redraw();
        }
    }
}
