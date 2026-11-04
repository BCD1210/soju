//! Authenticated offline request dispatch and original command execution.
use std::{ffi::c_void, sync::atomic::Ordering};
use bw_dat::{structs::{Game, Unit}, UnitId};
use serde_json::{json, Value};
use super::{Runtime, VARS, active_units, handle, visible, log, world, production};

// SAFETY: caller runs on the game thread and passes a live unit from
// active_units. The pool getter and unit remain valid during tag encoding.
unsafe fn unit_tag(r: &Runtime, producer: *mut Unit) -> Result<u32, &'static str> {
        let mut base = std::ptr::null_mut();
        let mut count = 0;
        (r.array)(&mut base,&mut count);
        let stride = std::mem::size_of::<Unit>();
        let delta = (producer as usize).checked_sub(base as usize).ok_or("invalid unit array")?;
        if base.is_null() || delta % stride != 0 || delta / stride >= count { return Err("unit array layout mismatch"); }
        let index = delta / stride;
        let bits = if count > 1700 {13} else {11};
        if index+1 >= (1<<bits) { return Err("unit index overflow"); }
        Ok((index as u32+1) | (((*producer).minor_unique_index as u32)<<bits))
}

// SAFETY: selected unit is live and owned; the original handler consumes bytes
// synchronously on the game thread. Unit tags are checked against the pool.
unsafe fn send_one(r: &Runtime, producer: *mut Unit, command: &[u8]) -> Result<(), &'static str> {
        let uid = unit_tag(r,producer)?;
        let mut select = vec![0x63,1];
        select.extend(uid.to_le_bytes());
        (r.send)(select.as_ptr().cast(),select.len() as u32);
        (r.send)(command.as_ptr().cast(),command.len() as u32);
    Ok(())
}

pub(super) unsafe fn request(r: &Runtime, req: &Value) -> Result<Value, &'static str> {
    if req.get("token").and_then(Value::as_str) != Some(r.token.as_str()) { return Err("unauthorized"); }
    let mut vars = [usize::MAX; 3];
    (r.read)(VARS.as_ptr(), vars.as_mut_ptr(), vars.len());
    if vars[0] != 0 || vars[1] != 0 || vars[2] >= 8 { return Err("offline single-player game required"); }
    let game = (r.game)() as *const Game;
    if game.is_null() { return Err("game unavailable"); }
    let game = &*game;
    if game.map_width_tiles == 0 || game.map_height_tiles == 0 { return Err("map unavailable"); }
    let all = active_units(r);
    let player = vars[2];
    let op = req["op"].as_str().ok_or("missing op")?;
    if op == "state" || op == "scene" { return Ok(world::snapshot(r, game, &all, player, op == "scene", req["since"].as_u64())); }
    if op == "unit_types" {
        let types: Vec<_> = (0..228).map(|id| {
            let kind = UnitId(id);
            json!({"id":id,"mineralCost":kind.mineral_cost(),"gasCost":kind.gas_cost(),
                "supplyCost":kind.supply_cost(),"buildTime":kind.build_time(),
                "building":kind.is_building(),"worker":kind.is_worker(),
                "race":kind.primary_race().map(|race|race.id())})
        }).collect();
        let spells: Vec<_> = super::spells::SPELLS.iter().map(|spell| {
            let tech=spell.order.tech();
            json!({"action":spell.action,"title":spell.title,"order":spell.order.0,
                "casters":spell.casters,"target":if spell.unit_target {"unit"}else {"ground"},
                "tech":tech.map(|id|id.0),"energy":tech.map(|id|id.energy_cost())})
        }).collect();
        let technologies: Vec<_> = (0..44).map(|id| {
            let tech=bw_dat::TechId(id);
            json!({"id":id,"mineralCost":tech.mineral_cost(),"gasCost":tech.gas_cost(),"energy":tech.energy_cost()})
        }).collect();
        return Ok(json!({"technologies":technologies,"ok":true,"source":"native-units.dat/orders.dat/techdata.dat","supplyUnits":"half-supply","types":types,"spells":spells}));
    }
    if op == "terrain" {
        let region = super::terrain::Region::parse(req,[game.map_width_tiles,game.map_height_tiles])?;
        let tiles = (r.tiles)();
        if tiles.is_null() {return Err("terrain unavailable");}
        let count = usize::from(game.map_width_tiles) * usize::from(game.map_height_tiles);
        // SAFETY: Samase returns the loaded map's tile array. Width/height come
        // from that same live Game; this callback runs on the simulation thread.
        let flags = unsafe {std::slice::from_raw_parts(tiles,count)};
        let mut result = region.snapshot(flags,usize::from(game.map_width_tiles),player)?;
        result["frame"] = json!(game.frame_count);
        return Ok(result);
    }
    let seq = req["seq"].as_u64().ok_or("missing seq")?;
    if seq <= r.sequence.load(Ordering::Relaxed) { return Err("stale command sequence"); }
    let wanted = req["units"].as_array().ok_or("missing units")?;
    if wanted.is_empty() || wanted.len() > 12 { return Err("select 1 to 12 units"); }
    let mut selected = Vec::new();
    for wanted in wanted {
        let id = wanted.as_str().ok_or("invalid unit id")?;
        let ptr = all.iter().copied().find(|&u| handle(u) == id).ok_or("stale unit")?;
        if (*ptr).player as usize != player || (*ptr).flingy.hitpoints <= 0 { return Err("unit is not owned or alive"); }
        if !selected.contains(&ptr) { selected.push(ptr); }
    }
    if op == "lift" && ![106,111,113,114,116,122].contains(&(*selected[0]).unit_id) {return Err("building cannot lift");}
    if op == "return_cargo" && ![7,41,64].contains(&(*selected[0]).unit_id) {return Err("return cargo requires a worker");}
    if op == "land" {
        let unit=selected[0];
        if selected.len()!=1 || ![106,111,113,114,116,122].contains(&(*unit).unit_id) {return Err("select one flying building");}
        let x=req["tileX"].as_u64().ok_or("invalid tileX")?;
        let y=req["tileY"].as_u64().ok_or("invalid tileY")?;
        if x>=u64::from(game.map_width_tiles) || y>=u64::from(game.map_height_tiles) {return Err("outside map");}
        let mut packet=vec![0x0c,0x47];packet.extend((x as u16).to_le_bytes());packet.extend((y as u16).to_le_bytes());packet.extend((*unit).unit_id.to_le_bytes());
        send_one(r,unit,&packet)?;r.sequence.store(seq,Ordering::Relaxed);
        return Ok(json!({"ok":true,"frame":game.frame_count,"queued":true}));
    }
    if let Some(command) = production::packet(req, UnitId((*selected[0]).unit_id).is_building())? {
        if selected.len() != 1 {return Err("select one producer or ability user");}
        send_one(r,selected[0],&command)?;
        r.sequence.store(seq,Ordering::Relaxed);
        return Ok(json!({"ok":true,"seq":seq,"frame":game.frame_count,"queued":true}));
    }
    if op == "build" || op == "train" || op == "addon" {
        if selected.len() != 1 { return Err("select one producer"); }
        let producer = selected[0];
        let kind = req["type"].as_u64().ok_or("missing unit type")?;
        let kind_id = u32::try_from(kind).ok().and_then(UnitId::optional).ok_or("invalid unit type")?;
        let mut command = Vec::new();
        if op == "build" || op == "addon" {
            if (op == "build" && (!UnitId((*producer).unit_id).is_worker() || kind_id.is_addon())) || (op == "addon" && (!UnitId((*producer).unit_id).is_building() || !kind_id.is_addon())) || !kind_id.is_building() {
                return Err("select a worker and a non-addon building");
            }
            if kind_id.primary_race() != UnitId((*producer).unit_id).primary_race() { return Err("builder race mismatch"); }
            let x = req["tileX"].as_u64().ok_or("invalid tileX")?;
            let y = req["tileY"].as_u64().ok_or("invalid tileY")?;
            if x >= game.map_width_tiles as u64 || y >= game.map_height_tiles as u64 { return Err("outside map"); }
            let build_order=if op == "addon" {36} else {match (*producer).unit_id { 7=>30,41=>25,64=>31,_=>return Err("unsupported builder") }};
            command.extend([0x0c,build_order]);
            command.extend((x as u16).to_le_bytes());
            command.extend((y as u16).to_le_bytes());
            command.extend((kind as u16).to_le_bytes());
        } else {
            if !UnitId((*producer).unit_id).is_building() || kind_id.is_building() { return Err("select a production building and a unit"); }
            // The original command handler enforces producer, tech and resources.
            command.push(0x1f);
            command.extend((kind as u16).to_le_bytes());
        }
        send_one(r,producer,&command)?;
        r.sequence.store(seq,Ordering::Relaxed);
        log(&format!("command seq={seq} op={op} type={kind} frame={}",game.frame_count));
        return Ok(json!({"ok":true,"seq":seq,"frame":game.frame_count,"queued":true}));
    }
    let target = match req.get("target").and_then(Value::as_str) {
        Some(id) => {
            let u = all.iter().copied().find(|&u| handle(u) == id).ok_or("stale target")?;
            if !visible(u, player) { return Err("target is not visible"); }
            u
        },
        None => std::ptr::null_mut()
    };
    if op == "rally" || op == "spell" {
        let spell=if op=="spell" {
            if selected.len()!=1 {return Err("select one spell caster");}
            Some(super::spells::validate(req["action"].as_str().ok_or("missing spell action")?,
                (*selected[0]).unit_id,!target.is_null())?)
        } else {None};
        if op == "rally" && selected.iter().any(|&u| !UnitId((*u).unit_id).is_building()) {
            return Err("rally requires a production building");
        }
        let (x,y,tag) = if target.is_null() {
            let x=req["x"].as_u64().ok_or("invalid x")?;
            let y=req["y"].as_u64().ok_or("invalid y")?;
            if x>=u64::from(game.map_width_tiles)*32 || y>=u64::from(game.map_height_tiles)*32 {
                return Err("outside map");
            }
            (u16::try_from(x).map_err(|_| "x overflow")?,u16::try_from(y).map_err(|_| "y overflow")?,0)
        } else {
            (u16::try_from((*target).flingy.position.x).map_err(|_| "target outside map")?,
             u16::try_from((*target).flingy.position.y).map_err(|_| "target outside map")?,unit_tag(r,target)?)
        };
        let packet=match spell {
            Some(spell)=>production::targeted_packet(x,y,tag,spell.order.0),
            None=>production::rally_packet(x,y,tag),
        };
        for &producer in &selected {send_one(r,producer,&packet)?;}
        r.sequence.store(seq,Ordering::Relaxed);
        return Ok(json!({"ok":true,"seq":seq,"frame":game.frame_count,"queued":true}));
    }
    let (order, x, y) = match op {
        "move" | "attack_move" | "patrol" => {
            let x = req["x"].as_u64().ok_or("invalid x")?;
            let y = req["y"].as_u64().ok_or("invalid y")?;
            if x >= game.map_width_tiles as u64*32 || y >= game.map_height_tiles as u64*32 { return Err("outside map"); }
            (if op == "move" {6} else if op == "patrol" {152} else {14}, x as u32, y as u32)
        },
        "gather" => {
            if target.is_null() || !UnitId((*target).unit_id).is_resource_container() { return Err("select a visible resource"); }
            if ![176,177,178,110,149,157].contains(&(*target).unit_id) { return Err("gas needs an owned refinery"); }
            if ![176,177,178].contains(&(*target).unit_id) && (*target).player as usize != player { return Err("gas needs an owned refinery"); }
            if selected.iter().any(|&u| ![7,41,64].contains(&(*u).unit_id)) { return Err("select workers"); }
            (if (176..=178).contains(&(*target).unit_id) {79} else {81}, (*target).flingy.position.x as u32, (*target).flingy.position.y as u32)
        },
        "follow" => {
            if target.is_null() || (*target).player as usize != player { return Err("select an owned follow target"); }
            if selected.contains(&target) { return Err("cannot follow self"); }
            (49, (*target).flingy.position.x as u32, (*target).flingy.position.y as u32)
        },
        "attack" => {
            if target.is_null() || (*target).player as usize == player || game.alliances[player][(*target).player as usize] != 0 { return Err("select a visible enemy"); }
            (10, (*target).flingy.position.x as u32, (*target).flingy.position.y as u32)
        },
        "repair" => {
            if target.is_null() || (*target).player as usize != player || !UnitId((*target).unit_id).is_mechanical() || selected.iter().any(|&u| (*u).unit_id != 7) {return Err("repair requires SCVs and an owned mechanical target");}
            (34,u32::try_from((*target).flingy.position.x).map_err(|_| "target outside map")?,u32::try_from((*target).flingy.position.y).map_err(|_| "target outside map")?)
        },
        "load" => {
            if target.is_null() || (*target).player as usize != player || selected.len()!=1 {return Err("load requires one transport and an owned target");}
            let kind=UnitId((*selected[0]).unit_id);
            if kind.cargo_space_provided()==0 {return Err("selected unit is not a transport");}
            (if kind.0==125 {95} else {94},u32::try_from((*target).flingy.position.x).map_err(|_| "target outside map")?,u32::try_from((*target).flingy.position.y).map_err(|_| "target outside map")?)
        },
        "hold" => (107,0,0),
        "stop" => (1,0,0),
        _ => return Err("unsupported op")
    };
    for &unit in &selected {
        let order=if op=="attack" {UnitId((*unit).unit_id).attack_unit_order().0 as u32} else {order};
        (r.issue)(unit as *mut c_void, order, x, y, target as *mut c_void, 0xe4);
    }
    r.sequence.store(seq, Ordering::Relaxed);
    log(&format!("command seq={seq} op={op} count={} frame={}",selected.len(),game.frame_count));
    Ok(json!({"ok":true,"seq":seq,"frame":game.frame_count,"issued":selected.len()}))
}
