//! Lights section of the Effects workspace: lighting style, main light, placed lights and rigs.

use crate::app::App;
use crate::material::{builtin_light_rigs, LightKind, SceneLight};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, RichText, Stroke};
use glam::Vec3;

impl App {
    pub fn lights_section(&mut self, ui: &mut egui::Ui) {
        self.lighting_style(ui);

        theme::section(ui, "Main light (sun)");
        theme::card(ui, |ui| {
            let l = &mut self.scene.light;
            let mut col = l.color.to_array();
            ui.horizontal(|ui| {
                if ui.color_edit_button_rgb(&mut col).changed() {
                    self.scene.light_edited = true;
                }
                ui.label("Colour and brightness");
            });
            l.color = Vec3::from(col);
            ui.horizontal(|ui| {
                ui.label("Direction");
                let mut ch = false;
                ch |= ui.add(egui::DragValue::new(&mut l.direction.x).speed(0.01).range(-1.0..=1.0)).changed();
                ch |= ui.add(egui::DragValue::new(&mut l.direction.y).speed(0.01).range(-1.0..=1.0)).changed();
                ch |= ui.add(egui::DragValue::new(&mut l.direction.z).speed(0.01).range(-1.0..=1.0)).changed();
                if ch {
                    self.scene.light_edited = true;
                }
            });
            ui.label(RichText::new("The sun casts the shadows. Key it in Animation → Camera & Light.").size(10.5).color(theme::TEXT_DIM));
        });

        theme::section(ui, "Light sources");
        ui.horizontal(|ui| {
            for k in LightKind::ALL {
                if ui.button(format!("＋ {}", k.label())).clicked() {
                    let mut l = SceneLight::new(k);
                    let n = self.graph.lights.len();
                    l.position = [(n as f32 * 9.0) - 9.0, 16.0, -12.0];
                    l.name = format!("{} {}", k.label(), n + 1);
                    self.graph.lights.push(l);
                    self.light_sel = Some(self.graph.lights.len() - 1);
                }
            }
        });
        if self.graph.lights.is_empty() {
            ui.label(RichText::new("No extra lights. Add point lights, spotlights or directional lights above, or load a rig below.").size(11.0).color(theme::TEXT_DIM));
        }
        let mut remove = None;
        let mut dup = None;
        let nl = self.graph.lights.len();
        for i in 0..nl {
            let sel = self.light_sel == Some(i);
            let l = &mut self.graph.lights[i];
            egui::Frame::new()
                .fill(if sel { theme::ACCENT.linear_multiply(0.18) } else { theme::BG2 })
                .stroke(Stroke::new(1.0, if sel { theme::ACCENT } else { theme::BG3 }))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut l.enabled, "");
                        let c = Color32::from_rgb((l.color[0] * 255.0) as u8, (l.color[1] * 255.0) as u8, (l.color[2] * 255.0) as u8);
                        ui.label(RichText::new("●").color(c));
                        if ui.selectable_label(sel, RichText::new(&l.name).strong()).clicked() {
                            self.light_sel = if sel { None } else { Some(i) };
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new("×").frame(false)).on_hover_text("Remove").clicked() {
                                remove = Some(i);
                            }
                            if ui.add(egui::Button::new("⧉").frame(false)).on_hover_text("Duplicate").clicked() {
                                dup = Some(i);
                            }
                        });
                    });
                    if sel {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            ui.text_edit_singleline(&mut l.name);
                        });
                        egui::ComboBox::from_id_salt(("lk", i)).selected_text(l.kind.label()).show_ui(ui, |ui| {
                            for k in LightKind::ALL {
                                ui.selectable_value(&mut l.kind, k, k.label());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.color_edit_button_rgb(&mut l.color);
                            ui.add(egui::Slider::new(&mut l.intensity, 0.0..=20.0).text("Intensity"));
                        });
                        if l.kind != LightKind::Directional {
                            ui.horizontal(|ui| {
                                ui.label("Position");
                                for v in l.position.iter_mut() {
                                    ui.add(egui::DragValue::new(v).speed(0.1));
                                }
                            });
                            ui.add(egui::Slider::new(&mut l.range, 2.0..=300.0).text("Reach"));
                        }
                        if l.kind != LightKind::Point {
                            ui.add(egui::Slider::new(&mut l.pitch, -90.0..=90.0).text("Tilt (°)"));
                            ui.add(egui::Slider::new(&mut l.yaw, -180.0..=180.0).text("Turn (°)"));
                        }
                        if l.kind == LightKind::Spot {
                            ui.add(egui::Slider::new(&mut l.cone, 2.0..=80.0).text("Cone (°)"));
                            ui.add(egui::Slider::new(&mut l.softness, 0.0..=1.0).text("Edge softness"));
                        }
                        ui.label(RichText::new("Drag the light's icon in the view to move it. Extra lights don't cast shadows.").size(10.5).color(theme::TEXT_DIM));
                    }
                });
            ui.add_space(2.0);
        }
        if let Some(i) = dup {
            let mut l = self.graph.lights[i].clone();
            l.name.push_str(" copy");
            l.position[0] += 4.0;
            self.graph.lights.push(l);
        }
        if let Some(i) = remove {
            self.graph.lights.remove(i);
            self.light_sel = None;
        }

        theme::section(ui, "Light rigs (presets)");
        ui.horizontal_wrapped(|ui| {
            for (name, rig) in builtin_light_rigs() {
                if ui.button(name).on_hover_text("Replace the extra lights with this rig").clicked() {
                    if self.graph.lights.is_empty() {
                        self.graph.lights = rig;
                        self.light_sel = None;
                        self.set_status(format!("Loaded light rig “{name}”"));
                    } else {
                        self.ask(
                            "Replace your lights?",
                            &format!("Your {} extra lights will be replaced by the “{name}” rig.", self.graph.lights.len()),
                            "Replace",
                            crate::ui::home::ConfirmAction::SetLights(rig, name.to_string()),
                        );
                    }
                }
            }
        });
        let mine = self.user_presets.light_rigs.clone();
        if !mine.is_empty() {
            ui.label(RichText::new("My rigs").size(11.0).color(theme::TEXT_DIM));
            ui.horizontal_wrapped(|ui| {
                for (name, rig) in &mine {
                    if ui.button(name).clicked() {
                        self.graph.lights = rig.clone();
                        self.light_sel = None;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.preset_name_buf).hint_text("rig name").desired_width(150.0));
            if ui.add_enabled(!self.graph.lights.is_empty(), egui::Button::new("💾 Save lights as my rig")).clicked() {
                let name = if self.preset_name_buf.trim().is_empty() { format!("My rig {}", self.user_presets.light_rigs.len() + 1) } else { self.preset_name_buf.trim().to_string() };
                self.user_presets.light_rigs.retain(|(n, _)| *n != name);
                self.user_presets.light_rigs.push((name.clone(), self.graph.lights.clone()));
                match self.user_presets.save() {
                    Ok(_) => self.set_status(format!("Saved light rig “{name}” for every project")),
                    Err(e) => self.set_error(format!("Could not save: {e}")),
                }
            }
        });
    }

    /// Light icons drawn over the viewport (Lights tab): click to select, drag to move.
    pub fn light_overlay(&mut self, ui: &mut egui::Ui) {
        let rect = self.viewport_rect;
        let aspect = rect.width() / rect.height().max(1.0);
        let pm = self.view_proj.unwrap_or_else(|| self.scene.camera.proj(aspect));
        let proj = crate::ui::viewport::Projector { vp: pm * self.scene.camera.view(), rect };
        let painter = ui.painter_at(rect);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut hovered = None;
        for (i, l) in self.graph.lights.iter().enumerate() {
            let pos = if l.kind == LightKind::Directional { self.scene.camera.target + Vec3::new(0.0, 14.0, 0.0) - l.direction() * 6.0 } else { Vec3::from(l.position) };
            let Some((sp, _)) = proj.project(pos) else { continue };
            let col = Color32::from_rgb((l.color[0].min(1.0) * 255.0) as u8, (l.color[1].min(1.0) * 255.0) as u8, (l.color[2].min(1.0) * 255.0) as u8);
            let sel = self.light_sel == Some(i);
            let alpha = if l.enabled { 255 } else { 90 };
            let col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha);
            painter.circle_filled(sp, if sel { 9.0 } else { 7.0 }, col);
            painter.circle_stroke(sp, if sel { 12.0 } else { 9.0 }, Stroke::new(if sel { 2.0 } else { 1.0 }, Color32::WHITE));
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let d = vec2(a.cos(), a.sin());
                painter.line_segment([sp + d * 13.0, sp + d * 17.0], Stroke::new(1.2, col));
            }
            if l.kind != LightKind::Point {
                if let Some((ep, _)) = proj.project(pos + l.direction() * 6.0) {
                    painter.arrow(sp, (ep - sp).normalized() * 34.0, Stroke::new(1.6, col));
                }
            }
            painter.text(sp + vec2(0.0, 22.0), egui::Align2::CENTER_TOP, &l.name, egui::FontId::proportional(11.0), Color32::from_white_alpha(200));
            if let Some(mp) = pointer {
                if (mp - sp).length() < 14.0 {
                    hovered = Some(i);
                }
            }
        }
        let (pressed, down, released, delta) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.delta()));
        if pressed {
            if let Some(h) = hovered {
                self.light_sel = Some(h);
                self.light_drag = Some(h);
            }
        }
        if let (true, Some(i)) = (down, self.light_drag) {
            if let Some(l) = self.graph.lights.get_mut(i) {
                if l.kind != LightKind::Directional {
                    let pos = Vec3::from(l.position);
                    let depth = (pos - self.scene.camera.eye()).length().max(1.0);
                    let units = depth * 2.0 * (self.scene.camera.fov.to_radians() * 0.5).tan() / rect.height().max(1.0);
                    let inv = self.scene.camera.view().inverse();
                    let right = inv.transform_vector3(Vec3::X).normalize_or_zero();
                    let up = inv.transform_vector3(Vec3::Y).normalize_or_zero();
                    let p = pos + (right * delta.x - up * delta.y) * units;
                    l.position = p.to_array();
                } else {
                    l.yaw += delta.x * 0.4;
                    l.pitch = (l.pitch - delta.y * 0.4).clamp(-90.0, 90.0);
                }
            }
        }
        if released {
            self.light_drag = None;
        }
        if hovered.is_some() || self.light_drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        let _ = pos2(0.0, 0.0);
    }
}
