//! Bounds-checked fog and visible creep from the original map tile flags.
use serde_json::{json, Value};

pub(super) struct Region {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Region {
    pub(super) fn parse(request: &Value, map: [u16; 2]) -> Result<Self, &'static str> {
        let field = |key| request[key].as_u64()
            .and_then(|value| usize::try_from(value).ok()).ok_or("invalid terrain region");
        let region = Self {x: field("x")?, y: field("y")?, width: field("width")?, height: field("height")?};
        if region.width == 0 || region.height == 0 || region.width > 256 || region.height > 256 ||
            region.x.checked_add(region.width).map_or(true, |end| end > usize::from(map[0])) ||
            region.y.checked_add(region.height).map_or(true, |end| end > usize::from(map[1])) {
            return Err("terrain region outside map or too large");
        }
        Ok(region)
    }

    pub(super) fn snapshot(&self, tiles: &[u32], map_width: usize, player: usize) -> Result<Value, &'static str> {
        if player >= 8 || map_width == 0 {return Err("invalid terrain view");}
        let mut fog = String::with_capacity(self.width * self.height);
        let mut creep = String::with_capacity(self.width * self.height);
        for y in self.y..self.y+self.height {
            for x in self.x..self.x+self.width {
                let index = y.checked_mul(map_width).and_then(|row| row.checked_add(x)).ok_or("terrain index overflow")?;
                let flags = tiles.get(index).ok_or("terrain backing array too short")?;
                let visibility = if flags & (0x100 << player) != 0 {'0'} else if flags & (1 << player) != 0 {'1'} else {'2'};
                fog.push(visibility);
                // Current creep must never reveal changes outside current vision.
                creep.push(if visibility == '2' && flags & 0x0040_0000 != 0 {'1'} else {'0'});
            }
        }
        Ok(json!({"ok":true,"x":self.x,"y":self.y,"width":self.width,"height":self.height,"fog":fog,"creep":creep}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creep_respects_fog_and_region_stride() {
        let region = Region::parse(&json!({"x":1,"y":1,"width":3,"height":1}),[4,2]).unwrap();
        let mut tiles = [0;8];
        tiles[5] = 0x0040_0000;
        tiles[6] = 0x0040_0000 | (1 << 6);
        tiles[7] = 0x0040_0000 | (0x100 << 6);
        let result = region.snapshot(&tiles,4,6).unwrap();
        assert_eq!(result["fog"],"210");
        assert_eq!(result["creep"],"100");
    }

    #[test]
    fn full_map_snapshot_is_bounded_and_preserves_visibility() {
        let region = Region::parse(&json!({"x":0,"y":0,"width":256,"height":256}),[256,256]).unwrap();
        let mut tiles=vec![0;256*256];
        tiles[65535]=0x100;
        let result=region.snapshot(&tiles,256,0).unwrap();
        assert_eq!(result["fog"].as_str().unwrap().len(),65536);
        assert_eq!(result["fog"].as_str().unwrap().as_bytes()[65535],b'0');
        assert!(Region::parse(&json!({"x":0,"y":0,"width":257,"height":256}),[257,256]).is_err());
    }

    #[test]
    fn rejects_overflow_and_short_backing_storage() {
        assert!(Region::parse(&json!({"x":u64::MAX,"y":0,"width":1,"height":1}),[256,256]).is_err());
        let region = Region::parse(&json!({"x":0,"y":0,"width":2,"height":1}),[2,1]).unwrap();
        assert!(region.snapshot(&[0],2,0).is_err());
        assert!(region.snapshot(&[0,0],2,8).is_err());
    }
}
