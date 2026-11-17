//! Physics tweaks per model: changed rigid bodies and joints, saved as a small preset file in
//! the settings folder and applied on top of the model's own values when it loads. The model
//! file itself is never changed. Presets are matched by the model file's contents, so a copy
//! of the same model elsewhere gets the same tweaks.

use crate::model::{Joint, RigidBody};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsTweaks {
    /// For people opening the file by hand.
    #[serde(default)]
    pub model_name: String,
    /// Changed rigid bodies by index (kept only if the name still matches).
    #[serde(default)]
    pub bodies: BTreeMap<usize, RigidBody>,
    #[serde(default)]
    pub joints: BTreeMap<usize, Joint>,
}

impl PhysicsTweaks {
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.joints.is_empty()
    }

    /// The model's own bodies and joints with these tweaks on top.
    pub fn apply(&self, bodies: &[RigidBody], joints: &[Joint]) -> (Vec<RigidBody>, Vec<Joint>) {
        let mut b = bodies.to_vec();
        for (&i, t) in &self.bodies {
            if let Some(x) = b.get_mut(i) {
                if x.name == t.name {
                    *x = t.clone();
                }
            }
        }
        let mut j = joints.to_vec();
        for (&i, t) in &self.joints {
            if let Some(x) = j.get_mut(i) {
                if x.name == t.name {
                    *x = t.clone();
                }
            }
        }
        (b, j)
    }
}

/// FNV-1a: a small, stable fingerprint of the model file (same file → same preset).
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn preset_dir() -> PathBuf {
    crate::ui::home::config_dir().join("physics-presets")
}

pub fn preset_path(key: &str) -> PathBuf {
    preset_dir().join(format!("{key}.json"))
}

pub fn load(key: &str) -> Option<PhysicsTweaks> {
    serde_json::from_str(&std::fs::read_to_string(preset_path(key)).ok()?).ok()
}

/// Save (or remove, when empty) the preset for a model.
pub fn save(key: &str, t: &PhysicsTweaks) -> Result<(), String> {
    let p = preset_path(key);
    if t.is_empty() {
        let _ = std::fs::remove_file(&p);
        return Ok(());
    }
    std::fs::create_dir_all(preset_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(t).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
}
