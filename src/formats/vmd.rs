//! VMD motion reader and writer (bone, morph, camera, light, self-shadow and visibility/IK keys).

use super::reader::{encode_sjis_fixed, Reader, Res, Writer};
use crate::motion::*;
use glam::Vec3;

const HEADER2: &[u8] = b"Vocaloid Motion Data 0002";

pub fn read(data: &[u8]) -> Res<Motion> {
    let mut r = Reader::new(data);
    let header = r.bytes(30)?;
    let name_len = if header.starts_with(HEADER2) {
        20
    } else if header.starts_with(b"Vocaloid Motion Data file") {
        10
    } else {
        return Err("not a VMD motion file".into());
    };
    let mut m = Motion { model_name: r.sjis_fixed(name_len)?, ..Default::default() };

    let bc = r.u32()?;
    for _ in 0..bc {
        let name = r.sjis_fixed(15)?;
        let frame = r.u32()?;
        let pos = r.vec3()?;
        let rot = r.quat()?;
        let ib = r.bytes(64)?;
        let mut curves = [Curve::default(); 4];
        for (i, c) in curves.iter_mut().enumerate() {
            *c = Curve { x1: ib[i] & 0x7f, y1: ib[i + 4] & 0x7f, x2: ib[i + 8] & 0x7f, y2: ib[i + 12] & 0x7f };
        }
        m.bones.entry(name).or_default().push(BoneKey { frame, pos, rot, curves });
    }
    for v in m.bones.values_mut() {
        sort_keys(v, |k| k.frame);
    }
    if r.remaining() >= 4 {
        let mc = r.u32()?;
        for _ in 0..mc {
            let name = r.sjis_fixed(15)?;
            let frame = r.u32()?;
            let weight = r.f32()?;
            m.morphs.entry(name).or_default().push(MorphKey { frame, weight });
        }
        for v in m.morphs.values_mut() {
            sort_keys(v, |k| k.frame);
        }
    }
    if r.remaining() >= 4 {
        let cc = r.u32()?;
        for _ in 0..cc {
            let frame = r.u32()?;
            let distance = r.f32()?;
            let target = r.vec3()?;
            let rot = r.vec3()?;
            let ib = r.bytes(24)?;
            let fov = r.u32()? as f32;
            let persp = r.u8()?;
            let mut curves = [Curve::default(); 6];
            for (i, c) in curves.iter_mut().enumerate() {
                let o = i * 4;
                *c = Curve { x1: ib[o] & 0x7f, x2: ib[o + 1] & 0x7f, y1: ib[o + 2] & 0x7f, y2: ib[o + 3] & 0x7f };
            }
            m.camera.push(CameraKey { frame, target, rot, distance, fov, ortho: persp != 0, curves });
        }
        sort_keys(&mut m.camera, |k| k.frame);
    }
    if r.remaining() >= 4 {
        let lc = r.u32()?;
        for _ in 0..lc {
            let frame = r.u32()?;
            let color = r.vec3()?;
            let direction = r.vec3()?;
            m.light.push(LightKey { frame, color, direction });
        }
        sort_keys(&mut m.light, |k| k.frame);
    }
    if r.remaining() >= 4 {
        let sc = r.u32()?;
        for _ in 0..sc {
            let frame = r.u32()?;
            let mode = r.u8()?;
            let distance = r.f32()?;
            m.shadow.push(ShadowKey { frame, mode, distance });
        }
        sort_keys(&mut m.shadow, |k| k.frame);
    }
    if r.remaining() >= 4 {
        let ic = r.u32()?;
        for _ in 0..ic {
            let frame = r.u32()?;
            let visible = r.u8()? != 0;
            let n = r.u32()?;
            let mut ik = Vec::new();
            for _ in 0..n {
                let name = r.sjis_fixed(20)?;
                let on = r.u8()? != 0;
                ik.push((name, on));
            }
            m.show_ik.push(ShowIkKey { frame, visible, ik });
        }
        sort_keys(&mut m.show_ik, |k| k.frame);
    }
    Ok(m)
}

/// Write a motion in the VMD layout so other MMD-compatible tools can open it.
pub fn write(m: &Motion) -> Vec<u8> {
    let mut w = Writer::new();
    let mut header = HEADER2.to_vec();
    header.resize(30, 0);
    w.bytes(&header);
    w.bytes(&encode_sjis_fixed(&m.model_name, 20));

    let total: usize = m.bones.values().map(|v| v.len()).sum();
    w.u32(total as u32);
    for (name, keys) in &m.bones {
        let nb = encode_sjis_fixed(name, 15);
        for k in keys {
            w.bytes(&nb);
            w.u32(k.frame);
            w.vec3(k.pos);
            w.quat(k.rot);
            // 64 interpolation bytes: first row holds X,Y,Z,R x1 / y1 / x2 / y2,
            // following rows repeat it shifted by one byte as the original tool does.
            let c = &k.curves;
            let mut row = [0u8; 16];
            for i in 0..4 {
                row[i] = c[i].x1;
                row[i + 4] = c[i].y1;
                row[i + 8] = c[i].x2;
                row[i + 12] = c[i].y2;
            }
            let mut ib = [0u8; 64];
            for line in 0..4 {
                for j in 0..16 {
                    let src = j + line;
                    ib[line * 16 + j] = if src < 16 { row[src] } else { 0 };
                }
            }
            // Bytes 2,3 of rows 2..4 are traditionally 0x01/0x00 markers; harmless either way.
            w.bytes(&ib);
        }
    }
    let total: usize = m.morphs.values().map(|v| v.len()).sum();
    w.u32(total as u32);
    for (name, keys) in &m.morphs {
        let nb = encode_sjis_fixed(name, 15);
        for k in keys {
            w.bytes(&nb);
            w.u32(k.frame);
            w.f32(k.weight);
        }
    }
    w.u32(m.camera.len() as u32);
    for k in &m.camera {
        w.u32(k.frame);
        w.f32(k.distance);
        w.vec3(k.target);
        w.vec3(k.rot);
        for c in &k.curves {
            w.u8(c.x1);
            w.u8(c.x2);
            w.u8(c.y1);
            w.u8(c.y2);
        }
        w.u32(k.fov.round().max(1.0) as u32);
        w.u8(if k.ortho { 1 } else { 0 });
    }
    w.u32(m.light.len() as u32);
    for k in &m.light {
        w.u32(k.frame);
        w.vec3(k.color);
        w.vec3(k.direction);
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
            w.bytes(&encode_sjis_fixed(n, 20));
            w.u8(*on as u8);
        }
    }
    let _ = Vec3::ZERO;
    w.buf
}
