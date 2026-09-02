//! glTF 2.0 (D031): `.glb` (binary container) and `.gltf` (JSON with an
//! embedded base64 buffer). Meshes travel triangulated with normals, one
//! node per object carrying its transform, and a base-colour material.
//! Positions stay as they are: glTF and Prism are both Y-up, right-handed.

mod export;
mod import;

use core::fmt;

pub use export::{export, write_file};
pub use import::{GltfObject, parse, read_file};

use crate::json::JsonError;

#[derive(Debug)]
pub enum GltfError {
    Io(std::io::Error),
    Json(JsonError),
    /// Not a GLB, or a broken one.
    Glb(String),
    /// Valid glTF this build does not handle (sparse accessors, Draco…).
    Unsupported(String),
    /// A reference the file makes to something it does not contain.
    Missing(String),
    /// No mesh geometry at all.
    Empty,
}

impl fmt::Display for GltfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GltfError::Io(e) => write!(f, "{e}"),
            GltfError::Json(e) => write!(f, "{e}"),
            GltfError::Glb(s) => write!(f, "bad GLB: {s}"),
            GltfError::Unsupported(s) => write!(f, "unsupported glTF: {s}"),
            GltfError::Missing(s) => write!(f, "broken glTF: {s}"),
            GltfError::Empty => write!(f, "no mesh geometry in file"),
        }
    }
}

impl std::error::Error for GltfError {}

impl From<std::io::Error> for GltfError {
    fn from(e: std::io::Error) -> Self {
        GltfError::Io(e)
    }
}

impl From<JsonError> for GltfError {
    fn from(e: JsonError) -> Self {
        GltfError::Json(e)
    }
}

const GLB_MAGIC: &[u8; 4] = b"glTF";
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

fn pad4(v: &mut Vec<u8>, fill: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(fill);
    }
}

/// Wrap JSON text and a binary buffer as a GLB.
pub fn pack_glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    pad4(&mut j, b' ');
    let mut b = bin.to_vec();
    pad4(&mut b, 0);
    let total = 12 + 8 + j.len() + if b.is_empty() { 0 } else { 8 + b.len() };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(GLB_MAGIC);
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&j);
    if !b.is_empty() {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&b);
    }
    out
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, GltfError> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| GltfError::Glb("truncated".into()))
}

/// Split a GLB into its JSON text and binary chunk.
pub fn unpack_glb(bytes: &[u8]) -> Result<(String, Option<Vec<u8>>), GltfError> {
    if bytes.len() < 12 || &bytes[..4] != GLB_MAGIC {
        return Err(GltfError::Glb("not a GLB (magic)".into()));
    }
    let version = u32_at(bytes, 4)?;
    if version != 2 {
        return Err(GltfError::Unsupported(format!("GLB version {version}")));
    }
    let mut pos = 12;
    let mut json = None;
    let mut bin = None;
    while pos + 8 <= bytes.len() {
        let len = u32_at(bytes, pos)? as usize;
        let kind = u32_at(bytes, pos + 4)?;
        pos += 8;
        let data = bytes.get(pos..pos + len).ok_or_else(|| GltfError::Glb("chunk runs past the end".into()))?;
        pos += len;
        match kind {
            CHUNK_JSON => json = Some(String::from_utf8_lossy(data).trim_end().to_owned()),
            CHUNK_BIN => bin = Some(data.to_vec()),
            _ => {}
        }
    }
    let json = json.ok_or_else(|| GltfError::Glb("no JSON chunk".into()))?;
    Ok((json, bin))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_and_base64_round_trip() {
        let (json, bin) = unpack_glb(&pack_glb(r#"{"a":1}"#, &[1, 2, 3, 4, 5])).unwrap();
        assert_eq!(json, r#"{"a":1}"#);
        assert_eq!(bin.unwrap(), vec![1, 2, 3, 4, 5, 0, 0, 0], "padded to four");
        let (json, bin) = unpack_glb(&pack_glb("{}", &[])).unwrap();
        assert_eq!((json.as_str(), bin), ("{}", None));
        assert!(unpack_glb(b"nope").is_err());
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0u8, 255, 128, 7]] {
            assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert!(base64_decode("!!").is_none());
    }
}
