//! The Phase 1 scene: a header, a panel, big text. Static — nothing here
//! updates on its own, so an idle window costs nothing. Sizes are in logical
//! pixels, multiples of 5, scaled to physical.

use prism_math::{Color, Rect, Vec2};
use prism_render::DrawList;
use prism_text::{TextEngine, TextStyle};

// Placeholder palette: neutral, dark, high contrast. Decided properly when
// there are pixels to argue about.
const HEADER: Color = Color::hex(0x1E1E1E);
const PANEL: Color = Color::hex(0x202020);
const BORDER: Color = Color::hex(0x303030);
const TEXT: Color = Color::hex(0xE6E6E6);
const DIM: Color = Color::hex(0x909090);

fn c(color: Color) -> [f32; 4] {
    color.to_gpu()
}

pub fn build(draw: &mut DrawList, text: &mut TextEngine, size: [u32; 2], scale: f64) {
    let (w, h) = (size[0] as f64, size[1] as f64);
    let s = |logical: f64| (logical * scale).round();
    let mut quads = Vec::new();

    // Header bar.
    let header_h = s(45.0);
    draw.rect(Rect::from_xywh(0.0, 0.0, w, header_h), HEADER);
    draw.hline(0.0, w, header_h - s(1.0), s(1.0), BORDER);
    let title = TextStyle::new(s(25.0) as f32).bold();
    let ascent = text.ascent(&title) as f64;
    let ty = ((header_h - title.line_height() as f64) * 0.5).round();
    text.place("Prism", &title, s(20.0) as f32, ty as f32, w as f32, c(TEXT), &mut quads);
    let _ = ascent;

    // Panel.
    let margin = s(20.0);
    let panel = Rect::new(Vec2::new(margin, header_h + margin), Vec2::new(w - margin, h - margin));
    draw.rounded_rect(panel, s(10.0), PANEL);
    draw.stroke_rect(panel, s(1.0), s(10.0), BORDER);

    // Text block inside the panel, clipped to it.
    draw.push_clip(panel.shrink(s(5.0)));
    let x = panel.min.x + s(30.0);
    let mut y = panel.min.y + s(30.0);
    let max_w = (panel.width() - s(60.0)) as f32;

    let big = TextStyle::new(s(60.0) as f32).bold();
    let m = text.place("Phase 1 · engine slice", &big, x as f32, y as f32, max_w, c(TEXT), &mut quads);
    y += m.height as f64 + s(10.0);

    let body = TextStyle::new(s(30.0) as f32);
    let lines: [(&str, TextStyle, Color); 7] = [
        ("One window, one 2D pass, one atlas. Rects, rounded corners, strokes and glyphs in a single draw call.", body.clone(), TEXT),
        ("The quick brown fox jumps over the lazy dog 🦊 — AVATAR Wave To Yo", body.clone(), TEXT),
        ("Ligatures and kerning: => != >= <= ffi ffl", body.clone(), DIM),
        ("こんにちは世界 · مرحبا بالعالم · שלום עולם · Привет мир", body.clone(), TEXT),
        ("Bold weight for headings and labels", body.clone().bold(), TEXT),
        ("Italic for the occasional aside", body.clone().italic(), DIM),
        ("fn main() { let extrude = 1.0_f64; }  // monospace", body.clone().mono(), DIM),
    ];
    for (line, style, color) in &lines {
        let m = text.place(line, style, x as f32, y as f32, max_w, c(*color), &mut quads);
        y += m.height as f64 + s(10.0);
        if y > panel.max.y {
            break;
        }
    }

    // A row of shapes at the bottom of the panel.
    let shape_y = panel.max.y - s(30.0) - s(60.0);
    if shape_y > y {
        let mut sx = x;
        for radius in [0.0, 5.0, 15.0, 30.0] {
            let r = Rect::from_xywh(sx, shape_y, s(60.0), s(60.0));
            draw.rounded_rect(r, s(radius), BORDER);
            draw.stroke_rect(r, s(2.0), s(radius), DIM);
            sx += s(80.0);
        }
        draw.line(Vec2::new(sx, shape_y + s(60.0)), Vec2::new(sx + s(120.0), shape_y), s(3.0), DIM);
    }
    draw.pop_clip();
    draw.glyphs(&quads);
}
