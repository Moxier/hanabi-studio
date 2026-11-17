//! Render workspace: quick presets, picture size, length, video frame rate (kept separate from
//! the animation's own frame rate), file type, a summary, the framed preview and progress.

use crate::app::App;
use crate::export::{ffmpeg_available, OutFormat, RenderSettings};
use crate::ui::theme;
use egui::{vec2, Color32, Rect, RichText};

/// 8.0 s → "0:08.0", 75 s → "1:15.0"
pub fn fmt_len(secs: f32) -> String {
    let s = secs.max(0.0);
    format!("{}:{:04.1}", (s / 60.0).floor() as u32, s % 60.0)
}

fn aspect(w: u32, h: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(w, h).max(1);
    let (a, b) = (w / g, h / g);
    match (a, b) {
        (16, 9) | (9, 16) | (1, 1) | (4, 3) | (21, 9) => format!("{a}:{b}"),
        _ => format!("{:.2}:1", w as f32 / h.max(1) as f32),
    }
}

struct Preset {
    name: &'static str,
    sub: &'static str,
    w: u32,
    h: u32,
    fps: f32,
    format: OutFormat,
}

const PRESETS: [Preset; 6] = [
    Preset { name: "YouTube HD", sub: "1080p · 60 fps · MP4", w: 1920, h: 1080, fps: 60.0, format: OutFormat::Mp4 },
    Preset { name: "YouTube 4K", sub: "2160p · 60 fps · MP4", w: 3840, h: 2160, fps: 60.0, format: OutFormat::Mp4 },
    Preset { name: "Shorts / TikTok", sub: "1080×1920 · 60 fps", w: 1080, h: 1920, fps: 60.0, format: OutFormat::Mp4 },
    Preset { name: "Classic 30 fps", sub: "1080p · 30 fps · MP4", w: 1920, h: 1080, fps: 30.0, format: OutFormat::Mp4 },
    Preset { name: "For editing", sub: "1080p · 60 fps · ProRes MOV", w: 1920, h: 1080, fps: 60.0, format: OutFormat::Mov },
    Preset { name: "Image frames", sub: "1080p · PNG sequence", w: 1920, h: 1080, fps: 30.0, format: OutFormat::PngSequence },
];

fn set_format(s: &mut RenderSettings, f: OutFormat) {
    s.format = f;
    s.path = if f.ext().is_empty() { s.path.with_extension("") } else { s.path.with_extension(f.ext()) };
}

/// A wide selectable tile with a title and a small second line.
fn tile(ui: &mut egui::Ui, selected: bool, title: &str, sub: &str, w: f32) -> egui::Response {
    let resp = egui::Frame::new()
        .fill(if selected { theme::ACCENT.linear_multiply(0.18) } else { theme::BG2 })
        .stroke(egui::Stroke::new(1.0, if selected { theme::ACCENT } else { theme::BG3 }))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(w - 16.0);
            ui.label(RichText::new(title).size(12.5).strong().color(Color32::WHITE));
            ui.label(RichText::new(sub).size(10.5).color(theme::TEXT_DIM));
        })
        .response
        .interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

impl App {
    pub fn render_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::left("render_settings").resizable(true).default_size(360.0).size_range(300.0..=480.0).show(ui, |ui| {
            // Summary and the Render button stay in view at the bottom; settings scroll above.
            egui::Panel::bottom("render_go").frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 8))).show(ui, |ui| self.render_footer(ui, frame));
            egui::ScrollArea::vertical().show(ui, |ui| self.render_settings_ui(ui));
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let avail = ui.max_rect().shrink(18.0);
            let aspect_r = self.render_settings.width as f32 / self.render_settings.height.max(1) as f32;
            let (mut w, mut h) = (avail.width(), avail.width() / aspect_r);
            if h > avail.height() - 70.0 {
                h = avail.height() - 70.0;
                w = h * aspect_r;
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
                        "Video frame {} / {}  ·  {:.0}%  ·  {:.1} frames/s  ·  about {} left",
                        job.next,
                        job.total(),
                        p * 100.0,
                        job.next as f32 / elapsed.max(0.001),
                        fmt_len(eta)
                    )).fill(theme::ACCENT));
                    if ui.button("Cancel").clicked() {
                        if let Some(j) = self.job.as_mut() {
                            j.cancel();
                        }
                        self.job = None;
                        self.set_status("Render cancelled");
                    }
                } else {
                    let mut s = self.render_settings.clone();
                    s.anim_fps = self.scene.fps;
                    ui.label(RichText::new(format!(
                        "Preview of the final framing · {}×{} · {} fps video · {} long",
                        s.width,
                        s.height,
                        s.fps,
                        fmt_len(s.seconds())
                    )).color(theme::TEXT_DIM));
                }
            });
        });
    }

    fn render_settings_ui(&mut self, ui: &mut egui::Ui) {
        let _busy = self.job.is_some();
        let anim_fps = self.scene.fps;
        let has_ff = ffmpeg_available();
        let has_song = self.scene.audio.is_some();
        let song_muted = self.scene.audio.as_ref().map(|a| a.gain() <= 0.0).unwrap_or(false);
        let (pstart, pend, last) = (self.scene.play_start, self.scene.play_end, self.scene.end_frame().max(1));
        let cur = self.scene.frame.round() as u32;
        self.render_settings.anim_fps = anim_fps;

        // ── Quick setup
        theme::section(ui, "Quick setup");
        let s = &mut self.render_settings;
        let tw = ((ui.available_width() - 6.0) / 2.0).max(120.0);
        for pair in PRESETS.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for p in pair {
                    let usable = p.format == OutFormat::PngSequence || has_ff;
                    let sel = s.width == p.w && s.height == p.h && s.fps == p.fps && s.format == p.format;
                    let r = ui.add_enabled_ui(usable, |ui| tile(ui, sel, p.name, p.sub, tw)).inner;
                    let r = if usable { r } else { r.on_hover_text("Needs ffmpeg for video files (see Output below)") };
                    if usable && r.clicked() {
                        s.width = p.w;
                        s.height = p.h;
                        s.fps = p.fps;
                        set_format(s, p.format);
                    }
                }
            });
            ui.add_space(4.0);
        }

        // ── Picture
        theme::section(ui, "Picture");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
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
                if ui.small_button("⇄").on_hover_text("Swap width and height (landscape ↔ portrait)").clicked() {
                    std::mem::swap(&mut s.width, &mut s.height);
                }
                ui.add(egui::DragValue::new(&mut s.height).range(64..=7680).suffix(" px"));
                ui.label(RichText::new(aspect(s.width, s.height)).size(11.0).color(theme::TEXT_DIM));
            });
            ui.checkbox(&mut s.msaa, "Smooth edges (4× anti-aliasing)");
        });

        // ── Length
        theme::section(ui, "Length");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label("From frame");
                ui.add(egui::DragValue::new(&mut s.start).range(0..=1_000_000));
                ui.label("to");
                let st = s.start;
                ui.add(egui::DragValue::new(&mut s.end).range(st..=1_000_000));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(s.start == pstart && s.end == pend, "Timeline range").on_hover_text("The play range set on the timeline").clicked() {
                    s.start = pstart;
                    s.end = pend;
                }
                if ui.selectable_label(s.start == 0 && s.end == last, "Whole animation").clicked() {
                    s.start = 0;
                    s.end = last;
                }
                if ui.small_button("From playhead").on_hover_text("Start at the current frame").clicked() {
                    s.start = cur.min(s.end);
                }
            });
            ui.label(RichText::new(format!("{} long · animation frames {}–{} at {} fps", fmt_len(s.seconds()), s.start, s.end, anim_fps)).size(11.0).color(theme::TEXT_DIM));
        });

        // ── Video frame rate
        theme::section(ui, "Video frame rate");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for v in [24.0, 30.0, 60.0] {
                    let label = format!("{v} fps");
                    if ui.add(egui::Button::selectable(s.fps == v, RichText::new(label).size(13.0)).min_size(vec2(70.0, 26.0))).clicked() {
                        s.fps = v;
                    }
                }
            });
            let (icon, text) = if (s.fps - anim_fps).abs() < 0.01 {
                ("=", format!("Same as your animation ({anim_fps} fps): one video frame for every animation frame."))
            } else if s.fps > anim_fps {
                ("↑", format!("Smoother than the animation. It's keyed at {anim_fps} fps, and the frames in between (motion, camera, physics) are calculated, so the speed and length stay the same."))
            } else {
                ("↓", format!("Fewer frames than the animation ({anim_fps} fps): some are skipped, but the speed and length stay the same."))
            };
            egui::Frame::new().fill(theme::BG1).corner_radius(egui::CornerRadius::same(5)).inner_margin(egui::Margin::same(7)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(icon).strong().color(theme::ACCENT2));
                    ui.add(egui::Label::new(RichText::new(text).size(11.0)).wrap());
                });
            });
            ui.label(RichText::new(format!("→ {} video frames", s.video_frames())).size(11.0).color(theme::TEXT_DIM));
            ui.label(RichText::new(format!("The animation's own rate ({anim_fps} fps) is set on the timeline; keep it at 30 for VMD dances.")).size(10.5).color(theme::TEXT_DIM));
        });

        // ── Output
        theme::section(ui, "Output");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            for (f, what) in [
                (OutFormat::Mp4, "Plays everywhere; best for uploading and sharing."),
                (OutFormat::WebM, "Open web format; smaller files."),
                (OutFormat::Mov, "Large, top quality; for video editing apps."),
                (OutFormat::PngSequence, "One picture per frame; works without ffmpeg."),
            ] {
                let enabled = f == OutFormat::PngSequence || has_ff;
                ui.add_enabled_ui(enabled, |ui| {
                    ui.horizontal(|ui| {
                        if ui.radio(s.format == f, f.label()).clicked() {
                            set_format(s, f);
                        }
                    });
                    ui.label(RichText::new(format!("      {what}")).size(10.5).color(theme::TEXT_DIM));
                });
            }
            if !has_ff {
                ui.label(RichText::new("Video files need the free program ffmpeg. PNG frames work now.").size(11.0).color(theme::KEY));
            }
            if matches!(s.format, OutFormat::Mp4 | OutFormat::WebM) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Quality");
                    for (n, q) in [("Best", 14), ("High", 18), ("Balanced", 23), ("Small file", 28)] {
                        if ui.selectable_label(s.quality == q, n).clicked() {
                            s.quality = q;
                        }
                    }
                });
                ui.collapsing(RichText::new("Fine quality setting").size(11.0), |ui| {
                    ui.add(egui::Slider::new(&mut s.quality, 12..=32).text("lower = better, bigger file"));
                });
            }
            ui.add_space(4.0);
            ui.add_enabled_ui(has_song && !song_muted && s.format != OutFormat::PngSequence, |ui| {
                ui.checkbox(&mut s.include_audio, "Include the song");
            });
            if !has_song {
                ui.label(RichText::new("No song in this project.").size(10.5).color(theme::TEXT_DIM));
            } else if song_muted {
                ui.label(RichText::new("The song is muted (Scene → Audio), so it's left out.").size(10.5).color(theme::TEXT_DIM));
            }
            ui.add_space(4.0);
            ui.label(RichText::new(if s.format == OutFormat::PngSequence { "Folder" } else { "File" }).size(11.0).color(theme::TEXT_DIM));
            ui.add(egui::Label::new(RichText::new(s.path.display().to_string()).size(11.0).monospace()).truncate()).on_hover_text(s.path.display().to_string());
            ui.horizontal(|ui| {
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
                ui.checkbox(&mut s.open_when_done, "Open folder when done");
            });
        });
        if !has_ff && ui.button("Set up video export…").clicked() {
            self.show_video_help = true;
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Frames are drawn on the GPU as fast as possible (not in real time), with physics stepped once per video frame, so the result matches playback.").size(10.5).color(theme::TEXT_DIM));
    }

    fn render_footer(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let busy = self.job.is_some();
        let has_ff = ffmpeg_available();
        let has_song = self.scene.audio.is_some();
        let song_muted = self.scene.audio.as_ref().map(|a| a.gain() <= 0.0).unwrap_or(false);
        self.render_settings.anim_fps = self.scene.fps;
        // ── Summary and render
        let s = self.render_settings.clone();
        let song = if s.include_audio && has_song && !song_muted && s.format != OutFormat::PngSequence { " · with song" } else { "" };
        egui::Frame::new().fill(theme::BG2).stroke(egui::Stroke::new(1.0, theme::BG3)).corner_radius(egui::CornerRadius::same(7)).inner_margin(egui::Margin::same(9)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("{}×{} · {} fps · {} · {}{song}", s.width, s.height, s.fps, fmt_len(s.seconds()), match s.format {
                OutFormat::Mp4 => "MP4",
                OutFormat::WebM => "WebM",
                OutFormat::Mov => "MOV",
                OutFormat::PngSequence => "PNG frames",
            })).strong());
            ui.label(RichText::new(format!("{} frames to render", s.video_frames())).size(11.0).color(theme::TEXT_DIM));
        });
        ui.add_space(6.0);
        let can_render = !busy && self.renderer.is_some() && (s.format == OutFormat::PngSequence || has_ff);
        ui.add_enabled_ui(can_render, |ui| {
            let b = egui::Button::new(RichText::new("🎬  Render animation").size(15.0).color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(ui.available_width(), 36.0));
            if ui.add(b).clicked() {
                self.start_render_job();
            }
        });
        ui.add_enabled_ui(!busy && self.renderer.is_some(), |ui| {
            if ui.add(egui::Button::new("📷  Save current frame as image…").min_size(vec2(ui.available_width(), 26.0))).clicked() {
                self.render_still_dialog(frame);
            }
        });
    }
}
