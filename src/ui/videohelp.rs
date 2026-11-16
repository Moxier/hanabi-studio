//! "Set up video export": step-by-step help for installing ffmpeg, which the studio runs to make
//! MP4 / WebM / MOV files. ffmpeg is separate software with its own licence; it is not shipped
//! with the studio, so the person installs it once and the studio finds it.

use crate::app::App;
use crate::export::{ffmpeg_path, recheck_ffmpeg, set_ffmpeg_path};
use crate::ui::theme;
use egui::{Color32, RichText};

fn open_url(url: &str) {
    crate::ui::links::open_url(url);
}

fn command_row(ui: &mut egui::Ui, label: &str, cmd: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_DIM));
        ui.label(RichText::new(cmd).monospace().color(theme::KEY));
        if ui.small_button("Copy").on_hover_text("Copy this command").clicked() {
            ui.ctx().copy_text(cmd.to_string());
        }
    });
}

fn step(ui: &mut egui::Ui, n: usize, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{n}")).strong().color(theme::ACCENT));
        ui.label(text);
    });
}

impl App {
    pub fn video_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_video_help {
            return;
        }
        let mut open = true;
        egui::Window::new("Set up video export").open(&mut open).collapsible(false).resizable(false).default_width(520.0).show(ctx, |ui| {
            match ffmpeg_path() {
                Some(p) => {
                    ui.label(RichText::new("● Video export is ready").color(Color32::from_rgb(110, 220, 140)).strong());
                    let where_ = if p.parent().map(|d| d.as_os_str().is_empty()).unwrap_or(true) { "Using the ffmpeg installed on this computer".to_string() } else { format!("Using ffmpeg at {}", p.display()) };
                    ui.label(RichText::new(where_).size(11.0).color(theme::TEXT_DIM));
                }
                None => {
                    ui.label(RichText::new("● ffmpeg isn't installed yet").color(Color32::from_rgb(255, 170, 110)).strong());
                    ui.label("Hanabi Studio renders every frame itself and hands them to ffmpeg, a free program that writes MP4, WebM and MOV files. Install it once and video export turns on. PNG image sequences work without it.");
                }
            }
            ui.add_space(6.0);
            let windows_first = cfg!(windows);
            let windows = |ui: &mut egui::Ui| {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Windows 10 / 11").strong());
                    step(ui, 1, "Open the Start menu, type “Terminal” (or “PowerShell”) and open it.");
                    step(ui, 2, "Paste this command and press Enter. It uses winget, which comes with Windows:");
                    command_row(ui, "   ", "winget install --id Gyan.FFmpeg -e");
                    step(ui, 3, "When it says it's done, close the terminal and click “Check again” below.");
                    ui.add_space(4.0);
                    ui.label(RichText::new("Without winget: download a “release essentials” build for Windows from the ffmpeg website, unzip it (for example to C:\\ffmpeg), then click “Choose ffmpeg.exe…” and pick bin\\ffmpeg.exe. You can also copy ffmpeg.exe into the same folder as Hanabi Studio.").size(11.5).color(theme::TEXT_DIM));
                });
            };
            let linux = |ui: &mut egui::Ui| {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("Linux").strong());
                    step(ui, 1, "Open a terminal and run the command for your system:");
                    command_row(ui, "Ubuntu, Mint, Debian, Pika:", "sudo apt install ffmpeg");
                    command_row(ui, "Fedora:", "sudo dnf install ffmpeg");
                    command_row(ui, "Arch, Manjaro:", "sudo pacman -S ffmpeg");
                    command_row(ui, "openSUSE:", "sudo zypper install ffmpeg");
                    step(ui, 2, "Click “Check again” below.");
                    ui.label(RichText::new("Fedora and openSUSE may need their extra multimedia repositories (RPM Fusion / Packman) for H.264 MP4.").size(11.5).color(theme::TEXT_DIM));
                });
            };
            if windows_first {
                windows(ui);
                ui.collapsing("Linux", |ui| linux(ui));
            } else {
                linux(ui);
                ui.collapsing("Windows 10 / 11", |ui| windows(ui));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Check again").clicked() {
                    if recheck_ffmpeg() {
                        self.set_status("ffmpeg found: video export is ready");
                    } else {
                        self.set_error("ffmpeg still wasn't found. If you just installed it, restart Hanabi Studio or pick ffmpeg with “Choose…”.");
                    }
                }
                let pick_label = if cfg!(windows) { "Choose ffmpeg.exe…" } else { "Choose ffmpeg…" };
                if ui.button(pick_label).clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_title("Where is ffmpeg?").pick_file() {
                        if set_ffmpeg_path(&p) {
                            self.set_status(format!("Using ffmpeg at {}", p.display()));
                        } else {
                            self.set_error(format!("{} didn't run as ffmpeg", p.display()));
                        }
                    }
                }
                if ui.button("Open the ffmpeg download page").clicked() {
                    open_url("https://ffmpeg.org/download.html");
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("ffmpeg is separate software under its own licence (LGPL/GPL). Hanabi Studio only starts it; it isn't included with the studio.").size(11.0).color(theme::TEXT_DIM));
        });
        self.show_video_help = open;
    }
}
