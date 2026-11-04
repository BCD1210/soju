//! Bounded Program-NCA loader. Format: https://switchbrew.org/wiki/NPDM
//! and https://switchbrew.org/wiki/NSO0. Unknown capabilities fail closed.
use crate::nro::{BASE, Homebrew, Region, STACK, TLS};
use soju_switch_fs::{
    keys::KeySet,
    nca::{ContentType, FsType, Nca},
    npdm, nso,
    pfs0::{Storage, Sub},
    title,
};

#[derive(Debug, Clone)]
pub struct Policy {
    pub syscalls: [bool; 192],
    pub lowest_priority: u32,
    pub highest_priority: u32,
    pub min_core: u32,
    pub max_core: u32,
}
pub struct Program {
    pub boot: Homebrew,
    pub metadata: npdm::Npdm,
    pub policy: Policy,
    pub modules: Vec<(String, u64)>,
    pub romfs: Vec<u8>,
    pub control: Option<Vec<u8>>,
}
fn fail(s: &str) -> String {
    s.into()
}
fn parse_policy(n: &npdm::Npdm) -> Result<Policy, String> {
    let mut policy = Policy {
        syscalls: [false; 192],
        lowest_priority: 63,
        highest_priority: 0,
        min_core: 0,
        max_core: 3,
    };
    let mut thread_info = false;
    let mut syscall_groups = [false; 8];
    for &cap in &n.kernel_caps {
        match cap.trailing_ones() {
            32 => {}
            3 if !thread_info => {
                thread_info = true;
                policy.lowest_priority = (cap >> 4) & 63;
                policy.highest_priority = (cap >> 10) & 63;
                policy.min_core = (cap >> 16) & 255;
                policy.max_core = cap >> 24;
            }
            4 => {
                let group = (cap >> 29) as usize;
                if syscall_groups[group] {
                    return Err(fail("duplicate syscall capability group"));
                }
                syscall_groups[group] = true;
                for bit in 0..24 {
                    policy.syscalls[group * 24 + bit] = cap & (1 << (5 + bit)) != 0;
                }
            }
            _ => {
                return Err(format!(
                    "unsupported kernel capability type {}",
                    cap.trailing_ones()
                ));
            }
        }
    }
    if !thread_info
        || policy.highest_priority > policy.lowest_priority
        || policy.max_core > 3
        || policy.min_core > policy.max_core
        || !(policy.highest_priority..=policy.lowest_priority)
            .contains(&u32::from(n.main_thread_priority))
        || !(policy.min_core..=policy.max_core).contains(&u32::from(n.main_thread_core))
    {
        return Err(fail("main thread violates ThreadInfo capability"));
    }
    Ok(policy)
}
impl Program {
    pub fn load(storage: &dyn Storage, extension: &str, keys: &KeySet) -> Result<Self, String> {
        let (keys, problems) =
            title::keys_with_tickets(storage, extension, keys).map_err(|e| e.to_string())?;
        if !problems.is_empty() {
            return Err(fail("unsupported or malformed container ticket"));
        }
        let entries = title::nca_entries(storage, extension).map_err(|e| e.to_string())?;
        let mut loaded = None;
        let mut controls = std::collections::BTreeMap::new();
        for entry in entries {
            let sub = Sub {
                inner: storage,
                offset: entry.offset,
                size: entry.size,
            };
            let nca = Nca::open(&sub, &keys).map_err(|e| e.to_string())?;
            if nca.header.content_type == ContentType::Control {
                for section in nca
                    .header
                    .sections
                    .iter()
                    .filter(|s| s.fs_type == FsType::RomFs)
                {
                    let storage = nca.section(section.index).map_err(|e| e.to_string())?;
                    for file in soju_switch_fs::romfs::list(&storage).map_err(|e| e.to_string())? {
                        if file.path == "/control.nacp" {
                            if file.size != 0x4000 {
                                return Err(fail("invalid control property size"));
                            }
                            let bytes = storage
                                .read_vec(file.offset, 0x4000)
                                .map_err(|e| e.to_string())?;
                            if controls.insert(nca.header.program_id, bytes).is_some() {
                                return Err(fail("duplicate application control property"));
                            }
                        }
                    }
                }
            }
            if nca.header.content_type != ContentType::Program {
                continue;
            }
            if loaded.is_some() {
                return Err(fail(
                    "multiple Program NCAs require explicit content selection",
                ));
            }
            loaded = Some(Self::from_nca(&nca)?);
        }
        let mut program = loaded.ok_or_else(|| fail("container has no Program NCA"))?;
        program.control = controls.remove(&program.metadata.program_id);
        Ok(program)
    }
    fn from_nca(nca: &Nca<'_>) -> Result<Self, String> {
        let sections: Vec<_> = nca
            .header
            .sections
            .iter()
            .filter(|s| s.fs_type == FsType::PartitionFs)
            .collect();
        if sections.len() != 1 {
            return Err(fail("expected one Program ExeFS"));
        }
        let index = sections[0].index;
        let files = nca.pfs0_files(index).map_err(|e| e.to_string())?;
        let section = nca.section(index).map_err(|e| e.to_string())?;
        let read = |name: &str| -> Result<Vec<u8>, String> {
            let file = files
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(|| format!("missing ExeFS {name}"))?;
            if file.size > 128 << 20 {
                return Err(fail("ExeFS file exceeds loader limit"));
            }
            section
                .read_vec(file.offset, file.size as usize)
                .map_err(|e| e.to_string())
        };
        let raw_metadata = read("main.npdm")?;
        let metadata = npdm::parse(&raw_metadata).map_err(|e| e.to_string())?;
        let aci = u32::from_le_bytes(raw_metadata[0x70..0x74].try_into().unwrap()) as usize;
        let cap_size = u32::from_le_bytes(raw_metadata[aci + 0x34..aci + 0x38].try_into().unwrap());
        if !cap_size.is_multiple_of(4) {
            return Err(fail("partial kernel capability descriptor"));
        }
        if !metadata.is_64bit || metadata.address_space != 3 {
            return Err(fail(
                "loader currently requires AArch64 with 39-bit address space",
            ));
        }
        if metadata.system_resource_size != 0
            || raw_metadata[0x0c] & 0xf0 != 0
            || raw_metadata[0x0d] != 0
        {
            return Err(fail("unsupported process resource or flag request"));
        }
        if metadata.program_id != nca.header.program_id {
            return Err(fail("NPDM and Program NCA identity mismatch"));
        }
        let stack = metadata.main_thread_stack_size as usize;
        if stack > 16 << 20 || !stack.is_multiple_of(4096) {
            return Err(fail("invalid main stack size"));
        }
        let policy = parse_policy(&metadata)?;
        let mut names = vec!["rtld".to_string(), "main".to_string()];
        for i in 0..10 {
            let name = format!("subsdk{i}");
            if files.iter().any(|f| f.name == name) {
                names.push(name);
            }
        }
        if files.iter().any(|f| f.name == "sdk") {
            names.push("sdk".into());
        }
        let mut regions = Vec::new();
        let mut modules = Vec::new();
        let mut base = BASE;
        for name in names {
            let raw = read(&name)?;
            if raw.len() < 0x100 || u32::from_le_bytes(raw[12..16].try_into().unwrap()) & !0x3f != 0
            {
                return Err(fail("unsupported NSO flags"));
            }
            let module = nso::parse(&raw).map_err(|e| e.to_string())?;
            if module.text.data.len() < 4 {
                return Err(fail("NSO has no entry instruction"));
            }
            let size = module.image_size();
            if size == 0
                || base
                    .checked_add(size)
                    .is_none_or(|end| end > BASE + (256 << 20))
            {
                return Err(fail("module image limit"));
            }
            modules.push((name, base));
            for (segment, permissions, bss) in [
                (&module.text, 5, 0),
                (&module.ro, 1, 0),
                (&module.data, 3, module.bss_size as usize),
            ] {
                let len = segment
                    .data
                    .len()
                    .checked_add(bss)
                    .ok_or_else(|| fail("segment overflow"))?
                    .div_ceil(4096)
                    * 4096;
                if len == 0 {
                    continue;
                }
                if !segment.memory_offset.is_multiple_of(4096) {
                    return Err(fail("NSO page alignment"));
                }
                let address = base + u64::from(segment.memory_offset);
                if regions.iter().any(|r: &Region| {
                    address < r.address + r.bytes.len() as u64 && r.address < address + len as u64
                }) {
                    return Err(fail("NSO rounded segments overlap"));
                }
                let mut bytes = segment.data.clone();
                bytes.resize(len, 0);
                regions.push(Region {
                    address,
                    bytes,
                    permissions,
                });
            }
            base += size;
        }
        if stack != 0 {
            regions.push(Region {
                address: STACK,
                bytes: vec![0; stack],
                permissions: 3,
            });
        }
        regions.push(Region {
            address: TLS,
            bytes: vec![0; 4096],
            permissions: 3,
        });
        let mut romfs = Vec::new();
        for s in nca
            .header
            .sections
            .iter()
            .filter(|s| s.fs_type == FsType::RomFs)
        {
            if !romfs.is_empty() {
                return Err(fail("multiple Program RomFS sections"));
            }
            let section = nca.section(s.index).map_err(|e| e.to_string())?;
            if section.len() > 256 << 20 {
                return Err(fail("RomFS exceeds skeleton limit"));
            }
            romfs = section
                .read_vec(0, section.len() as usize)
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            boot: Homebrew {
                regions,
                pc: BASE,
                sp: if stack == 0 { 0 } else { STACK + stack as u64 },
                x0: 0,
                x1: crate::kernel::MAIN_THREAD as u64,
                x30: 0,
                tpidrro_el0: TLS,
            },
            metadata,
            policy,
            modules,
            romfs,
            control: None,
        })
    }
}
