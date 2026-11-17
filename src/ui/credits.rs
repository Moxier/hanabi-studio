//! Credits of loaded files. Model, stage, motion and song makers ship their credits and terms
//! of use as readme files inside each download's folder; this window finds them next to every
//! file in the scene, shows them, and builds a credits list to paste under a video.

use crate::app::App;
use crate::ui::theme;
use egui::RichText;
use std::path::{Path, PathBuf};

const WORDS: [&str; 16] = ["readme", "read me", "read_me", "credit", "licen", "terms", "rule", "利用規約", "規約", "説明", "お読み", "読んで", "注意", "使用", "说明", "须知"];
const EXTS: [&str; 9] = ["txt", "md", "html", "htm", "rtf", "pdf", "png", "jpg", "jpeg"];

fn is_credit_file(p: &Path) -> bool {
    let name = crate::findfile::display_name(p.file_name().unwrap_or_default()).to_lowercase();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    EXTS.contains(&ext.as_str()) && (WORDS.iter().any(|w| name.contains(w)) || (ext == "txt" && name.len() < 40))
}

/// Credit files in the file's folder and the folder above it (downloads often keep the readme
/// one level up from the model).
pub fn credit_files(file: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dir = file.parent().map(|d| d.to_path_buf());
    for _ in 0..2 {
        let Some(d) = dir else { break };
        if let Ok(rd) = std::fs::read_dir(&d) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_credit_file(p)).collect();
            v.sort();
            out.extend(v);
        }
        if !out.is_empty() {
            break;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    out
}

/// Read a readme in whatever encoding it was saved with.
pub fn read_text(p: &Path) -> String {
    let Ok(b) = std::fs::read(p) else { return String::new() };
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&b[3..]).into_owned();
    }
    if b.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(&b[2..]).0.into_owned();
    }
    if b.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(&b[2..]).0.into_owned();
    }
    if let Ok(s) = std::str::from_utf8(&b) {
        return s.to_string();
    }
    for enc in [encoding_rs::SHIFT_JIS, encoding_rs::GBK, encoding_rs::BIG5] {
        let (s, bad) = enc.decode_without_bom_handling(&b);
        if !bad {
            return s.into_owned();
        }
    }
    encoding_rs::SHIFT_JIS.decode(&b).0.into_owned()
}

fn open_file(p: &Path) {
    crate::ui::links::open_external(p.as_os_str());
}

struct Entry {
    kind: &'static str,
    name: String,
    path: PathBuf,
    note: String,
}

impl App {
    fn credit_entries(&self) -> Vec<Entry> {
        let mut v = Vec::new();
        for m in &self.scene.models {
            let Some(p) = &m.data.path else { continue };
            let character = !m.data.rigid_bodies.is_empty() || m.data.bone_index("上半身").is_some() || m.data.bone_index("頭").is_some();
            let kind = if m.is_accessory() { "Accessory" } else if character { "Model" } else { "Stage" };
            let note = m.data.comment.lines().map(str::trim).filter(|l| !l.is_empty()).take(6).collect::<Vec<_>>().join("\n");
            v.push(Entry { kind, name: m.orig_name.clone(), path: p.clone(), note });
        }
        for p in &self.scene.sources {
            let kind = match crate::formats::kind_of(p) {
                crate::formats::FileKind::Motion | crate::formats::FileKind::OwnMotion => "Motion",
                crate::formats::FileKind::Pose => "Pose",
                crate::formats::FileKind::Audio => "Song",
                _ => continue,
            };
            let name = crate::findfile::display_name(p.file_name().unwrap_or_default());
            v.push(Entry { kind, name, path: p.clone(), note: String::new() });
        }
        v
    }

    pub fn credits_text(&self) -> String {
        let mut out = String::from("Credits\n");
        for kind in ["Model", "Stage", "Accessory", "Motion", "Pose", "Song"] {
            let items: Vec<Entry> = self.credit_entries().into_iter().filter(|e| e.kind == kind).collect();
            if items.is_empty() {
                continue;
            }
            let plural = match kind {
                "Accessory" => "Accessories".to_string(),
                k => format!("{k}s"),
            };
            out.push_str(&format!("\n{plural}:\n"));
            for e in items {
                let folder = e.path.parent().and_then(|d| d.file_name()).map(|n| crate::findfile::display_name(n)).unwrap_or_default();
                out.push_str(&format!("- {} ({folder})\n", e.name));
            }
        }
        out.push_str("\nMade with Hanabi Studio\n");
        out
    }

    pub fn credits_window(&mut self, ctx: &egui::Context) {
        if !self.show_credits {
            return;
        }
        let mut open = true;
        let entries = self.credit_entries();
        let mut copy = false;
        egui::Window::new("Credits of loaded files").open(&mut open).collapsible(false).default_width(560.0).default_height(520.0).show(ctx, |ui| {
            ui.label("Makers put their credits and terms of use in a readme next to each download. Read them before you publish a video; many ask to be credited or set rules for use.");
            ui.horizontal(|ui| {
                if ui.button("Copy credits list").on_hover_text("A list of every model, stage, motion and song in this scene, to paste under your video").clicked() {
                    copy = true;
                }
            });
            ui.separator();
            if entries.is_empty() {
                ui.label(RichText::new("Nothing loaded from files yet.").color(theme::TEXT_DIM));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, e) in entries.iter().enumerate() {
                    theme::card(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(e.kind).size(11.0).color(theme::ACCENT));
                            ui.label(RichText::new(&e.name).strong());
                        });
                        ui.label(RichText::new(e.path.parent().map(|d| d.display().to_string()).unwrap_or_default()).size(10.5).color(theme::TEXT_DIM));
                        if !e.note.is_empty() {
                            ui.label(RichText::new(&e.note).size(11.5));
                        }
                        let files = credit_files(&e.path);
                        if files.is_empty() {
                            ui.label(RichText::new("No readme found in this folder.").size(11.0).color(theme::TEXT_DIM));
                        }
                        for f in files {
                            let shown = crate::findfile::display_name(f.file_name().unwrap_or_default());
                            let ext = f.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("📄 {shown}")).color(theme::KEY));
                                if ui.small_button("Open").clicked() {
                                    open_file(&f);
                                }
                            });
                            if ext == "txt" || ext == "md" {
                                egui::CollapsingHeader::new("Read here").id_salt(("credit", i, shown.clone())).show(ui, |ui| {
                                    let mut t = read_text(&f);
                                    if t.len() > 20_000 {
                                        t.truncate(t.char_indices().nth(20_000).map(|(i, _)| i).unwrap_or(t.len()));
                                    }
                                    egui::ScrollArea::vertical().max_height(220.0).id_salt(("credit_scroll", i, shown)).show(ui, |ui| {
                                        ui.add(egui::Label::new(RichText::new(t).size(11.5)).wrap());
                                    });
                                });
                            }
                        }
                    });
                }
            });
        });
        if copy {
            ctx.copy_text(self.credits_text());
            self.set_status("Credits list copied");
        }
        self.show_credits = open;
    }
}
