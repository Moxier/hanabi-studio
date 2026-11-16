//! Colour grading settings (applied as the final pass on every frame and render).

use serde::{Deserialize, Serialize};

pub const LOOKS: &[&str] = &[
    "None",
    "Warm Film",
    "Cool Night",
    "Teal & Orange",
    "Pastel Dream",
    "Vivid Pop",
    "Faded Matte",
    "Mono",
    "Sepia",
    "Golden Hour",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ColorGrade {
    pub enabled: bool,
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub temperature: f32,
    pub tint: f32,
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
    pub shadows: f32,
    pub highlights: f32,
    pub look: usize,
    pub look_strength: f32,
    pub split_compare: bool,
    pub split_pos: f32,
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self {
            enabled: true,
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vibrance: 0.0,
            temperature: 0.0,
            tint: 0.0,
            lift: [0.0; 3],
            gamma: [0.0; 3],
            gain: [0.0; 3],
            shadows: 0.0,
            highlights: 0.0,
            look: 0,
            look_strength: 1.0,
            split_compare: false,
            split_pos: 0.5,
        }
    }
}

impl ColorGrade {
    pub fn presets() -> Vec<(&'static str, ColorGrade)> {
        let d = ColorGrade::default();
        vec![
            ("Neutral", d.clone()),
            ("Warm & Soft", ColorGrade { temperature: 0.25, contrast: 0.95, saturation: 1.05, look: 1, look_strength: 0.6, ..d.clone() }),
            ("Moonlight", ColorGrade { temperature: -0.35, exposure: -0.2, saturation: 0.8, look: 2, look_strength: 0.7, ..d.clone() }),
            ("Blockbuster", ColorGrade { contrast: 1.15, look: 3, look_strength: 0.8, vibrance: 0.2, ..d.clone() }),
            ("Pastel", ColorGrade { contrast: 0.85, look: 4, exposure: 0.15, look_strength: 0.8, ..d.clone() }),
            ("Pop", ColorGrade { saturation: 1.25, vibrance: 0.35, contrast: 1.1, look: 5, look_strength: 0.6, ..d.clone() }),
            ("Matte", ColorGrade { look: 6, contrast: 0.9, look_strength: 0.9, ..d.clone() }),
            ("Black & White", ColorGrade { look: 7, contrast: 1.15, ..d.clone() }),
            ("Golden Hour", ColorGrade { look: 9, temperature: 0.2, look_strength: 0.8, ..d }),
        ]
    }
}
