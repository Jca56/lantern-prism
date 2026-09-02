//! Property panels generated from `prism-props` metadata: one widget per
//! field, nested structs as collapsible sections, lists as editable rows.

use prism_math::{Color, Rect, Vec2, rad_to_deg, deg_to_rad};
use prism_props::{FieldInfo, Kind, Reflect, ReflectList, Subtype, Value};

use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

impl Ui<'_> {
    /// A header that hides or shows what `f` declares. Open by default.
    pub fn collapsing(&mut self, label: &str, f: impl FnOnce(&mut Ui)) {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        let r = self.interact(id, rect, Sense::CLICK);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let open = self.state.open_default(id, true);
        if r.clicked {
            *self.state.open(id) = !open;
        }
        let open = *self.state.open(id);
        let bg = self.widget_color(&r);
        self.fill(rect, bg);
        // Disclosure triangle.
        let s = self.m.px(6.0);
        let c = Vec2::new(rect.min.x + self.m.pad + s, rect.center().y);
        let w = self.m.px(2.0);
        let col = self.theme.text_dim;
        if open {
            self.draw.line(Vec2::new(c.x - s, c.y - s * 0.5), Vec2::new(c.x, c.y + s * 0.5), w, col);
            self.draw.line(Vec2::new(c.x, c.y + s * 0.5), Vec2::new(c.x + s, c.y - s * 0.5), w, col);
        } else {
            self.draw.line(Vec2::new(c.x - s * 0.5, c.y - s), Vec2::new(c.x + s * 0.5, c.y), w, col);
            self.draw.line(Vec2::new(c.x + s * 0.5, c.y), Vec2::new(c.x - s * 0.5, c.y + s), w, col);
        }
        let style = self.text_style();
        let inner = Rect::new(Vec2::new(c.x + s + self.m.pad, rect.min.y), rect.max);
        self.text_in_rect(label, &style, inner, self.theme.text);
        if open {
            self.push_id(label);
            let indent = self.m.pad * 2.0;
            self.indent(indent, f);
            self.pop_id();
        }
    }

    /// Widgets for every visible field of `target`. Returns `true` if any
    /// value changed.
    pub fn props_panel(&mut self, target: &mut dyn Reflect) -> bool {
        let info = target.type_info();
        let mut changed = false;
        for i in 0..info.fields.len() {
            let field = &info.fields[i];
            if field.is_hidden() {
                continue;
            }
            self.push_index(i);
            changed |= self.prop_field(target, i, field);
            self.pop_id();
        }
        changed
    }

    fn prop_field(&mut self, target: &mut dyn Reflect, i: usize, field: &FieldInfo) -> bool {
        match &field.kind {
            Kind::Struct(_) => {
                let mut changed = false;
                if let Some(sub) = target.get_struct_mut(i) {
                    let label = field.label.clone();
                    self.collapsing(&label, |ui| changed = ui.props_panel(sub));
                }
                changed
            }
            Kind::List(item) => {
                let item = (**item).clone();
                let mut changed = false;
                if let Some(list) = target.get_list_mut(i) {
                    let label = format!("{} ({})", field.label, list.len());
                    self.collapsing(&label, |ui| changed = ui.prop_list(list, &item, field));
                }
                changed
            }
            _ => {
                let mut changed = false;
                let value = target.get(i);
                let label = field.label.clone();
                self.labelled(&label, |ui| {
                    if let Some(v) = ui.prop_value(&value, field) {
                        changed = target.set(i, v).is_ok();
                    }
                });
                changed
            }
        }
    }

    fn prop_list(&mut self, list: &mut dyn ReflectList, item: &Kind, field: &FieldInfo) -> bool {
        let mut changed = false;
        let mut remove = None;
        for j in 0..list.len() {
            self.push_index(j);
            if matches!(item, Kind::Struct(_)) {
                if let Some(sub) = list.get_struct_mut(j) {
                    let label = format!("{} {}", field.label, j + 1);
                    self.collapsing(&label, |ui| changed |= ui.props_panel(sub));
                }
            } else {
                let value = list.get(j);
                let label = format!("{}", j + 1);
                self.labelled(&label, |ui| {
                    ui.row(|ui| {
                        if let Some(v) = ui.prop_value(&value, field) {
                            changed |= list.set(j, v).is_ok();
                        }
                        if ui.button("−").clicked {
                            remove = Some(j);
                        }
                    });
                });
            }
            self.pop_id();
        }
        if let Some(j) = remove {
            list.remove(j);
            changed = true;
        }
        if self.button("+ Add").clicked {
            list.push_default();
            changed = true;
        }
        changed
    }

    /// The widget for one leaf value. Returns the new value when edited.
    fn prop_value(&mut self, value: &Value, field: &FieldInfo) -> Option<Value> {
        match value {
            Value::Bool(b) => {
                let mut v = *b;
                self.toggle("", &mut v).then_some(Value::Bool(v))
            }
            Value::F64(x) => {
                let (min, max) = field.hard.map_or((f64::NEG_INFINITY, f64::INFINITY), |r| (r.min, r.max));
                let slider = field.slider_range().filter(|r| r.min.is_finite() && r.max.is_finite());
                let angle = matches!(field.subtype, Subtype::Angle);
                let percent = matches!(field.subtype, Subtype::Percentage);
                let (mut v, scale) = if angle {
                    (rad_to_deg(*x), 1.0)
                } else if percent {
                    (*x * 100.0, 100.0)
                } else {
                    (*x, 1.0)
                };
                let conv = |r: f64| if angle { rad_to_deg(r) } else { r * scale };
                let step = field.step.map_or(if angle { 1.0 } else { 0.01 * scale }, conv);
                let changed = match slider {
                    Some(r) => self.slider("", &mut v, conv(r.min), conv(r.max), step),
                    None => {
                        let range = (min.is_finite() || max.is_finite()).then_some((conv(min), conv(max)));
                        self.drag_value("", &mut v, step, range, 3)
                    }
                };
                changed.then(|| {
                    let back = if angle { deg_to_rad(v) } else { v / scale };
                    Value::F64(back.clamp(min, max))
                })
            }
            Value::I64(n) => {
                let mut v = *n;
                let range = field.hard.map(|r| (r.min.max(i64::MIN as f64) as i64, r.max.min(i64::MAX as f64) as i64));
                self.drag_int("", &mut v, range).then_some(Value::I64(v))
            }
            Value::Str(s) => {
                let mut v = s.clone();
                let r = self.text_field("", &mut v);
                r.changed.then_some(Value::Str(v))
            }
            Value::Enum(x) => {
                let Kind::Enum(info) = &field.kind else {
                    return None;
                };
                let labels: Vec<&str> = info.variants.iter().map(|v| v.label).collect();
                let mut idx = info.variants.iter().position(|v| v.value == *x).unwrap_or(0);
                self.dropdown("", &mut idx, &labels).then(|| Value::Enum(info.variants[idx].value))
            }
            Value::Vec2(v) => {
                let mut a = [v.x, v.y];
                self.vector_row(&mut a, &["X", "Y"]).then(|| Value::Vec2(prism_math::Vec2::new(a[0], a[1])))
            }
            Value::Vec3(v) => {
                let mut a = [v.x, v.y, v.z];
                let angle = matches!(field.subtype, Subtype::Euler);
                if angle {
                    a = a.map(rad_to_deg);
                }
                self.vector_row(&mut a, &["X", "Y", "Z"]).then(|| {
                    if angle {
                        a = a.map(deg_to_rad);
                    }
                    Value::Vec3(prism_math::Vec3::new(a[0], a[1], a[2]))
                })
            }
            Value::Vec4(v) => {
                let mut a = [v.x, v.y, v.z, v.w];
                self.vector_row(&mut a, &["X", "Y", "Z", "W"]).then(|| Value::Vec4(prism_math::Vec4::new(a[0], a[1], a[2], a[3])))
            }
            Value::Color(c) => {
                let mut a = [c.r, c.g, c.b, c.a];
                let mut changed = false;
                self.row(|ui| {
                    let sw = ui.alloc(Vec2::new(ui.m.widget_h, ui.m.widget_h));
                    ui.fill(sw, *c);
                    ui.outline(sw, ui.m.border, ui.theme.border);
                    for (k, name) in ["R", "G", "B", "A"].iter().enumerate() {
                        ui.push_index(k);
                        changed |= ui.drag_value(name, &mut a[k], 0.005, Some((0.0, 1.0)), 2);
                        ui.pop_id();
                    }
                });
                changed.then(|| Value::Color(Color::rgba(a[0], a[1], a[2], a[3])))
            }
            Value::Id(id) => {
                self.label_dim(&format!("{id}"));
                None
            }
            Value::None => None,
        }
    }

    fn vector_row<const N: usize>(&mut self, a: &mut [f64; N], names: &[&str; N]) -> bool {
        let mut changed = false;
        self.row(|ui| {
            for k in 0..N {
                ui.push_index(k);
                changed |= ui.drag_value(names[k], &mut a[k], 0.01, None, 3);
                ui.pop_id();
            }
        });
        changed
    }
}
