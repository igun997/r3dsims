//! Clay style: the voxel parts of a model turned into one smooth, soft-edged surface (blurred occupancy field,
//! naive surface nets, gradient normals, per-vertex palette index). The voxels stay the authoring format, so every
//! asset (units, castles, trees, props, an agent's own) can be drawn as hand-made clay figures without being
//! redrawn. A part is meshed once, in its own local space; the renderer only transforms the vertices per frame.

use glam::Vec3;

use crate::mesh::{Mesh, Vertex};
use crate::model::Part;
use crate::terrain::Terrain;

#[derive(Clone, Copy, Debug)]
pub struct ClayVert {
    /// part-local position in voxel units, relative to the part pivot (the same space voxel quads use)
    pub p: Vec3,
    pub n: Vec3,
    /// palette index of the nearest voxel, so recolouring an entity recolours its clay
    pub c: u8,
    /// cavities are a little darker
    pub ao: f32,
}

#[derive(Debug, Default)]
pub struct ClayMesh {
    pub verts: Vec<ClayVert>,
    /// triangle list
    pub idx: Vec<u32>,
}

/// Where the blurred field becomes surface: a little below one half so that thin pieces (sticks, antennas) survive.
const ISO: f32 = 0.33;
/// Parts with more voxels than this are meshed at the voxel grid itself; smaller ones at twice that resolution.
const FINE_LIMIT: usize = 3000;

struct Grid {
    d: [usize; 3],
    v: Vec<f32>,
}

impl Grid {
    fn new(d: [usize; 3]) -> Grid {
        Grid { d, v: vec![0.0; d[0] * d[1] * d[2]] }
    }

    fn ix(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.d[1] + y) * self.d[0] + x
    }

    fn at(&self, x: usize, y: usize, z: usize) -> f32 {
        self.v[self.ix(x, y, z)]
    }

    /// Trilinear sample at a position in node coordinates, clamped to the grid.
    fn sample(&self, p: Vec3) -> f32 {
        let c = |v: f32, n: usize| v.clamp(0.0, (n - 1) as f32 - 1e-4);
        let (x, y, z) = (c(p.x, self.d[0]), c(p.y, self.d[1]), c(p.z, self.d[2]));
        let (x0, y0, z0) = (x as usize, y as usize, z as usize);
        let (fx, fy, fz) = (x - x0 as f32, y - y0 as f32, z - z0 as f32);
        let mut s = 0.0;
        for k in 0..8 {
            let (dx, dy, dz) = (k & 1, (k >> 1) & 1, (k >> 2) & 1);
            let w = (if dx == 1 { fx } else { 1.0 - fx }) * (if dy == 1 { fy } else { 1.0 - fy }) * (if dz == 1 { fz } else { 1.0 - fz });
            s += w * self.at(x0 + dx, y0 + dy, z0 + dz);
        }
        s
    }

    /// One separable pass of the kernel `[k, 1 - 2k, k]` along every axis.
    fn blur(&self, k: f32) -> Grid {
        let mut cur = Grid { d: self.d, v: self.v.clone() };
        for axis in 0..3 {
            let mut out = Grid::new(self.d);
            for z in 0..self.d[2] {
                for y in 0..self.d[1] {
                    for x in 0..self.d[0] {
                        let p = [x, y, z];
                        let at = |o: i32| {
                            let mut q = p;
                            let n = q[axis] as i32 + o;
                            if n < 0 || n >= self.d[axis] as i32 {
                                return 0.0;
                            }
                            q[axis] = n as usize;
                            cur.at(q[0], q[1], q[2])
                        };
                        let v = k * at(-1) + (1.0 - 2.0 * k) * at(0) + k * at(1);
                        let i = out.ix(x, y, z);
                        out.v[i] = v;
                    }
                }
            }
            cur = out;
        }
        cur
    }

    /// Twice the node density (nodes at half steps), trilinearly interpolated.
    fn upsample2(&self) -> Grid {
        let d = [(self.d[0] - 1) * 2 + 1, (self.d[1] - 1) * 2 + 1, (self.d[2] - 1) * 2 + 1];
        let mut out = Grid::new(d);
        for z in 0..d[2] {
            for y in 0..d[1] {
                for x in 0..d[0] {
                    let v = self.sample(Vec3::new(x as f32 * 0.5, y as f32 * 0.5, z as f32 * 0.5));
                    let i = out.ix(x, y, z);
                    out.v[i] = v;
                }
            }
        }
        out
    }
}

/// Corner pairs of a cube's 12 edges; corner `i` sits at `(i & 1, (i >> 1) & 1, (i >> 2) & 1)`.
const EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

pub fn build(part: &Part) -> ClayMesh {
    let cells: Vec<([i32; 3], u8)> = part.voxels.iter().map(|v| ([v.p[0] as i32, v.p[1] as i32, v.p[2] as i32], v.c)).collect();
    build_cells(&cells, part.pivot, part.voxels.len() <= FINE_LIMIT)
}

/// Smooth surface of a set of coloured cells (`[x, y, z]`, palette index). Hollow shells are filled first, so a model
/// that only stores its exposed voxels still becomes a solid lump. `fine` meshes at twice the cell resolution.
pub fn build_cells(cells: &[([i32; 3], u8)], pivot: Vec3, fine: bool) -> ClayMesh {
    if cells.is_empty() {
        return ClayMesh::default();
    }
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for (p, _) in cells {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let pad = 3i32;
    let d = [(hi[0] - lo[0] + 1 + 2 * pad) as usize, (hi[1] - lo[1] + 1 + 2 * pad) as usize, (hi[2] - lo[2] + 1 + 2 * pad) as usize];
    let mut occ = Grid::new(d);
    let mut col = vec![0u8; occ.v.len()];
    let mut orig = vec![false; occ.v.len()];
    for (p, c) in cells {
        let (x, y, z) = ((p[0] - lo[0] + pad) as usize, (p[1] - lo[1] + pad) as usize, (p[2] - lo[2] + pad) as usize);
        let i = occ.ix(x, y, z);
        occ.v[i] = 1.0;
        col[i] = *c;
        orig[i] = true;
    }
    // everything the outside cannot reach is inside the lump
    let mut outside = vec![false; occ.v.len()];
    let mut stack = vec![[0usize, 0, 0]];
    outside[0] = true;
    while let Some([x, y, z]) = stack.pop() {
        for (dx, dy, dz) in [(1i32, 0i32, 0i32), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
            if nx < 0 || ny < 0 || nz < 0 || nx >= d[0] as i32 || ny >= d[1] as i32 || nz >= d[2] as i32 {
                continue;
            }
            let i = occ.ix(nx as usize, ny as usize, nz as usize);
            if !outside[i] && !orig[i] {
                outside[i] = true;
                stack.push([nx as usize, ny as usize, nz as usize]);
            }
        }
    }
    for i in 0..occ.v.len() {
        if !outside[i] {
            occ.v[i] = 1.0;
        }
    }
    let coarse = occ.blur(0.2);
    let (field, step) = if fine { (coarse.upsample2(), 0.5) } else { (coarse, 1.0) };

    // node coordinates of a field node -> 1x grid coordinates (the occupancy grid)
    let to_q = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * step;
    // 1x grid coordinates -> part-local voxel units (voxel centres sit at +0.5)
    let off = Vec3::new(lo[0] as f32 - pad as f32 + 0.5, lo[1] as f32 - pad as f32 + 0.5, lo[2] as f32 - pad as f32 + 0.5) - pivot;

    let [nx, ny, nz] = field.d;
    let cd = [nx - 1, ny - 1, nz - 1];
    let mut cell = vec![u32::MAX; cd[0] * cd[1] * cd[2]];
    let cix = |x: usize, y: usize, z: usize| (z * cd[1] + y) * cd[0] + x;
    let mut verts: Vec<ClayVert> = vec![];
    for z in 0..cd[2] {
        for y in 0..cd[1] {
            for x in 0..cd[0] {
                let mut vals = [0f32; 8];
                let mut mask = 0u8;
                for (i, v) in vals.iter_mut().enumerate() {
                    *v = field.at(x + (i & 1), y + ((i >> 1) & 1), z + ((i >> 2) & 1));
                    if *v >= ISO {
                        mask |= 1 << i;
                    }
                }
                if mask == 0 || mask == 255 {
                    continue;
                }
                let corner = |i: usize| Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32);
                let (mut sum, mut cnt) = (Vec3::ZERO, 0.0);
                for &(a, b) in &EDGES {
                    if ((mask >> a) & 1) != ((mask >> b) & 1) {
                        let t = (ISO - vals[a]) / (vals[b] - vals[a]);
                        sum += corner(a) + (corner(b) - corner(a)) * t;
                        cnt += 1.0;
                    }
                }
                let pn = Vec3::new(x as f32, y as f32, z as f32) + sum / cnt;
                let q = to_q(pn.x, pn.y, pn.z);
                cell[cix(x, y, z)] = verts.len() as u32;
                // normal: against the field gradient
                let h = 1.0;
                let g = Vec3::new(
                    field.sample(pn + Vec3::X * h) - field.sample(pn - Vec3::X * h),
                    field.sample(pn + Vec3::Y * h) - field.sample(pn - Vec3::Y * h),
                    field.sample(pn + Vec3::Z * h) - field.sample(pn - Vec3::Z * h),
                );
                let n = (-g).try_normalize().unwrap_or(Vec3::Y);
                // nearest solid voxel gives the colour; the fraction of solid in a 5x5x5 box gives the cavity darkening
                let (cx, cy, cz) = (q.x.round() as i32, q.y.round() as i32, q.z.round() as i32);
                let (mut best, mut best_d, mut solid, mut total) = (0u8, f32::MAX, 0.0f32, 0.0f32);
                for dz in -2..=2 {
                    for dy in -2..=2 {
                        for dx in -2..=2 {
                            let (ax, ay, az) = (cx + dx, cy + dy, cz + dz);
                            if ax < 0 || ay < 0 || az < 0 || ax >= d[0] as i32 || ay >= d[1] as i32 || az >= d[2] as i32 {
                                total += 1.0;
                                continue;
                            }
                            let i = occ.ix(ax as usize, ay as usize, az as usize);
                            total += 1.0;
                            if occ.v[i] > 0.5 {
                                solid += 1.0;
                            }
                            if orig[i] {
                                let dd = (Vec3::new(ax as f32, ay as f32, az as f32) - q).length_squared();
                                if dd < best_d {
                                    best_d = dd;
                                    best = col[i];
                                }
                            }
                        }
                    }
                }
                let frac = solid / total;
                let ao = (1.0 - (frac - 0.45).max(0.0) * 1.3).clamp(0.55, 1.0);
                verts.push(ClayVert { p: q + off, n, c: best, ao });
            }
        }
    }

    // one quad per grid edge that crosses the surface, joining the four cells around it
    let mut idx: Vec<u32> = vec![];
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let inside = field.at(x, y, z) >= ISO;
                for axis in 0..3 {
                    let mut n1 = [x, y, z];
                    n1[axis] += 1;
                    if n1[axis] >= field.d[axis] {
                        continue;
                    }
                    if (field.at(n1[0], n1[1], n1[2]) >= ISO) == inside {
                        continue;
                    }
                    let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
                    let p = [x, y, z];
                    if p[b] == 0 || p[c] == 0 {
                        continue;
                    }
                    let mut q = [0u32; 4];
                    let mut ok = true;
                    for (k, (db, dc)) in [(1, 1), (0, 1), (0, 0), (1, 0)].into_iter().enumerate() {
                        let mut cc = p;
                        cc[b] = p[b] - db;
                        cc[c] = p[c] - dc;
                        if cc[0] >= cd[0] || cc[1] >= cd[1] || cc[2] >= cd[2] {
                            ok = false;
                            break;
                        }
                        let v = cell[cix(cc[0], cc[1], cc[2])];
                        if v == u32::MAX {
                            ok = false;
                            break;
                        }
                        q[k] = v;
                    }
                    if !ok {
                        continue;
                    }
                    // wind the quad so its geometric normal agrees with the field gradient
                    let (a0, a1, a2) = (verts[q[0] as usize].p, verts[q[1] as usize].p, verts[q[2] as usize].p);
                    let geo = (a1 - a0).cross(a2 - a0);
                    let want = verts[q[0] as usize].n + verts[q[1] as usize].n + verts[q[2] as usize].n + verts[q[3] as usize].n;
                    if geo.dot(want) >= 0.0 {
                        idx.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
                    } else {
                        idx.extend([q[0], q[2], q[1], q[0], q[3], q[2]]);
                    }
                }
            }
        }
    }
    ClayMesh { verts, idx }
}

/// The ground as clay: every tile column becomes a stack of cells (the top one in the tile's top colour, the rest in
/// its side colour), smoothed like any model, so cliffs and the island rim are rounded instead of stepped.
pub fn terrain_mesh(t: &Terrain) -> Mesh {
    let per = ((t.cell / t.step).round() as i32).max(1);
    let vs = t.cell / per as f32;
    let mut pal: Vec<([u8; 3], bool)> = vec![];
    let mut intern = |c: [u8; 3], water: bool| -> u8 {
        if let Some(i) = pal.iter().position(|e| *e == (c, water)) {
            return i as u8;
        }
        if pal.len() < 255 {
            pal.push((c, water));
            (pal.len() - 1) as u8
        } else {
            0
        }
    };
    let mut cells: Vec<([i32; 3], u8)> = vec![];
    for z in 0..t.d {
        for x in 0..t.w {
            let (tile, st) = (t.tiles[z * t.w + x], t.styles[z * t.w + x]);
            let (top, side) = (intern(st.top, st.water), intern(st.side, false));
            for l in 0..tile.h {
                let c = if l == tile.h - 1 { top } else { side };
                for sz in 0..per {
                    for sx in 0..per {
                        cells.push(([x as i32 * per + sx, l, z as i32 * per + sz], c));
                    }
                }
            }
        }
    }
    let cm = build_cells(&cells, Vec3::ZERO, false);
    let sc = Vec3::new(vs, t.step, vs);
    // the smoothing puffs the surface up a quarter cell: sink it so flat tops sit at the tile height
    let lift = Vec3::new(0.0, -0.25 * t.step, 0.0);
    let mut mesh = Mesh::default();
    for v in &cm.verts {
        let (rgb, water) = pal[v.c as usize];
        mesh.verts.push(Vertex {
            pos: (v.p * sc + lift).to_array(),
            ao: v.ao,
            normal: (v.n / sc).normalize_or_zero().to_array(),
            kind_mat: if water { 1 } else { 0 },
            color: [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0],
            obj: 0,
            uv: [0.0; 2],
            seed: 0,
        });
    }
    mesh.idx = cm.idx;
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Voxel;

    fn cube(n: i16) -> Part {
        let mut voxels = vec![];
        for x in 0..n {
            for y in 0..n {
                for z in 0..n {
                    voxels.push(Voxel { p: [x, y, z], c: 3, f: 0, ao: [3; 6], cut: 0 });
                }
            }
        }
        Part { name: "cube".into(), parent: None, pivot: Vec3::splat(n as f32 / 2.0), voxels, cut_y: i16::MAX, clay: Default::default() }
    }

    #[test]
    fn a_cube_becomes_a_closed_soft_surface_around_the_same_extent() {
        let m = build(&cube(6));
        assert!(m.verts.len() > 100 && m.idx.len() > 300, "{} verts {} idx", m.verts.len(), m.idx.len());
        assert!(m.idx.iter().all(|&i| (i as usize) < m.verts.len()));
        // extent about 6 voxels (rounded corners shrink it a little), centred on the pivot
        let (lo, hi) = m.verts.iter().fold((Vec3::splat(9.0), Vec3::splat(-9.0)), |(l, h), v| (l.min(v.p), h.max(v.p)));
        assert!(lo.x > -3.6 && lo.x < -2.4 && hi.x < 3.6 && hi.x > 2.4, "{lo:?} {hi:?}");
        // normals are unit length and point away from the centre on the face middles
        assert!(m.verts.iter().all(|v| (v.n.length() - 1.0).abs() < 1e-3));
        let top = m.verts.iter().filter(|v| v.p.y > 2.5 && v.p.x.abs() < 1.0 && v.p.z.abs() < 1.0).collect::<Vec<_>>();
        assert!(!top.is_empty() && top.iter().all(|v| v.n.y > 0.8), "top face normals point up");
        assert!(m.verts.iter().all(|v| v.c == 3));
    }

    #[test]
    fn a_thin_stick_survives_the_smoothing() {
        let voxels: Vec<Voxel> = (0..10).map(|y| Voxel { p: [0, y, 0], c: 1, f: 0, ao: [3; 6], cut: 0 }).collect();
        let part = Part { name: "stick".into(), parent: None, pivot: Vec3::ZERO, voxels, cut_y: i16::MAX, clay: Default::default() };
        let m = build(&part);
        assert!(m.verts.len() > 30, "a one-voxel stick should still have a surface: {} verts", m.verts.len());
    }

    #[test]
    fn a_hollow_shell_becomes_one_solid_surface() {
        // only the exposed voxels of a 7-cube, as a baked model stores them
        let mut voxels = vec![];
        for x in 0..7i16 {
            for y in 0..7i16 {
                for z in 0..7i16 {
                    if [x, y, z].iter().any(|&v| v == 0 || v == 6) {
                        voxels.push(Voxel { p: [x, y, z], c: 2, f: 1, ao: [3; 6], cut: 0 });
                    }
                }
            }
        }
        let part = Part { name: "shell".into(), parent: None, pivot: Vec3::splat(3.5), voxels, cut_y: i16::MAX, clay: Default::default() };
        let m = build(&part);
        // no inner surface: every vertex lies near the outside of the cube, none deep inside it
        let deepest = m.verts.iter().map(|v| v.p.abs().max_element()).fold(f32::MAX, f32::min);
        assert!(deepest > 2.6, "found an inner surface at {deepest}");
    }

    #[test]
    fn the_clay_ground_sits_at_the_tile_height_and_keeps_water_apart() {
        use crate::terrain::{Terrain, TerrainDef, TileDef};
        let tile = |h: i32, top: &str, water: bool| TileDef { h, top: top.into(), side: None, jitter: 0.0, water };
        let def = TerrainDef {
            cell: 1.0,
            step: 0.5,
            legend: [("g".to_string(), tile(3, "#40a040", false)), ("w".to_string(), tile(1, "#3060d0", true))].into_iter().collect(),
            map: vec!["wwwwwww".into(), "wgggggw".into(), "wgggggw".into(), "wgggggw".into(), "wwwwwww".into()],
        };
        let t = Terrain::build(&def).unwrap();
        let m = terrain_mesh(&t);
        assert!(m.verts.len() > 200 && m.idx.len() % 3 == 0);
        // over the middle of the island the top is at 3 levels * 0.5 = 1.5
        let top: Vec<f32> = m.verts.iter().filter(|v| (v.pos[0] - 3.5).abs() < 0.6 && (v.pos[2] - 2.5).abs() < 0.6 && v.normal[1] > 0.9).map(|v| v.pos[1]).collect();
        assert!(!top.is_empty() && top.iter().all(|y| (y - 1.5).abs() < 0.12), "island top heights {top:?}");
        assert!(m.verts.iter().any(|v| v.kind_mat == 1), "the sea keeps its water kind");
        assert!(m.verts.iter().any(|v| v.kind_mat == 0 && v.color[1] > v.color[2]), "the land is green clay");
    }
}
