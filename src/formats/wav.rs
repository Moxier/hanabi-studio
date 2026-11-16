//! WAV (RIFF) audio reader: PCM 8/16/24/32-bit and 32-bit float. Produces a mono preview
//! used for the timeline waveform and the length of the song.

#[derive(Clone, Debug, Default)]
pub struct Audio {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: f32,
    /// Peak envelope, one value per 1/100 s, 0..1.
    pub peaks: Vec<f32>,
}

pub fn read(data: &[u8]) -> Result<Audio, String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut samples: Option<&[u8]> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        let body = &data[pos + 8..(pos + 8 + len).min(data.len())];
        if id == b"fmt " && body.len() >= 16 {
            let mut format = u16::from_le_bytes([body[0], body[1]]);
            let ch = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            if format == 0xFFFE && body.len() >= 26 {
                format = u16::from_le_bytes([body[24], body[25]]);
            }
            fmt = Some((format, ch, rate, bits));
        } else if id == b"data" {
            samples = Some(body);
        }
        pos += 8 + len + (len & 1);
    }
    let (format, ch, rate, bits) = fmt.ok_or("WAV file has no format chunk")?;
    let raw = samples.ok_or("WAV file has no data chunk")?;
    if ch == 0 || rate == 0 {
        return Err("invalid WAV format".into());
    }
    let bps = (bits as usize / 8).max(1);
    let frame_bytes = bps * ch as usize;
    let frames = raw.len() / frame_bytes;
    let sample = |i: usize| -> f32 {
        let b = &raw[i..i + bps];
        match (format, bits) {
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (_, 8) => (b[0] as f32 - 128.0) / 128.0,
            (_, 16) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (_, 24) => (((b[0] as i32) << 8 | (b[1] as i32) << 16 | (b[2] as i32) << 24) >> 8) as f32 / 8388608.0,
            (_, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2147483648.0,
            _ => 0.0,
        }
    };
    let per_bucket = (rate as usize / 100).max(1);
    let mut peaks = Vec::with_capacity(frames / per_bucket + 1);
    let mut f = 0;
    while f < frames {
        let end = (f + per_bucket).min(frames);
        let mut peak = 0.0f32;
        let mut k = f;
        while k < end {
            let v = sample(k * frame_bytes).abs();
            peak = peak.max(v);
            k += 4;
        }
        peaks.push(peak.min(1.0));
        f = end;
    }
    Ok(Audio { sample_rate: rate, channels: ch, duration: frames as f32 / rate as f32, peaks })
}

/// Decode the whole song to 16-bit interleaved stereo for the built-in player.
pub fn decode_stereo16(data: &[u8]) -> Result<crate::audio::Pcm, String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("only WAV songs can be played inside the app".into());
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut raw: Option<&[u8]> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        let body = &data[pos + 8..(pos + 8 + len).min(data.len())];
        if id == b"fmt " && body.len() >= 16 {
            let mut format = u16::from_le_bytes([body[0], body[1]]);
            let ch = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            if format == 0xFFFE && body.len() >= 26 {
                format = u16::from_le_bytes([body[24], body[25]]);
            }
            fmt = Some((format, ch, rate, bits));
        } else if id == b"data" {
            raw = Some(body);
        }
        pos += 8 + len + (len & 1);
    }
    let (format, ch, rate, bits) = fmt.ok_or("WAV file has no format chunk")?;
    let raw = raw.ok_or("WAV file has no data chunk")?;
    if ch == 0 || rate == 0 || !(matches!(bits, 8 | 16 | 24 | 32)) {
        return Err("this WAV format can't be played".into());
    }
    let bps = bits as usize / 8;
    let frame_bytes = bps * ch as usize;
    let frames = raw.len() / frame_bytes;
    let sample = |i: usize| -> i16 {
        let b = &raw[i..i + bps];
        match (format, bits) {
            (3, 32) => (f32::from_le_bytes([b[0], b[1], b[2], b[3]]).clamp(-1.0, 1.0) * 32767.0) as i16,
            (_, 8) => ((b[0] as i16) - 128) << 8,
            (_, 16) => i16::from_le_bytes([b[0], b[1]]),
            (_, 24) => i16::from_le_bytes([b[1], b[2]]),
            (_, 32) => i16::from_le_bytes([b[2], b[3]]),
            _ => 0,
        }
    };
    let mut samples = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let base = f * frame_bytes;
        let l = sample(base);
        let r = if ch >= 2 { sample(base + bps) } else { l };
        samples.push(l);
        samples.push(r);
    }
    Ok(crate::audio::Pcm { rate, samples })
}
