//! Colour look-up tables in the common `.cube` text format (3D LUTs, and 1D LUTs which are
//! turned into a 3D table). Written from the format's public description.

#[derive(Clone, Debug)]
pub struct Lut {
    pub title: String,
    pub size: usize,
    /// RGB triples, red changing fastest, then green, then blue.
    pub data: Vec<f32>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
}

fn three(parts: &[&str]) -> Option<[f32; 3]> {
    if parts.len() < 3 {
        return None;
    }
    Some([parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?])
}

pub fn parse_cube(text: &str) -> Result<Lut, String> {
    let mut title = String::new();
    let mut size3 = 0usize;
    let mut size1 = 0usize;
    let mut dmin = [0.0f32; 3];
    let mut dmax = [1.0f32; 3];
    let mut values: Vec<f32> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let key = parts[0].to_ascii_uppercase();
        match key.as_str() {
            "TITLE" => title = line[5..].trim().trim_matches('"').to_string(),
            "LUT_3D_SIZE" => size3 = parts.get(1).and_then(|v| v.parse().ok()).ok_or("bad LUT_3D_SIZE")?,
            "LUT_1D_SIZE" => size1 = parts.get(1).and_then(|v| v.parse().ok()).ok_or("bad LUT_1D_SIZE")?,
            "DOMAIN_MIN" => dmin = three(&parts[1..]).ok_or("bad DOMAIN_MIN")?,
            "DOMAIN_MAX" => dmax = three(&parts[1..]).ok_or("bad DOMAIN_MAX")?,
            "LUT_3D_INPUT_RANGE" | "LUT_1D_INPUT_RANGE" => {
                if let (Some(a), Some(b)) = (parts.get(1).and_then(|v| v.parse::<f32>().ok()), parts.get(2).and_then(|v| v.parse::<f32>().ok())) {
                    dmin = [a; 3];
                    dmax = [b; 3];
                }
            }
            _ => {
                if let Some(v) = three(&parts) {
                    values.extend_from_slice(&v);
                } else if key.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
                    // Unknown keyword from some tool: ignore.
                } else {
                    return Err(format!("can't read the line “{line}”"));
                }
            }
        }
    }
    if size3 > 0 {
        if !(2..=256).contains(&size3) {
            return Err(format!("unusual LUT size {size3}"));
        }
        let need = size3 * size3 * size3 * 3;
        if values.len() < need {
            return Err(format!("the LUT has {} of {} colours", values.len() / 3, need / 3));
        }
        values.truncate(need);
        return Ok(Lut { title, size: size3, data: values, domain_min: dmin, domain_max: dmax });
    }
    if size1 > 0 {
        if values.len() < size1 * 3 || size1 < 2 {
            return Err("the 1D LUT is incomplete".into());
        }
        // Build a 33³ table from the three curves.
        let n = 33usize;
        let curve = |ch: usize, x: f32| -> f32 {
            let f = x.clamp(0.0, 1.0) * (size1 - 1) as f32;
            let i = (f.floor() as usize).min(size1 - 2);
            let t = f - i as f32;
            values[i * 3 + ch] * (1.0 - t) + values[(i + 1) * 3 + ch] * t
        };
        let mut data = Vec::with_capacity(n * n * n * 3);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let s = |k: usize| k as f32 / (n - 1) as f32;
                    data.push(curve(0, s(r)));
                    data.push(curve(1, s(g)));
                    data.push(curve(2, s(b)));
                }
            }
        }
        return Ok(Lut { title, size: n, data, domain_min: dmin, domain_max: dmax });
    }
    Err("not a .cube LUT (no LUT_3D_SIZE or LUT_1D_SIZE line)".into())
}

pub fn load(path: &std::path::Path) -> Result<Lut, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_cube(&String::from_utf8_lossy(&bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn identity_cube() {
        let mut t = String::from("TITLE \"id\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    t.push_str(&format!("{r} {g} {b}\n"));
                }
            }
        }
        let l = super::parse_cube(&t).unwrap();
        assert_eq!(l.size, 2);
        assert_eq!(&l.data[3..6], &[1.0, 0.0, 0.0]);
    }
}
