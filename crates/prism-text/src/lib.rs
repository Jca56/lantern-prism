//! Prism text: fonts, shaping, layout, rasterization and atlas packing.
//! **GPU-free** — this crate never touches wgpu (D018). It produces positioned
//! glyph quads with texel-space atlas coordinates plus a CPU RGBA atlas image
//! with dirty-rect tracking; `prism-render` uploads the image and draws the
//! quads through the same 2D pass that draws panels.
//!
//! The stack, ported from Alva's `lntrn-text` engine:
//! - **Fonts**: sfnt/ttc containers, TrueType `glyf` (incl. composites), CFF
//!   Type2 charstrings, variable fonts (fvar/avar/gvar/HVAR instances),
//!   CBDT/CBLC color emoji (own PNG decoder), COLRv0 layers.
//! - **Discovery**: runtime scan of XDG/system/`~/.prism/fonts` directories
//!   via targeted metadata reads; family/weight/width/style matching;
//!   per-glyph fallback (fallback chain + cmap-coverage search).
//! - **Shaping**: grapheme-cluster-aware resolution, GSUB (ligatures,
//!   contextual, per-script plans, Arabic forms), GPOS (kerning, marks),
//!   UAX#9 BiDi with reordering + mirroring.
//! - **Layout**: whole-line shaping with cluster tracking, UAX#14 breaking,
//!   a colorless shaped-layout LRU.
//! - **Raster**: signed-area scanline AA with quarter-pixel subpixel bins.
//!
//! Text lives in **pixel space and `f32`** by design: it is a picture, not
//! geometry, and the f64 rule (D004) is about geometry.
//!
//! Unicode property tables are generated from the UCD files in `ucd/` by
//! `examples/gen_unicode.rs`.

pub mod atlas;
mod engine;
mod font;
mod layout;
mod raster;
mod shape;
pub mod unicode;

pub use atlas::{Atlas, AtlasEntry, IRect};
pub use engine::{Family, GlyphQuad, TextEngine, TextMetrics, TextStyle};
pub use font::FontError;

/// Snap sizes to a 0.25px grid so animated font sizes reuse cached rasters.
pub(crate) fn quantize_px(size: f32) -> f32 {
    (size * 4.0).round().max(1.0) / 4.0
}

/// Font weight for styled text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

/// Font style for styled text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}
