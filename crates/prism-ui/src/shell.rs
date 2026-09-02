//! The shell: one rebuild of the whole window. Lays the screen out, drives
//! separator drags and focus (D017), draws every area's header and body,
//! routes leftover keys through the keymap, hosts popups, and reports what
//! the app should do next.

use prism_doc::{Doc, ObjectMode};
use prism_math::{Rect, Vec2};
use prism_ops::keymap::{CTX_MESH, CTX_OBJECT, CTX_WINDOW};
use prism_ops::{Ctx, Executor, KeyConfig, UiRequest};
use prism_props::Value;
use prism_render::DrawList;
use prism_text::TextEngine;
use prism_viewport::{PickPurpose, PickRequest, PickResult, Shading, ViewportRequest};

use crate::context_menu::{ContextMenu, MenuContext, ViewFlags};
use crate::editors::{EditorCtx, EditorKind, GalleryState, OutlinerState, Prefs, PropertiesState, draw_editor, draw_editor_header, run_op};
use crate::event::Event;
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
    popup: Option<Popup>,
    drag_sep: Option<usize>,
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
        self.state.begin_frame(events, m.widget_h);
        let mut requests: Vec<UiRequest> = Vec::new();
        let mut viewports: Vec<ViewportRequest> = Vec::new();
        let mut picks: Vec<PickRequest> = Vec::new();
        let mut context_menu: Option<MenuContext> = None;
        let pointer = self.state.pointer;

        // A running modal operator sees every event first.
        if exec.is_modal() {
            for ev in events {
                let mut ctx = Ctx::new(doc);
                ctx.pointer = pointer;
                exec.modal_event(&mut ctx, ev);
                requests.append(&mut exec.requests);
            }
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
            if st.pointer_in_window && self.popup.is_none() && let Some(a) = self.screen.area_at(st.pointer) {
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
        if let Some(popup) = self.popup.as_mut() {
            let entries: Vec<(String, String)> = match popup {
                Popup::Palette { query, .. } => {
                    exec.registry.search(query).into_iter().map(|o| (o.id.to_owned(), o.label.to_owned())).collect()
                }
                _ => Vec::new(),
            };
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, window, window, WidgetId::ROOT.with("popup"), 0);
            ui.set_window_rect(window);
            let result = popups::draw(&mut ui, popup, window, &entries, doc, exec, &mut requests, pointer);
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
                area: l.area,
                viewport: area_vp,
                viewports: &mut viewports,
                picks: &mut picks,
                context_menu: &mut context_menu,
            };
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

        // ---- keymap: keys no widget consumed -----------------------------
        if self.popup.is_none() && !exec.is_modal() {
            let leftover: Vec<_> = self.state.keys.drain(..).collect();
            let editor_ctx = self
                .screen
                .active
                .and_then(|a| self.screen.area(a))
                .map_or("editor", |a| a.editor.keymap_context());
            let mode_ctx = if doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit) && doc.object_mesh(doc.active_object_id()).is_some() {
                CTX_MESH
            } else {
                CTX_OBJECT
            };
            let contexts = [editor_ctx, mode_ctx, CTX_WINDOW];
            for k in leftover {
                let ev = Event::Key { key: k.key, pressed: true, repeat: k.repeat, mods: k.mods };
                let item = {
                    let ctx = Ctx::new(doc);
                    self.keys.resolve(&contexts, &ev, |op| exec.registry.get(op).is_some_and(|i| i.poll(&ctx))).cloned()
                };
                if let Some(item) = item {
                    let _ = run_op(doc, exec, pointer, &item.op, &item.overrides, &mut requests);
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

        // ---- requests from operators --------------------------------------
        let mut quit = false;
        for r in requests {
            match r {
                UiRequest::Menu(name) => self.popup = popups::menu(&name, pointer),
                UiRequest::Palette => self.popup = Some(Popup::Palette { query: String::new(), selected: 0 }),
                UiRequest::PathDialog { op, save } => {
                    let text = doc.path.as_ref().map_or_else(|| "untitled.prism".to_owned(), |p| p.display().to_string());
                    self.popup = Some(Popup::Path { op, save, text });
                }
                UiRequest::Quit => quit = true,
                UiRequest::ViewFrame { selected } => {
                    let bounds = prism_viewport::scene_bounds(doc, selected);
                    if let Some(a) = self.target_viewport()
                        && let Some(area) = self.screen.area_mut(a)
                    {
                        area.viewport.camera.frame(&bounds);
                    }
                }
                UiRequest::ViewShading { wire } => {
                    if let Some(a) = self.target_viewport()
                        && let Some(area) = self.screen.area_mut(a)
                    {
                        area.viewport.shading = if wire { Shading::Wire } else { Shading::Solid };
                    }
                }
                UiRequest::ViewToggleGrid => {
                    if let Some(a) = self.target_viewport()
                        && let Some(area) = self.screen.area_mut(a)
                    {
                        area.viewport.overlays.grid = !area.viewport.overlays.grid;
                    }
                }
                UiRequest::Undo | UiRequest::Redo | UiRequest::HistoryClear => {}
            }
            self.state.request_rebuild = true;
        }

        // A tool in the context menu changed something it displays (shading,
        // grid, select mode). Now that its request has been applied, rebuild
        // the strip and title from live state; tabs keep their panels.
        let flags = self.view_flags_for(None);
        if refresh_menu && let Some(Popup::Context(menu)) = self.popup.as_mut() {
            let fresh = ContextMenu::build(menu.context, doc, exec, menu.pos, flags);
            menu.tools = fresh.tools;
            menu.title = fresh.title;
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
        let cursor = if edge_cursor != CursorIcon::Default { edge_cursor } else { sep_cursor.unwrap_or(self.state.cursor_icon) };
        ShellOutput { cursor, rebuild_again: self.state.request_rebuild, clear: theme.bg, window_command, quit, viewports, picks }
    }

    /// The viewport area view requests apply to: the active one if it is a
    /// viewport, else the first viewport on screen.
    fn target_viewport(&self) -> Option<AreaId> {
        let is_vp = |a: AreaId| self.screen.area(a).is_some_and(|ar| ar.editor == EditorKind::Viewport);
        self.screen.active.filter(|&a| is_vp(a)).or_else(|| self.screen.layouts().iter().map(|l| l.area).find(|&a| is_vp(a)))
    }

    /// Display flags of a viewport area (for the context menu's tool strip).
    fn view_flags_for(&self, area: Option<AreaId>) -> ViewFlags {
        area.or_else(|| self.target_viewport())
            .and_then(|a| self.screen.area(a))
            .map_or(ViewFlags::default(), |ar| ViewFlags { wire: ar.viewport.shading == Shading::Wire, grid: ar.viewport.overlays.grid })
    }

    /// Turn a resolved pick into a selection operator, or open the context
    /// menu for a right click.
    pub fn apply_pick(&mut self, doc: &mut Doc, exec: &mut Executor, req: &PickRequest, result: PickResult) {
        let mut requests = Vec::new();
        if req.purpose == PickPurpose::ContextMenu {
            // Right-clicking something unselected selects it first.
            let selected = match result {
                PickResult::Object(id) => doc.objects.get(id).is_some_and(|o| o.selected),
                PickResult::Vert(m, v) => doc.meshes.get(m).is_some_and(|b| b.mesh.vert_attrs().bools(prism_mesh::tables::V_SELECT)[v.idx()]),
                PickResult::Edge(m, e) => doc.meshes.get(m).is_some_and(|b| b.mesh.edge_attrs().bools(prism_mesh::tables::E_SELECT)[e.idx()]),
                PickResult::Face(m, f) => doc.meshes.get(m).is_some_and(|b| b.mesh.face_attrs().bools(prism_mesh::tables::F_SELECT)[f.idx()]),
                PickResult::Nothing => true,
            };
            if !selected {
                let plain = PickRequest { extend: false, toggle: false, purpose: PickPurpose::Select, ..*req };
                self.apply_pick(doc, exec, &plain, result);
            }
            let context = ContextMenu::context_for(doc, result);
            let flags = self.view_flags_for(Some(req.area));
            self.popup = Some(Popup::Context(ContextMenu::build(context, doc, exec, req.pos, flags)));
            self.state.request_rebuild = true;
            return;
        }
        let ov = |extra: Vec<(String, Value)>| -> Vec<(String, Value)> {
            let mut v = vec![("extend".to_owned(), Value::Bool(req.extend)), ("toggle".to_owned(), Value::Bool(req.toggle))];
            v.extend(extra);
            v
        };
        match result {
            PickResult::Nothing => {
                if !req.extend && !req.toggle {
                    let op = if req.mode == prism_viewport::PickMode::Object { "object.select_all" } else { "mesh.select_all" };
                    let _ = run_op(doc, exec, req.pos, op, &[("action".to_owned(), Value::Enum(2))], &mut requests);
                }
            }
            PickResult::Object(id) => {
                let _ = run_op(doc, exec, req.pos, "object.select", &ov(vec![("id".to_owned(), Value::Id(id))]), &mut requests);
            }
            PickResult::Vert(_, v) => {
                let _ = run_op(doc, exec, req.pos, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(0)), ("handle".to_owned(), Value::I64(v.to_raw() as i64))]), &mut requests);
            }
            PickResult::Edge(_, e) => {
                let _ = run_op(doc, exec, req.pos, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(1)), ("handle".to_owned(), Value::I64(e.to_raw() as i64))]), &mut requests);
            }
            PickResult::Face(_, f) => {
                let _ = run_op(doc, exec, req.pos, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(2)), ("handle".to_owned(), Value::I64(f.to_raw() as i64))]), &mut requests);
            }
        }
        self.state.request_rebuild = true;
    }
}
