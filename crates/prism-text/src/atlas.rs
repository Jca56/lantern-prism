//! CPU glyph atlas: one RGBA8 image, shelf-packed, growable, with a dirty
//! rectangle so the renderer uploads only what changed.
//!
//! Texels are **premultiplied, linear**: a coverage glyph is stored as
//! `(c, c, c, c)` so the shader's single `texel × tint` colors it; a color
//! glyph (emoji) is stored as premultiplied linear RGBA and drawn with a white
//! tint. Entries carry **texel** coordinates so the atlas can grow without
//! invalidating anything; growth bumps `generation`, which tells the renderer
//! to recreate its texture and upload the whole image once.

use std::collections::HashMap;
use std::sync::OnceLock;

/// A packed glyph's location in the atlas plus its placement metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AtlasEntry {
    /// Texel-space rect (pixels, not normalized).
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub width: u32,
    pub height: u32,
    /// Horizontal bearing: pixels from the pen origin to the glyph's left edge.
    pub left: i32,
    /// Vertical bearing: pixels from the baseline up to the glyph's top edge.
    pub top: i32,
    /// Color (emoji) glyph: drawn untinted apart from alpha.
    pub is_color: bool,
}

/// Integer pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl IRect {
    fn union(self, o: IRect) -> IRect {
        if self.w == 0 || self.h == 0 {
            return o;
        }
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        IRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

/// Growth cap. 8192² is guaranteed by wgpu's base limits.
pub const MAX_ATLAS_SIZE: u32 = 8192;

pub struct Atlas {
    size: u32,
    pixels: Vec<u8>,
    pad: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_h: u32,
    entries: HashMap<u64, AtlasEntry>,
    dirty: Option<IRect>,
    generation: u64,
}

impl Atlas {
    /// A `size × size` atlas (power of two recommended).
    pub fn new(size: u32) -> Self {
        Self {
            size,
            pixels: vec![0; (size * size * 4) as usize],
            pad: 1,
            cursor_x: 1,
            cursor_y: 1,
            shelf_h: 0,
            entries: HashMap::new(),
            dirty: None,
            generation: 0,
        }
    }

    /// Edge length in texels.
    #[inline]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// RGBA8 image, row-major, `size * size * 4` bytes.
    #[inline]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Bumped on every grow. The renderer recreates its texture on change.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The region changed since the last `take_dirty`, if any.
    pub fn dirty(&self) -> Option<IRect> {
        self.dirty
    }

    /// Hand the dirty region to the uploader and reset it.
    pub fn take_dirty(&mut self) -> Option<IRect> {
        self.dirty.take()
    }

    /// Number of cached entries (including zero-area whitespace ones).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: u64) -> Option<AtlasEntry> {
        self.entries.get(&key).copied()
    }

    /// Insert a coverage bitmap (`width * height` bytes, row-major) under
    /// `key`, stored as premultiplied white.
    pub fn insert_coverage(
        &mut self,
        key: u64,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        coverage: &[u8],
    ) -> AtlasEntry {
        if let Some(existing) = self.entries.get(&key) {
            return *existing;
        }
        let mut rgba = Vec::with_capacity(coverage.len() * 4);
        for &c in coverage {
            rgba.extend_from_slice(&[c, c, c, c]);
        }
        self.insert_raw(key, width, height, left, top, &rgba, false)
    }

    /// Insert a color glyph (straight-alpha, sRGB-encoded RGBA). Converted to
    /// premultiplied linear here.
    pub fn insert_rgba(
        &mut self,
        key: u64,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        rgba: &[u8],
    ) -> AtlasEntry {
        if let Some(existing) = self.entries.get(&key) {
            return *existing;
        }
        let lut = srgb_to_linear_lut();
        let mut out = rgba.to_vec();
        for px in out.chunks_exact_mut(4) {
            let a = px[3] as u32;
            for ch in px.iter_mut().take(3) {
                *ch = ((lut[*ch as usize] as u32 * a + 127) / 255) as u8;
            }
        }
        self.insert_raw(key, width, height, left, top, &out, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_raw(
        &mut self,
        key: u64,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        rgba: &[u8],
        is_color: bool,
    ) -> AtlasEntry {
        // Whitespace / empty glyph: a zero-area entry that still advances.
        if width == 0 || height == 0 || rgba.is_empty() {
            let entry = AtlasEntry { width, height, left, top, ..AtlasEntry::default() };
            self.entries.insert(key, entry);
            return entry;
        }
        debug_assert_eq!(rgba.len(), (width * height * 4) as usize);

        // Shelf packing, growing whenever the current page can't take it.
        loop {
            if self.cursor_x + width + self.pad > self.size {
                self.cursor_x = self.pad;
                self.cursor_y += self.shelf_h + self.pad;
                self.shelf_h = 0;
            }
            if self.cursor_y + height <= self.size && self.cursor_x + width + self.pad <= self.size {
                break;
            }
            if !self.grow() {
                prism_core::log_warn!("glyph atlas at max size ({0}x{0}); dropping glyph {1:#x}", self.size, key);
                let entry = AtlasEntry { left, top, ..AtlasEntry::default() };
                self.entries.insert(key, entry);
                return entry;
            }
        }

        let (x, y) = (self.cursor_x, self.cursor_y);
        let stride = (self.size * 4) as usize;
        let row_bytes = (width * 4) as usize;
        for r in 0..height as usize {
            let dst = (y as usize + r) * stride + x as usize * 4;
            self.pixels[dst..dst + row_bytes].copy_from_slice(&rgba[r * row_bytes..(r + 1) * row_bytes]);
        }
        let rect = IRect { x, y, w: width, h: height };
        self.dirty = Some(self.dirty.map_or(rect, |d| d.union(rect)));

        let entry = AtlasEntry {
            uv_min: [x as f32, y as f32],
            uv_max: [(x + width) as f32, (y + height) as f32],
            width,
            height,
            left,
            top,
            is_color,
        };
        self.cursor_x += width + self.pad;
        self.shelf_h = self.shelf_h.max(height);
        self.entries.insert(key, entry);
        entry
    }

    /// Double the atlas, keeping every texel at the same coordinates.
    fn grow(&mut self) -> bool {
        let new_size = self.size * 2;
        if new_size > MAX_ATLAS_SIZE {
            return false;
        }
        let old_stride = (self.size * 4) as usize;
        let new_stride = (new_size * 4) as usize;
        let mut pixels = vec![0u8; (new_size * new_size * 4) as usize];
        for row in 0..self.size as usize {
            pixels[row * new_stride..row * new_stride + old_stride]
                .copy_from_slice(&self.pixels[row * old_stride..(row + 1) * old_stride]);
        }
        prism_core::log_info!("glyph atlas grew {0}x{0} → {1}x{1}", self.size, new_size);
        self.pixels = pixels;
        self.size = new_size;
        self.generation += 1;
        // Everything must be re-uploaded into the new texture.
        self.dirty = Some(IRect { x: 0, y: 0, w: new_size, h: new_size });
        true
    }
}

fn srgb_to_linear_lut() -> &'static [u8; 256] {
    static LUT: OnceLock<[u8; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0u8; 256];
        for (i, v) in lut.iter_mut().enumerate() {
            let s = i as f32 / 255.0;
            let l = if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
            *v = (l * 255.0 + 0.5) as u8;
        }
        lut
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_tracks_dirty() {
        let mut a = Atlas::new(16);
        let e = a.insert_coverage(1, 4, 3, 1, 2, &[255; 12]);
        assert_eq!((e.width, e.height, e.left, e.top), (4, 3, 1, 2));
        assert_eq!(e.uv_min, [1.0, 1.0]);
        assert_eq!(e.uv_max, [5.0, 4.0]);
        assert!(!e.is_color);
        assert_eq!(a.dirty(), Some(IRect { x: 1, y: 1, w: 4, h: 3 }));
        // Texel (1,1) is premultiplied white.
        let i = (16 + 1) * 4;
        assert_eq!(&a.pixels()[i..i + 4], &[255, 255, 255, 255]);
        // Same key returns the cached entry, no new dirt.
        a.take_dirty();
        assert_eq!(a.insert_coverage(1, 9, 9, 0, 0, &[0; 81]), e);
        assert_eq!(a.dirty(), None);
        // Whitespace entries are zero-area but keep bearings.
        let ws = a.insert_coverage(2, 0, 0, 3, 0, &[]);
        assert_eq!(ws.width, 0);
        assert_eq!(ws.left, 3);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn wraps_shelves_and_grows() {
        let mut a = Atlas::new(8);
        // Three 3x3 glyphs: two fit on the first shelf (1+3+1+3+1 = 9 > 8, so
        // actually only one), forcing shelf wraps and eventually growth.
        for k in 0..6 {
            a.insert_coverage(k, 3, 3, 0, 0, &[128; 9]);
        }
        assert_eq!(a.size(), 16, "grew once");
        assert_eq!(a.generation(), 1);
        assert_eq!(a.dirty(), Some(IRect { x: 0, y: 0, w: 16, h: 16 }), "full re-upload after growth");
        // Every entry still points at its own texels.
        for k in 0..6 {
            let e = a.get(k).unwrap();
            let i = ((e.uv_min[1] as usize) * 16 + e.uv_min[0] as usize) * 4;
            assert_eq!(a.pixels()[i], 128, "glyph {k} survived the copy");
        }
    }

    #[test]
    fn color_glyphs_are_linear_premultiplied() {
        let mut a = Atlas::new(8);
        // sRGB (255, 128, 0) at 50% alpha.
        let e = a.insert_rgba(7, 1, 1, 0, 0, &[255, 128, 0, 128]);
        assert!(e.is_color);
        let px = &a.pixels()[(8 + 1) * 4..(8 + 1) * 4 + 4];
        assert_eq!(px[3], 128);
        assert_eq!(px[0], 128, "white stays white, then premultiplied");
        // sRGB 128 ≈ linear 55, premultiplied by 128/255 ≈ 28.
        assert!((26..=30).contains(&px[1]), "{px:?}");
        assert_eq!(px[2], 0);
    }

    #[test]
    fn refuses_past_max() {
        let mut a = Atlas::new(MAX_ATLAS_SIZE);
        let e = a.insert_coverage(1, MAX_ATLAS_SIZE, 4, 0, 0, &vec![1; (MAX_ATLAS_SIZE * 4) as usize]);
        assert_eq!(e.width, 0, "too wide to ever fit: dropped, not corrupted");
    }
}
