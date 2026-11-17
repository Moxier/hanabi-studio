//! Hanabi Studio — an original, MIT-licensed 3D dance animation studio with an MMD-style workflow.
//!
//! Usage:
//!   hanabi-studio [files...]               open models, motions, poses, songs or a project
//!   hanabi-studio --mcp-stdio              let an AI client drive the studio over stdio (MCP)
//!   hanabi-studio --mcp-port 7860          change the local MCP HTTP port (0 turns it off)
//!   hanabi-studio --screenshot out.png     draw ~120 frames, save the window and exit
//!   hanabi-studio --workspace effects      start in a workspace (animation/effects/color/render)
//!   hanabi-studio --preset "Water Stage"   start with an effect preset
//!   hanabi-studio --empty                  start without the demo mannequin

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod audio;
mod commands;
mod commands2;
mod demo;
mod effects;
mod export;
mod findfile;
mod formats;
mod grade;
mod lut;
mod material;
mod mcp;
mod model;
mod motion;
mod motionbin;
mod physics;
mod phystune;
mod project;
mod render;
mod scene;
mod selftest;
mod skeleton;
mod tools;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut args = app::Args { screenshot: None, mcp_stdio: false, mcp_port: 7860, workspace: None, files: Vec::new(), preset: None, no_demo: false, sample: false, home: false, frame: None, show: None, quit_after: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => args.screenshot = it.next().map(PathBuf::from),
            "--mcp-stdio" => args.mcp_stdio = true,
            "--mcp-port" => args.mcp_port = it.next().and_then(|s| s.parse().ok()).unwrap_or(7860),
            "--workspace" => args.workspace = it.next(),
            "--preset" => args.preset = it.next(),
            "--empty" => args.no_demo = true,
            "--sample" => args.sample = true,
            "--home" => args.home = true,
            "--show" => args.show = it.next(),
            "--quit-after" => args.quit_after = it.next().and_then(|s| s.parse().ok()),
            "--frame" => args.frame = it.next().and_then(|s| s.parse().ok()),
            "--resave" => {
                // Re-save a project in the current (compact) format: --resave old.hproj new.hproj
                let (Some(src), Some(dst)) = (it.next(), it.next()) else {
                    eprintln!("usage: --resave old.hproj new.hproj");
                    std::process::exit(2);
                };
                let t0 = std::time::Instant::now();
                let text = std::fs::read_to_string(&src).unwrap_or_default();
                let p = match project::load(&text) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                let t1 = t0.elapsed();
                let out = serde_json::to_string(&p).unwrap_or_default();
                let _ = project::write_safely(std::path::Path::new(&dst), &out, false);
                let t2 = t0.elapsed() - t1;
                let t3 = std::time::Instant::now();
                let back = project::load(&std::fs::read_to_string(&dst).unwrap_or_default()).map(|q| q.models.iter().map(|m| m.motion.key_count()).sum::<usize>() + q.camera_motion.key_count());
                let keys: usize = p.models.iter().map(|m| m.motion.key_count()).sum::<usize>() + p.camera_motion.key_count();
                println!(
                    "old: {} MB, opened in {:.2} s | new: {} MB, saved in {:.2} s, opened in {:.2} s | keys {} -> {:?}",
                    text.len() / 1_000_000,
                    t1.as_secs_f32(),
                    out.len() / 1_000_000,
                    t2.as_secs_f32(),
                    t3.elapsed().as_secs_f32(),
                    keys,
                    back
                );
                std::process::exit(0);
            }
            "--selftest" | "--selftest-deep" => {
                let deep = a == "--selftest-deep";
                let dir = it.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                let n = selftest::run(&dir, deep);
                std::process::exit(if n == 0 { 0 } else { 1 });
            }
            "--help" | "-h" => {
                println!("Hanabi Studio {} — see the top of main.rs or Help → Shortcuts", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => args.files.push(PathBuf::from(other)),
        }
    }
    // Screenshot mode must draw even when no one is looking. Wayland compositors may stop sending
    // frames to a window that is hidden, so use X11 (XWayland) for it when available.
    if (args.screenshot.is_some() || args.quit_after.is_some()) && std::env::var_os("DISPLAY").is_some() {
        std::env::remove_var("WAYLAND_DISPLAY");
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Hanabi Studio")
        .with_inner_size([1600.0, 940.0])
        .with_min_inner_size([1000.0, 640.0])
        .with_drag_and_drop(true);
    if args.screenshot.is_some() || args.quit_after.is_some() {
        // Never take focus from what the person is doing; on Windows also stay behind other
        // windows (a covered window still draws there).
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        depth_buffer: 24,
        multisampling: 0,
        ..Default::default()
    };
    eframe::run_native("Hanabi Studio", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)))))
}
