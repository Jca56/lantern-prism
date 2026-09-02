//! Interactive transforms — Move, Rotate, Scale — as modal operators driven
//! by the pointer (D024). A gizmo handle or a menu starts one; it acts on the
//! selected objects, or on the selected vertices of the mesh being edited,
//! in world space, and writes what it did into its props so Adjust Last
//! Operation can replay it.

use core::f64::consts::{PI, TAU};

use prism_math::{Vec2, Vec3};
use prism_props::props;

use crate::context::{Ctx, Flow, Outcome, ViewInfo};
use crate::input::{Event, Key, MouseButton};
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

mod targets;
#[cfg(test)]
pub(crate) mod tests;

use targets::{Op, Targets, apply, has_targets};

/// Modal transforms take every event while they run.
pub(crate) const INTERACTIVE: OpFlags = OpFlags(OpFlags::DEFAULT.0 | OpFlags::BLOCKING.0);

props! {
    /// A world axis to constrain to; `Free` follows the view.
    pub enum Axis {
        Free = 0,
        X = 1,
        Y = 2,
        Z = 3,
    }
}

impl Axis {
    pub fn dir(self) -> Option<Vec3> {
        match self {
            Axis::Free => None,
            Axis::X => Some(Vec3::X),
            Axis::Y => Some(Vec3::Y),
            Axis::Z => Some(Vec3::Z),
        }
    }

    fn from_dir(d: Vec3) -> Axis {
        if d == Vec3::X {
            Axis::X
        } else if d == Vec3::Y {
            Axis::Y
        } else if d == Vec3::Z {
            Axis::Z
        } else {
            Axis::Free
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Axis::Free => "",
            Axis::X => "  [X]",
            Axis::Y => "  [Y]",
            Axis::Z => "  [Z]",
        }
    }
}

// ---- event vocabulary shared with other pointer-driven modals -------------

/// What an event means to a running transform-like operator.
pub(crate) enum Control {
    Move(Vec2),
    Confirm,
    Cancel,
    /// X / Y / Z pressed: toggle that constraint.
    Axis(Axis),
    Ignore,
}

/// `release_confirm`: the operator began with a press (a drag), so the
/// release confirms; otherwise the next click does.
pub(crate) fn control(event: &Event, release_confirm: bool) -> Control {
    match event {
        Event::PointerMoved(p) => Control::Move(*p),
        Event::Button { button: MouseButton::Left, pressed, .. } if *pressed != release_confirm => Control::Confirm,
        Event::Button { button: MouseButton::Right, pressed: true, .. } => Control::Cancel,
        Event::Key { key: Key::Escape, pressed: true, .. } => Control::Cancel,
        Event::Key { key: Key::Enter | Key::Space, pressed: true, .. } => Control::Confirm,
        Event::Key { key: Key::Char(c), pressed: true, .. } => match c.to_ascii_lowercase() {
            'x' => Control::Axis(Axis::X),
            'y' => Control::Axis(Axis::Y),
            'z' => Control::Axis(Axis::Z),
            _ => Control::Ignore,
        },
        _ => Control::Ignore,
    }
}

/// The pointer position an event carries, if any.
pub(crate) fn pointer_of(event: &Event) -> Option<Vec2> {
    match event {
        Event::PointerMoved(p) => Some(*p),
        Event::Button { pos, .. } | Event::Wheel { pos, .. } => Some(*pos),
        _ => None,
    }
}

/// A press starts a drag that ends on release; anything else (a key, a menu
/// click that has already released) is confirmed by the next click.
pub(crate) fn starts_drag(event: &Event) -> bool {
    matches!(event, Event::Button { pressed: true, .. })
}

// ---- the running state ----------------------------------------------------

/// A pointer on the pivot when a scale starts would divide by nothing; treat
/// it as at least this far away (pixels).
const MIN_SCALE_RADIUS: f64 = 20.0;

/// Per-invocation state shared by the three transforms.
#[derive(Default)]
pub struct TransformModal {
    targets: Option<Targets>,
    pivot: Vec3,
    start: Vec2,
    last: Vec2,
    constraint: Option<Vec3>,
    release_confirm: bool,
    /// Rotate: angle swept so far (unwrapped past ±180°) and the last screen
    /// angle seen, so the sweep accumulates across events.
    angle: f64,
    last_angle: f64,
}

enum Step {
    Update,
    Finish,
    Cancel,
    Nothing,
}

fn screen_angle(view: &ViewInfo, pivot: Vec3, p: Vec2) -> f64 {
    let c = view.project(pivot).unwrap_or(p);
    (-(p.y - c.y)).atan2(p.x - c.x) // screen y points down; measure with y up
}

fn wrap_pi(a: f64) -> f64 {
    (a + PI).rem_euclid(TAU) - PI
}

impl TransformModal {
    /// Gather the targets and remember where the pointer started.
    fn begin(&mut self, ctx: &mut Ctx, event: &Event, axis: Axis) -> Result<ViewInfo, &'static str> {
        let view = ctx.view.ok_or("needs a 3D viewport")?;
        let targets = Targets::gather(ctx.doc).ok_or("nothing selected")?;
        self.pivot = targets.pivot();
        self.targets = Some(targets);
        self.start = pointer_of(event).unwrap_or(ctx.pointer);
        self.last = self.start;
        self.constraint = axis.dir();
        self.release_confirm = starts_drag(event);
        self.angle = 0.0;
        self.last_angle = screen_angle(&view, self.pivot, self.start);
        Ok(view)
    }

    fn step(&mut self, event: &Event) -> Step {
        match control(event, self.release_confirm) {
            Control::Move(p) => {
                self.last = p;
                Step::Update
            }
            Control::Confirm => Step::Finish,
            Control::Cancel => Step::Cancel,
            Control::Axis(a) => {
                let d = a.dir();
                self.constraint = if self.constraint == d { None } else { d };
                Step::Update
            }
            Control::Ignore => Step::Nothing,
        }
    }

    fn axis(&self) -> Axis {
        self.constraint.map_or(Axis::Free, Axis::from_dir)
    }

    /// Free: the pivot follows the pointer on the plane facing the camera.
    /// Constrained: along the axis, by the nearest point to the pointer ray.
    fn translation(&self, view: &ViewInfo) -> Vec3 {
        match self.constraint {
            Some(axis) => match (view.on_axis(self.pivot, axis, self.start), view.on_axis(self.pivot, axis, self.last)) {
                (Some(t0), Some(t1)) => axis * (t1 - t0),
                _ => Vec3::ZERO,
            },
            None => match (view.on_view_plane(self.pivot, self.start), view.on_view_plane(self.pivot, self.last)) {
                (Some(a), Some(b)) => b - a,
                _ => Vec3::ZERO,
            },
        }
    }

    /// Accumulate the angle the pointer has swept around the pivot on screen.
    fn sweep(&mut self, view: &ViewInfo) {
        let a = screen_angle(view, self.pivot, self.last);
        self.angle += wrap_pi(a - self.last_angle);
        self.last_angle = a;
    }

    /// The rotation axis is the constraint, else the axis pointing at the
    /// viewer. When the constraint points away, the sign flips so dragging
    /// counter-clockwise on screen still turns things counter-clockwise.
    fn rotation(&self, view: &ViewInfo) -> (Vec3, f64) {
        let toward_viewer = -view.forward;
        match self.constraint {
            Some(axis) => (axis, if axis.dot(toward_viewer) < 0.0 { -self.angle } else { self.angle }),
            None => (toward_viewer, self.angle),
        }
    }

    /// Ratio of the pointer's distance from the pivot now to at the start.
    fn scale(&self, view: &ViewInfo) -> Vec3 {
        let Some(c) = view.project(self.pivot) else {
            return Vec3::ONE;
        };
        let f = c.distance(self.last) / c.distance(self.start).max(MIN_SCALE_RADIUS);
        match self.constraint {
            Some(axis) => Vec3::ONE + axis.abs() * (f - 1.0),
            None => Vec3::splat(f),
        }
    }
}

/// Where a transform of the current selection would pivot, in world space:
/// what the gizmo is drawn around. `None` when nothing is selected.
pub fn pivot(doc: &prism_doc::Doc) -> Option<Vec3> {
    Targets::gather(doc).map(|t| t.pivot())
}

fn cancelled(ctx: &mut Ctx, label: &str, why: &str) -> OpResult<Flow> {
    ctx.report(format!("{label}: {why}"));
    Ok(Flow::Cancelled)
}

// ---- Move -----------------------------------------------------------------

props! {
    pub struct TranslateProps {
        pub delta: Vec3 = Vec3::ZERO => { id: 1, subtype: Translation },
        /// Axis the drag was constrained to (X / Y / Z while moving).
        pub axis: Axis = Axis::Free => { id: 2 },
    }
}

fn report_move(ctx: &mut Ctx, p: &TranslateProps) {
    let d = p.delta;
    ctx.report(format!("Move  X {:+.3}  Y {:+.3}  Z {:+.3}{}", d.x, d.y, d.z, p.axis.tag()));
}

pub struct Translate;
impl Operator for Translate {
    const ID: &'static str = "transform.translate";
    const LABEL: &'static str = "Move";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = TranslateProps;
    type Modal = TransformModal;
    fn poll(ctx: &Ctx) -> bool {
        has_targets(ctx.doc)
    }
    fn exec(ctx: &mut Ctx, p: &TranslateProps) -> OpResult<Outcome> {
        let Some(t) = Targets::gather(ctx.doc) else {
            return Ok(Outcome::Cancelled);
        };
        apply(ctx.doc, &t, t.pivot(), Op::Translate(p.delta));
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut TranslateProps, event: &Event, m: &mut TransformModal) -> OpResult<Flow> {
        if let Err(why) = m.begin(ctx, event, p.axis) {
            return cancelled(ctx, Self::LABEL, why);
        }
        report_move(ctx, p);
        Ok(Flow::Running)
    }
    fn modal(m: &mut TransformModal, ctx: &mut Ctx, p: &mut TranslateProps, event: &Event) -> OpResult<Flow> {
        match m.step(event) {
            Step::Update => {
                if let (Some(view), Some(t)) = (ctx.view, m.targets.as_ref()) {
                    p.delta = m.translation(&view);
                    p.axis = m.axis();
                    apply(ctx.doc, t, m.pivot, Op::Translate(p.delta));
                }
                report_move(ctx, p);
                Ok(Flow::Running)
            }
            Step::Finish => Ok(Flow::Finished),
            Step::Cancel => Ok(Flow::Cancelled),
            Step::Nothing => Ok(Flow::Running),
        }
    }
}

// ---- Rotate ---------------------------------------------------------------

props! {
    pub struct RotateProps {
        /// World-space axis; zero means "toward the viewer" while dragging.
        pub axis: Vec3 = Vec3::ZERO => { id: 1, subtype: Direction },
        pub angle: f64 = 0.0 => { id: 2, subtype: Angle },
    }
}

fn report_rotate(ctx: &mut Ctx, p: &RotateProps, axis: Axis) {
    ctx.report(format!("Rotate  {:+.1}°{}", p.angle.to_degrees(), axis.tag()));
}

pub struct Rotate;
impl Operator for Rotate {
    const ID: &'static str = "transform.rotate";
    const LABEL: &'static str = "Rotate";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = RotateProps;
    type Modal = TransformModal;
    fn poll(ctx: &Ctx) -> bool {
        has_targets(ctx.doc)
    }
    fn exec(ctx: &mut Ctx, p: &RotateProps) -> OpResult<Outcome> {
        let Some(axis) = p.axis.try_normalize() else {
            ctx.report("Rotate: needs an axis");
            return Ok(Outcome::Cancelled);
        };
        let Some(t) = Targets::gather(ctx.doc) else {
            return Ok(Outcome::Cancelled);
        };
        apply(ctx.doc, &t, t.pivot(), Op::Rotate { axis, angle: p.angle });
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut RotateProps, event: &Event, m: &mut TransformModal) -> OpResult<Flow> {
        match m.begin(ctx, event, Axis::from_dir(p.axis)) {
            Ok(view) => {
                (p.axis, p.angle) = m.rotation(&view);
                report_rotate(ctx, p, m.axis());
                Ok(Flow::Running)
            }
            Err(why) => cancelled(ctx, Self::LABEL, why),
        }
    }
    fn modal(m: &mut TransformModal, ctx: &mut Ctx, p: &mut RotateProps, event: &Event) -> OpResult<Flow> {
        match m.step(event) {
            Step::Update => {
                if let Some(view) = ctx.view {
                    m.sweep(&view);
                    (p.axis, p.angle) = m.rotation(&view);
                    if let Some(t) = m.targets.as_ref() {
                        apply(ctx.doc, t, m.pivot, Op::Rotate { axis: p.axis, angle: p.angle });
                    }
                }
                report_rotate(ctx, p, m.axis());
                Ok(Flow::Running)
            }
            Step::Finish => Ok(Flow::Finished),
            Step::Cancel => Ok(Flow::Cancelled),
            Step::Nothing => Ok(Flow::Running),
        }
    }
}

// ---- Scale ----------------------------------------------------------------

props! {
    pub struct ScaleProps {
        pub factor: Vec3 = Vec3::ONE => { id: 1, subtype: Scale },
        /// Axis the drag was constrained to (X / Y / Z while scaling).
        pub axis: Axis = Axis::Free => { id: 2 },
    }
}

fn report_scale(ctx: &mut Ctx, p: &ScaleProps) {
    let f = p.factor;
    ctx.report(format!("Scale  {:.3}  {:.3}  {:.3}{}", f.x, f.y, f.z, p.axis.tag()));
}

pub struct Scale;
impl Operator for Scale {
    const ID: &'static str = "transform.scale";
    const LABEL: &'static str = "Scale";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = ScaleProps;
    type Modal = TransformModal;
    fn poll(ctx: &Ctx) -> bool {
        has_targets(ctx.doc)
    }
    fn exec(ctx: &mut Ctx, p: &ScaleProps) -> OpResult<Outcome> {
        let Some(t) = Targets::gather(ctx.doc) else {
            return Ok(Outcome::Cancelled);
        };
        apply(ctx.doc, &t, t.pivot(), Op::Scale(p.factor));
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut ScaleProps, event: &Event, m: &mut TransformModal) -> OpResult<Flow> {
        if let Err(why) = m.begin(ctx, event, p.axis) {
            return cancelled(ctx, Self::LABEL, why);
        }
        report_scale(ctx, p);
        Ok(Flow::Running)
    }
    fn modal(m: &mut TransformModal, ctx: &mut Ctx, p: &mut ScaleProps, event: &Event) -> OpResult<Flow> {
        match m.step(event) {
            Step::Update => {
                if let (Some(view), Some(t)) = (ctx.view, m.targets.as_ref()) {
                    p.factor = m.scale(&view);
                    p.axis = m.axis();
                    apply(ctx.doc, t, m.pivot, Op::Scale(p.factor));
                }
                report_scale(ctx, p);
                Ok(Flow::Running)
            }
            Step::Finish => Ok(Flow::Finished),
            Step::Cancel => Ok(Flow::Cancelled),
            Step::Nothing => Ok(Flow::Running),
        }
    }
}

pub fn register(r: &mut Registry) {
    r.register::<Translate>();
    r.register::<Rotate>();
    r.register::<Scale>();
}
