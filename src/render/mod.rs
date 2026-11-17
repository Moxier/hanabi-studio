//! The renderer: shadows, reflections, toon/realistic characters, stage, particles, beams,
//! the effect-node post chain and the final colour grade. OpenGL 3.3 through glow.

pub mod gl;
pub mod shaders;

use crate::effects::{EffectGraph, MatPreset, Node, NodeKind, ParticleInstance, ParticleSim, SCENE};
use crate::material::{LightKind, MaterialFx, Special};
use crate::grade::ColorGrade;
use crate::model::{SphereMode, Toon};
use crate::scene::{ModelInstance, Scene};
use gl::{as_bytes, compile, MsTarget, Program, Target};
use glam::{Mat4, Vec2, Vec3, Vec4};
use glow::HasContext;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct GpuModel {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    dyn_vbo: glow::Buffer,
    ebo: glow::Buffer,
    bone_tex: glow::Texture,
    nbones: i32,
    textures: Vec<Option<glow::Texture>>,
    toon_rows: Vec<f32>,
    /// Per material: centre in bind pose and the bone that carries it (for luminous lights).
    centroids: Vec<(Vec3, usize)>,
}

/// Inputs shared by every draw of one pass.
struct DrawCtx<'a> {
    view: Mat4,
    proj: Mat4,
    light_vp: Mat4,
    eye: Vec3,
    vp: Vec2,
    mirror: Mat4,
    clip: Vec4,
    style: &'a Node,
    use_shadow: bool,
    mode: i32,
    light_dir: Vec3,
    light_color: Vec3,
}

#[derive(Default)]
struct TargetSet {
    ms: Option<MsTarget>,
    scene_t: Option<Target>,
    reflect: Option<Target>,
    pool: Vec<Target>,
    bloom: Vec<Target>,
    output: Option<Target>,
}

/// Uniforms for the physically based Ray Lighting path.
#[derive(Clone, Copy, Debug)]
struct RayUniforms {
    sun: f32,
    skylight: f32,
    sky_top: Vec3,
    sky_hor: Vec3,
    ground: Vec3,
    spec: f32,
    emissive: f32,
}

#[derive(Default, Clone, Debug)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub triangles: u64,
    pub particles: usize,
    pub gpu_ms: f32,
}

pub struct FrameParams<'a> {
    pub scene: &'a Scene,
    pub graph: &'a EffectGraph,
    pub grade: &'a ColorGrade,
    pub width: i32,
    pub height: i32,
    /// Seconds since the previous rendered frame (drives particles).
    pub dt: f32,
    /// Absolute scene time in seconds (drives water and beams).
    pub time: f32,
    pub msaa: bool,
    pub show_floor: bool,
    /// 0 = interactive viewport, 1 = offline render (separate buffers).
    pub slot: usize,
    /// Projection to use instead of the camera's own (render-area guide in the viewport).
    pub proj: Option<Mat4>,
}

pub struct Renderer {
    model: Program,
    floor: Program,
    water: Program,
    sky: Program,
    particle: Program,
    beam: Program,
    copy: Program,
    bright: Program,
    down: Program,
    up: Program,
    bloom_mix: Program,
    fx: Program,
    grade: Program,
    empty_vao: glow::VertexArray,
    plane_vao: glow::VertexArray,
    _plane_vbo: glow::Buffer,
    quad_vbo: glow::Buffer,
    particle_vao: glow::VertexArray,
    particle_inst: glow::Buffer,
    beam_vao: glow::VertexArray,
    beam_inst: glow::Buffer,
    white: glow::Texture,
    black: glow::Texture,
    toon_atlas: glow::Texture,
    toon_pixels: Vec<u8>,
    toon_rows_used: usize,
    toon_lookup: HashMap<PathBuf, f32>,
    shadow: Target,
    models: HashMap<u64, GpuModel>,
    tex_cache: HashMap<PathBuf, Option<glow::Texture>>,
    /// The loaded colour LUT: (file, 3D texture, size, domain min, domain max).
    lut_tex: Option<(PathBuf, glow::Texture, usize, Vec3, Vec3)>,
    /// A LUT that couldn't be read, and why (shown on the Color page).
    pub lut_error: Option<(PathBuf, String)>,
    pub lut_title: String,
    ms: Option<MsTarget>,
    scene_t: Option<Target>,
    reflect: Option<Target>,
    pool: Vec<Target>,
    bloom: Vec<Target>,
    output: Option<Target>,
    spare: TargetSet,
    slot: usize,
    pub particles: HashMap<u32, ParticleSim>,
    pub stats: RenderStats,
    bone_scratch: Vec<f32>,
    pub front_ccw: bool,
    lum: Option<Target>,
    adapt: [Option<(Target, Target)>; 2],
    adapt_first: [bool; 2],
    prev_vp: [Option<Mat4>; 2],
    ray: Option<RayUniforms>,
    mat_table: HashMap<u64, Vec<MaterialFx>>,
    /// Compiled material node graphs (key = hash of generated code) and their texture files.
    variants: HashMap<u64, (Program, Vec<String>)>,
    mat_variant: HashMap<u64, Vec<Option<u64>>>,
    /// Problems compiling node-editor materials, shown in the Materials panel.
    pub material_errors: HashMap<u64, String>,
    light_u: [Vec<f32>; 4],
    n_lights: i32,
    frame_time: f32,
    /// Shader style for the whole scene and per-model overrides ("Assign looks").
    global_style: i32,
    model_style: HashMap<u64, i32>,
    lum_prog: Program,
    adapt_prog: Program,
}

fn toon_row(i: usize) -> [u8; 256 * 4] {
    // Ten original shading ramps: (shadow colour, softness, threshold)
    let ramps: [([f32; 3], f32, f32); 11] = [
        ([0.80, 0.80, 0.84], 0.04, 0.50),
        ([0.86, 0.76, 0.74], 0.06, 0.48),
        ([0.70, 0.72, 0.86], 0.05, 0.50),
        ([0.88, 0.70, 0.78], 0.10, 0.46),
        ([0.62, 0.58, 0.72], 0.04, 0.52),
        ([0.90, 0.86, 0.80], 0.20, 0.45),
        ([0.75, 0.75, 0.75], 0.02, 0.50),
        ([0.82, 0.66, 0.62], 0.12, 0.47),
        ([0.66, 0.78, 0.80], 0.08, 0.50),
        ([0.94, 0.90, 0.92], 0.25, 0.44),
        ([1.0, 1.0, 1.0], 0.0, 0.5),
    ];
    let (c, s, t) = ramps[i.min(10)];
    let mut row = [0u8; 1024];
    for x in 0..256 {
        let u = x as f32 / 255.0;
        let k = if s <= 0.0 { 1.0 } else { ((u - (t - s)) / (2.0 * s)).clamp(0.0, 1.0) };
        let k = k * k * (3.0 - 2.0 * k);
        for ch in 0..3 {
            row[x * 4 + ch] = ((c[ch] + (1.0 - c[ch]) * k) * 255.0) as u8;
        }
        row[x * 4 + 3] = 255;
    }
    row
}

pub use crate::findfile::find_texture;

pub fn decode_image(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok().or_else(|| {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        let fmt = match ext.as_str() {
            "tga" => image::ImageFormat::Tga,
            "dds" => image::ImageFormat::Dds,
            "bmp" | "sph" | "spa" => image::ImageFormat::Bmp,
            "png" => image::ImageFormat::Png,
            "jpg" | "jpeg" => image::ImageFormat::Jpeg,
            _ => return None,
        };
        image::load_from_memory_with_format(&bytes, fmt).ok()
    })?;
    let rgba = img.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        use shaders::*;
        let model = compile(gl, "model", MODEL_VS, &MODEL_FS.replace("//@CUSTOM_MATERIAL", crate::material::EMPTY_CUSTOM))?;
        let floor = compile(gl, "floor", FLOOR_VS, FLOOR_FS)?;
        let water = compile(gl, "water", FLOOR_VS, WATER_FS)?;
        let sky = compile(gl, "sky", FS_VS, SKY_FS)?;
        let particle = compile(gl, "particle", PARTICLE_VS, PARTICLE_FS)?;
        let beam = compile(gl, "beam", BEAM_VS, BEAM_FS)?;
        let copy = compile(gl, "copy", FS_VS, COPY_FS)?;
        let bright = compile(gl, "bright", FS_VS, BRIGHT_FS)?;
        let down = compile(gl, "down", FS_VS, DOWN_FS)?;
        let up = compile(gl, "up", FS_VS, UP_FS)?;
        let bloom_mix = compile(gl, "bloom_mix", FS_VS, BLOOM_MIX_FS)?;
        let fx = compile(gl, "fx", FS_VS, FX_FS)?;
        let grade = compile(gl, "grade", FS_VS, GRADE_FS)?;
        let lum_prog = compile(gl, "lum", FS_VS, LUM_FS)?;
        let adapt_prog = compile(gl, "adapt", FS_VS, ADAPT_FS)?;
        unsafe {
            let empty_vao = gl.create_vertex_array()?;
            // ground plane
            let plane_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(plane_vao));
            let plane_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(plane_vbo));
            let verts: [f32; 18] = [-1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0, -1.0, 0.0, -1.0, 1.0, 0.0, 1.0, -1.0, 0.0, 1.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&verts), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
            // instanced corner quad
            let quad_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
            let corners: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&corners), glow::STATIC_DRAW);

            let particle_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(particle_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            let particle_inst = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(particle_inst));
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 32, 0);
            gl.vertex_attrib_divisor(1, 1);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 32, 16);
            gl.vertex_attrib_divisor(2, 1);

            let beam_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(beam_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            let beam_inst = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(beam_inst));
            for k in 0..3u32 {
                gl.enable_vertex_attrib_array(1 + k);
                gl.vertex_attrib_pointer_f32(1 + k, 4, glow::FLOAT, false, 48, (k * 16) as i32);
                gl.vertex_attrib_divisor(1 + k, 1);
            }
            gl.bind_vertex_array(None);

            let white = gl::texture_rgba8(gl, 1, 1, &[255, 255, 255, 255], true, false);
            let black = gl::texture_rgba8(gl, 1, 1, &[0, 0, 0, 255], true, false);
            let mut toon_pixels = vec![255u8; 256 * 64 * 4];
            for i in 0..11 {
                toon_pixels[i * 1024..(i + 1) * 1024].copy_from_slice(&toon_row(i));
            }
            let toon_atlas = gl::texture_rgba8(gl, 256, 64, &toon_pixels, false, false);
            gl.bind_texture(glow::TEXTURE_2D, Some(toon_atlas));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);

            // Shadow map
            let shadow = Target::new(gl, 4, 4, glow::R8, true);
            let shadow = {
                shadow.delete(gl);
                Target::new(gl, 2048, 2048, glow::R8, true)
            };
            Ok(Self {
                model,
                floor,
                water,
                sky,
                particle,
                beam,
                copy,
                bright,
                down,
                up,
                bloom_mix,
                fx,
                grade,
                empty_vao,
                plane_vao,
                _plane_vbo: plane_vbo,
                quad_vbo,
                particle_vao,
                particle_inst,
                beam_vao,
                beam_inst,
                white,
                black,
                toon_atlas,
                toon_pixels,
                toon_rows_used: 11,
                toon_lookup: HashMap::new(),
                shadow,
                models: HashMap::new(),
                tex_cache: HashMap::new(),
                lut_tex: None,
                lut_error: None,
                lut_title: String::new(),
                ms: None,
                scene_t: None,
                reflect: None,
                pool: Vec::new(),
                bloom: Vec::new(),
                output: None,
                spare: TargetSet::default(),
                slot: 0,
                particles: HashMap::new(),
                stats: RenderStats::default(),
                bone_scratch: Vec::new(),
                front_ccw: false,
                lum: None,
                adapt: [None, None],
                adapt_first: [true, true],
                prev_vp: [None, None],
                ray: None,
                mat_table: HashMap::new(),
                variants: HashMap::new(),
                mat_variant: HashMap::new(),
                material_errors: HashMap::new(),
                light_u: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
                n_lights: 0,
                frame_time: 0.0,
                global_style: 0,
                model_style: HashMap::new(),
                lum_prog,
                adapt_prog,
            })
        }
    }

    fn toon_row_for(&mut self, gl: &glow::Context, path: &Path) -> f32 {
        if let Some(r) = self.toon_lookup.get(path) {
            return *r;
        }
        let row = if self.toon_rows_used < 64 {
            if let Some((w, h, px)) = decode_image(path) {
                let r = self.toon_rows_used;
                self.toon_rows_used += 1;
                // Toon images are vertical (lit at top): resample the centre column into a row.
                for x in 0..256 {
                    let y = ((1.0 - x as f32 / 255.0) * (h as f32 - 1.0)) as u32;
                    let o = ((y * w + w / 2) * 4) as usize;
                    for c in 0..4 {
                        self.toon_pixels[r * 1024 + x * 4 + c] = px.get(o + c).copied().unwrap_or(255);
                    }
                }
                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.toon_atlas));
                    gl.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        r as i32,
                        256,
                        1,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&self.toon_pixels[r * 1024..(r + 1) * 1024])),
                    );
                }
                (r as f32 + 0.5) / 64.0
            } else {
                10.5 / 64.0
            }
        } else {
            10.5 / 64.0
        };
        self.toon_lookup.insert(path.to_path_buf(), row);
        row
    }

    fn upload_model(&mut self, gl: &glow::Context, m: &ModelInstance) {
        let d = &m.data;
        let n = d.vertices.len();
        // pos3 normal3 uv2 bones4(i32) weights4 edge1 = 17 * 4 bytes
        let mut buf: Vec<u8> = Vec::with_capacity(n * 68);
        for v in &d.vertices {
            for f in v.pos.to_array().iter().chain(v.normal.to_array().iter()).chain(v.uv.to_array().iter()) {
                buf.extend_from_slice(&f.to_le_bytes());
            }
            for b in v.bones {
                buf.extend_from_slice(&b.max(0).to_le_bytes());
            }
            for w in v.weights {
                buf.extend_from_slice(&w.to_le_bytes());
            }
            buf.extend_from_slice(&v.edge_scale.to_le_bytes());
        }
        let base_dir = d.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        // Decode textures in parallel.
        let paths: Vec<Option<PathBuf>> = d
            .textures
            .iter()
            .map(|t| base_dir.as_ref().and_then(|b| find_texture(b, t)))
            .collect();
        let missing: Vec<PathBuf> = paths.iter().flatten().filter(|p| !self.tex_cache.contains_key(*p)).cloned().collect();
        let decoded: Vec<(PathBuf, Option<(u32, u32, Vec<u8>)>)> = missing.into_par_iter().map(|p| {
            let img = decode_image(&p);
            (p, img)
        }).collect();
        for (p, img) in decoded {
            let t = img.map(|(w, h, px)| gl::texture_rgba8(gl, w, h, &px, true, true));
            self.tex_cache.insert(p, t);
        }
        let mut textures: Vec<Option<glow::Texture>> = paths.iter().map(|p| p.as_ref().and_then(|p| self.tex_cache.get(p).copied().flatten())).collect();
        for (i, e) in d.embedded_textures.iter().enumerate() {
            if let Some((w, h, px)) = e {
                if i < textures.len() {
                    textures[i] = Some(gl::texture_rgba8(gl, *w, *h, px, true, true));
                }
            }
        }
        let toon_rows: Vec<f32> = d
            .materials
            .iter()
            .map(|mat| match &mat.toon {
                Toon::None => 10.5 / 64.0,
                Toon::Shared(i) => (*i as f32).min(9.0) / 64.0 + 0.5 / 64.0,
                Toon::Texture(t) => {
                    let p = paths.get(*t as usize).cloned().flatten();
                    match p {
                        Some(p) => self.toon_row_for(gl, &p),
                        None => {
                            // "toon01.bmp".."toon10.bmp" style names map to the built-in ramps.
                            let name = d.textures.get(*t as usize).map(|s| s.to_lowercase()).unwrap_or_default();
                            let num: String = name.chars().filter(|c| c.is_ascii_digit()).collect();
                            match num.parse::<usize>() {
                                Ok(k) if (1..=10).contains(&k) => (k as f32 - 1.0 + 0.5) / 64.0,
                                _ => 10.5 / 64.0,
                            }
                        }
                    }
                }
            })
            .collect();
        unsafe {
            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &buf, glow::STATIC_DRAW);
            let st = 68;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, st, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, st, 12);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, st, 24);
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_i32(3, 4, glow::INT, st, 32);
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, st, 48);
            gl.enable_vertex_attrib_array(5);
            gl.vertex_attrib_pointer_f32(5, 1, glow::FLOAT, false, st, 64);
            let dyn_vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(dyn_vbo));
            gl.buffer_data_size(glow::ARRAY_BUFFER, (n * 20) as i32, glow::DYNAMIC_DRAW);
            gl.enable_vertex_attrib_array(6);
            gl.vertex_attrib_pointer_f32(6, 3, glow::FLOAT, false, 20, 0);
            gl.enable_vertex_attrib_array(7);
            gl.vertex_attrib_pointer_f32(7, 2, glow::FLOAT, false, 20, 12);
            let ebo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&d.indices), glow::STATIC_DRAW);
            gl.bind_vertex_array(None);
            let bone_tex = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(bone_tex));
            let nb = d.bones.len().max(1) as i32;
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, 4, nb, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            let mut centroids = Vec::new();
            let mut start = 0usize;
            for mat in &d.materials {
                let end = (start + mat.index_count as usize).min(d.indices.len());
                let mut c = Vec3::ZERO;
                let mut votes: HashMap<usize, u32> = HashMap::new();
                for &i in &d.indices[start..end] {
                    if let Some(v) = d.vertices.get(i as usize) {
                        c += v.pos;
                        *votes.entry(v.bones[0].max(0) as usize).or_default() += 1;
                    }
                }
                let n = (end - start).max(1) as f32;
                let bone = votes.into_iter().max_by_key(|(_, k)| *k).map(|(b, _)| b).unwrap_or(0);
                centroids.push((c / n, bone));
                start = end;
            }
            self.models.insert(m.id, GpuModel { vao, vbo, dyn_vbo, ebo, bone_tex, nbones: nb, textures, toon_rows, centroids });
        }
    }

    /// Free GPU data for models that left the scene.
    fn gc_models(&mut self, gl: &glow::Context, scene: &Scene) {
        let alive: Vec<u64> = scene.models.iter().map(|m| m.id).collect();
        let dead: Vec<u64> = self.models.keys().filter(|k| !alive.contains(k)).copied().collect();
        for k in dead {
            if let Some(g) = self.models.remove(&k) {
                unsafe {
                    gl.delete_vertex_array(g.vao);
                    gl.delete_buffer(g.vbo);
                    gl.delete_buffer(g.dyn_vbo);
                    gl.delete_buffer(g.ebo);
                    gl.delete_texture(g.bone_tex);
                }
            }
        }
    }

    fn sync_model(&mut self, gl: &glow::Context, m: &ModelInstance) {
        if !self.models.contains_key(&m.id) {
            self.upload_model(gl, m);
        }
        let g = &self.models[&m.id];
        self.bone_scratch.clear();
        for mat in &m.skin {
            self.bone_scratch.extend_from_slice(&mat.to_cols_array());
        }
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(g.bone_tex));
            let nb = (m.skin.len() as i32).min(g.nbones);
            if nb > 0 {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    4,
                    nb,
                    glow::RGBA,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(as_bytes(&self.bone_scratch[..(nb as usize * 16)]))),
                );
            }
        }
    }

    fn upload_morphs(gl: &glow::Context, g: &GpuModel, m: &ModelInstance) {
        let mut buf: Vec<f32> = Vec::with_capacity(m.morph_offsets.len() * 5);
        for (o, u) in m.morph_offsets.iter().zip(m.uv_offsets.iter()) {
            buf.extend_from_slice(&[o.x, o.y, o.z, u.x, u.y]);
        }
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(g.dyn_vbo));
            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, as_bytes(&buf));
        }
    }

    fn select_slot(&mut self, slot: usize) {
        if slot == self.slot {
            return;
        }
        std::mem::swap(&mut self.ms, &mut self.spare.ms);
        std::mem::swap(&mut self.scene_t, &mut self.spare.scene_t);
        std::mem::swap(&mut self.reflect, &mut self.spare.reflect);
        std::mem::swap(&mut self.pool, &mut self.spare.pool);
        std::mem::swap(&mut self.bloom, &mut self.spare.bloom);
        std::mem::swap(&mut self.output, &mut self.spare.output);
        self.slot = slot;
    }

    /// The viewport's output texture (stable across resizes).
    pub fn viewport_texture(&self) -> Option<glow::Texture> {
        if self.slot == 0 { self.output.as_ref().map(|o| o.color) } else { self.spare.output.as_ref().map(|o| o.color) }
    }

    fn ensure_targets(&mut self, gl: &glow::Context, w: i32, h: i32, msaa: bool) {
        let samples = if msaa { 4 } else { 0 };
        let need = match &self.scene_t {
            Some(t) => t.w != w || t.h != h,
            None => true,
        } || (self.ms.is_some() != (samples > 0));
        if !need {
            return;
        }
        if let Some(t) = self.ms.take() {
            t.delete(gl);
        }
        for t in [self.scene_t.take(), self.reflect.take()].into_iter().flatten() {
            t.delete(gl);
        }
        for t in self.pool.drain(..) {
            t.delete(gl);
        }
        for t in self.bloom.drain(..) {
            t.delete(gl);
        }
        if samples > 0 {
            self.ms = Some(MsTarget::new(gl, w, h, samples));
        }
        let mut st = Target::new(gl, w, h, glow::RGBA16F, true);
        st.add_gbuffer(gl);
        self.scene_t = Some(st);
        self.reflect = Some(Target::new(gl, (w / 2).max(1), (h / 2).max(1), glow::RGBA16F, true));
        match &mut self.output {
            Some(o) => o.resize(gl, w, h),
            None => self.output = Some(Target::new(gl, w, h, glow::RGBA8, false)),
        }
        let (mut bw, mut bh) = (w / 2, h / 2);
        for _ in 0..6 {
            if bw < 4 || bh < 4 {
                break;
            }
            self.bloom.push(Target::new(gl, bw, bh, glow::RGBA16F, false));
            bw /= 2;
            bh /= 2;
        }
    }

    fn pool_target(&mut self, gl: &glow::Context, i: usize) -> usize {
        let (w, h) = self.scene_t.as_ref().map(|t| (t.w, t.h)).unwrap_or((4, 4));
        while self.pool.len() <= i {
            self.pool.push(Target::new(gl, w, h, glow::RGBA16F, false));
        }
        i
    }

    fn draw_fullscreen(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    fn set_common(&self, gl: &glow::Context, p: &Program, c: &DrawCtx) {
        p.use_(gl);
        p.m4(gl, "u_view", &c.view);
        p.m4(gl, "u_proj", &c.proj);
        p.m4(gl, "u_light_vp", &c.light_vp);
        p.v3(gl, "u_light_dir", c.light_dir);
        p.v3(gl, "u_light_color", c.light_color);
        p.v3(gl, "u_eye", c.eye);
        p.v2(gl, "u_viewport", c.vp);
        p.m4(gl, "u_mirror", &c.mirror);
        p.v4(gl, "u_clip", c.clip);
        p.i(gl, "u_style", self.global_style);
        if let Some(r) = &self.ray {
            p.f(gl, "u_sun", r.sun);
            p.f(gl, "u_skylight", r.skylight);
            p.v3(gl, "u_sky_top", r.sky_top);
            p.v3(gl, "u_sky_hor", r.sky_hor);
            p.v3(gl, "u_ground", r.ground);
        }
        let st = c.style;
        p.f(gl, "u_rim", st.f("Rim light"));
        p.v3(gl, "u_rim_color", st.c("Rim colour"));
        p.f(gl, "u_spec_boost", st.f("Specular boost"));
        p.f(gl, "u_softness", st.f("Shadow softness"));
        p.v3(gl, "u_shadow_tint", st.c("Shadow tint"));
        p.f(gl, "u_skin", st.f("Skin glow"));
        p.f(gl, "u_sat", st.f("Saturation"));
        p.f(gl, "u_ambient_k", st.f("Ambient light"));
        p.tex(gl, "u_toon", 2, Some(self.toon_atlas));
        p.tex(gl, "u_shadow", 3, self.shadow.depth);
        p.i(gl, "u_use_shadow", c.use_shadow as i32);
        p.i(gl, "u_has_toon", 1);
        p.f(gl, "u_time", self.frame_time);
        p.i(gl, "u_nlights", if c.mode == 2 { 0 } else { self.n_lights });
        p.v4_array(gl, "u_lpos", &self.light_u[0]);
        p.v4_array(gl, "u_ldir", &self.light_u[1]);
        p.v4_array(gl, "u_lcol", &self.light_u[2]);
        p.v4_array(gl, "u_lcone", &self.light_u[3]);
    }

    fn set_model(&self, gl: &glow::Context, p: &Program, m: &ModelInstance, g: &GpuModel, mode: i32) {
        p.m4(gl, "u_model", &m.world);
        p.i(gl, "u_style", if mode == 2 { self.global_style } else { self.model_style.get(&m.id).copied().unwrap_or(self.global_style) });
        p.tex(gl, "u_bones", 4, Some(g.bone_tex));
        p.f(gl, "u_alpha_mul", if m.is_accessory() { m.acc_opacity } else { 1.0 });
        p.i(gl, "u_mode", if mode == 2 { 2 } else { 0 });
    }

    fn map_tex(&self, path: &Option<String>) -> Option<glow::Texture> {
        path.as_ref().and_then(|p| self.tex_cache.get(&PathBuf::from(p)).copied().flatten())
    }

    #[allow(clippy::too_many_arguments)]
    fn set_material(&self, gl: &glow::Context, p: &Program, m: &ModelInstance, g: &GpuModel, mi: usize, use_shadow: bool, variant: Option<u64>) {
        let mat = &m.data.materials[mi];
        let ms = &m.materials[mi];
        p.v4(gl, "u_diffuse", ms.diffuse);
        p.v3(gl, "u_ambient", ms.ambient);
        p.v3(gl, "u_specular", ms.specular);
        p.f(gl, "u_spec_power", ms.specular_power);
        p.v4(gl, "u_tex_tint", ms.texture_tint);
        let fx = self.mat_table.get(&m.id).and_then(|v| v.get(mi)).cloned().unwrap_or_default();
        let (rs, re) = self.ray.map(|r| (r.spec, r.emissive.max(0.0))).unwrap_or((1.0, 1.0));
        p.f(gl, "u_rough", fx.rough);
        p.f(gl, "u_metal", fx.metal);
        p.f(gl, "u_sss", fx.sss);
        p.f(gl, "u_emis", fx.emissive * if self.ray.is_some() { re / 2.0 } else { 1.0 });
        p.v3(gl, "u_emis_col", Vec3::from(fx.emissive_color));
        p.f(gl, "u_matspec", fx.spec * rs);
        p.i(gl, "u_mat_toon", fx.is_toon() as i32);
        p.i(gl, "u_special", fx.special.index());
        p.v3(gl, "u_mat_tint", Vec3::from(fx.tint));
        p.f(gl, "u_glass_op", fx.glass_opacity);
        p.f(gl, "u_wet", fx.wet);
        p.f(gl, "u_rain", fx.rain);
        p.f(gl, "u_ripple_scale", fx.ripple_scale);
        p.f(gl, "u_ripple_speed", fx.ripple_speed);
        p.f(gl, "u_fur_density", fx.fur_density);
        p.f(gl, "u_uv_scale", fx.uv_scale);
        p.f(gl, "u_shell", 0.0);
        p.f(gl, "u_shell_len", fx.fur_length);
        p.f(gl, "u_shell_droop", fx.fur_droop);
        for (name, flag, unit, path) in [
            ("u_nmap", "u_has_nmap", 6u32, &fx.normal_map),
            ("u_hmap", "u_has_hmap", 7, &fx.height_map),
            ("u_mmap", "u_has_mmap", 8, &fx.metal_map),
            ("u_rmap", "u_has_rmap", 9, &fx.rough_map),
        ] {
            let t = self.map_tex(path);
            p.i(gl, flag, t.is_some() as i32);
            p.tex(gl, name, unit, Some(t.unwrap_or(self.white)));
        }
        p.f(gl, "u_nmap_k", fx.normal_strength);
        p.f(gl, "u_hmap_k", fx.height_scale);
        if let Some((_, texs)) = variant.and_then(|k| self.variants.get(&k)) {
            for i in 0..4 {
                let t = texs.get(i).and_then(|t| self.tex_cache.get(&PathBuf::from(t)).copied().flatten());
                p.tex(gl, &format!("u_ctex{i}"), 10 + i as u32, Some(t.unwrap_or(self.white)));
            }
        }
        let tex = if mat.texture >= 0 { g.textures.get(mat.texture as usize).copied().flatten() } else { None };
        p.i(gl, "u_has_tex", tex.is_some() as i32);
        p.tex(gl, "u_tex", 0, Some(tex.unwrap_or(self.white)));
        let sph = if mat.sphere >= 0 { g.textures.get(mat.sphere as usize).copied().flatten() } else { None };
        let smode = match (sph, mat.sphere_mode) {
            (Some(_), SphereMode::Multiply) => 1,
            (Some(_), SphereMode::Add) => 2,
            _ => 0,
        };
        p.i(gl, "u_sphere_mode", smode);
        p.tex(gl, "u_sphere", 1, Some(sph.unwrap_or(self.black)));
        p.f(gl, "u_toon_row", g.toon_rows.get(mi).copied().unwrap_or(10.5 / 64.0));
        p.i(gl, "u_use_shadow", (use_shadow && mat.receive_shadow) as i32);
    }

    /// Draw every visible model for one pass (0 colour, 2 shadow). Returns (draw calls, triangles).
    #[allow(clippy::too_many_arguments)]
    fn draw_models(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        view: &Mat4,
        proj: &Mat4,
        light_vp: &Mat4,
        eye: Vec3,
        vp: Vec2,
        mode: i32,
        mirror: Option<(Mat4, Vec4)>,
        style: &Node,
        use_shadow: bool,
    ) {
        let (mirror_m, clip) = mirror.unwrap_or((Mat4::IDENTITY, Vec4::new(0.0, 0.0, 0.0, 1.0)));
        let ctx = DrawCtx {
            view: *view,
            proj: *proj,
            light_vp: *light_vp,
            eye,
            vp,
            mirror: mirror_m,
            clip,
            style,
            use_shadow,
            mode,
            light_dir: scene.light.direction.normalize_or(Vec3::NEG_Y),
            light_color: scene.light.color,
        };
        let (mut calls, mut tris) = (0u32, 0u64);
        self.set_common(gl, &self.model, &ctx);
        let mut prepared: Vec<u64> = Vec::new();
        unsafe {
            gl.front_face(if self.front_ccw { glow::CCW } else { glow::CW });
        }
        for m in &scene.models {
            if !m.shown() {
                continue;
            }
            let Some(g) = self.models.get(&m.id) else { continue };
            let mut cur: Option<Option<u64>> = None;
            unsafe {
                gl.bind_vertex_array(Some(g.vao));
            }
            let d = &m.data;
            let mut offset = 0u32;
            for (mi, mat) in d.materials.iter().enumerate() {
                let count = mat.index_count;
                let ms = &m.materials[mi];
                let skip = count == 0 || ms.diffuse.w <= 0.001 || (mode == 2 && !mat.cast_shadow);
                if !skip {
                    let key = if mode == 2 { None } else { self.mat_variant.get(&m.id).and_then(|v| v.get(mi)).copied().flatten() };
                    let p: &Program = key.and_then(|k| self.variants.get(&k).map(|v| &v.0)).unwrap_or(&self.model);
                    if cur != Some(key) {
                        if let Some(k) = key {
                            if !prepared.contains(&k) {
                                self.set_common(gl, p, &ctx);
                                prepared.push(k);
                            }
                        }
                        p.use_(gl);
                        self.set_model(gl, p, m, g, mode);
                        cur = Some(key);
                    }
                    unsafe {
                        if mat.double_sided || mirror.is_some() || mode == 2 {
                            gl.disable(glow::CULL_FACE);
                        } else {
                            gl.enable(glow::CULL_FACE);
                            gl.cull_face(glow::BACK);
                        }
                    }
                    if mode != 2 {
                        self.set_material(gl, p, m, g, mi, use_shadow, key);
                    }
                    unsafe {
                        gl.draw_elements(glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (offset * 4) as i32);
                    }
                    calls += 1;
                    tris += count as u64 / 3;
                    // fur and grass: extra shells pushed out along the surface
                    if mode == 0 && mirror.is_none() {
                        if let Some(fx) = self.mat_table.get(&m.id).and_then(|v| v.get(mi)) {
                            if matches!(fx.special, Special::Fur | Special::Grass) {
                                let layers = fx.fur_layers.clamp(2, 48);
                                unsafe {
                                    gl.disable(glow::CULL_FACE);
                                }
                                for k in 1..=layers {
                                    p.f(gl, "u_shell", k as f32 / layers as f32);
                                    unsafe {
                                        gl.draw_elements(glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (offset * 4) as i32);
                                    }
                                    calls += 1;
                                    tris += count as u64 / 3;
                                }
                                p.f(gl, "u_shell", 0.0);
                            }
                        }
                    }
                }
                offset += count;
            }
            // Outline (edge) pass, always with the standard program
            if mode == 0 && scene.stage.edges && mirror.is_none() && !m.is_accessory() {
                let p = &self.model;
                p.use_(gl);
                self.set_model(gl, p, m, g, mode);
                p.i(gl, "u_mode", 1);
                unsafe {
                    gl.enable(glow::CULL_FACE);
                    gl.cull_face(glow::FRONT);
                }
                let mut offset = 0u32;
                for (mi, mat) in d.materials.iter().enumerate() {
                    let ms = &m.materials[mi];
                    let glassy = self.mat_table.get(&m.id).and_then(|v| v.get(mi)).map(|f| matches!(f.special, Special::Glass | Special::Water)).unwrap_or(false);
                    if mat.edge && !glassy && mat.index_count > 0 && ms.diffuse.w > 0.001 && ms.edge_size > 0.0 {
                        p.v4(gl, "u_edge_color", ms.edge_color);
                        p.f(gl, "u_edge_px", ms.edge_size * m.edge_width * (vp.y / 720.0).max(0.5) * 1.7);
                        unsafe {
                            gl.draw_elements(glow::TRIANGLES, mat.index_count as i32, glow::UNSIGNED_INT, (offset * 4) as i32);
                        }
                        calls += 1;
                    }
                    offset += mat.index_count;
                }
                unsafe {
                    gl.cull_face(glow::BACK);
                }
                p.i(gl, "u_mode", 0);
            }
        }
        unsafe {
            gl.disable(glow::CULL_FACE);
        }
        self.stats.draw_calls += calls;
        self.stats.triangles += tris;
    }

    /// Load a picture file used by a material (normal / height / metal / roughness / node textures).
    fn ensure_file_tex(&mut self, gl: &glow::Context, path: &str) {
        let pb = PathBuf::from(path);
        if self.tex_cache.contains_key(&pb) {
            return;
        }
        let t = decode_image(&pb).map(|(w, h, px)| gl::texture_rgba8(gl, w, h, &px, true, true));
        self.tex_cache.insert(pb, t);
    }

    /// Resolve materials, compile node-editor materials, load maps and gather lights for this frame.
    fn prepare_materials(&mut self, gl: &glow::Context, scene: &Scene, graph: &EffectGraph, default_rough: f32, auto: bool, skin_k: f32, time: f32) {
        self.frame_time = time;
        self.mat_table.clear();
        self.mat_variant.clear();
        let mut paths: Vec<String> = Vec::new();
        let mut graphs: Vec<(u64, usize, crate::material::MatGraph)> = Vec::new();
        for m in &scene.models {
            let mut v = Vec::new();
            for (mi, mat) in m.data.materials.iter().enumerate() {
                let fx = match graph.mat_fx.get(&mat.name) {
                    Some(f) => f.clone(),
                    None => {
                        let preset = graph.materials.get(&mat.name).copied().unwrap_or(if auto { MatPreset::guess(&mat.name) } else { MatPreset::Cloth });
                        let mut f = MaterialFx::from_preset(preset, default_rough);
                        f.sss *= skin_k;
                        if !graph.materials.contains_key(&mat.name) && preset == MatPreset::Cloth {
                            let shine = (mat.specular_power / 100.0).clamp(0.0, 1.0) * mat.specular.length().min(1.0);
                            f.rough = (f.rough - shine * 0.3).max(0.15);
                        }
                        f
                    }
                };
                paths.extend(fx.texture_paths());
                if let Some(gr) = &fx.graph {
                    graphs.push((m.id, mi, gr.clone()));
                }
                v.push(fx);
            }
            self.mat_table.insert(m.id, v);
        }
        for p in paths {
            self.ensure_file_tex(gl, &p);
        }
        for (mid, mi, gr) in graphs {
            let (code, texs) = gr.compile();
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            code.hash(&mut h);
            let key = h.finish();
            if !self.variants.contains_key(&key) && !self.material_errors.contains_key(&key) {
                let fs = shaders::MODEL_FS.replace("//@CUSTOM_MATERIAL", &code);
                match compile(gl, "custom material", shaders::MODEL_VS, &fs) {
                    Ok(p) => {
                        self.variants.insert(key, (p, texs.clone()));
                    }
                    Err(e) => {
                        self.material_errors.insert(key, e);
                    }
                }
            }
            for t in &texs {
                self.ensure_file_tex(gl, t);
            }
            let slot = self.mat_variant.entry(mid).or_insert_with(Vec::new);
            if slot.len() <= mi {
                slot.resize(mi + 1, None);
            }
            if self.variants.contains_key(&key) {
                slot[mi] = Some(key);
            }
        }
        // Lights: placed lights, then glowing materials that light their surroundings.
        let mut lp = Vec::new();
        let mut ld = Vec::new();
        let mut lc = Vec::new();
        let mut lk = Vec::new();
        for l in graph.lights.iter().filter(|l| l.enabled) {
            if lp.len() >= 32 {
                break;
            }
            let ty = match l.kind {
                LightKind::Point => 0.0,
                LightKind::Spot => 1.0,
                LightKind::Directional => 2.0,
            };
            let dir = l.direction();
            let outer = l.cone.clamp(1.0, 89.0).to_radians();
            let inner = (l.cone * (1.0 - l.softness.clamp(0.0, 1.0))).clamp(0.5, 89.0).to_radians();
            let c = Vec3::from(l.color) * l.intensity;
            lp.extend_from_slice(&[l.position[0], l.position[1], l.position[2], ty]);
            ld.extend_from_slice(&[dir.x, dir.y, dir.z, l.range]);
            lc.extend_from_slice(&[c.x, c.y, c.z, outer.cos()]);
            lk.extend_from_slice(&[inner.cos(), 0.0, 0.0, 0.0]);
        }
        for m in &scene.models {
            let (Some(g), Some(fxs)) = (self.models.get(&m.id), self.mat_table.get(&m.id)) else { continue };
            for (mi, fx) in fxs.iter().enumerate() {
                if !fx.emit_light || fx.emissive <= 0.0 || lp.len() >= 32 || !m.shown() {
                    continue;
                }
                let Some(&(c0, bone)) = g.centroids.get(mi) else { continue };
                let skin = m.skin.get(bone).copied().unwrap_or(Mat4::IDENTITY);
                let pos = (m.world * skin).transform_point3(c0);
                let c = Vec3::from(fx.emissive_color) * fx.emissive;
                lp.extend_from_slice(&[pos.x, pos.y, pos.z, 0.0]);
                ld.extend_from_slice(&[0.0, -1.0, 0.0, fx.light_range]);
                lc.extend_from_slice(&[c.x, c.y, c.z, 0.0]);
                lk.extend_from_slice(&[1.0, 0.0, 0.0, 0.0]);
            }
        }
        self.n_lights = (lp.len() / 4) as i32;
        self.light_u = [lp, ld, lc, lk];
    }

    fn beam_instances(node: &Node, time: f32) -> Vec<[f32; 12]> {
        let count = node.f("Count").round().max(1.0) as usize;
        let color = node.c("Colour");
        let len = node.f("Length");
        let width = node.f("Width");
        let speed = node.f("Sweep speed");
        let spread = node.f("Spread");
        let bright = node.f("Brightness");
        let oh = node.f("Origin height");
        let pattern = node.i("Pattern");
        let rainbow = node.b("Rainbow");
        let mut v = Vec::with_capacity(count);
        for i in 0..count {
            let k = i as f32 / count as f32;
            let ph = time * speed + k * std::f32::consts::TAU;
            let (start, dir) = match pattern {
                1 => {
                    let x = (k - 0.5) * 50.0;
                    (Vec3::new(x, 0.0, 25.0), Vec3::new((ph).sin() * spread, 1.0, (ph * 0.7).cos() * spread * 0.5 - 0.2))
                }
                2 => {
                    let x = (k - 0.5) * 70.0;
                    (Vec3::new(x, oh * 0.4 + (i % 3) as f32 * 4.0, 40.0), Vec3::new((ph * 1.3).sin() * spread * 0.6, (ph * 0.9).cos() * 0.25, -1.0))
                }
                3 => {
                    let a = k * std::f32::consts::TAU + time * speed * 0.3;
                    (Vec3::new(a.cos() * 30.0, oh, a.sin() * 30.0), Vec3::new(-a.cos() * spread * 0.6, -1.0, -a.sin() * spread * 0.6))
                }
                _ => {
                    let x = (k - 0.5) * 60.0;
                    (Vec3::new(x, oh, 20.0), Vec3::new((ph).sin() * spread - x * 0.01, -1.0, (ph * 0.8).cos() * spread * 0.7 - 0.25))
                }
            };
            let end = start + dir.normalize() * len;
            let c = if rainbow {
                let h = (k + time * 0.1).fract() * 6.0;
                let x = 1.0 - ((h % 2.0) - 1.0).abs();
                let rgb = match h as i32 {
                    0 => Vec3::new(1.0, x, 0.0),
                    1 => Vec3::new(x, 1.0, 0.0),
                    2 => Vec3::new(0.0, 1.0, x),
                    3 => Vec3::new(0.0, x, 1.0),
                    4 => Vec3::new(x, 0.0, 1.0),
                    _ => Vec3::new(1.0, 0.0, x),
                };
                rgb * 0.7 + color * 0.3
            } else {
                color
            };
            v.push([start.x, start.y, start.z, width, end.x, end.y, end.z, 0.0, c.x, c.y, c.z, bright * 0.25]);
        }
        v
    }

    /// Render one frame of the stage at `width x height`; returns the 8-bit output texture.
    pub fn render(&mut self, gl: &glow::Context, fp: &FrameParams) -> glow::Texture {
        let t0 = std::time::Instant::now();
        self.stats = RenderStats::default();
        let (w, h) = (fp.width.max(16), fp.height.max(16));
        self.select_slot(fp.slot);
        self.gc_models(gl, fp.scene);
        self.ensure_targets(gl, w, h, fp.msaa);
        for m in &fp.scene.models {
            self.sync_model(gl, m);
            if m.morph_dirty {
                Self::upload_morphs(gl, &self.models[&m.id], m);
            }
        }
        let scene = fp.scene;
        let graph = fp.graph;
        let aspect = w as f32 / h as f32;
        let view = scene.camera.view();
        let proj = fp.proj.unwrap_or_else(|| scene.camera.proj(aspect));
        let eye = scene.camera.eye();
        let vp_size = Vec2::new(w as f32, h as f32);
        let default_style = Node::new(0, NodeKind::ShadingStyle, [0.0, 0.0]);
        let mut style = graph.fx_of(NodeKind::ShadingStyle).cloned().unwrap_or(default_style);
        if graph.fx_of(NodeKind::ShadingStyle).is_none() {
            // Classic toon for the scene; rim and skin glow keep their defaults for models
            // assigned a polished or bright look.
            style.set("Style", crate::effects::PVal::I(0));
        }

        // Ray Lighting: light energy, sky light and the material table for this frame.
        let ray_node = graph.fx_of(NodeKind::RayLighting).cloned();
        let sky_node = graph.fx_of(NodeKind::Sky).cloned();
        self.global_style = if ray_node.is_some() { 4 } else { style.i("Style") as i32 };
        self.model_style.clear();
        for m in &scene.models {
            if let Some(look) = graph.model_looks.get(m.name()) {
                self.model_style.insert(m.id, look.style());
            }
        }
        // A model can be "Realistic" even without a Ray Lighting node: use its default settings.
        let any_pbr = self.global_style == 4 || self.model_style.values().any(|&s| s == 4);
        let ray_src = ray_node.clone().or_else(|| any_pbr.then(|| Node::new(0, NodeKind::RayLighting, [0.0, 0.0])));
        self.ray = ray_src.as_ref().map(|r| {
            let (top, hor, ground) = match &sky_node {
                Some(s) => (s.c("Top"), s.c("Horizon"), s.c("Bottom").lerp(r.c("Ground colour"), 0.5)),
                None => (r.c("Sky colour"), r.c("Sky colour").lerp(Vec3::splat(0.85), 0.45), r.c("Ground colour")),
            };
            RayUniforms { sun: r.f("Sun strength"), skylight: r.f("Sky light"), sky_top: top, sky_hor: hor, ground, spec: r.f("Specular"), emissive: r.f("Emissive") }
        });
        let default_rough = ray_node.as_ref().map(|r| r.f("Roughness")).unwrap_or(0.55);
        let auto = ray_node.as_ref().map(|r| r.b("Auto materials")).unwrap_or(true);
        let skin_k = ray_node.as_ref().map(|r| r.f("Skin scattering")).unwrap_or(0.6);
        self.prepare_materials(gl, scene, graph, default_rough, auto, skin_k, fp.time);

        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
        }

        // ---- shadow map
        let ldir = scene.light.direction.normalize_or(Vec3::NEG_Y);
        let center = Vec3::new(scene.camera.target.x, 8.0, scene.camera.target.z);
        let ext = scene.stage.shadow_distance.max(10.0);
        let lup = if ldir.y.abs() > 0.95 { Vec3::Z } else { Vec3::Y };
        let light_view = Mat4::look_at_rh(center - ldir * 300.0, center, lup);
        let light_proj = Mat4::orthographic_rh_gl(-ext, ext, -ext, ext, 1.0, 700.0);
        let light_vp = light_proj * light_view;
        let use_shadow = scene.stage.self_shadow || scene.stage.ground_shadow;
        if use_shadow {
            self.shadow.bind(gl);
            unsafe {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LESS);
                gl.clear_color(1.0, 1.0, 1.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.5, 3.0);
            }
            self.draw_models(gl, scene, &light_view, &light_proj, &light_vp, eye, vp_size, 2, None, &style, false);
            unsafe {
                gl.disable(glow::POLYGON_OFFSET_FILL);
            }
        }

        let water = graph.fx_of(NodeKind::Water).cloned();
        let mirror_floor = graph.fx_of(NodeKind::MirrorFloor).cloned();
        let sky = graph.fx_of(NodeKind::Sky).cloned();
        let bg = Vec3::from(scene.stage.background);
        let (sky_top, sky_hor, sky_bot, stars) = match &sky {
            Some(s) => (s.c("Top"), s.c("Horizon"), s.c("Bottom"), s.f("Stars")),
            None => (bg * 1.15, bg, bg * 0.8, 0.0),
        };
        let inv_vp = (proj * view).inverse();

        // ---- planar reflection (water or mirror floor)
        let plane_h = water.as_ref().map(|w| w.f("Level")).unwrap_or(0.0);
        let want_reflect = water.as_ref().map(|w| w.f("Reflection") > 0.0).unwrap_or(false)
            || mirror_floor.as_ref().map(|m| m.f("Reflection") > 0.0).unwrap_or(false);
        if want_reflect {
            let r = self.reflect.as_ref().unwrap();
            r.bind(gl);
            unsafe {
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                gl.enable(glow::DEPTH_TEST);
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            }
            let mirror = Mat4::from_translation(Vec3::new(0.0, plane_h, 0.0))
                * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
                * Mat4::from_translation(Vec3::new(0.0, -plane_h, 0.0));
            let rvp = Vec2::new(r.w as f32, r.h as f32);
            self.draw_models(gl, scene, &view, &proj, &light_vp, eye, rvp, 0, Some((mirror, Vec4::new(0.0, -1.0, 0.0, plane_h + 0.05))), &style, false);
            unsafe {
                gl.disable(glow::BLEND);
            }
        }

        // ---- main scene
        match &self.ms {
            Some(ms) => ms.bind(gl),
            None => self.scene_t.as_ref().unwrap().bind(gl),
        }
        unsafe {
            gl.clear_color(bg.x, bg.y, bg.z, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.clear_buffer_f32_slice(glow::COLOR, 1, &[0.0, 0.0, 0.0, 1.0]);
            gl.disable(glow::DEPTH_TEST);
            gl.color_mask_draw_buffer(1, false, false, false, false);
        }
        // sky
        self.sky.use_(gl);
        self.sky.m4(gl, "u_inv_vp", &inv_vp);
        self.sky.v3(gl, "u_top", sky_top);
        self.sky.v3(gl, "u_horizon", sky_hor);
        self.sky.v3(gl, "u_bottom", sky_bot);
        self.sky.f(gl, "u_stars", stars);
        self.draw_fullscreen(gl);
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.enable(glow::BLEND);
            gl.disable_draw_buffer(glow::BLEND, 1);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.color_mask_draw_buffer(1, true, true, true, true);
        }
        // floor
        if fp.show_floor && water.is_none() {
            let f = &self.floor;
            f.use_(gl);
            f.m4(gl, "u_view", &view);
            f.m4(gl, "u_proj", &proj);
            f.m4(gl, "u_light_vp", &light_vp);
            f.f(gl, "u_size", 400.0);
            f.f(gl, "u_height", 0.0);
            f.i(gl, "u_grid", scene.stage.show_grid as i32);
            let floor_col = if sky.is_some() { sky_bot * 0.9 + Vec3::splat(0.03) } else { bg * 1.35 + Vec3::splat(0.02) };
            f.v3(gl, "u_color", floor_col);
            f.v3(gl, "u_eye", eye);
            f.tex(gl, "u_shadow", 3, self.shadow.depth);
            f.i(gl, "u_use_shadow", scene.stage.ground_shadow as i32);
            f.f(gl, "u_shadow_dark", 0.55);
            let floor_rough = match &mirror_floor {
                Some(mf) => (0.6 - mf.f("Reflection") * 0.6).max(0.04),
                None => 0.85,
            };
            f.f(gl, "u_floor_rough", floor_rough);
            f.i(gl, "u_nlights", self.n_lights);
            f.v4_array(gl, "u_lpos", &self.light_u[0]);
            f.v4_array(gl, "u_ldir", &self.light_u[1]);
            f.v4_array(gl, "u_lcol", &self.light_u[2]);
            f.v4_array(gl, "u_lcone", &self.light_u[3]);
            f.tex(gl, "u_reflect", 5, self.reflect.as_ref().map(|r| r.color));
            match &mirror_floor {
                Some(mf) => {
                    f.f(gl, "u_reflect_k", mf.f("Reflection"));
                    f.f(gl, "u_reflect_blur", mf.f("Blur"));
                    f.v3(gl, "u_reflect_tint", mf.c("Tint"));
                }
                None => f.f(gl, "u_reflect_k", 0.0),
            }
            unsafe {
                gl.depth_mask(false);
                gl.bind_vertex_array(Some(self.plane_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
                gl.depth_mask(true);
            }
            self.stats.draw_calls += 1;
        }
        // models
        self.draw_models(gl, scene, &view, &proj, &light_vp, eye, vp_size, 0, None, &style, scene.stage.self_shadow);
        unsafe {
            gl.enable(glow::BLEND);
            gl.disable_draw_buffer(glow::BLEND, 1);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
        // water
        if let Some(wn) = &water {
            let p = &self.water;
            p.use_(gl);
            p.m4(gl, "u_view", &view);
            p.m4(gl, "u_proj", &proj);
            p.m4(gl, "u_light_vp", &light_vp);
            p.f(gl, "u_size", wn.f("Size"));
            p.f(gl, "u_height", wn.f("Level"));
            p.v3(gl, "u_eye", eye);
            p.v3(gl, "u_light_dir", ldir);
            p.v3(gl, "u_light_color", scene.light.color * 1.6);
            p.f(gl, "u_time", fp.time);
            p.v3(gl, "u_deep", wn.c("Deep colour"));
            p.v3(gl, "u_shallow", wn.c("Shallow colour"));
            p.f(gl, "u_wave", wn.f("Wave size"));
            p.f(gl, "u_speed", wn.f("Wave speed"));
            p.f(gl, "u_reflect_k", wn.f("Reflection"));
            p.f(gl, "u_clarity", wn.f("Clarity"));
            p.f(gl, "u_sparkle", wn.f("Sparkle"));
            p.v3(gl, "u_sky", sky_hor);
            p.tex(gl, "u_reflect", 5, self.reflect.as_ref().map(|r| r.color));
            unsafe {
                gl.bind_vertex_array(Some(self.plane_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
            }
            self.stats.draw_calls += 1;
        }
        // beams and particles (no depth write)
        let right = view.row(0).truncate();
        let upv = view.row(1).truncate();
        unsafe {
            gl.depth_mask(false);
            gl.color_mask_draw_buffer(1, false, false, false, false);
        }
        for node in graph.active_fx() {
            match node.kind {
                NodeKind::LightBeams => {
                    let inst = Self::beam_instances(node, fp.time);
                    let p = &self.beam;
                    p.use_(gl);
                    p.m4(gl, "u_view", &view);
                    p.m4(gl, "u_proj", &proj);
                    p.v3(gl, "u_eye", eye);
                    unsafe {
                        gl.blend_func(glow::ONE, glow::ONE);
                        gl.bind_vertex_array(Some(self.beam_vao));
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.beam_inst));
                        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&inst), glow::STREAM_DRAW);
                        gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, inst.len() as i32);
                    }
                    self.stats.draw_calls += 1;
                }
                NodeKind::Particles => {
                    let sim = self.particles.entry(node.id).or_insert_with(ParticleSim::new);
                    sim.update(node, fp.dt.clamp(0.0, 0.1));
                    let kind = node.i("Kind");
                    let n = sim.instances.len();
                    self.stats.particles += n;
                    let inst: &[ParticleInstance] = &sim.instances;
                    let p = &self.particle;
                    p.use_(gl);
                    p.m4(gl, "u_view", &view);
                    p.m4(gl, "u_proj", &proj);
                    p.v3(gl, "u_right", right);
                    p.v3(gl, "u_up", upv);
                    p.i(gl, "u_shape", kind as i32);
                    unsafe {
                        if matches!(kind, 1 | 2 | 4 | 5) {
                            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                        } else {
                            gl.blend_func(glow::SRC_ALPHA, glow::ONE);
                        }
                        gl.bind_vertex_array(Some(self.particle_vao));
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.particle_inst));
                        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(inst), glow::STREAM_DRAW);
                        gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, n as i32);
                    }
                    self.stats.draw_calls += 1;
                }
                _ => {}
            }
        }
        unsafe {
            gl.depth_mask(true);
            gl.color_mask_draw_buffer(1, true, true, true, true);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
        }
        if let Some(ms) = &self.ms {
            ms.resolve(gl, self.scene_t.as_ref().unwrap());
        }

        // ---- post chain
        let passes = graph.plan();
        let scene_color = self.scene_t.as_ref().unwrap().color;
        let depth = self.scene_t.as_ref().unwrap().depth;
        let mut results: Vec<glow::Texture> = Vec::new();
        let texel = Vec2::new(1.0 / w as f32, 1.0 / h as f32);
        let inv_proj = proj.inverse();
        let gbuf = self.scene_t.as_ref().and_then(|t| t.gbuf);
        let cur_vp = proj * view;
        let prev_vp = self.prev_vp[self.slot].unwrap_or(cur_vp);
        self.prev_vp[self.slot] = Some(cur_vp);
        for (pi, pass) in passes.iter().enumerate() {
            let node = &graph.nodes[pass.node];
            let input = |k: usize, results: &Vec<glow::Texture>| -> glow::Texture {
                match pass.inputs.get(k).copied().unwrap_or(SCENE) {
                    SCENE => scene_color,
                    i => results.get(i).copied().unwrap_or(scene_color),
                }
            };
            let src = input(0, &results);
            let ti = self.pool_target(gl, pi);
            // Auto exposure: measure the picture, then ease the adapted brightness over time.
            let mut adapted: Option<glow::Texture> = None;
            if node.kind == NodeKind::AutoExposure {
                if self.lum.is_none() {
                    self.lum = Some(Target::new(gl, 64, 36, glow::RGBA16F, false));
                }
                let slot = self.slot;
                if self.adapt[slot].is_none() {
                    self.adapt[slot] = Some((Target::new(gl, 1, 1, glow::RGBA16F, false), Target::new(gl, 1, 1, glow::RGBA16F, false)));
                    self.adapt_first[slot] = true;
                }
                let lum = self.lum.as_ref().unwrap();
                lum.bind(gl);
                self.lum_prog.use_(gl);
                self.lum_prog.tex(gl, "u_tex", 0, Some(src));
                self.draw_fullscreen(gl);
                let (a, b) = self.adapt[slot].as_ref().unwrap();
                b.bind(gl);
                self.adapt_prog.use_(gl);
                self.adapt_prog.tex(gl, "u_lum", 0, Some(lum.color));
                self.adapt_prog.tex(gl, "u_prev", 1, Some(a.color));
                let rate = 1.0 - (-fp.dt.max(0.0) * node.f("Speed")).exp();
                self.adapt_prog.f(gl, "u_rate", rate.clamp(0.0, 1.0));
                self.adapt_prog.i(gl, "u_first", self.adapt_first[slot] as i32);
                self.draw_fullscreen(gl);
                adapted = Some(b.color);
                self.adapt_first[slot] = false;
                if let Some((a, b)) = self.adapt[slot].take() {
                    self.adapt[slot] = Some((b, a));
                }
            }
            if node.kind == NodeKind::Bloom {
                // threshold -> downsample chain -> upsample -> combine
                let nb = self.bloom.len();
                if nb > 0 {
                    let b0 = &self.bloom[0];
                    b0.bind(gl);
                    self.bright.use_(gl);
                    self.bright.tex(gl, "u_tex", 0, Some(src));
                    self.bright.f(gl, "u_threshold", node.f("Threshold"));
                    self.draw_fullscreen(gl);
                    for i in 1..nb {
                        let (a, b) = (&self.bloom[i - 1], &self.bloom[i]);
                        b.bind(gl);
                        self.down.use_(gl);
                        self.down.tex(gl, "u_tex", 0, Some(a.color));
                        self.down.v2(gl, "u_texel", Vec2::new(1.0 / a.w as f32, 1.0 / a.h as f32));
                        self.draw_fullscreen(gl);
                    }
                    // upsample with additive accumulation into the larger levels (in place via blend)
                    unsafe {
                        gl.enable(glow::BLEND);
                        gl.blend_func(glow::ONE, glow::ONE);
                    }
                    for i in (1..nb).rev() {
                        let (a, b) = (&self.bloom[i - 1], &self.bloom[i]);
                        a.bind(gl);
                        self.copy.use_(gl);
                        // tent upsample via the up shader would need a separate target; use copy with
                        // linear filtering, which is cheap and smooth at these sizes.
                        self.up.use_(gl);
                        self.up.tex(gl, "u_tex", 0, Some(b.color));
                        self.up.tex(gl, "u_base", 1, Some(self.black));
                        self.up.v2(gl, "u_texel", Vec2::new(1.0 / b.w as f32, 1.0 / b.h as f32));
                        self.up.f(gl, "u_radius", node.f("Radius"));
                        self.draw_fullscreen(gl);
                    }
                    unsafe {
                        gl.disable(glow::BLEND);
                    }
                    let t = &self.pool[ti];
                    t.bind(gl);
                    self.bloom_mix.use_(gl);
                    self.bloom_mix.tex(gl, "u_tex", 0, Some(src));
                    self.bloom_mix.tex(gl, "u_bloom", 1, Some(self.bloom[0].color));
                    self.bloom_mix.f(gl, "u_intensity", node.f("Intensity") * 0.5);
                    self.draw_fullscreen(gl);
                } else {
                    let t = &self.pool[ti];
                    t.bind(gl);
                    self.copy.use_(gl);
                    self.copy.tex(gl, "u_tex", 0, Some(src));
                    self.draw_fullscreen(gl);
                }
            } else {
                let p = &self.fx;
                let t = &self.pool[ti];
                t.bind(gl);
                p.use_(gl);
                p.tex(gl, "u_tex", 0, Some(src));
                p.tex(gl, "u_tex2", 1, Some(input(1, &results)));
                p.tex(gl, "u_depth", 2, depth);
                p.v2(gl, "u_texel", texel);
                p.f(gl, "u_time", fp.time);
                p.f(gl, "u_near", 0.5);
                p.f(gl, "u_far", 4000.0);
                p.m4(gl, "u_inv_proj", &inv_proj);
                p.m4(gl, "u_proj", &proj);
                p.m4(gl, "u_view", &view);
                p.m4(gl, "u_inv_vp", &cur_vp.inverse());
                p.m4(gl, "u_prev_vp", &prev_vp);
                p.tex(gl, "u_gbuf", 3, gbuf);
                p.i(gl, "u_has_gbuf", gbuf.is_some() as i32);
                if let Some(t) = adapted {
                    p.tex(gl, "u_tex2", 1, Some(t));
                }
                let (kind, p0, p1, p2) = match node.kind {
                    NodeKind::Tonemap => (1, Vec4::new(node.i("Curve") as f32, node.f("Exposure"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::Vignette => (2, Vec4::new(node.f("Amount"), node.f("Softness"), 0.0, 0.0), node.c("Colour").extend(1.0), Vec4::ZERO),
                    NodeKind::ChromaticAberration => (3, Vec4::new(node.f("Amount"), 0.0, 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::FilmGrain => (4, Vec4::new(node.f("Amount"), node.f("Size"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::Sharpen => (5, Vec4::new(node.f("Amount"), 0.0, 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::Fxaa => (6, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO),
                    NodeKind::DepthOfField => {
                        let focus = if node.b("Auto focus") { scene.camera.distance.abs().max(1.0) } else { node.f("Focus distance") };
                        // Thin lens in scene units (1 unit ≈ 8 cm): f = mm / 80, aperture = f / N,
                        // blur on a 24 mm-high sensor scaled to the picture height in pixels.
                        let f = node.f("Lens (mm)") / 80.0;
                        let aperture = f / node.f("F-stop").max(0.5);
                        let sensor = 24.0 / 80.0;
                        let k = aperture * f / (focus - f).max(0.05) / sensor * h as f32 * 0.5;
                        let blades = match node.i("Bokeh shape") {
                            1 => 6.0,
                            2 => 8.0,
                            _ => 0.0,
                        };
                        let maxpx = node.f("Max blur") * (h as f32 / 1080.0).max(0.4);
                        (7, Vec4::new(focus, k, maxpx, blades), Vec4::new(node.f("Bokeh brightness"), 0.0, 0.0, 0.0), Vec4::ZERO)
                    }
                    NodeKind::Ssao => (8, Vec4::new(node.f("Radius"), node.f("Strength"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::LightShafts => (
                        9,
                        Vec4::new(node.f("Strength"), node.f("Length"), node.f("Threshold"), 0.0),
                        node.c("Colour").extend(1.0),
                        Vec4::new(node.f("Source X"), node.f("Source Y"), 0.0, 0.0),
                    ),
                    NodeKind::Fog => (10, Vec4::new(node.f("Start"), node.f("Density"), 0.0, 0.0), node.c("Colour").extend(1.0), Vec4::ZERO),
                    NodeKind::Outline => (11, Vec4::new(node.f("Thickness"), node.f("Strength"), 0.0, 0.0), node.c("Colour").extend(1.0), Vec4::ZERO),
                    NodeKind::Letterbox => (12, Vec4::new(node.f("Aspect"), 0.0, 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::Posterize => (13, Vec4::new(node.f("Levels"), node.f("Mix"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::Mix => (14, Vec4::new(node.i("Mode") as f32, node.f("Factor"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::AutoExposure => (15, Vec4::new(node.f("Target brightness"), node.f("Min exposure"), node.f("Max exposure"), 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::MotionBlur => (16, Vec4::new(node.f("Shutter"), node.f("Samples"), 0.0, 0.0), Vec4::ZERO, Vec4::ZERO),
                    NodeKind::ScreenReflections => (
                        17,
                        Vec4::new(node.f("Strength"), node.f("Max distance"), node.f("Thickness"), node.f("Roughness cut-off")),
                        Vec4::ZERO,
                        Vec4::ZERO,
                    ),
                    _ => (0, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO),
                };
                p.i(gl, "u_kind", kind);
                p.v4(gl, "u_p0", p0);
                p.v4(gl, "u_p1", p1);
                p.v4(gl, "u_p2", p2);
                self.draw_fullscreen(gl);
            }
            results.push(self.pool[ti].color);
        }
        let final_hdr = results.last().copied().unwrap_or(scene_color);

        // ---- colour grade into the 8-bit output
        self.ensure_lut(gl, fp.grade.lut.as_deref());
        let lut = match (&fp.grade.lut, &self.lut_tex) {
            (Some(want), Some((have, tex, n, mn, mx))) if want == have && fp.grade.lut_strength > 0.0 => Some((*tex, *n, *mn, *mx)),
            _ => None,
        };
        let out = self.output.as_ref().unwrap();
        out.bind(gl);
        let g = fp.grade;
        let p = &self.grade;
        p.use_(gl);
        p.tex(gl, "u_tex", 0, Some(final_hdr));
        p.i(gl, "u_enabled", g.enabled as i32);
        p.f(gl, "u_exposure", g.exposure);
        p.f(gl, "u_contrast", g.contrast);
        p.f(gl, "u_saturation", g.saturation);
        p.f(gl, "u_vibrance", g.vibrance);
        p.f(gl, "u_temperature", g.temperature);
        p.f(gl, "u_tint", g.tint);
        p.v3(gl, "u_lift", Vec3::from(g.lift));
        p.v3(gl, "u_gamma", Vec3::from(g.gamma));
        p.v3(gl, "u_gain", Vec3::from(g.gain));
        p.f(gl, "u_shadows", g.shadows);
        p.f(gl, "u_highlights", g.highlights);
        p.i(gl, "u_look", g.look as i32);
        p.f(gl, "u_look_k", g.look_strength);
        p.i(gl, "u_split", g.split_compare as i32);
        p.f(gl, "u_split_pos", g.split_pos);
        p.v2(gl, "u_texel", texel);
        // The LUT always sits on texture unit 1 (a 3D sampler may not share unit 0 with u_tex).
        p.i(gl, "u_lut", 1);
        match lut {
            Some((tex, n, mn, mx)) => {
                unsafe {
                    gl.active_texture(glow::TEXTURE1);
                    gl.bind_texture(glow::TEXTURE_3D, Some(tex));
                }
                p.i(gl, "u_lut_on", 1);
                p.f(gl, "u_lut_k", fp.grade.lut_strength.clamp(0.0, 1.0));
                p.f(gl, "u_lut_n", n as f32);
                p.v3(gl, "u_lut_min", mn);
                p.v3(gl, "u_lut_max", mx);
            }
            None => p.i(gl, "u_lut_on", 0),
        }
        self.draw_fullscreen(gl);
        unsafe {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_3D, None);
            gl.active_texture(glow::TEXTURE0);
        }

        // restore state for egui
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.active_texture(glow::TEXTURE0);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.front_face(glow::CCW);
        }
        // Drop particle systems whose nodes were removed.
        let live: Vec<u32> = graph.nodes.iter().filter(|n| n.kind == NodeKind::Particles).map(|n| n.id).collect();
        self.particles.retain(|k, _| live.contains(k));
        self.stats.gpu_ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.output.as_ref().unwrap().color
    }

    /// Load the LUT the grade asks for (once per file; a broken file is remembered, not retried
    /// every frame).
    fn ensure_lut(&mut self, gl: &glow::Context, path: Option<&Path>) {
        let Some(path) = path else { return };
        if self.lut_tex.as_ref().map(|l| l.0 == path).unwrap_or(false) || self.lut_error.as_ref().map(|e| e.0 == path).unwrap_or(false) {
            return;
        }
        match crate::lut::load(path) {
            Ok(l) => unsafe {
                if let Some((_, old, ..)) = self.lut_tex.take() {
                    gl.delete_texture(old);
                }
                let Ok(t) = gl.create_texture() else { return };
                gl.bind_texture(glow::TEXTURE_3D, Some(t));
                for (k, v) in [
                    (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                    (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_R, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_3D, k, v as i32);
                }
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                let n = l.size as i32;
                gl.tex_image_3d(glow::TEXTURE_3D, 0, glow::RGB16F as i32, n, n, n, 0, glow::RGB, glow::FLOAT, glow::PixelUnpackData::Slice(Some(as_bytes(&l.data))));
                gl.bind_texture(glow::TEXTURE_3D, None);
                self.lut_title = l.title.clone();
                self.lut_error = None;
                self.lut_tex = Some((path.to_path_buf(), t, l.size, Vec3::from(l.domain_min), Vec3::from(l.domain_max)));
            },
            Err(e) => self.lut_error = Some((path.to_path_buf(), e)),
        }
    }

    /// Read back the last output (top row first), RGBA8.
    pub fn read_output(&self, gl: &glow::Context) -> (u32, u32, Vec<u8>) {
        let Some(out) = &self.output else { return (0, 0, vec![]) };
        let (w, h) = (out.w as usize, out.h as usize);
        let mut px = vec![0u8; w * h * 4];
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        let row = w * 4;
        let mut flipped = vec![0u8; px.len()];
        for y in 0..h {
            flipped[y * row..(y + 1) * row].copy_from_slice(&px[(h - 1 - y) * row..(h - y) * row]);
        }
        for a in flipped.iter_mut().skip(3).step_by(4) {
            *a = 255;
        }
        (w as u32, h as u32, flipped)
    }

    pub fn output_texture(&self) -> Option<glow::Texture> {
        self.output.as_ref().map(|o| o.color)
    }

    pub fn loaded_texture_count(&self) -> usize {
        self.tex_cache.values().filter(|t| t.is_some()).count()
    }

    pub fn invalidate_model(&mut self, gl: &glow::Context, id: u64) {
        if let Some(g) = self.models.remove(&id) {
            unsafe {
                gl.delete_vertex_array(g.vao);
                gl.delete_buffer(g.vbo);
                gl.delete_buffer(g.dyn_vbo);
                gl.delete_buffer(g.ebo);
                gl.delete_texture(g.bone_tex);
            }
        }
    }

    pub fn quad_vbo(&self) -> glow::Buffer {
        self.quad_vbo
    }
}
