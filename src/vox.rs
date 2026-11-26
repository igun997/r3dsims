//! Minimal MagicaVoxel `.vox` import so users can bring their own pixel models.
//!
//! Only the first model of the file is imported, as a single part named `body`
//! (animate it through entity-level clips on `body`). MagicaVoxel is Z-up and its front faces -Y;
//! we map (x, y, z) -> (x - sx/2, z, sy/2 - y - 1) so the front faces +Z and feet sit on y=0.

use anyhow::{Result, bail};

use crate::model::{AssetDef, Op, PartDef};

fn u32le(b: &[u8], at: usize) -> Result<u32> {
    match b.get(at..at + 4) {
        Some(s) => Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]])),
        None => bail!("vox: truncated file"),
    }
}

pub fn import(bytes: &[u8], name: &str) -> Result<AssetDef> {
    if bytes.len() < 20 || &bytes[0..4] != b"VOX " {
        bail!("not a MagicaVoxel .vox file");
    }
    // MAIN chunk header at 8: id(4) content(4) children(4); children follow at 20
    if &bytes[8..12] != b"MAIN" {
        bail!("vox: missing MAIN chunk");
    }
    let mut pos = 8 + 12 + u32le(bytes, 12)? as usize;
    let mut size = (0i32, 0i32, 0i32);
    let mut voxels: Option<Vec<[u8; 4]>> = None;
    let mut palette: Option<Vec<[u8; 3]>> = None;
    while pos + 12 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let content = u32le(bytes, pos + 4)? as usize;
        let children = u32le(bytes, pos + 8)? as usize;
        let data = pos + 12;
        let Some(body) = bytes.get(data..data + content) else { bail!("vox: truncated chunk") };
        match id {
            b"SIZE" if voxels.is_none() && content >= 12 => {
                size = (u32le(body, 0)? as i32, u32le(body, 4)? as i32, u32le(body, 8)? as i32);
            }
            b"XYZI" if voxels.is_none() => {
                let n = u32le(body, 0)? as usize;
                let Some(raw) = body.get(4..4 + n * 4) else { bail!("vox: truncated XYZI") };
                voxels = Some(raw.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
            }
            b"RGBA" if content >= 1024 => {
                palette = Some(body.chunks_exact(4).take(256).map(|c| [c[0], c[1], c[2]]).collect());
            }
            _ => {}
        }
        pos = data + content + children;
    }
    let Some(voxels) = voxels else { bail!("vox: no voxel data") };
    let (sx, sy, _sz) = size;
    let pal = |i: u8| -> [u8; 3] {
        match &palette {
            Some(p) => p[(i as usize - 1).min(255)],
            None => [i, i, i],
        }
    };
    let mut shape = vec![];
    for [x, y, z, i] in voxels {
        if i == 0 {
            continue;
        }
        let c = pal(i);
        shape.push(Op::Box {
            at: [x as i32 - sx / 2, z as i32, sy / 2 - y as i32 - 1],
            size: [1, 1, 1],
            c: format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
        });
    }
    Ok(AssetDef {
        name: name.to_string(),
        voxel: 0.1,
        palette: Default::default(),
        parts: vec![PartDef { name: "body".into(), parent: None, pivot: [0.0, 0.0, 0.0], shape }],
        anims: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], content: &[u8], children: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend((content.len() as u32).to_le_bytes());
        v.extend((children.len() as u32).to_le_bytes());
        v.extend(content);
        v.extend(children);
        v
    }

    #[test]
    fn imports_two_voxels() {
        let size = chunk(b"SIZE", &[4, 0, 0, 0, 4, 0, 0, 0, 4, 0, 0, 0], &[]);
        let mut xyzi = 2u32.to_le_bytes().to_vec();
        xyzi.extend([0, 0, 0, 1, 1, 1, 1, 1]);
        let xyzi = chunk(b"XYZI", &xyzi, &[]);
        let mut children = size;
        children.extend(xyzi);
        let main = chunk(b"MAIN", &[], &children);
        let mut file = b"VOX ".to_vec();
        file.extend(150u32.to_le_bytes());
        file.extend(main);
        let def = import(&file, "t").unwrap();
        assert_eq!(def.parts[0].shape.len(), 2);
        crate::model::Model::bake(&def).unwrap();
    }
}
