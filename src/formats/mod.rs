pub mod pmd;
pub mod pmx;
pub mod reader;
pub mod vmd;
pub mod vpd;
pub mod wav;
pub mod xfile;

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileKind {
    Model,
    Accessory,
    Motion,
    OwnMotion,
    Pose,
    Audio,
    Project,
    Image,
    Unknown,
}

pub fn kind_of(path: &Path) -> FileKind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pmx" | "pmd" => FileKind::Model,
        "x" => FileKind::Accessory,
        "vmd" => FileKind::Motion,
        "hmo" => FileKind::OwnMotion,
        "vpd" => FileKind::Pose,
        "wav" | "mp3" | "ogg" | "flac" | "m4a" => FileKind::Audio,
        "hproj" => FileKind::Project,
        "png" | "jpg" | "jpeg" | "bmp" | "tga" => FileKind::Image,
        _ => FileKind::Unknown,
    }
}
