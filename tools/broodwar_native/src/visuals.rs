//! Read-only independent sprite snapshots; animation timing remains native.
use std::collections::HashSet;
use bw_dat::structs::{LoneSprite, Sprite};
use serde_json::{json, Value};
use super::{image_chain, image_info};

pub(super) fn visible(flags: u8, mask: u8, player: usize) -> bool {
    player < 8 && flags & 0x20 == 0 && mask & (1 << player) != 0
}

/// Caller runs on the simulation thread with a valid SCR sprite list, whose
/// nodes and images remain allocated for the duration of the snapshot.
pub(super) unsafe fn lone_snapshot(mut ptr: *mut LoneSprite, player: usize) -> (Vec<Value>, bool) {
    let mut seen = HashSet::new();
    let mut sprites = Vec::new();
    while !ptr.is_null() && seen.len() < 4096 && seen.insert(ptr) {
        // SAFETY: Samase's game-thread getter supplies live LoneSprite nodes;
        // no original game function runs while their pointers are borrowed.
        let lone = unsafe { &*ptr };
        let sprite = lone.sprite;
        if !sprite.is_null() {
            // SAFETY: the live LoneSprite owns this SCR Sprite and its image chain.
            let s: &Sprite = unsafe { &*sprite };
            if visible(s.flags,s.visibility_mask,player) {
                // SAFETY: this build targets SCR only; the active union field is scr.
                let scr = unsafe { s.version_specific.scr };
                // SAFETY: these image pointers belong to the live sprite above.
                let (image, visual) = unsafe { (image_info(scr.main_image), image_chain(sprite)) };
                sprites.push(json!({"id":format!("{ptr:p}-{}",s.sprite_id),"spriteId":s.sprite_id,
                    "owner":s.player,"x":scr.position[0],"y":scr.position[1],
                    "elevation":s.elevation_level,"image":image,"visual":visual}));
            }
        }
        ptr = lone.next;
    }
    (sprites,!ptr.is_null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bw_dat::structs::{Image, SpriteScr, SpriteVersions};

    #[test]
    fn native_sprite_frames_follow_chain_and_cycles_are_bounded() {
        // SAFETY: these C structs contain only integers, raw pointers and unions
        // of those fields. All-zero is a valid representation; pointers are set
        // to live stack allocations before the reader is called.
        let (mut image, mut sprite, mut lone): (Image, Sprite, LoneSprite) = unsafe {
            (std::mem::zeroed(),std::mem::zeroed(),std::mem::zeroed())
        };
        image.image_id = 239;
        image.frame = 17;
        image.iscript.animation = 1;
        sprite.sprite_id = 130;
        sprite.visibility_mask = 1 << 6;
        let image_ptr = &raw mut image;
        sprite.version_specific = SpriteVersions {scr:SpriteScr {position:[64,96],main_image:image_ptr,first_image:image_ptr,last_image:image_ptr}};
        lone.sprite = &mut sprite;
        let lone_ptr = &raw mut lone;
        // SAFETY: write through the same provenance used for the cyclic list.
        unsafe {(*lone_ptr).next = lone_ptr;}
        // SAFETY: the list and its image allocations are live through the call.
        let (result,truncated) = unsafe {lone_snapshot(lone_ptr,6)};
        assert!(truncated);
        assert_eq!(result.len(),1);
        assert_eq!(result[0]["image"]["frame"],17);
        assert_eq!(result[0]["visual"]["images"][0]["animation"],1);
        assert_eq!(result[0]["x"],64);
        // SAFETY: this pointer still refers to the stack-owned sprite.
        unsafe {(*(*lone_ptr).sprite).flags = 0x20;}
        // SAFETY: same owned allocations; hidden sprites must not be exposed.
        assert!(unsafe {lone_snapshot(lone_ptr,6)}.0.is_empty());
    }

    #[test]
    fn visibility_requires_current_player_vision() {
        assert!(visible(0,1 << 6,6));
        assert!(!visible(0,1 << 5,6));
        assert!(!visible(0x20,1 << 6,6));
        assert!(!visible(0,255,8));
    }
}
