//! Render workspace: output settings, framed preview and progress.

use crate::app::App;
use crate::export::{ffmpeg_available, OutFormat};
use crate::ui::theme;
use egui::{vec2, Color32, Rect, RichText};

impl App {
    pub fn render_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::left("render_settings").resizable(true).default_size(320.0).size_range(260.0..=460.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.render_settings_ui(ui, frame));
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let avail = ui.max_rect().shrink(18.0);
            let aspect = self.render_settings.width as f32 / self.render_settings.height.max(1) as f32;
            let (mut w, mut h) = (avail.width(), avail.width() / aspect);
            if h > avail.height() - 70.0 {
                h = avail.height() - 70.0;
                w = h * aspect;
            }
            let rect = Rect::from_center_size(avail.center() - vec2(0.0, 30.0), vec2(w.max(32.0), h.max(32.0)));
            ui.painter().rect_filled(rect.expand(3.0), 4.0, Color32::BLACK);
            self.viewport(ui, frame, rect, false);
            let below = Rect::from_min_size(rect.left_bottom() + vec2(0.0, 12.0), vec2(rect.width(), 60.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(below), |ui| {
                if let Some(job) = &self.job {
                    let p = job.progress();
                    let elapsed = job.started.elapsed().as_secs_f32();
                    let eta = if p > 0.001 { elapsed / p - elapsed } else { 0.0 };
                    ui.add(egui::ProgressBar::new(p).text(format!(
                        "Frame {} / {}  ·  {:.0}%  ·  {:.1} fps  ·  about {:.0}s left",
                        job.next,
                        job.settings.end,
                        p * 100.0,
                        (job.next - job.settings.start) as f32 / elapsed.max(0.001),
                        eta
                    )).fill(theme::ACCENT));
                    if ui.button("Cancel").clicked() {
                        if let Some(j) = self.job.as_mut() {
                            j.cancel();
                        }
                        self.job = None;
                        self.set_status("Render cancelled");
                    }
                } else {
                    ui.label(RichText::new(format!(
                        "Preview of the final framing · {}×{} · frames {}–{} ({:.1} s)",
                        self.render_settings.width,
                        self.render_settings.height,
                        self.render_settings.start,
                        self.render_settings.end,
                        (self.render_settings.end - self.render_settings.start + 1) as f32 / self.render_settings.fps
                    )).color(theme::TEXT_DIM));
                }
            });
        });
    }

    fn render_settings_ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let busy = self.job.is_some();
        theme::section(ui, "Size");
        theme::card(ui, |ui| {
            let s = &mut self.render_settings;
            ui.horizontal_wrapped(|ui| {
                for (n, w, h) in [("720p", 1280, 720), ("1080p", 1920, 1080), ("1440p", 2560, 1440), ("4K", 3840, 2160), ("Vertical", 1080, 1920), ("Square", 1080, 1080)] {
                    if ui.selectable_label(s.width == w && s.height == h, n).clicked() {
                        s.width = w;
                        s.height = h;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut s.width).range(64..=7680).suffix(" px"));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut s.height).range(64..=7680).suffix(" px"));
            });
            ui.checkbox(&mut s.msaa, "Anti-aliasing (4× MSAA)");
        });
        theme::section(ui, "Time");
        theme::card(ui, |ui| {
            let s = &mut self.render_settings;
            ui.horizontal(|ui| {
                ui.label("Frames");
                ui.add(egui::DragValue::new(&mut s.start).range(0..=1_000_000));
                ui.label("to");
                let st = s.start;
                ui.add(egui::DragValue::new(&mut s.end).range(st..=1_000_000));
            });
            ui.horizontal(|ui| {
                if ui.button("Use timeline range").clicked() {
                    s.start = self.scene.play_start;
                    s.end = self.scene.play_end;
                }
                if ui.button("Whole animation").clicked() {
                    s.start = 0;
                    s.end = self.scene.end_frame().max(1);
                }
            });
            egui::ComboBox::from_label("Frame rate").selected_text(format!("{} fps", s.fps)).show_ui(ui, |ui| {
                for v in [24.0, 30.0, 60.0] {
                    ui.selectable_value(&mut s.fps, v, format!("{v} fps"));
                }
            });
        });
        theme::section(ui, "Output");
        let has_ff = ffmpeg_available();
        theme::card(ui, |ui| {
            let s = &mut self.render_settings;
            for f in OutFormat::ALL {
                let enabled = f == OutFormat::PngSequence || has_ff;
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.radio(s.format == f, f.label()).clicked() {
                        s.format = f;
                        if f.ext().is_empty() {
                            s.path = s.path.with_extension("");
                        } else {
                            s.path = s.path.with_extension(f.ext());
                        }
                    }
                });
            }
            if !has_ff {
                ui.label(RichText::new("Video files (MP4, WebM, MOV) need the free program ffmpeg. PNG sequences work now.").size(11.0).color(theme::KEY));
                if ui.button("Set up video export…").clicked() {
                    self.show_video_help = true;
                }
            }
            if matches!(s.format, OutFormat::Mp4 | OutFormat::WebM) {
                ui.add(egui::Slider::new(&mut s.quality, 12..=32).text("Quality (lower = better)"));
            }
            ui.checkbox(&mut s.include_audio, "Include the song");
            ui.label(RichText::new(s.path.display().to_string()).size(11.0).monospace());
            if ui.button("Choose where to save…").clicked() {
                let picked = if s.format == OutFormat::PngSequence {
                    rfd::FileDialog::new().pick_folder()
                } else {
                    rfd::FileDialog::new().set_file_name(format!("render.{}", s.format.ext())).save_file()
                };
                if let Some(p) = picked {
                    s.path = p;
                }
            }
        });
        ui.add_space(8.0);
        ui.add_enabled_ui(!busy && self.renderer.is_some(), |ui| {
            let b = egui::Button::new(RichText::new("🎬  Render animation").size(15.0).color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(ui.available_width(), 34.0));
            if ui.add(b).clicked() {
                self.start_render_job();
            }
            if ui.add(egui::Button::new("📷  Render current frame as image…").min_size(vec2(ui.available_width(), 26.0))).clicked() {
                self.render_still_dialog(frame);
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("Frames render on the GPU as fast as possible (not real time) with physics stepped exactly once per frame, so results match playback.").size(11.0).color(theme::TEXT_DIM));
    }
}
