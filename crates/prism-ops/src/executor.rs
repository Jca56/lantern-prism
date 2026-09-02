//! Runs operators transactionally: snapshot, execute, record undo, or roll
//! back on error. Also drives modal operators and adjust-last-operation.

use std::any::Any;

use prism_doc::{Doc, History, UndoStep};
use prism_props::{Reflect, Value};

use crate::context::{Ctx, Flow, Outcome, UiRequest};
use crate::input::Event;
use crate::operator::{OpError, OpFlags, OpResult};
use crate::registry::Registry;

pub struct RunningModal {
    pub op: &'static str,
    props: Box<dyn Reflect>,
    state: Box<dyn Any>,
    before: Doc,
}

pub struct Executor {
    pub registry: Registry,
    pub history: History,
    running: Option<RunningModal>,
    /// Requests operators made during the last call, for the UI to act on.
    pub requests: Vec<UiRequest>,
    pub last_report: Option<String>,
    saved_revision: u64,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(Registry::new())
    }
}

impl Executor {
    pub fn new(registry: Registry) -> Self {
        Self { registry, history: History::default(), running: None, requests: Vec::new(), last_report: None, saved_revision: 0 }
    }

    /// Every built-in operator registered.
    pub fn with_builtins() -> Self {
        let mut r = Registry::new();
        crate::builtin::register_all(&mut r);
        Self::new(r)
    }

    /// The document on disk matches the current history position.
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.history.revision();
    }

    pub fn is_dirty(&self) -> bool {
        self.history.revision() != self.saved_revision
    }

    pub fn is_modal(&self) -> bool {
        self.running.is_some()
    }

    pub fn running(&self) -> Option<&RunningModal> {
        self.running.as_ref()
    }

    fn take_requests(&mut self, ctx: &mut Ctx, doc_for_undo: bool) {
        self.last_report = ctx.report.take().or(self.last_report.take());
        for r in ctx.requests.drain(..) {
            match r {
                UiRequest::Undo if doc_for_undo => {
                    if let Some(d) = self.history.undo() {
                        *ctx.doc = d;
                    }
                }
                UiRequest::Redo if doc_for_undo => {
                    if let Some(d) = self.history.redo() {
                        *ctx.doc = d;
                    }
                }
                UiRequest::HistoryClear => self.history.clear(),
                other => self.requests.push(other),
            }
        }
    }

    fn record(&mut self, before: Doc, ctx: &Ctx, id: &str, label: &str, props: Box<dyn Reflect>) {
        self.history.push(UndoStep {
            before,
            after: ctx.doc.clone(),
            label: label.to_owned(),
            op_id: id.to_owned(),
            props: Some(props),
        });
    }

    /// Execute `id` with `props` (defaults when `None`).
    pub fn run(&mut self, id: &str, props: Option<Box<dyn Reflect>>, ctx: &mut Ctx) -> OpResult<Outcome> {
        if self.running.is_some() {
            return Err(OpError::Busy);
        }
        let info = self.registry.get(id).ok_or_else(|| OpError::Unknown(id.to_owned()))?;
        if !info.poll(ctx) {
            return Err(OpError::Poll(id.to_owned()));
        }
        let props = props.unwrap_or_else(|| info.new_props());
        let (op_id, label, flags) = (info.id, info.label, info.flags);
        let before = ctx.doc.clone();
        let result = info.exec(ctx, &*props);
        match result {
            Ok(Outcome::Finished) => {
                if flags.contains(OpFlags::UNDO) {
                    self.record(before, ctx, op_id, label, props);
                }
                self.take_requests(ctx, true);
                Ok(Outcome::Finished)
            }
            Ok(other) => {
                self.take_requests(ctx, true);
                Ok(other)
            }
            Err(e) => {
                *ctx.doc = before;
                ctx.requests.clear();
                self.last_report = Some(format!("{label}: {e}"));
                Err(e)
            }
        }
    }

    /// Execute with default props plus named overrides.
    pub fn run_with(&mut self, id: &str, overrides: &[(&str, Value)], ctx: &mut Ctx) -> OpResult<Outcome> {
        let info = self.registry.get(id).ok_or_else(|| OpError::Unknown(id.to_owned()))?;
        let mut props = info.new_props();
        for (name, v) in overrides {
            props.set_by_name(name, v.clone()).map_err(|e| OpError::Failed(e.to_string()))?;
        }
        self.run(id, Some(props), ctx)
    }

    /// Start an operator interactively from `event`. A modal operator stays
    /// running until [`Self::modal_event`] reports it finished.
    pub fn invoke(&mut self, id: &str, props: Option<Box<dyn Reflect>>, ctx: &mut Ctx, event: &Event) -> OpResult<Flow> {
        if self.running.is_some() {
            return Err(OpError::Busy);
        }
        let info = self.registry.get(id).ok_or_else(|| OpError::Unknown(id.to_owned()))?;
        if !info.poll(ctx) {
            return Err(OpError::Poll(id.to_owned()));
        }
        let mut props = props.unwrap_or_else(|| info.new_props());
        let mut state = info.new_modal();
        let (op_id, label, flags) = (info.id, info.label, info.flags);
        let before = ctx.doc.clone();
        match info.invoke(ctx, &mut *props, event, &mut *state) {
            Ok(Flow::Running) => {
                self.running = Some(RunningModal { op: op_id, props, state, before });
                self.take_requests(ctx, false);
                Ok(Flow::Running)
            }
            Ok(Flow::Finished) => {
                if flags.contains(OpFlags::UNDO) {
                    self.record(before, ctx, op_id, label, props);
                }
                self.take_requests(ctx, true);
                Ok(Flow::Finished)
            }
            Ok(other) => {
                *ctx.doc = before;
                self.take_requests(ctx, true);
                Ok(other)
            }
            Err(e) => {
                *ctx.doc = before;
                ctx.requests.clear();
                self.last_report = Some(format!("{label}: {e}"));
                Err(e)
            }
        }
    }

    /// Feed an event to the running modal operator. `None` if none runs.
    pub fn modal_event(&mut self, ctx: &mut Ctx, event: &Event) -> Option<OpResult<Flow>> {
        let mut running = self.running.take()?;
        let info = self.registry.get(running.op)?;
        let (op_id, label, flags) = (info.id, info.label, info.flags);
        let result = info.modal(&mut *running.state, ctx, &mut *running.props, event);
        match &result {
            Ok(Flow::Running) | Ok(Flow::PassThrough) => {
                self.running = Some(running);
                self.take_requests(ctx, false);
            }
            Ok(Flow::Finished) => {
                if flags.contains(OpFlags::UNDO) {
                    self.record(running.before, ctx, op_id, label, running.props);
                }
                self.take_requests(ctx, true);
            }
            Ok(Flow::Cancelled) | Err(_) => {
                *ctx.doc = running.before;
                ctx.requests.clear();
                if let Err(e) = &result {
                    self.last_report = Some(format!("{label}: {e}"));
                }
            }
        }
        Some(result)
    }

    /// Cancel the running modal operator, restoring the document.
    pub fn cancel_modal(&mut self, doc: &mut Doc) -> bool {
        match self.running.take() {
            Some(r) => {
                *doc = r.before;
                true
            }
            None => false,
        }
    }

    pub fn undo(&mut self, doc: &mut Doc) -> bool {
        match self.history.undo() {
            Some(d) => {
                *doc = d;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, doc: &mut Doc) -> bool {
        match self.history.redo() {
            Some(d) => {
                *doc = d;
                true
            }
            None => false,
        }
    }

    /// The last step's operator and its (editable) properties.
    pub fn last_step_props(&mut self) -> Option<(&'static str, &mut Box<dyn Reflect>)> {
        let step = self.history.last_mut()?;
        let id = self.registry.get(&step.op_id)?.id;
        step.props.as_mut().map(|p| (id, p))
    }

    /// Re-run the last operator from its `before` snapshot with the props
    /// currently stored on the step (edit them via [`Self::last_step_props`]).
    pub fn adjust_last(&mut self, ctx: &mut Ctx) -> OpResult<()> {
        let (before, op_id, props) = {
            let step = self.history.last().ok_or_else(|| OpError::Failed("nothing to adjust".into()))?;
            let props = step.props.clone().ok_or_else(|| OpError::Failed("last step has no properties".into()))?;
            (step.before.clone(), step.op_id.clone(), props)
        };
        let info = self.registry.get(&op_id).ok_or_else(|| OpError::Unknown(op_id.clone()))?;
        let previous_after = ctx.doc.clone();
        *ctx.doc = before;
        match info.exec(ctx, &*props) {
            Ok(_) => {
                let after = ctx.doc.clone();
                if let Some(step) = self.history.last_mut() {
                    step.after = after;
                    step.props = Some(props);
                }
                self.take_requests(ctx, false);
                Ok(())
            }
            Err(e) => {
                *ctx.doc = previous_after;
                ctx.requests.clear();
                Err(e)
            }
        }
    }
}
