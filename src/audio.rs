//! Song playback in sync with the timeline, built in: the WAV is decoded by the app and sent
//! straight to the system's sound output (ALSA on Linux, loaded at run time so nothing extra
//! is needed to build; the Windows wave-out service on Windows 10/11). If neither can be
//! opened, a system player (ffplay) is used as a last resort.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Song volume as f32 bits (1.0 = as recorded, 0 = silent, up to 2.0). Read by the sound
/// thread every few milliseconds, so the slider changes the level while the song plays.
static VOLUME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

pub fn set_volume(v: f32) {
    VOLUME.store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
}

pub fn volume() -> f32 {
    f32::from_bits(VOLUME.load(Ordering::Relaxed))
}

/// Decoded song: 16-bit interleaved stereo.
pub struct Pcm {
    pub rate: u32,
    pub samples: Vec<i16>,
}

pub struct AudioPlayer {
    child: Option<Child>,
    thread: Option<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>,
    cache: Option<(PathBuf, Arc<Pcm>)>,
    pub backend: Option<&'static str>,
    pub last_error: Option<String>,
}

fn have(cmd: &str) -> bool {
    Command::new(cmd).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

impl AudioPlayer {
    pub fn new() -> Self {
        let backend = if native::available() {
            Some(native::NAME)
        } else if have("ffplay") {
            Some("ffplay")
        } else {
            None
        };
        Self { child: None, thread: None, cache: None, backend, last_error: None }
    }

    fn pcm_for(&mut self, path: &Path) -> Option<Arc<Pcm>> {
        if let Some((p, pcm)) = &self.cache {
            if p == path {
                return Some(pcm.clone());
            }
        }
        let data = std::fs::read(path).ok()?;
        match crate::formats::wav::decode_stereo16(&data) {
            Ok(pcm) => {
                let pcm = Arc::new(pcm);
                self.cache = Some((path.to_path_buf(), pcm.clone()));
                Some(pcm)
            }
            Err(e) => {
                self.last_error = Some(e);
                None
            }
        }
    }

    pub fn play(&mut self, path: &Path, from_seconds: f32) {
        self.stop();
        if self.backend == Some(native::NAME) {
            if let Some(pcm) = self.pcm_for(path) {
                let stop = Arc::new(AtomicBool::new(false));
                let s2 = stop.clone();
                let handle = std::thread::Builder::new().name("song".into()).spawn(move || {
                    if let Err(e) = native::play(&pcm, from_seconds, &s2) {
                        eprintln!("sound output: {e}");
                    }
                });
                if let Ok(h) = handle {
                    self.thread = Some((stop, h));
                    return;
                }
            }
        }
        if have("ffplay") {
            self.child = Command::new("ffplay")
                .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-volume", &format!("{}", (volume() * 100.0).round().min(100.0) as i32), "-ss", &format!("{:.3}", from_seconds.max(0.0))])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok();
        }
    }

    /// Play a short slice of the song (for hearing the frame you are on while stepping or
    /// scrubbing). The slice fades in and out so it doesn't click; a new slice replaces the last.
    pub fn play_snippet(&mut self, path: &Path, from_seconds: f32, seconds: f32) {
        self.stop();
        if self.backend == Some(native::NAME) {
            let Some(pcm) = self.pcm_for(path) else { return };
            let total = pcm.samples.len() / 2;
            let start = (from_seconds.max(0.0) as f64 * pcm.rate as f64) as usize;
            if from_seconds < 0.0 || start >= total {
                return;
            }
            let len = ((seconds as f64 * pcm.rate as f64) as usize).min(total - start);
            if len < 4 {
                return;
            }
            let mut samples = pcm.samples[start * 2..(start + len) * 2].to_vec();
            let fade = (pcm.rate as usize / 200).min(len / 2).max(1); // 5 ms
            for k in 0..fade {
                let g = k as f32 / fade as f32;
                for c in 0..2 {
                    samples[k * 2 + c] = (samples[k * 2 + c] as f32 * g) as i16;
                    let e = (len - 1 - k) * 2 + c;
                    samples[e] = (samples[e] as f32 * g) as i16;
                }
            }
            let slice = Pcm { rate: pcm.rate, samples };
            let stop = Arc::new(AtomicBool::new(false));
            let s2 = stop.clone();
            if let Ok(h) = std::thread::Builder::new().name("song-slice".into()).spawn(move || {
                let _ = native::play(&slice, 0.0, &s2);
            }) {
                self.thread = Some((stop, h));
            }
        } else if have("ffplay") && from_seconds >= 0.0 {
            self.child = Command::new("ffplay")
                .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-volume", &format!("{}", (volume() * 100.0).round().min(100.0) as i32)])
                .args(["-ss", &format!("{:.3}", from_seconds), "-t", &format!("{:.3}", seconds)])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok();
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some((stop, h)) = self.thread.take() {
            stop.store(true, Ordering::SeqCst);
            let _ = h.join();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.child.is_some() || self.thread.as_ref().map(|(_, h)| !h.is_finished()).unwrap_or(false)
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Fill `out` (interleaved stereo) from the song starting at stereo frame `pos`, which may be
/// negative (silence before the song starts). Returns the next position.
fn fill(pcm: &Pcm, pos: i64, out: &mut [i16]) -> i64 {
    let frames = out.len() / 2;
    let total = (pcm.samples.len() / 2) as i64;
    let gain = volume();
    let scale = |v: i16| -> i16 { if gain == 1.0 { v } else { (v as f32 * gain).round().clamp(-32768.0, 32767.0) as i16 } };
    for k in 0..frames {
        let f = pos + k as i64;
        let (l, r) = if f >= 0 && f < total { (pcm.samples[f as usize * 2], pcm.samples[f as usize * 2 + 1]) } else { (0, 0) };
        out[k * 2] = scale(l);
        out[k * 2 + 1] = scale(r);
    }
    pos + frames as i64
}

#[cfg(target_os = "linux")]
mod native {
    use super::{fill, Pcm};
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};

    pub const NAME: &str = "built-in (ALSA)";

    type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
    type SetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
    type WriteI = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
    type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
    type Simple = unsafe extern "C" fn(*mut c_void) -> c_int;

    fn lib() -> Option<libloading::Library> {
        unsafe { libloading::Library::new("libasound.so.2").or_else(|_| libloading::Library::new("libasound.so")).ok() }
    }

    pub fn available() -> bool {
        lib().is_some()
    }

    pub fn play(pcm: &Pcm, from: f32, stop: &AtomicBool) -> Result<(), String> {
        let lib = lib().ok_or("libasound not found")?;
        unsafe {
            let open: libloading::Symbol<Open> = lib.get(b"snd_pcm_open\0").map_err(|e| e.to_string())?;
            let set_params: libloading::Symbol<SetParams> = lib.get(b"snd_pcm_set_params\0").map_err(|e| e.to_string())?;
            let writei: libloading::Symbol<WriteI> = lib.get(b"snd_pcm_writei\0").map_err(|e| e.to_string())?;
            let recover: libloading::Symbol<Recover> = lib.get(b"snd_pcm_recover\0").map_err(|e| e.to_string())?;
            let drop_fn: libloading::Symbol<Simple> = lib.get(b"snd_pcm_drop\0").map_err(|e| e.to_string())?;
            let drain: libloading::Symbol<Simple> = lib.get(b"snd_pcm_drain\0").map_err(|e| e.to_string())?;
            let close: libloading::Symbol<Simple> = lib.get(b"snd_pcm_close\0").map_err(|e| e.to_string())?;
            let mut h: *mut c_void = std::ptr::null_mut();
            // stream 0 = playback, format 2 = signed 16-bit little endian, access 3 = interleaved
            // HANABI_SOUND_DEVICE=null lets tests play into the silent device.
            let dev = std::ffi::CString::new(std::env::var("HANABI_SOUND_DEVICE").unwrap_or_else(|_| "default".into())).unwrap_or_default();
            if open(&mut h, dev.as_ptr(), 0, 0) < 0 || h.is_null() {
                return Err("can't open the default sound device".into());
            }
            if set_params(h, 2, 3, 2, pcm.rate, 1, 60_000) < 0 {
                close(h);
                return Err("the sound device refused the song's format".into());
            }
            let mut pos = (from as f64 * pcm.rate as f64).round() as i64;
            let total = (pcm.samples.len() / 2) as i64;
            let chunk = (pcm.rate / 100).max(64) as usize; // 10 ms
            let mut buf = vec![0i16; chunk * 2];
            while !stop.load(Ordering::SeqCst) && pos < total {
                let next = fill(pcm, pos, &mut buf);
                let mut done = 0usize;
                while done < chunk && !stop.load(Ordering::SeqCst) {
                    let r = writei(h, buf[done * 2..].as_ptr() as *const c_void, (chunk - done) as c_ulong);
                    if r < 0 {
                        if recover(h, r as c_int, 1) < 0 {
                            close(h);
                            return Err("sound output stopped".into());
                        }
                    } else {
                        done += r as usize;
                    }
                }
                pos = next;
            }
            if stop.load(Ordering::SeqCst) {
                drop_fn(h);
            } else {
                drain(h);
            }
            close(h);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::{fill, Pcm};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub const NAME: &str = "built-in (Windows)";

    #[repr(C)]
    struct WaveFormatEx {
        format_tag: u16,
        channels: u16,
        samples_per_sec: u32,
        avg_bytes_per_sec: u32,
        block_align: u16,
        bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct WaveHdr {
        data: *mut i8,
        buffer_length: u32,
        bytes_recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: *mut WaveHdr,
        reserved: usize,
    }

    #[link(name = "winmm")]
    extern "system" {
        fn waveOutOpen(h: *mut *mut c_void, device: u32, fmt: *const WaveFormatEx, cb: usize, inst: usize, flags: u32) -> u32;
        fn waveOutPrepareHeader(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutUnprepareHeader(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutWrite(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutReset(h: *mut c_void) -> u32;
        fn waveOutClose(h: *mut c_void) -> u32;
    }

    const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
    const WHDR_DONE: u32 = 1;
    const WHDR_PREPARED: u32 = 2;

    pub fn available() -> bool {
        true
    }

    pub fn play(pcm: &Pcm, from: f32, stop: &AtomicBool) -> Result<(), String> {
        let fmt = WaveFormatEx {
            format_tag: 1,
            channels: 2,
            samples_per_sec: pcm.rate,
            avg_bytes_per_sec: pcm.rate * 4,
            block_align: 4,
            bits_per_sample: 16,
            cb_size: 0,
        };
        let hsz = std::mem::size_of::<WaveHdr>() as u32;
        unsafe {
            let mut h: *mut c_void = std::ptr::null_mut();
            if waveOutOpen(&mut h, WAVE_MAPPER, &fmt, 0, 0, 0) != 0 {
                return Err("can't open the sound device".into());
            }
            let chunk = (pcm.rate / 50).max(256) as usize; // 20 ms
            const N: usize = 4;
            let mut bufs: Vec<Vec<i16>> = (0..N).map(|_| vec![0i16; chunk * 2]).collect();
            let mut hdrs: Vec<Box<WaveHdr>> = (0..N)
                .map(|_| Box::new(WaveHdr { data: std::ptr::null_mut(), buffer_length: 0, bytes_recorded: 0, user: 0, flags: WHDR_DONE, loops: 0, next: std::ptr::null_mut(), reserved: 0 }))
                .collect();
            let mut pos = (from as f64 * pcm.rate as f64).round() as i64;
            let total = (pcm.samples.len() / 2) as i64;
            'outer: while pos < total {
                for i in 0..N {
                    while std::ptr::read_volatile(&hdrs[i].flags) & WHDR_DONE == 0 {
                        if stop.load(Ordering::SeqCst) {
                            break 'outer;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    if stop.load(Ordering::SeqCst) || pos >= total {
                        break 'outer;
                    }
                    let hdr: *mut WaveHdr = &mut *hdrs[i];
                    if (*hdr).flags & WHDR_PREPARED != 0 {
                        waveOutUnprepareHeader(h, hdr, hsz);
                    }
                    pos = fill(pcm, pos, &mut bufs[i]);
                    (*hdr).data = bufs[i].as_mut_ptr() as *mut i8;
                    (*hdr).buffer_length = (chunk * 4) as u32;
                    (*hdr).flags = 0;
                    waveOutPrepareHeader(h, hdr, hsz);
                    waveOutWrite(h, hdr, hsz);
                }
            }
            if stop.load(Ordering::SeqCst) {
                waveOutReset(h);
            } else {
                // let the queued buffers finish
                for i in 0..N {
                    while std::ptr::read_volatile(&hdrs[i].flags) & WHDR_DONE == 0 && !stop.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
                waveOutReset(h);
            }
            for i in 0..N {
                let hdr: *mut WaveHdr = &mut *hdrs[i];
                if (*hdr).flags & WHDR_PREPARED != 0 {
                    waveOutUnprepareHeader(h, hdr, hsz);
                }
            }
            waveOutClose(h);
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod native {
    use super::Pcm;
    use std::sync::atomic::AtomicBool;
    pub const NAME: &str = "built-in";
    pub fn available() -> bool {
        false
    }
    pub fn play(_: &Pcm, _: f32, _: &AtomicBool) -> Result<(), String> {
        Err("no built-in sound output on this system".into())
    }
}

/// For the self-test: send up to `seconds` of a song to the sound output and report errors.
pub fn test_output(pcm: &Pcm, from: f32, seconds: f32) -> Result<&'static str, String> {
    if !native::available() {
        return Err("no built-in sound output".into());
    }
    let short = Pcm { rate: pcm.rate, samples: pcm.samples.iter().skip((from * pcm.rate as f32) as usize * 2).take((seconds * pcm.rate as f32) as usize * 2).copied().collect() };
    let stop = AtomicBool::new(false);
    native::play(&short, 0.0, &stop).map(|_| native::NAME)
}
