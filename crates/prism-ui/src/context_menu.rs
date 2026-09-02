//! The right-click context menu: built from what was under the pointer,
//! with tabs, submenus, live operator panels and a tool strip floating
//! outside its left edge (D023).

use prism_core::Id;
use prism_doc::{DataKind, Doc, ObjectMode, SelectMode};
use prism_math::Vec2;
use prism_ops::Executor;
use prism_props::{Reflect, Value};

use crate::icons::Icon;

/// What was under the pointer when the menu opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuContext {
    /// Empty space in a viewport or the outliner, object mode.
    Scene,
    Object(Id),
    /// Empty space while editing `mesh`.
    Mesh(Id),
    /// An element of `mesh` (already selected by the time the menu opens).
    Element { mesh: Id, kind: SelectMode },
}

#[derive(Clone, Debug)]
pub enum Item {
    Header(String),
    Separator,
    Action { label: String, op: String, overrides: Vec<(String, Value)> },
    Sub { label: String, items: Vec<Item> },
    /// An operator's properties with Apply; afterwards edits re-run it live.
    /// Used sparingly (Rename): mesh tools are plain actions whose knobs
    /// appear in the Properties editor's "Adjust Last Operation".
    OpPanel { op: String, label: String, props: Box<dyn Reflect>, applied: bool },
    /// The properties of an object, edited in place (undoable).
    ObjectProps(Id),
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub label: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub icon: Icon,
    pub active: bool,
    pub op: String,
    pub overrides: Vec<(String, Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Narrow,
    Wide,
}

#[derive(Clone, Debug)]
pub struct ContextMenu {
    pub context: MenuContext,
    pub title: String,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub tools: Vec<Tool>,
    pub pos: Vec2,
    pub width: Width,
    /// Submenu open in the current tab, by item index.
    pub open_sub: Option<usize>,
    /// Height measured last frame, for placement.
    pub height: f64,
}

fn act(label: &str, op: &str, overrides: Vec<(&str, Value)>) -> Item {
    Item::Action { label: label.into(), op: op.into(), overrides: overrides.into_iter().map(|(k, v)| (k.to_owned(), v)).collect() }
}

fn tool(icon: Icon, active: bool, op: &str, overrides: Vec<(&str, Value)>) -> Tool {
    Tool { icon, active, op: op.into(), overrides: overrides.into_iter().map(|(k, v)| (k.to_owned(), v)).collect() }
}

fn op_panel(exec: &Executor, op: &str) -> Option<Item> {
    let info = exec.registry.get(op)?;
    Some(Item::OpPanel { op: op.into(), label: info.label.into(), props: info.new_props(), applied: false })
}

fn add_items() -> Vec<Item> {
    vec![
        act("Plane", "object.add_primitive", vec![("kind", Value::Enum(0))]),
        act("Cube", "object.add_primitive", vec![("kind", Value::Enum(1))]),
        act("UV Sphere", "object.add_primitive", vec![("kind", Value::Enum(2))]),
        act("Cylinder", "object.add_primitive", vec![("kind", Value::Enum(3))]),
        act("Grid", "object.add_primitive", vec![("kind", Value::Enum(4)), ("segments", Value::I64(10))]),
        act("Circle", "object.add_primitive", vec![("kind", Value::Enum(5))]),
    ]
}

fn view_items() -> Vec<Item> {
    vec![
        act("Frame All", "view3d.frame_all", vec![]),
        act("Frame Selected", "view3d.frame_selected", vec![]),
    ]
}

/// The view tools every viewport menu carries.
fn view_tools(shading_wire: bool, grid: bool) -> Vec<Tool> {
    vec![
        tool(Icon::Solid, !shading_wire, "view3d.shading", vec![("wire", Value::Bool(false))]),
        tool(Icon::Wire, shading_wire, "view3d.shading", vec![("wire", Value::Bool(true))]),
        tool(Icon::Grid, grid, "view3d.toggle_grid", vec![]),
        tool(Icon::Frame, false, "view3d.frame_all", vec![]),
    ]
}

/// Viewport display state the menu reflects in its tool strip.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewFlags {
    pub wire: bool,
    pub grid: bool,
}

impl ContextMenu {
    pub fn build(context: MenuContext, doc: &Doc, exec: &Executor, pos: Vec2, view: ViewFlags) -> ContextMenu {
        let mut tools = view_tools(view.wire, view.grid);
        let (title, tabs, width) = match context {
            MenuContext::Scene => {
                let select = vec![
                    act("Select All", "object.select_all", vec![("action", Value::Enum(1))]),
                    act("Deselect All", "object.select_all", vec![("action", Value::Enum(2))]),
                    act("Invert", "object.select_all", vec![("action", Value::Enum(3))]),
                ];
                tools.insert(0, tool(Icon::Plus, false, "wm.call_menu", vec![("menu", Value::Str("add".into()))]));
                (
                    "Scene".to_owned(),
                    vec![
                        Tab { label: "Add".into(), items: add_items() },
                        Tab { label: "View".into(), items: view_items() },
                        Tab { label: "Select".into(), items: select },
                    ],
                    Width::Narrow,
                )
            }
            MenuContext::Object(id) => {
                let obj = doc.objects.get(id);
                let name = obj.map_or("Object".to_owned(), |o| o.name.clone());
                let is_mesh = obj.is_some_and(|o| o.kind == DataKind::Mesh);
                let mut actions = vec![
                    act("Duplicate", "object.duplicate", vec![]),
                    act("Delete", "object.delete", vec![]),
                    Item::Separator,
                ];
                if is_mesh {
                    actions.push(act("Edit Mode", "object.mode_set", vec![("mode", Value::Enum(1))]));
                    actions.push(act("Shade Smooth", "object.shade", vec![("smooth", Value::Bool(true))]));
                    actions.push(act("Shade Flat", "object.shade", vec![("smooth", Value::Bool(false))]));
                    actions.push(Item::Separator);
                    tools.push(tool(Icon::EditMode, false, "object.mode_set", vec![("mode", Value::Enum(1))]));
                }
                if let Some(p) = op_panel(exec, "object.rename") {
                    actions.push(p);
                }
                let transform = vec![Item::ObjectProps(id)];
                (name, vec![Tab { label: "Object".into(), items: actions }, Tab { label: "Transform".into(), items: transform }], Width::Wide)
            }
            MenuContext::Mesh(mesh) | MenuContext::Element { mesh, .. } => {
                let mode = doc.scene().map_or(SelectMode::Vertex, |s| s.tool.select_mode);
                let selected = doc.meshes.get(mesh).map_or(0, |b| {
                    let m = &b.mesh;
                    let sel = m.vert_attrs().bools(prism_mesh::tables::V_SELECT);
                    m.verts().filter(|v| sel[v.idx()]).count()
                });
                let title = match context {
                    MenuContext::Element { kind, .. } => format!("{} · {selected} selected", kind.label()),
                    _ => format!("Mesh · {selected} selected"),
                };
                // Verbs only: each runs with its defaults, and the Properties
                // editor's "Adjust Last Operation" holds the knobs afterwards.
                let mut edit: Vec<Item> = vec![
                    act("Extrude", "mesh.extrude", vec![]),
                    act("Subdivide", "mesh.subdivide", vec![]),
                    act("Merge by Distance", "mesh.merge_by_distance", vec![]),
                    Item::Separator,
                ];
                edit.push(Item::Sub {
                    label: "Delete".into(),
                    items: vec![
                        act("Vertices", "mesh.delete", vec![("kind", Value::Enum(0))]),
                        act("Edges", "mesh.delete", vec![("kind", Value::Enum(1))]),
                        act("Faces", "mesh.delete", vec![("kind", Value::Enum(2))]),
                        act("Only Faces", "mesh.delete", vec![("kind", Value::Enum(3))]),
                    ],
                });
                edit.push(Item::Sub {
                    label: "Dissolve".into(),
                    items: vec![
                        act("Vertices", "mesh.dissolve", vec![("kind", Value::Enum(0))]),
                        act("Edges", "mesh.dissolve", vec![("kind", Value::Enum(1))]),
                        act("Faces", "mesh.dissolve", vec![("kind", Value::Enum(2))]),
                    ],
                });
                edit.push(act("Flip Normals", "mesh.flip_normals", vec![]));
                edit.push(act("Recalculate Normals", "mesh.normals_make_consistent", vec![]));
                edit.push(Item::Separator);
                edit.push(act("Object Mode", "object.mode_set", vec![("mode", Value::Enum(0))]));
                let select = vec![
                    act("Select All", "mesh.select_all", vec![("action", Value::Enum(1))]),
                    act("Deselect All", "mesh.select_all", vec![("action", Value::Enum(2))]),
                    act("Invert", "mesh.select_all", vec![("action", Value::Enum(3))]),
                ];
                tools.splice(
                    0..0,
                    [
                        tool(Icon::Vertex, mode == SelectMode::Vertex, "mesh.select_mode", vec![("mode", Value::Enum(0))]),
                        tool(Icon::Edge, mode == SelectMode::Edge, "mesh.select_mode", vec![("mode", Value::Enum(1))]),
                        tool(Icon::Face, mode == SelectMode::Face, "mesh.select_mode", vec![("mode", Value::Enum(2))]),
                    ],
                );
                (title, vec![Tab { label: "Edit".into(), items: edit }, Tab { label: "Select".into(), items: select }], Width::Wide)
            }
        };
        ContextMenu { context, title, tabs, tab: 0, tools, pos, width, open_sub: None, height: 0.0 }
    }

    /// The menu for whatever the viewport pick returned.
    pub fn context_for(doc: &Doc, result: prism_viewport::PickResult) -> MenuContext {
        let editing = doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit);
        let mesh = doc.active_object().filter(|_| editing).map(|o| o.data);
        match (result, mesh) {
            (prism_viewport::PickResult::Object(id), None) => MenuContext::Object(id),
            (prism_viewport::PickResult::Vert(m, _), Some(_)) => MenuContext::Element { mesh: m, kind: SelectMode::Vertex },
            (prism_viewport::PickResult::Edge(m, _), Some(_)) => MenuContext::Element { mesh: m, kind: SelectMode::Edge },
            (prism_viewport::PickResult::Face(m, _), Some(_)) => MenuContext::Element { mesh: m, kind: SelectMode::Face },
            (_, Some(m)) => MenuContext::Mesh(m),
            _ => MenuContext::Scene,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_match_their_context() {
        let doc = Doc::starter();
        let exec = Executor::with_builtins();
        let scene = ContextMenu::build(MenuContext::Scene, &doc, &exec, Vec2::ZERO, ViewFlags::default());
        assert_eq!(scene.tabs.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), vec!["Add", "View", "Select"]);
        assert_eq!(scene.tabs[0].items.len(), 6);
        assert!(scene.tools.iter().any(|t| t.icon == Icon::Plus));

        let cube = doc.scene_objects()[0];
        let obj = ContextMenu::build(MenuContext::Object(cube), &doc, &exec, Vec2::ZERO, ViewFlags::default());
        assert_eq!(obj.title, "Cube");
        assert_eq!(obj.width, Width::Wide);
        assert!(obj.tabs[0].items.iter().any(|i| matches!(i, Item::Action { op, .. } if op == "object.mode_set")));
        assert!(obj.tabs[0].items.iter().any(|i| matches!(i, Item::OpPanel { op, .. } if op == "object.rename")));
        assert!(matches!(obj.tabs[1].items[0], Item::ObjectProps(id) if id == cube));
        assert!(obj.tools.iter().any(|t| t.icon == Icon::EditMode));

        let mesh_id = doc.objects.get(cube).unwrap().data;
        let el = ContextMenu::build(MenuContext::Element { mesh: mesh_id, kind: SelectMode::Face }, &doc, &exec, Vec2::ZERO, ViewFlags::default());
        assert!(el.title.starts_with("Face"));
        assert_eq!(el.tools[0].icon, Icon::Vertex);
        assert!(el.tools[0].active, "vertex mode is the scene default");
        assert!(!el.tabs[0].items.iter().any(|i| matches!(i, Item::OpPanel { .. })), "knobs live in the Properties editor");
        for op in ["mesh.extrude", "mesh.subdivide", "mesh.merge_by_distance"] {
            assert!(el.tabs[0].items.iter().any(|i| matches!(i, Item::Action { op: o, .. } if o == op)), "{op} is a plain action");
        }
        assert!(el.tabs[0].items.iter().any(|i| matches!(i, Item::Sub { label, items } if label == "Delete" && items.len() == 4)));
    }

    #[test]
    fn context_from_picks() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        assert_eq!(ContextMenu::context_for(&doc, prism_viewport::PickResult::Nothing), MenuContext::Scene);
        assert_eq!(ContextMenu::context_for(&doc, prism_viewport::PickResult::Object(cube)), MenuContext::Object(cube));
        // In edit mode, empty space is the mesh menu and a face is an element menu.
        doc.scene_mut().unwrap().active_object = cube;
        doc.objects.get_mut(cube).unwrap().mode = ObjectMode::Edit;
        let mesh = doc.objects.get(cube).unwrap().data;
        assert_eq!(ContextMenu::context_for(&doc, prism_viewport::PickResult::Nothing), MenuContext::Mesh(mesh));
        let f = doc.meshes.get(mesh).unwrap().mesh.faces().next().unwrap();
        assert_eq!(ContextMenu::context_for(&doc, prism_viewport::PickResult::Face(mesh, f)), MenuContext::Element { mesh, kind: SelectMode::Face });
    }
}
