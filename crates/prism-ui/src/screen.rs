//! The retained layout: a binary tree of splits whose leaves are areas. Each
//! area hosts one editor and has a header and a body. Changes only when the
//! user splits, joins or drags a separator.

use prism_math::{Rect, Vec2};

use crate::editors::EditorKind;

pub type NodeId = usize;
pub type AreaId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Children side by side (a vertical separator).
    Horizontal,
    /// Children stacked (a horizontal separator).
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Split { axis: Axis, ratio: f64, children: [NodeId; 2] },
    Leaf(AreaId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub editor: EditorKind,
    /// Camera and display settings when this area is a 3D viewport.
    pub viewport: prism_viewport::ViewportState,
}

/// Where an area landed this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaLayout {
    pub area: AreaId,
    pub rect: Rect,
    pub header: Rect,
    pub body: Rect,
}

/// A draggable gap between two siblings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Separator {
    pub node: NodeId,
    pub axis: Axis,
    /// The visible gap.
    pub gap: Rect,
    /// The grab zone (wider than the gap).
    pub grab: Rect,
}

/// Smallest side an area may be dragged down to, in header heights.
const MIN_AREA_HEADERS: f64 = 2.0;

#[derive(Clone, Debug)]
pub struct Screen {
    nodes: Vec<Option<Node>>,
    areas: Vec<Option<Area>>,
    root: NodeId,
    /// The area that receives keyboard input (D017).
    pub active: Option<AreaId>,
    layouts: Vec<AreaLayout>,
    separators: Vec<Separator>,
    node_rects: Vec<Rect>,
}

impl Screen {
    /// One area filling the window.
    pub fn new(editor: EditorKind) -> Self {
        Self {
            nodes: vec![Some(Node::Leaf(0))],
            areas: vec![Some(Area { editor, viewport: Default::default() })],
            root: 0,
            active: Some(0),
            layouts: Vec::new(),
            separators: Vec::new(),
            node_rects: Vec::new(),
        }
    }

    pub fn area_count(&self) -> usize {
        self.areas.iter().flatten().count()
    }

    pub fn area(&self, id: AreaId) -> Option<&Area> {
        self.areas.get(id).and_then(Option::as_ref)
    }

    pub fn area_mut(&mut self, id: AreaId) -> Option<&mut Area> {
        self.areas.get_mut(id).and_then(Option::as_mut)
    }

    fn alloc_node(&mut self, n: Node) -> NodeId {
        if let Some(i) = self.nodes.iter().position(Option::is_none) {
            self.nodes[i] = Some(n);
            i
        } else {
            self.nodes.push(Some(n));
            self.nodes.len() - 1
        }
    }

    fn alloc_area(&mut self, a: Area) -> AreaId {
        if let Some(i) = self.areas.iter().position(Option::is_none) {
            self.areas[i] = Some(a);
            i
        } else {
            self.areas.push(Some(a));
            self.areas.len() - 1
        }
    }

    fn leaf_of(&self, area: AreaId) -> Option<NodeId> {
        self.nodes.iter().position(|n| matches!(n, Some(Node::Leaf(a)) if *a == area))
    }

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.iter().position(|n| matches!(n, Some(Node::Split { children, .. }) if children.contains(&node)))
    }

    /// Split `area` along `axis`. The existing area keeps the first `ratio`
    /// of the space; the new area (with `editor`) takes the rest.
    pub fn split(&mut self, area: AreaId, axis: Axis, ratio: f64, editor: EditorKind) -> Option<AreaId> {
        let leaf = self.leaf_of(area)?;
        let new_area = self.alloc_area(Area { editor, viewport: Default::default() });
        let first = self.alloc_node(Node::Leaf(area));
        let second = self.alloc_node(Node::Leaf(new_area));
        self.nodes[leaf] = Some(Node::Split { axis, ratio: ratio.clamp(0.1, 0.9), children: [first, second] });
        Some(new_area)
    }

    /// Remove `area`; its sibling takes the parent's place. The last area
    /// cannot be removed.
    pub fn join(&mut self, area: AreaId) -> bool {
        let Some(leaf) = self.leaf_of(area) else {
            return false;
        };
        let Some(parent) = self.parent_of(leaf) else {
            return false; // root leaf
        };
        let Some(Node::Split { children, .. }) = self.nodes[parent].clone() else {
            return false;
        };
        let sibling = if children[0] == leaf { children[1] } else { children[0] };
        let sibling_node = self.nodes[sibling].take();
        self.nodes[parent] = sibling_node;
        self.nodes[leaf] = None;
        self.areas[area] = None;
        if self.active == Some(area) {
            self.active = self.areas.iter().position(Option::is_some);
        }
        true
    }

    pub fn set_ratio(&mut self, node: NodeId, ratio: f64) {
        if let Some(Node::Split { ratio: r, .. }) = self.nodes.get_mut(node).and_then(Option::as_mut) {
            *r = ratio.clamp(0.02, 0.98);
        }
    }

    /// Compute every area's rects for a window `rect`.
    pub fn layout(&mut self, rect: Rect, header_h: f64, sep: f64) {
        self.layouts.clear();
        self.separators.clear();
        self.node_rects.clear();
        self.node_rects.resize(self.nodes.len(), Rect::ZERO);
        let root = self.root;
        self.layout_node(root, rect, header_h, sep);
    }

    fn layout_node(&mut self, node: NodeId, rect: Rect, header_h: f64, sep: f64) {
        self.node_rects[node] = rect;
        match self.nodes[node].clone() {
            Some(Node::Leaf(area)) => {
                let (header, body) = rect.take_top(header_h.min(rect.height()));
                self.layouts.push(AreaLayout { area, rect, header: header.round(), body: body.round() });
            }
            Some(Node::Split { axis, ratio, children }) => {
                let grab = sep.max(1.0) + 2.0 * header_h * 0.15;
                match axis {
                    Axis::Horizontal => {
                        let x = (rect.min.x + (rect.width() - sep) * ratio).round();
                        let (a, rest) = rect.split_x(x);
                        let (gap, b) = rest.take_left(sep);
                        self.separators.push(Separator {
                            node,
                            axis,
                            gap,
                            grab: Rect::new(Vec2::new(gap.min.x - grab, gap.min.y), Vec2::new(gap.max.x + grab, gap.max.y)),
                        });
                        self.layout_node(children[0], a, header_h, sep);
                        self.layout_node(children[1], b, header_h, sep);
                    }
                    Axis::Vertical => {
                        let y = (rect.min.y + (rect.height() - sep) * ratio).round();
                        let (a, rest) = rect.split_y(y);
                        let (gap, b) = rest.take_top(sep);
                        self.separators.push(Separator {
                            node,
                            axis,
                            gap,
                            grab: Rect::new(Vec2::new(gap.min.x, gap.min.y - grab), Vec2::new(gap.max.x, gap.max.y + grab)),
                        });
                        self.layout_node(children[0], a, header_h, sep);
                        self.layout_node(children[1], b, header_h, sep);
                    }
                }
            }
            None => {}
        }
    }

    pub fn layouts(&self) -> &[AreaLayout] {
        &self.layouts
    }

    pub fn separators(&self) -> &[Separator] {
        &self.separators
    }

    pub fn layout_of(&self, area: AreaId) -> Option<&AreaLayout> {
        self.layouts.iter().find(|l| l.area == area)
    }

    pub fn area_at(&self, p: Vec2) -> Option<AreaId> {
        self.layouts.iter().find(|l| l.rect.contains(p)).map(|l| l.area)
    }

    /// Index into [`Self::separators`] under `p`, if any.
    pub fn separator_at(&self, p: Vec2) -> Option<usize> {
        self.separators.iter().position(|s| s.grab.contains(p))
    }

    /// Move separator `idx` so it sits at the pointer, keeping both sides at
    /// least `min_px` wide. Call [`Self::layout`] again afterwards.
    pub fn drag_separator(&mut self, idx: usize, pointer: Vec2, min_px: f64) {
        let Some(sep) = self.separators.get(idx).copied() else {
            return;
        };
        let rect = self.node_rects[sep.node];
        let ratio = match sep.axis {
            Axis::Horizontal => {
                let x = pointer.x.clamp(rect.min.x + min_px, rect.max.x - min_px);
                (x - rect.min.x) / rect.width().max(1.0)
            }
            Axis::Vertical => {
                let y = pointer.y.clamp(rect.min.y + min_px, rect.max.y - min_px);
                (y - rect.min.y) / rect.height().max(1.0)
            }
        };
        self.set_ratio(sep.node, ratio);
    }

    /// Minimum side length for dragging, given the header height.
    pub fn min_area_px(header_h: f64) -> f64 {
        header_h * MIN_AREA_HEADERS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> Rect {
        Rect::from_xywh(0.0, 0.0, 1000.0, 600.0)
    }

    #[test]
    fn single_area_fills_window() {
        let mut s = Screen::new(EditorKind::Empty);
        s.layout(win(), 45.0, 5.0);
        assert_eq!(s.layouts().len(), 1);
        let l = s.layouts()[0];
        assert_eq!(l.rect, win());
        assert_eq!(l.header.height(), 45.0);
        assert_eq!(l.body.min.y, 45.0);
        assert!(s.separators().is_empty());
        assert!(!s.join(0), "cannot remove the last area");
    }

    #[test]
    fn split_join_and_drag() {
        let mut s = Screen::new(EditorKind::Empty);
        let right = s.split(0, Axis::Horizontal, 0.6, EditorKind::Preferences).unwrap();
        assert_eq!(s.area_count(), 2);
        s.layout(win(), 45.0, 10.0);
        let l0 = *s.layout_of(0).unwrap();
        let l1 = *s.layout_of(right).unwrap();
        assert_eq!(l0.rect.width(), 594.0, "60% of (1000 - gap)");
        assert_eq!(l1.rect.min.x, 604.0);
        assert_eq!(l1.rect.max.x, 1000.0);
        assert_eq!(s.separators().len(), 1);
        assert_eq!(s.separators()[0].gap, Rect::from_xywh(594.0, 0.0, 10.0, 600.0));
        assert_eq!(s.area_at(Vec2::new(700.0, 10.0)), Some(right));
        assert_eq!(s.separator_at(Vec2::new(598.0, 300.0)), Some(0));
        assert_eq!(s.separator_at(Vec2::new(100.0, 300.0)), None);

        s.drag_separator(0, Vec2::new(300.0, 0.0), 90.0);
        s.layout(win(), 45.0, 10.0);
        assert!((s.layout_of(0).unwrap().rect.width() - 297.0).abs() <= 1.0);
        // Clamped to the minimum.
        s.drag_separator(0, Vec2::new(0.0, 0.0), 90.0);
        s.layout(win(), 45.0, 10.0);
        assert!(s.layout_of(0).unwrap().rect.width() >= 80.0);

        // Nested split of the right area, then join it away again.
        let bottom = s.split(right, Axis::Vertical, 0.5, EditorKind::Gallery).unwrap();
        s.layout(win(), 45.0, 10.0);
        assert_eq!(s.layouts().len(), 3);
        assert_eq!(s.separators().len(), 2);
        s.active = Some(bottom);
        assert!(s.join(bottom));
        assert_eq!(s.area_count(), 2);
        assert_ne!(s.active, Some(bottom));
        s.layout(win(), 45.0, 10.0);
        assert_eq!(s.layouts().len(), 2);
        assert!(s.join(right));
        s.layout(win(), 45.0, 10.0);
        assert_eq!(s.layouts()[0].rect, win(), "sibling took the whole window back");
        assert!(s.separators().is_empty());
    }
}
