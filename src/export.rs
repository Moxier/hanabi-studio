//! Offline rendering: PNG sequences (encoded in parallel) or video through ffmpeg.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutFormat {
    Mp4,
    WebM,
    Mov,
    PngSequence,
}

impl OutFormat {
    pub const ALL: [OutFormat; 4] = [OutFormat::Mp4, OutFormat::WebM, OutFormat::Mov, OutFormat::PngSequence];
    pub fn label(self) -> &'static str {
        match self {
            OutFormat::Mp4 => "MP4 video (H.264)",
            OutFormat::WebM => "WebM video (VP9)",
            OutFormat::Mov => "MOV (ProRes, for editing apps)",
            OutFormat::PngSequence => "PNG image sequence",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            OutFormat::Mp4 => "mp4",
            OutFormat::WebM => "webm",
            OutFormat::Mov => "mov",
            OutFormat::PngSequence => "",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    /// Video frame rate. Separate from the animation's own rate (`anim_fps`): a 30 fps
    /// animation rendered at 60 fps gets in-between frames, with the same speed and length.
    pub fps: f32,
    /// First and last frame to render, in animation (timeline) frames.
    pub start: u32,
    pub end: u32,
    /// The animation's frame rate, filled in from the project when the render starts.
    pub anim_fps: f32,
    /// Open the folder with the result when the render finishes.
    pub open_when_done: bool,
    pub format: OutFormat,
    pub quality: u32,
    pub include_audio: bool,
    pub msaa: bool,
    pub path: PathBuf,
}

impl RenderSettings {
    /// Length of the range in seconds (end frame included).
    pub fn seconds(&self) -> f32 {
        (self.end.saturating_sub(self.start) + 1) as f32 / self.anim_fps.max(1.0)
    }
    pub fn video_frames(&self) -> u32 {
        (self.end.saturating_sub(self.start) as f32 / self.anim_fps.max(1.0) * self.fps.max(1.0)).round() as u32 + 1
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60.0,
            start: 0,
            end: 240,
            anim_fps: 30.0,
            open_when_done: true,
            format: OutFormat::Mp4,
            quality: 18,
            include_audio: true,
            msaa: true,
            path: dirs_home().join("hanabi_render.mp4"),
        }
    }
}

pub fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Where ffmpeg was found (looked up once and on "Check again"; starting a process every frame
/// made the UI lag). ffmpeg is a separate program the person installs; it is never bundled.
static FFMPEG: std::sync::Mutex<Option<Option<PathBuf>>> = std::sync::Mutex::new(None);

fn ffmpeg_setting_file() -> PathBuf {
    crate::ui::home::config_dir().join("ffmpeg-path.txt")
}

fn runs(p: &std::path::Path) -> bool {
    let mut c = Command::new(p);
    c.arg("-version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c.status().map(|s| s.success()).unwrap_or(false)
}

fn find_ffmpeg() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(s) = std::fs::read_to_string(ffmpeg_setting_file()) {
        let s = s.trim();
        if !s.is_empty() {
            cands.push(PathBuf::from(s));
        }
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        cands.push(dir.join(exe));
        cands.push(dir.join("ffmpeg").join("bin").join(exe));
        cands.push(dir.join("ffmpeg").join(exe));
    }
    cands.push(PathBuf::from(exe));
    if cfg!(windows) {
        for (var, rel) in [
            ("LOCALAPPDATA", "Microsoft\\WinGet\\Links\\ffmpeg.exe"),
            ("USERPROFILE", "scoop\\shims\\ffmpeg.exe"),
            ("ProgramData", "chocolatey\\bin\\ffmpeg.exe"),
            ("ProgramFiles", "ffmpeg\\bin\\ffmpeg.exe"),
        ] {
            if let Some(v) = std::env::var_os(var) {
                cands.push(PathBuf::from(v).join(rel));
            }
        }
        cands.push(PathBuf::from("C:\\ffmpeg\\bin\\ffmpeg.exe"));
    } else {
        for p in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/snap/bin/ffmpeg", "/opt/homebrew/bin/ffmpeg"] {
            cands.push(PathBuf::from(p));
        }
    }
    cands.into_iter().find(|p| runs(p))
}

pub fn ffmpeg_path() -> Option<PathBuf> {
    let mut g = FFMPEG.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(find_ffmpeg());
    }
    g.clone().flatten()
}

pub fn ffmpeg_available() -> bool {
    ffmpeg_path().is_some()
}

/// Look for ffmpeg again (after the person installed it).
pub fn recheck_ffmpeg() -> bool {
    *FFMPEG.lock().unwrap_or_else(|e| e.into_inner()) = None;
    ffmpeg_available()
}

/// Remember an ffmpeg program the person picked. Returns false if it doesn't run.
pub fn set_ffmpeg_path(p: &std::path::Path) -> bool {
    if !runs(p) {
        return false;
    }
    let f = ffmpeg_setting_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, p.display().to_string());
    recheck_ffmpeg()
}

enum Sink {
    Video { tx: Option<SyncSender<Vec<u8>>>, writer: Option<std::thread::JoinHandle<Result<(), String>>>, child: Child },
    Png { dir: PathBuf, pending: Vec<std::thread::JoinHandle<()>> },
}

pub struct RenderJob {
    pub settings: RenderSettings,
    pub next: u32,
    pub started: Instant,
    sink: Sink,
    pub error: Option<String>,
    pub done: bool,
    pub last_frame_ms: f32,
}

impl RenderJob {
    pub fn start(s: RenderSettings, audio: Option<(PathBuf, f32, f32)>) -> Result<Self, String> {
        let sink = match s.format {
            OutFormat::PngSequence => {
                std::fs::create_dir_all(&s.path).map_err(|e| format!("{}: {e}", s.path.display()))?;
                Sink::Png { dir: s.path.clone(), pending: Vec::new() }
            }
            f => {
                let Some(ff) = ffmpeg_path() else {
                    return Err("Video export needs ffmpeg. Open Help → Set up video export, or choose “PNG image sequence”.".into());
                };
                let mut cmd = Command::new(ff);
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x0800_0000);
                }
                cmd.args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", s.width, s.height), "-r", &format!("{}", s.fps), "-i", "-"]);
                if let (true, Some((a, offset, gain))) = (s.include_audio, &audio) {
                    let start_sec = s.start as f32 / s.anim_fps.max(1.0) + offset;
                    cmd.args(["-ss", &format!("{:.3}", start_sec.max(0.0)), "-i"]).arg(a);
                    cmd.args(["-map", "0:v", "-map", "1:a", "-shortest"]);
                    // A song that starts after the first rendered frame is delayed; volume as set.
                    let mut filters = Vec::new();
                    if start_sec < 0.0 {
                        filters.push(format!("adelay={}:all=1", (-start_sec * 1000.0).round() as i64));
                    }
                    if (gain - 1.0).abs() > 1e-3 {
                        filters.push(format!("volume={gain:.3}"));
                    }
                    if !filters.is_empty() {
                        cmd.args(["-af", &filters.join(",")]);
                    }
                }
                match f {
                    OutFormat::Mp4 => {
                        cmd.args(["-c:v", "libx264", "-preset", "fast", "-crf", &s.quality.to_string(), "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]);
                    }
                    OutFormat::WebM => {
                        cmd.args(["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", &(s.quality + 12).to_string(), "-row-mt", "1", "-pix_fmt", "yuv420p", "-c:a", "libopus"]);
                    }
                    _ => {
                        cmd.args(["-c:v", "prores_ks", "-profile:v", "3", "-c:a", "pcm_s16le"]);
                    }
                }
                cmd.arg(&s.path);
                let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("could not start ffmpeg: {e}"))?;
                let mut stdin = child.stdin.take().unwrap();
                let (tx, rx): (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) = sync_channel(6);
                let writer = std::thread::spawn(move || {
                    for f in rx {
                        stdin.write_all(&f).map_err(|e| format!("ffmpeg stopped: {e}"))?;
                    }
                    Ok(())
                });
                Sink::Video { tx: Some(tx), writer: Some(writer), child }
            }
        };
        Ok(Self { next: 0, settings: s, started: Instant::now(), sink, error: None, done: false, last_frame_ms: 0.0 })
    }

    /// Number of video frames: the range's length in seconds times the video frame rate.
    pub fn total(&self) -> u32 {
        self.settings.video_frames()
    }

    pub fn progress(&self) -> f32 {
        self.next as f32 / self.total().max(1) as f32
    }

    /// The timeline position (in animation frames, may be fractional) of video frame `k`.
    pub fn timeline_frame(&self, k: u32) -> f32 {
        self.settings.start as f32 + k as f32 * self.settings.anim_fps.max(1.0) / self.settings.fps.max(1.0)
    }

    pub fn push_frame(&mut self, frame: u32, w: u32, h: u32, rgba: Vec<u8>) {
        match &mut self.sink {
            Sink::Video { tx, .. } => {
                if let Some(tx) = tx {
                    if tx.send(rgba).is_err() {
                        self.error = Some("ffmpeg stopped unexpectedly".into());
                    }
                }
            }
            Sink::Png { dir, pending } => {
                let path = dir.join(format!("frame_{frame:05}.png"));
                pending.retain(|h| !h.is_finished());
                // Bound memory: wait if too many frames are still encoding.
                while pending.len() > 16 {
                    if let Some(h) = pending.pop() {
                        let _ = h.join();
                    }
                }
                pending.push(std::thread::spawn(move || {
                    let _ = image::save_buffer(&path, &rgba, w, h, image::ColorType::Rgba8);
                }));
            }
        }
    }

    pub fn finish(&mut self) -> Result<String, String> {
        self.done = true;
        match &mut self.sink {
            Sink::Video { tx, writer, child } => {
                drop(tx.take());
                let write_err = match writer.take().map(|w| w.join()) {
                    Some(Ok(Err(e))) => Some(e),
                    _ => None,
                };
                // Always wait for ffmpeg and read what it said: its message explains a failure
                // far better than "broken pipe".
                let out = child.wait().map_err(|e| e.to_string())?;
                let mut err = String::new();
                if let Some(mut st) = child.stderr.take() {
                    use std::io::Read;
                    let _ = st.read_to_string(&mut err);
                }
                let said: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                let said = said.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join(" · ");
                if !out.success() || write_err.is_some() {
                    let _ = std::fs::remove_file(&self.settings.path);
                    return Err(if said.is_empty() {
                        format!("Video export failed: {}", write_err.unwrap_or_else(|| format!("ffmpeg exited with {out}")))
                    } else {
                        format!("Video export failed (ffmpeg: {said})")
                    });
                }
                let size = std::fs::metadata(&self.settings.path).map(|m| m.len()).unwrap_or(0);
                if size == 0 {
                    let _ = std::fs::remove_file(&self.settings.path);
                    return Err(format!("Video export failed: ffmpeg wrote an empty file{}", if said.is_empty() { String::new() } else { format!(" ({said})") }));
                }
            }
            Sink::Png { pending, dir } => {
                for h in pending.drain(..) {
                    let _ = h.join();
                }
                let n = std::fs::read_dir(&*dir).map(|r| r.flatten().filter(|e| e.path().extension().map(|x| x == "png").unwrap_or(false)).count()).unwrap_or(0);
                if n == 0 {
                    return Err(format!("Image export failed: no PNG files were written to {}", dir.display()));
                }
            }
        }
        let secs = self.started.elapsed().as_secs_f32();
        Ok(format!(
            "Rendered {} frames in {:.1} s ({:.1} fps) → {}",
            self.total(),
            secs,
            self.total() as f32 / secs.max(0.001),
            self.settings.path.display()
        ))
    }

    /// Stop a render. A half-written video file is removed (PNG frames already written stay).
    pub fn cancel(&mut self) -> String {
        self.done = true;
        if let Sink::Video { tx, child, .. } = &mut self.sink {
            drop(tx.take());
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&self.settings.path);
            return format!("Render cancelled after {} of {} frames; the unfinished video file was removed", self.next, self.total());
        }
        format!("Render cancelled after {} of {} frames; the frames already written stay in {}", self.next, self.total(), self.settings.path.display())
    }
}
