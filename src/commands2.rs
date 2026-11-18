//! MCP tools for everything added after the first set: workspaces, project status and credits,
//! scene objects and groups, audio, timeline, bone selection and animation tools, separate
//! camera / light keys, physics (global settings, per-part editing, undo) and render settings.
//! Each tool does what the matching button or slider in the app does.

use crate::app::{App, Workspace};
use crate::model::{RigidMode, Shape};
use crate::ui::animtools::{Scope, SelOp};
use serde_json::{json, Value};

fn v3(v: &Value) -> Option<glam::Vec3> {
    let a = v.as_array()?;
    Some(glam::Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}

fn mode_name(m: RigidMode) -> &'static str {
    match m {
        RigidMode::FollowBone => "follow_bone",
        RigidMode::Physics => "physics",
        RigidMode::PhysicsWithBonePosition => "physics_bone_position",
    }
}

/// Schemas of the tools in this file (added to the list in mcp.rs).
pub fn tools_more() -> Vec<Value> {
    let s = |d: &str| json!({"type": "string", "description": d});
    let n = |d: &str| json!({"type": "number", "description": d});
    let b = |d: &str| json!({"type": "boolean", "description": d});
    let arr = |d: &str| json!({"type": "array", "items": {"type": "number"}, "description": d});
    let sarr = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    let tool = |name: &str, desc: &str, props: Value, req: &[&str]| json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": req}});
    vec![
        // ── App and project
        tool("get_status", "What the app is doing: workspace, status message, unsaved changes, project file, undo steps (animation and physics), ffmpeg (video export) and sound output.", json!({}), &[]),
        tool("set_workspace", "Switch workspace: animation | effects | color | render | graph | home (welcome screen).", json!({"name": s("workspace")}), &["name"]),
        tool("new_project", "Start an empty project. Refuses when there are unsaved changes unless discard_unsaved=true.", json!({"discard_unsaved": b("throw away unsaved changes")}), &[]),
        tool("get_credits", "Credits list of every loaded model, stage, motion and song (text for a video description), plus the readme / terms files found next to each.", json!({}), &[]),
        // ── Scene objects and groups
        tool("list_objects", "Every object (model, stage, prop) with index, kind, group, visibility and physics; and the object groups.", json!({}), &[]),
        tool("set_object", "Change an object (by index or name): visible, name (rename), physics (on/off), outline_width, group (group name; '' = no group); for props also position [x,y,z], rotation [x,y,z] degrees, scale, opacity, attach_to_model + attach_to_bone (or attach_to_model='' for the ground).",
            json!({"index": n("object index"), "name": s("object name"), "visible": b("shown"), "rename": s("new name"), "physics": b("physics on"), "outline_width": n("0..3"), "group": s("group name or ''"), "position": arr("prop position"), "rotation": arr("prop rotation, degrees"), "scale": n("prop scale"), "opacity": n("0..1"), "attach_to_model": s("model name"), "attach_to_bone": s("bone name")}), &[]),
        tool("duplicate_object", "Duplicate an object (by index or name) with its motion.", json!({"index": n("object index"), "name": s("object name")}), &[]),
        tool("delete_object", "Remove an object (by index or name) from the project, with its keyframes. Needs confirm=true.", json!({"index": n("object index"), "name": s("object name"), "confirm": b("really delete")}), &["confirm"]),
        tool("object_group", "Object groups (like collections). action: create (name, parent) | rename (name, new_name) | show | hide | move (name, parent; '' = top level) | ungroup (keeps objects) | delete (with its objects, needs confirm=true).",
            json!({"action": s("create | rename | show | hide | move | ungroup | delete"), "name": s("group name"), "new_name": s("for rename"), "parent": s("parent group name, '' for top level"), "confirm": b("for delete")}), &["action", "name"]),
        // ── Audio and timeline
        tool("set_audio", "Song settings: volume (0..2, 1 = as recorded), muted, offset_frames (the song starts at this frame), play_with_frame (hear the song's slice when moving frames while stopped), remove=true to take the song out.",
            json!({"volume": n("0..2"), "muted": b("mute"), "offset_frames": n("start frame of the song"), "play_with_frame": b("Play WAV with frame"), "remove": b("remove the song")}), &[]),
        tool("set_timeline", "Timeline: play range start/end (frames), animation frame rate fps (24/30/60; VMD motions are 30), loop playback, follow (keep playhead in view).",
            json!({"start": n("first frame"), "end": n("last frame"), "fps": n("animation frames per second"), "loop": b("loop playback"), "follow": b("keep playhead in view")}), &[]),
        // ── Animation tools
        tool("select_bones", "Select bones on the selected model: names (list) and/or op all | none | invert | mirror | children | parent | group | keyed. add=true keeps the current selection. Returns the selection.",
            json!({"names": sarr("bone names"), "op": s("selection operation"), "add": b("add to the selection")}), &[]),
        tool("key_bones", "Register keys at the current frame for the selected bones and edited bones/morphs (like Enter). all_bones=true keys every posable bone.", json!({"all_bones": b("key every bone")}), &[]),
        tool("reset_bones", "Reset the selected bones' rotation and/or position (rotation/position true), or the whole pose (whole_pose=true). Unregistered until keyed, unless auto-key is on.",
            json!({"rotation": b("reset rotation"), "position": b("reset position"), "whole_pose": b("every bone")}), &[]),
        tool("transform_bones", "Multiply and offset the selected bones: scope pose (the current pose) | range (keys in the play range) | all (every key). position_multiply [x,y,z], rotation_multiply (scales the turn), position_add [x,y,z], rotation_add [x,y,z] degrees.",
            json!({"scope": s("pose | range | all"), "position_multiply": arr("[x,y,z]"), "rotation_multiply": n("factor"), "position_add": arr("[x,y,z]"), "rotation_add": arr("[x,y,z] degrees")}), &[]),
        tool("retime", "Timing: action insert (frames at the playhead) | delete (frames at the playhead) | stretch (play range by factor) | reverse (play range). selected_only = only the selected bones; include_camera also moves camera and light keys.",
            json!({"action": s("insert | delete | stretch | reverse"), "frames": n("for insert/delete"), "factor": n("for stretch"), "selected_only": b("selected bones only"), "include_camera": b("camera and light too")}), &["action"]),
        tool("pose_tools", "Pose helpers: action mirror (swap left/right) | copy (copy the pose) | paste | paste_mirrored | smooth (smooth the selected bones' keys) | delete_keys_here (selected bones' keys at the playhead) | auto_key_on | auto_key_off.",
            json!({"action": s("mirror | copy | paste | paste_mirrored | smooth | delete_keys_here | auto_key_on | auto_key_off")}), &["action"]),
        tool("key_camera_light", "Register a camera key and/or a light key at the current frame (camera/light true).", json!({"camera": b("key the camera"), "light": b("key the light")}), &[]),
        tool("redo", "Redo the last undone animation edit.", json!({}), &[]),
        // ── Physics
        tool("get_physics", "Physics settings (gravity, time step, sub-steps, damping and spring multipliers, wind, stability), per-model state (on, tweaked parts, blow-ups) and physics undo steps.", json!({}), &[]),
        tool("set_physics", "Change scene physics (live; undoable with physics_undo). Fields: enabled, gravity, gravity_dir [x,y,z], time_step ('frame' or 60/120/240 for fixed 1/N s), substeps, damping_multiplier, spring_multiplier, wind_strength, wind_dir (degrees), wind_gusts, air_drag, max_speed, big_move (snap|carry|off), big_move_distance, settle_steps, show_rigid_bodies, show_joints, show_physics_bones, defaults=true resets all.",
            json!({"settings": json!({"type": "object"})}), &["settings"]),
        tool("list_rigid_bodies", "Rigid bodies of the selected model (index, name, bone, mode, shape, size, mass, damping, bounce, friction, group, collides_with, changed).", json!({"filter": s("name contains")}), &[]),
        tool("list_joints", "Joints of the selected model (index, name, the two bodies, move/turn limits, springs, changed).", json!({"filter": s("name contains")}), &[]),
        tool("edit_rigid_body", "Edit a rigid body of the selected model (by index or name); saved to the model's preset file, never the model. Fields: mode (physics | physics_bone_position | follow_bone), mass, linear_damping, angular_damping, restitution, friction, group (1..16), collides_with [groups], no_collision_with [groups], size [x,y,z], shape (sphere|box|capsule). The simulation is rebuilt and stability-checked; a warning is returned if it looks unstable (undo with physics_undo).",
            json!({"index": n("body index"), "name": s("body name"), "mode": s("mode"), "mass": n("> 0"), "linear_damping": n("0..1"), "angular_damping": n("0..1"), "restitution": n("0..1"), "friction": n("0..2"), "group": n("1..16"), "collides_with": arr("group numbers 1..16"), "no_collision_with": arr("group numbers 1..16"), "size": arr("[x,y,z]"), "shape": s("sphere | box | capsule")}), &[]),
        tool("edit_joint", "Edit a joint of the selected model (by index or name): move_min/move_max [x,y,z], turn_min/turn_max [x,y,z] degrees, move_spring/turn_spring [x,y,z]. Equal min and max lock an axis; min > max frees it. Rebuilt and stability-checked like edit_rigid_body.",
            json!({"index": n("joint index"), "name": s("joint name"), "move_min": arr("[x,y,z]"), "move_max": arr("[x,y,z]"), "turn_min": arr("degrees"), "turn_max": arr("degrees"), "move_spring": arr("[x,y,z]"), "turn_spring": arr("[x,y,z]")}), &[]),
        tool("revert_physics", "Put the selected model's physics back to its own values: one part (body or joint name) or all=true.", json!({"body": s("rigid body name"), "joint": s("joint name"), "all": b("every part")}), &[]),
        tool("physics_undo", "Undo the last physics change (settings or parts).", json!({}), &[]),
        tool("physics_redo", "Redo a physics change.", json!({}), &[]),
        tool("reset_physics", "Put every hair, skirt and prop part back on its bone at the current frame (reset to the animation pose).", json!({}), &[]),
        // ── Render
        tool("get_render_settings", "Current render settings (size, video frame rate vs animation frame rate, range, format, quality, song, path) and whether ffmpeg is available.", json!({}), &[]),
        tool("set_render_settings", "Change render settings without starting: width, height, fps (video frame rate; the animation keeps its own rate), start, end, format (mp4|webm|mov|png), quality (12..32, lower = better; Best 14, High 18, Balanced 23, Small 28), include_audio, msaa, open_when_done, path.",
            json!({"width": n("px"), "height": n("px"), "fps": n("video fps"), "start": n("frame"), "end": n("frame"), "format": s("mp4 | webm | mov | png"), "quality": n("12..32"), "include_audio": b("song"), "msaa": b("smooth edges"), "open_when_done": b("open folder"), "path": s("output file or folder")}), &[]),
        tool("cancel_render", "Stop the running render.", json!({}), &[]),
    ]
}

impl App {
    fn obj_index(&self, a: &Value) -> Result<usize, String> {
        if let Some(i) = a.get("index").and_then(|v| v.as_u64()) {
            let i = i as usize;
            return if i < self.scene.models.len() { Ok(i) } else { Err("index out of range".into()) };
        }
        if let Some(nm) = a.get("name").and_then(|v| v.as_str()) {
            return self.scene.models.iter().position(|m| m.name() == nm).or_else(|| self.scene.models.iter().position(|m| m.name().contains(nm))).ok_or(format!("no object named {nm}"));
        }
        self.scene.selected.filter(|&i| i < self.scene.models.len()).ok_or("give index or name (no object selected)".into())
    }

    fn group_by_name(&self, name: &str) -> Option<u64> {
        self.scene.groups.iter().find(|g| g.name == name).or_else(|| self.scene.groups.iter().find(|g| g.name.eq_ignore_ascii_case(name))).map(|g| g.id)
    }

    fn sel_model(&self) -> Result<usize, String> {
        self.anim_model().ok_or_else(|| "Select a model first (select_model)".to_string())
    }

    pub fn tool_more(&mut self, name: &str, a: &Value) -> Option<Result<Value, String>> {
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let n = |k: &str| a.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
        let ob = |k: &str| a.get(k).and_then(|v| v.as_bool());
        let b = |k: &str| ob(k).unwrap_or(false);
        Some(match name {
            "get_status" => Ok(json!({
                "workspace": if self.home { "home".to_string() } else { format!("{:?}", self.ws).to_lowercase() },
                "status": self.status, "status_is_error": self.status_err,
                "unsaved_changes": self.dirty,
                "project": self.project_path.as_ref().map(|p| p.display().to_string()),
                "animation_undo_steps": self.undo.len(), "animation_redo_steps": self.redo.len(),
                "physics_undo_steps": self.phys_undo.len(), "physics_redo_steps": self.phys_redo.len(),
                "physics_warning": self.phys_warning.as_ref().map(|w| w.1.clone()),
                "ffmpeg": crate::export::ffmpeg_path().map(|p| p.display().to_string()),
                "sound_output": self.audio.backend,
                "auto_key": self.auto_key,
                "selected_bones": self.anim_model().map(|mi| self.selected_bones().iter().map(|&b| self.scene.models[mi].data.bones[b].name.clone()).collect::<Vec<_>>()).unwrap_or_default(),
                "camera_selected": self.sel_camera,
            })),
            "set_workspace" => {
                let w = s("name").unwrap_or_default().to_lowercase();
                if w == "home" {
                    self.home = true;
                    return Some(Ok(json!("welcome screen")));
                }
                self.home = false;
                self.ws = match w.as_str() {
                    x if x.starts_with("ani") => Workspace::Animation,
                    x if x.starts_with("eff") => Workspace::Effects,
                    x if x.starts_with("col") => Workspace::Color,
                    x if x.starts_with("ren") => Workspace::Render,
                    x if x.starts_with("gra") => Workspace::Graph,
                    _ => return Some(Err("workspace must be animation, effects, color, render, graph or home".into())),
                };
                Ok(json!(format!("workspace: {w}")))
            }
            "new_project" => {
                self.refresh_dirty(true);
                if self.dirty && !b("discard_unsaved") {
                    return Some(Err("there are unsaved changes: save_project first, or pass discard_unsaved=true".into()));
                }
                self.new_project();
                Ok(json!("new empty project"))
            }
            "get_credits" => {
                let mut files = Vec::new();
                for m in &self.scene.models {
                    if let Some(p) = &m.data.path {
                        files.push(json!({"file": p.display().to_string(), "readmes": crate::ui::credits::credit_files(p).iter().map(|f| f.display().to_string()).collect::<Vec<_>>()}));
                    }
                }
                for p in &self.scene.sources {
                    files.push(json!({"file": p.display().to_string(), "readmes": crate::ui::credits::credit_files(p).iter().map(|f| f.display().to_string()).collect::<Vec<_>>()}));
                }
                Ok(json!({"credits_text": self.credits_text(), "files": files}))
            }
            "list_objects" => {
                let objs: Vec<Value> = self
                    .scene
                    .models
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let character = !m.data.rigid_bodies.is_empty() || m.data.bone_index("上半身").is_some() || m.data.bone_index("頭").is_some();
                        let (lo, hi) = m.data.bounds();
                        let small = (hi - lo).max_element() < 25.0;
                        json!({
                            "index": i, "name": m.name(), "file_name": m.orig_name,
                            "kind": if m.is_accessory() || (!character && small) { "prop" } else if character { "model" } else { "stage" },
                            "outline_width": m.edge_width,
                            "placement": {"position": m.acc_pos.to_array(), "rotation_deg": m.acc_rot.to_array(), "scale": m.acc_scale, "opacity": m.acc_opacity,
                                "attached_to": m.attach.and_then(|(pid, bi)| self.scene.model_by_id(pid).map(|pi| json!({"model": self.scene.models[pi].name(), "bone": self.scene.models[pi].data.bones.get(bi).map(|b| b.name.clone())})))},
                            "group": m.group.and_then(|g| self.scene.group_index(g)).map(|g| self.scene.groups[g].name.clone()),
                            "visible": m.visible, "shown": m.shown(), "physics": m.physics_enabled,
                            "file": m.data.path.as_ref().map(|p| p.display().to_string()),
                            "keys": m.motion.key_count(), "selected": self.scene.selected == Some(i),
                        })
                    })
                    .collect();
                let groups: Vec<Value> = self
                    .scene
                    .groups
                    .iter()
                    .map(|g| json!({"name": g.name, "parent": g.parent.and_then(|p| self.scene.group_index(p)).map(|p| self.scene.groups[p].name.clone()), "visible": g.visible, "objects": self.scene.group_object_count(g.id)}))
                    .collect();
                Ok(json!({"objects": objs, "groups": groups}))
            }
            "set_object" => (|| {
                let i = self.obj_index(a)?;
                let group = match s("group") {
                    Some(g) if g.is_empty() => Some(None),
                    Some(g) => Some(Some(self.group_by_name(&g).ok_or(format!("no group named {g}"))?)),
                    None => None,
                };
                let attach = match s("attach_to_model") {
                    Some(mn) if mn.is_empty() => Some(None),
                    Some(mn) => {
                        let pi = self.scene.models.iter().position(|m| m.name() == mn || m.name().contains(&mn)).ok_or(format!("no model named {mn}"))?;
                        let bn = s("attach_to_bone").ok_or("attach_to_bone is needed with attach_to_model")?;
                        let bi = self.scene.models[pi].data.bone_index(&bn).ok_or(format!("no bone {bn}"))?;
                        Some(Some((self.scene.models[pi].id, bi)))
                    }
                    None => None,
                };
                let m = &mut self.scene.models[i];
                if let Some(v) = ob("visible") {
                    m.visible = v;
                }
                if let Some(v) = ob("physics") {
                    m.physics_enabled = v;
                }
                if let Some(v) = n("outline_width") {
                    m.edge_width = v.clamp(0.0, 3.0);
                }
                if let Some(r) = s("rename").filter(|r| !r.trim().is_empty()) {
                    std::sync::Arc::make_mut(&mut m.data).name = r;
                }
                if let Some(p) = a.get("position").and_then(v3) {
                    m.acc_pos = p;
                }
                if let Some(r) = a.get("rotation").and_then(v3) {
                    m.acc_rot = r;
                }
                if let Some(v) = n("scale") {
                    m.acc_scale = v.max(0.001);
                }
                if let Some(v) = n("opacity") {
                    m.acc_opacity = v.clamp(0.0, 1.0);
                }
                if let Some(at) = attach {
                    m.attach = at;
                }
                if let Some(g) = group {
                    m.group = g;
                }
                let name = m.name().to_string();
                self.scene.sync_groups();
                Ok(json!(format!("updated {name}")))
            })(),
            "duplicate_object" => (|| {
                let i = self.obj_index(a)?;
                let (data, motion, attach, pos, rot, scale, group) = {
                    let m = &self.scene.models[i];
                    ((*m.data).clone(), m.motion.clone(), m.attach, m.acc_pos, m.acc_rot, m.acc_scale, m.group)
                };
                let mut data = data;
                data.name = format!("{} copy", data.name);
                let name = data.name.clone();
                let ni = self.scene.add_model(data);
                let m = &mut self.scene.models[ni];
                m.motion = motion;
                m.attach = attach;
                m.acc_pos = pos + glam::Vec3::new(5.0, 0.0, 0.0);
                m.acc_rot = rot;
                m.acc_scale = scale;
                m.group = group;
                self.scene.sync_groups();
                Ok(json!({"created": name, "index": ni}))
            })(),
            "delete_object" => (|| {
                if !b("confirm") {
                    return Err("pass confirm=true to delete".into());
                }
                let i = self.obj_index(a)?;
                let name = self.scene.models[i].name().to_string();
                self.scene.remove_model(i);
                self.sel_bone = None;
                self.mat_sel = None;
                Ok(json!(format!("removed {name}")))
            })(),
            "object_group" => (|| {
                let act = s("action").unwrap_or_default();
                let gname = s("name").unwrap_or_default();
                let parent = match s("parent") {
                    Some(p) if p.is_empty() => None,
                    Some(p) => Some(self.group_by_name(&p).ok_or(format!("no group named {p}"))?),
                    None => None,
                };
                if act == "create" {
                    let id = self.scene.new_group(&gname, parent);
                    return Ok(json!(format!("created group {gname} ({id})")));
                }
                let g = self.group_by_name(&gname).ok_or(format!("no group named {gname}"))?;
                let gi = self.scene.group_index(g).unwrap();
                match act.as_str() {
                    "rename" => self.scene.groups[gi].name = s("new_name").ok_or("new_name is required")?,
                    "show" => self.scene.groups[gi].visible = true,
                    "hide" => self.scene.groups[gi].visible = false,
                    "move" => {
                        if !self.scene.move_group(g, parent) {
                            return Err("a group can't go inside itself".into());
                        }
                    }
                    "ungroup" => self.scene.delete_group(g, true),
                    "delete" => {
                        if !b("confirm") {
                            return Err("pass confirm=true to delete the group and its objects".into());
                        }
                        self.scene.delete_group(g, false);
                    }
                    _ => return Err("action must be create, rename, show, hide, move, ungroup or delete".into()),
                }
                self.scene.sync_groups();
                Ok(json!(format!("{act}: {gname}")))
            })(),
            "set_audio" => (|| {
                if b("remove") {
                    self.audio.stop();
                    self.scene.audio = None;
                    return Ok(json!("song removed"));
                }
                if let Some(v) = ob("play_with_frame") {
                    self.scene.audio_scrub = v;
                }
                let t = self.scene.audio.as_mut().ok_or("no song loaded (load a .wav with load_file)")?;
                if let Some(v) = n("volume") {
                    t.volume = v.clamp(0.0, 2.0);
                }
                if let Some(v) = ob("muted") {
                    t.muted = v;
                }
                if let Some(v) = n("offset_frames") {
                    t.offset_frames = v.round() as i32;
                }
                Ok(json!({"volume": t.volume, "muted": t.muted, "offset_frames": t.offset_frames, "play_with_frame": self.scene.audio_scrub}))
            })(),
            "set_timeline" => {
                if let Some(v) = n("start") {
                    self.scene.play_start = v.max(0.0) as u32;
                }
                if let Some(v) = n("end") {
                    self.scene.play_end = (v.max(0.0) as u32).max(self.scene.play_start);
                }
                if let Some(v) = n("fps") {
                    self.scene.fps = v.clamp(1.0, 240.0);
                }
                if let Some(v) = ob("loop") {
                    self.scene.loop_play = v;
                }
                if let Some(v) = ob("follow") {
                    self.tl.follow = v;
                }
                Ok(json!({"start": self.scene.play_start, "end": self.scene.play_end, "fps": self.scene.fps, "loop": self.scene.loop_play}))
            }
            "select_bones" => (|| {
                let mi = self.sel_model()?;
                let has_names = a.get("names").and_then(|v| v.as_array()).map(|l| !l.is_empty()).unwrap_or(false);
                // With names: those bones (plus the current selection when add=true).
                // Without names: the operation works on the current selection.
                let mut set: std::collections::BTreeSet<usize> = if b("add") || !has_names { self.selected_bones().into_iter().collect() } else { Default::default() };
                let mut missing = Vec::new();
                if let Some(list) = a.get("names").and_then(|v| v.as_array()) {
                    for nm in list.iter().filter_map(|v| v.as_str()) {
                        match self.scene.models[mi].data.bone_index(nm) {
                            Some(i) => {
                                set.insert(i);
                            }
                            None => missing.push(nm.to_string()),
                        }
                    }
                }
                let id = self.scene.models[mi].id;
                let active = set.iter().next().copied();
                self.sel_bones = (id, set);
                self.sel_bone = active;
                self.sel_camera = false;
                if let Some(op) = s("op") {
                    let op = match op.as_str() {
                        "all" => SelOp::All,
                        "none" => SelOp::None,
                        "invert" => SelOp::Invert,
                        "mirror" => SelOp::Mirror,
                        "children" => SelOp::Children,
                        "parent" => SelOp::Parent,
                        "group" => SelOp::Group,
                        "keyed" => SelOp::Keyed,
                        _ => return Err("op must be all, none, invert, mirror, children, parent, group or keyed".into()),
                    };
                    self.select_op(op);
                }
                let names: Vec<String> = self.selected_bones().iter().map(|&b| self.scene.models[mi].data.bones[b].name.clone()).collect();
                Ok(json!({"selected": names, "not_found": missing}))
            })(),
            "key_bones" => {
                if b("all_bones") {
                    self.key_all_bones();
                } else {
                    self.register_selected();
                }
                Ok(json!(self.status.clone()))
            }
            "reset_bones" => {
                if b("whole_pose") {
                    self.reset_pose(false);
                    Ok(json!("whole pose reset (press key_bones to key it)"))
                } else {
                    let (p, r) = (b("position"), b("rotation"));
                    if !p && !r {
                        return Some(Err("say rotation=true and/or position=true (or whole_pose=true)".into()));
                    }
                    self.reset_selected(p, r);
                    Ok(json!(self.status.clone()))
                }
            }
            "transform_bones" => {
                let mut t = crate::ui::animtools::ToolParams::default();
                t.scope = match s("scope").as_deref() {
                    Some("range") => Scope::Range,
                    Some("all") => Scope::All,
                    _ => Scope::Pose,
                };
                if let Some(v) = a.get("position_multiply").and_then(v3) {
                    t.pos_mul = v;
                }
                if let Some(v) = n("rotation_multiply") {
                    t.rot_mul = v;
                }
                if let Some(v) = a.get("position_add").and_then(v3) {
                    t.pos_add = v;
                }
                if let Some(v) = a.get("rotation_add").and_then(v3) {
                    t.rot_add = v;
                }
                let keep = std::mem::replace(&mut self.tool, t);
                self.transform_selected();
                let msg = (self.status.clone(), self.status_err);
                self.tool = keep;
                if msg.1 {
                    Err(msg.0)
                } else {
                    Ok(json!(msg.0))
                }
            }
            "retime" => {
                let ph = self.scene.frame.round() as u32;
                let (a0, b0) = (self.scene.play_start, self.scene.play_end);
                let keep = (self.tool.only_selected, self.tool.with_camera);
                self.tool.only_selected = b("selected_only");
                self.tool.with_camera = b("include_camera");
                let nfr = n("frames").map(|v| v.max(1.0) as u32).unwrap_or(10);
                let r = match s("action").as_deref() {
                    Some("insert") => {
                        self.retime(&format!("Inserted {nfr} frames at {ph}"), move |f| Some(if f >= ph { f + nfr } else { f }));
                        Ok(())
                    }
                    Some("delete") => {
                        self.retime(&format!("Deleted {nfr} frames at {ph}"), move |f| if f < ph { Some(f) } else if f < ph + nfr { None } else { Some(f - nfr) });
                        Ok(())
                    }
                    Some("stretch") => {
                        let k = n("factor").unwrap_or(1.0).clamp(0.05, 20.0);
                        let shift = ((b0.saturating_sub(a0)) as f32 * (k - 1.0)).round() as i64;
                        self.retime(&format!("Stretched {a0}–{b0} ×{k}"), move |f| {
                            let nf = if f < a0 { f as i64 } else if f <= b0 { a0 as i64 + ((f - a0) as f32 * k).round() as i64 } else { f as i64 + shift };
                            Some(nf.max(0) as u32)
                        });
                        Ok(())
                    }
                    Some("reverse") => {
                        self.retime(&format!("Reversed {a0}–{b0}"), move |f| Some(if f >= a0 && f <= b0 { a0 + b0 - f } else { f }));
                        Ok(())
                    }
                    _ => Err("action must be insert, delete, stretch or reverse".to_string()),
                };
                self.tool.only_selected = keep.0;
                self.tool.with_camera = keep.1;
                r.map(|_| json!(self.status.clone()))
            }
            "pose_tools" => match s("action").as_deref() {
                Some("mirror") => {
                    self.mirror_current_pose();
                    Ok(json!(self.status.clone()))
                }
                Some("copy") => {
                    self.sel_keys.clear();
                    self.copy_keys_or_pose();
                    Ok(json!(self.status.clone()))
                }
                Some("paste") => {
                    self.paste_keys_or_pose(false);
                    Ok(json!(self.status.clone()))
                }
                Some("paste_mirrored") => {
                    self.paste_keys_or_pose(true);
                    Ok(json!(self.status.clone()))
                }
                Some("smooth") => (|| {
                    let mi = self.sel_model()?;
                    self.checkpoint();
                    let names: Vec<String> = self.selected_bones().iter().map(|&b| self.scene.models[mi].data.bones[b].name.clone()).collect();
                    for nm in &names {
                        if let Some(v) = self.scene.models[mi].motion.bones.get_mut(nm) {
                            crate::tools::smooth_track(v, 0.8);
                        }
                    }
                    Ok(json!(format!("smoothed {} bone(s)", names.len())))
                })(),
                Some("delete_keys_here") => {
                    self.delete_selected_bone_keys();
                    Ok(json!(self.status.clone()))
                }
                Some("auto_key_on") => {
                    self.auto_key = true;
                    Ok(json!("auto key on"))
                }
                Some("auto_key_off") => {
                    self.auto_key = false;
                    Ok(json!("auto key off"))
                }
                _ => Err("unknown action".into()),
            },
            "key_camera_light" => {
                let (c, l) = (b("camera"), b("light"));
                if !c && !l {
                    return Some(Err("say camera=true and/or light=true".into()));
                }
                if c {
                    self.key_camera();
                }
                if l {
                    self.key_light();
                }
                Ok(json!(format!("keyed {}{}{} at frame {}", if c { "camera" } else { "" }, if c && l { " and " } else { "" }, if l { "light" } else { "" }, self.scene.frame.round())))
            }
            "redo" => {
                self.do_redo();
                Ok(json!("redone"))
            }
            "get_physics" => Ok(json!({
                "enabled": self.scene.stage.physics,
                "settings": self.scene.physics_cfg,
                "substeps": self.scene.stage.physics_substeps,
                "models": self.scene.models.iter().filter(|m| m.physics.is_some()).map(|m| {
                    // Widest gap between a physics bone and its parent compared with the rest
                    // pose: a quick way to tell a torn or exploded chain from normal swinging.
                    let d = &m.data;
                    let (mut gap, mut gap_bone) = (0.0f32, String::new());
                    for (bi, bn) in d.bones.iter().enumerate() {
                        if !m.physics_bones.get(bi).copied().unwrap_or(false) || bn.parent < 0 {
                            continue;
                        }
                        let rest = (bn.position - d.bones[bn.parent as usize].position).length();
                        let now = (m.skel.global_pos(bi) - m.skel.global_pos(bn.parent as usize)).length();
                        if now - rest > gap {
                            gap = now - rest;
                            gap_bone = bn.name.clone();
                        }
                    }
                    json!({
                    "name": m.name(), "physics": m.physics_enabled,
                    "widest_gap": gap, "widest_gap_bone": gap_bone,
                    "drift_score": m.physics.as_ref().map(|p| p.last_score).unwrap_or(0.0),
                    "torn_links": m.physics.as_ref().map(|p| p.torn).unwrap_or(0),
                    "flipped_links": m.physics.as_ref().map(|p| p.flipped).unwrap_or(0),
                    "max_flip_degrees": m.physics.as_ref().map(|p| p.max_flip_deg).unwrap_or(0.0),
                    "flip_events": m.physics.as_ref().map(|p| p.flip_events).unwrap_or(0),
                    "hip_parts": m.physics.as_ref().map(|p| p.hip_parts()).unwrap_or(0),
                    "skirt_hem_lift": m.physics.as_ref().map(|p| p.hem_height).unwrap_or(0.0),
                    "last_flip_parts": m.physics.as_ref().map(|p| p.last_flip.iter().take(12).filter_map(|&b| d.bones.get(b).map(|x| x.name.clone())).collect::<Vec<_>>()).unwrap_or_default(),
                    "flipped_parts": m.physics.as_ref().map(|p| p.flipped_bones.iter().take(12).filter_map(|&b| d.bones.get(b).map(|x| x.name.clone())).collect::<Vec<_>>()).unwrap_or_default(),
                    "changed_bodies": m.phys_tweaks.bodies.len(), "changed_joints": m.phys_tweaks.joints.len(),
                    "blowups": m.physics.as_ref().map(|p| p.blowups).unwrap_or(0),
                    "failed": m.physics.as_ref().map(|p| p.failed).unwrap_or(false),
                })}).collect::<Vec<_>>(),
                "undo_steps": self.phys_undo.len(),
                "warning": self.phys_warning.as_ref().map(|w| w.1.clone()),
            })),
            "set_physics" => (|| {
                let st = a.get("settings").ok_or("settings is required")?;
                self.phys_push_undo("global physics setting (AI)");
                if st.get("defaults").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.scene.physics_cfg = Default::default();
                    self.scene.stage.physics_substeps = 2;
                }
                let f = |k: &str| st.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
                let bo = |k: &str| st.get(k).and_then(|v| v.as_bool());
                let c = &mut self.scene.physics_cfg;
                if let Some(v) = f("gravity") { c.gravity = v.max(0.0); }
                if let Some(v) = st.get("gravity_dir").and_then(v3) { c.gravity_dir = v.to_array(); }
                if let Some(v) = f("damping_multiplier") { c.damping_mul = v.clamp(0.0, 10.0); }
                if let Some(v) = f("spring_multiplier") { c.spring_mul = v.clamp(0.0, 10.0); }
                if let Some(v) = f("wind_strength") { c.wind_strength = v.max(0.0); }
                if let Some(v) = f("wind_dir") { c.wind_dir = v; }
                if let Some(v) = f("wind_gusts") { c.wind_gusts = v.clamp(0.0, 1.0); }
                if let Some(v) = f("air_drag") { c.air_drag = v.clamp(0.0, 1.0); }
                if let Some(v) = f("max_speed") { c.max_speed = v.max(1.0); }
                if let Some(v) = f("big_move_distance") { c.big_move_dist = v.max(0.1); }
                if let Some(v) = f("settle_steps") { c.settle_steps = v.max(0.0) as u32; }
                if let Some(v) = st.get("big_move").and_then(|v| v.as_str()) {
                    c.big_move = match v {
                        "snap" => crate::physics::BigMove::Snap,
                        "carry" | "soften" => crate::physics::BigMove::Soften,
                        "off" => crate::physics::BigMove::Off,
                        _ => return Err("big_move must be snap, carry or off".into()),
                    };
                }
                match st.get("time_step") {
                    Some(Value::String(t)) if t == "frame" => c.fixed_step = 0.0,
                    Some(v) if v.as_f64().is_some() => {
                        let x = v.as_f64().unwrap() as f32;
                        c.fixed_step = if x >= 1.0 { 1.0 / x } else { x.max(0.0) };
                    }
                    _ => {}
                }
                if let Some(v) = f("substeps") { self.scene.stage.physics_substeps = (v as u32).clamp(1, 8); }
                if let Some(v) = bo("enabled") { self.scene.stage.physics = v; }
                if let Some(v) = bo("show_rigid_bodies") { self.scene.stage.show_rigid = v; }
                if let Some(v) = bo("show_joints") { self.scene.stage.show_joints = v; }
                if let Some(v) = bo("show_physics_bones") { self.scene.stage.show_physics_bones = v; }
                Ok(json!({"settings": self.scene.physics_cfg, "substeps": self.scene.stage.physics_substeps, "enabled": self.scene.stage.physics}))
            })(),
            "list_rigid_bodies" => (|| {
                let mi = self.sel_model()?;
                let m = &self.scene.models[mi];
                let f = s("filter").unwrap_or_default();
                Ok(json!(m
                    .data
                    .rigid_bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| f.is_empty() || r.name.contains(&f))
                    .map(|(i, r)| json!({
                        "index": i, "name": r.name,
                        "bone": m.data.bones.get(r.bone.max(0) as usize).filter(|_| r.bone >= 0).map(|b| b.name.clone()),
                        "mode": mode_name(r.mode), "shape": format!("{:?}", r.shape).to_lowercase(), "size": r.size.to_array(),
                        "mass": r.mass, "linear_damping": r.linear_damping, "angular_damping": r.angular_damping,
                        "restitution": r.restitution, "friction": r.friction, "group": r.group + 1,
                        "collides_with": (0..16).filter(|g| r.mask & (1 << g) != 0).map(|g| g + 1).collect::<Vec<_>>(),
                        "changed": m.phys_tweaks.bodies.contains_key(&i),
                    }))
                    .collect::<Vec<_>>()))
            })(),
            "list_joints" => (|| {
                let mi = self.sel_model()?;
                let m = &self.scene.models[mi];
                let f = s("filter").unwrap_or_default();
                let bn = |k: i32| m.data.rigid_bodies.get(k.max(0) as usize).filter(|_| k >= 0).map(|r| r.name.clone());
                Ok(json!(m
                    .data
                    .joints
                    .iter()
                    .enumerate()
                    .filter(|(_, j)| f.is_empty() || j.name.contains(&f))
                    .map(|(i, j)| json!({
                        "index": i, "name": j.name, "body_a": bn(j.a), "body_b": bn(j.b),
                        "move_min": j.lin_min.to_array(), "move_max": j.lin_max.to_array(),
                        "turn_min_deg": (j.ang_min * 57.29578).to_array(), "turn_max_deg": (j.ang_max * 57.29578).to_array(),
                        "move_spring": j.spring_lin.to_array(), "turn_spring": j.spring_ang.to_array(),
                        "changed": m.phys_tweaks.joints.contains_key(&i),
                    }))
                    .collect::<Vec<_>>()))
            })(),
            "edit_rigid_body" => (|| {
                let mi = self.sel_model()?;
                let d = &self.scene.models[mi].data;
                let i = match (a.get("index").and_then(|v| v.as_u64()), s("name")) {
                    (Some(i), _) if (i as usize) < d.rigid_bodies.len() => i as usize,
                    (_, Some(nm)) => d.rigid_bodies.iter().position(|r| r.name == nm).ok_or(format!("no rigid body named {nm}"))?,
                    _ => return Err("give index or name".into()),
                };
                let mut rb = d.rigid_bodies[i].clone();
                if let Some(m) = s("mode") {
                    rb.mode = match m.as_str() {
                        "physics" => RigidMode::Physics,
                        "physics_bone_position" => RigidMode::PhysicsWithBonePosition,
                        "follow_bone" => RigidMode::FollowBone,
                        _ => return Err("mode must be physics, physics_bone_position or follow_bone".into()),
                    };
                }
                if let Some(v) = n("mass") { rb.mass = v.max(0.001); }
                if let Some(v) = n("linear_damping") { rb.linear_damping = v.clamp(0.0, 0.999); }
                if let Some(v) = n("angular_damping") { rb.angular_damping = v.clamp(0.0, 0.999); }
                if let Some(v) = n("restitution") { rb.restitution = v.clamp(0.0, 1.0); }
                if let Some(v) = n("friction") { rb.friction = v.clamp(0.0, 2.0); }
                if let Some(v) = n("group") { rb.group = (v as u8).clamp(1, 16) - 1; }
                if let Some(list) = a.get("collides_with").and_then(|v| v.as_array()) {
                    rb.mask = list.iter().filter_map(|g| g.as_u64()).filter(|g| (1..=16).contains(g)).fold(0u16, |m, g| m | (1 << (g - 1)));
                }
                if let Some(list) = a.get("no_collision_with").and_then(|v| v.as_array()) {
                    for g in list.iter().filter_map(|g| g.as_u64()).filter(|g| (1..=16).contains(g)) {
                        rb.mask &= !(1 << (g - 1));
                    }
                }
                if let Some(v) = a.get("size").and_then(v3) { rb.size = v.max(glam::Vec3::splat(0.01)); }
                if let Some(sh) = s("shape") {
                    rb.shape = match sh.as_str() {
                        "sphere" => Shape::Sphere,
                        "box" => Shape::Box,
                        "capsule" => Shape::Capsule,
                        _ => return Err("shape must be sphere, box or capsule".into()),
                    };
                }
                let name = rb.name.clone();
                self.phys_push_undo(&format!("rigid body {name} (AI)"));
                self.commit_part(mi, Some((i, rb)), None);
                let warn = self.physics_finish();
                Ok(json!({"edited": name, "warning": warn}))
            })(),
            "edit_joint" => (|| {
                let mi = self.sel_model()?;
                let d = &self.scene.models[mi].data;
                let i = match (a.get("index").and_then(|v| v.as_u64()), s("name")) {
                    (Some(i), _) if (i as usize) < d.joints.len() => i as usize,
                    (_, Some(nm)) => d.joints.iter().position(|j| j.name == nm).ok_or(format!("no joint named {nm}"))?,
                    _ => return Err("give index or name".into()),
                };
                let mut j = d.joints[i].clone();
                let deg = |v: glam::Vec3| v * (std::f32::consts::PI / 180.0);
                if let Some(v) = a.get("move_min").and_then(v3) { j.lin_min = v; }
                if let Some(v) = a.get("move_max").and_then(v3) { j.lin_max = v; }
                if let Some(v) = a.get("turn_min").and_then(v3) { j.ang_min = deg(v); }
                if let Some(v) = a.get("turn_max").and_then(v3) { j.ang_max = deg(v); }
                if let Some(v) = a.get("move_spring").and_then(v3) { j.spring_lin = v.max(glam::Vec3::ZERO); }
                if let Some(v) = a.get("turn_spring").and_then(v3) { j.spring_ang = v.max(glam::Vec3::ZERO); }
                let name = j.name.clone();
                self.phys_push_undo(&format!("joint {name} (AI)"));
                self.commit_part(mi, None, Some((i, j)));
                let warn = self.physics_finish();
                Ok(json!({"edited": name, "warning": warn}))
            })(),
            "revert_physics" => (|| {
                let mi = self.sel_model()?;
                self.phys_push_undo("revert physics (AI)");
                let m = &mut self.scene.models[mi];
                if b("all") {
                    m.phys_tweaks = Default::default();
                } else if let Some(nm) = s("body") {
                    let i = m.phys_base.0.iter().position(|r| r.name == nm).ok_or(format!("no rigid body named {nm}"))?;
                    m.phys_tweaks.bodies.remove(&i);
                } else if let Some(nm) = s("joint") {
                    let i = m.phys_base.1.iter().position(|j| j.name == nm).ok_or(format!("no joint named {nm}"))?;
                    m.phys_tweaks.joints.remove(&i);
                } else {
                    return Err("give body, joint or all=true".into());
                }
                m.rebuild_physics();
                if let Some(k) = &m.file_key {
                    let _ = crate::phystune::save(k, &m.phys_tweaks);
                }
                self.scene.force_reset_physics();
                Ok(json!("reverted"))
            })(),
            "physics_undo" => {
                self.phys_undo();
                Ok(json!(self.status.clone()))
            }
            "physics_redo" => {
                self.phys_redo();
                Ok(json!(self.status.clone()))
            }
            "reset_physics" => {
                self.scene.force_reset_physics();
                Ok(json!("physics reset to the animation pose"))
            }
            "get_render_settings" => {
                let r = &self.render_settings;
                let mut tmp = r.clone();
                tmp.anim_fps = self.scene.fps;
                Ok(json!({
                    "width": r.width, "height": r.height, "video_fps": r.fps, "animation_fps": self.scene.fps,
                    "start": r.start, "end": r.end, "seconds": tmp.seconds(), "video_frames": tmp.video_frames(),
                    "format": format!("{:?}", r.format).to_lowercase(), "quality": r.quality, "include_audio": r.include_audio,
                    "msaa": r.msaa, "open_when_done": r.open_when_done, "path": r.path.display().to_string(),
                    "ffmpeg": crate::export::ffmpeg_path().map(|p| p.display().to_string()),
                    "rendering": self.job.as_ref().map(|j| json!({"progress": j.progress(), "frame": j.next, "frames": j.total()})),
                    "last_result": self.last_render.as_ref().map(|r| match r {
                        Ok(m) => json!({"ok": true, "message": m}),
                        Err(e) => json!({"ok": false, "message": e}),
                    }),
                }))
            }
            "set_render_settings" => (|| {
                let r = &mut self.render_settings;
                if let Some(v) = n("width") { r.width = (v as u32).clamp(64, 7680); }
                if let Some(v) = n("height") { r.height = (v as u32).clamp(64, 7680); }
                if let Some(v) = n("fps") { r.fps = v.clamp(1.0, 240.0); }
                if let Some(v) = n("start") { r.start = v.max(0.0) as u32; }
                if let Some(v) = n("end") { r.end = (v.max(0.0) as u32).max(r.start); }
                if let Some(v) = n("quality") { r.quality = (v as u32).clamp(12, 32); }
                if let Some(v) = ob("include_audio") { r.include_audio = v; }
                if let Some(v) = ob("msaa") { r.msaa = v; }
                if let Some(v) = ob("open_when_done") { r.open_when_done = v; }
                if let Some(p) = s("path") {
                    let pb = std::path::PathBuf::from(p);
                    if !pb.is_absolute() {
                        return Err(format!("path must be absolute (got “{}”)", pb.display()));
                    }
                    r.path = pb;
                }
                if let Some(f) = s("format") {
                    use crate::export::OutFormat;
                    r.format = match f.as_str() {
                        "mp4" => OutFormat::Mp4,
                        "webm" => OutFormat::WebM,
                        "mov" => OutFormat::Mov,
                        "png" => OutFormat::PngSequence,
                        _ => return Err("format must be mp4, webm, mov or png".into()),
                    };
                    r.path = if r.format.ext().is_empty() { r.path.with_extension("") } else { r.path.with_extension(r.format.ext()) };
                }
                Ok(json!(format!("{}×{} · {} fps · frames {}–{} · {:?} → {}", r.width, r.height, r.fps, r.start, r.end, r.format, r.path.display())))
            })(),
            "cancel_render" => {
                let msg = self.job.as_mut().map(|j| j.cancel());
                self.job = None;
                match msg {
                    Some(m) => {
                        self.last_render = Some(Err(m.clone()));
                        Ok(json!(m))
                    }
                    None => Ok(json!("no render running")),
                }
            }
            _ => return None,
        })
    }
}
