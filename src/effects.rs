//! Node-based effects: graph data, node catalogue, presets and particle simulation.
//! The presets are original looks that cover popular community effect styles
//! (realistic lighting, polished toon, bright anime, beams & particles, water…).

use glam::{Vec3, Vec4};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    SceneRender,
    Output,
    Bloom,
    Tonemap,
    Vignette,
    ChromaticAberration,
    FilmGrain,
    Sharpen,
    Fxaa,
    DepthOfField,
    Ssao,
    LightShafts,
    Fog,
    Outline,
    Letterbox,
    Posterize,
    Mix,
    ShadingStyle,
    Particles,
    LightBeams,
    Water,
    Sky,
    MirrorFloor,
    RayLighting,
    ScreenReflections,
    AutoExposure,
    MotionBlur,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Socket {
    Image,
    Fx,
}

#[derive(Clone, Debug)]
pub enum PKind {
    Float(f32, f32, f32),
    Color([f32; 3]),
    Choice(&'static [&'static str], usize),
    Toggle(bool),
}

#[derive(Clone, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: PKind,
    pub help: &'static str,
}

const fn fl(name: &'static str, min: f32, max: f32, d: f32, help: &'static str) -> ParamSpec {
    ParamSpec { name, kind: PKind::Float(min, max, d), help }
}
const fn col(name: &'static str, c: [f32; 3], help: &'static str) -> ParamSpec {
    ParamSpec { name, kind: PKind::Color(c), help }
}
const fn ch(name: &'static str, o: &'static [&'static str], d: usize, help: &'static str) -> ParamSpec {
    ParamSpec { name, kind: PKind::Choice(o, d), help }
}
const fn tg(name: &'static str, d: bool, help: &'static str) -> ParamSpec {
    ParamSpec { name, kind: PKind::Toggle(d), help }
}

pub const STYLES: &[&str] = &["Classic toon", "Polished toon", "Realistic light", "Bright anime"];
pub const PARTICLE_KINDS: &[&str] = &["Sparkles", "Snow", "Petals", "Embers", "Bubbles", "Confetti", "Fireflies"];
pub const TONEMAPS: &[&str] = &["None", "Filmic", "ACES-like", "Soft (Reinhard)", "Punchy"];
pub const MIX_MODES: &[&str] = &["Blend", "Add", "Screen", "Multiply"];

pub struct NodeInfo {
    pub title: &'static str,
    pub category: &'static str,
    pub about: &'static str,
    pub inputs: Vec<(&'static str, Socket)>,
    pub output: Option<Socket>,
    pub params: Vec<ParamSpec>,
}

impl NodeKind {
    pub const ALL: &'static [NodeKind] = &[
        NodeKind::SceneRender,
        NodeKind::Output,
        NodeKind::RayLighting,
        NodeKind::ShadingStyle,
        NodeKind::Particles,
        NodeKind::LightBeams,
        NodeKind::Water,
        NodeKind::MirrorFloor,
        NodeKind::Sky,
        NodeKind::Bloom,
        NodeKind::Tonemap,
        NodeKind::Ssao,
        NodeKind::ScreenReflections,
        NodeKind::AutoExposure,
        NodeKind::MotionBlur,
        NodeKind::DepthOfField,
        NodeKind::LightShafts,
        NodeKind::Fog,
        NodeKind::Outline,
        NodeKind::Vignette,
        NodeKind::ChromaticAberration,
        NodeKind::FilmGrain,
        NodeKind::Sharpen,
        NodeKind::Fxaa,
        NodeKind::Posterize,
        NodeKind::Letterbox,
        NodeKind::Mix,
    ];

    pub fn info(self) -> NodeInfo {
        use NodeKind::*;
        let img = || vec![("Image", Socket::Image)];
        let (title, category, about, inputs, output, params): (_, _, _, Vec<(&'static str, Socket)>, _, Vec<ParamSpec>) = match self {
            SceneRender => (
                "Scene Render",
                "Source",
                "Draws the stage: models, floor, shadows. Plug look nodes into Scene FX.",
                vec![("Scene FX", Socket::Fx)],
                Some(Socket::Image),
                vec![],
            ),
            Output => ("Output", "Source", "What you see and what gets rendered.", img(), None, vec![]),
            ShadingStyle => (
                "Shading Style",
                "Scene look",
                "How characters are lit: toon, polished toon, realistic or bright anime.",
                vec![],
                Some(Socket::Fx),
                vec![
                    ch("Style", STYLES, 1, "Overall material lighting model"),
                    fl("Rim light", 0.0, 2.0, 0.5, "Bright edge around silhouettes"),
                    col("Rim colour", [1.0, 0.95, 0.9], ""),
                    fl("Specular boost", 0.0, 3.0, 1.0, "Shine strength"),
                    fl("Shadow softness", 0.0, 1.0, 0.4, "Hard to soft toon edge"),
                    col("Shadow tint", [0.72, 0.62, 0.78], "Colour of shaded areas"),
                    fl("Skin glow", 0.0, 1.0, 0.25, "Subsurface-like warmth"),
                    fl("Saturation", 0.0, 2.0, 1.0, ""),
                    fl("Ambient light", 0.0, 2.0, 1.0, ""),
                ],
            ),
            RayLighting => (
                "Ray Lighting",
                "Scene look",
                "Physically based lighting: real light energy, sky light from all directions, metal / skin / cloth / glass materials, soft skin scattering. Pair with Ambient Occlusion, Screen Reflections, Auto Exposure and Tone Map.",
                vec![],
                Some(Socket::Fx),
                vec![
                    fl("Sun strength", 0.0, 8.0, 3.0, "Brightness of the main light"),
                    fl("Sky light", 0.0, 4.0, 1.0, "Light coming from the sky around the scene"),
                    col("Sky colour", [0.55, 0.68, 0.95], "Used when there is no Sky Gradient node"),
                    col("Ground colour", [0.35, 0.3, 0.27], "Light bounced from the floor"),
                    fl("Skin scattering", 0.0, 1.0, 0.6, "Soft, warm light through skin"),
                    fl("Roughness", 0.05, 1.0, 0.55, "Default surface roughness for cloth and unknown materials"),
                    fl("Specular", 0.0, 2.0, 1.0, "Strength of reflections and highlights"),
                    fl("Emissive", 0.0, 10.0, 2.0, "Brightness of glowing materials"),
                    tg("Auto materials", true, "Guess skin, hair, eyes, metal and glass from material names"),
                ],
            ),
            ScreenReflections => (
                "Screen Reflections",
                "Light",
                "Reflections of what is on screen in floors, water, metal and glossy surfaces.",
                img(),
                Some(Socket::Image),
                vec![
                    fl("Strength", 0.0, 2.0, 1.0, ""),
                    fl("Max distance", 5.0, 300.0, 80.0, "How far rays travel"),
                    fl("Thickness", 0.1, 10.0, 1.5, "How thick objects are assumed to be"),
                    fl("Roughness cut-off", 0.1, 1.0, 0.7, "Surfaces rougher than this reflect nothing"),
                ],
            ),
            AutoExposure => (
                "Auto Exposure",
                "Camera",
                "Adapts brightness like an eye or camera when the scene gets darker or brighter.",
                img(),
                Some(Socket::Image),
                vec![
                    fl("Target brightness", 0.1, 1.0, 0.45, "Average brightness to aim for"),
                    fl("Speed", 0.1, 10.0, 1.5, "How quickly it adapts"),
                    fl("Min exposure", -4.0, 0.0, -2.0, "In stops"),
                    fl("Max exposure", 0.0, 4.0, 2.0, "In stops"),
                ],
            ),
            MotionBlur => (
                "Motion Blur",
                "Camera",
                "Blur from camera movement, like a real shutter.",
                img(),
                Some(Socket::Image),
                vec![fl("Shutter", 0.0, 2.0, 0.5, "Amount of blur"), fl("Samples", 4.0, 24.0, 12.0, "")],
            ),
            Particles => (
                "Particles",
                "Scene look",
                "GPU-drawn particles: sparkles, snow, petals, embers, bubbles, confetti, fireflies.",
                vec![],
                Some(Socket::Fx),
                vec![
                    ch("Kind", PARTICLE_KINDS, 0, ""),
                    fl("Amount", 100.0, 60000.0, 1500.0, "Number of particles alive"),
                    fl("Size", 0.02, 2.0, 0.18, ""),
                    fl("Speed", 0.0, 4.0, 1.0, ""),
                    fl("Area", 2.0, 120.0, 30.0, "Width of the emitting area"),
                    fl("Height", 0.0, 80.0, 25.0, ""),
                    col("Colour", [1.0, 0.85, 0.55], ""),
                    fl("Glow", 0.0, 6.0, 1.2, "Brightness (feeds Bloom)"),
                    fl("Wind", -4.0, 4.0, 0.3, ""),
                ],
            ),
            LightBeams => (
                "Light Beams",
                "Scene look",
                "Sweeping stage beams and lasers from above or below.",
                vec![],
                Some(Socket::Fx),
                vec![
                    fl("Count", 1.0, 32.0, 8.0, ""),
                    col("Colour", [0.5, 0.75, 1.0], ""),
                    fl("Length", 5.0, 200.0, 80.0, ""),
                    fl("Width", 0.05, 6.0, 1.2, ""),
                    fl("Sweep speed", 0.0, 4.0, 0.6, ""),
                    fl("Spread", 0.0, 1.5, 0.6, "How far beams swing"),
                    fl("Brightness", 0.0, 8.0, 1.6, ""),
                    fl("Origin height", -10.0, 80.0, 40.0, ""),
                    ch("Pattern", &["Fan from above", "Fan from floor", "Laser grid", "Spot circle"], 0, ""),
                    tg("Rainbow", false, "Cycle beam colours"),
                ],
            ),
            Water => (
                "Water Surface",
                "Scene look",
                "Animated water with reflections, ripples and shine.",
                vec![],
                Some(Socket::Fx),
                vec![
                    fl("Level", -20.0, 20.0, 0.0, "Height of the water"),
                    col("Deep colour", [0.02, 0.12, 0.2], ""),
                    col("Shallow colour", [0.1, 0.45, 0.55], ""),
                    fl("Wave size", 0.1, 4.0, 1.0, ""),
                    fl("Wave speed", 0.0, 4.0, 1.0, ""),
                    fl("Reflection", 0.0, 1.0, 0.75, ""),
                    fl("Clarity", 0.0, 1.0, 0.35, "How see-through"),
                    fl("Size", 20.0, 600.0, 200.0, ""),
                    fl("Sparkle", 0.0, 4.0, 1.0, "Sun glints"),
                ],
            ),
            MirrorFloor => (
                "Mirror Floor",
                "Scene look",
                "Glossy stage floor that reflects the dancers.",
                vec![],
                Some(Socket::Fx),
                vec![fl("Reflection", 0.0, 1.0, 0.45, ""), fl("Blur", 0.0, 1.0, 0.3, ""), col("Tint", [0.8, 0.85, 1.0], "")],
            ),
            Sky => (
                "Sky Gradient",
                "Scene look",
                "Background gradient with optional stars.",
                vec![],
                Some(Socket::Fx),
                vec![
                    col("Top", [0.05, 0.07, 0.16], ""),
                    col("Horizon", [0.35, 0.3, 0.45], ""),
                    col("Bottom", [0.06, 0.05, 0.08], ""),
                    fl("Stars", 0.0, 1.0, 0.0, ""),
                ],
            ),
            Bloom => (
                "Bloom",
                "Light",
                "Soft glow around bright areas.",
                img(),
                Some(Socket::Image),
                vec![fl("Threshold", 0.0, 2.0, 0.8, ""), fl("Intensity", 0.0, 4.0, 0.8, ""), fl("Radius", 0.2, 2.0, 1.0, "")],
            ),
            Tonemap => (
                "Tone Map",
                "Colour",
                "Maps bright light to screen colour, like a camera.",
                img(),
                Some(Socket::Image),
                vec![ch("Curve", TONEMAPS, 1, ""), fl("Exposure", -3.0, 3.0, 0.0, "In stops")],
            ),
            Ssao => (
                "Ambient Occlusion",
                "Light",
                "Contact shadows in creases and where objects meet.",
                img(),
                Some(Socket::Image),
                vec![fl("Radius", 0.1, 6.0, 1.5, ""), fl("Strength", 0.0, 3.0, 1.0, "")],
            ),
            DepthOfField => (
                "Depth of Field",
                "Camera",
                "Lens blur like a real camera: set the f-stop and lens, with round or bladed bokeh and glowing highlights.",
                img(),
                Some(Socket::Image),
                vec![
                    tg("Auto focus", true, "Keep the camera target sharp"),
                    fl("Focus distance", 1.0, 300.0, 42.0, "Used when auto focus is off"),
                    fl("F-stop", 0.95, 22.0, 2.0, "Smaller number = more blur"),
                    fl("Lens (mm)", 14.0, 200.0, 85.0, "Longer lens = more blur"),
                    ch("Bokeh shape", &["Circle", "Hexagon", "Octagon"], 1, "Shape of out-of-focus highlights"),
                    fl("Bokeh brightness", 0.0, 4.0, 1.5, "How strongly bright spots bloom into discs"),
                    fl("Max blur", 2.0, 40.0, 18.0, "Largest blur in pixels"),
                ],
            ),
            LightShafts => (
                "Light Shafts",
                "Light",
                "God rays streaming from bright areas toward the camera.",
                img(),
                Some(Socket::Image),
                vec![
                    fl("Strength", 0.0, 3.0, 0.6, ""),
                    fl("Length", 0.1, 1.0, 0.7, ""),
                    fl("Threshold", 0.0, 2.0, 0.7, ""),
                    col("Colour", [1.0, 0.95, 0.85], ""),
                    fl("Source X", 0.0, 1.0, 0.5, ""),
                    fl("Source Y", 0.0, 1.0, 0.92, ""),
                ],
            ),
            Fog => (
                "Fog",
                "Light",
                "Atmospheric haze by distance.",
                img(),
                Some(Socket::Image),
                vec![col("Colour", [0.6, 0.65, 0.75], ""), fl("Start", 0.0, 300.0, 40.0, ""), fl("Density", 0.0, 1.0, 0.25, "")],
            ),
            Outline => (
                "Screen Outline",
                "Stylise",
                "Ink lines found from depth edges (on top of model edges).",
                img(),
                Some(Socket::Image),
                vec![fl("Thickness", 0.5, 4.0, 1.0, ""), fl("Strength", 0.0, 1.0, 0.7, ""), col("Colour", [0.05, 0.03, 0.08], "")],
            ),
            Vignette => (
                "Vignette",
                "Camera",
                "Darkens the corners.",
                img(),
                Some(Socket::Image),
                vec![fl("Amount", 0.0, 1.0, 0.35, ""), fl("Softness", 0.05, 1.0, 0.5, ""), col("Colour", [0.0, 0.0, 0.0], "")],
            ),
            ChromaticAberration => (
                "Colour Fringe",
                "Camera",
                "Lens-style red/blue fringes at the edges.",
                img(),
                Some(Socket::Image),
                vec![fl("Amount", 0.0, 3.0, 0.6, "")],
            ),
            FilmGrain => (
                "Film Grain",
                "Camera",
                "Moving grain like real film.",
                img(),
                Some(Socket::Image),
                vec![fl("Amount", 0.0, 0.3, 0.05, ""), fl("Size", 0.5, 4.0, 1.4, "")],
            ),
            Sharpen => ("Sharpen", "Stylise", "Crisper detail.", img(), Some(Socket::Image), vec![fl("Amount", 0.0, 2.0, 0.4, "")]),
            Fxaa => ("Smooth Edges", "Camera", "Fast anti-aliasing for jagged lines.", img(), Some(Socket::Image), vec![]),
            Posterize => (
                "Posterize",
                "Stylise",
                "Fewer colour steps for a graphic look.",
                img(),
                Some(Socket::Image),
                vec![fl("Levels", 2.0, 32.0, 8.0, ""), fl("Mix", 0.0, 1.0, 1.0, "")],
            ),
            Letterbox => (
                "Letterbox",
                "Camera",
                "Cinematic black bars.",
                img(),
                Some(Socket::Image),
                vec![fl("Aspect", 1.0, 3.0, 2.39, "Width / height of the picture")],
            ),
            Mix => (
                "Mix",
                "Utility",
                "Combine two images.",
                vec![("A", Socket::Image), ("B", Socket::Image)],
                Some(Socket::Image),
                vec![ch("Mode", MIX_MODES, 0, ""), fl("Factor", 0.0, 1.0, 0.5, "")],
            ),
        };
        NodeInfo { title, category, about, inputs, output, params }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PVal {
    F(f32),
    C([f32; 3]),
    I(usize),
    B(bool),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub kind: NodeKind,
    pub pos: [f32; 2],
    pub values: Vec<PVal>,
    pub enabled: bool,
}

impl Node {
    pub fn new(id: u32, kind: NodeKind, pos: [f32; 2]) -> Self {
        let values = kind
            .info()
            .params
            .iter()
            .map(|p| match p.kind {
                PKind::Float(_, _, d) => PVal::F(d),
                PKind::Color(c) => PVal::C(c),
                PKind::Choice(_, d) => PVal::I(d),
                PKind::Toggle(b) => PVal::B(b),
            })
            .collect();
        Self { id, kind, pos, values, enabled: true }
    }
    fn idx(&self, name: &str) -> Option<usize> {
        self.kind.info().params.iter().position(|p| p.name == name)
    }
    pub fn f(&self, name: &str) -> f32 {
        match self.idx(name).and_then(|i| self.values.get(i)) {
            Some(PVal::F(v)) => *v,
            Some(PVal::I(v)) => *v as f32,
            Some(PVal::B(b)) => *b as i32 as f32,
            _ => 0.0,
        }
    }
    pub fn c(&self, name: &str) -> Vec3 {
        match self.idx(name).and_then(|i| self.values.get(i)) {
            Some(PVal::C(c)) => Vec3::from(*c),
            _ => Vec3::ONE,
        }
    }
    pub fn i(&self, name: &str) -> usize {
        match self.idx(name).and_then(|i| self.values.get(i)) {
            Some(PVal::I(v)) => *v,
            _ => 0,
        }
    }
    pub fn b(&self, name: &str) -> bool {
        matches!(self.idx(name).and_then(|i| self.values.get(i)), Some(PVal::B(true)))
    }
    pub fn set(&mut self, name: &str, v: PVal) -> bool {
        if let Some(i) = self.idx(name) {
            if i < self.values.len() {
                self.values[i] = v;
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub from: u32,
    pub to: u32,
    pub slot: usize,
}

/// A named box of nodes in the editor (like a compound node). Collapsed groups draw as one tile.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeGroup {
    pub id: u32,
    pub name: String,
    pub nodes: Vec<u32>,
    pub color: [u8; 3],
    pub collapsed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectGraph {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub next_id: u32,
    pub preset_name: String,
    #[serde(default)]
    pub groups: Vec<NodeGroup>,
    /// Material type chosen per material name for Ray Lighting (missing = automatic).
    #[serde(default)]
    pub materials: std::collections::BTreeMap<String, MatPreset>,
    /// Look chosen per model (by model name); missing = follow the scene look.
    #[serde(default)]
    pub model_looks: std::collections::BTreeMap<String, ModelLook>,
    /// Full material settings per material name (maps, special kinds, node graph).
    #[serde(default)]
    pub mat_fx: std::collections::BTreeMap<String, crate::material::MaterialFx>,
    /// Extra lights placed in the scene.
    #[serde(default)]
    pub lights: Vec<crate::material::SceneLight>,
}

/// How one model is shaded, overriding the scene look (assigned per object, like effect assignment).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelLook {
    ClassicToon,
    PolishedToon,
    BrightAnime,
    Realistic,
}

impl ModelLook {
    pub const ALL: [ModelLook; 4] = [ModelLook::ClassicToon, ModelLook::PolishedToon, ModelLook::BrightAnime, ModelLook::Realistic];
    pub fn label(self) -> &'static str {
        match self {
            ModelLook::ClassicToon => "Classic toon",
            ModelLook::PolishedToon => "Polished toon",
            ModelLook::BrightAnime => "Bright anime",
            ModelLook::Realistic => "Realistic (Ray Lighting)",
        }
    }
    /// Shader style index.
    pub fn style(self) -> i32 {
        match self {
            ModelLook::ClassicToon => 0,
            ModelLook::PolishedToon => 1,
            ModelLook::BrightAnime => 3,
            ModelLook::Realistic => 4,
        }
    }
}

/// Physically based material types (Ray Lighting).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatPreset {
    Cloth,
    Skin,
    Hair,
    Eyes,
    Plastic,
    Leather,
    Metal,
    Gold,
    Glass,
    Emissive,
    Toon,
}

/// Roughness, metalness, skin scattering, emissive and specular scale of one material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatParams {
    pub rough: f32,
    pub metal: f32,
    pub sss: f32,
    pub emissive: f32,
    pub spec: f32,
    pub toon: bool,
}

impl Default for MatParams {
    fn default() -> Self {
        MatPreset::Cloth.params(0.55)
    }
}

impl MatPreset {
    pub const ALL: [MatPreset; 11] = [
        MatPreset::Cloth,
        MatPreset::Skin,
        MatPreset::Hair,
        MatPreset::Eyes,
        MatPreset::Plastic,
        MatPreset::Leather,
        MatPreset::Metal,
        MatPreset::Gold,
        MatPreset::Glass,
        MatPreset::Emissive,
        MatPreset::Toon,
    ];
    pub fn label(self) -> &'static str {
        match self {
            MatPreset::Cloth => "Cloth",
            MatPreset::Skin => "Skin",
            MatPreset::Hair => "Hair",
            MatPreset::Eyes => "Eyes (glossy)",
            MatPreset::Plastic => "Plastic",
            MatPreset::Leather => "Leather",
            MatPreset::Metal => "Metal",
            MatPreset::Gold => "Gold",
            MatPreset::Glass => "Glass / gem",
            MatPreset::Emissive => "Glowing",
            MatPreset::Toon => "Keep toon look",
        }
    }
    pub fn params(self, default_rough: f32) -> MatParams {
        let p = |rough, metal, sss, emissive, spec| MatParams { rough, metal, sss, emissive, spec, toon: false };
        match self {
            MatPreset::Cloth => p(default_rough, 0.0, 0.0, 0.0, 0.6),
            MatPreset::Skin => p(0.5, 0.0, 1.0, 0.0, 0.7),
            MatPreset::Hair => p(0.35, 0.0, 0.15, 0.0, 1.0),
            MatPreset::Eyes => p(0.08, 0.0, 0.0, 0.0, 1.2),
            MatPreset::Plastic => p(0.3, 0.0, 0.0, 0.0, 1.0),
            MatPreset::Leather => p(0.45, 0.0, 0.0, 0.0, 0.9),
            MatPreset::Metal => p(0.25, 1.0, 0.0, 0.0, 1.0),
            MatPreset::Gold => p(0.2, 1.0, 0.0, 0.0, 1.0),
            MatPreset::Glass => p(0.03, 0.0, 0.0, 0.0, 1.5),
            MatPreset::Emissive => p(0.5, 0.0, 0.0, 1.0, 0.5),
            MatPreset::Toon => MatParams { rough: 0.6, metal: 0.0, sss: 0.0, emissive: 0.0, spec: 0.5, toon: true },
        }
    }
    /// Guess a material type from its (Japanese or English) name.
    pub fn guess(name: &str) -> MatPreset {
        let n = name.to_lowercase();
        let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
        if has(&["目", "瞳", "eye", "iris", "pupil", "ハイライト"]) {
            MatPreset::Eyes
        } else if has(&["肌", "顔", "skin", "face", "body", "首", "手", "腕", "脚", "足", "mouth", "口", "舌", "tongue", "nail", "爪"]) {
            MatPreset::Skin
        } else if has(&["髪", "hair", "もみあげ", "ponytail", "bangs"]) {
            MatPreset::Hair
        } else if has(&["金", "gold", "golden"]) {
            MatPreset::Gold
        } else if has(&["metal", "銀", "silver", "steel", "chrome", "iron", "buckle"]) {
            MatPreset::Metal
        } else if has(&["glass", "ガラス", "lens", "レンズ", "gem", "宝石", "crystal", "jewel"]) {
            MatPreset::Glass
        } else if has(&["emit", "glow", "発光", "led", "neon"]) {
            MatPreset::Emissive
        } else if has(&["leather", "革", "belt", "boot", "靴", "shoe"]) {
            MatPreset::Leather
        } else if has(&["plastic", "ribbon", "リボン", "button", "ボタン"]) {
            MatPreset::Plastic
        } else {
            MatPreset::Cloth
        }
    }
}

/// Scene-look nodes the renderer uses only once; a preset updates the existing one instead of adding.
fn single_instance(k: NodeKind) -> bool {
    matches!(k, NodeKind::ShadingStyle | NodeKind::Sky | NodeKind::Water | NodeKind::MirrorFloor | NodeKind::RayLighting)
}

const GROUP_COLORS: [[u8; 3]; 6] = [[92, 196, 255], [255, 106, 146], [255, 196, 84], [120, 210, 140], [180, 140, 255], [255, 150, 90]];

/// One post-processing step after evaluation.
#[derive(Clone, Debug)]
pub struct Pass {
    pub node: usize,
    /// Indices of earlier passes used as inputs; usize::MAX means "scene".
    pub inputs: Vec<usize>,
}

pub const SCENE: usize = usize::MAX;

impl EffectGraph {
    pub fn empty() -> Self {
        let mut g = Self { nodes: vec![], links: vec![], next_id: 1, preset_name: "Custom".into(), groups: vec![], materials: Default::default(), model_looks: Default::default(), mat_fx: Default::default(), lights: Vec::new() };
        let s = g.add(NodeKind::SceneRender, [40.0, 160.0]);
        let o = g.add(NodeKind::Output, [720.0, 160.0]);
        g.connect(s, o, 0);
        g
    }

    pub fn add(&mut self, kind: NodeKind, pos: [f32; 2]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(Node::new(id, kind, pos));
        id
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn index(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    pub fn connect(&mut self, from: u32, to: u32, slot: usize) {
        let (Some(f), Some(t)) = (self.node(from), self.node(to)) else { return };
        let out = f.kind.info().output;
        let ins = t.kind.info().inputs;
        let Some(&(_, sock)) = ins.get(slot) else { return };
        if out != Some(sock) || from == to {
            return;
        }
        if sock == Socket::Image {
            self.links.retain(|l| !(l.to == to && l.slot == slot));
            // Prevent cycles: refuse if `to` already feeds `from`.
            if self.reaches(to, from) {
                return;
            }
        } else if self.links.iter().any(|l| l.from == from && l.to == to) {
            return;
        }
        self.links.push(Link { from, to, slot });
    }

    fn reaches(&self, a: u32, b: u32) -> bool {
        let mut stack = vec![a];
        let mut seen = vec![];
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if seen.contains(&n) {
                continue;
            }
            seen.push(n);
            for l in self.links.iter().filter(|l| l.from == n) {
                stack.push(l.to);
            }
        }
        false
    }

    pub fn remove(&mut self, id: u32) {
        if let Some(n) = self.node(id) {
            if matches!(n.kind, NodeKind::SceneRender | NodeKind::Output) {
                return;
            }
        }
        // Reconnect around a removed image node so the chain keeps working.
        let src = self.links.iter().find(|l| l.to == id && l.slot == 0).map(|l| l.from);
        let outs: Vec<(u32, usize)> = self.links.iter().filter(|l| l.from == id).map(|l| (l.to, l.slot)).collect();
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
        if let Some(s) = src {
            for (t, slot) in outs {
                self.connect(s, t, slot);
            }
        }
        self.tidy_groups();
    }

    /// Insert an image node just before Output.
    pub fn append_before_output(&mut self, kind: NodeKind) -> u32 {
        let out = self.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| n.id);
        let prev = out.and_then(|o| self.links.iter().find(|l| l.to == o).map(|l| l.from));
        let pos = out.and_then(|o| self.node(o)).map(|n| n.pos).unwrap_or([400.0, 160.0]);
        if let Some(o) = out.and_then(|o| self.node_mut(o)) {
            o.pos[0] += 190.0;
        }
        let id = self.add(kind, pos);
        if kind.info().output == Some(Socket::Fx) {
            if let Some(s) = self.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| n.id) {
                if let Some(n) = self.node_mut(id) {
                    n.pos = [pos[0] - 300.0, pos[1] + 140.0];
                }
                if let Some(o) = out.and_then(|o| self.node_mut(o)) {
                    o.pos[0] -= 190.0;
                }
                self.connect(id, s, 0);
            }
            return id;
        }
        if let (Some(o), Some(p)) = (out, prev) {
            self.connect(p, id, 0);
            self.connect(id, o, 0);
        }
        id
    }

    /// Group nodes (removing them from any other group). Returns the new group's id.
    pub fn group(&mut self, ids: &[u32], name: &str) -> Option<u32> {
        let ids: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| self.node(*id).map(|n| !matches!(n.kind, NodeKind::SceneRender | NodeKind::Output)).unwrap_or(false))
            .collect();
        if ids.is_empty() {
            return None;
        }
        for g in &mut self.groups {
            g.nodes.retain(|n| !ids.contains(n));
        }
        self.groups.retain(|g| !g.nodes.is_empty());
        let id = self.next_id;
        self.next_id += 1;
        let color = GROUP_COLORS[self.groups.len() % GROUP_COLORS.len()];
        self.groups.push(NodeGroup { id, name: name.to_string(), nodes: ids, color, collapsed: false });
        Some(id)
    }

    pub fn ungroup(&mut self, group_id: u32) {
        self.groups.retain(|g| g.id != group_id);
    }

    pub fn group_of(&self, node: u32) -> Option<&NodeGroup> {
        self.groups.iter().find(|g| g.nodes.contains(&node))
    }

    fn tidy_groups(&mut self) {
        let ids: Vec<u32> = self.nodes.iter().map(|n| n.id).collect();
        for g in &mut self.groups {
            g.nodes.retain(|n| ids.contains(n));
        }
        self.groups.retain(|g| !g.nodes.is_empty());
    }

    /// Add a preset's nodes into this graph (instead of replacing it): its looks plug into the
    /// existing Scene Render, its image chain is inserted before Output, and the new nodes are
    /// grouped under the preset's name. One-of-a-kind looks (shading, sky, water, mirror floor)
    /// update the node that is already there. Returns the ids of the added nodes.
    pub fn merge_preset(&mut self, preset: &EffectGraph, name: &str) -> Vec<u32> {
        let Some(scene) = self.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| (n.id, n.pos)) else {
            *self = preset.clone();
            return self.nodes.iter().map(|n| n.id).collect();
        };
        let Some(out) = self.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| (n.id, n.pos)) else { return vec![] };
        let p_scene = preset.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| n.id);
        let p_out = preset.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| n.id);
        let mut map: Vec<(u32, u32)> = Vec::new();
        if let Some(ps) = p_scene {
            map.push((ps, scene.0));
        }
        if let Some(po) = p_out {
            map.push((po, out.0));
        }
        let mut added = Vec::new();
        // Image chain of the preset, in order from its Scene Render to its Output.
        let mut chain = Vec::new();
        let mut cur = p_scene;
        while let Some(c) = cur {
            let next = preset.links.iter().find(|l| l.from == c && preset.node(l.to).map(|n| n.kind.info().output == Some(Socket::Image) || n.kind == NodeKind::Output).unwrap_or(false)).map(|l| l.to);
            match next {
                Some(n) if Some(n) != p_out && !chain.contains(&n) => {
                    chain.push(n);
                    cur = Some(n);
                }
                _ => break,
            }
        }
        // The preset's nodes go in a new row under everything that's there: looks first, then
        // its image chain, so the preset stays together as one tidy group.
        let row_y = self.nodes.iter().filter(|n| n.kind != NodeKind::Output).map(|n| n.pos[1]).fold(f32::MIN, f32::max) + 170.0;
        let mut x = scene.1[0];
        for pn in preset.nodes.iter().filter(|n| n.kind.info().output == Some(Socket::Fx)) {
            if single_instance(pn.kind) {
                let existing = self.links.iter().filter(|l| l.to == scene.0).filter_map(|l| self.node(l.from)).find(|n| n.kind == pn.kind).map(|n| n.id);
                if let Some(e) = existing {
                    if let Some(n) = self.node_mut(e) {
                        n.values = pn.values.clone();
                        n.enabled = true;
                    }
                    map.push((pn.id, e));
                    continue;
                }
            }
            let id = self.add(pn.kind, [x, row_y]);
            x += 200.0;
            if let Some(n) = self.node_mut(id) {
                n.values = pn.values.clone();
                n.enabled = pn.enabled;
            }
            map.push((pn.id, id));
            added.push(id);
        }
        // Image nodes: inserted before Output, which moves to the end of the new row.
        let prev = self.links.iter().find(|l| l.to == out.0 && l.slot == 0).map(|l| l.from).unwrap_or(scene.0);
        for &pid in &chain {
            let Some(pn) = preset.node(pid) else { continue };
            let id = self.add(pn.kind, [x, row_y]);
            x += 200.0;
            if let Some(n) = self.node_mut(id) {
                n.values = pn.values.clone();
                n.enabled = pn.enabled;
            }
            map.push((pid, id));
            added.push(id);
        }
        // Other preset nodes not on the main chain (e.g. Mix branches).
        for pn in &preset.nodes {
            if map.iter().any(|(a, _)| *a == pn.id) {
                continue;
            }
            let id = self.add(pn.kind, [x, row_y + 110.0]);
            x += 200.0;
            if let Some(n) = self.node_mut(id) {
                n.values = pn.values.clone();
            }
            map.push((pn.id, id));
            added.push(id);
        }
        if let Some(o) = self.node_mut(out.0) {
            o.pos = [x + 20.0, row_y];
        }
        let m = |id: u32| map.iter().find(|(a, _)| *a == id).map(|(_, b)| *b);
        // Re-link: the old last node feeds the first new chain node.
        if let Some(&first) = chain.first() {
            if let Some(f) = m(first) {
                self.connect(prev, f, 0);
            }
        }
        for l in &preset.links {
            let (Some(a), Some(b)) = (m(l.from), m(l.to)) else { continue };
            // Preset's "scene → first" link is replaced by "previous chain end → first".
            if Some(l.from) == p_scene && chain.first() == Some(&l.to) {
                continue;
            }
            if Some(l.from) == p_scene && Some(l.to) == p_out {
                continue;
            }
            self.connect(a, b, l.slot);
        }
        if !added.is_empty() {
            self.group(&added, name);
        }
        self.preset_name = if self.nodes.len() <= added.len() + 2 {
            name.to_string()
        } else {
            format!("{} + {}", self.preset_name.trim_end_matches(" (edited)"), name)
        };
        added
    }

    /// Image nodes from Scene Render to Output along the main path, in order.
    pub fn chain_ids(&self) -> Vec<u32> {
        let Some(out) = self.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| n.id) else { return vec![] };
        let mut v = Vec::new();
        let mut cur = out;
        for _ in 0..256 {
            let Some(src) = self.links.iter().find(|l| l.to == cur && l.slot == 0).map(|l| l.from) else { break };
            match self.node(src).map(|n| n.kind) {
                Some(NodeKind::SceneRender) | None => break,
                _ => {
                    if v.contains(&src) {
                        break;
                    }
                    v.push(src);
                    cur = src;
                }
            }
        }
        v.reverse();
        v
    }

    /// Relink the main path so it runs Scene Render → ids… → Output.
    pub fn set_chain(&mut self, ids: &[u32]) {
        let (Some(scene), Some(out)) = (
            self.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| n.id),
            self.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| n.id),
        ) else {
            return;
        };
        let old = self.chain_ids();
        let mut members: Vec<u32> = old.clone();
        members.push(out);
        self.links.retain(|l| !(l.slot == 0 && members.contains(&l.to)));
        let mut prev = scene;
        for &id in ids {
            self.links.push(Link { from: prev, to: id, slot: 0 });
            prev = id;
        }
        self.links.push(Link { from: prev, to: out, slot: 0 });
        // tidy the layout: one row left to right
        let y = self.node(scene).map(|n| n.pos[1]).unwrap_or(120.0);
        let mut x = self.node(scene).map(|n| n.pos[0]).unwrap_or(40.0) + 230.0;
        for &id in ids.iter().chain(std::iter::once(&out)) {
            if let Some(n) = self.node_mut(id) {
                n.pos = [x, y];
            }
            x += 200.0;
        }
    }

    /// Scene-look nodes linked into Scene Render (enabled or not), in link order.
    pub fn fx_ids(&self) -> Vec<u32> {
        let Some(scene) = self.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| n.id) else { return vec![] };
        self.links.iter().filter(|l| l.to == scene).map(|l| l.from).collect()
    }

    /// Scene-look nodes plugged into Scene Render (and enabled).
    pub fn active_fx(&self) -> Vec<&Node> {
        let Some(scene) = self.nodes.iter().find(|n| n.kind == NodeKind::SceneRender) else { return vec![] };
        self.links
            .iter()
            .filter(|l| l.to == scene.id)
            .filter_map(|l| self.node(l.from))
            .filter(|n| n.enabled)
            .collect()
    }

    pub fn fx_of(&self, kind: NodeKind) -> Option<&Node> {
        self.active_fx().into_iter().find(|n| n.kind == kind)
    }

    /// Order the image passes needed to produce the Output.
    pub fn plan(&self) -> Vec<Pass> {
        let mut passes = Vec::new();
        let mut memo: Vec<(u32, usize)> = Vec::new();
        let Some(out) = self.nodes.iter().find(|n| n.kind == NodeKind::Output) else { return passes };
        if let Some(l) = self.links.iter().find(|l| l.to == out.id && l.slot == 0) {
            let r = self.visit(l.from, &mut passes, &mut memo, 0);
            if r != SCENE {
                // final result is the last pass already
            }
        }
        passes
    }

    fn visit(&self, id: u32, passes: &mut Vec<Pass>, memo: &mut Vec<(u32, usize)>, depth: usize) -> usize {
        if depth > 64 {
            return SCENE;
        }
        if let Some(&(_, p)) = memo.iter().find(|(n, _)| *n == id) {
            return p;
        }
        let Some(ni) = self.index(id) else { return SCENE };
        let node = &self.nodes[ni];
        if node.kind == NodeKind::SceneRender {
            return SCENE;
        }
        let info = node.kind.info();
        let mut inputs = Vec::new();
        for slot in 0..info.inputs.len() {
            let src = self.links.iter().find(|l| l.to == id && l.slot == slot).map(|l| l.from);
            inputs.push(match src {
                Some(s) => self.visit(s, passes, memo, depth + 1),
                None => SCENE,
            });
        }
        if !node.enabled {
            let r = inputs.first().copied().unwrap_or(SCENE);
            memo.push((id, r));
            return r;
        }
        passes.push(Pass { node: ni, inputs });
        let r = passes.len() - 1;
        memo.push((id, r));
        r
    }

    /// Lay nodes out left-to-right in chain order (used by presets).
    pub fn chain(kinds: &[NodeKind], fx: &[NodeKind], name: &str) -> Self {
        let mut g = Self { nodes: vec![], links: vec![], next_id: 1, preset_name: name.into(), groups: vec![], materials: Default::default(), model_looks: Default::default(), mat_fx: Default::default(), lights: Vec::new() };
        let scene = g.add(NodeKind::SceneRender, [40.0, 120.0]);
        for (i, k) in fx.iter().enumerate() {
            let id = g.add(*k, [-230.0 + 0.0 * i as f32, 30.0 + 120.0 * i as f32]);
            g.connect(id, scene, 0);
        }
        let mut prev = scene;
        for (i, k) in kinds.iter().enumerate() {
            let id = g.add(*k, [270.0 + 200.0 * i as f32, 120.0 + if i % 2 == 1 { 50.0 } else { 0.0 }]);
            g.connect(prev, id, 0);
            prev = id;
        }
        let out = g.add(NodeKind::Output, [270.0 + 200.0 * kinds.len() as f32, 120.0]);
        g.connect(prev, out, 0);
        g
    }

    pub fn first_mut(&mut self, kind: NodeKind) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.kind == kind)
    }
}

pub struct Preset {
    pub name: &'static str,
    pub about: &'static str,
    pub build: fn() -> EffectGraph,
}

pub fn presets() -> Vec<Preset> {
    use NodeKind::*;
    vec![
        Preset {
            name: "Studio Clean",
            about: "Neutral starting point: polished toon, soft bloom, filmic tone.",
            build: || {
                let mut g = EffectGraph::chain(&[Bloom, Tonemap, Fxaa], &[ShadingStyle], "Studio Clean");
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Intensity", PVal::F(0.35));
                    b.set("Threshold", PVal::F(0.95));
                }
                g
            },
        },
        Preset {
            name: "Ray Lighting (PBR)",
            about: "Physically based look made from scratch: sky light, real materials (skin, metal, glass…), contact shadows, screen reflections, auto exposure and a film tone curve.",
            build: || {
                let mut g = EffectGraph::chain(&[Ssao, ScreenReflections, Bloom, AutoExposure, Tonemap, Fxaa], &[RayLighting, Sky], "Ray Lighting (PBR)");
                if let Some(t) = g.first_mut(Tonemap) {
                    t.set("Curve", PVal::I(2));
                }
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Threshold", PVal::F(1.0));
                    b.set("Intensity", PVal::F(0.5));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.32, 0.48, 0.82]));
                    s.set("Horizon", PVal::C([0.78, 0.8, 0.86]));
                    s.set("Bottom", PVal::C([0.3, 0.28, 0.27]));
                }
                g
            },
        },
        Preset {
            name: "Realistic Light",
            about: "Ray Lighting with a warm evening sun, light shafts, soft contact shadows and a gentle vignette.",
            build: || {
                let mut g = EffectGraph::chain(&[Ssao, ScreenReflections, LightShafts, Bloom, Tonemap, Vignette, Fxaa], &[RayLighting, Sky], "Realistic Light");
                if let Some(r) = g.first_mut(RayLighting) {
                    r.set("Sun strength", PVal::F(3.6));
                    r.set("Sky light", PVal::F(0.7));
                }
                if let Some(t) = g.first_mut(Tonemap) {
                    t.set("Curve", PVal::I(2));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.18, 0.24, 0.45]));
                    s.set("Horizon", PVal::C([0.95, 0.62, 0.42]));
                    s.set("Bottom", PVal::C([0.2, 0.16, 0.14]));
                }
                g
            },
        },
        Preset {
            name: "Polished Toon",
            about: "Smooth anime shading with rim light, soft glow and crisp lines (polish-style).",
            build: || {
                let mut g = EffectGraph::chain(&[Bloom, Outline, Tonemap, Sharpen, Fxaa], &[ShadingStyle, Sky], "Polished Toon");
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Threshold", PVal::F(0.95));
                    b.set("Intensity", PVal::F(0.45));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.07, 0.08, 0.13]));
                    s.set("Horizon", PVal::C([0.17, 0.16, 0.22]));
                    s.set("Bottom", PVal::C([0.06, 0.06, 0.08]));
                }
                if let Some(s) = g.first_mut(ShadingStyle) {
                    s.set("Style", PVal::I(1));
                    s.set("Rim light", PVal::F(0.55));
                    s.set("Skin glow", PVal::F(0.35));
                }
                if let Some(o) = g.first_mut(Outline) {
                    o.set("Strength", PVal::F(0.35));
                }
                g
            },
        },
        Preset {
            name: "Fresh Bright Anime",
            about: "High-key, airy colours with lifted shadows and a gentle bloom (bright shader style).",
            build: || {
                let mut g = EffectGraph::chain(&[Bloom, Tonemap, Fxaa], &[ShadingStyle, Sky], "Fresh Bright Anime");
                if let Some(s) = g.first_mut(ShadingStyle) {
                    s.set("Style", PVal::I(3));
                    s.set("Shadow tint", PVal::C([0.85, 0.78, 0.92]));
                    s.set("Saturation", PVal::F(1.2));
                    s.set("Ambient light", PVal::F(1.3));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.55, 0.78, 1.0]));
                    s.set("Horizon", PVal::C([0.95, 0.95, 1.0]));
                    s.set("Bottom", PVal::C([0.85, 0.9, 0.95]));
                }
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Threshold", PVal::F(0.7));
                    b.set("Intensity", PVal::F(0.6));
                }
                g
            },
        },
        Preset {
            name: "Beams & Sparkles",
            about: "Concert stage: sweeping light beams, floating sparkles, strong glow (beam/particle style).",
            build: || {
                let mut g = EffectGraph::chain(&[Bloom, LightShafts, Tonemap, ChromaticAberration], &[ShadingStyle, LightBeams, Particles, MirrorFloor, Sky], "Beams & Sparkles");
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Intensity", PVal::F(0.9));
                    b.set("Threshold", PVal::F(0.85));
                }
                if let Some(l) = g.first_mut(LightShafts) {
                    l.set("Strength", PVal::F(0.35));
                    l.set("Threshold", PVal::F(0.9));
                }
                if let Some(c) = g.first_mut(ChromaticAberration) {
                    c.set("Amount", PVal::F(0.35));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.02, 0.02, 0.05]));
                    s.set("Horizon", PVal::C([0.1, 0.06, 0.16]));
                    s.set("Bottom", PVal::C([0.03, 0.02, 0.05]));
                }
                g
            },
        },
        Preset {
            name: "Water Stage",
            about: "Dance on a reflective lake with ripples, haze and glints.",
            build: || {
                let mut g = EffectGraph::chain(&[Fog, Bloom, Tonemap, Vignette], &[ShadingStyle, Water, Sky, Particles], "Water Stage");
                if let Some(p) = g.first_mut(Particles) {
                    p.set("Kind", PVal::I(6));
                    p.set("Amount", PVal::F(800.0));
                    p.set("Colour", PVal::C([0.6, 1.0, 0.7]));
                }
                if let Some(f) = g.first_mut(Fog) {
                    f.set("Start", PVal::F(80.0));
                    f.set("Density", PVal::F(0.15));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.03, 0.05, 0.14]));
                    s.set("Horizon", PVal::C([0.45, 0.35, 0.5]));
                    s.set("Stars", PVal::F(0.6));
                }
                g
            },
        },
        Preset {
            name: "Cinematic",
            about: "Shallow focus, letterbox, film grain and colour fringe.",
            build: || {
                let mut g = EffectGraph::chain(&[DepthOfField, Bloom, Tonemap, ChromaticAberration, FilmGrain, Vignette, Letterbox], &[ShadingStyle], "Cinematic");
                if let Some(s) = g.first_mut(ShadingStyle) {
                    s.set("Style", PVal::I(2));
                }
                g
            },
        },
        Preset {
            name: "Snowy Night",
            about: "Falling snow, cool fog and soft glow.",
            build: || {
                let mut g = EffectGraph::chain(&[Fog, Bloom, Tonemap, Vignette], &[ShadingStyle, Particles, Sky], "Snowy Night");
                if let Some(p) = g.first_mut(Particles) {
                    p.set("Kind", PVal::I(1));
                    p.set("Amount", PVal::F(12000.0));
                    p.set("Colour", PVal::C([0.95, 0.97, 1.0]));
                    p.set("Glow", PVal::F(1.0));
                    p.set("Size", PVal::F(0.15));
                }
                if let Some(f) = g.first_mut(Fog) {
                    f.set("Colour", PVal::C([0.45, 0.5, 0.65]));
                }
                g
            },
        },
        Preset {
            name: "Sakura Breeze",
            about: "Drifting petals and a warm, pastel tone.",
            build: || {
                let mut g = EffectGraph::chain(&[Bloom, Tonemap, Fxaa], &[ShadingStyle, Particles, Sky], "Sakura Breeze");
                if let Some(p) = g.first_mut(Particles) {
                    p.set("Kind", PVal::I(2));
                    p.set("Amount", PVal::F(2500.0));
                    p.set("Colour", PVal::C([1.0, 0.72, 0.82]));
                    p.set("Glow", PVal::F(1.0));
                    p.set("Wind", PVal::F(1.2));
                }
                if let Some(s) = g.first_mut(Sky) {
                    s.set("Top", PVal::C([0.6, 0.75, 0.95]));
                    s.set("Horizon", PVal::C([1.0, 0.88, 0.92]));
                }
                g
            },
        },
        Preset {
            name: "Neon Club",
            about: "Laser grid, heavy glow, ink outlines and posterized punch.",
            build: || {
                let mut g = EffectGraph::chain(&[Outline, Bloom, Tonemap, ChromaticAberration], &[ShadingStyle, LightBeams, MirrorFloor], "Neon Club");
                if let Some(l) = g.first_mut(LightBeams) {
                    l.set("Pattern", PVal::I(2));
                    l.set("Rainbow", PVal::B(true));
                    l.set("Width", PVal::F(0.25));
                    l.set("Brightness", PVal::F(4.0));
                }
                if let Some(b) = g.first_mut(Bloom) {
                    b.set("Intensity", PVal::F(1.8));
                }
                if let Some(t) = g.first_mut(Tonemap) {
                    t.set("Curve", PVal::I(4));
                }
                g
            },
        },
        Preset {
            name: "Retro Film",
            about: "Posterized colour, heavy grain and vignette.",
            build: || EffectGraph::chain(&[Posterize, Tonemap, FilmGrain, Vignette], &[ShadingStyle], "Retro Film"),
        },
        Preset { name: "No Effects", about: "Just the raw scene.", build: || {
            let mut g = EffectGraph::empty();
            g.preset_name = "No Effects".into();
            g
        } },
    ]
}

// ---------------------------------------------------------------- particles

#[derive(Clone, Copy, Default)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub life: f32,
    pub max_life: f32,
    pub seed: f32,
}

/// Instance data uploaded to the GPU: position + size, colour.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ParticleInstance {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

pub struct ParticleSim {
    pub particles: Vec<Particle>,
    pub instances: Vec<ParticleInstance>,
    rng: u64,
    pub time: f32,
}

fn hash(mut x: u64) -> f32 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

impl ParticleSim {
    pub fn new() -> Self {
        Self { particles: Vec::new(), instances: Vec::new(), rng: 12345, time: 0.0 }
    }

    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9E3779B97F4A7C15);
        hash(self.rng)
    }

    fn spawn(&mut self, kind: usize, area: f32, height: f32, speed: f32, fresh: bool) -> Particle {
        let x = (self.rand() - 0.5) * area;
        let z = (self.rand() - 0.5) * area;
        let seed = self.rand();
        let (y, vel, life) = match kind {
            1 | 2 | 5 => (
                if fresh { self.rand() * height } else { height },
                Vec3::new(0.0, -(1.5 + self.rand() * 1.5) * speed * if kind == 1 { 1.0 } else { 1.6 }, 0.0),
                height / (2.0 * speed.max(0.05)) + 4.0,
            ),
            3 | 4 => (if fresh { self.rand() * height } else { 0.0 }, Vec3::new(0.0, (2.0 + self.rand() * 3.0) * speed, 0.0), 4.0 + self.rand() * 6.0),
            6 => (self.rand() * height * 0.6 + 1.0, Vec3::ZERO, 5.0 + self.rand() * 6.0),
            _ => (self.rand() * height, Vec3::new(0.0, 0.6 * speed, 0.0), 2.0 + self.rand() * 4.0),
        };
        let life = life.max(0.5);
        Particle { pos: Vec3::new(x, y, z), vel, life: if fresh { self.rand() * life } else { 0.0 }, max_life: life, seed }
    }

    /// Advance the simulation. `time` is the scene time so playback scrubbing stays consistent.
    pub fn update(&mut self, node: &crate::effects::Node, dt: f32) {
        let kind = node.i("Kind");
        let count = node.f("Amount") as usize;
        let area = node.f("Area");
        let height = node.f("Height");
        let speed = node.f("Speed");
        let wind = node.f("Wind");
        let size = node.f("Size");
        let glow = node.f("Glow");
        let base = node.c("Colour");
        self.time += dt;
        if self.particles.len() > count {
            self.particles.truncate(count);
        }
        while self.particles.len() < count {
            let p = self.spawn(kind, area, height, speed, true);
            self.particles.push(p);
        }
        // Respawn dead particles (serial for the RNG), then integrate in parallel.
        for i in 0..self.particles.len() {
            if self.particles[i].life >= self.particles[i].max_life || self.particles[i].pos.y < -2.0 {
                self.particles[i] = self.spawn(kind, area, height, speed, false);
            }
        }
        let t = self.time;
        self.particles.par_iter_mut().for_each(|p| {
            p.life += dt;
            let s = p.seed * 100.0;
            let sway = match kind {
                1 => Vec3::new((t * 0.7 + s).sin() * 0.6 + wind, 0.0, (t * 0.5 + s * 1.3).cos() * 0.6),
                2 => Vec3::new((t * 1.3 + s).sin() * 1.4 + wind * 2.0, (t * 2.1 + s).sin() * 0.4, (t * 0.9 + s).cos() * 1.2),
                4 => Vec3::new((t * 3.0 + s).sin() * 0.5, 0.0, (t * 2.6 + s).cos() * 0.5),
                6 => Vec3::new((t * 0.8 + s).sin() * 1.2, (t * 1.1 + s * 2.0).sin() * 0.8, (t * 0.7 + s * 3.0).cos() * 1.2),
                _ => Vec3::new(wind, 0.0, 0.0),
            };
            p.pos += (p.vel + sway * speed.max(0.1)) * dt;
        });
        self.instances.clear();
        self.instances.par_extend(self.particles.par_iter().map(|p| {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            let fade = (k * 8.0).min(1.0) * ((1.0 - k) * 4.0).min(1.0);
            let s = p.seed * 100.0;
            let (c, a, sz) = match kind {
                0 => {
                    let tw = 0.5 + 0.5 * (t * 6.0 + s).sin();
                    (base * glow * (0.6 + tw), fade * tw, size * (0.6 + tw * 0.6))
                }
                3 => (Vec3::new(1.0, 0.45 + 0.3 * (1.0 - k), 0.15) * base * glow * 1.5, fade, size * (1.0 - k * 0.6)),
                4 => (base * 0.9 + Vec3::splat(0.3), fade * 0.6, size * 1.6),
                5 => {
                    let h = (p.seed * 6.0).floor();
                    let pal = [
                        Vec3::new(1.0, 0.3, 0.4),
                        Vec3::new(1.0, 0.85, 0.2),
                        Vec3::new(0.3, 0.9, 0.5),
                        Vec3::new(0.3, 0.6, 1.0),
                        Vec3::new(0.8, 0.4, 1.0),
                        Vec3::new(1.0, 0.6, 0.2),
                    ];
                    (pal[h as usize % 6] * glow.max(0.6), fade, size)
                }
                6 => {
                    let tw = (0.5 + 0.5 * (t * 2.0 + s).sin()).powf(3.0);
                    (base * glow * 2.0 * tw, fade * tw, size * 0.8)
                }
                _ => (base * glow.max(0.5), fade, size),
            };
            ParticleInstance { pos: p.pos.to_array(), size: sz, color: Vec4::new(c.x, c.y, c.z, a).to_array() }
        }));
    }
}
