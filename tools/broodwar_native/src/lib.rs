#![recursion_limit = "256"]
//! Runs inside the unmodified Blizzard StarCraft executable via Samase.
//! All game memory access and order execution happen on the game thread.
use std::{ffi::c_void, fs::OpenOptions, io::Write, net::UdpSocket,
    sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}};
use bw_dat::{structs::{Game, Unit}, UnitId};
mod world;
use samase_plugin::{PluginApi, VarId};
use serde_json::{json, Value};

type Getter = unsafe extern "C" fn() -> *mut c_void;
type Order = unsafe extern "C" fn(*mut c_void, u32, u32, u32, *mut c_void, u32);
const VARS: [u16; 3] = [VarId::IsMultiplayer as u16, VarId::IsReplay as u16, VarId::LocalPlayerId as u16];
struct Runtime {
    game: Getter, units: Getter, hidden: Getter, bullets: Getter, issue: Order,
    history: Mutex<world::History>, session: String,
    tiles: unsafe extern "C" fn() -> *mut u32,
    send: unsafe extern "C" fn(*const c_void, u32),
    array: unsafe extern "C" fn(*mut *mut c_void, *mut usize),
    read: unsafe extern "C" fn(*const u16, *mut usize, usize),
    socket: UdpSocket, token: String, sequence: AtomicU64,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static RESPONSE_ID: AtomicU64 = AtomicU64::new(1);
fn send_response(socket: &UdpSocket, address: std::net::SocketAddr, data: &[u8]) {
    // macOS defaults reject UDP payloads above 9216 bytes, including through Wine.
    const CHUNK: usize = 8192 - 16;
    if data.len() > 2 * 1024 * 1024 {
        let _ = socket.send_to(b"{\"ok\":false,\"error\":\"snapshot exceeds 2 MiB\"}", address);
    } else if data.len() <= 8192 {
        if let Err(error) = socket.send_to(data, address) { log(&format!("response send: {error}")); }
    } else {
        let id = RESPONSE_ID.fetch_add(1, Ordering::Relaxed) as u32;
        let count = data.len().div_ceil(CHUNK) as u16;
        for (index, part) in data.chunks(CHUNK).enumerate() {
            let mut packet = Vec::with_capacity(16 + part.len());
            packet.extend(b"SBW1");packet.extend(id.to_le_bytes());
            packet.extend((index as u16).to_le_bytes());packet.extend(count.to_le_bytes());
            packet.extend((data.len() as u32).to_le_bytes());packet.extend(part);
            if let Err(error) = socket.send_to(&packet, address) { log(&format!("chunk send: {error}"));break; }
        }
    }
}

fn log(message: &str) {
    let path = std::env::var("SOJU_BW_LOG").unwrap_or_else(|_| "soju-bw-native.log".into());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

unsafe fn active_units(r: &Runtime) -> Vec<*mut Unit> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for first in [r.units, r.hidden] {
        let mut unit = first() as *mut Unit;
        for _ in 0..16384 {
            if unit.is_null() || !seen.insert(unit as usize) { break; }
            result.push(unit);
            unit = (*unit).flingy.next as *mut Unit;
        }
    }
    result
}
unsafe fn handle(unit: *mut Unit) -> String {
    format!("{:x}-{}", unit as usize, (*unit).minor_unique_index)
}
unsafe fn visible(unit: *mut Unit, player: usize) -> bool {
    if (*unit).player as usize == player { return true; }
    let sprite = (*unit).flingy.sprite;
    !sprite.is_null() && (*sprite).flags & 0x20 == 0 &&
        (*sprite).visibility_mask & (1 << player) != 0 &&
        ((*unit).flags & 0x300 == 0 || (*unit).detection_status & (1 << player) != 0)
}
unsafe fn image_info(image: *mut bw_dat::structs::Image) -> Value {
    if image.is_null() { return Value::Null; }
    let i = &*image;
    json!({"id":i.image_id,"frame":i.frame,"frameset":i.frameset,"flags":i.flags,
        "direction":i.direction,"drawfunc":i.drawfunc,"offsetX":i.x_offset,"offsetY":i.y_offset,
        "mapX":i.map_position.x,"mapY":i.map_position.y,"animation":i.iscript.animation})
}
unsafe fn image_chain(sprite: *mut bw_dat::structs::Sprite) -> Value {
    if sprite.is_null() { return json!({"images":[],"truncated":false}); }
    let mut ptr = (*sprite).version_specific.scr.first_image;
    let mut seen = Vec::new();
    let mut images = Vec::new();
    while !ptr.is_null() && seen.len() < 32 && !seen.contains(&ptr) {
        seen.push(ptr);
        images.push(image_info(ptr));
        ptr = (*ptr).next;
    }
    json!({"images":images,"truncated":!ptr.is_null()})
}
unsafe fn request(r: &Runtime, req: &Value) -> Result<Value, &'static str> {
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
    if op == "terrain" {
        let x = req["x"].as_u64().ok_or("missing tile x")?;
        let y = req["y"].as_u64().ok_or("missing tile y")?;
        let w = req["width"].as_u64().ok_or("missing tile width")?;
        let h = req["height"].as_u64().ok_or("missing tile height")?;
        if w == 0 || h == 0 || w > 64 || h > 64 ||
            x.checked_add(w).map_or(true, |v| v > game.map_width_tiles as u64) ||
            y.checked_add(h).map_or(true, |v| v > game.map_height_tiles as u64) {
            return Err("terrain region outside map or too large");
        }
        let tiles = (r.tiles)();
        if tiles.is_null() { return Err("terrain unavailable"); }
        let mut fog = String::with_capacity((w*h) as usize);
        for ty in y..y+h {
            for tx in x..x+w {
                let f = *tiles.add((ty*game.map_width_tiles as u64+tx) as usize);
                fog.push(if f & (0x100 << player) != 0 {'0'} else if f & (1 << player) != 0 {'1'} else {'2'});
            }
        }
        return Ok(json!({"ok":true,"frame":game.frame_count,"x":x,"y":y,"width":w,"height":h,"fog":fog}));
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
    if op == "build" || op == "train" {
        if selected.len() != 1 { return Err("select one producer"); }
        let producer = selected[0];
        let kind = req["type"].as_u64().ok_or("missing unit type")?;
        let kind_id = u32::try_from(kind).ok().and_then(UnitId::optional).ok_or("invalid unit type")?;
        let mut command = Vec::new();
        if op == "build" {
            if !UnitId((*producer).unit_id).is_worker() || !kind_id.is_building() || kind_id.is_addon() {
                return Err("select a worker and a non-addon building");
            }
            if kind_id.primary_race() != UnitId((*producer).unit_id).primary_race() { return Err("builder race mismatch"); }
            let x = req["tileX"].as_u64().ok_or("invalid tileX")?;
            let y = req["tileY"].as_u64().ok_or("invalid tileY")?;
            if x >= game.map_width_tiles as u64 || y >= game.map_height_tiles as u64 { return Err("outside map"); }
            let build_order=match (*producer).unit_id { 7=>30,41=>25,64=>31,_=>return Err("unsupported builder") };
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
        let mut base = std::ptr::null_mut();
        let mut count = 0;
        (r.array)(&mut base,&mut count);
        let stride = std::mem::size_of::<Unit>();
        let delta = (producer as usize).checked_sub(base as usize).ok_or("invalid unit array")?;
        if base.is_null() || delta % stride != 0 || delta / stride >= count { return Err("unit array layout mismatch"); }
        let index = delta / stride;
        let bits = if count > 1700 {13} else {11};
        if index+1 >= (1<<bits) { return Err("unit index overflow"); }
        let uid = (index as u32+1) | (((*producer).minor_unique_index as u32)<<bits);
        let mut select = vec![0x63,1];
        select.extend(uid.to_le_bytes());
        (r.send)(select.as_ptr().cast(),select.len() as u32);
        (r.send)(command.as_ptr().cast(),command.len() as u32);
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
unsafe extern "C" fn frame() {
    let Some(r) = RUNTIME.get() else { return; };
    let mut vars = [usize::MAX; 3];
    (r.read)(VARS.as_ptr(), vars.as_mut_ptr(), vars.len());
    let game=(r.game)() as *const Game;
    if vars[0]==0 && vars[1]==0 && vars[2]<8 && !game.is_null() && (*game).map_width_tiles>0 {
        world::observe(r,&*game,&active_units(r),vars[2]);
    }
    let mut buffer = [0u8; 4096];
    // Bounded input work keeps the original simulation responsive.
    for _ in 0..16 {
        match r.socket.recv_from(&mut buffer) {
            Ok((size, addr)) => {
                if !addr.ip().is_loopback() { continue; }
                let result = match serde_json::from_slice::<Value>(&buffer[..size]) {
                    Ok(req) => request(r, &req).unwrap_or_else(|e| json!({"ok":false,"error":e})),
                    Err(_) => json!({"ok":false,"error":"invalid JSON"})
                };
                if let Ok(data) = serde_json::to_vec(&result) {
                    send_response(&r.socket, addr, &data);
                }
            },
            Err(_) => break,
        }
    }
}

unsafe extern "C" fn step_order(unit:*mut c_void, original:unsafe extern "C" fn(*mut c_void)) {
    if let Some(r)=RUNTIME.get() {
        let ptr=unit as *mut Unit;
        if !ptr.is_null() && (*ptr).order==bw_dat::order::DIE.0 {
            let mut vars=[usize::MAX;3];(r.read)(VARS.as_ptr(),vars.as_mut_ptr(),vars.len());
            let game=(r.game)() as *const Game;
            if vars[0]==0 && vars[1]==0 && vars[2]<8 && !game.is_null() {
                world::death(r,ptr,&*game,vars[2]);
            }
        }
    }
    original(unit);
}

#[no_mangle]
pub unsafe extern "C" fn samase_plugin_init(api: *const PluginApi) {
    if api.is_null() { return; }
    let version = std::ptr::addr_of!((*api).version).read();
    log(&format!("soju_bw native plugin pid={} pointer_bits={} api={version}",std::process::id(),usize::BITS));
    // Require the API version validated with the pinned native client.
    if version < 44 { log("unsupported API: need >=44"); return; }
    let (Some(game), Some(units), Some(issue)) = (((*api).game)(), ((*api).first_active_unit)(), ((*api).issue_order)()) else {
        log("required game functions unavailable"); return;
    };
    let (Some(send),Some(array)) = (((*api).send_command)(),((*api).unit_array_len)()) else {
        log("native command functions unavailable"); return;
    };
    let (Some(hidden),Some(bullets),Some(units_dat)) = (((*api).first_hidden_unit)(),((*api).first_active_bullet)(),((*api).extended_dat)(0)) else {
        log("visual state functions unavailable");return;
    };
    let mut dat_len=0usize;let dat=units_dat(&mut dat_len);
    if dat.is_null() || dat_len<54 { log("unit DAT unavailable");return; }
    bw_dat::init_units(dat.cast(),dat_len);
    let Some(weapons_dat)=((*api).extended_dat)(1) else {log("weapons DAT unavailable");return;};
    let dat=weapons_dat(&mut dat_len);
    if dat.is_null() || dat_len==0 {log("weapons DAT unavailable");return;}
    bw_dat::init_weapons(dat.cast(),dat_len);
    let mut supported = [0u8;3];
    ((*api).load_vars)(VARS.as_ptr(),supported.as_mut_ptr(),VARS.len());
    if supported.iter().any(|&x| x < 2) { log("required game variables unavailable"); return; }
    let Ok(token) = std::env::var("SOJU_BW_TOKEN") else { log("missing session token"); return; };
    if token.len() < 24 { log("invalid session token"); return; }
    let Ok(socket) = UdpSocket::bind("127.0.0.1:8795") else { log("bridge port unavailable"); return; };
    if socket.set_nonblocking(true).is_err() { log("nonblocking socket unavailable"); return; }
    let Some(tiles) = ((*api).map_tile_flags)() else { log("terrain flags unavailable"); return; };
    let runtime = Runtime {game,units,hidden,bullets,issue,tiles,send,array,
        history:Mutex::new(world::History::default()),
        session:format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()),read:(*api).read_vars,socket,token,sequence:AtomicU64::new(0)};
    if RUNTIME.set(runtime).is_err() { return; }
    let result = ((*api).hook_step_objects)(frame,1);
    let deaths=((*api).hook_step_order)(step_order);
    let hidden_deaths=((*api).hook_step_order_hidden)(step_order);
    log(&format!("native bridge ready protocol=2 frame_hook={result} death_hook={deaths} hidden_death_hook={hidden_deaths} port=8795"));
}
