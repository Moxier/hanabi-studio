//! Small little-endian binary reader/writer used by the file format modules.

use glam::{Quat, Vec2, Vec3, Vec4};

pub type Res<T> = Result<T, String>;

pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn eof(&self) -> bool {
        self.remaining() == 0
    }
    pub fn bytes(&mut self, n: usize) -> Res<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(format!("unexpected end of file at byte {} (wanted {} more)", self.pos, n));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn skip(&mut self, n: usize) -> Res<()> {
        self.bytes(n).map(|_| ())
    }
    pub fn u8(&mut self) -> Res<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn i8(&mut self) -> Res<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn u16(&mut self) -> Res<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Res<i16> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Res<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self) -> Res<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn f32(&mut self) -> Res<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn vec2(&mut self) -> Res<Vec2> {
        Ok(Vec2::new(self.f32()?, self.f32()?))
    }
    pub fn vec3(&mut self) -> Res<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn vec4(&mut self) -> Res<Vec4> {
        Ok(Vec4::new(self.f32()?, self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn quat(&mut self) -> Res<Quat> {
        let v = self.vec4()?;
        let q = Quat::from_xyzw(v.x, v.y, v.z, v.w);
        Ok(if q.length_squared() > 1e-8 { q.normalize() } else { Quat::IDENTITY })
    }
    /// Index of a given byte size (1, 2 or 4). Size 1 and 2 are unsigned for vertices
    /// when `unsigned_small` is set; otherwise signed with -1 meaning "none".
    pub fn index(&mut self, size: u8, unsigned_small: bool) -> Res<i32> {
        Ok(match size {
            1 => {
                if unsigned_small {
                    self.u8()? as i32
                } else {
                    self.i8()? as i32
                }
            }
            2 => {
                if unsigned_small {
                    self.u16()? as i32
                } else {
                    self.i16()? as i32
                }
            }
            _ => self.i32()?,
        })
    }
    /// Fixed-size Shift-JIS string padded with zero bytes.
    pub fn sjis_fixed(&mut self, n: usize) -> Res<String> {
        let b = self.bytes(n)?;
        Ok(decode_sjis_z(b))
    }
}

/// Decode a zero-terminated name. Files from Japanese PCs use Shift-JIS; files saved on
/// Chinese PCs often hold GBK instead, so fall back to it when Shift-JIS doesn't fit.
/// A name cut off in the middle of a two-byte character is decoded without the half character.
pub fn decode_sjis_z(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let b = &b[..end];
    let clean = |enc: &'static encoding_rs::Encoding, b: &[u8]| -> Option<String> {
        let (s, bad) = enc.decode_without_bom_handling(b);
        (!bad).then(|| s.trim_end().to_string())
    };
    if let Some(s) = clean(encoding_rs::SHIFT_JIS, b) {
        return s;
    }
    if !b.is_empty() {
        if let Some(s) = clean(encoding_rs::SHIFT_JIS, &b[..b.len() - 1]) {
            if clean(encoding_rs::GBK, b).is_none() {
                return s;
            }
        }
    }
    if let Some(s) = clean(encoding_rs::GBK, b) {
        return s;
    }
    if !b.is_empty() {
        if let Some(s) = clean(encoding_rs::GBK, &b[..b.len() - 1]) {
            return s;
        }
    }
    let (s, _, _) = encoding_rs::SHIFT_JIS.decode(b);
    s.trim_end().to_string()
}

pub fn decode_sjis(b: &[u8]) -> String {
    let (s, _, _) = encoding_rs::SHIFT_JIS.decode(b);
    s.into_owned()
}

fn encodable(enc: &'static encoding_rs::Encoding, s: &str) -> bool {
    let (_, _, bad) = enc.encode(s);
    !bad
}

/// Encode a name into a fixed-size zero-padded field: Shift-JIS when every character fits,
/// otherwise GBK (names made on Chinese PCs), otherwise Shift-JIS with `?` for what doesn't fit.
/// Truncates on a character boundary when the text is too long.
pub fn encode_sjis_fixed(s: &str, n: usize) -> Vec<u8> {
    let enc = if encodable(encoding_rs::SHIFT_JIS, s) || !encodable(encoding_rs::GBK, s) { encoding_rs::SHIFT_JIS } else { encoding_rs::GBK };
    let mut out = Vec::with_capacity(n);
    for ch in s.chars() {
        let mut buf = [0u8; 4];
        let (bytes, _, bad) = enc.encode(ch.encode_utf8(&mut buf));
        let bytes: &[u8] = if bad { b"?" } else { &bytes };
        if out.len() + bytes.len() > n {
            break;
        }
        out.extend_from_slice(bytes);
    }
    if out.len() < n {
        out.push(0);
    }
    out.resize(n, 0);
    out
}

/// What a name looks like after being stored in an `n`-byte VMD field and read back.
pub fn fit_name(s: &str, n: usize) -> String {
    decode_sjis_z(&encode_sjis_fixed(s, n))
}

pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    pub fn quat(&mut self, q: Quat) {
        self.f32(q.x);
        self.f32(q.y);
        self.f32(q.z);
        self.f32(q.w);
    }
}
