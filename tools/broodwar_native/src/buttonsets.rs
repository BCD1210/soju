//! Read the loaded client's original command-button records and evaluate their conditions on the game thread.
//! Layout and locator: neivv/samase_scarf f1681dbac8c7d1ac184dc84918ab7dd5771061ed,
//! src/firegraft.rs and src/struct_layouts.rs. No button content is embedded here.
use serde_json::Value;
#[cfg(windows)]
use serde_json::json;

#[cfg(any(windows, test))]
fn integer<const N: usize>(bytes: &[u8], offset: usize) -> Option<[u8; N]> {
    bytes.get(offset..offset.checked_add(N)?)?.try_into().ok()
}

#[cfg(any(windows, test))]
fn dword(bytes: &[u8], offset: usize) -> Option<u32> {
    integer(bytes, offset).map(u32::from_le_bytes)
}

#[cfg(any(windows, test))]
fn locate(data: &[u8]) -> Result<usize, &'static str> {
    let signature = [6, 9, 6, 5, 0, 7, 0, 9, 7, 8, 6, 7, 6];
    let mut found = None;
    for offset in (0..data.len().saturating_sub(signature.len() * 24)).step_by(4) {
        if signature.iter().enumerate().all(|(index, count)| {
            dword(data, offset + index * 24) == Some(*count)
                && dword(data, offset + index * 24 + 16) == Some(0xffff)
        }) {
            if found.is_some() { return Err("ambiguous original button table"); }
            found = Some(offset);
        }
    }
    found.ok_or("original button table not found")
}

#[cfg(windows)]
mod memory {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *const c_void;
        fn GetCurrentProcess() -> *mut c_void;
        fn ReadProcessMemory(process: *mut c_void, source: *const c_void,
            destination: *mut c_void, size: usize, read: *mut usize) -> i32;
    }
    pub fn base() -> usize {
        // SAFETY: null requests the executable module; no borrowed buffer is passed.
        unsafe { GetModuleHandleW(std::ptr::null()) }.expose_provenance()
    }
    pub fn read(address: usize, size: usize) -> Result<Vec<u8>, &'static str> {
        if address == 0 || size > 64 * 1024 * 1024 { return Err("invalid memory range"); }
        let mut bytes = vec![0; size];
        let mut count = 0;
        // SAFETY: GetCurrentProcess takes no arguments and returns a pseudo handle.
        let process = unsafe { GetCurrentProcess() };
        // SAFETY: Windows validates the source; destination/count are writable for
        // the supplied lengths. The foreign address is never dereferenced by Rust.
        let ok = unsafe { ReadProcessMemory(process, std::ptr::with_exposed_provenance(address),
            bytes.as_mut_ptr().cast(), bytes.len(), &mut count) };
        if ok == 0 || count != size { return Err("original button memory unreadable"); }
        Ok(bytes)
    }
}

#[cfg(windows)]
pub fn snapshot() -> Result<Value, &'static str> {
    let base = memory::base();
    let dos = memory::read(base, 64)?;
    if dos.get(..2) != Some(b"MZ") { return Err("invalid executable header"); }
    let pe = usize::try_from(dword(&dos, 60).ok_or("missing PE offset")?).map_err(|_| "PE offset overflow")?;
    if pe > 1024 * 1024 { return Err("PE offset outside headers"); }
    let header = memory::read(base.checked_add(pe).ok_or("PE address overflow")?, 264)?;
    if header.get(..4) != Some(b"PE\0\0") || integer(&header, 24) != Some(0x20bu16.to_le_bytes()) {
        return Err("64-bit PE required");
    }
    let sections = usize::from(u16::from_le_bytes(integer(&header, 6).ok_or("missing section count")?));
    let optional = usize::from(u16::from_le_bytes(integer(&header, 20).ok_or("missing optional header")?));
    if sections > 96 { return Err("too many executable sections"); }
    let table = memory::read(base.checked_add(pe + 24 + optional).ok_or("section address overflow")?, sections * 40)?;
    let code = table.chunks_exact(40).find(|row| row.get(..8) == Some(b".text\0\0\0")).ok_or("text section missing")?;
    let code_start = base + dword(code, 12).ok_or("missing code RVA")? as usize;
    let code_end = code_start + dword(code, 8).ok_or("missing code size")? as usize;
    let section = table.chunks_exact(40).find(|row| row.get(..8) == Some(b".data\0\0\0")).ok_or("data section missing")?;
    let size = usize::try_from(dword(section, 8).ok_or("missing section size")?).map_err(|_| "section size overflow")?;
    let rva = usize::try_from(dword(section, 12).ok_or("missing section RVA")?).map_err(|_| "section RVA overflow")?;
    let data = memory::read(base.checked_add(rva).ok_or("data address overflow")?, size)?;
    let offset = locate(&data)?;
    let mut sets = Vec::with_capacity(250);
    for id in 0..250 {
        let row = data.get(offset + id * 24..offset + (id + 1) * 24).ok_or("truncated button table")?;
        let count = usize::try_from(dword(row, 0).ok_or("missing button count")?).map_err(|_| "button count overflow")?;
        if count > 64 { return Err("invalid button count"); }
        let address = usize::try_from(u64::from_le_bytes(integer(row, 8).ok_or("missing button pointer")?)).map_err(|_| "button pointer overflow")?;
        let records = if count == 0 { Vec::new() } else { memory::read(address, count * 32)? };
        let mut buttons = Vec::with_capacity(count);
        for button in records.chunks_exact(32) {
            let word = |offset| integer(button, offset).map(u16::from_le_bytes).ok_or("truncated button");
            let pointer = |offset| integer(button, offset).map(u64::from_le_bytes).ok_or("truncated callback");
            buttons.push(json!({"position":word(0)?,"icon":word(2)?,
                "conditionAddress":pointer(8)?,"actionAddress":pointer(16)?,
                "conditionParameter":word(24)?,"actionParameter":word(26)?,
                "enabledString":word(28)?,"disabledString":word(30)?}));
        }
        sets.push(json!({"type":id,"linked":dword(row,16),"recordsAddress":address,"buttons":buttons}));
    }
    Ok(json!({"ok":true,"source":"loaded-original-client-buttonsets","tableRva":rva+offset,
        "sets":sets,"evaluated":false,"codeStart":code_start,"codeEnd":code_end}))
}

#[cfg(not(windows))]
pub fn snapshot() -> Result<Value, &'static str> {
    Err("original button tables require the Windows game process")
}

// Shared selection rules: BoomerangAide/GPTP ce321f0fa83174aee741b91f1b2eaec30300773e,
// GPTP/hooks/interface/buttonsets.cpp, updateButtonSetEx.
#[cfg(windows)]
unsafe fn root_set(sets: &[Value], units: &[*mut bw_dat::structs::Unit]) -> Result<usize, &'static str> {
    let mut active = (*units[0]).buttons as usize;
    if active >= sets.len() { return Err("invalid current button set"); }
    let mut mixed = None;
    let mut workers = 0;
    let mut cloakers = 0;
    let mut ground_zerg = 0;
    let mut burrowers = 0;
    for &ptr in units {
        let unit = &*ptr;
        let kind = bw_dat::UnitId(unit.unit_id);
        workers += usize::from(kind.is_worker());
        cloakers += usize::from(kind.flags() & 0x200 != 0);
        burrowers += usize::from(kind.flags() & 0x0010_0000 != 0);
        ground_zerg += usize::from(unit.flags & 4 == 0 && kind.group_flags() & 1 != 0);
        let selected = unit.buttons as usize;
        if selected >= sets.len() { return Err("invalid selected button set"); }
        if selected != active && sets[selected]["recordsAddress"] != sets[active]["recordsAddress"] && sets[selected]["linked"].as_u64() != Some(active as u64) {
            if sets[active]["linked"].as_u64() == Some(selected as u64) {
                active = selected;
                mixed = Some(selected);
            } else { mixed = Some(244); }
        }
    }
    if let Some(mut result) = mixed {
        if workers == units.len() { result = 245; }
        else if cloakers == units.len() { result = 246; }
        else if burrowers != 0 && ground_zerg == units.len() { result = 247; }
        Ok(result)
    } else { Ok(active) }
}

#[cfg(windows)]
struct SelectionScope<'a> {
    runtime: &'a super::Runtime,
    array: *mut *mut bw_dat::structs::Unit,
    count: *mut u8,
    saved: [*mut bw_dat::structs::Unit; 12],
    saved_count: u8,
    error: usize,
}
#[cfg(windows)]
impl Drop for SelectionScope<'_> {
    fn drop(&mut self) {
        // SAFETY: the validated client selection storage is stable on the game
        // thread; restore every borrowed global before returning to the client.
        unsafe {
            std::ptr::copy_nonoverlapping(self.saved.as_ptr(), self.array, 12);
            self.count.write(self.saved_count);
            let id = samase_plugin::VarId::DatRequirementError as u16;
            (self.runtime.write)(&id, &self.error, 1);
        }
    }
}

#[cfg(windows)]
pub(super) unsafe fn evaluate(r: &super::Runtime, units: &[*mut bw_dat::structs::Unit], player: usize, menu: Option<u64>) -> Result<Value, &'static str> {
    if !r.button_error_writable { return Err("button requirement error variable unavailable"); }
    static TABLE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    if TABLE.get().is_none() { let _ = TABLE.set(snapshot()?); }
    let table = TABLE.get().ok_or("button table unavailable")?;
    let sets = table["sets"].as_array().ok_or("missing sets")?;
    let root = root_set(sets, units)?;
    let submenu_action = sets[7]["buttons"].as_array().ok_or("missing worker buttons")?.iter()
        .find(|b| b["actionParameter"] == 239).ok_or("missing build submenu")?["actionAddress"].clone();
    let mut chosen = root;
    if let Some(menu) = menu.filter(|&x| x != 228) {
        if menu >= sets.len() as u64 || !sets[root]["buttons"].as_array().ok_or("missing root buttons")?.iter()
            .any(|b| b["actionAddress"] == submenu_action && b["actionParameter"].as_u64() == Some(menu)) {
            return Err("submenu is not linked from selection");
        }
        chosen = menu as usize;
    }
    let getter = r.client_selection.ok_or("client selection unavailable")?;
    let array = getter() as *mut *mut bw_dat::structs::Unit;
    let selection = memory::read(array as usize, 12 * 8)?;
    // Resolve the count from the original train condition's RIP-relative byte
    // comparison, then require it to be adjacent to the API's selection array.
    let train = sets[106]["buttons"].as_array().ok_or("missing producer buttons")?.iter()
        .find(|b| b["actionParameter"] == 7).ok_or("missing train condition")?;
    let address = train["conditionAddress"].as_u64().ok_or("missing condition")? as usize;
    let code_start = table["codeStart"].as_u64().ok_or("missing code bounds")? as usize;
    let code_end = table["codeEnd"].as_u64().ok_or("missing code bounds")? as usize;
    if address < code_start || address + 64 > code_end { return Err("condition outside executable code"); }
    let code = memory::read(address, 64)?;
    let candidates: Vec<_> = code.windows(7).enumerate().filter_map(|(i,b)| {
        if b[..2] != [0x80,0x3d] || b[6] != 1 { return None; }
        let displacement = i32::from_le_bytes(b[2..6].try_into().ok()?);
        let target = (address + i + 7).checked_add_signed(displacement as isize)?;
        if target < array as usize && (array as usize) - target <= 32 { Some(target) } else { None }
    }).collect();
    if candidates.len() != 1 { return Err("ambiguous client selection count"); }
    let count = candidates[0] as *mut u8;
    let saved_count = memory::read(count as usize, 1)?[0];
    let saved: [*mut bw_dat::structs::Unit; 12] = std::array::from_fn(|i| {
        std::ptr::with_exposed_provenance_mut(u64::from_le_bytes(selection[i*8..i*8+8].try_into().unwrap()) as usize)
    });
    if saved_count as usize != saved.iter().filter(|p| !p.is_null()).count() { return Err("selection count layout mismatch"); }
    let id = samase_plugin::VarId::DatRequirementError as u16;
    let mut error = 0;
    (r.read)(&id, &mut error, 1);
    let _scope = SelectionScope { runtime:r, array, count, saved, saved_count, error };
    std::ptr::write_bytes(array, 0, 12);
    std::ptr::copy_nonoverlapping(units.as_ptr(), array, units.len());
    count.write(units.len() as u8);
    let mut evaluated = Vec::new();
    for (index, button) in sets[chosen]["buttons"].as_array().ok_or("missing chosen buttons")?.iter().enumerate() {
        let pointer = button["conditionAddress"].as_u64().ok_or("missing callback")? as usize;
        if pointer < code_start || pointer >= code_end { return Err("callback outside executable code"); }
        // SAFETY: callback comes from the validated original button table in
        // executable .text, uses the client's x64 ABI, and runs with live units.
        let condition: unsafe extern "C" fn(u32,u32,*mut bw_dat::structs::Unit)->i32 = std::mem::transmute(pointer);
        let state = condition(button["conditionParameter"].as_u64().ok_or("missing parameter")? as u8 as u32, player as u32, units[0]);
        if ![-1,0,1].contains(&state) { return Err("unexpected original button state"); }
        let mut row = button.clone();
        row["conditionState"] = json!(state);
        row["family"] = json!(action_family(sets, &button["actionAddress"])?);
        row["recordIndex"] = json!(index);
        evaluated.push(row);
    }
    drop(_scope);
    if memory::read(array as usize, 96)? != selection || memory::read(count as usize, 1)?[0] != saved_count { return Err("client selection restoration failed"); }
    Ok(json!({"ok":true,"evaluated":true,"selectionRestored":true,"source":"loaded-original-client-button-conditions",
        "rootSet":root,"buttonSet":chosen,"selectionCount":units.len(),"frame":(*(r.game)().cast::<bw_dat::structs::Game>()).frame_count,
        "buttons":evaluated}))
}

#[cfg(not(windows))]
pub(super) unsafe fn evaluate(_: &super::Runtime, _: &[*mut bw_dat::structs::Unit], _: usize, _: Option<u64>) -> Result<Value, &'static str> {
    Err("original button conditions require the Windows game process")
}

// Representative records identify callback families in the loaded executable;
// no executable addresses or original button contents are compiled into the bridge.
#[cfg(windows)]
fn action_family(sets: &[Value], address: &Value) -> Result<&'static str, &'static str> {
    const REFERENCES: &[(usize, usize, &str)] = &[
        (0,0,"move"),(0,1,"stop"),(0,2,"attack_move"),(0,3,"patrol"),(0,4,"hold"),
        (0,5,"stim"),(1,5,"cloak"),(1,6,"decloak"),(1,7,"tech"),(1,8,"nuclear_strike"),
        (5,5,"siege"),(5,6,"unsiege"),(7,3,"repair"),(7,4,"gather"),(7,5,"return_cargo"),
        (7,6,"submenu"),(11,5,"load"),(11,6,"unload"),(34,4,"heal"),(35,0,"morph"),
        (36,0,"cancel_morph"),(37,5,"burrow"),(37,6,"unburrow"),(50,2,"attack_move"),
        (61,6,"merge_dark_archon"),(67,7,"merge_archon"),(72,0,"move"),(72,1,"stop"),
        (72,2,"attack_move"),(72,5,"train_ammunition"),(72,6,"cancel_train"),
        (81,1,"stop"),(81,2,"attack_move"),(106,1,"train"),(106,3,"rally"),
        (106,4,"addon"),(106,6,"land"),(106,7,"lift"),(106,9,"cancel_addon"),
        (112,0,"upgrade"),(112,1,"research"),(112,5,"cancel_research"),(112,6,"cancel_upgrade"),
        (124,1,"attack_move"),(131,0,"select_larvae"),(131,3,"morph"),(134,0,"nydus_exit"),
        (172,0,"recharge_shields"),(197,0,"place_cop"),(229,0,"cancel_target"),
        (230,0,"cancel_target"),(231,0,"cancel_construction"),(235,0,"cancel_infestation"),
        (237,0,"cancel_nuke"),(238,0,"build"),(239,0,"build"),(240,0,"build"),
        (248,0,"replay_slow"),(248,1,"replay_play"),(248,2,"replay_fast"),
    ];
    REFERENCES.iter().find_map(|&(set,index,name)|
        (sets.get(set)?["buttons"].get(index)?["actionAddress"] == *address).then_some(name))
        .ok_or("unknown original action callback")
}
