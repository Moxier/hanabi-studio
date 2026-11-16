//! Format-independent model data. PMX, PMD and X loaders all produce a `ModelData`.

use glam::{Quat, Vec2, Vec3, Vec4};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pos: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub bones: [i32; 4],
    pub weights: [f32; 4],
    pub edge_scale: f32,
    /// SDEF data (C, R0, R1); approximated as linear blend on the GPU.
    pub sdef: Option<[Vec3; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SphereMode {
    Off,
    Multiply,
    Add,
    SubTexture,
}

#[derive(Clone, Debug)]
pub enum Toon {
    None,
    /// Built-in procedural ramp 0..=9 (shared toon). Generated, not shipped.
    Shared(u8),
    Texture(i32),
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub name_en: String,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub double_sided: bool,
    pub ground_shadow: bool,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub edge: bool,
    pub edge_color: Vec4,
    pub edge_size: f32,
    pub texture: i32,
    pub sphere: i32,
    pub sphere_mode: SphereMode,
    pub toon: Toon,
    pub index_count: u32,
    pub memo: String,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            name_en: String::new(),
            diffuse: Vec4::new(0.8, 0.8, 0.8, 1.0),
            specular: Vec3::splat(0.1),
            specular_power: 5.0,
            ambient: Vec3::splat(0.4),
            double_sided: false,
            ground_shadow: true,
            cast_shadow: true,
            receive_shadow: true,
            edge: true,
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 1.0,
            texture: -1,
            sphere: -1,
            sphere_mode: SphereMode::Off,
            toon: Toon::None,
            index_count: 0,
            memo: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IkLink {
    pub bone: i32,
    pub limit: Option<(Vec3, Vec3)>,
}

#[derive(Clone, Debug)]
pub struct Ik {
    pub target: i32,
    pub iterations: i32,
    pub limit_angle: f32,
    pub links: Vec<IkLink>,
}

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub name_en: String,
    pub position: Vec3,
    pub parent: i32,
    pub level: i32,
    pub rotatable: bool,
    pub movable: bool,
    pub visible: bool,
    pub operable: bool,
    pub after_physics: bool,
    pub tail_bone: i32,
    pub tail_offset: Vec3,
    pub append_parent: i32,
    pub append_ratio: f32,
    pub append_rotate: bool,
    pub append_translate: bool,
    pub append_local: bool,
    pub fixed_axis: Option<Vec3>,
    pub local_axes: Option<(Vec3, Vec3)>,
    pub ik: Option<Ik>,
}

impl Bone {
    pub fn new(name: &str, position: Vec3, parent: i32) -> Self {
        Self {
            name: name.to_string(),
            name_en: String::new(),
            position,
            parent,
            level: 0,
            rotatable: true,
            movable: false,
            visible: true,
            operable: true,
            after_physics: false,
            tail_bone: -1,
            tail_offset: Vec3::ZERO,
            append_parent: -1,
            append_ratio: 0.0,
            append_rotate: false,
            append_translate: false,
            append_local: false,
            fixed_axis: None,
            local_axes: None,
            ik: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaterialMorphOffset {
    pub material: i32,
    pub additive: bool,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub edge_color: Vec4,
    pub edge_size: f32,
    pub texture_tint: Vec4,
    pub sphere_tint: Vec4,
    pub toon_tint: Vec4,
}

#[derive(Clone, Debug)]
pub enum MorphKind {
    Group(Vec<(i32, f32)>),
    Vertex(Vec<(u32, Vec3)>),
    Bone(Vec<(i32, Vec3, Quat)>),
    Uv(Vec<(u32, Vec4)>),
    Material(Vec<MaterialMorphOffset>),
    Flip(Vec<(i32, f32)>),
    Impulse(Vec<(i32, bool, Vec3, Vec3)>),
}

#[derive(Clone, Debug)]
pub struct Morph {
    pub name: String,
    pub name_en: String,
    /// 1 eyebrow, 2 eye, 3 mouth, 4 other (0 = system)
    pub panel: u8,
    pub kind: MorphKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Sphere,
    Box,
    Capsule,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RigidMode {
    FollowBone,
    Physics,
    PhysicsWithBonePosition,
}

#[derive(Clone, Debug)]
pub struct RigidBody {
    pub name: String,
    pub bone: i32,
    pub group: u8,
    pub mask: u16,
    pub shape: Shape,
    pub size: Vec3,
    pub position: Vec3,
    pub rotation: Vec3,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub mode: RigidMode,
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub name: String,
    pub a: i32,
    pub b: i32,
    pub position: Vec3,
    pub rotation: Vec3,
    pub lin_min: Vec3,
    pub lin_max: Vec3,
    pub ang_min: Vec3,
    pub ang_max: Vec3,
    pub spring_lin: Vec3,
    pub spring_ang: Vec3,
}

#[derive(Clone, Debug)]
pub struct DisplayFrame {
    pub name: String,
    pub special: bool,
    /// (is_morph, index)
    pub items: Vec<(bool, i32)>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelData {
    pub name: String,
    pub name_en: String,
    pub comment: String,
    pub path: Option<PathBuf>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub textures: Vec<String>,
    pub materials: Vec<Material>,
    pub bones: Vec<Bone>,
    pub morphs: Vec<Morph>,
    pub frames: Vec<DisplayFrame>,
    pub rigid_bodies: Vec<RigidBody>,
    pub joints: Vec<Joint>,
    /// True for .x accessories (no skeleton of their own).
    pub is_accessory: bool,
    /// Embedded RGBA textures (procedural demo model), keyed by texture index.
    pub embedded_textures: Vec<Option<(u32, u32, Vec<u8>)>>,
}

impl ModelData {
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
    pub fn morph_index(&self, name: &str) -> Option<usize> {
        self.morphs.iter().position(|m| m.name == name)
    }

    /// Make sure every model has at least one bone and display frames.
    pub fn finalize(&mut self) {
        self.repair_numbers();
        if self.bones.is_empty() {
            let mut b = Bone::new("root", Vec3::ZERO, -1);
            b.movable = true;
            self.bones.push(b);
        }
        for v in &mut self.vertices {
            for i in 0..4 {
                if v.bones[i] < 0 || v.bones[i] as usize >= self.bones.len() {
                    v.bones[i] = 0;
                    v.weights[i] = if i == 0 && v.weights.iter().all(|w| *w == 0.0) { 1.0 } else { v.weights[i] };
                }
            }
            let s: f32 = v.weights.iter().sum();
            if s > 1e-6 {
                for w in &mut v.weights {
                    *w /= s;
                }
            } else {
                v.weights = [1.0, 0.0, 0.0, 0.0];
            }
        }
        if self.frames.is_empty() || self.frames.iter().all(|f| f.items.is_empty()) {
            self.frames.clear();
            self.frames.push(DisplayFrame {
                name: "Root".into(),
                special: true,
                items: vec![(false, 0)],
            });
            self.frames.push(DisplayFrame {
                name: "Bones".into(),
                special: false,
                items: (1..self.bones.len() as i32).filter(|&i| self.bones[i as usize].operable).map(|i| (false, i)).collect(),
            });
            self.frames.push(DisplayFrame {
                name: "Expressions".into(),
                special: true,
                items: (0..self.morphs.len() as i32).map(|i| (true, i)).collect(),
            });
        }
        // Bounds-check material index counts against the index buffer.
        let mut total = 0u32;
        for m in &mut self.materials {
            let left = (self.indices.len() as u32).saturating_sub(total);
            m.index_count = m.index_count.min(left);
            total += m.index_count;
        }
        if self.materials.is_empty() && !self.indices.is_empty() {
            self.materials.push(Material {
                name: "default".into(),
                index_count: self.indices.len() as u32,
                ..Default::default()
            });
        }
        // Fix knee-style IK links in PMD which have no explicit limits.
        if self.embedded_textures.len() < self.textures.len() {
            self.embedded_textures.resize(self.textures.len(), None);
        }
    }

    /// Some community models store "not a number" values (for example a rigid body rotation),
    /// which would break skinning or the physics engine. Replace them with safe values.
    /// Returns how many values were repaired.
    pub fn repair_numbers(&mut self) -> usize {
        fn v3(v: &mut Vec3, d: Vec3) -> usize {
            if v.is_finite() {
                0
            } else {
                *v = Vec3::select(v.is_finite_mask(), *v, d);
                1
            }
        }
        fn f(v: &mut f32, d: f32) -> usize {
            if v.is_finite() {
                0
            } else {
                *v = d;
                1
            }
        }
        let mut n = 0;
        for v in &mut self.vertices {
            n += v3(&mut v.pos, Vec3::ZERO);
            n += v3(&mut v.normal, Vec3::Y);
            if !v.uv.is_finite() {
                v.uv = Vec2::ZERO;
                n += 1;
            }
            for w in &mut v.weights {
                n += f(w, 0.0);
            }
            n += f(&mut v.edge_scale, 1.0);
            if let Some(s) = &v.sdef {
                if !s.iter().all(|x| x.is_finite()) {
                    v.sdef = None;
                    n += 1;
                }
            }
        }
        for b in &mut self.bones {
            n += v3(&mut b.position, Vec3::ZERO);
            n += v3(&mut b.tail_offset, Vec3::ZERO);
            n += f(&mut b.append_ratio, 0.0);
            if let Some(a) = &b.fixed_axis {
                if !a.is_finite() || a.length_squared() < 1e-12 {
                    b.fixed_axis = None;
                    n += 1;
                }
            }
            if let Some((x, z)) = &b.local_axes {
                if !x.is_finite() || !z.is_finite() {
                    b.local_axes = None;
                    n += 1;
                }
            }
            if let Some(ik) = &mut b.ik {
                n += f(&mut ik.limit_angle, 1.0);
                for l in &mut ik.links {
                    if let Some((lo, hi)) = &l.limit {
                        if !lo.is_finite() || !hi.is_finite() {
                            l.limit = None;
                            n += 1;
                        }
                    }
                }
            }
        }
        for rb in &mut self.rigid_bodies {
            n += v3(&mut rb.size, Vec3::splat(0.1));
            n += v3(&mut rb.position, Vec3::ZERO);
            n += v3(&mut rb.rotation, Vec3::ZERO);
            n += f(&mut rb.mass, 1.0);
            n += f(&mut rb.linear_damping, 0.5);
            n += f(&mut rb.angular_damping, 0.5);
            n += f(&mut rb.restitution, 0.0);
            n += f(&mut rb.friction, 0.5);
        }
        for j in &mut self.joints {
            n += v3(&mut j.position, Vec3::ZERO);
            n += v3(&mut j.rotation, Vec3::ZERO);
            n += v3(&mut j.lin_min, Vec3::ZERO);
            n += v3(&mut j.lin_max, Vec3::ZERO);
            n += v3(&mut j.ang_min, Vec3::ZERO);
            n += v3(&mut j.ang_max, Vec3::ZERO);
            n += v3(&mut j.spring_lin, Vec3::ZERO);
            n += v3(&mut j.spring_ang, Vec3::ZERO);
        }
        for m in &mut self.morphs {
            match &mut m.kind {
                MorphKind::Vertex(list) => list.iter_mut().for_each(|(_, o)| n += v3(o, Vec3::ZERO)),
                MorphKind::Bone(list) => list.iter_mut().for_each(|(_, t, r)| {
                    n += v3(t, Vec3::ZERO);
                    if !r.is_finite() {
                        *r = Quat::IDENTITY;
                        n += 1;
                    }
                }),
                MorphKind::Uv(list) => list.iter_mut().for_each(|(_, o)| {
                    if !o.is_finite() {
                        *o = Vec4::ZERO;
                        n += 1;
                    }
                }),
                _ => {}
            }
        }
        n
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            mn = mn.min(v.pos);
            mx = mx.max(v.pos);
        }
        if self.vertices.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (mn, mx)
        }
    }
}

/// Tell whether a bone name refers to a left/right side and give its mirror name.
pub fn mirror_name(name: &str) -> Option<String> {
    let pairs: [(&str, &str); 6] = [
        ("左", "右"),
        ("_L", "_R"),
        (".L", ".R"),
        ("Left", "Right"),
        ("left", "right"),
        (" L", " R"),
    ];
    for (a, b) in pairs {
        if name.contains(a) {
            return Some(name.replacen(a, b, 1));
        }
        if name.contains(b) {
            return Some(name.replacen(b, a, 1));
        }
    }
    None
}
