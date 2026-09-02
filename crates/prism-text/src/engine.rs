//! `TextEngine`: fonts + layout cache + atlas → positioned glyph quads.
//!
//! Layout-cache-driven: text lays out once (shape + wrap + fallback), then
//! repeat calls replay cached glyph placements. Glyphs rasterize into the
//! atlas on first use, at one of four quarter-pixel subpixel bins so
//! proportional text keeps even spacing.

use crate::atlas::{Atlas, AtlasEntry};
use crate::font::FontError;
use crate::font::db::FontDb;
use crate::layout::{LayoutCache, LayoutKey, line};
use crate::{FontStyle, FontWeight, quantize_px, raster};

/// Initial atlas edge in texels; grows on demand.
const ATLAS_SIZE: u32 = 1024;

/// Which family a style resolves against.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Family {
    /// The engine's proportional default.
    #[default]
    Sans,
    /// The engine's monospace default.
    Mono,
    /// A specific installed family, falling back to `Sans` if absent.
    Named(String),
}

/// Everything that decides a glyph's shape (never its color).
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub family: Family,
}

impl TextStyle {
    pub fn new(size: f32) -> Self {
        Self { size, weight: FontWeight::Normal, style: FontStyle::Normal, family: Family::Sans }
    }
    pub fn bold(mut self) -> Self {
        self.weight = FontWeight::Bold;
        self
    }
    pub fn italic(mut self) -> Self {
        self.style = FontStyle::Italic;
        self
    }
    pub fn mono(mut self) -> Self {
        self.family = Family::Mono;
        self
    }
    pub fn family(mut self, name: &str) -> Self {
        self.family = Family::Named(name.to_owned());
        self
    }
    /// Line advance for this size.
    pub fn line_height(&self) -> f32 {
        quantize_px(self.size) * 1.2
    }
    fn resolve_args(&self) -> (Option<&str>, bool) {
        match &self.family {
            Family::Sans => (None, false),
            Family::Mono => (None, true),
            Family::Named(n) => (Some(n.as_str()), false),
        }
    }
    fn key(&self, text: &str, max_width: f32) -> LayoutKey {
        let (family, mono) = self.resolve_args();
        LayoutKey {
            text: text.to_owned(),
            font_size_bits: quantize_px(self.size).to_bits(),
            max_width_bits: max_width.to_bits(),
            weight: self.weight as u8,
            style: self.style as u8 | (mono as u8) << 1,
            family: family.unwrap_or("").to_owned(),
        }
    }
}

/// Size of a laid-out block.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Widest line's advance width.
    pub width: f32,
    /// `lines × line_height`.
    pub height: f32,
    pub lines: usize,
}

/// One glyph ready to draw: pixel rect, texel-space atlas rect, tint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// The caller's color, passed through untouched. For color glyphs the
    /// RGB is forced to white so only alpha applies.
    pub color: [f32; 4],
    pub is_color: bool,
}

/// Width used for unbounded measurement and single-line layout.
const UNBOUNDED: f32 = 1.0e6;

pub struct TextEngine {
    db: FontDb,
    layouts: LayoutCache,
    atlas: Atlas,
    cache_hits: u64,
    cache_misses: u64,
}

/// Atlas cache key for a rasterized glyph: 2 bits of subpixel bin, 16 of face
/// id, 16 of glyph id, 28 of quarter-pixel size.
fn glyph_cache_key(face_id: usize, gid: u16, size: f32, bin: u32) -> u64 {
    let q = ((size * 4.0).round() as u64) & 0x0FFF_FFFF;
    (1 << 63) | ((bin as u64 & 0x3) << 61) | ((face_id as u64 & 0xFFFF) << 44) | ((gid as u64) << 28) | q
}

impl TextEngine {
    /// Scan the system font directories. `sans` and `mono` are the preferred
    /// default families; sensible chains apply when they are not installed.
    pub fn new(sans: &str, mono: &str) -> Self {
        Self {
            db: FontDb::discover(sans, mono),
            layouts: LayoutCache::new(),
            atlas: Atlas::new(ATLAS_SIZE),
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Register a font from raw `.ttf`/`.ttc`/`.otf` bytes (face 0).
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<(), FontError> {
        self.db.add_font_data(data)
    }

    /// Known faces, discovered plus embedded.
    pub fn face_count(&self) -> usize {
        self.db.face_count()
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    /// `(layout cache hits, misses)` since creation.
    pub fn cache_stats(&self) -> (u64, u64) {
        (self.cache_hits, self.cache_misses)
    }

    /// Layouts currently cached.
    pub fn cached_layouts(&self) -> usize {
        self.layouts.len()
    }

    fn layout(&mut self, text: &str, style: &TextStyle, max_width: f32) -> LayoutKey {
        let max_width = quantize_px(max_width.max(1.0));
        let key = style.key(text, max_width);
        if self.layouts.get(&key).is_some() {
            self.cache_hits += 1;
        } else {
            self.cache_misses += 1;
            let (family, mono) = style.resolve_args();
            let built = line::build(
                &mut self.db,
                mono,
                text,
                quantize_px(style.size),
                max_width,
                style.weight,
                style.style,
                family,
            );
            self.layouts.insert(key.clone(), built);
        }
        key
    }

    fn metrics_of(key: &LayoutKey, layouts: &mut LayoutCache, style: &TextStyle) -> TextMetrics {
        match layouts.get(key) {
            Some(l) => TextMetrics { width: l.width, height: l.lines as f32 * style.line_height(), lines: l.lines },
            None => TextMetrics::default(),
        }
    }

    /// Advance width of `text` with no wrapping (widest line if multi-line).
    pub fn measure(&mut self, text: &str, style: &TextStyle) -> f32 {
        self.measure_wrapped(text, style, UNBOUNDED).width
    }

    /// Size of `text` wrapped at `max_width`.
    pub fn measure_wrapped(&mut self, text: &str, style: &TextStyle, max_width: f32) -> TextMetrics {
        let key = self.layout(text, style, max_width);
        Self::metrics_of(&key, &mut self.layouts, style)
    }

    /// Baseline offset from the top of the line box.
    pub fn ascent(&mut self, style: &TextStyle) -> f32 {
        let (family, mono) = style.resolve_args();
        let size = quantize_px(style.size);
        self.db
            .resolve(family, mono, style.weight, style.style)
            .and_then(|id| self.db.font(id).map(|f| f.ascender_px(size)))
            .unwrap_or(size * 0.8)
    }

    /// Cumulative pen x at every cluster boundary of one line, as
    /// `(byte_offset, x)` pairs from `(0, 0)` to `(len, width)`. One shaping
    /// pass answers every caret/selection query a text field needs.
    pub fn advances(&mut self, text: &str, style: &TextStyle, out: &mut Vec<(u32, f32)>) {
        let (family, mono) = style.resolve_args();
        line::advances(&mut self.db, mono, text, quantize_px(style.size), style.weight, style.style, family, out);
    }

    /// Lay out `text` at `(x, y)` (top-left of the line box), wrapping at
    /// `max_width`, and append one quad per visible glyph to `out`.
    /// Returns the block's metrics.
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &mut self,
        text: &str,
        style: &TextStyle,
        x: f32,
        y: f32,
        max_width: f32,
        color: [f32; 4],
        out: &mut Vec<GlyphQuad>,
    ) -> TextMetrics {
        let key = self.layout(text, style, max_width);
        let size = quantize_px(style.size);
        let metrics = Self::metrics_of(&key, &mut self.layouts, style);
        let Some(layout) = self.layouts.get(&key) else {
            return metrics;
        };
        for g in &layout.glyphs {
            let (pen_px, bin) = subpixel_bin(x + g.x);
            let entry = raster_entry(&mut self.atlas, &mut self.db, g.face as usize, g.gid, size, bin);
            if entry.width > 0 && entry.height > 0 {
                out.push(GlyphQuad {
                    x: pen_px + entry.left as f32,
                    y: (y + g.y).round() - entry.top as f32,
                    w: entry.width as f32,
                    h: entry.height as f32,
                    uv_min: entry.uv_min,
                    uv_max: entry.uv_max,
                    color: if entry.is_color { [1.0, 1.0, 1.0, color[3]] } else { color },
                    is_color: entry.is_color,
                });
            }
        }
        metrics
    }
}

/// Split a pen x into (whole pixel, quarter-pixel bin 0–3).
fn subpixel_bin(pen_x: f32) -> (f32, u32) {
    let xi = pen_x.floor();
    let bin = ((pen_x - xi) * 4.0).round() as u32;
    if bin == 4 { (xi + 1.0, 0) } else { (xi, bin) }
}

/// The atlas entry for a glyph at a subpixel bin, rasterizing on first use.
fn raster_entry(atlas: &mut Atlas, db: &mut FontDb, face_id: usize, gid: u16, size: f32, bin: u32) -> AtlasEntry {
    let key = glyph_cache_key(face_id, gid, size, bin);
    if let Some(entry) = atlas.get(key) {
        return entry;
    }
    // Color glyphs (CBDT strikes / COLRv0 layers) win over outlines.
    if let Some(c) = db.font(face_id).and_then(|f| f.color_glyph(gid, size)) {
        return atlas.insert_rgba(key, c.width, c.height, c.left, c.top, &c.rgba);
    }
    let raster = db.font(face_id).and_then(|f| {
        let scale = f.scale(size);
        match f.outline(gid) {
            Ok(outline) => raster::rasterize(&outline, scale, bin as f32 * 0.25),
            Err(e) => {
                prism_core::log_warn!("face {face_id} glyph {gid}: {e}");
                None
            }
        }
    });
    match raster {
        Some(g) => atlas.insert_coverage(key, g.width, g.height, g.left, g.top, &g.coverage),
        // Whitespace / empty outline: cache a zero-area entry.
        None => atlas.insert_coverage(key, 0, 0, 0, 0, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> TextEngine {
        TextEngine::new("Noto Sans", "DejaVu Sans Mono")
    }

    #[test]
    fn subpixel_bins() {
        assert_eq!(subpixel_bin(10.0), (10.0, 0));
        assert_eq!(subpixel_bin(10.3), (10.0, 1));
        assert_eq!(subpixel_bin(10.5), (10.0, 2));
        assert_eq!(subpixel_bin(10.9), (11.0, 0));
        assert_eq!(subpixel_bin(-0.25), (-1.0, 3));
    }

    #[test]
    fn cache_keys_separate_bins_faces_and_sizes() {
        let a = glyph_cache_key(1, 40, 20.0, 0);
        assert_ne!(a, glyph_cache_key(1, 40, 20.0, 1));
        assert_ne!(a, glyph_cache_key(2, 40, 20.0, 0));
        assert_ne!(a, glyph_cache_key(1, 41, 20.0, 0));
        assert_ne!(a, glyph_cache_key(1, 40, 20.25, 0));
        assert_eq!(a, glyph_cache_key(1, 40, 20.0, 0));
    }

    #[test]
    fn style_keys_distinguish_mono() {
        let s = TextStyle::new(20.0);
        let m = TextStyle::new(20.0).mono();
        assert_ne!(s.key("a", 100.0), m.key("a", 100.0));
        assert_eq!(s.key("a", 100.0), TextStyle::new(20.0).key("a", 100.0));
        assert_eq!(TextStyle::new(20.0).line_height(), 24.0);
    }

    /// Needs installed fonts; skips silently on a bare machine.
    #[test]
    fn lays_out_real_text() {
        let mut e = engine();
        if e.face_count() == 0 {
            eprintln!("no fonts installed — skipping layout test");
            return;
        }
        let style = TextStyle::new(40.0);
        let w = e.measure("Prism", &style);
        assert!(w > 40.0 && w < 400.0, "width {w}");
        let bold = e.measure("Prism", &style.clone().bold());
        assert!(bold >= w * 0.95, "bold should not be much narrower: {bold} vs {w}");
        let wrapped = e.measure_wrapped("one two three four five six", &style, w);
        assert!(wrapped.lines >= 2, "{wrapped:?}");
        assert_eq!(wrapped.height, wrapped.lines as f32 * 48.0);

        let mut quads = Vec::new();
        let m = e.place("Prism 🦊", &style, 100.0, 50.0, 1000.0, [1.0, 0.5, 0.0, 1.0], &mut quads);
        assert_eq!(m.lines, 1);
        assert!(quads.len() >= 5, "{} quads", quads.len());
        assert!(quads.iter().all(|q| q.x >= 100.0 && q.y >= 50.0 && q.w > 0.0 && q.h > 0.0));
        assert!(quads.iter().any(|q| !q.is_color && q.color == [1.0, 0.5, 0.0, 1.0]));
        assert!(e.atlas().dirty().is_some(), "glyphs landed in the atlas");
        assert!(e.atlas().len() >= 5);
        // Second placement is a cache hit and rasterizes nothing new.
        e.atlas_mut().take_dirty();
        let n = e.atlas().len();
        let mut again = Vec::new();
        e.place("Prism 🦊", &style, 100.0, 50.0, 1000.0, [1.0; 4], &mut again);
        assert_eq!(again.len(), quads.len());
        assert_eq!(e.atlas().len(), n);
        assert_eq!(e.atlas().dirty(), None);
        assert!(e.cache_stats().0 >= 1);

        let mut adv = Vec::new();
        e.advances("ab", &style, &mut adv);
        assert_eq!(adv.first(), Some(&(0, 0.0)));
        assert_eq!(adv.last().map(|a| a.0), Some(2));
        assert!(e.ascent(&style) > 20.0);
    }
}
