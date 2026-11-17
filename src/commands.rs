//! Executes MCP tool calls on the UI thread.

use crate::app::App;
use crate::effects::{presets, NodeKind, PVal};
use crate::grade::LOOKS;
use crate::motion::{quat_from_euler_deg, Curve, TrackId};
use serde_json::{json, Value};
use std::path::PathBuf;

fn v3(v: &Value) -> Option<glam::Vec3> {
    let a = v.as_array()?;
    Some(glam::Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}

fn curve_named(s: &str) -> Result<Curve, String> {
    Ok(match s {
        "linear" => Curve::LINEAR,
        "ease_in" => Curve::EASE_IN,
        "ease_out" => Curve::EASE_OUT,
        "snappy" => Curve::SHARP,
        "hold" => Curve::STEP,
        "" | "smooth" | "ease_in_out" => Curve::EASE_IN_OUT,
        other => return Err(format!("unknown curve “{other}”; use linear, smooth, ease_in, ease_out, snappy or hold")),
    })
}

impl App {
    pub fn process_mcp(&mut self, frame: &mut eframe::Frame) {
        let mut calls = Vec::new();
        if let Some(m) = &self.mcp {
            while let Ok(c) = m.rx.try_recv() {
                calls.push(c);
            }
        }
        for c in calls {
            let r = self.tool(&c.name, &c.args, frame);
            if let Some(m) = self.mcp.as_mut() {
                m.calls += 1;
            }
            if let Err(e) = &r {
                self.set_error(format!("AI tool {}: {e}", c.name));
            } else {
                self.set_status(format!("AI used tool “{}”", c.name));
            }
            let _ = c.reply.send(r);
        }
    }

    fn model_index(&self) -> Result<usize, String> {
        self.scene.selected.filter(|&i| i < self.scene.models.len()).ok_or_else(|| "No model selected. Load one with load_file first.".to_string())
    }

    pub fn tool(&mut self, name: &str, a: &Value, frame: &mut eframe::Frame) -> Result<Value, String> {
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let n = |k: &str| a.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
        let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        match name {
            "get_scene" => Ok(json!({
                "frame": self.scene.frame.round(),
                "fps": self.scene.fps,
                "playing": self.scene.playing,
                "range": [self.scene.play_start, self.scene.play_end],
                "last_key_frame": self.scene.end_frame(),
                "models": self.scene.models.iter().enumerate().map(|(i, m)| json!({
                    "index": i, "name": m.name(), "accessory": m.is_accessory(), "visible": m.visible,
                    "bones": m.data.bones.len(), "morphs": m.data.morphs.len(), "keys": m.motion.key_count(),
                    "rigid_bodies": m.data.rigid_bodies.len()
                })).collect::<Vec<_>>(),
                "selected_model": self.scene.selected,
                "camera": {"target": self.scene.camera.target.to_array(), "rotation_deg": (self.scene.camera.rot * 57.29578).to_array(), "distance": self.scene.camera.distance, "fov": self.scene.camera.fov, "keys": self.scene.camera_motion.camera.len()},
                "light": {"color": self.scene.light.color.to_array(), "direction": self.scene.light.direction.to_array()},
                "effect_preset": self.graph.preset_name,
                "color_grade": self.grade,
                "audio": self.scene.audio.as_ref().map(|a| a.path.display().to_string()),
                "last_render": self.last_render.as_ref().map(|r| match r {
                    Ok(m) => json!({"ok": true, "message": m}),
                    Err(e) => json!({"ok": false, "message": e}),
                }),
                "render_job": self.job.as_ref().map(|j| json!({"progress": j.progress(), "frame": j.next, "frames": j.total(), "end": j.settings.end, "path": j.settings.path.display().to_string()})),
            })),
            "load_file" => {
                let p = PathBuf::from(s("path").ok_or("path is required")?);
                if !p.exists() {
                    return Err(format!("file not found: {}", p.display()));
                }
                self.open_path(&p);
                if self.status_err {
                    Err(self.status.clone())
                } else {
                    Ok(json!(self.status.clone()))
                }
            }
            "select_model" => {
                let i = match (n("index"), s("name")) {
                    (Some(i), _) => i as usize,
                    (None, Some(nm)) => self.scene.models.iter().position(|m| m.name() == nm || m.name().contains(&nm)).ok_or("no model with that name")?,
                    _ => return Err("give index or name".into()),
                };
                if i >= self.scene.models.len() {
                    return Err("index out of range".into());
                }
                self.scene.selected = Some(i);
                self.sel_camera = false;
                self.sel_bone = None;
                Ok(json!(format!("Selected {}", self.scene.models[i].name())))
            }
            "list_bones" => {
                let mi = self.model_index()?;
                let m = &self.scene.models[mi];
                let f = s("filter").unwrap_or_default();
                let list: Vec<Value> = m
                    .data
                    .bones
                    .iter()
                    .filter(|bn| f.is_empty() || bn.name.contains(&f) || bn.name_en.to_lowercase().contains(&f.to_lowercase()))
                    .map(|bn| json!({"name": bn.name, "english": bn.name_en, "rotate": bn.rotatable, "move": bn.movable, "ik": bn.ik.is_some(), "keys": m.motion.bones.get(&bn.name).map(|v| v.len()).unwrap_or(0)}))
                    .collect();
                Ok(json!(list))
            }
            "list_morphs" => {
                let mi = self.model_index()?;
                let m = &self.scene.models[mi];
                let f = s("filter").unwrap_or_default();
                Ok(json!(m.data.morphs.iter().enumerate().filter(|(_, mo)| f.is_empty() || mo.name.contains(&f)).map(|(i, mo)| json!({"name": mo.name, "english": mo.name_en, "weight": m.morph_weights[i]})).collect::<Vec<_>>()))
            }
            "set_frame" => {
                let f = match a.get("frame") {
                    None => return Err("frame is required".into()),
                    Some(v) => v.as_f64().ok_or("frame must be a number")? as f32,
                };
                self.scene.set_frame(f.max(0.0).round());
                Ok(json!(format!("Frame {}", self.scene.frame)))
            }
            "playback" => {
                match s("action").as_deref() {
                    Some("play") => {
                        if !self.scene.playing {
                            self.toggle_play();
                        }
                    }
                    Some("pause") => {
                        if self.scene.playing {
                            self.toggle_play();
                        }
                    }
                    Some("stop") => self.stop(),
                    _ => return Err("action must be play, pause or stop".into()),
                }
                Ok(json!("ok"))
            }
            "pose_bone" => {
                let mi = self.model_index()?;
                let bone = s("bone").ok_or("bone is required")?;
                let bi = self.scene.models[mi].data.bone_index(&bone).or_else(|| crate::tools::find_bone(&self.scene.models[mi].data, &[&bone]).and_then(|n| self.scene.models[mi].data.bone_index(&n))).ok_or(format!("no bone named {bone}"))?;
                let (mut p, mut r) = self.scene.models[mi].bone_pose(bi);
                if let Some(rot) = a.get("rotation").and_then(v3) {
                    r = quat_from_euler_deg(rot);
                }
                if let Some(pos) = a.get("position").and_then(v3) {
                    p = pos;
                }
                let curve = curve_named(&s("curve").unwrap_or_default())?;
                if b("key") {
                    self.checkpoint();
                }
                self.scene.models[mi].pose_edit.insert(bi, (p, r));
                self.scene.models[mi].skel.states[bi].anim_pos = p;
                self.scene.models[mi].skel.states[bi].anim_rot = r;
                self.sel_bone = Some(bi);
                if b("key") {
                    let f = self.scene.frame.round() as u32;
                    self.scene.models[mi].register_bone_key(bi, f, [curve; 4]);
                    return Ok(json!(format!("Posed and keyed {bone} at frame {f}")));
                }
                Ok(json!(format!("Posed {bone} (not keyed)")))
            }
            "set_morph" => {
                let mi = self.model_index()?;
                let name = s("morph").ok_or("morph is required")?;
                let i = self.scene.models[mi].data.morph_index(&name).ok_or(format!("no morph named {name}"))?;
                let w = n("weight").unwrap_or(0.0).clamp(0.0, 1.0);
                self.scene.models[mi].morph_edit.insert(i, w);
                self.scene.models[mi].morph_weights[i] = w;
                if b("key") {
                    self.checkpoint();
                    let f = self.scene.frame.round() as u32;
                    self.scene.models[mi].register_morph_key(i, f);
                }
                Ok(json!(format!("{name} = {w:.2}")))
            }
            "delete_keys" => {
                let t = s("track").ok_or("track is required")?;
                let f = n("frame").ok_or("frame is required")? as u32;
                self.checkpoint();
                let ok = match t.as_str() {
                    "camera" => self.scene.camera_motion.delete_key(&TrackId::Camera, f),
                    "light" => self.scene.camera_motion.delete_key(&TrackId::Light, f),
                    _ => {
                        let mi = self.model_index()?;
                        let m = &mut self.scene.models[mi].motion;
                        m.delete_key(&TrackId::Bone(t.clone()), f) | m.delete_key(&TrackId::Morph(t.clone()), f)
                    }
                };
                Ok(json!(if ok { "deleted" } else { "no key there" }))
            }
            "set_camera" => {
                let c = &mut self.scene.camera;
                if let Some(t) = a.get("target").and_then(v3) {
                    c.target = t;
                }
                if let Some(r) = a.get("rotation").and_then(v3) {
                    c.rot = r * (std::f32::consts::PI / 180.0);
                }
                if let Some(d) = n("distance") {
                    c.distance = if d > 0.0 { -d } else { d };
                }
                if let Some(f) = n("fov") {
                    c.fov = f.clamp(1.0, 150.0);
                }
                self.scene.camera_edited = true;
                if b("key") {
                    self.sel_camera = true;
                    self.register_selected();
                }
                Ok(json!("camera updated"))
            }
            "set_light" => {
                if let Some(c) = a.get("color").and_then(v3) {
                    self.scene.light.color = c;
                }
                if let Some(d) = a.get("direction").and_then(v3) {
                    self.scene.light.direction = d;
                }
                self.scene.light_edited = true;
                Ok(json!("light updated"))
            }
            "list_effect_presets" => Ok(json!(presets().iter().map(|p| json!({"name": p.name, "about": p.about})).collect::<Vec<_>>())),
            "apply_effect_preset" => {
                let nm = s("name").ok_or("name is required")?;
                let replace = b("replace");
                if !self.apply_preset(&nm, replace) {
                    return Err(format!("no preset {nm}; options: {}", presets().iter().map(|p| p.name).collect::<Vec<_>>().join(", ")));
                }
                Ok(json!(self.status.clone()))
            }
            "add_effect_node" => {
                let nm = s("node").ok_or("node is required")?;
                let k = NodeKind::ALL.iter().find(|k| k.info().title.eq_ignore_ascii_case(&nm)).ok_or(format!("unknown node {nm}"))?;
                let id = self.graph.append_before_output(*k);
                self.graph.preset_name = "Custom".into();
                Ok(json!({"added": nm, "id": id}))
            }
            "set_effect_param" => {
                let nm = s("node").ok_or("node is required")?;
                let param = s("param").ok_or("param is required")?;
                let v = a.get("value").ok_or("value is required")?;
                let node = self.graph.nodes.iter_mut().find(|n| n.kind.info().title.eq_ignore_ascii_case(&nm)).ok_or(format!("no {nm} node in the graph"))?;
                let pv = if let Some(x) = v.as_f64() {
                    let spec = node.kind.info().params.into_iter().find(|p| p.name.eq_ignore_ascii_case(&param));
                    match spec.map(|s| s.kind) {
                        Some(crate::effects::PKind::Choice(..)) => PVal::I(x as usize),
                        _ => PVal::F(x as f32),
                    }
                } else if let Some(bb) = v.as_bool() {
                    PVal::B(bb)
                } else if let Some(c) = v3(v) {
                    PVal::C(c.to_array())
                } else if let Some(txt) = v.as_str() {
                    let spec = node.kind.info().params.into_iter().find(|p| p.name.eq_ignore_ascii_case(&param));
                    match spec.map(|s| s.kind) {
                        Some(crate::effects::PKind::Choice(opts, _)) => PVal::I(opts.iter().position(|o| o.eq_ignore_ascii_case(txt)).ok_or("unknown option")?),
                        _ => return Err("value type not understood".into()),
                    }
                } else {
                    return Err("value type not understood".into());
                };
                let pname = node.kind.info().params.iter().find(|p| p.name.eq_ignore_ascii_case(&param)).map(|p| p.name).ok_or(format!("{nm} has no parameter {param}"))?;
                node.set(pname, pv);
                Ok(json!("ok"))
            }
            "get_effect_graph" => {
                let nodes: Vec<Value> = self
                    .graph
                    .nodes
                    .iter()
                    .map(|n| {
                        let info = n.kind.info();
                        let params: serde_json::Map<String, Value> = info.params.iter().zip(n.values.iter()).map(|(p, v)| (p.name.to_string(), serde_json::to_value(v).unwrap_or(Value::Null))).collect();
                        json!({"id": n.id, "node": info.title, "enabled": n.enabled, "params": params})
                    })
                    .collect();
                Ok(json!({"preset": self.graph.preset_name, "nodes": nodes, "links": self.graph.links}))
            }
            "set_color_grade" => {
                let st = a.get("settings").ok_or("settings is required")?;
                if st.get("reset").and_then(|v| v.as_bool()).unwrap_or(false) {
                    let keep = (self.grade.lut.clone(), self.grade.lut_strength);
                    self.grade = Default::default();
                    self.grade.lut = keep.0;
                    self.grade.lut_strength = keep.1;
                }
                let g = &mut self.grade;
                for (k, slot) in [("lift", &mut g.lift), ("gamma", &mut g.gamma), ("gain", &mut g.gain)] {
                    if let Some(v) = st.get(k).and_then(v3) {
                        *slot = v.clamp(glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5)).to_array();
                    }
                }
                if let Some(p) = st.get("lut").and_then(|v| v.as_str()) {
                    if p.is_empty() {
                        g.lut = None;
                    } else {
                        let pb = PathBuf::from(p);
                        crate::lut::load(&pb).map_err(|e| format!("can't use that LUT: {e}"))?;
                        g.lut = Some(pb);
                    }
                }
                if let Some(v) = st.get("lut_strength").and_then(|v| v.as_f64()) {
                    g.lut_strength = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = st.get("split_compare").and_then(|v| v.as_bool()) {
                    g.split_compare = v;
                }
                let f = |k: &str| st.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
                if let Some(v) = f("exposure") { g.exposure = v; }
                if let Some(v) = f("contrast") { g.contrast = v; }
                if let Some(v) = f("saturation") { g.saturation = v; }
                if let Some(v) = f("vibrance") { g.vibrance = v; }
                if let Some(v) = f("temperature") { g.temperature = v; }
                if let Some(v) = f("tint") { g.tint = v; }
                if let Some(v) = f("shadows") { g.shadows = v; }
                if let Some(v) = f("highlights") { g.highlights = v; }
                if let Some(v) = f("look_strength") { g.look_strength = v; }
                if let Some(v) = st.get("enabled").and_then(|v| v.as_bool()) { g.enabled = v; }
                if let Some(l) = st.get("look").and_then(|v| v.as_str()) {
                    g.look = LOOKS.iter().position(|x| x.eq_ignore_ascii_case(l)).ok_or(format!("unknown look {l}; options: {}", LOOKS.join(", ")))?;
                }
                Ok(json!(self.grade))
            }
            "render_image" => {
                let p = PathBuf::from(s("path").ok_or("path is required")?);
                if !p.is_absolute() {
                    return Err(format!("path must be absolute (got “{}”)", p.display()));
                }
                let (w0, h0) = (self.render_settings.width, self.render_settings.height);
                if let (Some(w), Some(h)) = (n("width"), n("height")) {
                    self.render_settings.width = (w as u32).clamp(16, 8192);
                    self.render_settings.height = (h as u32).clamp(16, 8192);
                }
                let r = self.render_still(frame, &p);
                self.render_settings.width = w0;
                self.render_settings.height = h0;
                r.map(Value::from)
            }
            "render_video" => {
                if self.job.is_some() {
                    return Err("a render is already running".into());
                }
                let p = PathBuf::from(s("path").ok_or("path is required")?);
                let rs = &mut self.render_settings;
                rs.path = p.clone();
                rs.format = match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
                    Some("mp4") => crate::export::OutFormat::Mp4,
                    Some("webm") => crate::export::OutFormat::WebM,
                    Some("mov") => crate::export::OutFormat::Mov,
                    _ => crate::export::OutFormat::PngSequence,
                };
                rs.start = n("start").map(|v| v as u32).unwrap_or(self.scene.play_start);
                rs.end = n("end").map(|v| v as u32).unwrap_or(self.scene.play_end).max(rs.start);
                if let Some(w) = n("width") { rs.width = w as u32; }
                if let Some(h) = n("height") { rs.height = h as u32; }
                if let Some(f) = n("fps") { rs.fps = f; }
                if let Some(q) = n("quality") { rs.quality = (q as u32).clamp(12, 32); }
                if let Some(v) = a.get("include_audio").and_then(|v| v.as_bool()) { rs.include_audio = v; }
                self.start_render_job();
                if self.status_err {
                    return Err(self.status.clone());
                }
                Ok(json!(format!("Rendering started → {}. Poll get_scene for progress.", p.display())))
            }
            "save_motion" => {
                let p = PathBuf::from(s("path").ok_or("path is required")?);
                self.save_motion_to(&p, b("camera")).map(Value::from)
            }
            "save_project" => {
                let p = PathBuf::from(s("path").ok_or("path is required")?);
                self.save_project_to(&p).map(Value::from)
            }
            "generate_motion" => {
                let g = crate::tools::Generator::from_name(&s("kind").unwrap_or_default()).ok_or("kind must be idle, wave, nod or bounce")?;
                let start = n("start").map(|v| v as u32).unwrap_or(self.scene.frame.round() as u32);
                let len = n("length").map(|v| v as u32).unwrap_or(120);
                let bpm = n("bpm").unwrap_or(120.0);
                let k = self.generate_motion(g, start, len, bpm);
                if k == 0 {
                    Err("no matching bones on this model".into())
                } else {
                    Ok(json!(format!("{} keys created", k)))
                }
            }
            "set_material_type" => {
                use crate::effects::MatPreset;
                let (Some(mat), Some(t)) = (s("material"), s("type")) else {
                    let list: Vec<Value> = self
                        .scene
                        .models
                        .iter()
                        .flat_map(|m| m.data.materials.iter().map(|x| x.name.clone()))
                        .map(|n| {
                            let cur = self.graph.materials.get(&n).map(|p| p.label().to_string()).unwrap_or_else(|| format!("auto ({})", MatPreset::guess(&n).label()));
                            json!({"material": n, "type": cur})
                        })
                        .collect();
                    return Ok(json!(list));
                };
                if t.eq_ignore_ascii_case("auto") {
                    self.graph.materials.remove(&mat);
                    return Ok(json!(format!("{mat}: automatic")));
                }
                let p = MatPreset::ALL
                    .iter()
                    .find(|p| p.label().to_lowercase().starts_with(&t.to_lowercase()) || format!("{p:?}").eq_ignore_ascii_case(&t))
                    .copied()
                    .ok_or(format!("unknown type {t}"))?;
                self.graph.materials.insert(mat.clone(), p);
                Ok(json!(format!("{mat}: {}", p.label())))
            }
            "add_light" => {
                use crate::material::{builtin_light_rigs, LightKind, SceneLight};
                if b("clear") {
                    self.graph.lights.clear();
                    return Ok(json!("removed all extra lights"));
                }
                if let Some(r) = s("rig") {
                    let rig = builtin_light_rigs().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(&r)).ok_or(format!("no rig {r}"))?;
                    self.graph.lights = rig.1;
                    return Ok(json!(format!("loaded rig {}", rig.0)));
                }
                let kind = match s("kind").unwrap_or_else(|| "point".into()).to_ascii_lowercase().as_str() {
                    "spot" | "spotlight" => LightKind::Spot,
                    "directional" | "dir" => LightKind::Directional,
                    _ => LightKind::Point,
                };
                let mut l = SceneLight::new(kind);
                if let Some(c) = a.get("color").and_then(v3) {
                    l.color = c.to_array();
                }
                if let Some(p) = a.get("position").and_then(v3) {
                    l.position = p.to_array();
                }
                if let Some(v) = n("intensity") {
                    l.intensity = v;
                }
                if let Some(v) = n("pitch") {
                    l.pitch = v;
                }
                if let Some(v) = n("yaw") {
                    l.yaw = v;
                }
                if let Some(v) = n("cone") {
                    l.cone = v;
                }
                if let Some(v) = n("range") {
                    l.range = v;
                }
                l.name = format!("{} {}", kind.label(), self.graph.lights.len() + 1);
                self.graph.lights.push(l);
                Ok(json!(format!("{} lights in the scene", self.graph.lights.len())))
            }
            "set_material" => {
                use crate::material::{builtin_material_presets, MaterialFx, Special};
                let name = s("material").ok_or("material is required")?;
                let known: Vec<String> = self.scene.models.iter().flat_map(|m| m.data.materials.iter().map(|x| x.name.clone())).collect();
                if !known.contains(&name) {
                    let mut list = known.clone();
                    list.sort();
                    list.dedup();
                    return Err(format!("no material named “{name}”. Materials in the scene: {}", list.into_iter().take(60).collect::<Vec<_>>().join(", ")));
                }
                if b("auto") {
                    self.graph.mat_fx.remove(&name);
                    return Ok(json!(format!("{name}: automatic")));
                }
                let mut fx = self.graph.mat_fx.get(&name).cloned().unwrap_or_else(|| MaterialFx::from_preset(crate::effects::MatPreset::guess(&name), 0.55));
                if let Some(pn) = s("preset") {
                    let all = builtin_material_presets();
                    let names: Vec<String> = all.iter().map(|(n, _)| n.to_string()).collect();
                    let (_, m) = all.into_iter().find(|(n, _)| n.eq_ignore_ascii_case(&pn)).ok_or(format!("no material preset “{pn}”; presets: {}", names.join(", ")))?;
                    fx = m;
                }
                if let Some(sp) = s("special") {
                    fx.special = match sp.to_ascii_lowercase().as_str() {
                        "glass" => Special::Glass,
                        "water" => Special::Water,
                        "wet" | "rain" => Special::Wet,
                        "fur" => Special::Fur,
                        "grass" => Special::Grass,
                        "glow" | "luminous" => Special::Glow,
                        "none" | "" => Special::None,
                        other => return Err(format!("unknown special “{other}”; use none, glass, water, wet, fur, grass or glow")),
                    };
                }
                if let Some(v) = n("roughness") {
                    fx.rough = v.clamp(0.02, 1.0);
                }
                if let Some(v) = n("metallic") {
                    fx.metal = v.clamp(0.0, 1.0);
                }
                if let Some(v) = n("glow") {
                    fx.emissive = v.max(0.0);
                }
                if let Some(c) = a.get("glow_color").and_then(v3) {
                    fx.emissive_color = c.to_array();
                }
                if let Some(v) = a.get("emit_light").and_then(|v| v.as_bool()) {
                    fx.emit_light = v;
                }
                for (key, slot) in [("normal_map", &mut fx.normal_map), ("height_map", &mut fx.height_map), ("metal_map", &mut fx.metal_map), ("rough_map", &mut fx.rough_map)] {
                    if let Some(p) = s(key) {
                        *slot = if p.is_empty() { None } else { Some(p) };
                    }
                }
                self.graph.mat_fx.insert(name.clone(), fx);
                Ok(json!(format!("{name} updated")))
            }
            "undo" => {
                self.do_undo();
                Ok(json!("undone"))
            }
            _ => self.tool_more(name, a).unwrap_or_else(|| Err(format!("unknown tool {name}"))),
        }
    }
}
