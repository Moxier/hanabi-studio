//! Lights, materials and the material node graph, plus presets the user can save and reuse
//! across projects. Everything here is data; the renderer turns it into shader inputs.

use crate::effects::MatPreset;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ------------------------------------------------------------------ lights

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightKind {
    Point,
    Spot,
    Directional,
}

impl LightKind {
    pub const ALL: [LightKind; 3] = [LightKind::Point, LightKind::Spot, LightKind::Directional];
    pub fn label(self) -> &'static str {
        match self {
            LightKind::Point => "Point light",
            LightKind::Spot => "Spotlight",
            LightKind::Directional => "Directional light",
        }
    }
}

/// An extra light placed in the scene (the main light / sun stays in Camera & Light).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneLight {
    pub name: String,
    pub kind: LightKind,
    pub enabled: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub position: [f32; 3],
    /// Where it points (spot / directional), in degrees: pitch (down is negative) and yaw.
    pub pitch: f32,
    pub yaw: f32,
    pub range: f32,
    pub cone: f32,
    pub softness: f32,
}

impl SceneLight {
    pub fn new(kind: LightKind) -> Self {
        Self {
            name: kind.label().to_string(),
            kind,
            enabled: true,
            color: [1.0, 0.92, 0.8],
            intensity: match kind {
                LightKind::Directional => 0.6,
                _ => 2.0,
            },
            position: [8.0, 18.0, -10.0],
            pitch: -45.0,
            yaw: 30.0,
            range: 40.0,
            cone: 30.0,
            softness: 0.3,
        }
    }

    /// Direction the light travels (scene space).
    pub fn direction(&self) -> glam::Vec3 {
        let (p, y) = (self.pitch.to_radians(), self.yaw.to_radians());
        glam::Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos()).normalize_or(glam::Vec3::NEG_Y)
    }
}

// ------------------------------------------------------------------ materials

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Special {
    None,
    Glass,
    Water,
    Wet,
    Fur,
    Grass,
    Glow,
}

impl Special {
    pub const ALL: [Special; 7] = [Special::None, Special::Glass, Special::Water, Special::Wet, Special::Fur, Special::Grass, Special::Glow];
    pub fn label(self) -> &'static str {
        match self {
            Special::None => "Standard",
            Special::Glass => "Glass",
            Special::Water => "Water + ripples",
            Special::Wet => "Wet + rain",
            Special::Fur => "Fur",
            Special::Grass => "Grass",
            Special::Glow => "Glow / luminous",
        }
    }
    pub fn index(self) -> i32 {
        Special::ALL.iter().position(|s| *s == self).unwrap_or(0) as i32
    }
}

/// Everything about how one material is drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialFx {
    pub base: MatPreset,
    pub special: Special,
    pub rough: f32,
    pub metal: f32,
    pub spec: f32,
    pub sss: f32,
    pub tint: [f32; 3],
    pub emissive: f32,
    pub emissive_color: [f32; 3],
    /// Luminous: also light up the surroundings like a lamp.
    pub emit_light: bool,
    pub light_range: f32,
    pub normal_map: Option<String>,
    pub normal_strength: f32,
    pub height_map: Option<String>,
    pub height_scale: f32,
    pub metal_map: Option<String>,
    pub rough_map: Option<String>,
    pub uv_scale: f32,
    pub wet: f32,
    pub rain: f32,
    pub fur_length: f32,
    pub fur_density: f32,
    pub fur_layers: u32,
    pub fur_droop: f32,
    pub glass_opacity: f32,
    pub ripple_scale: f32,
    pub ripple_speed: f32,
    /// Built in the node editor; replaces parts of the settings above when present.
    pub graph: Option<MatGraph>,
}

impl Default for MaterialFx {
    fn default() -> Self {
        Self::from_preset(MatPreset::Cloth, 0.55)
    }
}

impl MaterialFx {
    pub fn from_preset(p: MatPreset, default_rough: f32) -> Self {
        let mp = p.params(default_rough);
        let mut m = Self {
            base: p,
            special: Special::None,
            rough: mp.rough,
            metal: mp.metal,
            spec: mp.spec,
            sss: mp.sss,
            tint: [1.0; 3],
            emissive: mp.emissive,
            emissive_color: [1.0, 0.85, 0.6],
            emit_light: false,
            light_range: 25.0,
            normal_map: None,
            normal_strength: 1.0,
            height_map: None,
            height_scale: 0.03,
            metal_map: None,
            rough_map: None,
            uv_scale: 1.0,
            wet: 0.7,
            rain: 0.5,
            fur_length: 0.25,
            fur_density: 260.0,
            fur_layers: 16,
            fur_droop: 0.3,
            glass_opacity: 0.25,
            ripple_scale: 1.0,
            ripple_speed: 1.0,
            graph: None,
        };
        match p {
            MatPreset::Glass => m.special = Special::Glass,
            MatPreset::Emissive => {
                m.special = Special::Glow;
                m.emissive = 2.0;
            }
            _ => {}
        }
        m
    }

    pub fn is_toon(&self) -> bool {
        self.base == MatPreset::Toon
    }

    /// Every texture file this material uses.
    pub fn texture_paths(&self) -> Vec<String> {
        let mut v: Vec<String> = [&self.normal_map, &self.height_map, &self.metal_map, &self.rough_map].iter().filter_map(|x| (*x).clone()).collect();
        if let Some(g) = &self.graph {
            v.extend(g.texture_paths());
        }
        v
    }
}

/// Built-in material looks to start from.
pub fn builtin_material_presets() -> Vec<(&'static str, MaterialFx)> {
    let d = |p| MaterialFx::from_preset(p, 0.55);
    let mut list = vec![
        ("Matte cloth", d(MatPreset::Cloth)),
        ("Silk / satin", MaterialFx { rough: 0.28, spec: 1.1, ..d(MatPreset::Cloth) }),
        ("Skin", d(MatPreset::Skin)),
        ("Hair", d(MatPreset::Hair)),
        ("Glossy eyes", d(MatPreset::Eyes)),
        ("Plastic", d(MatPreset::Plastic)),
        ("Leather", d(MatPreset::Leather)),
        ("Brushed metal", MaterialFx { rough: 0.38, ..d(MatPreset::Metal) }),
        ("Chrome", MaterialFx { rough: 0.06, ..d(MatPreset::Metal) }),
        ("Gold", d(MatPreset::Gold)),
        ("Clear glass", MaterialFx { glass_opacity: 0.15, ..d(MatPreset::Glass) }),
        ("Tinted crystal", MaterialFx { glass_opacity: 0.45, tint: [0.75, 0.85, 1.0], ..d(MatPreset::Glass) }),
        ("Water + ripples", MaterialFx { special: Special::Water, rough: 0.04, ..d(MatPreset::Plastic) }),
        ("Wet in rain", MaterialFx { special: Special::Wet, ..d(MatPreset::Cloth) }),
        ("Wet skin", MaterialFx { special: Special::Wet, wet: 0.5, rain: 0.0, ..d(MatPreset::Skin) }),
        ("Short fur", MaterialFx { special: Special::Fur, rough: 0.7, ..d(MatPreset::Cloth) }),
        ("Long fluffy fur", MaterialFx { special: Special::Fur, fur_length: 0.55, fur_density: 180.0, fur_layers: 24, fur_droop: 0.6, ..d(MatPreset::Cloth) }),
        ("Grass", MaterialFx { special: Special::Grass, fur_length: 0.9, fur_density: 90.0, fur_layers: 20, fur_droop: 0.15, tint: [0.55, 0.85, 0.4], ..d(MatPreset::Cloth) }),
        ("Soft glow", MaterialFx { special: Special::Glow, emissive: 1.2, ..d(MatPreset::Plastic) }),
        ("Neon tube", MaterialFx { special: Special::Glow, emissive: 4.0, emissive_color: [1.0, 0.3, 0.7], ..d(MatPreset::Plastic) }),
        ("Light bulb (lights the scene)", MaterialFx { special: Special::Glow, emissive: 3.0, emit_light: true, ..d(MatPreset::Plastic) }),
        ("Keep toon look", d(MatPreset::Toon)),
    ];
    list.iter_mut().for_each(|(_, m)| {
        if m.special == Special::Glow && m.emissive == 0.0 {
            m.emissive = 1.5;
        }
    });
    list
}

// ------------------------------------------------------------------ material node graph

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatNodeKind {
    Output,
    BaseTexture,
    Texture,
    Color,
    Value,
    Uv,
    Time,
    Fresnel,
    Noise,
    FacingUp,
    Mix,
    Multiply,
    Add,
    Contrast,
    Invert,
}

pub struct MatNodeInfo {
    pub title: &'static str,
    pub about: &'static str,
    pub inputs: &'static [&'static str],
    pub output: bool,
    /// (name, min, max, default) for params f[0..]
    pub params: &'static [(&'static str, f32, f32, f32)],
    pub color_param: bool,
}

impl MatNodeKind {
    pub const ADDABLE: &'static [MatNodeKind] = &[
        MatNodeKind::BaseTexture,
        MatNodeKind::Texture,
        MatNodeKind::Color,
        MatNodeKind::Value,
        MatNodeKind::Uv,
        MatNodeKind::Time,
        MatNodeKind::Fresnel,
        MatNodeKind::Noise,
        MatNodeKind::FacingUp,
        MatNodeKind::Mix,
        MatNodeKind::Multiply,
        MatNodeKind::Add,
        MatNodeKind::Contrast,
        MatNodeKind::Invert,
    ];
    pub fn info(self) -> MatNodeInfo {
        use MatNodeKind::*;
        let (title, about, inputs, output, params, color_param): (_, _, &'static [&'static str], _, &'static [(&'static str, f32, f32, f32)], _) = match self {
            Output => ("Material Output", "What the surface becomes.", &["Base colour", "Roughness", "Metallic", "Emission", "Alpha", "Normal map"], false, &[], false),
            BaseTexture => ("Model Texture", "The model's own texture and colour.", &[], true, &[], false),
            Texture => ("Image Texture", "A picture file (PNG, JPG, BMP, TGA, DDS).", &["UV"], true, &[], false),
            Color => ("Colour", "A fixed colour.", &[], true, &[], true),
            Value => ("Value", "A fixed number.", &[], true, &[("Value", 0.0, 4.0, 0.5)], false),
            Uv => ("UV", "Texture coordinates, scaled and scrolling.", &[], true, &[("Scale", 0.05, 50.0, 1.0), ("Scroll X", -2.0, 2.0, 0.0), ("Scroll Y", -2.0, 2.0, 0.0)], false),
            Time => ("Time", "Seconds since the start, for animation.", &[], true, &[("Speed", -10.0, 10.0, 1.0)], false),
            Fresnel => ("Fresnel", "Bright at grazing angles (edges).", &[], true, &[("Power", 0.5, 10.0, 3.0)], false),
            Noise => ("Noise", "Smooth random pattern.", &["UV"], true, &[("Scale", 0.5, 200.0, 12.0), ("Speed", 0.0, 5.0, 0.0)], false),
            FacingUp => ("Facing Up", "1 where the surface points at the sky (snow, wetness on top).", &[], true, &[("Sharpness", 0.5, 8.0, 2.0)], false),
            Mix => ("Mix", "Blend A to B by Factor.", &["A", "B", "Factor"], true, &[("Factor", 0.0, 1.0, 0.5)], false),
            Multiply => ("Multiply", "A × B.", &["A", "B"], true, &[], false),
            Add => ("Add", "A + B.", &["A", "B"], true, &[], false),
            Contrast => ("Ramp", "Remap: below Low is 0, above High is 1.", &["A"], true, &[("Low", 0.0, 1.0, 0.3), ("High", 0.0, 1.0, 0.7)], false),
            Invert => ("Invert", "1 − A.", &["A"], true, &[], false),
        };
        MatNodeInfo { title, about, inputs, output, params, color_param }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatNode {
    pub id: u32,
    pub kind: MatNodeKind,
    pub pos: [f32; 2],
    pub f: [f32; 4],
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatLink {
    pub from: u32,
    pub to: u32,
    pub slot: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatGraph {
    pub nodes: Vec<MatNode>,
    pub links: Vec<MatLink>,
    pub next_id: u32,
}

impl MatGraph {
    /// A starting graph: the model's texture straight into the output.
    pub fn starter() -> Self {
        let mut g = Self { nodes: vec![], links: vec![], next_id: 1 };
        let b = g.add(MatNodeKind::BaseTexture, [20.0, 60.0]);
        let o = g.add(MatNodeKind::Output, [360.0, 40.0]);
        g.connect(b, o, 0);
        g
    }

    pub fn add(&mut self, kind: MatNodeKind, pos: [f32; 2]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut f = [0.0; 4];
        let info = kind.info();
        for (i, p) in info.params.iter().enumerate() {
            f[i] = p.3;
        }
        if info.color_param {
            f = [0.9, 0.6, 0.3, 1.0];
        }
        self.nodes.push(MatNode { id, kind, pos, f, path: None });
        id
    }

    pub fn node(&self, id: u32) -> Option<&MatNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: u32) -> Option<&mut MatNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
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
            stack.extend(self.links.iter().filter(|l| l.from == n).map(|l| l.to));
        }
        false
    }

    pub fn connect(&mut self, from: u32, to: u32, slot: usize) {
        let (Some(f), Some(t)) = (self.node(from), self.node(to)) else { return };
        if !f.kind.info().output || slot >= t.kind.info().inputs.len() || from == to || self.reaches(to, from) {
            return;
        }
        self.links.retain(|l| !(l.to == to && l.slot == slot));
        self.links.push(MatLink { from, to, slot });
    }

    pub fn remove(&mut self, id: u32) {
        if self.node(id).map(|n| n.kind) == Some(MatNodeKind::Output) {
            return;
        }
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
    }

    pub fn texture_paths(&self) -> Vec<String> {
        self.nodes.iter().filter(|n| n.kind == MatNodeKind::Texture).filter_map(|n| n.path.clone()).collect()
    }

    /// GLSL for `custom_material(...)` and the texture files bound to u_ctex0..3, in order.
    pub fn compile(&self) -> (String, Vec<String>) {
        let mut code = String::new();
        let mut textures: Vec<String> = Vec::new();
        let mut done: Vec<u32> = Vec::new();
        let Some(out) = self.nodes.iter().find(|n| n.kind == MatNodeKind::Output) else {
            return (EMPTY_CUSTOM.to_string(), vec![]);
        };
        let mut body = String::new();
        let input_of = |id: u32, slot: usize| self.links.iter().find(|l| l.to == id && l.slot == slot).map(|l| l.from);
        fn emit(g: &MatGraph, id: u32, body: &mut String, done: &mut Vec<u32>, textures: &mut Vec<String>, depth: usize) {
            if done.contains(&id) || depth > 64 {
                return;
            }
            let Some(n) = g.node(id) else { return };
            let ins: Vec<Option<u32>> = (0..n.kind.info().inputs.len()).map(|s| g.links.iter().find(|l| l.to == id && l.slot == s).map(|l| l.from)).collect();
            for i in ins.iter().flatten() {
                emit(g, *i, body, done, textures, depth + 1);
            }
            let inp = |s: usize, def: &str| ins.get(s).copied().flatten().map(|i| format!("n{i}")).unwrap_or_else(|| def.to_string());
            let f = n.f;
            let expr = match n.kind {
                MatNodeKind::Output => return,
                MatNodeKind::BaseTexture => "vec4(base, alpha)".to_string(),
                MatNodeKind::Texture => {
                    let slot = match &n.path {
                        Some(p) if textures.len() < 4 || textures.contains(p) => {
                            if !textures.contains(p) {
                                textures.push(p.clone());
                            }
                            textures.iter().position(|t| t == p).unwrap_or(0) as i32
                        }
                        _ => -1,
                    };
                    let uv = inp(0, "vec4(uv, 0.0, 0.0)");
                    if slot >= 0 { format!("texture(u_ctex{slot}, ({uv}).xy)") } else { "vec4(1.0, 0.0, 1.0, 1.0)".to_string() }
                }
                MatNodeKind::Color => format!("vec4({:.4}, {:.4}, {:.4}, 1.0)", f[0], f[1], f[2]),
                MatNodeKind::Value => format!("vec4({:.4})", f[0]),
                MatNodeKind::Uv => format!("vec4(uv * {:.4} + vec2({:.4}, {:.4}) * t, 0.0, 0.0)", f[0], f[1], f[2]),
                MatNodeKind::Time => format!("vec4(t * {:.4})", f[0]),
                MatNodeKind::Fresnel => format!("vec4(pow(1.0 - max(dot(n, v), 0.0), {:.4}))", f[0]),
                MatNodeKind::Noise => format!("vec4(cm_noise(({}).xy * {:.4} + vec2(t * {:.4})))", inp(0, "vec4(uv, 0.0, 0.0)"), f[0], f[1]),
                MatNodeKind::FacingUp => format!("vec4(pow(clamp(n.y, 0.0, 1.0), {:.4}))", f[0]),
                MatNodeKind::Mix => format!("mix({}, {}, ({}).x)", inp(0, "vec4(0.0)"), inp(1, "vec4(1.0)"), inp(2, &format!("vec4({:.4})", f[0]))),
                MatNodeKind::Multiply => format!("({}) * ({})", inp(0, "vec4(1.0)"), inp(1, "vec4(1.0)")),
                MatNodeKind::Add => format!("({}) + ({})", inp(0, "vec4(0.0)"), inp(1, "vec4(0.0)")),
                MatNodeKind::Contrast => format!("smoothstep(vec4({:.4}), vec4(max({:.4}, {:.4} + 0.001)), {})", f[0], f[1], f[0], inp(0, "vec4(0.5)")),
                MatNodeKind::Invert => format!("vec4(1.0) - ({})", inp(0, "vec4(0.0)")),
            };
            body.push_str(&format!("    vec4 n{id} = {expr};\n"));
            done.push(id);
        }
        for s in 0..6 {
            if let Some(src) = input_of(out.id, s) {
                emit(self, src, &mut body, &mut done, &mut textures, 0);
            }
        }
        let assign = [
            "    base = n{}.rgb;\n",
            "    rough = clamp(n{}.x, 0.02, 1.0);\n",
            "    metal = clamp(n{}.x, 0.0, 1.0);\n",
            "    emis += n{}.rgb;\n",
            "    alpha *= clamp(n{}.x, 0.0, 1.0);\n",
            "    nmap = n{}.rgb; use_nmap = 1.0;\n",
        ];
        for (s, a) in assign.iter().enumerate() {
            if let Some(src) = input_of(out.id, s) {
                body.push_str(&a.replace("{}", &src.to_string()));
            }
        }
        for i in 0..4 {
            code.push_str(&format!("uniform sampler2D u_ctex{i};\n"));
        }
        code.push_str("float cm_hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }\n");
        code.push_str("float cm_noise(vec2 p) { vec2 i = floor(p); vec2 f = fract(p); f = f * f * (3.0 - 2.0 * f);\n  return mix(mix(cm_hash(i), cm_hash(i + vec2(1, 0)), f.x), mix(cm_hash(i + vec2(0, 1)), cm_hash(i + vec2(1, 1)), f.x), f.y); }\n");
        code.push_str("void custom_material(inout vec3 base, inout float rough, inout float metal, inout vec3 emis, inout float alpha, inout vec3 nmap, inout float use_nmap, vec2 uv, vec3 n, vec3 v, float t) {\n");
        code.push_str(&body);
        code.push_str("}\n");
        (code, textures)
    }
}

pub const EMPTY_CUSTOM: &str = "void custom_material(inout vec3 base, inout float rough, inout float metal, inout vec3 emis, inout float alpha, inout vec3 nmap, inout float use_nmap, vec2 uv, vec3 n, vec3 v, float t) {}\n";

// ------------------------------------------------------------------ user presets on disk

/// Presets the person saved themselves: reused in every project.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPresets {
    pub materials: Vec<(String, MaterialFx)>,
    pub light_rigs: Vec<(String, Vec<SceneLight>)>,
    pub screen_stacks: Vec<(String, crate::effects::EffectGraph)>,
}

pub fn presets_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("hanabi-studio").join("presets.json")
}

impl UserPresets {
    pub fn load() -> Self {
        std::fs::read_to_string(presets_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let p = presets_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

/// Built-in light rigs.
pub fn builtin_light_rigs() -> Vec<(&'static str, Vec<SceneLight>)> {
    let l = |kind, name: &str, color: [f32; 3], intensity, position: [f32; 3], pitch, yaw, cone| SceneLight {
        name: name.to_string(),
        kind,
        enabled: true,
        color,
        intensity,
        position,
        pitch,
        yaw,
        range: 60.0,
        cone,
        softness: 0.35,
    };
    vec![
        (
            "Studio three-point",
            vec![
                l(LightKind::Spot, "Key", [1.0, 0.95, 0.88], 3.0, [-14.0, 24.0, -18.0], -40.0, 35.0, 35.0),
                l(LightKind::Point, "Fill", [0.7, 0.8, 1.0], 1.0, [16.0, 12.0, -14.0], 0.0, 0.0, 30.0),
                l(LightKind::Spot, "Rim", [1.0, 1.0, 1.0], 3.5, [0.0, 22.0, 16.0], -40.0, 180.0, 40.0),
            ],
        ),
        (
            "Concert spots",
            vec![
                l(LightKind::Spot, "Pink spot", [1.0, 0.3, 0.6], 5.0, [-12.0, 34.0, -6.0], -65.0, 60.0, 18.0),
                l(LightKind::Spot, "Blue spot", [0.3, 0.5, 1.0], 5.0, [12.0, 34.0, -6.0], -65.0, -60.0, 18.0),
                l(LightKind::Spot, "Back white", [1.0, 1.0, 1.0], 4.0, [0.0, 30.0, 14.0], -55.0, 180.0, 25.0),
            ],
        ),
        (
            "Warm room",
            vec![
                l(LightKind::Point, "Lamp", [1.0, 0.7, 0.4], 2.5, [-10.0, 14.0, -6.0], 0.0, 0.0, 30.0),
                l(LightKind::Point, "Window bounce", [0.6, 0.75, 1.0], 0.8, [12.0, 18.0, 8.0], 0.0, 0.0, 30.0),
            ],
        ),
        (
            "Night city",
            vec![
                l(LightKind::Point, "Neon magenta", [1.0, 0.2, 0.7], 3.0, [-10.0, 8.0, 4.0], 0.0, 0.0, 30.0),
                l(LightKind::Point, "Neon cyan", [0.2, 0.9, 1.0], 3.0, [10.0, 10.0, 2.0], 0.0, 0.0, 30.0),
                l(LightKind::Directional, "Moon", [0.5, 0.6, 1.0], 0.3, [0.0, 30.0, 0.0], -50.0, 150.0, 30.0),
            ],
        ),
    ]
}
