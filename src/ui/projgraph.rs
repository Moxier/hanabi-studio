//! Project graph: the whole project as nodes — animation, scene objects, materials, lights,
//! stage effects and the render chain (scene → screen effects → colour grade → output).
//! The graph is built from the project data every frame, so it always matches the panels;
//! node positions are remembered per item.

use crate::app::App;
use crate::effects::{ModelLook, NodeKind, Socket};
use crate::material::{LightKind, SceneLight};
use crate::ui::home::ConfirmAction;
use crate::ui::theme;
use egui::{pos2, vec2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke};
use std::collections::{HashMap, HashSet};

const W: f32 = 184.0;
const HEAD: f32 = 24.0;
const ROW: f32 = 19.0;

#[derive(Default)]
pub struct ProjGraphUi {
    pub pos: HashMap<String, [f32; 2]>,
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub selected: Option<String>,
    pub link_from: Option<String>,
    pub dragging: Option<String>,
    pub add_menu_at: Option<Pos2>,
    pub expanded: HashSet<u64>,
    pub fit_pending: bool,
    pub fit_size: egui::Vec2,
    pub touched: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PgKind {
    Motion(u64),
    CameraMotion,
    Song,
    Model(u64),
    Accessory(u64),
    Material(u64, usize),
    Camera,
    Sun,
    Light(usize),
    Fx(u32),
    SceneRender,
    Effect(u32),
    Grade,
    Output,
}

#[derive(Clone)]
pub struct PgNode {
    pub key: String,
    pub kind: PgKind,
    pub title: String,
    pub sub: String,
    pub inputs: Vec<&'static str>,
    pub output: bool,
    pub color: Color32,
    pub enabled: bool,
    /// Column used for the automatic layout.
    pub col: usize,
}

struct Link {
    from: String,
    to: String,
    slot: usize,
}

const C_ANIM: Color32 = Color32::from_rgb(200, 130, 60);
const C_OBJ: Color32 = Color32::from_rgb(70, 140, 210);
const C_MAT: Color32 = Color32::from_rgb(170, 90, 190);
const C_LIGHT: Color32 = Color32::from_rgb(215, 180, 60);
const C_STAGE: Color32 = Color32::from_rgb(60, 160, 140);
const C_RENDER: Color32 = Color32::from_rgb(200, 80, 120);
const C_FX: Color32 = Color32::from_rgb(110, 100, 200);

impl App {
    fn build_project_graph(&self) -> (Vec<PgNode>, Vec<Link>) {
        let mut n = Vec::new();
        let mut l = Vec::new();
        let node = |key: String, kind: PgKind, title: String, sub: String, inputs: Vec<&'static str>, output: bool, color: Color32, col: usize| PgNode {
            key,
            kind,
            title,
            sub,
            inputs,
            output,
            color,
            enabled: true,
            col,
        };
        // Animation
        for m in &self.scene.models {
            if m.is_accessory() {
                continue;
            }
            let keys = m.motion.key_count();
            let len = m.motion.max_frame();
            n.push(node(format!("motion:{}", m.id), PgKind::Motion(m.id), format!("Motion · {}", m.name()), if keys > 0 { format!("{keys} keys · {len} frames") } else { "no keys yet".into() }, vec![], true, C_ANIM, 0));
            l.push(Link { from: format!("motion:{}", m.id), to: format!("model:{}", m.id), slot: 0 });
        }
        let ck = self.scene.camera_motion.camera.len() + self.scene.camera_motion.light.len();
        n.push(node("cammotion".into(), PgKind::CameraMotion, "Camera & light motion".into(), if ck > 0 { format!("{ck} keys") } else { "no keys yet".into() }, vec![], true, C_ANIM, 0));
        l.push(Link { from: "cammotion".into(), to: "camera".into(), slot: 0 });
        l.push(Link { from: "cammotion".into(), to: "sun".into(), slot: 0 });
        if let Some(a) = &self.scene.audio {
            n.push(node("song".into(), PgKind::Song, "Song".into(), format!("{} · {:.1} s", a.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), a.info.duration), vec![], true, C_ANIM, 0));
            l.push(Link { from: "song".into(), to: "output".into(), slot: 1 });
        }
        // Scene objects
        for m in &self.scene.models {
            let key = format!("model:{}", m.id);
            let mut nd = if m.is_accessory() {
                let att = m.attach.and_then(|(pid, b)| self.scene.models.iter().find(|x| x.id == pid).map(|p| format!("on {} · {}", p.name(), p.data.bones.get(b).map(|x| x.name.as_str()).unwrap_or("?"))));
                node(key.clone(), PgKind::Accessory(m.id), m.name().to_string(), att.unwrap_or_else(|| "on the ground".into()), vec!["Attach to"], true, C_OBJ, 1)
            } else {
                let look = self.graph.model_looks.get(m.name()).map(|x| x.label()).unwrap_or("scene look");
                node(key.clone(), PgKind::Model(m.id), m.name().to_string(), format!("{} parts · {look}", m.data.materials.len()), vec!["Motion"], true, C_OBJ, 1)
            };
            nd.enabled = m.visible;
            n.push(nd);
            l.push(Link { from: key.clone(), to: "render".into(), slot: 0 });
            if let Some((pid, _)) = m.attach {
                l.push(Link { from: key.clone(), to: format!("model:{pid}"), slot: 1 });
            }
            if self.pg.expanded.contains(&m.id) {
                for (k, mat) in m.data.materials.iter().enumerate() {
                    let kind = match self.graph.mat_fx.get(&mat.name) {
                        Some(f) => {
                            let s = if f.special != crate::material::Special::None { f.special.label() } else { f.base.label() };
                            format!("{s}{}", if f.graph.is_some() { " · nodes" } else { "" })
                        }
                        None => "automatic".into(),
                    };
                    let mk = format!("mat:{}:{k}", m.id);
                    n.push(node(mk.clone(), PgKind::Material(m.id, k), mat.name.clone(), kind, vec!["Model"], false, C_MAT, 2));
                    l.push(Link { from: key.clone(), to: mk, slot: 0 });
                }
            }
        }
        // Model nodes accept an accessory on input 1 ("Props").
        for nd in n.iter_mut() {
            if matches!(nd.kind, PgKind::Model(_)) {
                nd.inputs.push("Props");
            }
        }
        n.push(node("camera".into(), PgKind::Camera, "Camera".into(), format!("fov {:.0}° · distance {:.0}", self.scene.camera.fov, self.scene.camera.distance.abs()), vec!["Motion"], true, C_OBJ, 1));
        n.push(node("sun".into(), PgKind::Sun, "Sun (main light)".into(), "casts the shadows".into(), vec!["Motion"], true, C_LIGHT, 1));
        l.push(Link { from: "camera".into(), to: "render".into(), slot: 0 });
        l.push(Link { from: "sun".into(), to: "render".into(), slot: 0 });
        for (i, li) in self.graph.lights.iter().enumerate() {
            let mut nd = node(format!("light:{i}"), PgKind::Light(i), li.name.clone(), format!("{} · {:.1}", li.kind.label(), li.intensity), vec![], true, C_LIGHT, 1);
            nd.enabled = li.enabled;
            n.push(nd);
            l.push(Link { from: format!("light:{i}"), to: "render".into(), slot: 0 });
        }
        // Looks and stage effects plugged into the scene
        let scene_id = self.graph.nodes.iter().find(|x| x.kind == NodeKind::SceneRender).map(|x| x.id);
        for fx in &self.graph.nodes {
            if fx.kind.info().output != Some(Socket::Fx) {
                continue;
            }
            let info = fx.kind.info();
            let linked = scene_id.map(|s| self.graph.links.iter().any(|lk| lk.from == fx.id && lk.to == s)).unwrap_or(false);
            let color = if matches!(fx.kind, NodeKind::ShadingStyle | NodeKind::RayLighting | NodeKind::Sky) { C_LIGHT } else { C_STAGE };
            let mut nd = node(format!("fx:{}", fx.id), PgKind::Fx(fx.id), info.title.to_string(), info.category.to_string(), vec![], true, color, 1);
            nd.enabled = fx.enabled;
            n.push(nd);
            if linked {
                l.push(Link { from: format!("fx:{}", fx.id), to: "render".into(), slot: 1 });
            }
        }
        // Render chain
        n.push(node("render".into(), PgKind::SceneRender, "Scene Render".into(), "everything on stage".into(), vec!["Objects & lights", "Looks & stage"], true, C_RENDER, 3));
        for fx in &self.graph.nodes {
            if fx.kind.info().output != Some(Socket::Image) || fx.kind == NodeKind::SceneRender {
                continue;
            }
            let info = fx.kind.info();
            let mut nd = node(format!("eff:{}", fx.id), PgKind::Effect(fx.id), info.title.to_string(), info.category.to_string(), info.inputs.iter().map(|(n, _)| *n).collect(), true, C_FX, 4);
            nd.enabled = fx.enabled;
            n.push(nd);
        }
        let key_of = |id: u32| -> String {
            match self.graph.node(id).map(|x| x.kind) {
                Some(NodeKind::SceneRender) => "render".into(),
                Some(NodeKind::Output) => "grade".into(),
                _ => format!("eff:{id}"),
            }
        };
        for lk in &self.graph.links {
            let (Some(a), Some(b)) = (self.graph.node(lk.from), self.graph.node(lk.to)) else { continue };
            if a.kind.info().output == Some(Socket::Image) {
                l.push(Link { from: key_of(a.id), to: key_of(b.id), slot: lk.slot });
            }
        }
        let gl = if self.grade.enabled { format!("look: {}", crate::grade::LOOKS[self.grade.look.min(crate::grade::LOOKS.len() - 1)]) } else { "off".into() };
        n.push(node("grade".into(), PgKind::Grade, "Colour Grade".into(), gl, vec!["Image"], true, C_RENDER, 5));
        let rs = &self.render_settings;
        n.push(node("output".into(), PgKind::Output, "Output".into(), format!("{}×{} · {} fps", rs.width, rs.height, rs.fps), vec!["Image", "Song"], false, C_RENDER, 6));
        l.push(Link { from: "grade".into(), to: "output".into(), slot: 0 });
        (n, l)
    }

    /// Positions for nodes that don't have one yet: columns left to right, stacked downwards.
    fn layout_missing(&mut self, nodes: &[PgNode]) {
        let mut col_y = [0.0f32; 8];
        // Keep stacks below nodes already placed in the same column.
        for nd in nodes {
            if let Some(p) = self.pg.pos.get(&nd.key) {
                col_y[nd.col] = col_y[nd.col].max(p[1] + 70.0);
            }
        }
        let chain = self.graph.chain_ids();
        for nd in nodes {
            if self.pg.pos.contains_key(&nd.key) {
                continue;
            }
            let x = match nd.col {
                0 => 0.0,
                1 => 240.0,
                2 => 480.0,
                3 => 720.0,
                _ => 0.0,
            };
            let pos = match &nd.kind {
                // the render chain wraps into rows of four so the graph stays compact
                PgKind::Effect(id) => {
                    let i = chain.iter().position(|c| c == id).unwrap_or(chain.len() + 1);
                    [960.0 + (i % 4) as f32 * 210.0, 40.0 + (i / 4) as f32 * 110.0]
                }
                PgKind::Grade => {
                    let i = chain.len();
                    [960.0 + (i % 4) as f32 * 210.0, 40.0 + (i / 4) as f32 * 110.0]
                }
                PgKind::Output => {
                    let i = chain.len() + 1;
                    [960.0 + (i % 4) as f32 * 210.0, 40.0 + (i / 4) as f32 * 110.0]
                }
                PgKind::SceneRender => [720.0, 40.0],
                PgKind::Material(mid, k) => {
                    let base = self.pg.pos.get(&format!("model:{mid}")).copied().unwrap_or([240.0, 0.0]);
                    [480.0, base[1] + *k as f32 * 34.0]
                }
                _ => {
                    let y = col_y[nd.col];
                    col_y[nd.col] += HEAD + nd.inputs.len().max(1) as f32 * ROW + 26.0;
                    [x, y]
                }
            };
            self.pg.pos.insert(nd.key.clone(), pos);
        }
    }

    pub fn graph_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.pg.zoom == 0.0 {
            self.pg.zoom = 0.8;
            self.pg.pan = vec2(30.0, 50.0);
            self.pg.fit_pending = true;
            // show the parts of small models straight away
            for m in &self.scene.models {
                if !m.is_accessory() && m.data.materials.len() <= 8 {
                    self.pg.expanded.insert(m.id);
                }
            }
        }
        egui::Panel::right("pg_inspector").resizable(true).default_size(340.0).size_range(280.0..=520.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.pg_inspector(ui));
        });
        let target = (ui.available_height() * 0.6).clamp(260.0, 900.0);
        egui::Panel::bottom("pg_canvas").resizable(true).default_size(target).size_range(200.0..=1200.0).frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            self.pg_canvas(ui);
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let rect = ui.max_rect();
            self.viewport(ui, frame, rect, false);
        });
    }

    fn pg_canvas(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(ui.available_size());
        let rect = ui.max_rect().intersect(ui.clip_rect());
        let (nodes, links) = self.build_project_graph();
        self.layout_missing(&nodes);
        let size_of = |nd: &PgNode| -> egui::Vec2 {
            if matches!(nd.kind, PgKind::Material(..)) {
                vec2(W, 30.0)
            } else {
                vec2(W, HEAD + nd.inputs.len().max(1) as f32 * ROW + 20.0)
            }
        };
        let size_changed = (rect.size() - self.pg.fit_size).length() > 4.0;
        if (self.pg.fit_pending || (size_changed && !self.pg.touched)) && rect.width() > 200.0 && rect.height() > 120.0 {
            self.pg.fit_pending = false;
            self.pg.fit_size = rect.size();
            let (mut mn, mut mx) = (vec2(f32::MAX, f32::MAX), vec2(f32::MIN, f32::MIN));
            for nd in &nodes {
                if let Some(p) = self.pg.pos.get(&nd.key) {
                    let s = size_of(nd);
                    mn = mn.min(vec2(p[0], p[1]));
                    mx = mx.max(vec2(p[0], p[1]) + s);
                }
            }
            if mn.x < f32::MAX {
                let size = (mx - mn).max(vec2(1.0, 1.0));
                let avail = rect.size() - vec2(40.0, 70.0);
                let z = (avail.x / size.x).min(avail.y / size.y).clamp(0.35, 1.0);
                self.pg.zoom = z;
                self.pg.pan = rect.size() * 0.5 + vec2(0.0, 16.0) - (mn + mx) * 0.5 * z;
            }
        }
        let resp = ui.interact(rect, ui.scope_id().with("pgcanvas"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(19, 20, 25));
        let z = self.pg.zoom.clamp(0.25, 2.0);
        let origin = rect.min.to_vec2() + self.pg.pan;
        let gs = 24.0 * z;
        let mut x = rect.left() + (origin.x - rect.left()).rem_euclid(gs);
        while x < rect.right() {
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(25, 27, 33)));
            x += gs;
        }
        let to_screen = |p: [f32; 2]| pos2(origin.x + p[0] * z, origin.y + p[1] * z);
        // column titles
        for (cx, t) in [(0.0, "ANIMATION"), (240.0, "SCENE"), (480.0, "MATERIALS"), (720.0, "RENDER")] {
            painter.text(to_screen([cx, -26.0]), egui::Align2::LEFT_BOTTOM, t, egui::FontId::proportional(11.0 * z.max(0.7)), theme::TEXT_DIM);
        }
        struct G {
            key: String,
            rect: Rect,
            ins: Vec<Pos2>,
            out: Option<Pos2>,
        }
        let geo: Vec<G> = nodes
            .iter()
            .map(|nd| {
                let p = self.pg.pos.get(&nd.key).copied().unwrap_or([0.0, 0.0]);
                let r = Rect::from_min_size(to_screen(p), size_of(nd) * z);
                let ins = (0..nd.inputs.len()).map(|i| pos2(r.left(), r.top() + (HEAD + ROW * (i as f32 + 0.5)) * z)).collect();
                let out = nd.output.then(|| pos2(r.right(), r.top() + (if matches!(nd.kind, PgKind::Material(..)) { 15.0 } else { HEAD + ROW * 0.5 }) * z));
                G { key: nd.key.clone(), rect: r, ins, out }
            })
            .collect();
        let find = |k: &str| geo.iter().find(|g| g.key == k);
        let wire = |a: Pos2, b: Pos2, col: Color32, w: f32| {
            let d = ((b.x - a.x).abs() * 0.5).max(40.0);
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke([a, a + vec2(d, 0.0), b - vec2(d, 0.0), b], false, Color32::TRANSPARENT, Stroke::new(w, col)));
        };
        for lk in &links {
            let (Some(a), Some(b)) = (find(&lk.from), find(&lk.to)) else { continue };
            let pa = a.out.unwrap_or(pos2(a.rect.right(), a.rect.center().y));
            let pb = b.ins.get(lk.slot).copied().unwrap_or(pos2(b.rect.left(), b.rect.center().y));
            let col = nodes.iter().find(|n| n.key == lk.from).map(|n| n.color).unwrap_or(theme::TEXT_DIM);
            wire(pa, pb, col.linear_multiply(0.6), 1.8 * z.max(0.7));
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        if let (Some(from), Some(mp)) = (&self.pg.link_from, pointer) {
            if let Some(pa) = find(from).and_then(|g| g.out) {
                wire(pa, mp, theme::KEY, 2.0);
            }
        }
        let mut hov_node: Option<String> = None;
        let mut hov_out: Option<String> = None;
        let mut hov_in: Option<(String, usize)> = None;
        for (nd, g) in nodes.iter().zip(geo.iter()) {
            let sel = self.pg.selected.as_deref() == Some(&nd.key);
            painter.rect_filled(g.rect.translate(vec2(2.0, 3.0)), 7.0 * z, Color32::from_black_alpha(80));
            painter.rect_filled(g.rect, 7.0 * z, theme::BG2);
            let compact = matches!(nd.kind, PgKind::Material(..));
            let head = if compact { g.rect } else { Rect::from_min_size(g.rect.min, vec2(g.rect.width(), HEAD * z)) };
            let hc = if nd.enabled { nd.color } else { nd.color.linear_multiply(0.3) };
            if compact {
                painter.rect_filled(Rect::from_min_size(g.rect.min, vec2(5.0 * z, g.rect.height())), CornerRadius { nw: (7.0 * z) as u8, sw: (7.0 * z) as u8, ne: 0, se: 0 }, hc);
            } else {
                painter.rect_filled(head, CornerRadius { nw: (7.0 * z) as u8, ne: (7.0 * z) as u8, sw: 0, se: 0 }, hc);
            }
            painter.rect_stroke(g.rect, 7.0 * z, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { Color32::WHITE } else { theme::BG3 }), egui::StrokeKind::Outside);
            let title_pos = if compact { g.rect.left_center() + vec2(12.0 * z, -5.0 * z) } else { head.left_center() + vec2(8.0 * z, 0.0) };
            painter.text(title_pos, egui::Align2::LEFT_CENTER, &nd.title, egui::FontId::proportional(12.0 * z), Color32::WHITE);
            let sub_pos = if compact { g.rect.left_center() + vec2(12.0 * z, 7.0 * z) } else { pos2(g.rect.left() + 8.0 * z, g.rect.bottom() - 10.0 * z) };
            painter.text(sub_pos, egui::Align2::LEFT_CENTER, &nd.sub, egui::FontId::proportional(10.0 * z), Color32::from_gray(165));
            if !nd.enabled {
                painter.text(head.right_center() - vec2(8.0 * z, 0.0), egui::Align2::RIGHT_CENTER, "off", egui::FontId::proportional(10.0 * z), Color32::from_white_alpha(150));
            }
            if let PgKind::Model(id) = nd.kind {
                let open = self.pg.expanded.contains(&id);
                painter.text(head.right_center() - vec2(8.0 * z, 0.0), egui::Align2::RIGHT_CENTER, if open { "parts −" } else { "parts +" }, egui::FontId::proportional(10.0 * z), Color32::from_white_alpha(200));
            }
            for (i, p) in g.ins.iter().enumerate() {
                painter.circle_filled(*p, 5.0 * z, theme::ACCENT2);
                if !compact {
                    painter.text(*p + vec2(9.0 * z, 0.0), egui::Align2::LEFT_CENTER, nd.inputs[i], egui::FontId::proportional(10.0 * z), theme::TEXT_DIM);
                }
                if pointer.map(|mp| (mp - *p).length() < 9.0).unwrap_or(false) {
                    hov_in = Some((nd.key.clone(), i));
                }
            }
            if let Some(p) = g.out {
                painter.circle_filled(p, 5.0 * z, theme::KEY);
                if pointer.map(|mp| (mp - p).length() < 9.0).unwrap_or(false) {
                    hov_out = Some(nd.key.clone());
                }
            }
            if pointer.map(|mp| g.rect.contains(mp)).unwrap_or(false) {
                hov_node = Some(nd.key.clone());
            }
        }
        // toolbar
        let bar_y = rect.top() + 6.0;
        let mut bx = rect.right() - 8.0;
        let mut tool = |ui: &mut egui::Ui, label: &str| -> bool {
            let w = (20.0 + label.chars().count() as f32 * 7.0).max(56.0);
            let r = Rect::from_min_size(pos2(bx - w, bar_y), vec2(w, 22.0));
            bx -= w + 4.0;
            ui.put(r, egui::Button::new(RichText::new(label).size(11.5))).clicked()
        };
        if tool(ui, "Fit view") {
            self.pg.fit_pending = true;
        }
        if tool(ui, "Tidy layout") {
            self.pg.pos.clear();
            self.pg.fit_pending = true;
            self.pg.touched = false;
        }
        let over_bar = pointer.map(|p| p.y < bar_y + 24.0 && p.x > bx).unwrap_or(false);
        painter.text(rect.left_top() + vec2(10.0, 10.0), egui::Align2::LEFT_TOP, "Project graph — everything in this project. Click a node to edit it.", egui::FontId::proportional(12.0), theme::TEXT_DIM);
        painter.text(
            rect.left_bottom() + vec2(10.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "drag: move · drag a prop's output onto a model's Props to attach it · drag effect wires to reorder · right-click: add · middle-drag: pan · wheel: zoom",
            egui::FontId::proportional(11.0),
            theme::TEXT_DIM,
        );

        // interactions
        if resp.drag_started() || (resp.hovered() && ui.input(|i| i.smooth_scroll_delta.y != 0.0)) {
            self.pg.touched = true;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) && !over_bar {
            if let Some(o) = hov_out.clone() {
                self.pg.link_from = Some(o);
            } else if let Some((k, slot)) = hov_in.clone() {
                // pick up an existing editable wire
                if k.starts_with("eff:") || k == "grade" {
                    if let Some(lk) = links.iter().find(|lk| lk.to == k && lk.slot == slot) {
                        self.pg.link_from = Some(lk.from.clone());
                        self.pg_unlink(&lk.from, &k, slot);
                    }
                } else if k == "render" && slot == 1 {
                    // unplug nothing on drag start; plugging happens on drop
                }
            } else if let Some(nk) = hov_node.clone() {
                self.pg.dragging = Some(nk.clone());
                self.pg.selected = Some(nk);
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(k) = self.pg.dragging.clone() {
                let d = resp.drag_delta() / z;
                if let Some(p) = self.pg.pos.get_mut(&k) {
                    p[0] += d.x;
                    p[1] += d.y;
                }
                // a model drags its parts along
                if let Some(id) = k.strip_prefix("model:").and_then(|s| s.parse::<u64>().ok()) {
                    let prefix = format!("mat:{id}:");
                    for (pk, pv) in self.pg.pos.iter_mut() {
                        if pk.starts_with(&prefix) {
                            pv[0] += d.x;
                            pv[1] += d.y;
                        }
                    }
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && self.pg.dragging.is_none() && self.pg.link_from.is_none()) {
            self.pg.pan += resp.drag_delta();
        }
        if resp.drag_stopped() {
            if let Some(from) = self.pg.link_from.take() {
                let target = hov_in.clone().or_else(|| hov_node.clone().map(|k| (k, usize::MAX)));
                if let Some((to, slot)) = target {
                    self.pg_link(&from, &to, slot);
                }
            }
            self.pg.dragging = None;
        }
        if resp.clicked() && !over_bar {
            if let Some(k) = hov_node.clone() {
                // clicking a model's header toggles its parts
                if let (Some(id), Some(mp), Some(g)) = (k.strip_prefix("model:").and_then(|s| s.parse::<u64>().ok()), pointer, find(&k)) {
                    if mp.y < g.rect.top() + HEAD * z && mp.x > g.rect.right() - 60.0 * z && !self.scene.model_by_id(id).map(|i| self.scene.models[i].is_accessory()).unwrap_or(true) {
                        if !self.pg.expanded.remove(&id) {
                            self.pg.expanded.insert(id);
                        }
                    }
                }
            }
            self.pg.selected = hov_node;
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                let old = self.pg.zoom;
                let new = (old * (s * 0.0015).exp()).clamp(0.25, 2.0);
                if let Some(mp) = pointer {
                    let world = (mp.to_vec2() - rect.min.to_vec2() - self.pg.pan) / old;
                    self.pg.pan = mp.to_vec2() - rect.min.to_vec2() - world * new;
                }
                self.pg.zoom = new;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                if let Some(k) = self.pg.selected.clone() {
                    self.pg_delete(&k);
                }
            }
        }
        if resp.secondary_clicked() {
            self.pg.add_menu_at = pointer;
        }
        if let Some(at) = self.pg.add_menu_at {
            let mut close = false;
            let area = egui::Area::new(ui.scope_id().with("pg_add")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new("Add to the project").strong());
                    for (k, l) in [("model", "Model (.pmx / .pmd)…"), ("accessory", "Prop (.x)…"), ("motion", "Motion (.vmd)…"), ("audio", "Song (.wav)…")] {
                        if ui.button(l).clicked() {
                            self.load_dialog(k);
                            close = true;
                        }
                    }
                    ui.separator();
                    for kind in LightKind::ALL {
                        if ui.button(format!("{}", kind.label())).clicked() {
                            let mut li = SceneLight::new(kind);
                            li.name = format!("{} {}", kind.label(), self.graph.lights.len() + 1);
                            self.graph.lights.push(li);
                            self.pg.selected = Some(format!("light:{}", self.graph.lights.len() - 1));
                            close = true;
                        }
                    }
                    ui.separator();
                    ui.menu_button("Stage & looks", |ui| {
                        for k in [NodeKind::Particles, NodeKind::LightBeams, NodeKind::Water, NodeKind::MirrorFloor, NodeKind::Sky, NodeKind::ShadingStyle, NodeKind::RayLighting] {
                            if ui.button(k.info().title).on_hover_text(k.info().about).clicked() {
                                let id = self.graph.append_before_output(k);
                                self.pg.selected = Some(format!("fx:{id}"));
                                close = true;
                            }
                        }
                    });
                    ui.menu_button("Screen effect", |ui| {
                        for k in NodeKind::ALL.iter().filter(|k| k.info().output == Some(Socket::Image) && **k != NodeKind::SceneRender) {
                            if ui.button(k.info().title).on_hover_text(k.info().about).clicked() {
                                let id = self.graph.append_before_output(*k);
                                self.pg.selected = Some(format!("eff:{id}"));
                                close = true;
                            }
                        }
                    });
                });
            });
            if close || (ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pointer.unwrap_or(at)) && !resp.secondary_clicked()) {
                self.pg.add_menu_at = None;
            }
        }
    }

    fn effect_id_of(key: &str) -> Option<u32> {
        key.strip_prefix("eff:").or_else(|| key.strip_prefix("fx:")).and_then(|s| s.parse().ok())
    }

    fn special_effect_id(&self, key: &str) -> Option<u32> {
        match key {
            "render" => self.graph.nodes.iter().find(|n| n.kind == NodeKind::SceneRender).map(|n| n.id),
            "grade" => self.graph.nodes.iter().find(|n| n.kind == NodeKind::Output).map(|n| n.id),
            _ => Self::effect_id_of(key),
        }
    }

    /// Make a connection the person drew. `slot == usize::MAX` means "dropped on the node body".
    fn pg_link(&mut self, from: &str, to: &str, slot: usize) {
        // a prop onto a model: attach it to that model's centre-most bone
        if let (Some(pid), Some(tid)) = (from.strip_prefix("model:").and_then(|s| s.parse::<u64>().ok()), to.strip_prefix("model:").and_then(|s| s.parse::<u64>().ok())) {
            if let (Some(pi), Some(ti)) = (self.scene.model_by_id(pid), self.scene.model_by_id(tid)) {
                if self.scene.models[pi].is_accessory() && !self.scene.models[ti].is_accessory() {
                    let bone = crate::tools::find_bone(&self.scene.models[ti].data, crate::tools::CENTER).and_then(|n| self.scene.models[ti].data.bone_index(&n)).unwrap_or(0);
                    self.scene.models[pi].attach = Some((tid, bone));
                    self.set_status("Attached the prop (pick the exact bone in the panel on the right)");
                }
            }
            return;
        }
        // look / stage effect into the scene
        if from.starts_with("fx:") && to == "render" {
            if let (Some(f), Some(s)) = (Self::effect_id_of(from), self.special_effect_id("render")) {
                self.graph.connect(f, s, 0);
            }
            return;
        }
        // image chain: render / effects / grade(=Output)
        if (from.starts_with("eff:") || from == "render") && (to.starts_with("eff:") || to == "grade") {
            let (Some(f), Some(t)) = (self.special_effect_id(from), self.special_effect_id(to)) else { return };
            let slot = if slot == usize::MAX { 0 } else { slot };
            self.graph.connect(f, t, slot);
            self.graph.preset_name = "Custom".into();
        }
    }

    fn pg_unlink(&mut self, from: &str, to: &str, slot: usize) {
        let (Some(f), Some(t)) = (self.special_effect_id(from), self.special_effect_id(to)) else { return };
        self.graph.links.retain(|l| !(l.from == f && l.to == t && l.slot == slot));
    }

    fn pg_delete(&mut self, key: &str) {
        if let Some(id) = key.strip_prefix("model:").and_then(|s| s.parse::<u64>().ok()) {
            if let Some(i) = self.scene.model_by_id(id) {
                let name = self.scene.models[i].name().to_string();
                self.ask("Delete from the scene?", &format!("“{name}” and its keyframes will be removed."), "Delete", ConfirmAction::DeleteModel(id));
            }
        } else if let Some(i) = key.strip_prefix("light:").and_then(|s| s.parse::<usize>().ok()) {
            if i < self.graph.lights.len() {
                self.graph.lights.remove(i);
                self.pg.selected = None;
            }
        } else if let Some(id) = Self::effect_id_of(key) {
            self.graph.remove(id);
            self.pg.selected = None;
        } else if key == "song" {
            self.ask("Remove the song?", "The song will be taken out of the project (the file stays on disk).", "Remove", ConfirmAction::RemoveAudio);
        }
    }

    fn pg_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.pg.selected.clone() else {
            theme::section(ui, "Project graph");
            ui.label("Everything in your project is a node here:");
            ui.label("• Animation: each model's motion, the camera motion and the song.");
            ui.label("• Scene: models, props, camera, sun, lights, looks and stage effects.");
            ui.label("• Materials: each model's parts (click a model's “parts” to show them).");
            ui.label("• Render: Scene Render → screen effects → Colour Grade → Output.");
            ui.add_space(6.0);
            ui.label(RichText::new("Click a node to edit it here. The other workspaces edit the same things.").color(theme::TEXT_DIM));
            return;
        };
        let (nodes, _) = self.build_project_graph();
        let Some(nd) = nodes.into_iter().find(|n| n.key == key) else {
            self.pg.selected = None;
            return;
        };
        theme::section(ui, &nd.title);
        match nd.kind.clone() {
            PgKind::Motion(id) => {
                let Some(i) = self.scene.model_by_id(id) else { return };
                let m = &self.scene.models[i];
                ui.label(format!("{} keys on {} bones and {} expressions", m.motion.key_count(), m.motion.bones.len(), m.motion.morphs.len()));
                ui.label(format!("Last key at frame {}", m.motion.max_frame()));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Load motion…").clicked() {
                        self.scene.selected = Some(i);
                        self.load_dialog("motion");
                    }
                    if ui.button("Save as VMD / HMO…").clicked() {
                        self.scene.selected = Some(i);
                        self.save_motion_dialog(false);
                    }
                    if ui.button("Edit in Animation").clicked() {
                        self.scene.selected = Some(i);
                        self.ws = crate::app::Workspace::Animation;
                    }
                });
                if self.scene.models[i].motion.key_count() > 0 && ui.button(RichText::new("Clear all keys…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                    self.ask("Clear this motion?", "All keyframes of this model will be deleted (Undo can bring them back).", "Clear keys", ConfirmAction::ClearMotion(id));
                }
            }
            PgKind::CameraMotion => {
                ui.label(format!("{} camera keys · {} light keys", self.scene.camera_motion.camera.len(), self.scene.camera_motion.light.len()));
                ui.checkbox(&mut self.scene.camera_follows_motion, "Camera follows this motion");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Load camera motion…").clicked() {
                        self.load_dialog("motion");
                    }
                    if ui.button("Save as VMD…").clicked() {
                        self.save_motion_dialog(true);
                    }
                });
                if !self.scene.camera_motion.camera.is_empty() && ui.button(RichText::new("Clear camera keys…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                    self.ask("Clear the camera motion?", "All camera and light keys will be deleted (Undo can bring them back).", "Clear keys", ConfirmAction::ClearCameraMotion);
                }
            }
            PgKind::Song => {
                if let Some(a) = self.scene.audio.as_mut() {
                    ui.label(a.path.display().to_string());
                    ui.add(egui::DragValue::new(&mut a.offset_frames).prefix("Start offset: ").suffix(" frames"));
                }
                if ui.button("Remove the song…").clicked() {
                    self.pg_delete("song");
                }
            }
            PgKind::Model(id) | PgKind::Accessory(id) => {
                let Some(i) = self.scene.model_by_id(id) else { return };
                let is_acc = self.scene.models[i].is_accessory();
                theme::card(ui, |ui| {
                    let m = &mut self.scene.models[i];
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut m.visible, "Visible");
                        if !is_acc {
                            ui.checkbox(&mut m.physics_enabled, "Physics");
                        }
                    });
                    if !is_acc {
                        ui.add(egui::Slider::new(&mut m.edge_width, 0.0..=3.0).text("Outline width"));
                    }
                    ui.label(RichText::new(format!("{} vertices · {} bones · {} parts", m.data.vertices.len(), m.data.bones.len(), m.data.materials.len())).size(11.0).color(theme::TEXT_DIM));
                });
                if !is_acc {
                    let name = self.scene.models[i].name().to_string();
                    let cur = self.graph.model_looks.get(&name).copied();
                    egui::ComboBox::from_label("Look").selected_text(cur.map(|l| l.label()).unwrap_or("Same as scene")).show_ui(ui, |ui| {
                        if ui.selectable_label(cur.is_none(), "Same as scene").clicked() {
                            self.graph.model_looks.remove(&name);
                        }
                        for l in ModelLook::ALL {
                            if ui.selectable_label(cur == Some(l), l.label()).clicked() {
                                self.graph.model_looks.insert(name.clone(), l);
                            }
                        }
                    });
                    let mut open = self.pg.expanded.contains(&id);
                    if ui.checkbox(&mut open, "Show its parts (materials) in the graph").changed() {
                        if open {
                            self.pg.expanded.insert(id);
                        } else {
                            self.pg.expanded.remove(&id);
                        }
                    }
                } else {
                    self.accessory_inspector(ui, i);
                }
                ui.horizontal(|ui| {
                    if ui.button("Pose in Animation").clicked() {
                        self.scene.selected = Some(i);
                        self.sel_camera = false;
                        self.ws = crate::app::Workspace::Animation;
                    }
                    if ui.button(RichText::new("Delete…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                        self.pg_delete(&key);
                    }
                });
            }
            PgKind::Material(id, k) => {
                if let Some(i) = self.scene.model_by_id(id) {
                    self.mat_sel = Some((i, k));
                    self.material_editor(ui);
                    if self.graph.mat_fx.get(&self.scene.models[i].data.materials[k].name).and_then(|f| f.graph.as_ref()).is_some() && ui.button("Open its node editor (Effects → Materials)").clicked() {
                        self.ws = crate::app::Workspace::Effects;
                        self.fx_simple = true;
                        self.fx_tab = crate::app::FxTab::Materials;
                        self.mat_nodes_open = true;
                    }
                }
            }
            PgKind::Camera => {
                self.camera_inspector(ui);
            }
            PgKind::Sun => {
                let l = &mut self.scene.light;
                let mut col = l.color.to_array();
                ui.horizontal(|ui| {
                    if ui.color_edit_button_rgb(&mut col).changed() {
                        self.scene.light_edited = true;
                    }
                    ui.label("Colour and brightness");
                });
                l.color = glam::Vec3::from(col);
                ui.horizontal(|ui| {
                    ui.label("Direction");
                    ui.add(egui::DragValue::new(&mut l.direction.x).speed(0.01).range(-1.0..=1.0));
                    ui.add(egui::DragValue::new(&mut l.direction.y).speed(0.01).range(-1.0..=1.0));
                    ui.add(egui::DragValue::new(&mut l.direction.z).speed(0.01).range(-1.0..=1.0));
                });
                ui.checkbox(&mut self.scene.stage.self_shadow, "Self shadow");
                ui.checkbox(&mut self.scene.stage.ground_shadow, "Ground shadow");
            }
            PgKind::Light(i) => {
                let Some(l) = self.graph.lights.get_mut(i) else { return };
                ui.checkbox(&mut l.enabled, "On");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut l.name);
                });
                egui::ComboBox::from_label("Kind").selected_text(l.kind.label()).show_ui(ui, |ui| {
                    for k in LightKind::ALL {
                        ui.selectable_value(&mut l.kind, k, k.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.color_edit_button_rgb(&mut l.color);
                    ui.add(egui::Slider::new(&mut l.intensity, 0.0..=20.0).text("Intensity"));
                });
                if l.kind != LightKind::Directional {
                    ui.horizontal(|ui| {
                        ui.label("Position");
                        for v in l.position.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(0.1));
                        }
                    });
                    ui.add(egui::Slider::new(&mut l.range, 2.0..=300.0).text("Reach"));
                }
                if l.kind != LightKind::Point {
                    ui.add(egui::Slider::new(&mut l.pitch, -90.0..=90.0).text("Tilt (°)"));
                    ui.add(egui::Slider::new(&mut l.yaw, -180.0..=180.0).text("Turn (°)"));
                }
                if l.kind == LightKind::Spot {
                    ui.add(egui::Slider::new(&mut l.cone, 2.0..=80.0).text("Cone (°)"));
                    ui.add(egui::Slider::new(&mut l.softness, 0.0..=1.0).text("Edge softness"));
                }
                if ui.button("Remove light").clicked() {
                    self.pg_delete(&key);
                }
            }
            PgKind::Fx(id) | PgKind::Effect(id) => {
                if let Some(node) = self.graph.node_mut(id) {
                    let info = node.kind.info();
                    ui.label(RichText::new(info.about).size(11.0).color(theme::TEXT_DIM));
                    ui.checkbox(&mut node.enabled, "On");
                    if crate::ui::fxstack::node_params(ui, node) {
                        self.graph.preset_name = format!("{} (edited)", self.graph.preset_name.trim_end_matches(" (edited)"));
                    }
                }
                if ui.button("Remove").clicked() {
                    self.pg_delete(&key);
                }
            }
            PgKind::SceneRender => {
                let st = &mut self.scene.stage;
                ui.checkbox(&mut st.edges, "Outlines (edges)");
                ui.checkbox(&mut st.show_floor, "Floor");
                ui.checkbox(&mut st.show_grid, "Grid");
                ui.checkbox(&mut st.msaa, "Anti-aliasing");
                ui.horizontal(|ui| {
                    ui.label("Background");
                    ui.color_edit_button_rgb(&mut st.background);
                });
                ui.label(RichText::new("Plug looks and stage effects into “Looks & stage”; screen effects run from its output.").size(11.0).color(theme::TEXT_DIM));
            }
            PgKind::Grade => {
                let g = &mut self.grade;
                ui.checkbox(&mut g.enabled, "On");
                ui.add(egui::Slider::new(&mut g.exposure, -3.0..=3.0).text("Exposure"));
                ui.add(egui::Slider::new(&mut g.contrast, 0.5..=1.6).text("Contrast"));
                ui.add(egui::Slider::new(&mut g.saturation, 0.0..=2.0).text("Saturation"));
                ui.add(egui::Slider::new(&mut g.temperature, -1.0..=1.0).text("Temperature"));
                ui.add(egui::Slider::new(&mut g.tint, -1.0..=1.0).text("Tint"));
                egui::ComboBox::from_label("Look").selected_text(crate::grade::LOOKS[g.look.min(crate::grade::LOOKS.len() - 1)]).show_ui(ui, |ui| {
                    for (i, l) in crate::grade::LOOKS.iter().enumerate() {
                        ui.selectable_value(&mut g.look, i, *l);
                    }
                });
                if ui.button("Open the Color workspace (wheels, scopes)").clicked() {
                    self.ws = crate::app::Workspace::Color;
                }
            }
            PgKind::Output => {
                let s = &mut self.render_settings;
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.width).range(64..=7680).suffix(" px"));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut s.height).range(64..=7680).suffix(" px"));
                });
                ui.label(format!("{} fps · frames {}–{} · {}", s.fps, s.start, s.end, s.format.label()));
                if ui.button("Open the Render workspace").clicked() {
                    self.ws = crate::app::Workspace::Render;
                }
            }
        }
    }
}
