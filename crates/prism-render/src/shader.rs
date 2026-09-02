//! WGSL sources embedded from `shaders/`, with a tiny `#include "name"`
//! preprocessor so shared snippets live in one file.

use core::fmt;

/// Every shader in `shaders/`, embedded at compile time.
const SOURCES: &[(&str, &str)] = &[
    ("ui.wgsl", include_str!("../../../shaders/ui.wgsl")),
    ("common3d.wgsl", include_str!("../../../shaders/common3d.wgsl")),
    ("grid.wgsl", include_str!("../../../shaders/grid.wgsl")),
    ("mesh.wgsl", include_str!("../../../shaders/mesh.wgsl")),
    ("wire.wgsl", include_str!("../../../shaders/wire.wgsl")),
    ("points.wgsl", include_str!("../../../shaders/points.wgsl")),
    ("pick.wgsl", include_str!("../../../shaders/pick.wgsl")),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderError {
    NotFound(String),
    IncludeCycle(String),
    BadInclude { file: String, line: usize },
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShaderError::NotFound(n) => write!(f, "shader `{n}` not found"),
            ShaderError::IncludeCycle(n) => write!(f, "shader include cycle through `{n}`"),
            ShaderError::BadInclude { file, line } => write!(f, "{file}:{line}: malformed #include"),
        }
    }
}

impl std::error::Error for ShaderError {}

/// Preprocess the embedded shader `name`.
pub fn load(name: &str) -> Result<String, ShaderError> {
    preprocess(name, &|n| SOURCES.iter().find(|(k, _)| *k == n).map(|(_, s)| *s))
}

/// Expand `#include "file"` lines recursively. `resolve` maps a name to its
/// source text. Each file is included at most once per expansion.
pub fn preprocess(name: &str, resolve: &dyn Fn(&str) -> Option<&'static str>) -> Result<String, ShaderError> {
    let mut out = String::new();
    let mut stack = Vec::new();
    let mut seen = Vec::new();
    expand(name, resolve, &mut out, &mut stack, &mut seen)?;
    Ok(out)
}

fn expand(
    name: &str,
    resolve: &dyn Fn(&str) -> Option<&'static str>,
    out: &mut String,
    stack: &mut Vec<String>,
    seen: &mut Vec<String>,
) -> Result<(), ShaderError> {
    if stack.iter().any(|s| s == name) {
        return Err(ShaderError::IncludeCycle(name.to_owned()));
    }
    if seen.iter().any(|s| s == name) {
        return Ok(()); // include-once
    }
    let src = resolve(name).ok_or_else(|| ShaderError::NotFound(name.to_owned()))?;
    stack.push(name.to_owned());
    seen.push(name.to_owned());
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("#include") {
            let rest = rest.trim();
            let inner = rest
                .strip_prefix('"')
                .and_then(|r| r.strip_suffix('"'))
                .ok_or_else(|| ShaderError::BadInclude { file: name.to_owned(), line: i + 1 })?;
            expand(inner, resolve, out, stack, seen)?;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    stack.pop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(n: &str) -> Option<&'static str> {
        match n {
            "a" => Some("// a\n#include \"common\"\nfn a() {}\n"),
            "b" => Some("#include \"common\"\n#include \"a\"\nfn b() {}\n"),
            "common" => Some("const X: f32 = 1.0;\n"),
            "cyc1" => Some("#include \"cyc2\"\n"),
            "cyc2" => Some("#include \"cyc1\"\n"),
            "bad" => Some("#include common\n"),
            _ => None,
        }
    }

    #[test]
    fn includes_once() {
        let s = preprocess("b", &fake).unwrap();
        assert_eq!(s.matches("const X").count(), 1, "common included once:\n{s}");
        assert!(s.contains("fn a()") && s.contains("fn b()"));
        assert!(s.find("const X").unwrap() < s.find("fn a()").unwrap());
    }

    #[test]
    fn errors() {
        assert_eq!(preprocess("nope", &fake), Err(ShaderError::NotFound("nope".into())));
        assert_eq!(preprocess("cyc1", &fake), Err(ShaderError::IncludeCycle("cyc1".into())));
        assert_eq!(preprocess("bad", &fake), Err(ShaderError::BadInclude { file: "bad".into(), line: 1 }));
    }

    #[test]
    fn embedded_ui_shader_loads() {
        let s = load("ui.wgsl").unwrap();
        assert!(s.contains("fn vs_main") && s.contains("fn fs_main"));
        for name in ["grid.wgsl", "mesh.wgsl", "wire.wgsl", "points.wgsl", "pick.wgsl"] {
            let s = load(name).unwrap();
            assert!(s.contains("struct ViewUniforms"), "{name} pulls in common3d");
            assert_eq!(s.matches("struct ViewUniforms").count(), 1);
        }
    }
}
