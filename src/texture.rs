//! Material ids. The patterns themselves (plank joints, brick bond, tile grout, grass speckle...)
//! live in `shader.wgsl` (`pattern`), evaluated per texel on the GPU: voxel faces use one texel
//! per voxel so patterns line up across neighbouring voxels, terrain tops use `TERRAIN_TEXELS`
//! texels per tile.

pub const NONE: u8 = 0;
pub const WOOD: u8 = 1;
pub const BRICK: u8 = 2;
pub const TILE: u8 = 3;
pub const STONE: u8 = 4;
pub const GRASS: u8 = 5;
pub const DIRT: u8 = 6;
pub const FABRIC: u8 = 7;
pub const METAL: u8 = 8;
pub const PLASTER: u8 = 9;
pub const SAND: u8 = 10;

/// Texels per terrain tile edge for texturing tile tops.
pub const TERRAIN_TEXELS: f32 = 4.0;

pub fn from_name(name: &str) -> Option<u8> {
    Some(match name {
        "" | "none" | "flat" => NONE,
        "wood" => WOOD,
        "brick" | "roof" => BRICK,
        "tile" | "tiles" => TILE,
        "stone" | "cobble" => STONE,
        "grass" => GRASS,
        "dirt" | "soil" => DIRT,
        "fabric" | "carpet" | "cloth" => FABRIC,
        "metal" => METAL,
        "plaster" | "wall" => PLASTER,
        "sand" => SAND,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve() {
        assert_eq!(from_name("wood"), Some(WOOD));
        assert_eq!(from_name("carpet"), Some(FABRIC));
        assert_eq!(from_name("nope"), None);
    }
}
