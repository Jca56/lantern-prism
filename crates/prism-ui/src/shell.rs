//! The shell: one rebuild of the whole window. Lays the screen out, drives
//! separator drags and focus (D017), draws every area's header and body,
//! routes leftover keys through the keymap, hosts popups, and reports what
//! the app should do next.

use prism_doc::{Doc, ObjectMode};
use prism_math::{Rect, Vec2};
use prism_ops::keymap::{CTX_MESH, CTX_OBJECT, CTX_WINDOW};
use prism_ops::{Ctx, Executor, KeyConfig, UiRequest};
use prism_render::DrawList;
use prism_text::TextEngine;
use prism_viewport::{PickRequest, PickResult, ViewportRequest};

use crate::context_menu::{ContextMenu, MenuContext};
use crate::editors::{EditorCtx, EditorKind, GalleryState, OutlinerState, Prefs, PropertiesState, draw_editor, draw_editor_header, run_op, viewport};
use crate::event::{Event, MouseButton};
use crate::id::WidgetId;
use crate::popups::{self, Popup};
use crate::screen::{AreaId, Axis, Screen};
use crate::state::{CursorIcon, UiState};
use crate::theme::Metrics;
use crate::titlebar::WindowCommand;
use crate::ui::Ui;

/// What the app needs to know after a rebuild.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShellOutput {
    pub cursor: CursorIcon,
    /// Run another rebuild immediately (a popup closed, a value committed).
    pub rebuild_again: bool,
    /// Background to clear the window with.
    pub clear: prism_math::Color,
    /// Something the title bar asked the window system to do.
    pub window_command: Option<WindowCommand>,
    pub quit: bool,
    /// 3D viewports to render this frame, in draw order.
    pub viewports: Vec<ViewportRequest>,
    /// Clicks to resolve with the renderer, then feed to `apply_pick`.
    pub picks: Vec<PickRequest>,
    /// Redraw after this long even without input (a tooltip is due).
    pub wake_in: Option<std::time::Duration>,
}

/// Facts about the window the shell cannot know on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowState {
    pub maximized: bool,
    pub focused: bool,
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
    pub keys: KeyConfig,
    pub gallery: GalleryState,
    pub outliner: OutlinerState,
    pub properties: PropertiesState,
    pub(crate) popup: Option<Popup>,
    pub(crate) drag_sep: Option<usize>,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    /// Default layout: viewport left; outliner over properties on the right.
    pub fn new() -> Self {
        let mut screen = Screen::new(EditorKind::Viewport);
        if let Some(right) = screen.split(0, Axis::Horizontal, 0.68, EditorKind::Outliner) {
            screen.split(right, Axis::Vertical, 0.35, EditorKind::Properties);
        }
        Self {
            screen,
            state: UiState::new(),
            prefs: Prefs::default(),
            keys: KeyConfig::default_prism(),
            gallery: GalleryState::default(),
            outliner: OutlinerState::default(),
            properties: PropertiesState::default(),
            popup: None,
            drag_sep: None,
        }
    }

    /// Metrics for the current preferences at `window_scale`.
    pub fn metrics(&self, window_scale: f64) -> Metrics {
        self.prefs.theme.metrics(window_scale * self.prefs.ui_scale)
    }

    fn document_title(doc: &Doc, exec: &Executor) -> String {
        let name = doc
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());
        format!("Prism · {name}{}", if exec.is_dirty() { " *" } else { "" })
    }

    /// One rebuild. `window` is the whole window in physical pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        events: &[Event],
        window: Rect,
        window_scale: f64,
        ws: WindowState,
        doc: &mut Doc,
        exec: &mut Executor,
        text: &mut TextEngine,
        draw: &mut DrawList,
    ) -> ShellOutput {
        let theme = self.prefs.theme.clone();
        let m = self.metrics(window_scale);
        let mut requests: Vec<UiRequest> = Vec::new();
        let mut viewports: Vec<ViewportRequest> = Vec::new();
        let mut picks: Vec<PickRequest> = Vec::new();
        let mut context_menu: Option<MenuContext> = None;

        // A running modal operator owns button presses and keys; the UI still
        // sees motion (hover, cursor), releases (so a drag that started a
        // modal settles), and the middle button and wheel, so the view can be
        // navigated mid-transform.
        let modal = exec.is_modal();
        let owned_by_modal =
            |e: &Event| matches!(e, Event::Button { button: MouseButton::Left | MouseButton::Right, pressed: true, .. } | Event::Key { .. } | Event::Text(_));
        let ui_events: Vec<Event> = if modal { events.iter().filter(|e| !owned_by_modal(e)).cloned().collect() } else { Vec::new() };
        self.state.begin_frame(if modal { &ui_events } else { events }, m.widget_h);
        let pointer = self.state.pointer;
        if modal {
            self.modal_events(events, doc, exec, &mut requests);
        }

        // Resize grabs along the undecorated edges come before everything.
        let mut window_command = self.resize_edges(window, m, ws);
        let edge_cursor = self.state.cursor_icon;
        let title = Self::document_title(doc, exec);
        let status = exec.last_report.clone().unwrap_or_default();
        let (area_rect, title_cmd) = self.title_bar(draw, text, &theme, m, window, ws, &title, &status);
        window_command = window_command.or(title_cmd);
        let areas_window = area_rect;
        self.screen.layout(areas_window, m.header_h, m.sep);

        // ---- separators -------------------------------------------------
        let st = &mut self.state;
        if st.released {
            self.drag_sep = None;
        }
        if let Some(idx) = self.drag_sep {
            if st.down {
                self.screen.drag_separator(idx, st.pointer, Screen::min_area_px(m.header_h));
                self.screen.layout(areas_window, m.header_h, m.sep);
            }
        } else if st.pressed
            && !st.press_claimed
            && st.popup.is_none()
            && self.popup.is_none()
            && let Some(idx) = self.screen.separator_at(st.press_pos)
        {
            self.drag_sep = Some(idx);
            st.press_claimed = true;
            st.active = Some(WidgetId::ROOT.with("separator"));
        }
        let hover_sep = self.drag_sep.or_else(|| {
            (st.popup.is_none() && st.active.is_none() && self.popup.is_none())
                .then(|| self.screen.separator_at(st.pointer))
                .flatten()
        });
        let sep_cursor = hover_sep.map(|i| match self.screen.separators()[i].axis {
            Axis::Horizontal => CursorIcon::EwResize,
            Axis::Vertical => CursorIcon::NsResize,
        });

        // ---- focus (D017) -------------------------------------------------
        if self.prefs.focus_follows_mouse {
            if st.pointer_in_window && self.popup.is_none() && !modal && let Some(a) = self.screen.area_at(st.pointer) {
                self.screen.active = Some(a);
            }
        } else if st.pressed
            && self.drag_sep.is_none()
            && let Some(a) = self.screen.area_at(st.press_pos)
            && st.popup.is_none_or(|(r, _)| !r.contains(st.press_pos))
        {
            self.screen.active = Some(a);
        }

        // ---- popup (drawn first so it claims the pointer) --------------
        let mut refresh_menu = false;
        let popup_view = self.target_view();
        if let Some(popup) = self.popup.as_mut() {
            let entries: Vec<(String, String)> = match popup {
                Popup::Palette { query, .. } => {
                    exec.registry.search(query).into_iter().map(|o| (o.id.to_owned(), o.label.to_owned())).collect()
                }
                _ => Vec::new(),
            };
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, window, window, WidgetId::ROOT.with("popup"), 0);
            ui.set_window_rect(window);
            let result = popups::draw(&mut ui, popup, window, &entries, doc, exec, &mut requests, pointer, popup_view);
            ui.finish();
            if result.close {
                self.popup = None;
            } else {
                // Rebuilt below, once the requests the tool raised have run.
                refresh_menu = result.refresh;
            }
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

            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.rect_gradient(l.header, theme.top(theme.header), theme.bottom(theme.header));
            draw.hline(l.header.min.x, l.header.max.x, l.header.min.y, m.border, theme.highlight(theme.header));
            draw.hline(l.header.min.x, l.header.max.x, l.header.max.y - m.border, m.border, theme.border_dark);
            if kind != EditorKind::Viewport {
                draw.rect(l.body, theme.panel); // a viewport body is painted by the 3D pass
            }
            draw.stroke_rect(l.rect, m.border, 0.0, theme.border_dark);
            draw.pop_clip();

            let content = Rect::new(
                Vec2::new(l.header.min.x + m.gap, l.header.min.y + ((l.header.height() - m.widget_h) * 0.5).round()),
                Vec2::new(l.header.max.x - m.gap, l.header.max.y),
            );
            let area_vp = &mut self.screen.area_mut(l.area).expect("live area").viewport;
            let view = (kind == EditorKind::Viewport).then(|| viewport::view_info(&area_vp.camera, l.body));
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, content, l.header, base.with("header"), 0);
            ui.set_window_rect(window);
            ui.row(|ui| {
                let labels: Vec<&str> = EditorKind::ALL.iter().map(|k| k.label()).collect();
                let mut idx = kind.index();
                if ui.dropdown("editor", &mut idx, &labels) {
                    actions.push(Action::SetEditor(l.area, EditorKind::ALL[idx]));
                }
                {
                    let mut ctx = EditorCtx {
                        doc,
                        exec,
                        prefs: &mut self.prefs,
                        gallery: &mut self.gallery,
                        outliner: &mut self.outliner,
                        properties: &mut self.properties,
                        requests: &mut requests,
                        pointer,
                        view,
                        area: l.area,
                        viewport: area_vp,
                        viewports: &mut viewports,
                        picks: &mut picks,
                        context_menu: &mut context_menu,
                    };
                    draw_editor_header(kind, ui, &mut ctx);
                }
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

            let body_content = l.body.shrink(m.pad);
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, body_content, l.body, base.with("body"), 0);
            ui.set_window_rect(window);
            let mut ctx = EditorCtx {
                doc,
                exec,
                prefs: &mut self.prefs,
                gallery: &mut self.gallery,
                outliner: &mut self.outliner,
                properties: &mut self.properties,
                requests: &mut requests,
                pointer,
                view,
                area: l.area,
                viewport: area_vp,
                viewports: &mut viewports,
                picks: &mut picks,
                context_menu: &mut context_menu,
            };
            changed_globals |= draw_editor(kind, &mut ui, &mut ctx);
            ui.finish();
        }

        // Focused area outline, on top of its content, in the mode colour:
        // blue in Object mode, gold in Edit mode.
        let editing = doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit) && doc.object_mesh(doc.active_object_id()).is_some();
        if let Some(active) = self.screen.active
            && let Some(l) = self.screen.layout_of(active)
        {
            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.stroke_rect(l.rect, m.focus_border, 0.0, theme.mode_color(editing));
            draw.pop_clip();
        }

        // ---- keymap: keys no widget consumed -----------------------------
        if self.popup.is_none() && !exec.is_modal() {
            let leftover: Vec<_> = self.state.keys.drain(..).collect();
            let editor_ctx = self
                .screen
                .active
                .and_then(|a| self.screen.area(a))
                .map_or("editor", |a| a.editor.keymap_context());
            let mode_ctx = if editing { CTX_MESH } else { CTX_OBJECT };
            let contexts = [editor_ctx, mode_ctx, CTX_WINDOW];
            let view = self.target_view();
            for k in leftover {
                let ev = Event::Key { key: k.key, pressed: true, repeat: k.repeat, mods: k.mods };
                let item = {
                    let ctx = Ctx::new(doc);
                    self.keys.resolve(&contexts, &ev, |op| exec.registry.get(op).is_some_and(|i| i.poll(&ctx))).cloned()
                };
                if let Some(item) = item {
                    let _ = run_op(doc, exec, pointer, view, &item.op, &item.overrides, &mut requests);
                    self.state.request_rebuild = true;
                }
            }
        }

        // ---- context menus asked for by editors ---------------------------
        if let Some(mc) = context_menu {
            let flags = self.view_flags_for(None);
            self.popup = Some(Popup::Context(ContextMenu::build(mc, doc, exec, pointer, flags)));
            self.state.request_rebuild = true;
        }

        let quit = self.apply_requests(requests, doc);

        // A tool in the context menu changed something it displays. Now that
        // its request has been applied, rebuild the strip and title from live
        // state (tabs keep their panels) — or the whole menu, when the tool
        // switched between Object and Edit mode and the menu's context is stale.
        let flags = self.view_flags_for(None);
        if refresh_menu && let Some(Popup::Context(menu)) = self.popup.as_mut() {
            let now_editing = doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit) && doc.object_mesh(doc.active_object_id()).is_some();
            let menu_editing = matches!(menu.context, MenuContext::Mesh(_) | MenuContext::Element { .. });
            if now_editing != menu_editing {
                let context = match (now_editing, doc.active_object_id()) {
                    (false, id) if doc.objects.contains(id) => MenuContext::Object(id),
                    _ => ContextMenu::context_for(doc, PickResult::Nothing),
                };
                *menu = ContextMenu::build(context, doc, exec, menu.pos, flags);
            } else {
                let fresh = ContextMenu::build(menu.context, doc, exec, menu.pos, flags);
                menu.tools = fresh.tools;
                menu.title = fresh.title;
            }
            self.state.request_rebuild = true;
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
        let cursor = if exec.is_modal() {
            CursorIcon::Grabbing
        } else if edge_cursor != CursorIcon::Default {
            edge_cursor
        } else {
            sep_cursor.unwrap_or(self.state.cursor_icon)
        };
        let wake_in = self.state.wake_in.take();
        ShellOutput { cursor, rebuild_again: self.state.request_rebuild, clear: theme.bg, window_command, quit, viewports, picks, wake_in }
    }
}
