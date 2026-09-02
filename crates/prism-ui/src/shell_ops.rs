//! The shell's side of the operator system: which viewport an operator sees,
//! feeding a running modal operator, acting on operator requests, and turning
//! resolved picks into selection or a context menu (D024).

use prism_doc::{Doc, Elem, SelectMode};
use prism_ops::builtin::select;
use prism_ops::{Ctx, Executor, Flow, UiRequest, ViewInfo};
use prism_props::Value;
use prism_viewport::{GizmoMode, PickMode, PickPurpose, PickRequest, PickResult, PickSet, Shading, ViewportState};

use crate::context_menu::{ContextMenu, ViewFlags};
use crate::editors::{EditorKind, record_edit, run_op, viewport};
use crate::event::Event;
use crate::popups::Popup;
use crate::screen::AreaId;
use crate::shell::Shell;

impl Shell {
    /// The viewport area view requests apply to: the active one if it is a
    /// viewport, else the first viewport on screen.
    pub(crate) fn target_viewport(&self) -> Option<AreaId> {
        let is_vp = |a: AreaId| self.screen.area(a).is_some_and(|ar| ar.editor == EditorKind::Viewport);
        self.screen.active.filter(|&a| is_vp(a)).or_else(|| self.screen.layouts().iter().map(|l| l.area).find(|&a| is_vp(a)))
    }

    /// What an operator started outside any particular area (a key, the
    /// palette, a menu) sees: the target viewport as laid out last frame.
    pub(crate) fn target_view(&self) -> Option<ViewInfo> {
        let a = self.target_viewport()?;
        let body = self.screen.layout_of(a)?.body;
        let vp = &self.screen.area(a)?.viewport;
        Some(viewport::view_info(&vp.camera, body))
    }

    /// Display flags of a viewport area (for the context menu's tool strip).
    pub(crate) fn view_flags_for(&self, area: Option<AreaId>) -> ViewFlags {
        area.or_else(|| self.target_viewport())
            .and_then(|a| self.screen.area(a))
            .map_or(ViewFlags::default(), |ar| ViewFlags { wire: ar.viewport.shading == Shading::Wire, grid: ar.viewport.overlays.grid, gizmo: ar.viewport.gizmo })
    }

    /// Change the target viewport's state, if there is one.
    fn with_target_viewport(&mut self, f: impl FnOnce(&mut ViewportState)) {
        if let Some(a) = self.target_viewport()
            && let Some(area) = self.screen.area_mut(a)
        {
            f(&mut area.viewport);
        }
    }

    /// Hand every event of this frame to the running modal operator, with the
    /// view it works in. Rebuild once it finishes so the UI settles.
    pub(crate) fn modal_events(&mut self, events: &[Event], doc: &mut Doc, exec: &mut Executor, requests: &mut Vec<UiRequest>) {
        let view = self.target_view();
        let mut pointer = self.state.pointer;
        for ev in events {
            match ev {
                Event::PointerMoved(p) | Event::Button { pos: p, .. } | Event::Wheel { pos: p, .. } => pointer = *p,
                _ => {}
            }
            let mut ctx = Ctx::new(doc);
            ctx.pointer = pointer;
            ctx.view = view;
            ctx.mods = self.state.mods;
            let flow = exec.modal_event(&mut ctx, ev);
            requests.append(&mut exec.requests);
            if matches!(flow, Some(Ok(Flow::Finished | Flow::Cancelled)) | Some(Err(_))) {
                self.state.request_rebuild = true;
            }
        }
    }

    /// Carry out what operators asked of the UI. Returns `true` to quit.
    pub(crate) fn apply_requests(&mut self, requests: Vec<UiRequest>, doc: &mut Doc) -> bool {
        let mut quit = false;
        let pointer = self.state.pointer;
        for r in requests {
            match r {
                UiRequest::Menu(name) => self.popup = crate::popups::menu(&name, pointer),
                UiRequest::Palette => self.popup = Some(Popup::Palette { query: String::new(), selected: 0 }),
                UiRequest::PathDialog { op, save, suggest } => {
                    let text = suggest.unwrap_or_else(|| doc.path.as_ref().map_or_else(|| "untitled.prism".to_owned(), |p| p.display().to_string()));
                    self.popup = Some(Popup::Path { op, save, text });
                }
                UiRequest::Quit => quit = true,
                UiRequest::ViewFrame { selected } => {
                    let bounds = prism_viewport::scene_bounds(doc, selected);
                    self.with_target_viewport(|vp| vp.camera.frame(&bounds));
                }
                UiRequest::ViewShading { wire } => {
                    self.with_target_viewport(|vp| vp.shading = if wire { Shading::Wire } else { Shading::Solid });
                }
                UiRequest::ViewToggleGrid => self.with_target_viewport(|vp| vp.overlays.grid = !vp.overlays.grid),
                UiRequest::GizmoCycle => self.with_target_viewport(|vp| vp.gizmo = vp.gizmo.next()),
                UiRequest::GizmoSet(i) => self.with_target_viewport(|vp| vp.gizmo = GizmoMode::from_index(i)),
                UiRequest::Undo | UiRequest::Redo | UiRequest::HistoryClear => {}
            }
            self.state.request_rebuild = true;
        }
        quit
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
            // The right-clicked viewport becomes the one the menu acts on.
            self.screen.active = Some(req.area);
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
        let view = Some(viewport::view_info(&req.camera, req.rect));
        match result {
            PickResult::Nothing => {
                if !req.extend && !req.toggle {
                    let op = if req.mode == prism_viewport::PickMode::Object { "object.select_all" } else { "mesh.select_all" };
                    let _ = run_op(doc, exec, req.pos, view, op, &[("action".to_owned(), Value::Enum(2))], &mut requests);
                }
            }
            PickResult::Object(id) => {
                let _ = run_op(doc, exec, req.pos, view, "object.select", &ov(vec![("id".to_owned(), Value::Id(id))]), &mut requests);
            }
            PickResult::Vert(_, v) => {
                let _ = run_op(doc, exec, req.pos, view, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(0)), ("handle".to_owned(), Value::I64(v.to_raw() as i64))]), &mut requests);
            }
            PickResult::Edge(_, e) => {
                let _ = run_op(doc, exec, req.pos, view, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(1)), ("handle".to_owned(), Value::I64(e.to_raw() as i64))]), &mut requests);
            }
            PickResult::Face(_, f) => {
                let _ = run_op(doc, exec, req.pos, view, "mesh.select", &ov(vec![("kind".to_owned(), Value::Enum(2)), ("handle".to_owned(), Value::I64(f.to_raw() as i64))]), &mut requests);
            }
        }
        self.state.request_rebuild = true;
    }

    /// Apply a box select (D025). Selection is set directly and recorded as
    /// one "Box Select" undo step (D021 rule 3), since the set of hits has
    /// no natural operator props.
    pub fn apply_box(&mut self, doc: &mut Doc, exec: &mut Executor, req: &PickRequest, set: PickSet) {
        let before = doc.clone();
        let (extend, subtract) = (req.extend, req.toggle);
        let edit_mesh = doc.active_object().map(|o| o.data);
        let elems = |block: &mut prism_doc::MeshBlock, mode: SelectMode, elems: &[Elem]| select::select_elems(block, mode, elems, extend, subtract);
        let changed = match set {
            PickSet::Objects(ids) => select::select_objects(doc, &ids, extend, subtract),
            PickSet::Nothing if req.mode == PickMode::Object => select::select_objects(doc, &[], extend, subtract),
            PickSet::Nothing => {
                let mode = doc.scene().map_or(SelectMode::Vertex, |s| s.tool.select_mode);
                edit_mesh.and_then(|m| doc.meshes.get_mut(m)).is_some_and(|b| elems(b, mode, &[]))
            }
            PickSet::Verts(m, vs) => doc.meshes.get_mut(m).is_some_and(|b| elems(b, SelectMode::Vertex, &vs.iter().copied().map(Elem::Vert).collect::<Vec<_>>())),
            PickSet::Edges(m, es) => doc.meshes.get_mut(m).is_some_and(|b| elems(b, SelectMode::Edge, &es.iter().copied().map(Elem::Edge).collect::<Vec<_>>())),
            PickSet::Faces(m, fs) => doc.meshes.get_mut(m).is_some_and(|b| elems(b, SelectMode::Face, &fs.iter().copied().map(Elem::Face).collect::<Vec<_>>())),
        };
        if changed {
            record_edit(exec, doc, before, "Box Select", false);
        }
        self.state.request_rebuild = true;
    }
}
