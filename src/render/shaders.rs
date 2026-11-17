//! GLSL 330 shader sources. All written for this project.

pub const MODEL_VS: &str = r#"
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_normal;
layout(location=2) in vec2 a_uv;
layout(location=3) in ivec4 a_bones;
layout(location=4) in vec4 a_weights;
layout(location=5) in float a_edge;
layout(location=6) in vec3 a_morph;
layout(location=7) in vec2 a_uvmorph;
uniform sampler2D u_bones;
uniform mat4 u_model;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform mat4 u_light_vp;
uniform int u_mode;
uniform float u_edge_px;
uniform vec2 u_viewport;
uniform mat4 u_mirror;
uniform float u_shell;
uniform float u_shell_len;
uniform float u_shell_droop;
out float v_shell;
out vec3 v_wpos;
out vec3 v_normal;
out vec2 v_uv;
out vec4 v_lpos;
out vec3 v_vnormal;
mat4 bm(int i) {
    return mat4(texelFetch(u_bones, ivec2(0, i), 0), texelFetch(u_bones, ivec2(1, i), 0),
                texelFetch(u_bones, ivec2(2, i), 0), texelFetch(u_bones, ivec2(3, i), 0));
}
void main() {
    mat4 skin = bm(a_bones.x) * a_weights.x + bm(a_bones.y) * a_weights.y
              + bm(a_bones.z) * a_weights.z + bm(a_bones.w) * a_weights.w;
    vec4 wp = u_mirror * u_model * skin * vec4(a_pos + a_morph, 1.0);
    vec3 n = normalize(mat3(u_mirror) * mat3(u_model) * mat3(skin) * a_normal);
    if (u_shell > 0.0) {
        wp.xyz += n * u_shell_len * u_shell + vec3(0.0, -1.0, 0.0) * u_shell_droop * u_shell_len * u_shell * u_shell;
    }
    v_shell = u_shell;
    v_wpos = wp.xyz;
    v_normal = n;
    v_uv = a_uv + a_uvmorph;
    v_lpos = u_light_vp * wp;
    v_vnormal = mat3(u_view) * n;
    vec4 cp = u_proj * u_view * wp;
    if (u_mode == 1) {
        vec4 cn = u_proj * vec4(mat3(u_view) * n, 0.0);
        vec2 d = cn.xy;
        float l = length(d);
        if (l > 1e-5) d /= l;
        cp.xy += d * u_edge_px * a_edge * 2.0 / u_viewport * cp.w;
    }
    gl_Position = (u_mode == 2) ? u_light_vp * wp : cp;
}
"#;

pub const MODEL_FS: &str = r#"
in vec3 v_wpos;
in vec3 v_normal;
in vec2 v_uv;
in vec4 v_lpos;
in vec3 v_vnormal;
uniform vec4 u_diffuse;
uniform vec3 u_ambient;
uniform vec3 u_specular;
uniform float u_spec_power;
uniform vec4 u_tex_tint;
uniform sampler2D u_tex;
uniform int u_has_tex;
uniform sampler2D u_sphere;
uniform int u_sphere_mode;
uniform sampler2D u_toon;
uniform float u_toon_row;
uniform int u_has_toon;
uniform vec3 u_light_dir;
uniform vec3 u_light_color;
uniform vec3 u_eye;
uniform sampler2D u_shadow;
uniform int u_use_shadow;
uniform int u_mode;
uniform vec4 u_edge_color;
uniform int u_style;
uniform float u_rim;
uniform vec3 u_rim_color;
uniform float u_spec_boost;
uniform float u_softness;
uniform vec3 u_shadow_tint;
uniform float u_skin;
uniform float u_sat;
uniform float u_ambient_k;
uniform float u_alpha_mul;
uniform vec4 u_clip;
// Ray Lighting (physically based) inputs
uniform float u_rough;
uniform float u_metal;
uniform float u_sss;
uniform float u_emis;
uniform float u_matspec;
uniform int u_mat_toon;
uniform float u_sun;
uniform float u_skylight;
uniform vec3 u_sky_top;
uniform vec3 u_sky_hor;
uniform vec3 u_sky_bot;
uniform vec3 u_ground;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 o_gbuf;
in float v_shell;
// Material settings
uniform int u_special;
uniform vec3 u_mat_tint;
uniform vec3 u_emis_col;
uniform float u_glass_op;
uniform float u_wet;
uniform float u_rain;
uniform float u_ripple_scale;
uniform float u_ripple_speed;
uniform float u_fur_density;
uniform float u_time;
uniform float u_uv_scale;
uniform sampler2D u_nmap;
uniform int u_has_nmap;
uniform float u_nmap_k;
uniform sampler2D u_hmap;
uniform int u_has_hmap;
uniform float u_hmap_k;
uniform sampler2D u_mmap;
uniform int u_has_mmap;
uniform sampler2D u_rmap;
uniform int u_has_rmap;
// Extra lights: pos.w = type (0 point, 1 spot, 2 directional), dir.w = range,
// col.rgb = colour x intensity, col.w = cos(outer cone), cone.x = cos(inner cone)
uniform int u_nlights;
uniform vec4 u_lpos[8];
uniform vec4 u_ldir[8];
uniform vec4 u_lcol[8];
uniform vec4 u_lcone[8];

//@CUSTOM_MATERIAL

float mh(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
// Normal-map frame from screen derivatives (no tangents needed in the mesh).
mat3 cot_frame(vec3 N, vec3 p, vec2 uv) {
    vec3 dp1 = dFdx(p);
    vec3 dp2 = dFdy(p);
    vec2 du1 = dFdx(uv);
    vec2 du2 = dFdy(uv);
    vec3 dp2perp = cross(dp2, N);
    vec3 dp1perp = cross(N, dp1);
    vec3 T = dp2perp * du1.x + dp1perp * du2.x;
    vec3 B = dp2perp * du1.y + dp1perp * du2.y;
    float inv = inversesqrt(max(max(dot(T, T), dot(B, B)), 1e-12));
    return mat3(T * inv, B * inv, N);
}
// Height of expanding rain rings on a surface (for wet + rain and water).
float ripple_h(vec2 q, float t) {
    float h = 0.0;
    for (int k = 0; k < 2; k++) {
        vec2 c = floor(q) + float(k) * vec2(0.37, 0.61);
        vec2 f = fract(q + float(k) * vec2(0.37, 0.61)) - 0.5;
        float rnd = mh(c);
        vec2 off = vec2(mh(c + 3.1), mh(c + 7.7)) - 0.5;
        float ph = fract(t + rnd);
        float d = length(f - off * 0.5);
        h += sin((d - ph * 0.5) * 40.0) * exp(-abs(d - ph * 0.5) * 20.0) * (1.0 - ph);
    }
    return h;
}
float waves_h(vec2 q, float t) {
    return sin(q.x * 1.3 + t * 1.7) * 0.5 + sin(q.y * 1.9 - t * 1.3) * 0.35 + sin((q.x + q.y) * 2.7 + t * 2.3) * 0.2;
}
// Light energy arriving from extra light i (colour x falloff), and its direction.
vec3 extra_light(int i, vec3 p, out vec3 L) {
    vec4 lp = u_lpos[i];
    vec4 ld = u_ldir[i];
    vec4 lc = u_lcol[i];
    int ty = int(lp.w + 0.5);
    if (ty == 2) {
        L = normalize(-ld.xyz);
        return lc.rgb;
    }
    vec3 dv = lp.xyz - p;
    float dist = length(dv);
    L = dv / max(dist, 1e-4);
    float range = max(ld.w, 0.1);
    float att = pow(clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0), 2.0) / (1.0 + dist * dist * 0.02);
    if (ty == 1) {
        float cd = dot(-L, normalize(ld.xyz));
        att *= smoothstep(lc.w, u_lcone[i].x, cd);
    }
    return lc.rgb * att;
}

// Sky light seen along direction d, blurred toward its average as surfaces get rougher.
vec3 sky_env(vec3 d, float rough) {
    float y = d.y;
    vec3 up = y > 0.0 ? mix(u_sky_hor, u_sky_top, pow(clamp(y, 0.0, 1.0), 0.6)) : mix(u_sky_hor, u_ground, pow(clamp(-y, 0.0, 1.0), 0.45));
    vec3 avg = (u_sky_top + u_sky_hor * 2.0 + u_ground) * 0.25;
    vec3 hemi = mix(u_ground, mix(u_sky_hor, u_sky_top, 0.5), d.y * 0.5 + 0.5);
    return mix(up, mix(hemi, avg, 0.3), clamp(rough * 1.15, 0.0, 1.0));
}
vec3 to_lin(vec3 c) { return pow(max(c, 0.0), vec3(2.2)); }
// Split-sum environment response approximation (analytic fit, written for this project).
vec2 env_brdf(float ndv, float rough) {
    float a = rough * rough;
    float f = pow(1.0 - ndv, 5.0);
    float scale = mix(1.0 - f, 0.55 - 0.45 * f, a);
    float bias = f * (1.0 - a) * 0.9;
    return vec2(scale, bias);
}

float shadow_factor(vec3 n, vec3 l) {
    if (u_use_shadow == 0) return 1.0;
    vec3 p = v_lpos.xyz / v_lpos.w * 0.5 + 0.5;
    if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 || p.z > 1.0) return 1.0;
    float bias = max(0.0025 * (1.0 - dot(n, l)), 0.0006);
    vec2 ts = 1.0 / vec2(textureSize(u_shadow, 0));
    float s = 0.0;
    for (int x = -1; x <= 1; x++)
        for (int y = -1; y <= 1; y++) {
            float d = texture(u_shadow, p.xy + vec2(x, y) * ts * 1.5).r;
            s += (p.z - bias > d) ? 0.0 : 1.0;
        }
    return s / 9.0;
}

void main() {
    if (dot(vec4(v_wpos, 1.0), u_clip) < 0.0) discard;
    o_gbuf = vec4(0.0, 0.0, 0.0, 1.0);
    if (u_mode == 1) { o = u_edge_color; if (o.a < 0.01) discard; return; }
    if (u_mode == 2) { o = vec4(1.0); return; }
    vec3 n = normalize(v_normal);
    if (!gl_FrontFacing) n = -n;
    vec3 v = normalize(u_eye - v_wpos);
    vec2 tuv = v_uv;
    vec2 muv = v_uv * u_uv_scale;
    mat3 frame = cot_frame(n, v_wpos, v_uv);
    // height map: shift texture lookups (parallax) for a carved, bumpy look
    if (u_has_hmap == 1) {
        vec3 vt = transpose(frame) * v;
        float hgt = texture(u_hmap, muv).r - 0.5;
        vec2 shift = vt.xy / max(vt.z, 0.25) * hgt * u_hmap_k;
        muv += shift * u_uv_scale;
        tuv += shift;
    }
    // fur and grass: each shell keeps only the strands tall enough to reach it
    if ((u_special == 4 || u_special == 5) && v_shell > 0.0) {
        vec2 g = tuv * u_fur_density;
        vec2 cell = floor(g);
        vec2 f = fract(g) - 0.5;
        float tall = 0.35 + 0.65 * mh(cell);
        float thick = (u_special == 5) ? 0.18 : 0.42;
        if (v_shell > tall || length(f * vec2(1.0, u_special == 5 ? 0.6 : 1.0)) > thick * (1.0 - v_shell / tall)) discard;
    }
    vec4 albedo = vec4(1.0);
    if (u_has_tex == 1) albedo = texture(u_tex, tuv);
    albedo *= u_tex_tint;
    albedo.rgb *= u_mat_tint;
    if (u_special == 4 || u_special == 5) albedo.rgb *= mix(0.35, 1.05, v_shell);
    float alpha = u_diffuse.a * albedo.a * u_alpha_mul;
    if (alpha < 0.004) discard;
    float m_rough = u_rough;
    float m_metal = u_metal;
    if (u_has_rmap == 1) m_rough = clamp(texture(u_rmap, muv).r, 0.02, 1.0);
    if (u_has_mmap == 1) m_metal = texture(u_mmap, muv).r;
    vec3 m_emis = u_emis_col * u_emis;
    // normal map
    if (u_has_nmap == 1) {
        vec3 t = texture(u_nmap, muv).xyz * 2.0 - 1.0;
        t.xy *= u_nmap_k;
        n = normalize(frame * t);
    }
    // water ripples / rain drops bend the surface
    if (u_special == 2 || (u_special == 3 && u_rain > 0.0)) {
        vec2 q = v_wpos.xz * 0.6 * u_ripple_scale;
        float t = u_time * u_ripple_speed;
        float e = 0.02;
        float k = (u_special == 2) ? 0.08 : 0.05 * u_rain;
        float h0 = ripple_h(q * 2.0, t) + (u_special == 2 ? waves_h(q, t) : 0.0);
        float hx = ripple_h((q + vec2(e, 0.0)) * 2.0, t) + (u_special == 2 ? waves_h(q + vec2(e, 0.0), t) : 0.0);
        float hz = ripple_h((q + vec2(0.0, e)) * 2.0, t) + (u_special == 2 ? waves_h(q + vec2(0.0, e), t) : 0.0);
        vec3 bump = vec3(-(hx - h0) / e, 0.0, -(hz - h0) / e) * k;
        n = normalize(n + bump * smoothstep(0.2, 0.8, abs(n.y) + (u_special == 2 ? 1.0 : 0.0)));
    }
    if (u_special == 3) {
        // wet: darker, glossier, puddles gather on upward faces
        float w = u_wet * mix(0.6, 1.0, clamp(n.y, 0.0, 1.0));
        albedo.rgb *= mix(1.0, 0.55, w);
        m_rough = mix(m_rough, 0.06, w);
    }
    if (u_special == 2) {
        m_rough = min(m_rough, 0.06);
        alpha *= mix(0.55, 1.0, pow(1.0 - max(dot(n, v), 0.0), 3.0));
    }
    if (u_special == 1) {
        m_rough = min(m_rough, 0.08);
        float fr = 0.04 + 0.96 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
        alpha *= clamp(mix(u_glass_op, 1.0, fr), 0.02, 1.0);
    }
    if (u_special == 6) m_emis += albedo.rgb * u_emis * 0.5;
    vec3 cm_base = albedo.rgb;
    vec3 cm_n = vec3(0.5, 0.5, 1.0);
    float cm_use_n = 0.0;
    custom_material(cm_base, m_rough, m_metal, m_emis, alpha, cm_n, cm_use_n, v_uv, n, v, u_time);
    albedo.rgb = cm_base;
    if (cm_use_n > 0.5) n = normalize(frame * (cm_n * 2.0 - 1.0));
    if (alpha < 0.004) discard;
    vec3 l = normalize(-u_light_dir);
    vec3 h = normalize(l + v);
    float ndl = dot(n, l);
    float ndv = max(dot(n, v), 0.0);
    float sh = shadow_factor(normalize(v_normal), l);

    vec3 base;
    vec3 col;
    vec3 spec = vec3(0.0);
    float lit = min(ndl * 0.5 + 0.5, mix(0.5, 1.0, sh) * (ndl > 0.0 ? 1.0 : 0.5));
    if (u_style == 4 && u_mat_toon == 0) {
        // Ray Lighting: energy-based shading in linear light.
        vec3 base_col = to_lin(albedo.rgb * clamp(u_diffuse.rgb + u_ambient * 0.35, 0.0, 1.0));
        float rough = clamp(m_rough, 0.03, 1.0);
        float a2 = rough * rough * rough * rough;
        float metal = clamp(m_metal, 0.0, 1.0);
        vec3 f0 = mix(vec3(0.04), base_col, metal);
        float nl = max(ndl, 0.0);
        float nv = max(ndv, 1e-3);
        float nh = max(dot(n, h), 0.0);
        float vh = max(dot(v, h), 0.0);
        float dd = nh * nh * (a2 - 1.0) + 1.0;
        float D = a2 / (3.14159 * dd * dd + 1e-5);
        float k = (rough + 1.0) * (rough + 1.0) / 8.0;
        float G = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
        vec3 F = f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
        vec3 radiance = u_light_color / 0.6 * u_sun;
        vec3 spec_d = D * G * F / max(4.0 * nl * nv, 1e-4) * nl;
        // skin: wrapped diffuse and a warm glow where light fades into shadow
        float w = 0.45 * u_sss;
        float wrap = max((ndl + w) / (1.0 + w), 0.0);
        float diff_term = mix(nl, wrap, u_sss);
        vec3 scatter = u_sss * vec3(1.0, 0.32, 0.22) * smoothstep(-0.35, 0.25, ndl) * (1.0 - smoothstep(0.2, 0.75, ndl)) * 0.35;
        vec3 kd = (1.0 - F) * (1.0 - metal);
        float shadow = mix(0.0, 1.0, sh);
        vec3 direct = (kd * base_col * (diff_term + scatter * shadow) + spec_d * u_matspec * u_spec_boost) * radiance * shadow;
        // sky light from all around, specular from the reflected direction
        vec3 r = reflect(-v, n);
        vec2 eb = env_brdf(nv, rough);
        vec3 amb_d = to_lin(sky_env(n, 1.0)) * base_col * kd;
        vec3 amb_s = to_lin(sky_env(r, rough)) * (f0 * eb.x + eb.y) * u_matspec;
        float sky_occ = mix(0.55, 1.0, sh) * (0.6 + 0.4 * clamp(n.y * 0.5 + 0.5, 0.0, 1.0));
        vec3 ambient = (amb_d + amb_s) * u_skylight * u_ambient_k * sky_occ;
        // extra lights
        for (int i = 0; i < 8; i++) {
            if (i >= u_nlights) break;
            vec3 L;
            vec3 rad = extra_light(i, v_wpos, L);
            float enl = max(dot(n, L), 0.0);
            if (enl <= 0.0 || dot(rad, rad) < 1e-8) continue;
            vec3 H = normalize(L + v);
            float enh = max(dot(n, H), 0.0);
            float edd = enh * enh * (a2 - 1.0) + 1.0;
            float eD = a2 / (3.14159 * edd * edd + 1e-5);
            float eG = (enl / (enl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
            vec3 eF = f0 + (1.0 - f0) * pow(1.0 - max(dot(v, H), 0.0), 5.0);
            vec3 ekd = (1.0 - eF) * (1.0 - metal);
            direct += (ekd * base_col * mix(enl, max((dot(n, L) + w) / (1.0 + w), 0.0), u_sss) + eD * eG * eF / max(4.0 * enl * nv, 1e-4) * enl * u_matspec) * rad;
        }
        vec3 emis = to_lin(m_emis);
        vec3 lin = direct + ambient + emis;
        col = pow(max(lin, 0.0), vec3(1.0 / 2.2));
    } else if (u_style == 2) {
        // Physically-inspired: lambert + hemispheric ambient + GGX-like specular.
        vec3 a = albedo.rgb * clamp(u_diffuse.rgb + u_ambient * 0.4, 0.0, 1.0);
        a = pow(a, vec3(2.2));
        float rough = clamp(sqrt(2.0 / (u_spec_power + 2.0)), 0.08, 1.0);
        float r2 = rough * rough;
        float ndh = max(dot(n, h), 0.0);
        float dnm = ndh * ndh * (r2 * r2 - 1.0) + 1.0;
        float D = r2 * r2 / (3.14159 * dnm * dnm + 1e-4);
        float F = 0.04 + 0.96 * pow(1.0 - max(dot(h, v), 0.0), 5.0);
        vec3 sky = mix(vec3(0.25, 0.22, 0.2), vec3(0.45, 0.5, 0.6), n.y * 0.5 + 0.5) * u_ambient_k;
        vec3 diff = a * (u_light_color * 2.4 * max(ndl, 0.0) * sh + sky);
        spec = u_light_color * 2.0 * D * F * max(ndl, 0.0) * sh * u_spec_boost * (0.3 + length(u_specular));
        vec3 rim = u_rim_color * pow(1.0 - ndv, 4.0) * u_rim * 0.5 * a * 3.0;
        col = pow(max(diff + rim, 0.0), vec3(1.0 / 2.2));
        col += spec;
    } else {
        base = clamp(u_ambient * u_ambient_k + u_diffuse.rgb * u_light_color, 0.0, 1.0);
        col = base * albedo.rgb;
        if (u_sphere_mode != 0) {
            vec2 suv = v_vnormal.xy * vec2(0.5, -0.5) + 0.5;
            vec3 sp = texture(u_sphere, suv).rgb;
            if (u_sphere_mode == 1) col *= sp; else if (u_sphere_mode == 2) col += sp;
        }
        if (u_style == 0) {
            vec3 toon = vec3(1.0);
            if (u_has_toon == 1) toon = texture(u_toon, vec2(clamp(lit, 0.02, 0.98), u_toon_row)).rgb;
            col *= toon;
        } else {
            float soft = max(u_softness, 0.02) * 0.5;
            float shade = smoothstep(0.5 - soft, 0.5 + soft, lit);
            vec3 tint = u_shadow_tint;
            if (u_style == 3) tint = mix(tint, vec3(1.0), 0.35);
            col *= mix(tint, vec3(1.0), shade);
            col += u_skin * shade * (1.0 - shade) * 1.6 * vec3(0.9, 0.3, 0.2) * albedo.rgb;
            if (u_style == 3) col *= 1.08;
        }
        float sp = pow(max(dot(n, h), 0.0), max(u_spec_power, 1.0));
        if (u_style == 1 || u_style == 3) sp = smoothstep(0.45, 0.55, sp);
        spec = u_specular * u_light_color * sp * u_spec_boost * sh;
        col += spec;
        if (u_style != 0) {
            float rim = pow(1.0 - ndv, 3.0) * smoothstep(-0.3, 0.6, ndl * 0.5 + 0.5);
            col += u_rim_color * rim * u_rim * (u_style == 3 ? 0.7 : 0.45);
        }
        // glossy materials (wet, glass, water) get a sharp highlight even in toon looks
        if (m_rough < 0.15) {
            float gs = pow(max(dot(n, h), 0.0), 200.0) * (1.0 - m_rough * 5.0);
            col += u_light_color * gs * sh * 1.5;
        }
        // extra lights, banded in toon looks
        for (int i = 0; i < 8; i++) {
            if (i >= u_nlights) break;
            vec3 L;
            vec3 rad = extra_light(i, v_wpos, L);
            float enl = dot(n, L);
            float band = (u_style == 0) ? smoothstep(0.0, 0.08, enl) : smoothstep(-0.1, 0.4, enl);
            // soft-clip so several strong lights tint rather than blow out the toon colours
            vec3 add = albedo.rgb * u_diffuse.rgb * rad * band * 0.3;
            col += add / (1.0 + max(max(col.r, col.g), col.b));
            col += rad * pow(max(dot(n, normalize(L + v)), 0.0), max(u_spec_power, 8.0)) * length(u_specular) * 0.2;
        }
        col += m_emis;
    }
    float luma = dot(col, vec3(0.299, 0.587, 0.114));
    col = mix(vec3(luma), col, u_sat);
    o = vec4(col, alpha);
    // Surface info for screen-space reflections: normal and roughness (negative = metal).
    float gr = max(m_rough, 0.02) * (m_metal > 0.5 ? -1.0 : 1.0);
    o_gbuf = vec4(n, alpha > 0.5 ? gr : 1.0);
}
"#;

pub const FLOOR_VS: &str = r#"
layout(location=0) in vec3 a_pos;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform mat4 u_light_vp;
uniform float u_size;
uniform float u_height;
out vec3 v_wpos;
out vec4 v_lpos;
out vec4 v_clip;
void main() {
    vec3 p = vec3(a_pos.x * u_size, u_height, a_pos.z * u_size);
    v_wpos = p;
    v_lpos = u_light_vp * vec4(p, 1.0);
    v_clip = u_proj * u_view * vec4(p, 1.0);
    gl_Position = v_clip;
}
"#;

pub const FLOOR_FS: &str = r#"
in vec3 v_wpos;
in vec4 v_lpos;
in vec4 v_clip;
uniform sampler2D u_shadow;
uniform int u_use_shadow;
uniform int u_grid;
uniform vec3 u_color;
uniform vec3 u_eye;
uniform sampler2D u_reflect;
uniform float u_reflect_k;
uniform float u_reflect_blur;
uniform vec3 u_reflect_tint;
uniform float u_shadow_dark;
uniform float u_floor_rough;
uniform int u_nlights;
uniform vec4 u_lpos[8];
uniform vec4 u_ldir[8];
uniform vec4 u_lcol[8];
uniform vec4 u_lcone[8];
vec3 extra_light(int i, vec3 p, out vec3 L) {
    vec4 lp = u_lpos[i];
    vec4 ld = u_ldir[i];
    vec4 lc = u_lcol[i];
    int ty = int(lp.w + 0.5);
    if (ty == 2) { L = normalize(-ld.xyz); return lc.rgb; }
    vec3 dv = lp.xyz - p;
    float dist = length(dv);
    L = dv / max(dist, 1e-4);
    float range = max(ld.w, 0.1);
    float att = pow(clamp(1.0 - pow(dist / range, 4.0), 0.0, 1.0), 2.0) / (1.0 + dist * dist * 0.02);
    if (ty == 1) att *= smoothstep(lc.w, u_lcone[i].x, dot(-L, normalize(ld.xyz)));
    return lc.rgb * att;
}
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 o_gbuf;
float grid(vec2 p, float s, float w) {
    vec2 g = abs(fract(p / s - 0.5) - 0.5) / fwidth(p / s);
    return 1.0 - min(min(g.x, g.y) / w, 1.0);
}
void main() {
    float dist = length(v_wpos.xz - u_eye.xz);
    vec3 c = u_color;
    if (u_grid == 1) {
        float g1 = grid(v_wpos.xz, 5.0, 1.0);
        float g2 = grid(v_wpos.xz, 25.0, 1.4);
        c = mix(c, c * 1.9 + 0.03, g1 * 0.35 + g2 * 0.4);
        float ax = 1.0 - min(abs(v_wpos.x) / fwidth(v_wpos.x) / 1.5, 1.0);
        float az = 1.0 - min(abs(v_wpos.z) / fwidth(v_wpos.z) / 1.5, 1.0);
        c = mix(c, vec3(0.85, 0.3, 0.35), az * 0.6);
        c = mix(c, vec3(0.3, 0.45, 0.9), ax * 0.6);
    }
    if (u_reflect_k > 0.0) {
        vec2 suv = v_clip.xy / v_clip.w * 0.5 + 0.5;
        vec4 r = vec4(0.0);
        float b = u_reflect_blur * 0.006;
        r += texture(u_reflect, suv) * 0.4;
        r += texture(u_reflect, suv + vec2(b, 0.0)) * 0.15;
        r += texture(u_reflect, suv - vec2(b, 0.0)) * 0.15;
        r += texture(u_reflect, suv + vec2(0.0, b)) * 0.15;
        r += texture(u_reflect, suv - vec2(0.0, b)) * 0.15;
        // Only darken/replace the floor where something is actually reflected.
        float cover = clamp(r.a, 0.0, 1.0) * u_reflect_k;
        c = c * (1.0 - cover) + r.rgb * u_reflect_tint * u_reflect_k;
    }
    if (u_use_shadow == 1) {
        vec3 p = v_lpos.xyz / v_lpos.w * 0.5 + 0.5;
        if (p.x > 0.0 && p.x < 1.0 && p.y > 0.0 && p.y < 1.0 && p.z < 1.0) {
            vec2 ts = 1.0 / vec2(textureSize(u_shadow, 0));
            float s = 0.0;
            for (int x = -2; x <= 2; x++)
                for (int y = -2; y <= 2; y++)
                    s += (p.z - 0.0015 > texture(u_shadow, p.xy + vec2(x, y) * ts * 1.5).r) ? 0.0 : 1.0;
            c *= mix(1.0 - u_shadow_dark, 1.0, s / 25.0);
        }
    }
    for (int i = 0; i < 8; i++) {
        if (i >= u_nlights) break;
        vec3 L;
        vec3 rad = extra_light(i, v_wpos, L);
        c += rad * max(L.y, 0.0) * (u_color * 1.6 + 0.12) * 0.6;
    }
    float fade = 1.0 - smoothstep(120.0, 380.0, dist);
    o = vec4(c, fade);
    o_gbuf = vec4(0.0, 1.0, 0.0, u_floor_rough);
}
"#;

pub const FS_VS: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub const SKY_FS: &str = r#"
in vec2 v_uv;
uniform mat4 u_inv_vp;
uniform vec3 u_top;
uniform vec3 u_horizon;
uniform vec3 u_bottom;
uniform float u_stars;
out vec4 o;
float hash(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
void main() {
    vec4 a = u_inv_vp * vec4(v_uv * 2.0 - 1.0, -1.0, 1.0);
    vec4 b = u_inv_vp * vec4(v_uv * 2.0 - 1.0, 1.0, 1.0);
    vec3 d = normalize(b.xyz / b.w - a.xyz / a.w);
    float y = d.y;
    vec3 c = y > 0.0 ? mix(u_horizon, u_top, pow(clamp(y, 0.0, 1.0), 0.6)) : mix(u_horizon, u_bottom, pow(clamp(-y, 0.0, 1.0), 0.4));
    if (u_stars > 0.0 && y > 0.0) {
        vec3 q = floor(d * 300.0);
        float h = hash(q);
        if (h > 0.997) c += vec3(1.0, 0.95, 0.9) * u_stars * (h - 0.997) * 400.0 * y;
    }
    o = vec4(c, 1.0);
}
"#;

pub const WATER_FS: &str = r#"
in vec3 v_wpos;
in vec4 v_lpos;
in vec4 v_clip;
uniform vec3 u_eye;
uniform vec3 u_light_dir;
uniform vec3 u_light_color;
uniform float u_time;
uniform vec3 u_deep;
uniform vec3 u_shallow;
uniform float u_wave;
uniform float u_speed;
uniform float u_reflect_k;
uniform float u_clarity;
uniform float u_sparkle;
uniform sampler2D u_reflect;
uniform vec3 u_sky;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 o_gbuf;
vec2 waveGrad(vec2 p, float t) {
    vec2 g = vec2(0.0);
    vec2 dirs[5] = vec2[5](vec2(1.0, 0.2), vec2(-0.4, 1.0), vec2(0.7, -0.8), vec2(-0.9, -0.3), vec2(0.2, 0.9));
    float f = 0.18 / u_wave;
    float a = 0.35;
    for (int i = 0; i < 5; i++) {
        vec2 d = normalize(dirs[i]);
        float ph = dot(d, p) * f + t * (1.0 + float(i) * 0.37);
        g += d * cos(ph) * f * a;
        f *= 1.83;
        a *= 0.62;
    }
    return g;
}
void main() {
    float t = u_time * u_speed;
    vec2 g = waveGrad(v_wpos.xz, t);
    vec3 n = normalize(vec3(-g.x, 1.0, -g.y));
    vec3 v = normalize(u_eye - v_wpos);
    float fres = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    vec2 suv = v_clip.xy / v_clip.w * 0.5 + 0.5;
    vec3 refl = texture(u_reflect, suv + n.xz * 0.04).rgb;
    refl = mix(u_sky, refl, step(0.001, dot(refl, vec3(1.0))));
    vec3 body = mix(u_deep, u_shallow, 0.5 + 0.5 * n.y * (1.0 - fres));
    vec3 c = mix(body, refl, clamp(fres * 1.6 * u_reflect_k + 0.25 * u_reflect_k, 0.0, 1.0));
    vec3 l = normalize(-u_light_dir);
    vec3 r = reflect(-l, n);
    float glint = pow(max(dot(r, v), 0.0), 220.0) * u_sparkle * 6.0;
    c += u_light_color * glint;
    float dist = length(v_wpos.xz - u_eye.xz);
    float a = mix(1.0, 0.55, u_clarity) * (1.0 - smoothstep(250.0, 600.0, dist));
    o = vec4(c, a);
    o_gbuf = vec4(n, 0.04);
}
"#;

pub const PARTICLE_VS: &str = r#"
layout(location=0) in vec2 a_corner;
layout(location=1) in vec4 i_pos_size;
layout(location=2) in vec4 i_color;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform vec3 u_right;
uniform vec3 u_up;
out vec2 v_c;
out vec4 v_col;
void main() {
    vec3 p = i_pos_size.xyz + (u_right * a_corner.x + u_up * a_corner.y) * i_pos_size.w;
    v_c = a_corner;
    v_col = i_color;
    gl_Position = u_proj * u_view * vec4(p, 1.0);
}
"#;

pub const PARTICLE_FS: &str = r#"
in vec2 v_c;
in vec4 v_col;
uniform int u_shape;
out vec4 o;
void main() {
    float r = length(v_c);
    float a;
    if (u_shape == 2) {
        // petal: elongated soft ellipse
        vec2 q = v_c * vec2(1.6, 0.9);
        a = smoothstep(1.0, 0.6, length(q));
    } else if (u_shape == 0 || u_shape == 6) {
        // sparkle: core + star cross
        a = exp(-r * r * 9.0) + 0.5 * exp(-abs(v_c.x) * 18.0) * exp(-abs(v_c.y) * 3.0) + 0.5 * exp(-abs(v_c.y) * 18.0) * exp(-abs(v_c.x) * 3.0);
    } else if (u_shape == 4) {
        a = smoothstep(1.0, 0.85, r) * (0.3 + 0.7 * smoothstep(0.6, 0.95, r));
    } else if (u_shape == 5) {
        a = step(abs(v_c.x), 0.7) * step(abs(v_c.y), 0.4);
    } else {
        a = smoothstep(1.0, 0.2, r);
    }
    if (a < 0.003) discard;
    o = vec4(v_col.rgb, v_col.a * a);
}
"#;

pub const BEAM_VS: &str = r#"
layout(location=0) in vec2 a_corner;
layout(location=1) in vec4 i_start;
layout(location=2) in vec4 i_end;
layout(location=3) in vec4 i_color;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform vec3 u_eye;
out vec2 v_c;
out vec4 v_col;
void main() {
    vec3 s = i_start.xyz;
    vec3 e = i_end.xyz;
    float t = a_corner.y * 0.5 + 0.5;
    vec3 p = mix(s, e, t);
    vec3 axis = normalize(e - s);
    vec3 side = normalize(cross(axis, normalize(u_eye - p)) + vec3(1e-5));
    float w = mix(i_start.w * 0.15, i_start.w, t);
    p += side * a_corner.x * w;
    v_c = vec2(a_corner.x, t);
    v_col = i_color;
    gl_Position = u_proj * u_view * vec4(p, 1.0);
}
"#;

pub const BEAM_FS: &str = r#"
in vec2 v_c;
in vec4 v_col;
out vec4 o;
void main() {
    float across = exp(-v_c.x * v_c.x * 4.0);
    float along = pow(1.0 - v_c.y, 1.4) * smoothstep(0.0, 0.03, v_c.y);
    o = vec4(v_col.rgb * across * along * v_col.a, 1.0);
}
"#;

// ------------------------------------------------------------------ post effects

pub const COPY_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 o;
void main() { o = texture(u_tex, v_uv); }
"#;

pub const BRIGHT_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform float u_threshold;
out vec4 o;
void main() {
    vec3 c = texture(u_tex, v_uv).rgb;
    float l = max(c.r, max(c.g, c.b));
    float k = max(l - u_threshold, 0.0) / max(l, 1e-4);
    o = vec4(c * k, 1.0);
}
"#;

pub const DOWN_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec2 u_texel;
out vec4 o;
void main() {
    vec2 t = u_texel;
    vec3 c = texture(u_tex, v_uv).rgb * 4.0;
    c += texture(u_tex, v_uv + vec2(-t.x, -t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(t.x, -t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(-t.x, t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(t.x, t.y)).rgb;
    o = vec4(c / 8.0, 1.0);
}
"#;

pub const UP_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform sampler2D u_base;
uniform vec2 u_texel;
uniform float u_radius;
out vec4 o;
void main() {
    vec2 t = u_texel * u_radius;
    vec3 c = texture(u_tex, v_uv + vec2(-t.x, 0.0)).rgb * 2.0;
    c += texture(u_tex, v_uv + vec2(t.x, 0.0)).rgb * 2.0;
    c += texture(u_tex, v_uv + vec2(0.0, -t.y)).rgb * 2.0;
    c += texture(u_tex, v_uv + vec2(0.0, t.y)).rgb * 2.0;
    c += texture(u_tex, v_uv + vec2(-t.x, -t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(t.x, -t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(-t.x, t.y)).rgb;
    c += texture(u_tex, v_uv + vec2(t.x, t.y)).rgb;
    c += texture(u_tex, v_uv).rgb * 4.0;
    o = vec4(c / 16.0 + texture(u_base, v_uv).rgb, 1.0);
}
"#;

pub const BLOOM_MIX_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform sampler2D u_bloom;
uniform float u_intensity;
out vec4 o;
void main() { o = vec4(texture(u_tex, v_uv).rgb + texture(u_bloom, v_uv).rgb * u_intensity, 1.0); }
"#;

/// One shader for many simple per-pixel effects, picked with u_kind.
pub const FX_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform sampler2D u_tex2;
uniform sampler2D u_depth;
uniform int u_kind;
uniform vec4 u_p0;
uniform vec4 u_p1;
uniform vec4 u_p2;
uniform vec2 u_texel;
uniform float u_time;
uniform float u_near;
uniform float u_far;
uniform mat4 u_inv_proj;
uniform mat4 u_proj;
uniform mat4 u_view;
uniform mat4 u_inv_vp;
uniform mat4 u_prev_vp;
uniform sampler2D u_gbuf;
uniform int u_has_gbuf;
out vec4 o;

float lin_depth(vec2 uv) {
    float d = texture(u_depth, uv).r * 2.0 - 1.0;
    return 2.0 * u_near * u_far / (u_far + u_near - d * (u_far - u_near));
}
vec3 view_pos(vec2 uv) {
    float d = texture(u_depth, uv).r * 2.0 - 1.0;
    vec4 p = u_inv_proj * vec4(uv * 2.0 - 1.0, d, 1.0);
    return p.xyz / p.w;
}
float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
vec3 aces(vec3 x) { return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0); }
// Input is display-referred (1.0 = white); values above 1.0 are highlights to roll off.
vec3 tonemap(vec3 c, int k) {
    c = max(c, 0.0);
    if (k == 1) { // filmic: gentle toe, soft shoulder above 0.75
        vec3 knee = vec3(0.75);
        vec3 over = max(c - knee, 0.0);
        vec3 y = min(c, knee) + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
        return pow(y, vec3(1.04));
    }
    if (k == 2) { // ACES-like in linear light
        vec3 lin = pow(c, vec3(2.2));
        return pow(aces(lin * 1.25), vec3(1.0 / 2.2));
    }
    if (k == 3) { vec3 lin = pow(c, vec3(2.2)) * 1.6; return pow(lin / (1.0 + lin) * 1.25, vec3(1.0 / 2.2)); }
    if (k == 4) { vec3 x = clamp(c, 0.0, 1.5) / 1.5; return clamp(x * x * (3.0 - 2.0 * x) * 1.08 + c * 0.12 - 0.04, 0.0, 1.0); }
    return c;
}
void main() {
    vec4 src = texture(u_tex, v_uv);
    vec3 c = src.rgb;
    if (u_kind == 1) { // tonemap: p0.x curve, p0.y exposure
        c = tonemap(c * exp2(u_p0.y), int(u_p0.x));
    } else if (u_kind == 2) { // vignette: amount, softness, colour
        float d = length((v_uv - 0.5) * vec2(1.0, 0.8)) * 1.4;
        float v = smoothstep(1.0 - u_p0.y, 1.0 + u_p0.y * 0.2, d) * u_p0.x;
        c = mix(c, u_p1.rgb, v);
    } else if (u_kind == 3) { // chromatic aberration
        vec2 d = (v_uv - 0.5) * u_p0.x * 0.012;
        c = vec3(texture(u_tex, v_uv + d).r, c.g, texture(u_tex, v_uv - d).b);
    } else if (u_kind == 4) { // film grain
        vec2 g = floor(v_uv / u_texel / u_p0.y);
        float n = hash(g + fract(u_time) * 91.7) - 0.5;
        c += n * u_p0.x * (0.5 + 0.5 * dot(c, vec3(0.33)));
    } else if (u_kind == 5) { // sharpen
        vec3 b = texture(u_tex, v_uv + vec2(u_texel.x, 0)).rgb + texture(u_tex, v_uv - vec2(u_texel.x, 0)).rgb
               + texture(u_tex, v_uv + vec2(0, u_texel.y)).rgb + texture(u_tex, v_uv - vec2(0, u_texel.y)).rgb;
        c = c + (c - b * 0.25) * u_p0.x;
    } else if (u_kind == 6) { // fxaa-lite
        vec3 L = vec3(0.299, 0.587, 0.114);
        float lM = dot(c, L);
        float lN = dot(texture(u_tex, v_uv + vec2(0, u_texel.y)).rgb, L);
        float lS = dot(texture(u_tex, v_uv - vec2(0, u_texel.y)).rgb, L);
        float lE = dot(texture(u_tex, v_uv + vec2(u_texel.x, 0)).rgb, L);
        float lW = dot(texture(u_tex, v_uv - vec2(u_texel.x, 0)).rgb, L);
        float range = max(max(max(lN, lS), max(lE, lW)), lM) - min(min(min(lN, lS), min(lE, lW)), lM);
        if (range > 0.06) {
            vec2 dir = normalize(vec2(-(lN - lS), lE - lW) + 1e-5) * u_texel;
            vec3 a = 0.5 * (texture(u_tex, v_uv + dir * 0.5).rgb + texture(u_tex, v_uv - dir * 0.5).rgb);
            vec3 b = 0.5 * a + 0.25 * (texture(u_tex, v_uv + dir * 1.5).rgb + texture(u_tex, v_uv - dir * 1.5).rgb);
            c = mix(a, b, 0.5);
        }
    } else if (u_kind == 7) { // lens depth of field: p0 focus, aperture*focal (px scale), max px, blades ; p1.x highlight boost
        // circle of confusion from a thin-lens model, in pixels (signed: near < 0 < far)
        float focus = u_p0.x;
        float z = lin_depth(v_uv);
        float coc = clamp(u_p0.y * (z - focus) / max(z, 0.01), -u_p0.z, u_p0.z);
        float blades = u_p0.w;
        float boost = u_p1.x;
        vec3 acc = vec3(0.0);
        float wsum = 0.0;
        float maxr = u_p0.z;
        for (int i = 0; i < 64; i++) {
            float fi = float(i) + 0.5;
            float a = fi * 2.39996;
            float rr = sqrt(fi / 64.0);
            // bladed aperture: pull the disc in between the blades
            if (blades > 2.5) {
                float seg = 6.28318 / blades;
                float m = mod(a, seg) - seg * 0.5;
                rr *= cos(seg * 0.5) / cos(m);
            }
            vec2 off = vec2(cos(a), sin(a)) * rr * maxr;
            vec2 suv = v_uv + off * u_texel;
            float zs = lin_depth(suv);
            float cs = clamp(u_p0.y * (zs - focus) / max(zs, 0.01), -u_p0.z, u_p0.z);
            float dist = rr * maxr;
            // a sample contributes if its own blur reaches this pixel; nearer blur spills over focus
            float reach = (zs < z) ? abs(cs) : min(abs(cs), abs(coc));
            float k = smoothstep(dist - 1.5, dist + 0.5, reach);
            vec3 sc = texture(u_tex, suv).rgb;
            float lum = dot(sc, vec3(0.299, 0.587, 0.114));
            float hw = 1.0 + boost * max(lum - 0.8, 0.0) * 4.0;
            acc += sc * k * hw;
            wsum += k * hw;
        }
        float self_w = 1.0 / (1.0 + abs(coc));
        c = (acc + c * self_w) / (wsum + self_w);
    } else if (u_kind == 8) { // ssao: radius, strength
        vec3 p = view_pos(v_uv);
        if (texture(u_depth, v_uv).r < 0.9999) {
            vec3 n = normalize(cross(dFdx(p), dFdy(p)));
            vec4 gbn = texture(u_gbuf, v_uv);
            if (u_has_gbuf == 1 && dot(gbn.xyz, gbn.xyz) > 0.5) n = normalize(mat3(u_view) * gbn.xyz);
            float occ = 0.0;
            float rnd = hash(v_uv / u_texel) * 6.2831;
            for (int i = 0; i < 12; i++) {
                float a = float(i) * 2.39996 + rnd;
                float r = (float(i) + 1.0) / 12.0;
                vec3 dir = normalize(vec3(cos(a) * r, sin(a) * r, sqrt(max(1.0 - r * r, 0.0))));
                vec3 t = normalize(cross(n, abs(n.y) < 0.9 ? vec3(0, 1, 0) : vec3(1, 0, 0)));
                vec3 b = cross(n, t);
                vec3 s = p + (t * dir.x + b * dir.y + n * dir.z) * u_p0.x * r;
                vec4 q = u_proj * vec4(s, 1.0);
                vec2 suv = q.xy / q.w * 0.5 + 0.5;
                float sz = view_pos(suv).z;
                float range = smoothstep(0.0, 1.0, u_p0.x / max(abs(p.z - sz), 1e-3));
                occ += (sz >= s.z + 0.05 ? 1.0 : 0.0) * range;
            }
            c *= 1.0 - clamp(occ / 12.0 * u_p0.y, 0.0, 0.85);
        }
    } else if (u_kind == 9) { // light shafts: strength, length, threshold ; p1 colour ; p2.xy source
        vec2 src2 = u_p2.xy;
        vec2 delta = (v_uv - src2) * u_p0.y / 48.0;
        vec2 uv = v_uv;
        vec3 acc = vec3(0.0);
        float decay = 1.0;
        for (int i = 0; i < 48; i++) {
            uv -= delta;
            vec3 s = texture(u_tex, uv).rgb;
            float bright = max(dot(s, vec3(0.333)) - u_p0.z, 0.0);
            float sky = texture(u_depth, uv).r > 0.9999 ? 1.0 : 0.0;
            acc += s * (bright * 0.6 + sky * 0.15) * decay;
            decay *= 0.96;
        }
        c += acc / 48.0 * u_p0.x * u_p1.rgb * 1.4;
    } else if (u_kind == 10) { // fog: colour p1 ; p0.x start, p0.y density
        float z = lin_depth(v_uv);
        float f = 1.0 - exp(-max(z - u_p0.x, 0.0) * u_p0.y * 0.02);
        if (texture(u_depth, v_uv).r > 0.9999) f *= 0.5;
        c = mix(c, u_p1.rgb, clamp(f, 0.0, 1.0));
    } else if (u_kind == 11) { // outline: thickness, strength ; colour p1
        vec2 t = u_texel * u_p0.x;
        float z = lin_depth(v_uv);
        float e = 0.0;
        e += abs(lin_depth(v_uv + vec2(t.x, 0)) - z);
        e += abs(lin_depth(v_uv - vec2(t.x, 0)) - z);
        e += abs(lin_depth(v_uv + vec2(0, t.y)) - z);
        e += abs(lin_depth(v_uv - vec2(0, t.y)) - z);
        float k = smoothstep(0.02, 0.06, e / max(z, 1.0));
        c = mix(c, u_p1.rgb, k * u_p0.y);
    } else if (u_kind == 12) { // letterbox: aspect
        float screen = u_texel.y / u_texel.x;
        float h = screen / u_p0.x;
        if (h < 1.0 && abs(v_uv.y - 0.5) > h * 0.5) c = vec3(0.0);
    } else if (u_kind == 13) { // posterize: levels, mix
        vec3 q = floor(c * u_p0.x + 0.5) / u_p0.x;
        c = mix(c, q, u_p0.y);
    } else if (u_kind == 15) { // auto exposure: p0 target, min EV, max EV ; tex2 = adapted log2 luminance
        float avg = exp2(texture(u_tex2, vec2(0.5)).r);
        float ev = clamp(log2(u_p0.x / max(avg, 1e-4)), u_p0.y, u_p0.z);
        c *= exp2(ev);
    } else if (u_kind == 16) { // camera motion blur: p0.x shutter, p0.y samples
        float d = texture(u_depth, v_uv).r * 2.0 - 1.0;
        vec4 wp = u_inv_vp * vec4(v_uv * 2.0 - 1.0, d, 1.0);
        wp /= wp.w;
        vec4 pp = u_prev_vp * wp;
        vec2 puv = pp.xy / pp.w * 0.5 + 0.5;
        vec2 vel = (v_uv - puv) * u_p0.x;
        float len = length(vel);
        if (len > 0.06) vel *= 0.06 / len;
        int n = int(u_p0.y);
        vec3 acc = c;
        float wsum = 1.0;
        for (int i = 1; i < 24; i++) {
            if (i >= n) break;
            float t = float(i) / float(max(n - 1, 1)) - 0.5;
            acc += texture(u_tex, v_uv + vel * t).rgb;
            wsum += 1.0;
        }
        c = acc / wsum;
    } else if (u_kind == 17) { // screen-space reflections: p0 strength, max distance, thickness, roughness cut-off
        float d0 = texture(u_depth, v_uv).r;
        vec4 gb = texture(u_gbuf, v_uv);
        float rough = abs(gb.a);
        if (u_has_gbuf == 1 && d0 < 0.9999 && rough < u_p0.w && dot(gb.xyz, gb.xyz) > 0.5) {
            vec3 P = view_pos(v_uv);
            vec3 N = normalize(mat3(u_view) * gb.xyz);
            vec3 Vd = normalize(P);
            vec3 R = normalize(reflect(Vd, N));
            float stepLen = u_p0.y / 40.0;
            vec3 pos = P + N * 0.05 + R * stepLen * hash(v_uv / u_texel);
            vec2 hit = vec2(-1.0);
            for (int i = 0; i < 40; i++) {
                pos += R * stepLen;
                vec4 q = u_proj * vec4(pos, 1.0);
                if (q.w <= 0.0) break;
                vec2 uv = q.xy / q.w * 0.5 + 0.5;
                if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) break;
                float dz = view_pos(uv).z - pos.z;
                if (dz > 0.0 && dz < u_p0.z * (1.0 + float(i) * 0.05)) {
                    vec3 lo = pos - R * stepLen;
                    vec3 hi = pos;
                    for (int j = 0; j < 5; j++) {
                        vec3 m = (lo + hi) * 0.5;
                        vec4 qm = u_proj * vec4(m, 1.0);
                        vec2 um = qm.xy / qm.w * 0.5 + 0.5;
                        if (view_pos(um).z - m.z > 0.0) hi = m; else lo = m;
                    }
                    vec4 qh = u_proj * vec4(hi, 1.0);
                    hit = qh.xy / qh.w * 0.5 + 0.5;
                    break;
                }
                stepLen *= 1.05;
            }
            if (hit.x >= 0.0 && texture(u_depth, hit).r < 0.9999) {
                // blur a little with roughness
                float br = rough * 0.012;
                vec3 rc = texture(u_tex, hit).rgb * 0.4
                        + texture(u_tex, hit + vec2(br, 0.0)).rgb * 0.15 + texture(u_tex, hit - vec2(br, 0.0)).rgb * 0.15
                        + texture(u_tex, hit + vec2(0.0, br)).rgb * 0.15 + texture(u_tex, hit - vec2(0.0, br)).rgb * 0.15;
                vec2 e = smoothstep(vec2(0.0), vec2(0.08), hit) * (1.0 - smoothstep(vec2(0.92), vec2(1.0), hit));
                float edge = e.x * e.y;
                float fres = 0.04 + 0.96 * pow(1.0 - max(dot(-Vd, N), 0.0), 5.0);
                float rk = 1.0 - smoothstep(0.0, u_p0.w, rough);
                if (gb.a < 0.0) {
                    c = mix(c, rc * (0.45 + 0.55 * c / max(dot(c, vec3(0.333)), 0.05)), clamp(edge * rk * u_p0.x * 0.8, 0.0, 1.0));
                } else {
                    c += rc * clamp(edge * rk * fres * u_p0.x, 0.0, 1.0);
                }
            }
        }
    } else if (u_kind == 14) { // mix: mode, factor
        vec3 b = texture(u_tex2, v_uv).rgb;
        int m = int(u_p0.x);
        vec3 r = b;
        if (m == 1) r = c + b; else if (m == 2) r = 1.0 - (1.0 - c) * (1.0 - b); else if (m == 3) r = c * b;
        c = mix(c, r, u_p0.y);
    }
    o = vec4(c, 1.0);
}
"#;

/// Final colour grade: everything ends up in 8-bit display colour.
pub const GRADE_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
uniform int u_enabled;
uniform float u_exposure;
uniform float u_contrast;
uniform float u_saturation;
uniform float u_vibrance;
uniform float u_temperature;
uniform float u_tint;
uniform vec3 u_lift;
uniform vec3 u_gamma;
uniform vec3 u_gain;
uniform float u_shadows;
uniform float u_highlights;
uniform int u_look;
uniform float u_look_k;
uniform int u_split;
uniform float u_split_pos;
uniform vec2 u_texel;
uniform sampler3D u_lut;
uniform int u_lut_on;
uniform float u_lut_k;
uniform float u_lut_n;
uniform vec3 u_lut_min;
uniform vec3 u_lut_max;
out vec4 o;
float luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
vec3 look(vec3 c, int k) {
    float l = luma(c);
    if (k == 1) return pow(c, vec3(0.95, 1.0, 1.1)) * vec3(1.05, 1.0, 0.92) + vec3(0.02, 0.01, 0.0);
    if (k == 2) return mix(c, vec3(l) * vec3(0.75, 0.88, 1.15), 0.45) * 0.95;
    if (k == 3) { vec3 teal = vec3(0.0, 0.45, 0.55); vec3 orange = vec3(1.0, 0.6, 0.3); return mix(c, mix(teal, orange, smoothstep(0.2, 0.8, l)) * l * 1.8, 0.35); }
    if (k == 4) return mix(c, vec3(l), 0.25) * 0.85 + vec3(0.12, 0.1, 0.14);
    if (k == 5) return mix(vec3(l), c, 1.35) * 1.02;
    if (k == 6) return c * 0.86 + vec3(0.07, 0.065, 0.075);
    if (k == 7) return vec3(l);
    if (k == 8) return vec3(l) * vec3(1.07, 0.9, 0.7);
    if (k == 9) return c * vec3(1.1, 0.98, 0.82) + vec3(0.03, 0.015, 0.0);
    return c;
}
void main() {
    vec3 c = clamp(texture(u_tex, v_uv).rgb, 0.0, 64.0);
    bool apply = u_enabled == 1 && !(u_split == 1 && v_uv.x < u_split_pos);
    if (apply) {
        c *= exp2(u_exposure);
        c *= vec3(1.0 + u_temperature * 0.25 + u_tint * 0.08, 1.0 - u_tint * 0.15, 1.0 - u_temperature * 0.25 + u_tint * 0.08);
        c = clamp(c, 0.0, 1.0);
        float l = luma(c);
        c += u_shadows * 0.35 * (1.0 - smoothstep(0.0, 0.5, l)) * (c + 0.02);
        c += u_highlights * 0.35 * smoothstep(0.5, 1.0, l) * c;
        c = (c - 0.5) * u_contrast + 0.5;
        c = clamp(c, 0.0, 1.0);
        vec3 gain = 1.0 + u_gain;
        vec3 lift = u_lift * 0.5;
        c = c * gain + lift * (1.0 - c);
        c = pow(max(c, 0.0), 1.0 / max(1.0 + u_gamma, vec3(0.05)));
        l = luma(c);
        float sat = max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
        c = mix(vec3(l), c, u_saturation * (1.0 + u_vibrance * (1.0 - sat)));
        if (u_look > 0) c = mix(c, look(clamp(c, 0.0, 1.0), u_look), u_look_k);
        if (u_lut_on == 1) {
            vec3 t = clamp((c - u_lut_min) / max(u_lut_max - u_lut_min, vec3(1e-5)), 0.0, 1.0);
            t = t * ((u_lut_n - 1.0) / u_lut_n) + 0.5 / u_lut_n;
            c = mix(c, texture(u_lut, t).rgb, u_lut_k);
        }
    }
    c = clamp(c, 0.0, 1.0);
    if (u_split == 1 && abs(v_uv.x - u_split_pos) < u_texel.x * 1.5) c = vec3(1.0);
    float dither = (fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
    o = vec4(c + dither, 1.0);
}
"#;

/// Log luminance of the picture into a small target (first step of auto exposure).
pub const LUM_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 o;
void main() {
    float l = dot(texture(u_tex, v_uv).rgb, vec3(0.2126, 0.7152, 0.0722));
    o = vec4(log2(max(l, 1e-4)), 0.0, 0.0, 1.0);
}
"#;

/// Average the small log-luminance image and ease toward it over time (1x1 output).
pub const ADAPT_FS: &str = r#"
in vec2 v_uv;
uniform sampler2D u_lum;
uniform sampler2D u_prev;
uniform float u_rate;
uniform int u_first;
out vec4 o;
void main() {
    ivec2 sz = textureSize(u_lum, 0);
    float acc = 0.0;
    float wsum = 0.0;
    for (int y = 0; y < 36; y++) {
        for (int x = 0; x < 64; x++) {
            if (x >= sz.x || y >= sz.y) continue;
            vec2 c = (vec2(x, y) + 0.5) / vec2(sz) - 0.5;
            float w = 1.0 - 0.6 * length(c) * 1.4;
            acc += texelFetch(u_lum, ivec2(x, y), 0).r * w;
            wsum += w;
        }
    }
    float avg = acc / max(wsum, 1e-4);
    float prev = texture(u_prev, vec2(0.5)).r;
    o = vec4(u_first == 1 ? avg : mix(prev, avg, u_rate), 0.0, 0.0, 1.0);
}
"#;
