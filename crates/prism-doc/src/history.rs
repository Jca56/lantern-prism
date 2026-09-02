//! Undo as time travel: each step keeps the document before and after an
//! operator ran. Snapshots share everything they did not change, so a step
//! costs only the chunks it touched.

use std::collections::HashSet;

use prism_props::Reflect;

use crate::doc::Doc;

pub struct UndoStep {
    pub before: Doc,
    pub after: Doc,
    pub label: String,
    pub op_id: String,
    /// The operator's properties, for "adjust last operation".
    pub props: Option<Box<dyn Reflect>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryStats {
    pub steps: usize,
    pub cursor: usize,
    /// Distinct mesh chunks referenced by every snapshot, times chunk bytes.
    pub unique_mesh_bytes: usize,
}

pub struct History {
    steps: Vec<UndoStep>,
    /// Number of steps currently applied (0..=steps.len()).
    cursor: usize,
    budget: usize,
    /// Bumps on every push, undo and redo: a cheap "did anything change".
    revision: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::new(256)
    }
}

impl History {
    pub fn new(budget: usize) -> Self {
        Self { steps: Vec::new(), cursor: 0, budget: budget.max(1), revision: 0 }
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget.max(1);
        self.trim();
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.steps.len()
    }

    /// Record a step; anything after the cursor (redo tail) is dropped.
    pub fn push(&mut self, step: UndoStep) {
        self.steps.truncate(self.cursor);
        self.steps.push(step);
        self.cursor = self.steps.len();
        self.revision += 1;
        self.trim();
    }

    fn trim(&mut self) {
        while self.steps.len() > self.budget {
            self.steps.remove(0);
            self.cursor = self.cursor.saturating_sub(1);
        }
    }

    /// The document as it was before the last applied step.
    pub fn undo(&mut self) -> Option<Doc> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.revision += 1;
        Some(self.steps[self.cursor].before.clone())
    }

    /// The document as it was after the next unapplied step.
    pub fn redo(&mut self) -> Option<Doc> {
        if self.cursor >= self.steps.len() {
            return None;
        }
        let doc = self.steps[self.cursor].after.clone();
        self.cursor += 1;
        self.revision += 1;
        Some(doc)
    }

    /// The last applied step, for adjusting.
    pub fn last(&self) -> Option<&UndoStep> {
        self.cursor.checked_sub(1).map(|i| &self.steps[i])
    }

    pub fn last_mut(&mut self) -> Option<&mut UndoStep> {
        self.cursor.checked_sub(1).map(|i| &mut self.steps[i])
    }

    /// Labels of every step, oldest first, with the applied count.
    pub fn labels(&self) -> (Vec<&str>, usize) {
        (self.steps.iter().map(|s| s.label.as_str()).collect(), self.cursor)
    }

    pub fn clear(&mut self) {
        self.steps.clear();
        self.cursor = 0;
        self.revision += 1;
    }

    /// Memory accounting across every snapshot (both sides of each step).
    pub fn stats(&self) -> HistoryStats {
        let mut chunks: HashSet<usize> = HashSet::new();
        for s in &self.steps {
            for doc in [&s.before, &s.after] {
                for (_, m) in doc.meshes.iter() {
                    m.mesh.chunk_ptrs(&mut chunks);
                }
            }
        }
        HistoryStats {
            steps: self.steps.len(),
            cursor: self.cursor,
            unique_mesh_bytes: chunks.len() * prism_core::CHUNK * 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::Vec3;

    fn step(before: &Doc, after: &Doc, label: &str) -> UndoStep {
        UndoStep { before: before.clone(), after: after.clone(), label: label.into(), op_id: "test".into(), props: None }
    }

    #[test]
    fn undo_redo_walk() {
        let mut h = History::new(10);
        let d0 = Doc::starter();
        let cube = d0.scene_objects()[0];
        let mut d1 = d0.clone();
        d1.objects.get_mut(cube).unwrap().location = Vec3::X;
        let mut d2 = d1.clone();
        d2.objects.get_mut(cube).unwrap().location = Vec3::Y;
        h.push(step(&d0, &d1, "move x"));
        h.push(step(&d1, &d2, "move y"));
        assert!(h.can_undo() && !h.can_redo());
        let back = h.undo().unwrap();
        assert_eq!(back.objects.get(cube).unwrap().location, Vec3::X);
        let back = h.undo().unwrap();
        assert_eq!(back.objects.get(cube).unwrap().location, Vec3::ZERO);
        assert!(h.undo().is_none());
        let fwd = h.redo().unwrap();
        assert_eq!(fwd.objects.get(cube).unwrap().location, Vec3::X);
        // A new step after undoing drops the redo tail.
        h.push(step(&d1, &d0, "back to start"));
        assert!(!h.can_redo());
        assert_eq!(h.labels(), (vec!["move x", "back to start"], 2));
    }

    #[test]
    fn budget_and_sharing() {
        let mut h = History::new(5);
        let base = Doc::starter();
        let cube = base.scene_objects()[0];
        let mut cur = base.clone();
        for i in 0..20 {
            let mut next = cur.clone();
            next.objects.get_mut(cube).unwrap().location = Vec3::X * i as f64;
            h.push(step(&cur, &next, "move"));
            cur = next;
        }
        assert_eq!(h.len(), 5);
        assert_eq!(h.cursor(), 5);
        // Object moves never touch mesh chunks: every snapshot shares them all.
        let stats = h.stats();
        let one_mesh: HashSet<usize> = {
            let mut s = HashSet::new();
            base.object_mesh(cube).unwrap().mesh.chunk_ptrs(&mut s);
            s
        };
        assert_eq!(stats.unique_mesh_bytes, one_mesh.len() * prism_core::CHUNK * 8);
    }
}
