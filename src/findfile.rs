//! Find files that a model refers to, the way people actually have them on disk:
//! different letter case, Windows `\` paths, absolute paths from the author's PC, textures moved
//! into a sub-folder, and Japanese/Chinese names that an unzip tool turned into garbled
//! characters (Shift-JIS or GBK bytes shown as the old DOS code page 437).

use std::path::{Path, PathBuf};

/// Code page 437, bytes 0x80..=0xFF (public character table).
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Turn a garbled CP437 name back into the bytes the archive stored, if it looks like one.
fn cp437_bytes(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let mut high = false;
    for c in s.chars() {
        if (c as u32) < 0x80 {
            out.push(c as u8);
        } else {
            let i = CP437_HIGH.iter().position(|&h| h == c)?;
            out.push(0x80 + i as u8);
            high = true;
        }
    }
    high.then_some(out)
}

/// Every way a file name on disk might be meant, lower-cased.
pub fn readings(name: &std::ffi::OsStr) -> Vec<String> {
    let mut v = Vec::with_capacity(3);
    match name.to_str() {
        Some(s) => {
            v.push(s.to_lowercase());
            if let Some(b) = cp437_bytes(s) {
                push_decoded(&b, &mut v);
            }
        }
        None => {
            // Raw bytes that aren't UTF-8 (unzipped without any conversion).
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                push_decoded(name.as_bytes(), &mut v);
            }
            v.push(name.to_string_lossy().to_lowercase());
        }
    }
    v
}

/// The most likely intended spelling of a file name (for showing it to the person).
pub fn display_name(name: &std::ffi::OsStr) -> String {
    match name.to_str() {
        Some(s) => match cp437_bytes(s) {
            Some(b) => {
                let (d, _, bad) = encoding_rs::SHIFT_JIS.decode(&b);
                if bad { s.to_string() } else { d.into_owned() }
            }
            None => s.to_string(),
        },
        None => readings(name).into_iter().next().unwrap_or_default(),
    }
}

fn push_decoded(b: &[u8], v: &mut Vec<String>) {
    for enc in [encoding_rs::SHIFT_JIS, encoding_rs::GBK, encoding_rs::BIG5, encoding_rs::EUC_KR] {
        let (s, _, bad) = enc.decode(b);
        if !bad {
            v.push(s.to_lowercase());
        }
    }
}

fn find_in(dir: &Path, want: &str) -> Option<PathBuf> {
    let direct = dir.join(want);
    if direct.exists() {
        return Some(direct);
    }
    let want = want.to_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().find(|e| readings(&e.file_name()).iter().any(|r| *r == want)).map(|e| e.path())
}

fn find_rel(dir: &Path, rel: &str) -> Option<PathBuf> {
    let direct = dir.join(rel.replace('\\', "/"));
    if direct.is_file() {
        return Some(direct);
    }
    let mut cur = dir.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." {
            cur = cur.parent()?.to_path_buf();
            continue;
        }
        cur = find_in(&cur, part)?;
    }
    cur.is_file().then_some(cur)
}

fn search_down(dir: &Path, base: &str, depth: u32) -> Option<PathBuf> {
    if let Some(p) = find_in(dir, base).filter(|p| p.is_file()) {
        return Some(p);
    }
    if depth == 0 {
        return None;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subdirs.sort();
    subdirs.into_iter().find_map(|d| search_down(&d, base, depth - 1))
}

/// Find a file a model refers to, starting from the model's folder.
pub fn find_texture(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim().trim_matches('"');
    if rel.is_empty() {
        return None;
    }
    if let Some(p) = find_rel(dir, rel) {
        return Some(p);
    }
    // Absolute path from the author's PC, or a texture moved into another sub-folder:
    // look for the bare file name in the model folder and up to three levels below it.
    let base = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
    if base.is_empty() {
        return None;
    }
    search_down(dir, base, 3).or_else(|| {
        // Props kept in a sub-folder of a stage sometimes share the stage's textures.
        let parent = dir.parent()?;
        find_in(parent, base).filter(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn garbled_sjis_name_is_recognised() {
        let (b, _, _) = encoding_rs::SHIFT_JIS.encode("髪.png");
        let garbled: String = b.iter().map(|&c| if c < 0x80 { c as char } else { CP437_HIGH[(c - 0x80) as usize] }).collect();
        let r = readings(std::ffi::OsStr::new(&garbled));
        assert!(r.iter().any(|s| s == "髪.png"), "{r:?}");
    }
}
