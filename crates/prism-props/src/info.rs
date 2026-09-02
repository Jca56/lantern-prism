//! Per-type metadata: the list of fields, built once and cached in a static.

use crate::field::{FieldInfo, FieldInfoBuilder};

#[derive(Debug)]
pub struct TypeInfo {
    pub name: &'static str,
    pub doc: String,
    pub fields: Vec<FieldInfo>,
}

impl TypeInfo {
    pub fn builder(name: &'static str) -> TypeInfoBuilder {
        TypeInfoBuilder { info: TypeInfo { name, doc: String::new(), fields: Vec::new() } }
    }

    /// Field index by Rust name.
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.name == name)
    }

    /// Field index by stable id.
    pub fn field_index_by_id(&self, id: u32) -> Option<usize> {
        self.fields.iter().position(|f| f.id == id)
    }

    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.field_index(name).map(|i| &self.fields[i])
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

pub struct TypeInfoBuilder {
    info: TypeInfo,
}

impl TypeInfoBuilder {
    pub fn doc_line(mut self, line: &str) -> Self {
        if !self.info.doc.is_empty() {
            self.info.doc.push('\n');
        }
        self.info.doc.push_str(line.strip_prefix(' ').unwrap_or(line));
        self
    }

    pub fn field(mut self, f: FieldInfoBuilder) -> Self {
        self.info.fields.push(f.build());
        self
    }

    /// Assign implicit ids (`index + 1`) and check the description is sane.
    /// Panics on duplicate ids or names: that is a programming error and this
    /// runs once, at first use, in every build.
    pub fn build(mut self) -> TypeInfo {
        for (i, f) in self.info.fields.iter_mut().enumerate() {
            if f.id == 0 {
                f.id = i as u32 + 1;
            }
        }
        let name = self.info.name;
        for (i, a) in self.info.fields.iter().enumerate() {
            for b in &self.info.fields[i + 1..] {
                assert!(a.id != b.id, "props! {name}: fields `{}` and `{}` share id {}", a.name, b.name, a.id);
                assert!(a.name != b.name, "props! {name}: duplicate field `{}`", a.name);
            }
        }
        self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn implicit_ids_and_lookup() {
        let t = TypeInfo::builder("T")
            .doc_line(" A type.")
            .field(FieldInfo::builder::<f64>("a", Value::F64(0.0)))
            .field(FieldInfo::builder::<bool>("b", Value::Bool(false)).id(10))
            .field(FieldInfo::builder::<i64>("c", Value::I64(0)))
            .build();
        assert_eq!(t.doc, "A type.");
        assert_eq!(t.fields.iter().map(|f| f.id).collect::<Vec<_>>(), vec![1, 10, 3]);
        assert_eq!(t.field_index("c"), Some(2));
        assert_eq!(t.field_index_by_id(10), Some(1));
        assert_eq!(t.field("zzz").map(|f| f.id), None);
        assert_eq!(t.len(), 3);
    }

    #[test]
    #[should_panic(expected = "share id 1")]
    fn duplicate_ids_panic() {
        TypeInfo::builder("T")
            .field(FieldInfo::builder::<f64>("a", Value::F64(0.0)))
            .field(FieldInfo::builder::<f64>("b", Value::F64(0.0)).id(1))
            .build();
    }
}
