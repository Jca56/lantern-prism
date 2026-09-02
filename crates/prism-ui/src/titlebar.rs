//! Client-side window frame: a title bar with drag, double-click to
//! maximize, minimize / maximize / close, and resize grabs along the edges.
//! The shell draws it; the app carries out the resulting [`WindowCommand`].

use prism_math::{Rect, Vec2};
use prism_render::DrawList;
use prism_text::TextEngine;

use crate::id::WidgetId;
use crate::shell::{Shell, WindowState};
use crate::state::CursorIcon;
use crate::theme::{Metrics, Theme};
use crate::ui::{Sense, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    /// Start an interactive move (the pointer button is down on the title).
    Drag,
    Minimize,
    ToggleMaximize,
    Close,
    /// Start an interactive resize from this edge or corner.
    Resize(ResizeEdge),
}

/// Grab zone along the window edges, in logical pixels.
const EDGE: f64 = 5.0;
/// Corner zone: this far from both edges.
const CORNER: f64 = 15.0;

fn edge_at(p: Vec2, window: Rect, edge: f64, corner: f64) -> Option<ResizeEdge> {
    let n = p.y < window.min.y + edge;
    let s = p.y >= window.max.y - edge;
    let w = p.x < window.min.x + edge;
    let e = p.x >= window.max.x - edge;
    let near_n = p.y < window.min.y + corner;
    let near_s = p.y >= window.max.y - corner;
    let near_w = p.x < window.min.x + corner;
    let near_e = p.x >= window.max.x - corner;
    match () {
        _ if (n && near_w) || (w && near_n) => Some(ResizeEdge::NorthWest),
        _ if (n && near_e) || (e && near_n) => Some(ResizeEdge::NorthEast),
        _ if (s && near_w) || (w && near_s) => Some(ResizeEdge::SouthWest),
        _ if (s && near_e) || (e && near_s) => Some(ResizeEdge::SouthEast),
        _ if n => Some(ResizeEdge::North),
        _ if s => Some(ResizeEdge::South),
        _ if w => Some(ResizeEdge::West),
        _ if e => Some(ResizeEdge::East),
        _ => None,
    }
}

fn cursor_for(edge: ResizeEdge) -> CursorIcon {
    match edge {
        ResizeEdge::North | ResizeEdge::South => CursorIcon::NsResize,
        ResizeEdge::East | ResizeEdge::West => CursorIcon::EwResize,
        ResizeEdge::NorthEast | ResizeEdge::SouthWest => CursorIcon::NeswResize,
        ResizeEdge::NorthWest | ResizeEdge::SouthEast => CursorIcon::NwseResize,
    }
}

impl Shell {
    /// Hover cursor and press handling for the resize zones. Nothing when
    /// maximized (the compositor owns the edges then).
    pub(crate) fn resize_edges(&mut self, window: Rect, m: Metrics, ws: WindowState) -> Option<WindowCommand> {
        if ws.maximized || !self.state.pointer_in_window || self.state.active.is_some() {
            return None;
        }
        let (edge, corner) = (m.px(EDGE), m.px(CORNER));
        let hovered = edge_at(self.state.pointer, window, edge, corner);
        if let Some(e) = hovered {
            self.state.cursor_icon = cursor_for(e);
        }
        if self.state.pressed && !self.state.press_claimed
            && let Some(e) = edge_at(self.state.press_pos, window, edge, corner)
        {
            self.state.press_claimed = true;
            return Some(WindowCommand::Resize(e));
        }
        None
    }

    /// Draw the title bar across the top of `window`. Returns the rect left
    /// for the areas and any window command the user triggered.
    pub(crate) fn title_bar(
        &mut self,
        draw: &mut DrawList,
        text: &mut TextEngine,
        theme: &Theme,
        m: Metrics,
        window: Rect,
        ws: WindowState,
    ) -> (Rect, Option<WindowCommand>) {
        let (bar, rest) = window.take_top(m.header_h);
        let mut cmd = None;
        let bg = if ws.focused { theme.bg } else { theme.bg.lerp(theme.panel, 0.5) };
        draw.set_layer(0);
        draw.push_clip_absolute(bar);
        draw.rect(bar, bg);
        draw.pop_clip();

        let mut ui = Ui::new(draw, text, theme, m, &mut self.state, bar, bar, WidgetId::ROOT.with("titlebar"), 0);
        ui.set_window_rect(window);

        // Window buttons, right to left: close, maximize, minimize.
        let side = m.header_h;
        let mut x = bar.max.x;
        let buttons = [("close", 2usize), ("maximize", 1), ("minimize", 0)];
        for (name, kind) in buttons {
            x -= side;
            let r = Rect::from_min_size(Vec2::new(x, bar.min.y), Vec2::new(side, side));
            let resp = ui.interact(ui.id(name), r, Sense::CLICK);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
                let hover = if kind == 2 { theme.close } else { theme.widget_hover };
                ui.fill_square(r, if resp.held { theme.widget_active } else { hover });
            }
            let c = r.center();
            let s = m.px(6.0);
            let w = m.px(2.0);
            let ink = theme.text;
            match kind {
                0 => ui.draw.hline(c.x - s, c.x + s, (c.y - w * 0.5).round(), w, ink),
                1 => {
                    if ws.maximized {
                        let a = Rect::from_center_size(c + Vec2::new(-w, w), Vec2::splat(s * 1.6));
                        let b = Rect::from_center_size(c + Vec2::new(w, -w), Vec2::splat(s * 1.6));
                        ui.draw.stroke_rect(b, w, 0.0, ink);
                        ui.draw.rect(a, if resp.hovered { theme.widget_hover } else { bg });
                        ui.draw.stroke_rect(a, w, 0.0, ink);
                    } else {
                        ui.draw.stroke_rect(Rect::from_center_size(c, Vec2::splat(s * 2.0)), w, 0.0, ink);
                    }
                }
                _ => {
                    ui.draw.line(c + Vec2::new(-s, -s), c + Vec2::new(s, s), w, ink);
                    ui.draw.line(c + Vec2::new(-s, s), c + Vec2::new(s, -s), w, ink);
                }
            }
            if resp.clicked {
                cmd = Some(match kind {
                    0 => WindowCommand::Minimize,
                    1 => WindowCommand::ToggleMaximize,
                    _ => WindowCommand::Close,
                });
            }
        }

        // Everything left of the buttons drags the window.
        let drag_rect = Rect::new(bar.min, Vec2::new(x, bar.max.y));
        let resp = ui.interact(ui.id("drag"), drag_rect, Sense::DRAG);
        if resp.pressed {
            cmd = Some(if resp.double_clicked { WindowCommand::ToggleMaximize } else { WindowCommand::Drag });
            // The window system takes over; forget the press so nothing else reacts.
            ui.state.active = None;
        }

        let title_style = ui.text_style().bold();
        let title_rect = Rect::new(Vec2::new(bar.min.x + m.pad, bar.min.y), Vec2::new(x - m.pad, bar.max.y));
        ui.text_in_rect("Prism", &title_style, title_rect, if ws.focused { theme.text } else { theme.text_dim });
        ui.finish();

        (rest, cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_corners() {
        let w = Rect::from_xywh(0.0, 0.0, 1000.0, 500.0);
        assert_eq!(edge_at(Vec2::new(2.0, 250.0), w, 5.0, 15.0), Some(ResizeEdge::West));
        assert_eq!(edge_at(Vec2::new(998.0, 250.0), w, 5.0, 15.0), Some(ResizeEdge::East));
        assert_eq!(edge_at(Vec2::new(500.0, 2.0), w, 5.0, 15.0), Some(ResizeEdge::North));
        assert_eq!(edge_at(Vec2::new(500.0, 497.0), w, 5.0, 15.0), Some(ResizeEdge::South));
        assert_eq!(edge_at(Vec2::new(2.0, 10.0), w, 5.0, 15.0), Some(ResizeEdge::NorthWest));
        assert_eq!(edge_at(Vec2::new(990.0, 498.0), w, 5.0, 15.0), Some(ResizeEdge::SouthEast));
        assert_eq!(edge_at(Vec2::new(500.0, 250.0), w, 5.0, 15.0), None);
        assert_eq!(cursor_for(ResizeEdge::NorthEast), CursorIcon::NeswResize);
    }
}
