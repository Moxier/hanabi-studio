//! VPD pose files (text, usually Shift-JIS).

use super::reader::decode_sjis;
use glam::{Quat, Vec3};

#[derive(Clone, Debug, Default)]
pub struct Pose {
    pub model_name: String,
    pub bones: Vec<(String, Vec3, Quat)>,
    pub morphs: Vec<(String, f32)>,
}

fn nums(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter_map(|t| t.trim().parse::<f32>().ok())
        .collect()
}

pub fn read(data: &[u8]) -> Result<Pose, String> {
    let text = match std::str::from_utf8(data) {
        Ok(s) => s.to_string(),
        Err(_) => decode_sjis(data),
    };
    if !text.trim_start_matches('\u{feff}').starts_with("Vocaloid Pose Data file") {
        return Err("not a VPD pose file".into());
    }
    // Strip comments.
    let clean: String = text
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut pose = Pose::default();
    let mut lines = clean.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.next();
    if let Some(n) = lines.next() {
        pose.model_name = n.trim_end_matches(';').trim_end_matches(".osm").to_string();
    }
    let rest: Vec<&str> = lines.collect();
    let mut i = 0;
    while i < rest.len() {
        let l = rest[i];
        if let Some(open) = l.find('{') {
            let head = &l[..open];
            let name = l[open + 1..].trim().to_string();
            let mut body = Vec::new();
            i += 1;
            while i < rest.len() && !rest[i].starts_with('}') {
                body.push(rest[i]);
                i += 1;
            }
            if head.starts_with("Bone") && body.len() >= 2 {
                let t = nums(body[0]);
                let q = nums(body[1]);
                if t.len() >= 3 && q.len() >= 4 {
                    let quat = Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
                    pose.bones.push((name, Vec3::new(t[0], t[1], t[2]), quat));
                }
            } else if head.starts_with("Morph") && !body.is_empty() {
                if let Some(w) = nums(body[0]).first() {
                    pose.morphs.push((name, *w));
                }
            }
        }
        i += 1;
    }
    Ok(pose)
}

/// Write a pose (UTF-8 is not what older tools expect, so encode as Shift-JIS).
pub fn write(p: &Pose) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("Vocaloid Pose Data file\r\n\r\n");
    s.push_str(&format!("{}.osm;\r\n", p.model_name));
    s.push_str(&format!("{};\r\n\r\n", p.bones.len()));
    for (i, (n, t, q)) in p.bones.iter().enumerate() {
        s.push_str(&format!(
            "Bone{i}{{{n}\r\n  {:.6},{:.6},{:.6};\r\n  {:.6},{:.6},{:.6},{:.6};\r\n}}\r\n\r\n",
            t.x, t.y, t.z, q.x, q.y, q.z, q.w
        ));
    }
    for (i, (n, w)) in p.morphs.iter().enumerate() {
        s.push_str(&format!("Morph{i}{{{n}\r\n  {:.6};\r\n}}\r\n\r\n", w));
    }
    let (b, _, _) = encoding_rs::SHIFT_JIS.encode(&s);
    b.into_owned()
}
