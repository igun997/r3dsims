// r3dsims GPU renderer. Passes: shadow (depth only), scene (flat voxel shading + procedural
// materials + corner AO + PCF shadows, writes colour and object id), sky, post (pixel outlines +
// label overlay). Everything renders at the low "pixel" resolution; upscaling is nearest.

struct Globals {
    view_proj: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    sun_dir: vec4<f32>,     // direction light travels
    sun_color: vec4<f32>,
    ambient: vec4<f32>,
    ground: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    misc: vec4<f32>,        // time, night glow, ortho (1/0), shadow texel size
    depth: vec4<f32>,       // near, far, width, height
    style: vec4<f32>,       // x = 1 for the clay style
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadow_tex: texture_depth_2d;
@group(0) @binding(2) var shadow_smp: sampler_comparison;

@group(1) @binding(0) var scene_color: texture_2d<f32>;
@group(1) @binding(1) var scene_id: texture_2d<u32>;
@group(1) @binding(2) var scene_depth: texture_depth_2d;
@group(1) @binding(3) var overlay_tex: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) ao: f32,
    @location(2) normal: vec3<f32>,
    @location(3) kind_mat: u32,
    @location(4) color: vec3<f32>,
    @location(5) obj: u32,
    @location(6) uv: vec2<f32>,
    @location(7) seed: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) ao: f32,
    @location(2) normal: vec3<f32>,
    @location(3) @interpolate(flat) kind_mat: u32,
    @location(4) color: vec3<f32>,
    @location(5) @interpolate(flat) obj: u32,
    @location(6) uv: vec2<f32>,
    @location(7) @interpolate(flat) seed: u32,
};

struct FOut {
    @location(0) color: vec4<f32>,
    @location(1) id: u32,
};

@vertex
fn vs_shadow(v: VIn) -> @builtin(position) vec4<f32> {
    return g.light_vp * vec4<f32>(v.pos, 1.0);
}

@vertex
fn vs_scene(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.ao = v.ao;
    o.normal = v.normal;
    o.kind_mat = v.kind_mat;
    o.color = v.color;
    o.obj = v.obj;
    o.uv = v.uv;
    o.seed = v.seed;
    return o;
}

fn hash3(a: i32, b: i32, c: i32) -> f32 {
    var h = bitcast<u32>(a) * 0x9E3779B1u ^ bitcast<u32>(b) * 0x85EBCA77u ^ bitcast<u32>(c) * 0xC2B2AE3Du;
    h = h ^ (h >> 15u);
    h = h * 0x2C1B3C6Du;
    h = h ^ (h >> 12u);
    h = h * 0x297A2D39u;
    h = h ^ (h >> 15u);
    return f32(h & 0xFFFFu) / 32767.5 - 1.0;
}

fn md(a: i32, b: i32) -> i32 {
    return ((a % b) + b) % b;
}

fn fdiv(a: i32, b: i32) -> i32 {
    var q = a / b;
    if ((a % b) != 0 && ((a < 0) != (b < 0))) {
        q = q - 1;
    }
    return q;
}

// Brightness multiplier of one texel of a material (ids match texture.rs).
fn pattern(mat: u32, tx: i32, ty: i32, seed: u32) -> f32 {
    let s = i32(seed & 0xFFFFu);
    switch mat {
        case 1u: { // wood: plank rows with staggered end joints
            let joint = md(tx + ty * 3, 7) == 0;
            let row = 0.94 + 0.06 * hash3(0, ty, s);
            return row * select(1.0, 0.84, joint) + 0.025 * hash3(tx, ty, s ^ 1);
        }
        case 2u: { // brick bond
            let joint = md(tx + (ty & 1) * 2, 4) == 0;
            let brick = 0.92 + 0.1 * hash3(fdiv(tx + (ty & 1) * 2, 4), ty, s);
            return brick * select(1.0, 0.8, joint);
        }
        case 3u: { // tile with grout and a faint checker
            let grout = md(tx, 3) == 0 || md(ty, 3) == 0;
            let check = select(0.95, 1.0, ((fdiv(tx, 3) + fdiv(ty, 3)) & 1) == 0);
            return select(check + 0.02 * hash3(tx, ty, s), 0.8, grout);
        }
        case 4u: { // stone
            return 0.9 + 0.13 * hash3(fdiv(tx, 2), fdiv(ty, 2), s) + 0.03 * hash3(tx, ty, s ^ 2);
        }
        case 5u: { // grass speckle
            let r = hash3(tx, ty, s);
            if (r > 0.55) { return 1.1; }
            if (r < -0.55) { return 0.9; }
            return 1.0 + 0.03 * r;
        }
        case 6u: { return 1.0 + 0.09 * hash3(tx, ty, s); }  // dirt
        case 7u: { // fabric / carpet
            return 1.0 + 0.05 * hash3(tx, ty, s) - select(0.0, 0.03, ((tx + ty) & 3) == 0);
        }
        case 8u: { return 1.0 + 0.02 * hash3(0, ty, s); }   // metal
        case 9u: { return 1.0 + 0.035 * hash3(tx, ty, s); } // plaster
        case 10u: { return 1.0 + 0.06 * hash3(tx, ty, s); } // sand
        default: { return 1.0; }
    }
}

fn shadow_at(p: vec3<f32>) -> f32 {
    let lc = g.light_vp * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(lc.x * 0.5 + 0.5, 0.5 - lc.y * 0.5);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || lc.z > 1.0) {
        return 1.0;
    }
    let texel = g.misc.w;
    var s = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let o = vec2<f32>(f32(dx), f32(dy)) * texel;
            s += textureSampleCompareLevel(shadow_tex, shadow_smp, uv + o, lc.z - 0.0007);
        }
    }
    return s / 9.0;
}

// Cheap smooth wobble in -1..1 per axis (the dimples and fingerprints of hand-made clay).
fn wob(p: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        sin(p.y * 1.7 + sin(p.z * 2.3)),
        sin(p.z * 1.9 + sin(p.x * 2.1)),
        sin(p.x * 1.3 + sin(p.y * 2.7)));
}

// Clay: matte, wrapped soft light, soft shadows, a slightly bumpy surface and a faint warm glow at the terminator.
fn clay_light(base: vec3<f32>, n0: vec3<f32>, p: vec3<f32>, ao: f32) -> vec3<f32> {
    let dn = wob(p * 2.3) * 0.65 + wob(p * 5.1 + vec3<f32>(3.1, 1.7, 5.3)) * 0.35;
    let n = normalize(n0 + dn * 0.12);
    let tint = 1.0 + 0.04 * dn.x;
    let l = normalize(g.sun_dir.xyz);
    let d = dot(n, -l);
    let wrap = clamp(d * 0.5 + 0.5, 0.0, 1.0);
    let soft = mix(max(d, 0.0), wrap * wrap, 0.55);
    let sh = shadow_at(p + n0 * 0.05);
    let sun_term = soft * mix(0.4, 1.0, sh);
    let amb = mix(g.ground.xyz, g.ambient.xyz, 0.5 + 0.5 * n.y);
    let lit = amb * ao * 1.12 + g.sun_color.xyz * sun_term * mix(0.7, 1.0, ao);
    let sss = pow(clamp(1.0 - abs(d), 0.0, 1.0), 3.0) * 0.07;
    return base * tint * (lit + vec3<f32>(sss, sss * 0.65, sss * 0.45) * length(g.sun_color.xyz));
}

@fragment
fn fs_scene(in: VOut) -> FOut {
    let kind = in.kind_mat & 255u;
    let mat = (in.kind_mat >> 8u) & 255u;
    let n = normalize(in.normal);
    var base = in.color;
    if (mat != 0u && kind != 1u && g.style.x < 0.5) {
        let t = floor(in.uv);
        base = base * pattern(mat, i32(t.x), i32(t.y), in.seed);
    }
    var c: vec3<f32>;
    if (kind == 2u) {
        c = base;
    } else if (g.style.x > 0.5 && kind != 1u) {
        c = clay_light(base, n, in.world, in.ao);
        if (kind == 3u) {
            c = mix(c, min(base * 1.1, vec3<f32>(1.0)), g.misc.y);
        }
    } else {
        let l = normalize(g.sun_dir.xyz);
        let ndl = max(dot(n, -l), 0.0);
        var sh = 0.0;
        if (ndl > 0.0) {
            sh = shadow_at(in.world + n * 0.04);
        }
        let amb = mix(g.ground.xyz, g.ambient.xyz, 0.5 + 0.5 * n.y);
        let ao = in.ao;
        let lit = amb * ao + g.sun_color.xyz * (ndl * sh * mix(0.65, 1.0, ao));
        c = base * lit;
        if (kind == 1u) {
            let t = g.misc.x;
            let s = sin(in.world.x * 2.3 + t * 1.4) * sin(in.world.z * 2.1 - t * 1.1);
            c = c * (1.0 + 0.07 * s);
            let sp = sin(in.world.x * 6.7 + t * 2.7) * sin(in.world.z * 5.9 - t * 2.3);
            if (sp > 0.92) {
                c = mix(c, vec3<f32>(1.0), 0.45 * ((sp - 0.92) / 0.08) * min(length(g.sun_color.xyz), 1.0));
            }
        } else if (kind == 3u) {
            c = mix(c, min(base * 1.1, vec3<f32>(1.0)), g.misc.y);
        }
    }
    var o: FOut;
    o.color = vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    o.id = in.obj;
    return o;
}

struct FsIn {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> FsIn {
    var o: FsIn;
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return o;
}

@fragment
fn fs_sky(in: FsIn) -> FOut {
    let u = 1.0 - in.pos.y / g.depth.w;
    var o: FOut;
    o.color = vec4<f32>(mix(g.sky_bottom.xyz, g.sky_top.xyz, clamp(u, 0.0, 1.0)), 1.0);
    o.id = 0xFFFFFFFFu;
    return o;
}

fn linear_depth(z: f32) -> f32 {
    if (g.misc.z > 0.5) {
        return g.depth.x + z * (g.depth.y - g.depth.x);
    }
    return g.depth.x * g.depth.y / (g.depth.y - z * (g.depth.y - g.depth.x));
}

@fragment
fn fs_post(in: FsIn) -> @location(0) vec4<f32> {
    let c = vec2<i32>(in.pos.xy);
    let dims = vec2<i32>(textureDimensions(scene_color));
    var out = textureLoad(scene_color, c, 0).rgb;
    let id_i = textureLoad(scene_id, c, 0).r;
    var d_i = 1.0e9;
    if (id_i != 0xFFFFFFFFu) {
        d_i = linear_depth(textureLoad(scene_depth, c, 0));
    }
    var best = 1.0e9;
    var best_c = vec2<i32>(0);
    for (var k = 0; k < 4; k++) {
        var off = vec2<i32>(1, 0);
        if (k == 1) { off = vec2<i32>(-1, 0); }
        if (k == 2) { off = vec2<i32>(0, 1); }
        if (k == 3) { off = vec2<i32>(0, -1); }
        let nc = c + off;
        if (nc.x < 0 || nc.y < 0 || nc.x >= dims.x || nc.y >= dims.y) { continue; }
        let nid = textureLoad(scene_id, nc, 0).r;
        if (nid == 0xFFFFFFFFu) { continue; }
        let dj = linear_depth(textureLoad(scene_depth, nc, 0));
        if (dj + 0.15 < d_i && (nid != id_i || d_i - dj > 0.9) && dj < best) {
            best = dj;
            best_c = nc;
        }
    }
    if (best < 1.0e8) {
        out = textureLoad(scene_color, best_c, 0).rgb * select(0.38, 0.72, g.style.x > 0.5);
    }
    let ov = textureLoad(overlay_tex, c, 0);
    out = mix(out, ov.rgb, ov.a);
    return vec4<f32>(out, 1.0);
}
