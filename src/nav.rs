//! Pathfinding over several walkable layers: the terrain (layer 0) and every floor of every
//! building. Each layer is a tile grid; stairs and doors link layers. A* (8-neighbour, no corner
//! cutting) with line-of-sight smoothing inside a layer. Water, cliffs (> 2 levels), walls and
//! entity footprints block; stairs can only be entered from their low end and left at the top.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use glam::Vec2;

use crate::building::{Building, Kind, layer_id};
use crate::terrain::Terrain;
use crate::world::Entity;

/// A position on a layer.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub pos: Vec2,
    pub layer: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct PathPt {
    pub pos: Vec2,
    pub layer: u16,
}

struct Grid {
    ox: f32,
    oz: f32,
    cell: f32,
    w: i32,
    d: i32,
    blocked: Vec<bool>,
    /// terrain levels, for the cliff rule (empty on building floors)
    heights: Vec<i32>,
    /// stair ascent direction per tile
    stair: Vec<Option<(i32, i32)>>,
    /// tiles that stairs run through, top tile of a run
    offset: usize,
}

pub struct Nav {
    grids: Vec<Option<Grid>>,
    nodes: Vec<(u16, i16, i16)>,
    links: HashMap<usize, Vec<usize>>,
}

impl Grid {
    fn idx(&self, x: i32, z: i32) -> Option<usize> {
        (x >= 0 && z >= 0 && x < self.w && z < self.d).then(|| (z * self.w + x) as usize)
    }
    fn cell_of(&self, p: Vec2) -> (i32, i32) {
        (
            (((p.x - self.ox) / self.cell).floor() as i32).clamp(0, self.w - 1),
            (((p.y - self.oz) / self.cell).floor() as i32).clamp(0, self.d - 1),
        )
    }
    fn center(&self, c: (i32, i32)) -> Vec2 {
        Vec2::new(self.ox + (c.0 as f32 + 0.5) * self.cell, self.oz + (c.1 as f32 + 0.5) * self.cell)
    }
    fn free(&self, c: (i32, i32)) -> bool {
        self.idx(c.0, c.1).is_some_and(|i| !self.blocked[i])
    }
    fn stair_at(&self, c: (i32, i32)) -> Option<(i32, i32)> {
        self.idx(c.0, c.1).and_then(|i| self.stair[i])
    }
    /// May an entity step from `a` to the adjacent tile `b` (same layer)?
    fn step_ok(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        let (Some(i), Some(j)) = (self.idx(a.0, a.1), self.idx(b.0, b.1)) else { return false };
        if !self.heights.is_empty() && (self.heights[i] - self.heights[j]).abs() > 2 {
            return false;
        }
        let (sa, sb) = (self.stair[i], self.stair[j]);
        let delta = (b.0 - a.0, b.1 - a.1);
        match (sa, sb) {
            (None, None) => true,
            (Some(d), Some(e)) => d == e && (delta == d || delta == (-d.0, -d.1)),
            // stairs -> plain tile: only off the low end
            (Some(d), None) => delta == (-d.0, -d.1),
            // plain tile -> stairs: only onto the low end
            (None, Some(e)) => delta == e,
        }
    }
    fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let start = self.cell_of(a);
        let steps = ((b - a).length() / 0.2).ceil().max(1.0) as i32;
        let mut prev = start;
        for k in 1..=steps {
            let c = self.cell_of(a.lerp(b, k as f32 / steps as f32));
            if c == prev {
                continue;
            }
            if (!self.free(c) && c != start) || self.stair_at(c).is_some() || self.stair_at(prev).is_some() || !self.step_ok(prev, c) {
                return false;
            }
            prev = c;
        }
        true
    }
}

fn mark(g: &mut Grid, x0: f32, z0: f32, x1: f32, z1: f32) {
    let (a, b) = (g.cell_of(Vec2::new(x0, z0)), g.cell_of(Vec2::new(x1, z1)));
    for cz in a.1..=b.1 {
        for cx in a.0..=b.0 {
            if let Some(i) = g.idx(cx, cz) {
                g.blocked[i] = true;
            }
        }
    }
}

impl Nav {
    pub fn build(t: &Terrain, buildings: &[Building], ents: &[Entity]) -> Nav {
        let nlayers = 1 + buildings.len() * 8;
        let mut grids: Vec<Option<Grid>> = (0..nlayers).map(|_| None).collect();
        let mut g0 = Grid {
            ox: 0.0,
            oz: 0.0,
            cell: t.cell,
            w: t.w as i32,
            d: t.d as i32,
            blocked: t.tiles.iter().map(|x| x.water).collect(),
            heights: t.tiles.iter().map(|x| x.h).collect(),
            stair: vec![None; t.w * t.d],
            offset: 0,
        };
        // buildings block the terrain under their footprint
        for b in buildings {
            for cz in 0..b.rows {
                for cx in 0..b.cols {
                    if (0..b.floors()).any(|f| b.kind_at(f, cx, cz) != Kind::Void) {
                        let (x0, z0) = (b.origin.x + cx as f32 * b.cell, b.origin.z + cz as f32 * b.cell);
                        mark(&mut g0, x0 + 0.05, z0 + 0.05, x0 + b.cell - 0.05, z0 + b.cell - 0.05);
                    }
                }
            }
        }
        grids[0] = Some(g0);
        for (bi, b) in buildings.iter().enumerate() {
            for f in 0..b.floors() {
                let mut blocked = vec![true; (b.cols * b.rows) as usize];
                let mut stair = vec![None; (b.cols * b.rows) as usize];
                for cz in 0..b.rows {
                    for cx in 0..b.cols {
                        let i = (cz * b.cols + cx) as usize;
                        blocked[i] = !b.kind_at(f, cx, cz).walkable();
                        stair[i] = b.grids[f].stair[i].map(|s| s.dir);
                    }
                }
                grids[layer_id(bi, f) as usize] = Some(Grid {
                    ox: b.origin.x,
                    oz: b.origin.z,
                    cell: b.cell,
                    w: b.cols,
                    d: b.rows,
                    blocked,
                    heights: vec![],
                    stair,
                    offset: 0,
                });
            }
        }
        // entity footprints block the layer they stand on
        for e in ents {
            let [fw, fd] = e.footprint;
            if fw <= 0.0 || fd <= 0.0 {
                continue;
            }
            if let Some(Some(g)) = grids.get_mut(e.layer as usize) {
                mark(g, e.pos.x - fw * 0.5 + 0.05, e.pos.z - fd * 0.5 + 0.05, e.pos.x + fw * 0.5 - 0.05, e.pos.z + fd * 0.5 - 0.05);
            }
        }

        let mut nodes = vec![];
        for (l, g) in grids.iter_mut().enumerate() {
            if let Some(g) = g {
                g.offset = nodes.len();
                for z in 0..g.d {
                    for x in 0..g.w {
                        nodes.push((l as u16, x as i16, z as i16));
                    }
                }
            }
        }
        let mut nav = Nav { grids, nodes, links: HashMap::new() };

        // links: stair tops to the next floor's landing, ground-floor doors to the terrain
        for (bi, b) in buildings.iter().enumerate() {
            for f in 0..b.floors() {
                let lf = layer_id(bi, f);
                for cz in 0..b.rows {
                    for cx in 0..b.cols {
                        let i = (cz * b.cols + cx) as usize;
                        if let Some(s) = b.grids[f].stair[i]
                            && s.top
                            && f + 1 < b.floors()
                        {
                            let (lx, lz) = (cx + s.dir.0, cz + s.dir.1);
                            if b.kind_at(f + 1, lx, lz).walkable() {
                                nav.link(lf, (cx, cz), layer_id(bi, f + 1), (lx, lz));
                            }
                        }
                        if f == 0 && b.kind_at(0, cx, cz) == Kind::Door {
                            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                                if b.kind_at(0, cx + dx, cz + dz) == Kind::Void {
                                    let wx = b.origin.x + (cx as f32 + 0.5) * b.cell + dx as f32 * (b.cell * 0.5 + 0.5);
                                    let wz = b.origin.z + (cz as f32 + 0.5) * b.cell + dz as f32 * (b.cell * 0.5 + 0.5);
                                    let outside = nav.grids[0].as_ref().map(|g| g.cell_of(Vec2::new(wx, wz)));
                                    if let Some(oc) = outside
                                        && nav.grids[0].as_ref().is_some_and(|g| g.free(oc))
                                    {
                                        nav.link(lf, (cx, cz), 0, oc);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        nav
    }

    fn node(&self, layer: u16, c: (i32, i32)) -> Option<usize> {
        let g = self.grids.get(layer as usize)?.as_ref()?;
        g.idx(c.0, c.1).map(|i| g.offset + i)
    }

    fn link(&mut self, la: u16, ca: (i32, i32), lb: u16, cb: (i32, i32)) {
        if let (Some(a), Some(b)) = (self.node(la, ca), self.node(lb, cb)) {
            self.links.entry(a).or_default().push(b);
            self.links.entry(b).or_default().push(a);
        }
    }

    fn pos_of(&self, n: usize) -> Vec2 {
        let (l, x, z) = self.nodes[n];
        self.grids[l as usize].as_ref().unwrap().center((x as i32, z as i32))
    }

    fn grid(&self, layer: u16) -> Option<&Grid> {
        self.grids.get(layer as usize)?.as_ref()
    }

    fn neighbors(&self, n: usize, buf: &mut Vec<usize>) {
        buf.clear();
        let (l, x, z) = self.nodes[n];
        let g = self.grids[l as usize].as_ref().unwrap();
        let cur = (x as i32, z as i32);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nb = (cur.0 + dx, cur.1 + dz);
                if !g.free(nb) || !g.step_ok(cur, nb) {
                    continue;
                }
                if dx != 0 && dz != 0 && (!g.free((cur.0 + dx, cur.1)) || !g.free((cur.0, cur.1 + dz)) || g.stair_at(cur).is_some()) {
                    continue;
                }
                if let Some(ni) = g.idx(nb.0, nb.1) {
                    buf.push(g.offset + ni);
                }
            }
        }
        if let Some(ls) = self.links.get(&n) {
            buf.extend(ls.iter().copied());
        }
    }

    /// Node sequence from `s` to `goal` (both included), or None when unreachable.
    fn astar(&self, s: usize, goal: usize) -> Option<Vec<usize>> {
        let goal_pos = self.pos_of(goal);
        let h = |n: usize| (self.pos_of(n) - goal_pos).length();
        let mut gscore = vec![f32::INFINITY; self.nodes.len()];
        let mut came = vec![usize::MAX; self.nodes.len()];
        let mut heap = BinaryHeap::new();
        gscore[s] = 0.0;
        heap.push((Reverse((h(s) * 1000.0) as u32), s));
        let mut found = false;
        let mut nbuf: Vec<usize> = vec![];
        while let Some((_, n)) = heap.pop() {
            if n == goal {
                found = true;
                break;
            }
            self.neighbors(n, &mut nbuf);
            let here = self.pos_of(n);
            for &nb in &nbuf {
                let cost = gscore[n] + (self.pos_of(nb) - here).length().max(0.01);
                if cost < gscore[nb] {
                    gscore[nb] = cost;
                    came[nb] = n;
                    heap.push((Reverse(((cost + h(nb)) * 1000.0) as u32), nb));
                }
            }
        }
        if !found {
            return None;
        }
        let mut seq = vec![goal];
        let mut cur = goal;
        while cur != s {
            cur = came[cur];
            seq.push(cur);
        }
        seq.reverse();
        Some(seq)
    }

    /// The plain tile of `layer` reachable from `s` that lies closest to `target`.
    fn nearest_reachable(&self, s: usize, layer: u16, target: Vec2) -> Option<usize> {
        let mut seen = vec![false; self.nodes.len()];
        let mut queue = std::collections::VecDeque::from([s]);
        seen[s] = true;
        let mut nbuf = vec![];
        let mut best: Option<(f32, usize)> = None;
        while let Some(n) = queue.pop_front() {
            let (l, x, z) = self.nodes[n];
            let on_stair = self.grids[l as usize].as_ref().is_some_and(|g| g.stair_at((x as i32, z as i32)).is_some());
            if l == layer && !on_stair {
                let d = (self.pos_of(n) - target).length();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, n));
                }
            }
            self.neighbors(n, &mut nbuf);
            for &nb in &nbuf {
                if !seen[nb] {
                    seen[nb] = true;
                    queue.push_back(nb);
                }
            }
        }
        best.map(|(_, n)| n)
    }

    /// Waypoints from `from` to `to` (excluding `from`), or None when unreachable. A target inside
    /// a blocked tile (furniture) or an enclosed pocket walks to the closest reachable tile.
    pub fn path(&self, from: Place, to: Place) -> Option<Vec<PathPt>> {
        let (gs, gg) = (self.grid(from.layer)?, self.grid(to.layer)?);
        let s = self.node(from.layer, gs.cell_of(from.pos))?;
        let want = gg.cell_of(to.pos);
        let direct = self.node(to.layer, want)?;
        let mut goal = direct;
        let mut seq = if gg.free(want) && gg.stair_at(want).is_none() { self.astar(s, goal) } else { None };
        if seq.is_none() {
            goal = self.nearest_reachable(s, to.layer, to.pos)?;
            seq = Some(if s == goal { vec![s] } else { self.astar(s, goal)? });
        }
        let seq = seq?;
        let end_pos = if goal == direct { to.pos } else { self.pos_of(goal) };
        if seq.len() <= 1 {
            return Some(vec![PathPt { pos: end_pos, layer: to.layer }]);
        }
        // (position, layer, keep): stairs and layer changes keep their exact waypoints
        let mut pts: Vec<(Vec2, u16, bool)> = vec![];
        for (i, &n) in seq.iter().enumerate().skip(1) {
            let (l, x, z) = self.nodes[n];
            let g = self.grids[l as usize].as_ref().unwrap();
            let on_stair = g.stair_at((x as i32, z as i32)).is_some();
            let layer_change = self.nodes[seq[i - 1]].0 != l || seq.get(i + 1).is_some_and(|&m| self.nodes[m].0 != l);
            pts.push((self.pos_of(n), l, on_stair || layer_change));
        }
        if let Some(last) = pts.last_mut() {
            last.0 = end_pos;
        }
        // string pulling inside runs of plain tiles of one layer
        let mut out: Vec<PathPt> = vec![];
        let mut anchor = from.pos;
        let mut i = 0;
        while i < pts.len() {
            if pts[i].2 {
                out.push(PathPt { pos: pts[i].0, layer: pts[i].1 });
                anchor = pts[i].0;
                i += 1;
                continue;
            }
            let layer = pts[i].1;
            let mut end = i;
            while end + 1 < pts.len() && !pts[end + 1].2 && pts[end + 1].1 == layer {
                end += 1;
            }
            let g = self.grid(layer).unwrap();
            let mut j = i;
            while j <= end {
                let mut far = j;
                for k in (j..=end).rev() {
                    if g.line_clear(anchor, pts[k].0) {
                        far = k;
                        break;
                    }
                }
                out.push(PathPt { pos: pts[far].0, layer });
                anchor = pts[far].0;
                j = far + 1;
            }
            i = end + 1;
        }
        Some(out)
    }
}
