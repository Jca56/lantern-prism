//! The winit application: window, GPU wiring, event translation, one frame.

use std::sync::Arc;

use prism_core::{log_error, log_info, log_trace};
use prism_math::{Color, Vec2};
use prism_render::wgpu;
use prism_render::{DrawList, Gpu, Pass2d, RenderGraph, SurfaceTarget, TexturePool};
use prism_text::TextEngine;
use prism_ui::{Event, Modifiers};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// Background color for the whole window. Placeholder until the palette is
/// decided (docs/DECISIONS.md, open questions).
pub const BACKGROUND: Color = Color::hex(0x141414);

struct Gfx {
    window: Arc<Window>,
    gpu: Gpu,
    surface: SurfaceTarget,
    pass2d: Pass2d,
    pool: TexturePool,
}

pub struct App {
    gfx: Option<Gfx>,
    text: TextEngine,
    draw: DrawList,
    /// Events since the last frame, in order. Phase 2's UI consumes these.
    events: Vec<Event>,
    mods: Modifiers,
    pointer: Vec2,
    scale: f64,
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
            events: Vec::new(),
            mods: Modifiers::NONE,
            pointer: Vec2::ZERO,
            scale: 1.0,
        }
    }

    fn init_gfx(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("Prism")
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
        self.gfx = Some(Gfx { window, gpu, surface, pass2d, pool: TexturePool::new() });
    }

    fn render(&mut self) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        let size = gfx.surface.size();
        self.draw.clear();
        demo::build(&mut self.draw, &mut self.text, size, self.scale);

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
            pass2d.draw(gpu, enc, views.get(backbuffer), size, draw, text.atlas_mut(), Some(BACKGROUND));
        });
        graph.execute(&gfx.gpu, &mut gfx.pool, &mut encoder);
        gfx.pool.end_frame();

        gfx.gpu.queue.submit([encoder.finish()]);
        gfx.window.pre_present_notify();
        frame.present();
        log_trace!("frame: {} vertices", self.draw.vertex_count());
    }
}

use crate::demo;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            self.init_gfx(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(ev) = crate::translate::window_event(&event, &mut self.mods, &mut self.pointer) {
            let redraw = ev.wants_redraw();
            self.events.push(ev);
            if let Some(text) = crate::translate::key_text(&event) {
                self.events.push(text);
            }
            if redraw && let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
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
            WindowEvent::RedrawRequested => {
                self.events.clear();
                self.render();
            }
            _ => {}
        }
    }
}
