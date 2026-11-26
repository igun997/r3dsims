//! ASCII-map terrain: each char of `map` is one tile column whose height/color/material comes from
//! `legend`. Row 0 is the back (z = 0), char 0 is x = 0. Top layer gets a grass-lip look, lower
//! layers show strata; tile tops carry a procedural material texture and corner AO.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::mesh::{AO_LEVELS, Quad};
use crate::model::{Rgb, parse_hex};
use crate::texture;

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn jitter() -> f32 {
    0.05
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TerrainDef {
    /// Tile width/depth in world units.
    #[serde(default = "one")]
    pub cell: f32,
    /// Height of one level in world units.
    #[serde(default = "half")]
    pub step: f32,
    pub legend: BTreeMap<String, TileDef>,
    pub map: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TileDef {
    /// Height in levels (1 = lowest).
    pub h: i32,
    /// Top colour; may carry a material: `#78b856:grass`.
    pub top: String,
    /// Side/strata colour; may carry a material (`#8a6040:dirt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default = "jitter")]
    pub jitter: f32,
    #[serde(default)]
    pub water: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub h: i32,
    pub water: bool,
}

pub struct Terrain {
    pub w: usize,
    pub d: usize,
    pub cell: f32,
    pub step: f32,
    pub tiles: Vec<Tile>,
    pub quads: Vec<Quad>,
}

pub(crate) fn hash3(a: i32, b: i32, c: i32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ (c as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 32767.5 - 1.0
}

fn rgbf(c: Rgb, k: f32) -> [f32; 3] {
    [c[0] as f32 / 255.0 * k, c[1] as f32 / 255.0 * k, c[2] as f32 / 255.0 * k]
}

impl Terrain {
    pub fn build(def: &TerrainDef) -> Result<Terrain> {
        let d = def.map.len();
        let w = def.map.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        if w == 0 || d == 0 {
            bail!("terrain map is empty");
        }
        struct Look {
            tile: Tile,
            top: Rgb,
            top_mat: u8,
            side: Rgb,
            side_mat: u8,
            jit: f32,
        }
        let mut looks: BTreeMap<char, Look> = BTreeMap::new();
        for (k, t) in &def.legend {
            let ch = k.chars().next().ok_or_else(|| anyhow!("empty legend key"))?;
            let (top, tf) = parse_hex(&t.top).ok_or_else(|| anyhow!("legend '{k}': bad top colour '{}'", t.top))?;
            let (side, sf) = match &t.side {
                Some(s) => parse_hex(s).ok_or_else(|| anyhow!("legend '{k}': bad side colour '{s}'"))?,
                None => ([118, 82, 56], texture::DIRT << 2),
            };
            looks.insert(ch, Look { tile: Tile { h: t.h.max(1), water: t.water }, top, top_mat: tf >> 2, side, side_mat: sf >> 2, jit: t.jitter });
        }
        let mut tiles = Vec::with_capacity(w * d);
        let mut tile_look: Vec<&Look> = Vec::with_capacity(w * d);
        for (z, row) in def.map.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            for x in 0..w {
                let ch = chars.get(x).copied().unwrap_or(' ');
                let look = looks.get(&ch).ok_or_else(|| anyhow!("map char '{ch}' at x={x} z={z} is not in legend"))?;
                tiles.push(look.tile);
                tile_look.push(look);
            }
        }

        let (cell, step) = (def.cell, def.step);
        let tn = texture::TERRAIN_TEXELS;
        let mut quads = vec![];
        let height = |x: i32, z: i32| -> i32 {
            if x < 0 || z < 0 || x >= w as i32 || z >= d as i32 { 0 } else { tiles[z as usize * w + x as usize].h }
        };
        for z in 0..d as i32 {
            for x in 0..w as i32 {
                let i = z as usize * w + x as usize;
                let (t, look) = (tiles[i], tile_look[i]);
                let (x0, x1, z0, z1) = (x as f32 * cell, (x + 1) as f32 * cell, z as f32 * cell, (z + 1) as f32 * cell);
                let ytop = t.h as f32 * step;
                let jit = 1.0 + look.jit * hash3(x, z, 0);

                // top face: corner AO from higher neighbours, texel uv continuous across tiles
                let corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
                let mut ao = [1.0f32; 4];
                for (ci, (cx, cz)) in corners.iter().enumerate() {
                    let occ = |dx: i32, dz: i32| (height(x + dx, z + dz) > t.h) as i32;
                    let (s1, s2, c) = (occ(*cx, 0), occ(0, *cz), occ(*cx, *cz));
                    let lvl = if s1 == 1 && s2 == 1 { 0 } else { 3 - (s1 + s2 + c) };
                    ao[ci] = AO_LEVELS[lvl as usize];
                }
                let (u0, v0) = (x as f32 * tn, z as f32 * tn);
                quads.push(Quad {
                    p: [Vec3::new(x0, ytop, z0), Vec3::new(x1, ytop, z0), Vec3::new(x1, ytop, z1), Vec3::new(x0, ytop, z1)],
                    n: Vec3::Y,
                    col: rgbf(look.top, jit),
                    id: 0,
                    kind: if t.water { 1 } else { 0 },
                    mat: if t.water { 0 } else { look.top_mat },
                    ao,
                    uv: [[u0, v0], [u0 + tn, v0], [u0 + tn, v0 + tn], [u0, v0 + tn]],
                    seed: 0,
                });

                let sides: [(i32, i32, Vec3); 4] = [(1, 0, Vec3::X), (-1, 0, Vec3::NEG_X), (0, 1, Vec3::Z), (0, -1, Vec3::NEG_Z)];
                for (dx, dz, n) in sides {
                    let nh = height(x + dx, z + dz);
                    for l in nh..t.h {
                        let (y0, y1) = (l as f32 * step, (l + 1) as f32 * step);
                        let p = if dx == 1 {
                            [Vec3::new(x1, y0, z0), Vec3::new(x1, y1, z0), Vec3::new(x1, y1, z1), Vec3::new(x1, y0, z1)]
                        } else if dx == -1 {
                            [Vec3::new(x0, y0, z0), Vec3::new(x0, y1, z0), Vec3::new(x0, y1, z1), Vec3::new(x0, y0, z1)]
                        } else if dz == 1 {
                            [Vec3::new(x0, y0, z1), Vec3::new(x1, y0, z1), Vec3::new(x1, y1, z1), Vec3::new(x0, y1, z1)]
                        } else {
                            [Vec3::new(x0, y0, z0), Vec3::new(x1, y0, z0), Vec3::new(x1, y1, z0), Vec3::new(x0, y1, z0)]
                        };
                        let band = 0.86 + 0.14 * ((l & 1) as f32) + look.jit * hash3(x * 7 + dx, z * 7 + dz, l + 1);
                        let (base, k, kind, mat) = if l == t.h - 1 {
                            (look.top, if t.water { 0.75 } else { 0.82 }, if t.water { 1 } else { 0 }, if t.water { 0 } else { look.top_mat })
                        } else {
                            (look.side, band, 0, look.side_mat)
                        };
                        let uv = p.map(|q| {
                            let h = if dx != 0 { q.z } else { q.x };
                            [h / cell * tn, q.y / step * tn]
                        });
                        // contact darkening along the bottom edge where the face meets lower ground
                        let low = if l == nh { 0.78 } else { 1.0 };
                        let ao = if dx != 0 { [low, 1.0, 1.0, low] } else { [low, low, 1.0, 1.0] };
                        quads.push(Quad {
                            p,
                            n,
                            col: rgbf(base, k),
                            id: 0,
                            kind,
                            mat,
                            ao,
                            uv,
                            seed: (l * 31 + x * 7 + z * 13) as u32,
                        });
                    }
                }
            }
        }
        Ok(Terrain { w, d, cell, step, tiles, quads })
    }

    pub fn tile_at(&self, x: f32, z: f32) -> (usize, usize) {
        let ix = (x / self.cell).floor().clamp(0.0, self.w as f32 - 1.0) as usize;
        let iz = (z / self.cell).floor().clamp(0.0, self.d as f32 - 1.0) as usize;
        (ix, iz)
    }

    /// Ground height (world units) under a world position.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let (ix, iz) = self.tile_at(x, z);
        self.tiles[iz * self.w + ix].h as f32 * self.step
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.w as f32 * self.cell, self.d as f32 * self.cell)
    }

    pub fn center(&self) -> Vec3 {
        let s = self.size();
        Vec3::new(s.x * 0.5, 0.0, s.y * 0.5)
    }

    pub fn radius(&self) -> f32 {
        self.size().length() * 0.5
    }
}
