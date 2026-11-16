//! Thin helpers over glow: programs with cached uniforms, render targets, textures.

use glam::{Mat4, Vec2, Vec3, Vec4};
use glow::HasContext;
use std::cell::RefCell;
use std::collections::HashMap;

pub struct Program {
    pub id: glow::Program,
    uniforms: RefCell<HashMap<String, Option<glow::UniformLocation>>>,
}

pub const HEADER: &str = "#version 330 core\n";

pub fn compile(gl: &glow::Context, name: &str, vs: &str, fs: &str) -> Result<Program, String> {
    unsafe {
        let p = gl.create_program()?;
        let mut shaders = Vec::new();
        for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let s = gl.create_shader(ty)?;
            gl.shader_source(s, &format!("{HEADER}{src}"));
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                let log = gl.get_shader_info_log(s);
                return Err(format!("shader {name} ({}): {log}", if ty == glow::VERTEX_SHADER { "vertex" } else { "fragment" }));
            }
            gl.attach_shader(p, s);
            shaders.push(s);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(format!("link {name}: {}", gl.get_program_info_log(p)));
        }
        for s in shaders {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        Ok(Program { id: p, uniforms: RefCell::new(HashMap::new()) })
    }
}

impl Program {
    pub fn loc(&self, gl: &glow::Context, name: &str) -> Option<glow::UniformLocation> {
        let mut u = self.uniforms.borrow_mut();
        if let Some(l) = u.get(name) {
            return l.clone();
        }
        let l = unsafe { gl.get_uniform_location(self.id, name) };
        u.insert(name.to_string(), l.clone());
        l
    }
    pub fn use_(&self, gl: &glow::Context) {
        unsafe { gl.use_program(Some(self.id)) }
    }
    pub fn f(&self, gl: &glow::Context, n: &str, v: f32) {
        unsafe { gl.uniform_1_f32(self.loc(gl, n).as_ref(), v) }
    }
    pub fn i(&self, gl: &glow::Context, n: &str, v: i32) {
        unsafe { gl.uniform_1_i32(self.loc(gl, n).as_ref(), v) }
    }
    pub fn v2(&self, gl: &glow::Context, n: &str, v: Vec2) {
        unsafe { gl.uniform_2_f32(self.loc(gl, n).as_ref(), v.x, v.y) }
    }
    pub fn v3(&self, gl: &glow::Context, n: &str, v: Vec3) {
        unsafe { gl.uniform_3_f32(self.loc(gl, n).as_ref(), v.x, v.y, v.z) }
    }
    pub fn v4(&self, gl: &glow::Context, n: &str, v: Vec4) {
        unsafe { gl.uniform_4_f32(self.loc(gl, n).as_ref(), v.x, v.y, v.z, v.w) }
    }
    /// Set a `vec4 name[N]` array from packed floats (4 per element).
    pub fn v4_array(&self, gl: &glow::Context, n: &str, v: &[f32]) {
        if v.is_empty() {
            return;
        }
        unsafe { gl.uniform_4_f32_slice(self.loc(gl, n).as_ref(), v) }
    }
    pub fn m4(&self, gl: &glow::Context, n: &str, m: &Mat4) {
        unsafe { gl.uniform_matrix_4_f32_slice(self.loc(gl, n).as_ref(), false, &m.to_cols_array()) }
    }
    pub fn tex(&self, gl: &glow::Context, n: &str, unit: u32, t: Option<glow::Texture>) {
        unsafe {
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, t);
            gl.uniform_1_i32(self.loc(gl, n).as_ref(), unit as i32);
        }
    }
}

pub fn texture_rgba8(gl: &glow::Context, w: u32, h: u32, data: &[u8], repeat: bool, mipmap: bool) -> glow::Texture {
    unsafe {
        let t = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(data)),
        );
        let wrap = if repeat { glow::REPEAT } else { glow::CLAMP_TO_EDGE } as i32;
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        if mipmap {
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        } else {
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        }
        t
    }
}

/// A colour (+ optional depth texture) render target.
pub struct Target {
    pub fbo: glow::Framebuffer,
    pub color: glow::Texture,
    pub depth: Option<glow::Texture>,
    /// Second colour attachment: surface normal (rgb) and roughness (a, negative = metal).
    pub gbuf: Option<glow::Texture>,
    pub w: i32,
    pub h: i32,
    pub format: u32,
}

impl Target {
    pub fn new(gl: &glow::Context, w: i32, h: i32, format: u32, with_depth: bool) -> Self {
        unsafe {
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let color = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(color));
            let (fmt, ty) = match format {
                glow::RGBA8 => (glow::RGBA, glow::UNSIGNED_BYTE),
                glow::R8 => (glow::RED, glow::UNSIGNED_BYTE),
                _ => (glow::RGBA, glow::HALF_FLOAT),
            };
            gl.tex_image_2d(glow::TEXTURE_2D, 0, format as i32, w, h, 0, fmt, ty, glow::PixelUnpackData::Slice(None));
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
            let depth = if with_depth {
                let d = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(d));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::DEPTH_COMPONENT24 as i32,
                    w,
                    h,
                    0,
                    glow::DEPTH_COMPONENT,
                    glow::UNSIGNED_INT,
                    glow::PixelUnpackData::Slice(None),
                );
                for (k, v) in [
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
                }
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(d), 0);
                Some(d)
            } else {
                None
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Self { fbo, color, depth, gbuf: None, w, h, format }
        }
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.w, self.h);
        }
    }

    /// Attach a half-float normal/roughness buffer as colour attachment 1.
    pub fn add_gbuffer(&mut self, gl: &glow::Context) {
        unsafe {
            let t = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, self.w, self.h, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT1, glow::TEXTURE_2D, Some(t), 0);
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gbuf = Some(t);
        }
    }

    /// Re-specify storage at a new size, keeping the same texture handles.
    pub fn resize(&mut self, gl: &glow::Context, w: i32, h: i32) {
        if w == self.w && h == self.h {
            return;
        }
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            let (fmt, ty) = match self.format {
                glow::RGBA8 => (glow::RGBA, glow::UNSIGNED_BYTE),
                glow::R8 => (glow::RED, glow::UNSIGNED_BYTE),
                _ => (glow::RGBA, glow::HALF_FLOAT),
            };
            gl.tex_image_2d(glow::TEXTURE_2D, 0, self.format as i32, w, h, 0, fmt, ty, glow::PixelUnpackData::Slice(None));
            if let Some(d) = self.depth {
                gl.bind_texture(glow::TEXTURE_2D, Some(d));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, w, h, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
            }
            if let Some(g) = self.gbuf {
                gl.bind_texture(glow::TEXTURE_2D, Some(g));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
            }
        }
        self.w = w;
        self.h = h;
    }

    pub fn delete(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.color);
            if let Some(d) = self.depth {
                gl.delete_texture(d);
            }
            if let Some(g) = self.gbuf {
                gl.delete_texture(g);
            }
        }
    }
}

/// Multisampled colour + depth renderbuffers that resolve into a `Target`.
pub struct MsTarget {
    pub fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    color2: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    pub w: i32,
    pub h: i32,
    pub samples: i32,
}

impl MsTarget {
    pub fn new(gl: &glow::Context, w: i32, h: i32, samples: i32) -> Self {
        unsafe {
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let color = gl.create_renderbuffer().unwrap();
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA16F, w, h);
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
            let color2 = gl.create_renderbuffer().unwrap();
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color2));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA16F, w, h);
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT1, glow::RENDERBUFFER, Some(color2));
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
            let depth = gl.create_renderbuffer().unwrap();
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH_COMPONENT24, w, h);
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Self { fbo, color, color2, depth, w, h, samples }
        }
    }
    pub fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.w, self.h);
        }
    }
    pub fn resolve(&self, gl: &glow::Context, dst: &Target) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(dst.fbo));
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
            gl.blit_framebuffer(0, 0, self.w, self.h, 0, 0, dst.w, dst.h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            if dst.gbuf.is_some() {
                gl.read_buffer(glow::COLOR_ATTACHMENT1);
                gl.draw_buffers(&[glow::NONE, glow::COLOR_ATTACHMENT1]);
                gl.blit_framebuffer(0, 0, self.w, self.h, 0, 0, dst.w, dst.h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
                gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
                gl.read_buffer(glow::COLOR_ATTACHMENT0);
            }
            gl.blit_framebuffer(0, 0, self.w, self.h, 0, 0, dst.w, dst.h, glow::DEPTH_BUFFER_BIT, glow::NEAREST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
    }
    pub fn delete(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_renderbuffer(self.color);
            gl.delete_renderbuffer(self.color2);
            gl.delete_renderbuffer(self.depth);
        }
    }
}

pub fn as_bytes<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
