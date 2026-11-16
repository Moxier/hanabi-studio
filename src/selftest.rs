//! `--selftest <folder>`: load every model, accessory, motion and song under a folder without
//! opening a window, then play real dances on real models with physics, and print a report.
//! Used to test the studio against community files before a release.

use crate::formats::{self, FileKind};
use crate::scene::{ModelInstance, Scene};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if p.is_dir() {
            // Archives that were not unpacked live in "!Unzip" folders; skip them.
            if name.starts_with('!') {
                continue;
            }
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn short(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

struct ModelResult {
    path: PathBuf,
    data: Option<std::sync::Arc<crate::model::ModelData>>,
}

pub fn run(root: &Path, deep: bool) -> i32 {
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut failures = 0;
    let mut models: Vec<ModelResult> = Vec::new();
    let mut motions: Vec<(PathBuf, crate::motion::Motion)> = Vec::new();
    let mut played = false;
    println!("# Self-test of {}\n", root.display());
    println!("## Files\n");
    for p in &files {
        let kind = formats::kind_of(p);
        if !matches!(kind, FileKind::Model | FileKind::Accessory | FileKind::Motion | FileKind::Audio | FileKind::Pose) {
            continue;
        }
        let name = short(root, p);
        let t0 = Instant::now();
        let bytes = match std::fs::read(p) {
            Ok(b) => b,
            Err(e) => {
                println!("FAIL  {name}: can't read ({e})");
                failures += 1;
                continue;
            }
        };
        let read_ms = t0.elapsed().as_secs_f32() * 1000.0;
        match kind {
            FileKind::Model | FileKind::Accessory => {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let t1 = Instant::now();
                let r = match ext.as_str() {
                    "pmd" => formats::pmd::read(&bytes),
                    "x" => formats::xfile::read(&bytes),
                    _ => formats::pmx::read(&bytes),
                };
                let parse_ms = t1.elapsed().as_secs_f32() * 1000.0;
                match r {
                    Ok(mut m) => {
                        m.path = Some(p.clone());
                        m.finalize();
                        let dir = p.parent().unwrap_or(Path::new("."));
                        let mut missing = Vec::new();
                        let mut undecodable = Vec::new();
                        let t2 = Instant::now();
                        for t in &m.textures {
                            let tt = t.trim();
                            // Folder names and "." / ".." are placeholders some editors leave behind.
                            if tt.is_empty() || tt.ends_with('/') || tt.ends_with('\\') || tt.trim_matches('.').is_empty() {
                                continue;
                            }
                            match crate::render::find_texture(dir, t) {
                                Some(tp) => {
                                    if deep && crate::render::decode_image(&tp).is_none() {
                                        undecodable.push(t.clone());
                                    }
                                }
                                None => {
                                    // toon01..10 refer to the shared ramps, which are built in.
                                    let l = t.to_lowercase();
                                    if !(l.starts_with("toon") && l.len() <= 10) {
                                        missing.push(t.clone());
                                    }
                                }
                            }
                        }
                        let tex_ms = t2.elapsed().as_secs_f32() * 1000.0;
                        let t3 = Instant::now();
                        let inst = ModelInstance::new(0, m);
                        let build_ms = t3.elapsed().as_secs_f32() * 1000.0;
                        let d = inst.data.clone();
                        let (mn, mx) = d.bounds();
                        println!(
                            "ok    {name}: “{}” {} verts, {} mats, {} tex, {} bones, {} morphs, {} bodies, {} joints, height {:.1} | read {read_ms:.0} ms, parse {parse_ms:.0} ms, textures {tex_ms:.0} ms, setup {build_ms:.0} ms",
                            d.name,
                            d.vertices.len(),
                            d.materials.len(),
                            d.textures.len(),
                            d.bones.len(),
                            d.morphs.len(),
                            d.rigid_bodies.len(),
                            d.joints.len(),
                            mx.y - mn.y
                        );
                        if !missing.is_empty() {
                            println!("      missing textures ({}): {}", missing.len(), missing.iter().take(6).cloned().collect::<Vec<_>>().join(", "));
                        }
                        if !undecodable.is_empty() {
                            println!("      textures that can't be decoded ({}): {}", undecodable.len(), undecodable.iter().take(6).cloned().collect::<Vec<_>>().join(", "));
                        }
                        let bad_bodies: Vec<String> = d
                            .rigid_bodies
                            .iter()
                            .filter(|b| !(b.size.is_finite() && b.position.is_finite() && b.rotation.is_finite() && b.mass.is_finite() && b.linear_damping.is_finite() && b.angular_damping.is_finite() && b.friction.is_finite() && b.restitution.is_finite()))
                            .map(|b| format!("{} size {:?} pos {:?} rot {:?} mass {}", b.name, b.size, b.position, b.rotation, b.mass))
                            .collect();
                        let bad_joints: Vec<String> = d
                            .joints
                            .iter()
                            .filter(|j| !(j.position.is_finite() && j.rotation.is_finite() && j.lin_min.is_finite() && j.lin_max.is_finite() && j.ang_min.is_finite() && j.ang_max.is_finite() && j.spring_lin.is_finite() && j.spring_ang.is_finite()))
                            .map(|j| format!("{} lin {:?}..{:?} ang {:?}..{:?} spring {:?} {:?}", j.name, j.lin_min, j.lin_max, j.ang_min, j.ang_max, j.spring_lin, j.spring_ang))
                            .collect();
                        if !bad_bodies.is_empty() || !bad_joints.is_empty() {
                            println!("      broken numbers in {} rigid bodies / {} joints (fixed on load): {}", bad_bodies.len(), bad_joints.len(), bad_bodies.iter().chain(bad_joints.iter()).take(3).cloned().collect::<Vec<_>>().join("; "));
                        }
                        if let Ok(want) = std::env::var("HST_DUMP") {
                            for (ri, rb) in d.rigid_bodies.iter().enumerate() {
                                let bname = d.bones.get(rb.bone.max(0) as usize).map(|b| b.name.as_str()).unwrap_or("-");
                                if bname.contains(want.as_str()) || rb.name.contains(want.as_str()) {
                                    println!("      body #{ri} “{}” bone {bname} ({}) mode {:?} shape {:?} size {:?} mass {} damp {}/{} group {} mask {:016b}", rb.name, rb.bone, rb.mode, rb.shape, rb.size, rb.mass, rb.linear_damping, rb.angular_damping, rb.group, rb.mask);
                                    for j in d.joints.iter().filter(|j| j.a == ri as i32 || j.b == ri as i32) {
                                        let other = if j.a == ri as i32 { j.b } else { j.a };
                                        let on = d.rigid_bodies.get(other.max(0) as usize).map(|r| (r.name.clone(), r.mode)).unwrap_or_default_name();
                                        println!("        joint “{}” {}→{} (other: {:?}) lin {:?}..{:?} ang {:?}..{:?} spring {:?}/{:?}", j.name, j.a, j.b, on, j.lin_min, j.lin_max, j.ang_min, j.ang_max, j.spring_lin, j.spring_ang);
                                    }
                                    if let Some(b) = d.bones.get(rb.bone.max(0) as usize) {
                                        println!("        bone parent {} ({}), pos {:?}", b.parent, d.bones.get(b.parent.max(0) as usize).map(|p| p.name.as_str()).unwrap_or("-"), b.position);
                                    }
                                }
                            }
                        }
                        if !d.is_accessory {
                            models.push(ModelResult { path: p.clone(), data: Some(d) });
                        }
                    }
                    Err(e) => {
                        if bytes.is_empty() {
                            println!("SKIP  {name}: empty file (0 bytes)");
                        } else {
                            println!("FAIL  {name}: {e}");
                            failures += 1;
                        }
                        models.push(ModelResult { path: p.clone(), data: None });
                    }
                }
            }
            FileKind::Motion => {
                let t1 = Instant::now();
                match formats::vmd::read(&bytes) {
                    Ok(m) => {
                        let ms = t1.elapsed().as_secs_f32() * 1000.0;
                        let bk: usize = m.bones.values().map(|v| v.len()).sum();
                        let mk: usize = m.morphs.values().map(|v| v.len()).sum();
                        println!(
                            "ok    {name}: for “{}”, {} bone tracks / {bk} keys, {} morph tracks / {mk} keys, {} camera, {} light, last frame {} | {ms:.0} ms",
                            m.model_name,
                            m.bones.len(),
                            m.morphs.len(),
                            m.camera.len(),
                            m.light.len(),
                            m.max_frame()
                        );
                        // Round trip: write as VMD and read back, the keys must match.
                        let back = formats::vmd::read(&formats::vmd::write(&m));
                        match back {
                            Ok(b) => {
                                let bk2: usize = b.bones.values().map(|v| v.len()).sum();
                                let mk2: usize = b.morphs.values().map(|v| v.len()).sum();
                                let same_pose = m.bones.iter().all(|(n, v)| {
                                    b.bones.get(n).map(|w| w.len() == v.len() && v.iter().zip(w).all(|(a, c)| a.frame == c.frame && (a.pos - c.pos).length() < 1e-4 && a.rot.dot(c.rot).abs() > 0.9999 && a.curves == c.curves)).unwrap_or(false)
                                });
                                if bk2 != bk || mk2 != mk || b.camera.len() != m.camera.len() || !same_pose {
                                    println!("FAIL  {name}: saving as VMD changed the keys ({bk}->{bk2} bone, {mk}->{mk2} morph, same values: {same_pose})");
                                    failures += 1;
                                }
                            }
                            Err(e) => {
                                println!("FAIL  {name}: VMD save/reload failed: {e}");
                                failures += 1;
                            }
                        }
                        motions.push((p.clone(), m));
                    }
                    Err(e) => {
                        println!("FAIL  {name}: {e}");
                        failures += 1;
                    }
                }
            }
            FileKind::Audio => match formats::wav::read(&bytes) {
                Ok(a) => {
                    let t1 = Instant::now();
                    let pcm = formats::wav::decode_stereo16(&bytes);
                    let dec_ms = t1.elapsed().as_secs_f32() * 1000.0;
                    match pcm {
                        Ok(_) => println!("ok    {name}: {} Hz, {} ch, {:.1} s | read {read_ms:.0} ms, decoded for playback in {dec_ms:.0} ms", a.sample_rate, a.channels, a.duration),
                        Err(e) => {
                            println!("FAIL  {name}: can't be played inside the app: {e}");
                            failures += 1;
                        }
                    }
                    if std::env::var_os("HANABI_SOUND_DEVICE").is_some() && !played {
                        played = true;
                        let r = formats::wav::decode_stereo16(&bytes).and_then(|pcm| crate::audio::test_output(&pcm, 10.0, 0.5));
                        let alive = r.is_ok();
                        println!("{}  sound output: {}", if alive { "ok  " } else { "FAIL" }, match &r { Ok(n) => format!("{n} played 0.5 s of {name}"), Err(e) => e.clone() });
                        if !alive {
                            failures += 1;
                        }
                    }
                }
                Err(e) => {
                    println!("FAIL  {name}: {e}");
                    failures += 1;
                }
            },
            FileKind::Pose => match formats::vpd::read(&bytes) {
                Ok(v) => println!("ok    {name}: pose with {} bones", v.bones.len()),
                Err(e) => {
                    println!("FAIL  {name}: {e}");
                    failures += 1;
                }
            },
            _ => {}
        }
    }

    // Play dances on dancers with physics on and look for trouble.
    let dancers: Vec<&ModelResult> = models.iter().filter(|m| m.data.as_ref().map(|d| d.rigid_bodies.len() > 0 && d.bone_index("センター").is_some()).unwrap_or(false)).collect();
    let dances: Vec<&(PathBuf, crate::motion::Motion)> = motions.iter().filter(|(_, m)| m.bones.len() > 20 && m.max_frame() > 300).collect();
    if !dancers.is_empty() && !dances.is_empty() {
        println!("\n## Dances with physics\n");
        let pairs = if deep { dancers.len().max(dances.len()) } else { dancers.len() };
        for k in 0..pairs {
            let model = dancers[k % dancers.len()];
            let (mp, motion) = dances[k % dances.len()];
            let d = model.data.as_ref().unwrap();
            let mut scene = Scene::new();
            scene.models.clear();
            let i = scene.add_model((**d).clone());
            let mut motion = motion.clone();
            let renamed = motion.fit_to_model(d);
            let motion = &motion;
            scene.models[i].motion = motion.clone();
            let matched = motion.bones.keys().filter(|n| d.bone_index(n).is_some()).count();
            let morph_matched = motion.morphs.keys().filter(|n| d.morph_index(n).is_some()).count();
            let end = motion.max_frame().min(if deep { 100_000 } else { 1800 });
            scene.playing = true;
            let t0 = Instant::now();
            let mut worst_stretch = 0.0f32;
            let mut worst_frame = 0;
            let mut worst_gap = 0.0f32;
            let mut worst_bone = String::new();
            let mut nan_frame = None;
            let mut f = 0.0;
            scene.force_reset_physics();
            while f <= end as f32 {
                scene.frame = f;
                scene.evaluate();
                let m = &scene.models[i];
                if nan_frame.is_none() && m.skin.iter().any(|s| !s.is_finite()) {
                    nan_frame = Some(f as u32);
                }
                // How far physics bones drift from their parents compared with the rest pose.
                // Only while the model is shown: dance files hide extra dancers at times.
                for (bi, b) in d.bones.iter().enumerate().filter(|_| m.visible) {
                    if !m.physics_bones[bi] || b.parent < 0 {
                        continue;
                    }
                    let rest = (b.position - d.bones[b.parent as usize].position).length();
                    if rest < 0.05 {
                        continue;
                    }
                    let now = (m.skel.global_pos(bi) - m.skel.global_pos(b.parent as usize)).length();
                    let s = now / rest;
                    if s > worst_stretch {
                        worst_stretch = s;
                        worst_frame = f as u32;
                    }
                    if now - rest > worst_gap {
                        worst_gap = now - rest;
                        worst_bone = format!("{} at frame {}", b.name, f as u32);
                    }
                }
                f += 1.0;
            }
            let secs = t0.elapsed().as_secs_f32();
            let fps = (end as f32 + 1.0) / secs.max(1e-6);
            let verdict = if nan_frame.is_some() {
                failures += 1;
                "FAIL"
            } else if worst_gap > 2.0 {
                failures += 1;
                "WARN"
            } else {
                "ok  "
            };
            println!(
                "{verdict}  {} + {}: {matched}/{} bones, {morph_matched}/{} morphs matched ({renamed} long names restored), {} frames at {fps:.0} fps (motion + IK + physics), worst physics stretch ×{worst_stretch:.2} at frame {worst_frame}, widest gap {worst_gap:.2} units ({worst_bone}){}",
                short(root, &model.path),
                short(root, mp),
                motion.bones.len(),
                motion.morphs.len(),
                end + 1,
                nan_frame.map(|f| format!(", broken numbers from frame {f}")).unwrap_or_default()
            );
        }
    }
    // Crowd: five dancers with physics on the biggest stage, all on one timeline.
    if dancers.len() >= 2 && !dances.is_empty() {
        let stage = models.iter().filter_map(|m| m.data.as_ref()).filter(|d| d.rigid_bodies.is_empty() || d.bone_index("センター").is_none()).max_by_key(|d| d.vertices.len());
        let mut scene = Scene::new();
        scene.models.clear();
        let mut verts = 0;
        if let Some(st) = stage {
            scene.add_model((**st).clone());
            verts += st.vertices.len();
        }
        let n = dancers.len().min(5);
        for k in 0..n {
            let d = dancers[k].data.as_ref().unwrap();
            let i = scene.add_model((**d).clone());
            let mut mo = dances[k % dances.len()].1.clone();
            mo.fit_to_model(d);
            scene.models[i].motion = mo;
            scene.models[i].world = glam::Mat4::from_translation(glam::Vec3::new((k as f32 - 2.0) * 12.0, 0.0, 0.0));
            verts += d.vertices.len();
        }
        scene.playing = true;
        scene.force_reset_physics();
        let t0 = Instant::now();
        let frames = 600;
        for f in 0..frames {
            scene.frame = f as f32;
            scene.evaluate();
        }
        let fps = frames as f32 / t0.elapsed().as_secs_f32();
        let bodies: usize = scene.models.iter().map(|m| m.data.rigid_bodies.len()).sum();
        println!("\n## Crowd\n\n{}  {n} dancers + stage ({verts} vertices, {bodies} rigid bodies): motion + IK + physics at {fps:.0} fps", if fps >= 60.0 { "ok  " } else { "SLOW" });
        if fps < 60.0 {
            failures += 1;
        }
    }
    println!("\n{} problem(s) found.", failures);
    failures
}

trait OrDefaultName {
    fn unwrap_or_default_name(self) -> (String, crate::model::RigidMode);
}
impl OrDefaultName for Option<(String, crate::model::RigidMode)> {
    fn unwrap_or_default_name(self) -> (String, crate::model::RigidMode) {
        self.unwrap_or(("-".into(), crate::model::RigidMode::FollowBone))
    }
}
