//! Welcome screen: start actions and support links on the left, the project library on the
//! right. The library is a grid that wraps to the window width and scrolls down (or a compact
//! list), with search, sorting and pinned projects, so every project stays reachable.

use crate::app::App;
use crate::ui::home::{fmt_date, save_recent, thumb_path_for, ConfirmAction, RecentProject};
use crate::ui::{links, theme};
use egui::{vec2, Color32, CornerRadius, RichText, Sense, Stroke};

#[derive(Default)]
pub struct HomeUi {
    pub search: String,
    pub sort_by_name: bool,
    pub list_view: bool,
}

const KOFI_COLOR: Color32 = Color32::from_rgb(255, 94, 91);

fn ago(secs: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let d = now.saturating_sub(secs);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86399 => format!("{} h ago", d / 3600),
        86400..=172799 => "yesterday".into(),
        172800..=604799 => format!("{} days ago", d / 86400),
        _ => fmt_date(secs).split(' ').next().unwrap_or_default().to_string(),
    }
}

enum Act {
    Open(std::path::PathBuf),
    Forget(std::path::PathBuf),
    Pin(std::path::PathBuf),
    Folder(std::path::PathBuf),
}

/// A big clickable start card.
fn start_card(ui: &mut egui::Ui, icon: &str, text: &str, sub: &str) -> bool {
    let resp = egui::Frame::new()
        .fill(theme::BG2)
        .stroke(Stroke::new(1.0, theme::BG3))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).size(18.0).color(theme::ACCENT));
                ui.vertical(|ui| {
                    ui.label(RichText::new(text).size(14.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new(sub).size(11.0).color(theme::TEXT_DIM));
                });
            });
        })
        .response
        .interact(Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().rect_stroke(resp.rect, 8.0, Stroke::new(1.0, theme::ACCENT.linear_multiply(0.7)), egui::StrokeKind::Inside);
    }
    ui.add_space(6.0);
    resp.clicked()
}

fn link_button(ui: &mut egui::Ui, text: &str, url: Option<String>, tip: &str) {
    let enabled = url.is_some();
    let label = if enabled { text.to_string() } else { format!("{text} · coming soon") };
    let b = egui::Button::new(RichText::new(label).size(12.5)).min_size(vec2(ui.available_width(), 28.0));
    let r = ui.add_enabled(enabled, b).on_hover_text(tip).on_disabled_hover_text("The address will be added when the project is published");
    if r.clicked() {
        if let Some(u) = url {
            links::open_url(&u);
        }
    }
}

impl App {
    pub fn welcome_ui(&mut self, ui: &mut egui::Ui) {
        // ── Left: start and links
        egui::Panel::left("home_side")
            .resizable(false)
            .exact_size(320.0)
            .frame(egui::Frame::new().fill(theme::BG1).inner_margin(egui::Margin::symmetric(22, 24)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                    let p = ui.painter();
                    for k in 0..12 {
                        let a = k as f32 / 12.0 * std::f32::consts::TAU;
                        let d = vec2(a.cos(), a.sin());
                        p.line_segment([r.center() + d * 7.0, r.center() + d * 21.0], Stroke::new(3.0, if k % 2 == 0 { theme::ACCENT } else { theme::KEY }));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Hanabi Studio").size(24.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(11.0).color(theme::TEXT_DIM));
                    });
                });
                ui.label(RichText::new("3D dance animation, effects and rendering").size(12.0).color(theme::TEXT_DIM));
                ui.add_space(18.0);
                theme::section(ui, "Start");
                if start_card(ui, "＋", "New project", "An empty stage for your model, motion and song") {
                    self.new_project();
                }
                if start_card(ui, "📂", "Open project…", "Continue a saved .hproj project") {
                    self.run_confirmed(ConfirmAction::OpenProjectDialog);
                }
                if start_card(ui, "◆", "Start from a model…", "New project, then pick a .pmx / .pmd model") {
                    self.new_project();
                    self.load_dialog("model");
                }
                if start_card(ui, "✨", "Sample project", "The demo mannequin with a short dance") {
                    self.open_sample();
                }
                ui.label(RichText::new("Tip: drop model, motion, pose and song files onto the window to open them.").size(10.5).color(theme::TEXT_DIM));

                // Links at the bottom of the panel
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(RichText::new("Free and open source · MIT licence").size(10.5).color(theme::TEXT_DIM));
                    ui.add_space(8.0);
                    if ui.add(egui::Button::new(RichText::new("Help & shortcuts").size(12.5)).min_size(vec2(ui.available_width(), 28.0))).clicked() {
                        self.show_help = true;
                    }
                    link_button(ui, "Report a problem", links::github_issues(), "Tell us about a bug or ask for a feature");
                    link_button(ui, "GitHub", (!links::GITHUB.is_empty()).then(|| links::GITHUB.to_string()), "Source code, releases and updates");
                    let kofi = egui::Button::new(RichText::new("☕  Support on Ko-fi").size(13.5).strong().color(Color32::WHITE)).fill(KOFI_COLOR).min_size(vec2(ui.available_width(), 34.0));
                    if ui.add(kofi).on_hover_text("Hanabi Studio is free. If it helps you, you can buy the developer a coffee.").clicked() {
                        links::open_url(links::KOFI);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("SUPPORT & LINKS").size(10.5).color(theme::TEXT_DIM).strong());
                });
            });

        // ── Right: the project library
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0).inner_margin(egui::Margin::symmetric(28, 24))).show(ui, |ui| {
            let mut acts: Vec<Act> = Vec::new();
            let missing = self.recent.iter().filter(|r| !r.path.exists()).count();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Projects").size(20.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(format!("{}", self.recent.len())).size(12.0).color(theme::TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let h = &mut self.home_ui;
                    if ui.selectable_label(h.list_view, "☰ List").on_hover_text("Compact list").clicked() {
                        h.list_view = true;
                    }
                    if ui.selectable_label(!h.list_view, "▦ Grid").on_hover_text("Pictures").clicked() {
                        h.list_view = false;
                    }
                    ui.add_space(8.0);
                    egui::ComboBox::from_id_salt("home_sort").width(120.0).selected_text(if h.sort_by_name { "Sort: name" } else { "Sort: last opened" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut h.sort_by_name, false, "Last opened");
                        ui.selectable_value(&mut h.sort_by_name, true, "Name");
                    });
                    ui.add(egui::TextEdit::singleline(&mut h.search).hint_text("🔍 Search projects").desired_width(200.0));
                });
            });
            if missing > 0 {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{missing} project file(s) can't be found (moved or deleted).")).size(11.0).color(theme::TEXT_DIM));
                    if ui.small_button("Remove them from the list").clicked() {
                        self.recent.retain(|r| r.path.exists());
                        save_recent(&self.recent);
                    }
                });
            }
            ui.add_space(10.0);

            if self.recent.is_empty() {
                egui::Frame::new().fill(theme::BG1).stroke(Stroke::new(1.0, theme::BG3)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(28)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No projects yet").size(16.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new("Projects you save or open appear here with a picture of the scene.").color(theme::TEXT_DIM));
                        ui.add_space(10.0);
                        if ui.add(egui::Button::new(RichText::new("＋ New project").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(160.0, 30.0))).clicked() {
                            self.new_project();
                        }
                        if ui.button("Try the sample project").clicked() {
                            self.open_sample();
                        }
                    });
                });
                return;
            }

            // Filter and sort
            let q = self.home_ui.search.trim().to_lowercase();
            let mut items: Vec<RecentProject> = self.recent.iter().filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.path.display().to_string().to_lowercase().contains(&q)).cloned().collect();
            if self.home_ui.sort_by_name {
                items.sort_by_key(|r| r.name.to_lowercase());
            } else {
                items.sort_by(|a, b| b.opened_secs.cmp(&a.opened_secs));
            }
            let pinned: Vec<RecentProject> = items.iter().filter(|r| r.pinned).cloned().collect();
            let rest: Vec<RecentProject> = items.into_iter().filter(|r| !r.pinned).collect();
            let list_view = self.home_ui.list_view;

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if pinned.is_empty() && rest.is_empty() {
                    ui.label(RichText::new("No project matches your search.").color(theme::TEXT_DIM));
                }
                for (title, list) in [("Pinned", &pinned), ("Recent", &rest)] {
                    if list.is_empty() {
                        continue;
                    }
                    if !pinned.is_empty() {
                        theme::section(ui, title);
                    }
                    if list_view {
                        for r in list.iter() {
                            self.recent_row(ui, r, &mut acts);
                        }
                    } else {
                        // As many columns as fit; cards stretch to fill the row (scrollbar room kept).
                        let gap = 12.0;
                        let avail = ui.available_width() - 14.0;
                        let cols = (((avail + gap) / (210.0 + gap)).floor() as usize).max(1);
                        let card_w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).clamp(180.0, 320.0);
                        for row in list.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for r in row {
                                    self.recent_card(ui, r, card_w, &mut acts);
                                }
                            });
                            ui.add_space(gap);
                        }
                    }
                    ui.add_space(8.0);
                }
            });

            for a in acts {
                match a {
                    Act::Open(p) => self.run_confirmed(ConfirmAction::OpenProject(p)),
                    Act::Forget(p) => {
                        self.recent.retain(|r| r.path != p);
                        save_recent(&self.recent);
                    }
                    Act::Pin(p) => {
                        if let Some(r) = self.recent.iter_mut().find(|r| r.path == p) {
                            r.pinned = !r.pinned;
                        }
                        save_recent(&self.recent);
                    }
                    Act::Folder(p) => {
                        if let Some(d) = p.parent() {
                            links::open_external(d.as_os_str());
                        }
                    }
                }
            }
        });
    }

    fn recent_menu(ui: &mut egui::Ui, r: &RecentProject, exists: bool, acts: &mut Vec<Act>) {
        if ui.add_enabled(exists, egui::Button::new("Open")).clicked() {
            acts.push(Act::Open(r.path.clone()));
            ui.close();
        }
        if ui.add_enabled(exists, egui::Button::new("Show in folder")).clicked() {
            acts.push(Act::Folder(r.path.clone()));
            ui.close();
        }
        if ui.button(if r.pinned { "Unpin" } else { "Pin to the top" }).clicked() {
            acts.push(Act::Pin(r.path.clone()));
            ui.close();
        }
        ui.separator();
        if ui.button("Remove from this list").on_hover_text("The project file stays on disk").clicked() {
            acts.push(Act::Forget(r.path.clone()));
            ui.close();
        }
    }

    fn recent_thumb(&mut self, ui: &egui::Ui, r: &RecentProject) -> Option<egui::TextureHandle> {
        let thumb = r.thumb.clone().or_else(|| Some(thumb_path_for(&r.path)).filter(|t| t.exists()));
        thumb.and_then(|t| self.thumb_texture(ui.ctx(), &t))
    }

    fn recent_card(&mut self, ui: &mut egui::Ui, r: &RecentProject, w: f32, acts: &mut Vec<Act>) {
        let exists = r.path.exists();
        let tex = self.recent_thumb(ui, r);
        let card_id = ui.make_persistent_id(("recent_card", r.path.display().to_string()));
        let behind = theme::card_behind(ui, card_id);
        let hovered = behind.as_ref().map(|b| b.hovered()).unwrap_or(false);
        let inner_w = w - 16.0;
        let resp = egui::Frame::new()
            .fill(if hovered { theme::BG3 } else { theme::BG2 })
            .stroke(Stroke::new(1.0, if hovered { theme::ACCENT.linear_multiply(0.7) } else { theme::BG3 }))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(inner_w);
                ui.vertical(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(inner_w, inner_w * 9.0 / 16.0), Sense::hover());
                    match &tex {
                        Some(t) => {
                            ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), if exists { Color32::WHITE } else { Color32::from_gray(90) });
                        }
                        None => {
                            ui.painter().rect_filled(rect, 5.0, theme::BG0);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "no picture yet", egui::FontId::proportional(11.0), theme::TEXT_DIM);
                        }
                    }
                    if !exists {
                        ui.painter().text(rect.center() + vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, "file missing", egui::FontId::proportional(12.0), Color32::from_rgb(255, 140, 120));
                    }
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button("⋯", |ui| Self::recent_menu(ui, r, exists, acts));
                            let star = if r.pinned { RichText::new("★").color(theme::KEY) } else { RichText::new("☆").color(theme::TEXT_DIM) };
                            if ui.add(egui::Button::new(star).frame(false)).on_hover_text(if r.pinned { "Unpin" } else { "Pin to the top" }).clicked() {
                                acts.push(Act::Pin(r.path.clone()));
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(RichText::new(&r.name).strong().color(if exists { Color32::WHITE } else { theme::TEXT_DIM })).truncate());
                            });
                        });
                    });
                    ui.label(RichText::new(ago(r.opened_secs)).size(10.5).color(theme::TEXT_DIM));
                });
            })
            .response;
        theme::remember_card(ui, card_id, resp.rect);
        if let Some(b) = behind {
            let b = b.on_hover_text(r.path.display().to_string());
            if b.hovered() && exists {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if b.clicked() && exists {
                acts.push(Act::Open(r.path.clone()));
            }
            b.context_menu(|ui| Self::recent_menu(ui, r, exists, acts));
        }
    }

    fn recent_row(&mut self, ui: &mut egui::Ui, r: &RecentProject, acts: &mut Vec<Act>) {
        let exists = r.path.exists();
        let tex = self.recent_thumb(ui, r);
        let card_id = ui.make_persistent_id(("recent_row", r.path.display().to_string()));
        let behind = theme::card_behind(ui, card_id);
        let hovered = behind.as_ref().map(|b| b.hovered()).unwrap_or(false);
        let resp = egui::Frame::new()
            .fill(if hovered { theme::BG3 } else { theme::BG2 })
            .stroke(Stroke::new(1.0, theme::BG3))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(64.0, 36.0), Sense::hover());
                    match &tex {
                        Some(t) => {
                            ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            ui.painter().rect_filled(rect, 4.0, theme::BG0);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("⋯", |ui| Self::recent_menu(ui, r, exists, acts));
                        let star = if r.pinned { RichText::new("★").color(theme::KEY) } else { RichText::new("☆").color(theme::TEXT_DIM) };
                        if ui.add(egui::Button::new(star).frame(false)).on_hover_text(if r.pinned { "Unpin" } else { "Pin to the top" }).clicked() {
                            acts.push(Act::Pin(r.path.clone()));
                        }
                        ui.label(RichText::new(if exists { ago(r.opened_secs) } else { "file missing".into() }).size(11.0).color(theme::TEXT_DIM));
                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.add(egui::Label::new(RichText::new(&r.name).strong().color(if exists { Color32::WHITE } else { theme::TEXT_DIM })).truncate());
                            ui.add(egui::Label::new(RichText::new(r.path.display().to_string()).size(10.5).color(theme::TEXT_DIM)).truncate());
                        });
                    });
                });
            })
            .response;
        theme::remember_card(ui, card_id, resp.rect);
        if let Some(b) = behind {
            if b.clicked() && exists {
                acts.push(Act::Open(r.path.clone()));
            }
            b.context_menu(|ui| Self::recent_menu(ui, r, exists, acts));
        }
        ui.add_space(4.0);
    }
}
