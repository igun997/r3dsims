//! CPU-side geometry for the GPU renderer: voxel faces and terrain tiles become `Quad`s, quads
//! become indexed vertices. Voxel models are meshed per frame (poses move); terrain is built once.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::model::{Model, Pose, Rgb, part_matrices};
use crate::terrain::hash3;

/// Ambient-occlusion brightness for AO level 0 (most occluded) ..= 3 (open).
pub const AO_LEVELS: [f32; 4] = [0.52, 0.70, 0.86, 1.0];

#[derive(Clone, Debug)]
pub struct Quad {
    /// Corners in a cycle; AO and uv are per corner in this order.
    pub p: [Vec3; 4],
    pub n: Vec3,
    pub col: [f32; 3],
    /// 0 = terrain, otherwise entity/building object id (outlines separate objects)
    pub id: u32,
    /// 0 normal, 1 water, 2 always glowing, 3 glows at night
    pub kind: u8,
    /// material id (`texture`)
    pub mat: u8,
    /// brightness per corner (1 = unoccluded)
    pub ao: [f32; 4],
    /// texel coordinates per corner (pattern lookup)
    pub uv: [[f32; 2]; 4],
    pub seed: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub ao: f32,
    pub normal: [f32; 3],
    pub kind_mat: u32,
    pub color: [f32; 3],
    pub obj: u32,
    pub uv: [f32; 2],
    pub seed: u32,
}

#[derive(Default)]
pub struct Mesh {
    pub verts: Vec<Vertex>,
    pub idx: Vec<u32>,
}

impl Mesh {
    pub fn clear(&mut self) {
        self.verts.clear();
        self.idx.clear();
    }

    pub fn from_quads(q: &[Quad]) -> Mesh {
        let mut m = Mesh::default();
        for quad in q {
            m.push(quad);
        }
        m
    }

    pub fn push(&mut self, q: &Quad) {
        let base = self.verts.len() as u32;
        for i in 0..4 {
            self.verts.push(Vertex {
                pos: q.p[i].to_array(),
                ao: q.ao[i],
                normal: q.n.to_array(),
                kind_mat: q.kind as u32 | (q.mat as u32) << 8,
                color: q.col,
                obj: q.id,
                uv: q.uv[i],
                seed: q.seed,
            });
        }
        // split along the diagonal that hides AO anisotropy
        if q.ao[0] + q.ao[2] >= q.ao[1] + q.ao[3] {
            self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            self.idx.extend([base + 1, base + 2, base + 3, base + 1, base + 3, base]);
        }
    }
}

/// One voxel model placed in the world, with the per-frame view state buildings need.
pub struct Placed<'a> {
    pub model: &'a Model,
    pub colors: &'a [Rgb],
    pub root: Mat4,
    pub pose: &'a [Pose],
    pub id: u32,
    /// Parts with an index >= this are not drawn (floors above the visible one).
    pub parts_shown: usize,
    /// Bits 0..4 = sides +x -x +z -z that face the camera (their cut walls are lowered).
    pub facing: u8,
    /// Lower interior walls too.
    pub lower_interior: bool,
    /// Master switch for wall cut-away.
    pub cutaway: bool,
}

const AX: [Vec3; 6] = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
const CORNERS: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

pub fn push_model(m: &Placed, mats: &mut Vec<Mat4>, mesh: &mut Mesh) {
    let model = m.model;
    part_matrices(model, m.pose, m.root, mats);
    for (pi, part) in model.parts.iter().enumerate() {
        if pi >= m.parts_shown {
            continue;
        }
        let mat = mats[pi];
        let normals: [Vec3; 6] = AX.map(|a| mat.transform_vector3(a).normalize());
        for v in &part.voxels {
            let mut f = v.f;
            if m.cutaway && v.cut & 0x80 != 0 && part.cut_y != i16::MAX {
                let lowered = (v.cut & 0x10 != 0 && m.lower_interior) || (v.cut & m.facing & 0x0f) != 0;
                if lowered {
                    if v.p[1] >= part.cut_y {
                        continue;
                    }
                    if v.p[1] == part.cut_y - 1 {
                        f |= 1 << 2;
                    }
                }
            }
            let flags = model.glow[v.c as usize];
            let kind = match flags & 3 {
                1 => 2,
                2 => 3,
                _ => 0,
            };
            let rgb = m.colors[v.c as usize];
            let jit = if kind == 0 { 1.0 + 0.03 * hash3(v.p[0] as i32, v.p[1] as i32, v.p[2] as i32 + pi as i32 * 31) } else { 1.0 };
            let col = [rgb[0] as f32 / 255.0 * jit, rgb[1] as f32 / 255.0 * jit, rgb[2] as f32 / 255.0 * jit];
            let c = Vec3::new(v.p[0] as f32 + 0.5, v.p[1] as f32 + 0.5, v.p[2] as f32 + 0.5) - part.pivot;
            for k in 0..6 {
                if f & (1 << k) == 0 {
                    continue;
                }
                let (a, s) = (k / 2, if k % 2 == 0 { 0.5 } else { -0.5 });
                let (t1, t2) = ((a + 1) % 3, (a + 2) % 3);
                let mut quad = Quad {
                    p: [Vec3::ZERO; 4],
                    n: normals[k],
                    col,
                    id: m.id,
                    kind,
                    mat: flags >> 2,
                    ao: [1.0; 4],
                    uv: [[0.0; 2]; 4],
                    seed: pi as u32 * 131,
                };
                for (ci, (sx, sy)) in CORNERS.iter().enumerate() {
                    let mut l = c;
                    l[a] += s;
                    l[t1] += 0.5 * sx;
                    l[t2] += 0.5 * sy;
                    quad.p[ci] = mat.transform_point3(l);
                    quad.ao[ci] = AO_LEVELS[((v.ao[k] >> (ci * 2)) & 3) as usize];
                    let mut g = [v.p[0] as f32, v.p[1] as f32, v.p[2] as f32];
                    g[t1] += if *sx > 0.0 { 1.0 } else { 0.0 };
                    g[t2] += if *sy > 0.0 { 1.0 } else { 0.0 };
                    quad.uv[ci] = match a {
                        0 => [g[2], g[1]],
                        1 => [g[0], g[2]],
                        _ => [g[0], g[1]],
                    };
                }
                mesh.push(&quad);
            }
        }
    }
}

/// Which sides of an axis-aligned building face a camera at `yaw` degrees (bits +x -x +z -z).
pub fn facing_sides(yaw: f32) -> u8 {
    let y = yaw.to_radians();
    let (ex, ez) = (y.sin(), y.cos());
    let mut m = 0;
    if ex > 0.25 {
        m |= 1;
    }
    if ex < -0.25 {
        m |= 2;
    }
    if ez > 0.25 {
        m |= 4;
    }
    if ez < -0.25 {
        m |= 8;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_is_tightly_packed() {
        assert_eq!(std::mem::size_of::<Vertex>(), 60);
    }

    #[test]
    fn camera_at_45_degrees_sees_plus_x_and_plus_z() {
        assert_eq!(facing_sides(45.0), 1 | 4);
        assert_eq!(facing_sides(225.0), 2 | 8);
        assert_eq!(facing_sides(0.0), 4);
    }
}
