#![recursion_limit = "256"]
//! Runs inside the unmodified Blizzard StarCraft executable via Samase.
//! All game memory access and order execution happen on the game thread.
use std::{ffi::c_void, fs::OpenOptions, io::Write, net::UdpSocket,
    sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}};
use bw_dat::structs::{Game, Unit};
mod world;
mod production;
mod requirements;
mod buttonsets;
mod specialized;
mod spells;
mod commands;
mod terrain;
mod visuals;
use samase_plugin::{PluginApi, VarId};
use serde_json::{json, Value};

type Getter = unsafe extern "C" fn() -> *mut c_void;
type Order = unsafe extern "C" fn(*mut c_void, u32, u32, u32, *mut c_void, u32);
const VARS: [u16; 3] = [VarId::IsMultiplayer as u16, VarId::IsReplay as u16, VarId::LocalPlayerId as u16];
struct Runtime {
    requirements: Option<unsafe extern "C" fn(u32,u32)->*const u16>,
    game: Getter, units: Getter, hidden: Getter, bullets: Getter, issue: Order,
    client_selection: Option<Getter>,
    button_error_writable: bool,
    write: unsafe extern "C" fn(*const u16, *const usize, usize),
    lone_sprites: Option<Getter>,
    sprite_position: Option<visuals::SpritePosition>,
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
unsafe extern "C" fn frame() {
    let Some(r) = RUNTIME.get() else { return; };
    let mut vars = [usize::MAX; 3];
    (r.read)(VARS.as_ptr(), vars.as_mut_ptr(), vars.len());
    let game=(r.game)() as *const Game;
    if vars[0]==0 && vars[1]==0 && vars[2]<8 && !game.is_null() && (*game).map_width_tiles>0 {
        world::observe(r,&*game,&active_units(r),vars[2]);
    }
    let frame_unix_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64()*1000.0;
    let mut buffer = [0u8; 4096];
    // Bounded input work keeps the original simulation responsive.
    for _ in 0..16 {
        match r.socket.recv_from(&mut buffer) {
            Ok((size, addr)) => {
                if !addr.ip().is_loopback() { continue; }
                let mut result = match serde_json::from_slice::<Value>(&buffer[..size]) {
                    Ok(req) => commands::request(r, &req).unwrap_or_else(|e| json!({"ok":false,"error":e})),
                    Err(_) => json!({"ok":false,"error":"invalid JSON"})
                };
                result["nativeFrameUnixMs"] = json!(frame_unix_ms);
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
    let Some(tech_dat)=((*api).extended_dat)(4) else {log("tech DAT unavailable");return;};
    let dat=tech_dat(&mut dat_len);
    if dat.is_null() || dat_len<4 {log("tech DAT unavailable");return;}
    bw_dat::init_techdata(dat.cast(),dat_len);
    let Some(upgrades_dat)=((*api).extended_dat)(3) else {log("upgrades DAT unavailable");return;};
    let dat=upgrades_dat(&mut dat_len);
    if dat.is_null() || dat_len<6 {log("upgrades DAT unavailable");return;}
    bw_dat::init_upgrades(dat.cast(),dat_len);
    let Some(orders_dat)=((*api).extended_dat)(7) else {log("orders DAT unavailable");return;};
    let dat=orders_dat(&mut dat_len);
    if dat.is_null() || dat_len<15 {log("orders DAT unavailable");return;}
    bw_dat::init_orders(dat.cast(),dat_len);
    let mut supported = [0u8;3];
    ((*api).load_vars)(VARS.as_ptr(),supported.as_mut_ptr(),VARS.len());
    if supported.iter().any(|&x| x < 2) { log("required game variables unavailable"); return; }
    let Ok(token) = std::env::var("SOJU_BW_TOKEN") else { log("missing session token"); return; };
    if token.len() < 24 { log("invalid session token"); return; }
    let Ok(socket) = UdpSocket::bind("127.0.0.1:8795") else { log("bridge port unavailable"); return; };
    if socket.set_nonblocking(true).is_err() { log("nonblocking socket unavailable"); return; }
    let Some(tiles) = ((*api).map_tile_flags)() else { log("terrain flags unavailable"); return; };
    let error_id = VarId::DatRequirementError as u16;
    let mut error_support = 0u8;
    ((*api).load_vars)(&error_id, &mut error_support, 1);
    let runtime = Runtime {requirements:((*api).dat_requirements)(),game,units,hidden,bullets,issue,tiles,send,array,
        button_error_writable: error_support == 3,
        client_selection: ((*api).client_selection)(), write: (*api).write_vars,
        lone_sprites: ((*api).first_lone_sprite)(),
        sprite_position: ((*api).get_sprite_position)(),
        history:Mutex::new(world::History::default()),
        session:format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()),read:(*api).read_vars,socket,token,sequence:AtomicU64::new(0)};
    if RUNTIME.set(runtime).is_err() { return; }
    let result = ((*api).hook_step_objects)(frame,1);
    let deaths=((*api).hook_step_order)(step_order);
    let hidden_deaths=((*api).hook_step_order_hidden)(step_order);
    log(&format!("native bridge ready protocol=2 frame_hook={result} death_hook={deaths} hidden_death_hook={hidden_deaths} port=8795"));
}
