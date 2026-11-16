//! Compact binary form of a motion, stored inside project files as one base64 text field.
//! A key takes 48 bytes instead of several hundred as JSON, so a dense captured dance saves and
//! opens many times faster. Projects saved before this still open (the field may also be the
//! old JSON object).

use crate::motion::*;
use glam::{Quat, Vec3};
use serde::{Deserialize, Deserializer, Serializer};

const MAGIC: &[u8; 4] = b"HMB1";

struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn v3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    fn s(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn curve(&mut self, c: &Curve) {
        self.0.extend_from_slice(&[c.x1, c.y1, c.x2, c.y2]);
    }
}

struct R<'a>(&'a [u8], usize);
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.1 + n > self.0.len() {
            return Err("motion data is cut short".into());
        }
        let b = &self.0[self.1..self.1 + n];
        self.1 += n;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn v3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn s(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn curve(&mut self) -> Result<Curve, String> {
        let b = self.take(4)?;
        Ok(Curve { x1: b[0], y1: b[1], x2: b[2], y2: b[3] })
    }
    fn count(&mut self, min_size: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n.saturating_mul(min_size) > self.0.len() - self.1 {
            return Err("motion data is damaged".into());
        }
        Ok(n)
    }
}

pub fn encode(m: &Motion) -> Vec<u8> {
    let mut w = W(Vec::with_capacity(64 + m.key_count() * 48));
    w.0.extend_from_slice(MAGIC);
    w.s(&m.model_name);
    w.u32(m.bones.len() as u32);
    for (name, keys) in &m.bones {
        w.s(name);
        w.u32(keys.len() as u32);
        for k in keys {
            w.u32(k.frame);
            w.v3(k.pos);
            w.f32(k.rot.x);
            w.f32(k.rot.y);
            w.f32(k.rot.z);
            w.f32(k.rot.w);
            for c in &k.curves {
                w.curve(c);
            }
        }
    }
    w.u32(m.morphs.len() as u32);
    for (name, keys) in &m.morphs {
        w.s(name);
        w.u32(keys.len() as u32);
        for k in keys {
            w.u32(k.frame);
            w.f32(k.weight);
        }
    }
    w.u32(m.camera.len() as u32);
    for k in &m.camera {
        w.u32(k.frame);
        w.v3(k.target);
        w.v3(k.rot);
        w.f32(k.distance);
        w.f32(k.fov);
        w.u8(k.ortho as u8);
        for c in &k.curves {
            w.curve(c);
        }
    }
    w.u32(m.light.len() as u32);
    for k in &m.light {
        w.u32(k.frame);
        w.v3(k.color);
        w.v3(k.direction);
    }
    w.u32(m.shadow.len() as u32);
    for k in &m.shadow {
        w.u32(k.frame);
        w.u8(k.mode);
        w.f32(k.distance);
    }
    w.u32(m.show_ik.len() as u32);
    for k in &m.show_ik {
        w.u32(k.frame);
        w.u8(k.visible as u8);
        w.u32(k.ik.len() as u32);
        for (n, on) in &k.ik {
            w.s(n);
            w.u8(*on as u8);
        }
    }
    w.0
}

pub fn decode(b: &[u8]) -> Result<Motion, String> {
    let mut r = R(b, 0);
    if r.take(4)? != MAGIC {
        return Err("not motion data".into());
    }
    let mut m = Motion { model_name: r.s()?, ..Default::default() };
    for _ in 0..r.count(8)? {
        let name = r.s()?;
        let n = r.count(48)?;
        let mut keys = Vec::with_capacity(n);
        for _ in 0..n {
            let frame = r.u32()?;
            let pos = r.v3()?;
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            let curves = [r.curve()?, r.curve()?, r.curve()?, r.curve()?];
            keys.push(BoneKey { frame, pos, rot, curves });
        }
        m.bones.insert(name, keys);
    }
    for _ in 0..r.count(8)? {
        let name = r.s()?;
        let n = r.count(8)?;
        let mut keys = Vec::with_capacity(n);
        for _ in 0..n {
            keys.push(MorphKey { frame: r.u32()?, weight: r.f32()? });
        }
        m.morphs.insert(name, keys);
    }
    for _ in 0..r.count(61)? {
        let frame = r.u32()?;
        let target = r.v3()?;
        let rot = r.v3()?;
        let distance = r.f32()?;
        let fov = r.f32()?;
        let ortho = r.u8()? != 0;
        let mut curves = [Curve::default(); 6];
        for c in &mut curves {
            *c = r.curve()?;
        }
        m.camera.push(CameraKey { frame, target, rot, distance, fov, ortho, curves });
    }
    for _ in 0..r.count(28)? {
        m.light.push(LightKey { frame: r.u32()?, color: r.v3()?, direction: r.v3()? });
    }
    for _ in 0..r.count(9)? {
        m.shadow.push(ShadowKey { frame: r.u32()?, mode: r.u8()?, distance: r.f32()? });
    }
    for _ in 0..r.count(9)? {
        let frame = r.u32()?;
        let visible = r.u8()? != 0;
        let mut ik = Vec::new();
        for _ in 0..r.count(5)? {
            let n = r.s()?;
            ik.push((n, r.u8()? != 0));
        }
        m.show_ik.push(ShowIkKey { frame, visible, ik });
    }
    Ok(m)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn unbase64(s: &str) -> Result<Vec<u8>, String> {
    let mut lut = [255u8; 256];
    for (i, &c) in B64.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for c in bytes.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for i in 0..4 {
            let ch = *c.get(i).unwrap_or(&b'=');
            let v = if ch == b'=' {
                pad += 1;
                0
            } else {
                let v = lut[ch as usize];
                if v == 255 {
                    return Err("damaged motion text".into());
                }
                v as u32
            };
            n = n << 6 | v;
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// `#[serde(with = "crate::motionbin")]` for motion fields in the project file.
pub fn serialize<S: Serializer>(m: &Motion, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&base64(&encode(m)))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Motion, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Packed(String),
        Old(Motion),
    }
    match Repr::deserialize(d)? {
        Repr::Packed(t) => unbase64(&t).and_then(|b| decode(&b)).map_err(serde::de::Error::custom),
        Repr::Old(m) => Ok(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let mut m = Motion { model_name: "テスト".into(), ..Default::default() };
        m.set_bone_key("センター", BoneKey { frame: 3, pos: Vec3::new(1.0, 2.0, 3.0), rot: Quat::from_rotation_y(0.5), curves: [Curve::EASE_IN; 4] });
        m.set_morph_key("あ", MorphKey { frame: 7, weight: 0.5 });
        let b = decode(&unbase64(&base64(&encode(&m))).unwrap()).unwrap();
        assert_eq!(b.key_count(), m.key_count());
        assert_eq!(b.bones["センター"][0].curves, m.bones["センター"][0].curves);
    }
}
