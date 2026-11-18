//! Model Context Protocol server so AI assistants can drive the studio.
//! Two transports share one tool set:
//!  * HTTP on 127.0.0.1 (POST JSON-RPC to /mcp), always on while the app runs (configurable port)
//!  * stdio, when the app is started with `--mcp-stdio` (the AI client launches the studio)
//! Tool calls are executed on the UI thread through a channel.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub struct ToolCall {
    pub name: String,
    pub args: Value,
    pub reply: Sender<Result<Value, String>>,
}

pub struct Mcp {
    pub rx: Receiver<ToolCall>,
    pub port: Option<u16>,
    pub calls: u64,
}

pub const PROTOCOL: &str = "2025-06-18";

pub fn tools() -> Value {
    let s = |d: &str| json!({"type": "string", "description": d});
    let n = |d: &str| json!({"type": "number", "description": d});
    let b = |d: &str| json!({"type": "boolean", "description": d});
    let tool = |name: &str, desc: &str, props: Value, req: &[&str]| {
        json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": req}})
    };
    let mut list = json!([
        tool("get_scene", "Summary of the stage: models, selected model, frame, playback range, camera, active effect preset, colour grade.", json!({}), &[]),
        tool("load_file", "Open a file from disk: model (.pmx/.pmd), accessory (.x), motion (.vmd/.hmo, applied to the selected model or to the camera), pose (.vpd), audio (.wav) or project (.hproj).", json!({"path": s("Absolute path")}), &["path"]),
        tool("select_model", "Select a model by index or name.", json!({"index": n("Model index"), "name": s("Model name")}), &[]),
        tool("list_bones", "List bones of the selected model with whether they rotate/move and whether they have keys.", json!({"filter": s("Optional substring")}), &[]),
        tool("list_morphs", "List morphs (facial expressions) of the selected model.", json!({"filter": s("Optional substring")}), &[]),
        tool("set_frame", "Move the playhead to a frame.", json!({"frame": n("Frame number (30 fps by default)")}), &["frame"]),
        tool("playback", "Play, pause or stop.", json!({"action": s("play | pause | stop")}), &["action"]),
        tool("pose_bone", "Pose a bone on the selected model. Rotation is Euler degrees (x pitch, y yaw, z roll). Set key=true to register a keyframe at the current frame.",
            json!({"bone": s("Bone name"), "rotation": json!({"type":"array","items":{"type":"number"},"description":"[x,y,z] degrees"}), "position": json!({"type":"array","items":{"type":"number"},"description":"[x,y,z] offset"}), "key": b("Register a keyframe"), "curve": s("linear | smooth | ease_in | ease_out | snappy | hold")}), &["bone"]),
        tool("set_morph", "Set a morph weight (0..1) on the selected model; key=true registers a keyframe.", json!({"morph": s("Morph name"), "weight": n("0..1"), "key": b("Register a keyframe")}), &["morph", "weight"]),
        tool("delete_keys", "Delete keys at a frame on a bone or morph (or camera with track='camera').", json!({"track": s("Bone/morph name, or 'camera' / 'light'"), "frame": n("Frame")}), &["track", "frame"]),
        tool("set_camera", "Set the camera. Optional fields keep their value. key=true registers a camera keyframe.",
            json!({"target": json!({"type":"array","items":{"type":"number"}}), "rotation": json!({"type":"array","items":{"type":"number"},"description":"degrees"}), "distance": n("Negative = in front"), "fov": n("degrees"), "key": b("Register keyframe")}), &[]),
        tool("set_light", "Set the main light colour [r,g,b] (0..1) and direction [x,y,z].", json!({"color": json!({"type":"array","items":{"type":"number"}}), "direction": json!({"type":"array","items":{"type":"number"}})}), &[]),
        tool("list_effect_presets", "List the effect presets.", json!({}), &[]),
        tool("apply_effect_preset", "Add a preset's nodes into the effect graph (grouped under the preset name). replace=true swaps the whole graph instead.", json!({"name": s("Preset name"), "replace": b("Replace the whole graph instead of adding")}), &["name"]),
        tool("add_effect_node", "Add an effect node (e.g. Bloom, Particles, Water Surface, Light Beams, Depth of Field) to the graph.", json!({"node": s("Node title")}), &["node"]),
        tool("set_effect_param", "Change a parameter of the first node with that title. Numbers, [r,g,b] colours, option index or booleans.", json!({"node": s("Node title"), "param": s("Parameter name"), "value": json!({"description": "number | [r,g,b] | boolean"})}), &["node", "param", "value"]),
        tool("get_effect_graph", "Describe the effect graph: nodes, parameters and links.", json!({}), &[]),
        tool("set_color_grade", "Adjust colour grading. Fields: exposure, contrast, saturation, vibrance, temperature, tint, shadows, highlights, lift/gamma/gain ([r,g,b] wheels, -0.5..0.5), look (name), look_strength, lut (path to a .cube file, '' removes it), lut_strength (0..1), split_compare (before/after), enabled, reset=true for neutral.", json!({"settings": json!({"type":"object"})}), &["settings"]),
        tool("render_image", "Render the current frame to a PNG file.", json!({"path": s("Absolute .png path"), "width": n("pixels"), "height": n("pixels")}), &["path"]),
        tool("render_video", "Start rendering a frame range (animation frames) to a video (.mp4/.webm/.mov via ffmpeg) or a PNG sequence (folder path). fps is the video frame rate; the animation keeps its own rate (a 30 fps dance rendered at 60 gets in-between frames, same speed). Other settings come from set_render_settings. Returns immediately; poll get_render_settings for progress.",
            json!({"path": s("Output .mp4/.webm/.mov file or folder"), "start": n("first frame"), "end": n("last frame"), "width": n("px"), "height": n("px"), "fps": n("video frames per second"), "quality": n("12..32, lower = better"), "include_audio": b("include the song")}), &["path"]),
        tool("save_motion", "Save the selected model's motion (or camera motion with camera=true) as .vmd (MMD-compatible) or .hmo (this app's format).", json!({"path": s("Absolute path ending .vmd or .hmo"), "camera": b("Save camera/light motion instead")}), &["path"]),
        tool("save_project", "Save the whole project (.hproj).", json!({"path": s("Absolute path")}), &["path"]),
        tool("generate_motion", "Create a procedural motion on the selected model: idle (breathing/sway), wave (hand wave), nod, or bounce (rhythmic bounce at a BPM).", json!({"kind": s("idle | wave | nod | bounce"), "start": n("first frame"), "length": n("frames"), "bpm": n("for bounce")}), &["kind"]),
        tool("set_material_type", "For Ray Lighting: say what a model material is made of (cloth, skin, hair, eyes, plastic, leather, metal, gold, glass, glowing, toon) or 'auto'. Lists materials when called without arguments.", json!({"material": s("Material name"), "type": s("Material type or auto")}), &[]),
        tool("add_light", "Place an extra light: kind point | spot | directional, colour [r,g,b], intensity, position [x,y,z], pitch/yaw degrees (spot/directional), cone degrees, range. Or rig='Studio three-point' | 'Concert spots' | 'Warm room' | 'Night city' to load a rig; clear=true removes all extra lights.",
            json!({"kind": s("point | spot | directional"), "color": json!({"type":"array","items":{"type":"number"}}), "intensity": n("brightness"), "position": json!({"type":"array","items":{"type":"number"}}), "pitch": n("degrees"), "yaw": n("degrees"), "cone": n("degrees"), "range": n("reach"), "rig": s("rig name"), "clear": b("remove all extra lights")}), &[]),
        tool("set_material", "Edit one material (by name): preset (e.g. 'Chrome', 'Wet in rain', 'Short fur', 'Grass', 'Neon tube', 'Clear glass', 'Water + ripples'), special (none|glass|water|wet|fur|grass|glow), roughness, metallic, glow, glow_color [r,g,b], emit_light, normal_map / height_map / metal_map / rough_map (absolute file paths, '' to clear), auto=true returns it to automatic.",
            json!({"material": s("Material name"), "preset": s("Material preset name"), "special": s("special kind"), "roughness": n("0..1"), "metallic": n("0..1"), "glow": n("0..10"), "glow_color": json!({"type":"array","items":{"type":"number"}}), "emit_light": b("lights the surroundings"), "normal_map": s("path"), "height_map": s("path"), "metal_map": s("path"), "rough_map": s("path"), "auto": b("back to automatic")}), &["material"]),
        tool("undo", "Undo the last animation edit.", json!({}), &[]),
    ]);
    if let Some(v) = list.as_array_mut() {
        v.extend(crate::commands2::tools_more());
    }
    list
}

fn http_reply(stream: &mut TcpStream, status: &str, body: &str, session: bool) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: http://localhost\r\nConnection: close\r\n",
        body.len()
    );
    if session {
        head.push_str("Mcp-Session-Id: hanabi-local\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

/// Handle one JSON-RPC message. Returns None for notifications.
pub fn handle_message(msg: &Value, tx: &Sender<ToolCall>) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = id?;
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "hanabi-studio", "title": "Hanabi Studio", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Controls the Hanabi Studio 3D dance animation app. Start with get_status and get_scene / list_objects. You can load models, stages, props, motions, poses, songs and projects (load_file); organise objects in groups; pose bones (select_bones, pose_bone, transform_bones, reset_bones, key_bones, retime, pose_tools); key the camera and light; set song volume/offset; tune physics globally (set_physics) or per part (edit_rigid_body, edit_joint; undo with physics_undo); apply effects, materials, lights and colour grading (with .cube LUTs); and render images or videos (set_render_settings, render_video). Destructive actions need confirm=true."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or("").to_string();
            let args = msg["params"].get("arguments").cloned().unwrap_or(json!({}));
            let (rtx, rrx) = channel();
            if tx.send(ToolCall { name, args, reply: rtx }).is_err() {
                Err((-32603, "studio is closing".into()))
            } else {
                match rrx.recv_timeout(Duration::from_secs(120)) {
                    Ok(Ok(v)) => {
                        let text = if v.is_string() { v.as_str().unwrap().to_string() } else { serde_json::to_string_pretty(&v).unwrap_or_default() };
                        Ok(json!({"content": [{"type": "text", "text": text}], "isError": false}))
                    }
                    Ok(Err(e)) => Ok(json!({"content": [{"type": "text", "text": e}], "isError": true})),
                    Err(_) => Err((-32603, "timed out waiting for the studio".into())),
                }
            }
        }
        "resources/list" => Ok(json!({"resources": []})),
        "prompts/list" => Ok(json!({"prompts": []})),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, m)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}}),
    })
}

/// Is this a local address ("localhost", "127.0.0.1" or "[::1]", with an optional port)?
fn is_local_host(h: &str) -> bool {
    let h = h.trim();
    let host = if let Some(rest) = h.strip_prefix('[') {
        // [::1]:7860
        match rest.split_once(']') {
            Some((inside, after)) if after.is_empty() || after.starts_with(':') => inside,
            _ => return false,
        }
    } else {
        h.split(':').next().unwrap_or("")
    };
    let port_ok = match h.rsplit_once(':') {
        Some((before, port)) if !h.starts_with('[') || before.ends_with(']') => port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty(),
        _ => true,
    };
    port_ok && matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Browser pages may only call the studio from a local page; anything else (other sites,
/// look-alike names such as localhost.example.com, sandboxed frames and local files that
/// send "null") is refused. Programs that send no Origin at all (AI clients) are allowed.
fn origin_allowed(origin: &str) -> bool {
    let o = origin.trim();
    let rest = match o.strip_prefix("http://").or_else(|| o.strip_prefix("https://")) {
        Some(r) => r,
        None => return false,
    };
    !rest.contains('/') && is_local_host(rest)
}

fn serve_http(stream: TcpStream, tx: Sender<ToolCall>) {
    let mut stream = stream;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Ok(clone) = stream.try_clone() else { return };
    // A request (headers + body) can't be bigger than 17 MB, so it can't eat memory.
    let mut reader = BufReader::new(clone.take(17 * 1024 * 1024));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut len = 0usize;
    let mut origin_ok = true;
    let mut host_ok = true;
    let mut headers = 0;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
            break;
        }
        headers += 1;
        if headers > 100 {
            return;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = lower.strip_prefix("origin:") {
            origin_ok = origin_allowed(v);
        }
        // DNS rebinding: a page on another name resolving to 127.0.0.1 still sends its own Host.
        if let Some(v) = lower.strip_prefix("host:") {
            host_ok = is_local_host(v);
        }
    }
    if !origin_ok || !host_ok {
        http_reply(&mut stream, "403 Forbidden", "{}", false);
        return;
    }
    if method == "GET" {
        let info = json!({"name": "Hanabi Studio MCP", "endpoint": "/mcp", "transport": "POST JSON-RPC 2.0"}).to_string();
        http_reply(&mut stream, if path.starts_with("/mcp") || path == "/" { "200 OK" } else { "404 Not Found" }, &info, false);
        return;
    }
    if method == "DELETE" {
        http_reply(&mut stream, "200 OK", "{}", false);
        return;
    }
    if method != "POST" || !path.starts_with("/mcp") {
        http_reply(&mut stream, "404 Not Found", "{}", false);
        return;
    }
    if len > 16 * 1024 * 1024 {
        http_reply(&mut stream, "413 Payload Too Large", "{}", false);
        return;
    }
    let mut body = vec![0u8; len];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        http_reply(&mut stream, "400 Bad Request", &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}).to_string(), false);
        return;
    };
    let is_init = msg.get("method").and_then(|m| m.as_str()) == Some("initialize");
    let resp = if let Some(arr) = msg.as_array() {
        let v: Vec<Value> = arr.iter().filter_map(|m| handle_message(m, &tx)).collect();
        if v.is_empty() { None } else { Some(Value::Array(v)) }
    } else {
        handle_message(&msg, &tx)
    };
    match resp {
        Some(r) => http_reply(&mut stream, "200 OK", &r.to_string(), is_init),
        None => {
            let _ = stream.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    }
}

impl Mcp {
    pub fn start(port: u16, stdio: bool, ctx: egui::Context) -> Self {
        let (tx, rx) = channel::<ToolCall>();
        // Wake the UI when a call arrives.
        let (wtx, wrx) = channel::<ToolCall>();
        let fwd_ctx = ctx.clone();
        let tx_ui = tx.clone();
        std::thread::spawn(move || {
            while let Ok(c) = wrx.recv() {
                if tx_ui.send(c).is_err() {
                    break;
                }
                fwd_ctx.request_repaint();
            }
        });
        let mut bound = None;
        if port > 0 {
            for p in port..port + 10 {
                if let Ok(listener) = TcpListener::bind(("127.0.0.1", p)) {
                    bound = Some(p);
                    let wtx = wtx.clone();
                    std::thread::spawn(move || {
                        for s in listener.incoming().flatten() {
                            let wtx = wtx.clone();
                            std::thread::spawn(move || serve_http(s, wtx));
                        }
                    });
                    break;
                }
            }
        }
        if stdio {
            let wtx = wtx.clone();
            std::thread::spawn(move || {
                let stdin = std::io::stdin();
                let mut out = std::io::stdout();
                for line in stdin.lock().lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let resp = match serde_json::from_str::<Value>(&line) {
                        Ok(msg) => handle_message(&msg, &wtx),
                        Err(_) => Some(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}})),
                    };
                    if let Some(r) = resp {
                        let _ = writeln!(out, "{}", r);
                        let _ = out.flush();
                    }
                }
            });
        }
        drop(tx);
        Self { rx, port: bound, calls: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origins() {
        for ok in ["http://localhost", "http://localhost:3000", "http://127.0.0.1:7860", "http://[::1]:8080", "https://localhost"] {
            assert!(origin_allowed(ok), "{ok}");
        }
        for bad in ["null", "http://localhost.evil.com", "http://127.0.0.1.evil.com", "http://evil.com", "file://", "http://localhost@evil.com", "http://localhost:80/x", "http://[::1].evil:1"] {
            assert!(!origin_allowed(bad), "{bad}");
        }
        assert!(is_local_host("127.0.0.1:7860") && is_local_host("localhost") && !is_local_host("evil.com:7860") && !is_local_host("localhost.evil.com"));
    }
}
