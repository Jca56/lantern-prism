//! Type-erased operators, keyed by id, searchable for the palette.

use std::any::Any;
use std::collections::HashMap;

use prism_props::{Reflect, TypeInfo};

use crate::context::{Ctx, Flow, Outcome};
use crate::input::Event;
use crate::operator::{OpError, OpFlags, OpResult, Operator};

pub struct OpInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub flags: OpFlags,
    pub props_info: fn() -> &'static TypeInfo,
    new_props: fn() -> Box<dyn Reflect>,
    new_modal: fn() -> Box<dyn Any>,
    poll: fn(&Ctx) -> bool,
    exec: fn(&mut Ctx, &dyn Reflect) -> OpResult<Outcome>,
    invoke: fn(&mut Ctx, &mut dyn Reflect, &Event, &mut dyn Any) -> OpResult<Flow>,
    modal: fn(&mut dyn Any, &mut Ctx, &mut dyn Reflect, &Event) -> OpResult<Flow>,
}

fn props_of<T: Operator>(p: &dyn Reflect) -> OpResult<&T::Props> {
    p.downcast_ref::<T::Props>().ok_or_else(|| OpError::Failed(format!("{}: wrong props type", T::ID)))
}

fn props_of_mut<T: Operator>(p: &mut dyn Reflect) -> OpResult<&mut T::Props> {
    p.downcast_mut::<T::Props>().ok_or_else(|| OpError::Failed(format!("{}: wrong props type", T::ID)))
}

fn modal_of<T: Operator>(m: &mut dyn Any) -> OpResult<&mut T::Modal> {
    m.downcast_mut::<T::Modal>().ok_or_else(|| OpError::Failed(format!("{}: wrong modal state", T::ID)))
}

impl OpInfo {
    fn of<T: Operator>() -> Self
    where
        T::Props: prism_props::ReflectStatic,
    {
        Self {
            id: T::ID,
            label: T::LABEL,
            flags: T::FLAGS,
            props_info: <T::Props as prism_props::ReflectStatic>::info,
            new_props: || Box::new(T::Props::default()),
            new_modal: || Box::new(T::Modal::default()),
            poll: T::poll,
            exec: |ctx, p| T::exec(ctx, props_of::<T>(p)?),
            invoke: |ctx, p, ev, m| T::invoke(ctx, props_of_mut::<T>(p)?, ev, modal_of::<T>(m)?),
            modal: |m, ctx, p, ev| T::modal(modal_of::<T>(m)?, ctx, props_of_mut::<T>(p)?, ev),
        }
    }

    pub fn new_props(&self) -> Box<dyn Reflect> {
        (self.new_props)()
    }
    pub fn new_modal(&self) -> Box<dyn Any> {
        (self.new_modal)()
    }
    pub fn poll(&self, ctx: &Ctx) -> bool {
        (self.poll)(ctx)
    }
    pub fn exec(&self, ctx: &mut Ctx, props: &dyn Reflect) -> OpResult<Outcome> {
        (self.exec)(ctx, props)
    }
    pub fn invoke(&self, ctx: &mut Ctx, props: &mut dyn Reflect, ev: &Event, modal: &mut dyn Any) -> OpResult<Flow> {
        (self.invoke)(ctx, props, ev, modal)
    }
    pub fn modal(&self, modal: &mut dyn Any, ctx: &mut Ctx, props: &mut dyn Reflect, ev: &Event) -> OpResult<Flow> {
        (self.modal)(modal, ctx, props, ev)
    }
    pub fn category(&self) -> &'static str {
        self.id.split('.').next().unwrap_or(self.id)
    }
}

#[derive(Default)]
pub struct Registry {
    ops: Vec<OpInfo>,
    index: HashMap<&'static str, usize>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Operator>(&mut self)
    where
        T::Props: prism_props::ReflectStatic,
    {
        if self.index.contains_key(T::ID) {
            return;
        }
        self.index.insert(T::ID, self.ops.len());
        self.ops.push(OpInfo::of::<T>());
    }

    pub fn get(&self, id: &str) -> Option<&OpInfo> {
        self.index.get(id).map(|&i| &self.ops[i])
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &OpInfo> {
        self.ops.iter()
    }

    /// Registered operators whose label or id matches `query` (subsequence,
    /// case-insensitive), best matches first. Empty query lists everything.
    pub fn search(&self, query: &str) -> Vec<&OpInfo> {
        let q: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
        let mut scored: Vec<(i64, &OpInfo)> = self
            .ops
            .iter()
            .filter(|o| o.flags.contains(OpFlags::REGISTER))
            .filter_map(|o| {
                let s = score(&q, &o.label.to_lowercase()).or_else(|| score(&q, o.id).map(|s| s - 100))?;
                Some((s, o))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(b.1.label)));
        scored.into_iter().map(|(_, o)| o).collect()
    }
}

/// Subsequence match score: higher is better, `None` if not all chars found.
fn score(query: &[char], text: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let mut s = 0i64;
    let mut qi = 0;
    let mut prev_hit = false;
    for (i, c) in text.chars().enumerate() {
        if qi < query.len() && c == query[qi] {
            s += if prev_hit { 3 } else { 1 };
            if i == 0 {
                s += 5;
            }
            qi += 1;
            prev_hit = true;
        } else {
            prev_hit = false;
        }
    }
    (qi == query.len()).then_some(s - text.len() as i64 / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring() {
        let q: Vec<char> = "ext".chars().collect();
        assert!(score(&q, "extrude").unwrap() > score(&q, "exit textures").unwrap());
        assert!(score(&q, "nope").is_none());
        assert_eq!(score(&[], "anything"), Some(0));
    }
}
