//! The shell: one rebuild of the whole window. Lays the screen out, drives
//! separator drags and focus (D017), draws every area's header and body, and
//! reports what the app should do next.

use prism_math::{Rect, Vec2};
use prism_render::DrawList;
use prism_text::TextEngine;

use crate::editors::{EditorCtx, EditorKind, GalleryState, Prefs, draw_editor};
use crate::event::Event;
use crate::id::WidgetId;
use crate::screen::{AreaId, Axis, Screen};
use crate::state::{CursorIcon, UiState};
use crate::theme::Metrics;
use crate::ui::{FILL, Ui};

/// What the app needs to know after a rebuild.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShellOutput {
    pub cursor: CursorIcon,
    /// Run another rebuild immediately (a popup closed, a value committed).
    pub rebuild_again: bool,
    /// Background to clear the window with.
    pub clear: prism_math::Color,
}

enum Action {
    Split(AreaId, Axis),
    Close(AreaId),
    SetEditor(AreaId, EditorKind),
}

pub struct Shell {
    pub screen: Screen,
    pub state: UiState,
    pub prefs: Prefs,
    pub gallery: GalleryState,
    drag_sep: Option<usize>,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    /// Default layout: viewport left, gallery over preferences on the right.
    pub fn new() -> Self {
        let mut screen = Screen::new(EditorKind::Viewport);
        if let Some(right) = screen.split(0, Axis::Horizontal, 0.62, EditorKind::Gallery) {
            screen.split(right, Axis::Vertical, 0.5, EditorKind::Preferences);
        }
        Self { screen, state: UiState::new(), prefs: Prefs::default(), gallery: GalleryState::default(), drag_sep: None }
    }

    /// Metrics for the current preferences at `window_scale`.
    pub fn metrics(&self, window_scale: f64) -> Metrics {
        self.prefs.theme.metrics(window_scale * self.prefs.ui_scale)
    }

    /// One rebuild. `window` is the whole window in physical pixels.
    pub fn frame(
        &mut self,
        events: &[Event],
        window: Rect,
        window_scale: f64,
        text: &mut TextEngine,
        draw: &mut DrawList,
    ) -> ShellOutput {
        let theme = self.prefs.theme.clone();
        let m = self.metrics(window_scale);
        self.state.begin_frame(events, m.widget_h);
        self.screen.layout(window, m.header_h, m.sep);

        // ---- separators -------------------------------------------------
        let st = &mut self.state;
        if st.released {
            self.drag_sep = None;
        }
        if let Some(idx) = self.drag_sep {
            if st.down {
                self.screen.drag_separator(idx, st.pointer, Screen::min_area_px(m.header_h));
                self.screen.layout(window, m.header_h, m.sep);
            }
        } else if st.pressed
            && !st.press_claimed
            && st.popup.is_none()
            && let Some(idx) = self.screen.separator_at(st.press_pos)
        {
            self.drag_sep = Some(idx);
            st.press_claimed = true;
            st.active = Some(WidgetId::ROOT.with("separator"));
        }
        let hover_sep = self.drag_sep.or_else(|| {
            (st.popup.is_none() && st.active.is_none()).then(|| self.screen.separator_at(st.pointer)).flatten()
        });
        let sep_cursor = hover_sep.map(|i| match self.screen.separators()[i].axis {
            Axis::Horizontal => CursorIcon::EwResize,
            Axis::Vertical => CursorIcon::NsResize,
        });

        // ---- focus (D017) -------------------------------------------------
        if self.prefs.focus_follows_mouse {
            if st.pointer_in_window && let Some(a) = self.screen.area_at(st.pointer) {
                self.screen.active = Some(a);
            }
        } else if st.pressed
            && self.drag_sep.is_none()
            && let Some(a) = self.screen.area_at(st.press_pos)
            && st.popup.is_none_or(|(r, _)| !r.contains(st.press_pos))
        {
            self.screen.active = Some(a);
        }

        // ---- areas ------------------------------------------------------
        let layouts: Vec<_> = self.screen.layouts().to_vec();
        let mut actions = Vec::new();
        let mut changed_globals = false;
        for l in &layouts {
            let Some(area) = self.screen.area(l.area) else {
                continue;
            };
            let kind = area.editor;
            let base = WidgetId::ROOT.with_u64(l.area as u64);

            // Header.
            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.rect(l.header, theme.header);
            draw.hline(l.header.min.x, l.header.max.x, l.header.max.y - m.border, m.border, theme.border);
            draw.rect(l.body, theme.panel);
            draw.pop_clip();

            let content = Rect::new(
                Vec2::new(l.header.min.x + m.gap, l.header.min.y + ((l.header.height() - m.widget_h) * 0.5).round()),
                Vec2::new(l.header.max.x - m.gap, l.header.max.y),
            );
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, content, l.header, base.with("header"), 0);
            ui.set_window_rect(window);
            ui.row(|ui| {
                let labels: Vec<&str> = EditorKind::ALL.iter().map(|k| k.label()).collect();
                let mut idx = kind.index();
                if ui.dropdown("editor", &mut idx, &labels) {
                    actions.push(Action::SetEditor(l.area, EditorKind::ALL[idx]));
                }
                // Push the menu to the right edge.
                let style = ui.text_style();
                let menu_w = ui.measure("⋮", &style) + ui.m.pad * 2.0;
                let spacer = (ui.avail_width() - menu_w - ui.m.gap).max(0.0);
                ui.alloc(Vec2::new(spacer, 1.0));
                if let Some(i) = ui.menu_button("⋮", &["Split Left | Right", "Split Top | Bottom", "Close Area"]) {
                    actions.push(match i {
                        0 => Action::Split(l.area, Axis::Horizontal),
                        1 => Action::Split(l.area, Axis::Vertical),
                        _ => Action::Close(l.area),
                    });
                }
            });
            ui.finish();

            // Body.
            let body_content = l.body.shrink(m.pad);
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, body_content, l.body, base.with("body"), 0);
            ui.set_window_rect(window);
            let mut ctx = EditorCtx { prefs: &mut self.prefs, gallery: &mut self.gallery };
            changed_globals |= draw_editor(kind, &mut ui, &mut ctx);
            ui.finish();
        }

        // Focused area outline, on top of its content.
        if let Some(active) = self.screen.active
            && let Some(l) = self.screen.layout_of(active)
        {
            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.stroke_rect(l.rect, m.focus_border, 0.0, theme.focus);
            draw.pop_clip();
        }

        for a in actions {
            match a {
                Action::Split(area, axis) => {
                    let kind = self.screen.area(area).map_or(EditorKind::Empty, |a| a.editor);
                    self.screen.split(area, axis, 0.5, kind);
                }
                Action::Close(area) => {
                    self.screen.join(area);
                }
                Action::SetEditor(area, kind) => {
                    if let Some(a) = self.screen.area_mut(area) {
                        a.editor = kind;
                    }
                }
            }
            self.state.request_rebuild = true;
        }
        if changed_globals {
            self.state.request_rebuild = true;
        }

        self.state.end_frame();
        ShellOutput {
            cursor: sep_cursor.unwrap_or(self.state.cursor_icon),
            rebuild_again: self.state.request_rebuild,
            clear: theme.bg,
        }
    }
}

#[allow(dead_code)]
const _: f64 = FILL;
