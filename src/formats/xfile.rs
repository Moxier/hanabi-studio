//! DirectX text `.x` mesh reader for stage props and accessories.
//! Supports Mesh, MeshNormals, MeshTextureCoords, MeshMaterialList, Material and
//! TextureFilename inside optional Frame hierarchies. Binary/compressed .x is reported as unsupported.

use super::reader::decode_sjis;
use crate::model::*;
use glam::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f32),
    Str(String),
    Open,
    Close,
    Sep,
}

fn tokenize(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && i + 1 < c.len() && c[i + 1] == '/' || ch == '#' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '{' {
            out.push(Tok::Open);
            i += 1;
        } else if ch == '}' {
            out.push(Tok::Close);
            i += 1;
        } else if ch == ',' || ch == ';' {
            out.push(Tok::Sep);
            i += 1;
        } else if ch == '"' {
            let mut j = i + 1;
            while j < c.len() && c[j] != '"' {
                j += 1;
            }
            out.push(Tok::Str(c[i + 1..j.min(c.len())].iter().collect()));
            i = j + 1;
        } else if ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit() {
            let mut j = i + 1;
            while j < c.len() && (c[j].is_ascii_alphanumeric() || c[j] == '.' || c[j] == '-' || c[j] == '+') {
                j += 1;
            }
            let t: String = c[i..j].iter().collect();
            out.push(match t.parse::<f32>() {
                Ok(v) => Tok::Num(v),
                Err(_) => Tok::Ident(t),
            });
            i = j;
        } else if ch == '<' {
            while i < c.len() && c[i] != '>' {
                i += 1;
            }
            i += 1;
        } else {
            let mut j = i;
            while j < c.len() && !c[j].is_whitespace() && !"{};,\"".contains(c[j]) {
                j += 1;
            }
            if j == i {
                j += 1;
            }
            out.push(Tok::Ident(c[i..j].iter().collect()));
            i = j;
        }
    }
    out
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn num(&mut self) -> Result<f32, String> {
        while let Some(Tok::Sep) = self.peek() {
            self.i += 1;
        }
        match self.t.get(self.i) {
            Some(Tok::Num(v)) => {
                self.i += 1;
                Ok(*v)
            }
            other => Err(format!("expected number, found {other:?}")),
        }
    }
    fn skip_block(&mut self) {
        // assumes we are just after an Open
        let mut depth = 1;
        while self.i < self.t.len() && depth > 0 {
            match self.t[self.i] {
                Tok::Open => depth += 1,
                Tok::Close => depth -= 1,
                _ => {}
            }
            self.i += 1;
        }
    }
    /// Read `Ident [name] {` and return the identifier.
    fn block_head(&mut self) -> Option<String> {
        let id = match self.peek()? {
            Tok::Ident(s) => s.clone(),
            _ => return None,
        };
        self.i += 1;
        if let Some(Tok::Ident(_)) = self.peek() {
            self.i += 1;
        }
        if let Some(Tok::Open) = self.peek() {
            self.i += 1;
            Some(id)
        } else {
            Some(String::new())
        }
    }
}

#[derive(Default)]
struct MeshPart {
    pos: Vec<Vec3>,
    faces: Vec<Vec<u32>>,
    normals: Vec<Vec3>,
    nfaces: Vec<Vec<u32>>,
    uvs: Vec<Vec2>,
    face_mat: Vec<u32>,
    mats: Vec<(Vec4, f32, Vec3, Vec3, String)>,
}

fn parse_material(p: &mut P) -> Result<(Vec4, f32, Vec3, Vec3, String), String> {
    let d = Vec4::new(p.num()?, p.num()?, p.num()?, p.num()?);
    let pow = p.num()?;
    let s = Vec3::new(p.num()?, p.num()?, p.num()?);
    let e = Vec3::new(p.num()?, p.num()?, p.num()?);
    let mut tex = String::new();
    loop {
        match p.peek() {
            Some(Tok::Close) => {
                p.i += 1;
                break;
            }
            Some(Tok::Ident(_)) => {
                let h = p.block_head().unwrap_or_default();
                if h == "TextureFilename" || h == "TextureFileName" {
                    while let Some(t) = p.peek().cloned() {
                        p.i += 1;
                        match t {
                            Tok::Str(s) => tex = s,
                            Tok::Close => break,
                            _ => {}
                        }
                    }
                } else if !h.is_empty() {
                    p.skip_block();
                }
            }
            None => break,
            _ => p.i += 1,
        }
    }
    Ok((d, pow, s, e, tex))
}

fn parse_mesh(p: &mut P) -> Result<MeshPart, String> {
    let mut m = MeshPart::default();
    let nv = p.num()? as usize;
    for _ in 0..nv {
        m.pos.push(Vec3::new(p.num()?, p.num()?, p.num()?));
    }
    let nf = p.num()? as usize;
    for _ in 0..nf {
        let n = p.num()? as usize;
        let mut f = Vec::with_capacity(n);
        for _ in 0..n {
            f.push(p.num()? as u32);
        }
        m.faces.push(f);
    }
    loop {
        match p.peek() {
            Some(Tok::Close) => {
                p.i += 1;
                break;
            }
            Some(Tok::Ident(_)) => {
                let h = p.block_head().unwrap_or_default();
                match h.as_str() {
                    "MeshNormals" => {
                        let n = p.num()? as usize;
                        for _ in 0..n {
                            m.normals.push(Vec3::new(p.num()?, p.num()?, p.num()?));
                        }
                        let nf = p.num()? as usize;
                        for _ in 0..nf {
                            let c = p.num()? as usize;
                            let mut f = Vec::new();
                            for _ in 0..c {
                                f.push(p.num()? as u32);
                            }
                            m.nfaces.push(f);
                        }
                        p.skip_block();
                    }
                    "MeshTextureCoords" => {
                        let n = p.num()? as usize;
                        for _ in 0..n {
                            m.uvs.push(Vec2::new(p.num()?, p.num()?));
                        }
                        p.skip_block();
                    }
                    "MeshMaterialList" => {
                        let _nm = p.num()? as usize;
                        let nf = p.num()? as usize;
                        for _ in 0..nf {
                            m.face_mat.push(p.num()? as u32);
                        }
                        loop {
                            match p.peek() {
                                Some(Tok::Close) => {
                                    p.i += 1;
                                    break;
                                }
                                Some(Tok::Ident(_)) => {
                                    let h = p.block_head().unwrap_or_default();
                                    if h == "Material" {
                                        m.mats.push(parse_material(p)?);
                                    } else if !h.is_empty() {
                                        p.skip_block();
                                    }
                                }
                                None => break,
                                _ => p.i += 1,
                            }
                        }
                    }
                    "" => {}
                    _ => p.skip_block(),
                }
            }
            None => break,
            _ => p.i += 1,
        }
    }
    Ok(m)
}

fn collect(p: &mut P, out: &mut Vec<MeshPart>) -> Result<(), String> {
    while let Some(t) = p.peek().cloned() {
        match t {
            Tok::Close => {
                p.i += 1;
                return Ok(());
            }
            Tok::Ident(_) => {
                let h = p.block_head().unwrap_or_default();
                match h.as_str() {
                    "Mesh" => out.push(parse_mesh(p)?),
                    "Frame" => collect(p, out)?,
                    "" => {}
                    _ => p.skip_block(),
                }
            }
            _ => p.i += 1,
        }
    }
    Ok(())
}

pub fn read(data: &[u8]) -> Result<ModelData, String> {
    if data.len() < 16 || &data[0..4] != b"xof " {
        return Err("not a DirectX .x file".into());
    }
    let fmt = &data[8..12];
    if fmt != b"txt " {
        return Err("binary or compressed .x files are not supported yet (save it as text .x)".into());
    }
    let text = match std::str::from_utf8(&data[16..]) {
        Ok(s) => s.to_string(),
        Err(_) => decode_sjis(&data[16..]),
    };
    let mut p = P { t: tokenize(&text), i: 0 };
    let mut parts = Vec::new();
    collect(&mut p, &mut parts)?;
    if parts.is_empty() {
        return Err("no mesh found in .x file".into());
    }
    let mut model = ModelData { name: "Accessory".into(), is_accessory: true, ..Default::default() };
    for part in parts {
        let nmat = part.mats.len().max(1);
        let base_mat = model.materials.len();
        for (d, pow, s, e, tex) in &part.mats {
            let tex_i = if tex.is_empty() {
                -1
            } else {
                let (main, sph) = match tex.split_once('*') {
                    Some((a, b)) => (a.to_string(), Some(b.to_string())),
                    None => (tex.clone(), None),
                };
                let _ = sph;
                model.textures.push(main.replace('\\', "/"));
                (model.textures.len() - 1) as i32
            };
            model.materials.push(Material {
                name: format!("x{}", model.materials.len()),
                diffuse: *d,
                specular_power: *pow,
                specular: *s,
                // .x accessories treat the emissive colour as ambient.
                ambient: *e,
                texture: tex_i,
                edge: false,
                double_sided: true,
                ..Default::default()
            });
        }
        if part.mats.is_empty() {
            model.materials.push(Material { name: "x".into(), edge: false, ..Default::default() });
        }
        // Gather triangles per material.
        let mut per: Vec<Vec<u32>> = vec![Vec::new(); nmat];
        for (fi, f) in part.faces.iter().enumerate() {
            let mi = part.face_mat.get(fi).copied().unwrap_or(0) as usize % nmat;
            for k in 1..f.len().saturating_sub(1) {
                let tri = [f[0], f[k], f[k + 1]];
                for (corner, &vi) in tri.iter().enumerate() {
                    let pos = part.pos.get(vi as usize).copied().unwrap_or(Vec3::ZERO);
                    let normal = part
                        .nfaces
                        .get(fi)
                        .and_then(|nf| nf.get(match corner { 0 => 0, 1 => k, _ => k + 1 }))
                        .and_then(|&ni| part.normals.get(ni as usize))
                        .copied()
                        .unwrap_or(Vec3::Y);
                    let uv = part.uvs.get(vi as usize).copied().unwrap_or(Vec2::ZERO);
                    model.vertices.push(Vertex {
                        pos,
                        normal,
                        uv,
                        bones: [0; 4],
                        weights: [1.0, 0.0, 0.0, 0.0],
                        edge_scale: 0.0,
                        sdef: None,
                    });
                    per[mi].push((model.vertices.len() - 1) as u32);
                }
            }
        }
        for (i, idx) in per.into_iter().enumerate() {
            model.materials[base_mat + i].index_count = idx.len() as u32;
            model.indices.extend(idx);
        }
    }
    // Smooth normals if the file had none.
    model.finalize();
    Ok(model)
}
