//! Every widget, live, for poking at.

use crate::ui::Ui;

#[derive(Clone, Debug)]
pub struct GalleryState {
    pub clicks: u32,
    pub tab: usize,
    pub toggle_a: bool,
    pub toggle_b: bool,
    pub slider: f64,
    pub number: f64,
    pub count: i64,
    pub text: String,
    pub choice: usize,
    pub selected: usize,
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            clicks: 0,
            tab: 0,
            toggle_a: true,
            toggle_b: false,
            slider: 0.35,
            number: 1.5,
            count: 3,
            text: "Type here".to_owned(),
            choice: 1,
            selected: 2,
        }
    }
}

const CHOICES: [&str; 4] = ["Solid", "Wireframe", "Material Preview", "Rendered"];

pub fn draw(ui: &mut Ui, g: &mut GalleryState) {
    ui.tabs(&mut g.tab, &["Controls", "Text", "Lists"]);
    ui.space(ui.m.gap);
    match g.tab {
        0 => controls(ui, g),
        1 => text(ui, g),
        _ => lists(ui, g),
    }
}

fn controls(ui: &mut Ui, g: &mut GalleryState) {
    ui.scroll_area("controls", None, |ui| {
        ui.heading("Buttons");
        ui.row(|ui| {
            if ui.button("Click me").clicked {
                g.clicks += 1;
            }
            if ui.button("Reset").clicked {
                g.clicks = 0;
            }
            ui.label(&format!("Clicked {} times", g.clicks));
        });
        if ui.button_wide("Wide button").clicked {
            g.clicks += 10;
        }
        ui.separator();
        ui.heading("Toggles");
        ui.toggle("Show overlays", &mut g.toggle_a);
        ui.toggle("Snap to grid", &mut g.toggle_b);
        ui.separator();
        ui.heading("Numbers");
        ui.slider("Opacity", &mut g.slider, 0.0, 1.0, 0.0);
        ui.labelled("Extrude", |ui| {
            ui.drag_value("", &mut g.number, 0.01, None, 3);
        });
        ui.labelled("Segments", |ui| {
            ui.drag_int("", &mut g.count, Some((1, 64)));
        });
        ui.labelled("Position", |ui| {
            ui.row(|ui| {
                let mut v = [g.number, g.slider, g.count as f64];
                ui.push_id("pos");
                for (k, n) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.push_index(k);
                    ui.drag_value(n, &mut v[k], 0.01, None, 2);
                    ui.pop_id();
                }
                ui.pop_id();
            });
        });
        ui.separator();
        ui.heading("Choice");
        ui.labelled("Shading", |ui| {
            ui.dropdown("shading", &mut g.choice, &CHOICES);
        });
        ui.row(|ui| {
            if let Some(i) = ui.menu_button("Menu", &["New", "Open…", "Save", "Quit"]) {
                g.clicks += i as u32;
            }
            ui.dropdown("shading2", &mut g.choice, &CHOICES);
        });
    });
}

fn text(ui: &mut Ui, g: &mut GalleryState) {
    ui.heading("Text field");
    ui.text_field("name", &mut g.text);
    ui.label_dim(&format!("{} bytes, {} chars", g.text.len(), g.text.chars().count()));
    ui.separator();
    ui.heading("Paragraph");
    ui.paragraph(
        "Immediate-mode widgets inside a retained area tree. Everything you see here is re-declared on every \
         rebuild, and a rebuild only happens when an input event arrives. Idle costs nothing.",
    );
    ui.separator();
    ui.heading("Weights");
    let body = ui.text_style();
    let w = ui.avail_width();
    for (name, style) in [
        ("Regular", body.clone()),
        ("Bold", body.clone().bold()),
        ("Italic", body.clone().italic()),
        ("Mono", body.clone().mono()),
    ] {
        let r = ui.alloc(prism_math::Vec2::new(crate::ui::FILL, ui.m.widget_h));
        ui.text_in_rect(&format!("{name}: The quick brown fox jumps over the lazy dog"), &style, r, ui.theme.text);
        let _ = w;
    }
}

fn lists(ui: &mut Ui, g: &mut GalleryState) {
    ui.heading("Selectable list in a scroll area");
    ui.scroll_area("list", None, |ui| {
        for i in 0..40 {
            ui.push_index(i);
            if ui.selectable(&format!("Object {:02}", i + 1), g.selected == i).clicked {
                g.selected = i;
            }
            ui.pop_id();
        }
    });
}
