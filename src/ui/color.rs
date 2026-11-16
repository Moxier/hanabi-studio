//! Color workspace: lift/gamma/gain wheels, tone sliders, looks, presets and a live histogram.

use crate::app::App;
use crate::grade::{ColorGrade, LOOKS};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, Mesh, Rect, RichText, Sense, Stroke};

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let c = egui::ecolor::Hsva::new(h, s, v, 1.0);
    c.into()
}

/// A colour wheel that edits an RGB offset (balanced so the wheel doesn't change brightness).
fn wheel(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], range: f32) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.label(RichText::new(label).strong());
        let size = 118.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
        let c = rect.center();
        let r = size * 0.5 - 4.0;
        let p = ui.painter_at(rect.expand(2.0));
        // colour disc as a triangle fan with vertex colours
        let mut mesh = Mesh::default();
        let n = 72;
        mesh.colored_vertex(c, Color32::from_gray(128));
        for k in 0..=n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(c + vec2(a.cos(), -a.sin()) * r, hsv(a / std::f32::consts::TAU, 0.75, 0.9));
        }
        for k in 1..=n as u32 {
            mesh.add_triangle(0, k, k + 1);
        }
        p.add(mesh);
        p.circle_stroke(c, r, Stroke::new(1.5, theme::BG3));
        p.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], Stroke::new(0.5, Color32::from_black_alpha(80)));
        p.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], Stroke::new(0.5, Color32::from_black_alpha(80)));
        // value -> puck: project the offset (minus its average) onto the hue plane
        let avg = (value[0] + value[1] + value[2]) / 3.0;
        let d = [value[0] - avg, value[1] - avg, value[2] - avg];
        let px = d[0] - 0.5 * d[1] - 0.5 * d[2];
        let py = 0.866 * (d[1] - d[2]);
        let puck = c + vec2(px, -py) / range * r * 0.9;
        if resp.dragged() || resp.clicked() {
            if let Some(mp) = resp.interact_pointer_pos() {
                let mut v = (mp - c) / (r * 0.9);
                if v.length() > 1.0 {
                    v = v.normalized();
                }
                let (x, y) = (v.x * range, -v.y * range);
                // inverse of the projection above (keep the average)
                let r_ = avg + x * 2.0 / 3.0;
                let g_ = avg - x / 3.0 + y / 1.732;
                let b_ = avg - x / 3.0 - y / 1.732;
                *value = [r_, g_, b_];
                changed = true;
            }
        }
        if resp.double_clicked() {
            *value = [avg; 3];
            changed = true;
        }
        p.circle_filled(puck, 6.0, Color32::WHITE);
        p.circle_stroke(puck, 6.0, Stroke::new(1.5, Color32::BLACK));
        let mut lum = avg;
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 84.0;
            if ui.add(egui::Slider::new(&mut lum, -range..=range).show_value(false)).on_hover_text("Brightness").changed() {
                let dl = lum - avg;
                for c in value.iter_mut() {
                    *c += dl;
                }
                changed = true;
            }
            if ui.small_button("⟲").on_hover_text("Reset").clicked() {
                *value = [0.0; 3];
                changed = true;
            }
        });
    });
    changed
}

impl App {
    pub fn color_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::right("grade_side").resizable(true).default_size(290.0).size_range(240.0..=420.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.grade_side(ui));
        });
        egui::Panel::bottom("grade_wheels").resizable(false).exact_size(268.0).show(ui, |ui| {
            self.grade_wheels(ui);
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let rect = ui.max_rect();
            self.viewport(ui, frame, rect, false);
            if self.grade.split_compare {
                let x = rect.left() + rect.width() * self.grade.split_pos;
                let p = ui.painter_at(rect);
                p.text(pos2(x - 8.0, rect.top() + 10.0), egui::Align2::RIGHT_TOP, "Before", egui::FontId::proportional(12.0), Color32::WHITE);
                p.text(pos2(x + 8.0, rect.top() + 10.0), egui::Align2::LEFT_TOP, "After", egui::FontId::proportional(12.0), Color32::WHITE);
                let handle = Rect::from_center_size(pos2(x, rect.center().y), vec2(14.0, rect.height()));
                let r = ui.interact(handle, ui.scope_id().with("split"), Sense::drag());
                if r.dragged() {
                    if let Some(mp) = r.interact_pointer_pos() {
                        self.grade.split_pos = ((mp.x - rect.left()) / rect.width()).clamp(0.02, 0.98);
                    }
                }
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
        });
    }

    fn grade_wheels(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let g = &mut self.grade;
            wheel(ui, "Shadows (lift)", &mut g.lift, 0.5);
            ui.add_space(10.0);
            wheel(ui, "Midtones (gamma)", &mut g.gamma, 0.5);
            ui.add_space(10.0);
            wheel(ui, "Highlights (gain)", &mut g.gain, 0.5);
            ui.add_space(18.0);
            ui.separator();
            ui.vertical(|ui| {
                ui.spacing_mut().slider_width = 190.0;
                ui.label(RichText::new("Tone").strong());
                ui.add(egui::Slider::new(&mut g.exposure, -3.0..=3.0).text("Exposure").suffix(" EV"));
                ui.add(egui::Slider::new(&mut g.contrast, 0.5..=1.6).text("Contrast"));
                ui.add(egui::Slider::new(&mut g.highlights, -1.0..=1.0).text("Highlights"));
                ui.add(egui::Slider::new(&mut g.shadows, -1.0..=1.0).text("Shadows"));
                ui.label(RichText::new("Colour").strong());
                ui.add(egui::Slider::new(&mut g.temperature, -1.0..=1.0).text("Temperature"));
                ui.add(egui::Slider::new(&mut g.tint, -1.0..=1.0).text("Tint"));
                ui.add(egui::Slider::new(&mut g.saturation, 0.0..=2.0).text("Saturation"));
                ui.add(egui::Slider::new(&mut g.vibrance, -1.0..=1.0).text("Vibrance"));
            });
        });
    }

    fn grade_side(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Grading");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.grade.enabled, "Enabled");
            ui.checkbox(&mut self.grade.split_compare, "Before / after split");
        });
        if ui.button("Reset all").clicked() {
            self.grade = ColorGrade { split_compare: self.grade.split_compare, ..Default::default() };
        }
        theme::section(ui, "Scopes");
        theme::card(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 110.0), Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 3.0, theme::BG0);
            if self.histogram.is_empty() {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "measuring…", egui::FontId::proportional(11.0), theme::TEXT_DIM);
            }
            let n = self.histogram.len().max(1) as f32;
            let cols = [Color32::from_rgba_unmultiplied(255, 80, 80, 110), Color32::from_rgba_unmultiplied(80, 255, 120, 110), Color32::from_rgba_unmultiplied(90, 150, 255, 110)];
            for (ch, col) in cols.iter().enumerate() {
                let mut mesh = Mesh::default();
                for (i, b) in self.histogram.iter().enumerate() {
                    let x0 = rect.left() + rect.width() * i as f32 / n;
                    let x1 = rect.left() + rect.width() * (i + 1) as f32 / n;
                    let h = b[ch].sqrt() * rect.height();
                    mesh.add_colored_rect(Rect::from_min_max(pos2(x0, rect.bottom() - h), pos2(x1, rect.bottom())), *col);
                }
                p.add(mesh);
            }
            ui.label(RichText::new("RGB histogram of the graded picture").size(10.5).color(theme::TEXT_DIM));
        });
        theme::section(ui, "Look");
        theme::card(ui, |ui| {
            egui::ComboBox::from_id_salt("look").selected_text(LOOKS[self.grade.look.min(LOOKS.len() - 1)]).width(200.0).show_ui(ui, |ui| {
                for (i, l) in LOOKS.iter().enumerate() {
                    ui.selectable_value(&mut self.grade.look, i, *l);
                }
            });
            ui.add(egui::Slider::new(&mut self.grade.look_strength, 0.0..=1.0).text("Strength"));
        });
        theme::section(ui, "Grade presets");
        ui.horizontal_wrapped(|ui| {
            for (name, g) in ColorGrade::presets() {
                if ui.button(name).clicked() {
                    let split = (self.grade.split_compare, self.grade.split_pos);
                    self.grade = g;
                    self.grade.split_compare = split.0;
                    self.grade.split_pos = split.1;
                }
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("Grading is the last step for every frame, in the viewport and in final renders.").size(11.0).color(theme::TEXT_DIM));
    }
}
