//! Placeholder until `prism-viewport` arrives in Phase 5: a recessed well
//! with a grid, so the area reads as the stage everything else surrounds.

use prism_math::{Color, Rect, Vec2};

use crate::ui::Ui;

pub fn draw(ui: &mut Ui) {
    let clip = ui.clip();
    let well = ui.theme.field;
    ui.fill_square(clip, well);
    let step = ui.m.px(50.0);
    let line = well.scale_rgb(1.45);
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
    // Inner shadow along the top and left, like a well cut into the panel.
    let d = ui.m.px(15.0);
    ui.draw.rect_gradient(Rect::new(clip.min, Vec2::new(clip.max.x, clip.min.y + d)), Color::BLACK.fade(0.45), Color::TRANSPARENT);
    let left = Rect::new(clip.min, Vec2::new(clip.min.x + d, clip.max.y));
    ui.draw.rect(left, Color::BLACK.fade(0.12));
    let style = ui.heading_style();
    ui.text_centered("3D Viewport · Phase 5", &style, clip, ui.theme.text_dim);
}
