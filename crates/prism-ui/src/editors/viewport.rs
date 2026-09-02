//! Placeholder until `prism-viewport` arrives in Phase 5.

use prism_math::Vec2;

use crate::ui::Ui;

pub fn draw(ui: &mut Ui) {
    let clip = ui.clip();
    let darker = ui.theme.bg.lerp(ui.theme.panel, 0.5);
    ui.fill_square(clip, darker);
    // A plain grid so the area reads as a viewport-to-be.
    let step = ui.m.px(50.0);
    let line = ui.theme.border;
    let mut x = clip.min.x + step;
    while x < clip.max.x {
        ui.draw.vline(x.round(), clip.min.y, clip.max.y, ui.m.border, line);
        x += step;
    }
    let mut y = clip.min.y + step;
    while y < clip.max.y {
        ui.draw.hline(clip.min.x, clip.max.x, y.round(), ui.m.border, line);
        y += step;
    }
    let style = ui.heading_style();
    ui.text_centered("3D Viewport · Phase 5", &style, clip, ui.theme.text_dim);
    let _ = Vec2::ZERO;
}
