//! Walk a reflected value, and get/set by dotted path (`a.b[2].c`).

use crate::field::FieldInfo;
use crate::reflect::{PropError, Reflect};
use crate::value::{Kind, Value};

/// Visit every leaf field, depth first, with its dotted path.
pub fn walk(r: &dyn Reflect, f: &mut dyn FnMut(&str, &FieldInfo, Value)) {
    walk_inner(r, String::new(), f);
}

fn walk_inner(r: &dyn Reflect, prefix: String, f: &mut dyn FnMut(&str, &FieldInfo, Value)) {
    let info = r.type_info();
    for (i, field) in info.fields.iter().enumerate() {
        let path = if prefix.is_empty() { field.name.to_owned() } else { format!("{prefix}.{}", field.name) };
        match &field.kind {
            Kind::Struct(_) => {
                if let Some(s) = r.get_struct(i) {
                    walk_inner(s, path, f);
                }
            }
            Kind::List(item) => {
                if let Some(l) = r.get_list(i) {
                    for j in 0..l.len() {
                        let item_path = format!("{path}[{j}]");
                        if matches!(**item, Kind::Struct(_)) {
                            if let Some(s) = l.get_struct(j) {
                                walk_inner(s, item_path, f);
                            }
                        } else {
                            f(&item_path, field, l.get(j));
                        }
                    }
                }
            }
            _ => f(&path, field, r.get(i)),
        }
    }
}

/// One step of a path: a field name and an optional list index.
struct Segment<'a> {
    name: &'a str,
    index: Option<usize>,
}

fn parse_segment(seg: &str) -> Option<Segment<'_>> {
    match seg.find('[') {
        None => Some(Segment { name: seg, index: None }),
        Some(b) => {
            let close = seg.strip_suffix(']')?;
            let index = close[b + 1..].parse().ok()?;
            Some(Segment { name: &seg[..b], index: Some(index) })
        }
    }
}

/// Read a leaf by path. `None` if the path does not resolve to a leaf.
pub fn get_path(r: &dyn Reflect, path: &str) -> Option<Value> {
    let mut segs = path.split('.').peekable();
    let mut cur: &dyn Reflect = r;
    while let Some(seg) = segs.next() {
        let seg = parse_segment(seg)?;
        let fi = cur.type_info().field_index(seg.name)?;
        let last = segs.peek().is_none();
        match seg.index {
            Some(j) => {
                let l = cur.get_list(fi)?;
                if last {
                    return (j < l.len() && l.get_struct(j).is_none()).then(|| l.get(j));
                }
                cur = l.get_struct(j)?;
            }
            None => {
                if last {
                    return cur.get_struct(fi).is_none().then(|| cur.get(fi));
                }
                cur = cur.get_struct(fi)?;
            }
        }
    }
    None
}

/// Write a leaf by path.
pub fn set_path(r: &mut dyn Reflect, path: &str, value: Value) -> Result<(), PropError> {
    let bad = || PropError::BadPath(path.to_owned());
    let mut segs = path.split('.').peekable();
    let mut cur: &mut dyn Reflect = r;
    while let Some(seg) = segs.next() {
        let seg = parse_segment(seg).ok_or_else(bad)?;
        let fi = cur.type_info().field_index(seg.name).ok_or_else(bad)?;
        let last = segs.peek().is_none();
        match seg.index {
            Some(j) => {
                let l = cur.get_list_mut(fi).ok_or_else(bad)?;
                if last {
                    return l.set(j, value);
                }
                cur = l.get_struct_mut(j).ok_or_else(bad)?;
            }
            None => {
                if last {
                    return cur.set(fi, value);
                }
                cur = cur.get_struct_mut(fi).ok_or_else(bad)?;
            }
        }
    }
    Err(bad())
}
