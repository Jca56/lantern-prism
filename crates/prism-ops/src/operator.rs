//! The `Operator` trait (D007).

use core::fmt;

use prism_props::Reflect;

use crate::context::{Ctx, Flow, Outcome};
use crate::input::Event;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct OpFlags(pub u32);

impl OpFlags {
    pub const NONE: OpFlags = OpFlags(0);
    /// Listed in menus and the palette.
    pub const REGISTER: OpFlags = OpFlags(1);
    /// Records an undo step when it finishes.
    pub const UNDO: OpFlags = OpFlags(2);
    /// While modal, takes every event.
    pub const BLOCKING: OpFlags = OpFlags(4);
    pub const DEFAULT: OpFlags = OpFlags(1 | 2);

    pub const fn contains(self, o: OpFlags) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for OpFlags {
    type Output = OpFlags;
    fn bitor(self, o: OpFlags) -> OpFlags {
        OpFlags(self.0 | o.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpError {
    /// `poll` said no.
    Poll(String),
    Unknown(String),
    Failed(String),
    /// A running modal operator owns input right now.
    Busy,
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Poll(id) => write!(f, "`{id}` cannot run here"),
            OpError::Unknown(id) => write!(f, "unknown operator `{id}`"),
            OpError::Failed(m) => write!(f, "{m}"),
            OpError::Busy => write!(f, "a modal operator is running"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<prism_mesh::EulerError> for OpError {
    fn from(e: prism_mesh::EulerError) -> Self {
        OpError::Failed(e.to_string())
    }
}

impl From<prism_doc::FileError> for OpError {
    fn from(e: prism_doc::FileError) -> Self {
        OpError::Failed(e.to_string())
    }
}

impl From<prism_doc::ObjError> for OpError {
    fn from(e: prism_doc::ObjError) -> Self {
        OpError::Failed(e.to_string())
    }
}

pub type OpResult<T> = Result<T, OpError>;

/// One user-facing verb. `exec` is the whole story for most operators;
/// interactive ones override `invoke` (start) and `modal` (each event).
pub trait Operator: 'static {
    /// `"category.name"`, e.g. `"mesh.extrude"`.
    const ID: &'static str;
    const LABEL: &'static str;
    const FLAGS: OpFlags = OpFlags::DEFAULT;
    type Props: Reflect + Default + Clone;
    /// Per-invocation state for modal operators; `()` when not needed.
    type Modal: Default + 'static;

    /// May this operator run in this context, right now?
    fn poll(_ctx: &Ctx) -> bool {
        true
    }

    fn exec(ctx: &mut Ctx, props: &Self::Props) -> OpResult<Outcome>;

    fn invoke(ctx: &mut Ctx, props: &mut Self::Props, _event: &Event, _modal: &mut Self::Modal) -> OpResult<Flow> {
        Self::exec(ctx, props).map(Flow::from)
    }

    fn modal(_modal: &mut Self::Modal, _ctx: &mut Ctx, _props: &mut Self::Props, _event: &Event) -> OpResult<Flow> {
        Ok(Flow::Finished)
    }
}
