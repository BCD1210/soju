//! Read-only visual state and a bounded, replayable native event journal.
//! Missing from sight is never treated as a death. Deaths come from the original
//! game's Die order hook; damage and firing come from native HP/cooldowns.
use std::collections::{HashMap, HashSet, VecDeque};
use bw_dat::{structs::{Game, Unit, Bullet}, UnitId};
use serde_json::{json, Value};
use super::{Runtime, handle, visible, image_info, image_chain};

#[derive(Clone)]
struct Sample { hp:i32, shields:i32, ground:u8, air:u8, kind:u16 }
#[derive(Default)]
pub struct History {
    pub frame:u32, pub generation:u32, pub sequence:u64,
    samples:HashMap<String,Sample>, events:VecDeque<Value>,
    deaths:HashMap<String,u32>, bullet_ids:HashMap<usize,u64>, next_bullet:u64,
}
impl History {
    fn push(&mut self,mut event:Value) {
        self.sequence+=1;event["seq"]=json!(self.sequence);event["frame"]=json!(self.frame);
        self.events.push_back(event);
        while self.events.len()>1024 {self.events.pop_front();}
    }
    fn reset_if_needed(&mut self,frame:u32) {
        if frame<self.frame { let generation=self.generation+1;*self=Self::default();self.generation=generation; }
        self.frame=frame;
    }
}
pub fn cstring(data:&[u8])->String {
    String::from_utf8_lossy(&data[..data.iter().position(|&c|c==0).unwrap_or(data.len())]).into_owned()
}
unsafe fn target_id(ptr:*mut Unit,all:&HashSet<usize>,player:usize)->Value {
    if !ptr.is_null() && all.contains(&(ptr as usize)) && visible(ptr,player) {json!(handle(ptr))} else {Value::Null}
}
pub unsafe fn unit_info(ptr:*mut Unit,all:&HashSet<usize>,player:usize,details:bool)->Value {
    let u=&*ptr;let sprite=u.flingy.sprite;let kind=UnitId(u.unit_id);
    let image=if sprite.is_null(){std::ptr::null_mut()}else{(*sprite).version_specific.scr.main_image};
    let turret=if !u.subunit.is_null() && UnitId((*u.subunit).unit_id).is_subunit() {u.subunit}else{ptr};
    let ti=if (*turret).flingy.sprite.is_null(){std::ptr::null_mut()}else{(*(*turret).flingy.sprite).version_specific.scr.main_image};
    let dead=u.order==bw_dat::order::DIE.0;
    let hidden=sprite.is_null() || (*sprite).flags&0x20!=0 || u.flags&0x60!=0;
    json!({"id":handle(ptr),"type":u.unit_id,"owner":u.player,
        "x":u.flingy.position.x,"y":u.flingy.position.y,"facing":u.flingy.facing_direction,
        "movementDirection":u.flingy.movement_direction,"speed":u.flingy.current_speed,
        "hp":u.flingy.hitpoints as f64/256.0,"maxHP":kind.hitpoints() as f64/256.0,
        "shields":u.shields as f64/256.0,"maxShields":kind.shields() as f64/256.0,"energy":u.energy as f64/256.0,
        "completed":u.flags&1!=0,"order":u.order,"orderState":u.order_state,"secondaryOrder":u.secondary_order,
        "target":target_id((*turret).order_target.unit,all,player),
        "targetX":(*turret).order_target.pos.x,"targetY":(*turret).order_target.pos.y,
        "carrying":u.carried_powerup_flags,"flags":u.flags,"dead":dead,"renderHidden":hidden,
        "airborne":u.flags&4!=0,"burrowed":u.flags&0x10!=0,"cloaked":u.flags&0x300!=0,
        "transported":u.flags&0x60!=0,"building":kind.is_building(),"worker":kind.is_worker(),
        "townHall":kind.is_town_hall(),"resource":kind.is_resource_container(),"race":kind.primary_race().map(|r|r.id()),
        "resources":if kind.is_resource_container(){u.unit_specific2.resource.amount as u32}else{0},
        "groundCooldown":(*turret).ground_cooldown,"airCooldown":(*turret).air_cooldown,
        "groundWeapon":UnitId((*turret).unit_id).ground_weapon().map(|w|w.0),"airWeapon":UnitId((*turret).unit_id).air_weapon().map(|w|w.0),
        "turret":{"type":(*turret).unit_id,"facing":(*turret).flingy.facing_direction,"image":image_info(ti)},
        "buildTimeRemaining":u.remaining_build_time,"buildQueue":u.build_queue,
        "status":{"stim":u.stim_timer,"ensnare":u.ensnare_timer,"lockdown":u.lockdown_timer,
            "irradiate":u.irradiate_timer,"stasis":u.stasis_timer,"plague":u.plague_timer,
            "storm":u.is_under_storm,"matrix":u.matrix_timer,"maelstrom":u.maelstrom_timer,
            "hallucination":u.flags&0x4000_0000!=0},
        "image":image_info(image),"visual":if details{image_chain(sprite)}else{Value::Null}})
}
pub unsafe fn observe(r:&Runtime,game:&Game,units:&[*mut Unit],player:usize) {
    let mut h=r.history.lock().unwrap();h.reset_if_needed(game.frame_count);
    let all:HashSet<usize>=units.iter().map(|&u|u as usize).collect();
    let mut next=HashMap::new();
    for &ptr in units {
        let u=&*ptr;if !visible(ptr,player) || UnitId(u.unit_id).is_subunit(){continue}
        let id=handle(ptr);let turret=if !u.subunit.is_null() && UnitId((*u.subunit).unit_id).is_subunit(){u.subunit}else{ptr};
        let sample=Sample{hp:u.flingy.hitpoints,shields:u.shields,ground:(*turret).ground_cooldown,air:(*turret).air_cooldown,kind:u.unit_id};
        if let Some(old)=h.samples.get(&id).cloned() {
            if sample.hp<old.hp || sample.shields<old.shields {
                h.push(json!({"kind":"damage","unit":id,"type":u.unit_id,"x":u.flingy.position.x,"y":u.flingy.position.y,
                    "airborne":u.flags&4!=0,"hpLoss":(old.hp-sample.hp).max(0) as f64/256.0,"shieldLoss":(old.shields-sample.shields).max(0) as f64/256.0}));
            }
            if sample.ground>old.ground || sample.air>old.air {
                let weapon=if sample.air>old.air{UnitId((*turret).unit_id).air_weapon()}else{UnitId((*turret).unit_id).ground_weapon()};
                h.push(json!({"kind":"attack","unit":id,"weapon":weapon.map(|w|w.0),
                    "x":u.flingy.position.x,"y":u.flingy.position.y,"airborne":u.flags&4!=0,
                    "target":target_id((*turret).order_target.unit,&all,player),
                    "targetX":(*turret).order_target.pos.x,"targetY":(*turret).order_target.pos.y}));
            }
            if sample.kind!=old.kind {h.push(json!({"kind":"transform","unit":id,"fromType":old.kind,"type":sample.kind}));}
        }
        next.insert(id,sample);
    }
    h.samples=next;
    h.deaths.retain(|_,f|game.frame_count.saturating_sub(*f)<240);
}
pub unsafe fn death(r:&Runtime,ptr:*mut Unit,game:&Game,player:usize) {
    if !visible(ptr,player) || UnitId((*ptr).unit_id).is_subunit(){return}
    let mut h=r.history.lock().unwrap();h.reset_if_needed(game.frame_count);let id=handle(ptr);
    if h.deaths.insert(id.clone(),game.frame_count).is_none() {
        h.push(json!({"kind":"death","unit":id,"state":unit_info(ptr,&HashSet::new(),player,false)}));
    }
}
pub unsafe fn snapshot(r:&Runtime,game:&Game,all:&[*mut Unit],player:usize,details:bool,since:Option<u64>)->Value {
    let pointers:HashSet<usize>=all.iter().map(|&u|u as usize).collect();
    let visible:Vec<_>=all.iter().copied().filter(|&u|visible(u,player) && !UnitId((*u).unit_id).is_subunit() && (*u).order!=bw_dat::order::DIE.0).collect();
    let units:Vec<_>=visible.iter().take(4096).map(|&u|unit_info(u,&pointers,player,details)).collect();
    let dying: Vec<_> = all.iter().copied().filter(|&unit| {
        // SAFETY: all is the live list collected on the same simulation thread.
        let unit_ref = unsafe {&*unit};
        let sprite = unit_ref.flingy.sprite;
        if sprite.is_null() || unit_ref.flags & 0x60 != 0 {return false;}
        // SAFETY: the unit owns this live sprite through the game-thread read.
        if unsafe {(*sprite).flags & 0x20 != 0} {return false;}
        (unsafe {super::visible(unit,player)}) && unit_ref.order == bw_dat::order::DIE.0 &&
            !UnitId(unit_ref.unit_id).is_subunit()
    }).take(4096).map(|unit| {
        // SAFETY: these pointers belong to the same live list and no game step
        // occurs while serializing the original image-chain state.
        unsafe {unit_info(unit,&pointers,player,true)}
    }).collect();
    let (lone_sprites,lone_truncated) = match r.lone_sprites {
        Some(first) => {
            // SAFETY: supported Samase getter and pinned SCR layout, read only
            // on the game thread; lone_snapshot bounds/cycle-checks traversal.
            unsafe {super::visuals::lone_snapshot(first().cast(),player)}
        }
        None => (Vec::new(),false),
    };
    let mut h=r.history.lock().unwrap();let mut bullets=Vec::new();let mut live=HashSet::new();
    let mut bullet=(r.bullets)() as *mut Bullet;
    for _ in 0..4096 {
        if bullet.is_null() || !live.insert(bullet as usize){break}
        let b=&*bullet;let sprite=b.flingy.sprite;
        if !sprite.is_null() && (*sprite).visibility_mask&(1<<player)!=0 {
            let key=bullet as usize;
            if !h.bullet_ids.contains_key(&key){h.next_bullet+=1;let id=h.next_bullet;h.bullet_ids.insert(key,id);}
            bullets.push(json!({"id":h.bullet_ids[&key],"weapon":b.weapon_id,"owner":b.player,
                "x":b.flingy.position.x,"y":b.flingy.position.y,"facing":b.flingy.facing_direction,
                "source":target_id(b.parent,&pointers,player),"target":target_id(b.target.unit,&pointers,player),
                "targetX":b.target.pos.x,"targetY":b.target.pos.y,"state":b.state,"remaining":b.death_timer,
                "image":image_info((*sprite).version_specific.scr.main_image)}));
        }
        bullet=b.flingy.next as *mut Bullet;
    }
    h.bullet_ids.retain(|p,_|live.contains(p));
    let earliest=h.events.front().and_then(|e|e["seq"].as_u64()).unwrap_or(h.sequence+1);
    let events:Vec<_>=since.map(|since|h.events.iter().filter(|e|e["seq"].as_u64().unwrap_or(0)>since).cloned().collect()).unwrap_or_default();
    json!({"ok":true,"protocol":2,"source":"blizzard-starcraft","pid":std::process::id(),
        "session":format!("{}:{}",r.session,h.generation),"frame":game.frame_count,"broodWar":game.is_bw!=0,"player":player,
        "minerals":game.minerals[player],"gas":game.gas[player],"supplies":(0..3).map(|race|json!({"race":race,"used":game.supplies[race].used[player],"provided":game.supplies[race].provided[player]})).collect::<Vec<_>>(),
        "playerColors":game.scr_init_color_rgba,"alliances":game.alliances[player],
        "map":{"width":game.map_width_tiles as u32*32,"height":game.map_height_tiles as u32*32,"tileset":game.tileset,
            "path":cstring(&game.map_path),"title":cstring(&game.map_title)},
        "units":units,"dyingUnits":dying,"loneSprites":lone_sprites,
        "loneSpritesAvailable":r.lone_sprites.is_some(),"loneSpritesTruncated":lone_truncated,
        "bullets":bullets,"events":events,"eventSeq":h.sequence,
        "eventsLost":since.map(|s|s+1<earliest).unwrap_or(false),"visibleCount":visible.len(),"truncated":visible.len()>4096})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn journal_is_bounded_and_ordered(){let mut h=History::default();for _ in 0..1100{h.push(json!({"kind":"damage"}));}assert_eq!(h.events.len(),1024);assert_eq!(h.events.front().unwrap()["seq"],77);assert_eq!(h.sequence,1100);}
    #[test] fn new_game_resets_history(){let mut h=History::default();h.reset_if_needed(500);h.push(json!({"kind":"death"}));h.reset_if_needed(1);assert_eq!(h.generation,1);assert!(h.events.is_empty());assert_eq!(h.sequence,0);}
}
