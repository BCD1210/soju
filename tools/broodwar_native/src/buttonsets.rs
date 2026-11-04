//! Read the loaded client's original command-button records without executing them.
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
        sets.push(json!({"type":id,"linked":dword(row,16),"buttons":buttons}));
    }
    Ok(json!({"ok":true,"source":"loaded-original-client-buttonsets","tableRva":rva+offset,
        "sets":sets,"evaluated":false}))
}

#[cfg(not(windows))]
pub fn snapshot() -> Result<Value, &'static str> {
    Err("original button tables require the Windows game process")
}
