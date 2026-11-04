//! Session routing, domain lifetime and transactional IPC reply publication.
use super::{
    codec,
    fs::Filesystem,
    media::{AudioPacket, Media},
    nv::Nv,
    platform::Platform,
    types::*,
};
use crate::{
    kernel::{IpcRequest, Kernel, ObjectId},
    memory::GuestMemory,
};
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
struct Domain {
    next: u32,
    objects: BTreeMap<u32, ObjectId>,
}
pub struct Hub {
    platform: Platform,
    filesystem: Filesystem,
    nv: Nv,
    media: Media,
    domains: BTreeMap<ObjectId, Domain>,
    now_ns: u64,
}
const SERVICES: &[(&str, Family)] = &[
    ("set", Family::Platform),
    ("set:sys", Family::Platform),
    ("appletAE", Family::Platform),
    ("appletOE", Family::Platform),
    ("hid", Family::Platform),
    ("time:u", Family::Platform),
    ("time:s", Family::Platform),
    ("acc:u0", Family::Platform),
    ("acc:u1", Family::Platform),
    ("apm:p", Family::Platform),
    ("apm", Family::Platform),
    ("friend:u", Family::Platform),
    ("prepo:u", Family::Platform),
    ("arp:r", Family::Platform),
    ("pctl", Family::Platform),
    ("pctl:a", Family::Platform),
    ("pl:u", Family::Platform),
    ("bsd:s", Family::Platform),
    ("bsd:u", Family::Platform),
    ("ssl", Family::Platform),
    ("sfdnsres", Family::Platform),
    ("csrng", Family::Platform),
    ("nifm:u", Family::Platform),
    ("ns:am2", Family::Platform),
    ("psm", Family::Platform),
    ("fsp-srv", Family::Filesystem),
    ("nvdrv:a", Family::Nv),
    ("nvdrv", Family::Nv),
    ("vi:m", Family::Media),
    ("vi:u", Family::Media),
    ("audout:u", Family::Media),
    ("audren:u", Family::Media),
];
const SM: &[CommandSpec<Hub>] = &[
    CommandSpec {
        id: 0,
        schema: Schema::ANY,
        handler: Hub::sm_initialize,
    },
    CommandSpec {
        id: 1,
        schema: Schema::at_least(8),
        handler: Hub::sm_open,
    },
];
impl Hub {
    pub fn new(nro_path: &Path, home: &Path) -> Result<Self, Error> {
        let mut platform = Platform::new(home)?;
        let bytes = std::fs::read(nro_path)?;
        if let Some(control) = nro_control(&bytes) {
            platform.set_control(control)?;
        }
        drop(bytes);
        Ok(Self {
            platform,
            filesystem: Filesystem::new(nro_path, home)?,
            nv: Nv::default(),
            media: Media::default(),
            domains: BTreeMap::new(),
            now_ns: 0,
        })
    }
    pub fn new_program(program: &crate::program::Program, home: &Path) -> Result<Self, Error> {
        let mut platform = Platform::new(home)?;
        if let Some(control) = &program.control {
            platform.set_control(control.clone())?;
        }
        Ok(Self {
            platform,
            filesystem: Filesystem::new_program(program, home)?,
            nv: Nv::default(),
            media: Media::default(),
            domains: BTreeMap::new(),
            now_ns: 0,
        })
    }
    pub fn program_id(&self) -> u64 {
        self.filesystem.program_id()
    }
    pub fn set_buttons(&mut self, mask: u64) {
        self.platform.set_buttons(mask);
    }
    pub fn set_input(&mut self, input: soju_switch_input::NpadState) {
        self.platform.set_input(input);
    }
    pub fn exit_grace(&self) -> std::time::Duration {
        self.platform.exit_grace()
    }
    pub fn request_exit(&mut self, kernel: &mut Kernel) -> Result<bool, Error> {
        self.platform.request_exit(kernel)
    }
    pub fn frame_conversion_time(&self) -> std::time::Duration {
        self.media.frame_conversion_time
    }
    pub fn take_frame(&mut self) -> Option<Frame> {
        self.media.take_frame()
    }
    pub fn take_audio(&mut self) -> Option<AudioPacket> {
        self.media.take_audio()
    }
    pub fn audio_state(&self, object: ObjectId) -> Option<bool> {
        self.media.audio_state(object)
    }
    pub fn audio_running(&self, object: ObjectId) -> bool {
        self.media.audio_running(object)
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        self.filesystem.finish()
    }
    fn sm_initialize(&mut self, _req: &Request, _ctx: &mut Context<'_>) -> Result<Reply, Error> {
        Ok(Reply::success())
    }
    fn sm_open(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let raw = req.data.get(..8).ok_or(Error::Malformed("service name"))?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let name = std::str::from_utf8(&raw[..end])
            .map_err(|_| Error::Malformed("service name encoding"))?;
        let family = SERVICES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, f)| *f)
            .ok_or_else(|| Error::Message(format!("unsupported service {name}")))?;
        let handle = match family {
            Family::Platform => self.platform.open(name, ctx)?,
            Family::Filesystem => self.filesystem.open(name, ctx)?,
            Family::Nv => self.nv.open(name, ctx)?,
            Family::Media => self.media.open(name, ctx)?,
            Family::Sm => return Err(Error::Malformed("recursive SM registration")),
        };
        Ok(Reply::object(handle))
    }
    pub fn poll_services(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now_ns: u64,
    ) -> Result<(), Error> {
        self.now_ns = now_ns;
        self.cleanup(kernel, memory)?;
        self.platform.poll(kernel, memory, now_ns)?;
        self.nv.poll(kernel, memory, now_ns)?;
        self.media.poll(kernel, memory, now_ns)
    }
    fn cleanup(&mut self, kernel: &mut Kernel, memory: &mut dyn GuestMemory) -> Result<(), Error> {
        loop {
            let closed = kernel.take_closed_sessions();
            if closed.is_empty() {
                break;
            }
            for id in closed {
                if let Some(domain) = self.domains.remove(&id) {
                    for object in domain.objects.into_values().filter(|object| *object != id) {
                        kernel
                            .release_object(memory, object)
                            .map_err(Error::Kernel)?;
                    }
                }
                self.platform.close_object(id);
                self.filesystem.close_object(id);
                self.nv.close_object(id);
                self.media.close_object(id);
            }
        }
        Ok(())
    }
    pub fn dispatch_ipc(
        &mut self,
        ipc: &IpcRequest,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
    ) -> Result<(), Error> {
        self.cleanup(kernel, memory)?;
        let mut bytes = [0; 256];
        memory.read(ipc.tls, &mut bytes)?;
        let mut req = codec::decode(&bytes, self.domains.contains_key(&ipc.object))?;
        if req.kind == MessageKind::Close {
            kernel
                .close_handle(memory, ipc.session)
                .map_err(Error::Kernel)?;
            return self.cleanup(kernel, memory);
        }
        let domain_request = req.domain_target.is_some();
        let object = if let Some(target) = req.domain_target {
            *self
                .domains
                .get(&ipc.object)
                .and_then(|d| d.objects.get(&target))
                .ok_or(Error::Malformed("unknown domain target"))?
        } else {
            ipc.object
        };
        for id in &req.domain_objects {
            req.objects.push(
                *self
                    .domains
                    .get(&ipc.object)
                    .and_then(|d| d.objects.get(id))
                    .ok_or(Error::Malformed("unknown input domain object"))?,
            );
        }
        for handle in &req.copy_handles {
            if matches!(*handle, 0xffff8000 | 0xffff8001) {
                continue;
            }
            let id = kernel.object_id(*handle).map_err(Error::Kernel)?;
            if kernel.session_service(id).is_ok() {
                req.objects.push(id);
            }
        }
        for handle in &req.move_handles {
            let id = kernel.object_id(*handle).map_err(Error::Kernel)?;
            if kernel.session_service(id).is_ok() {
                req.objects.push(id);
            }
        }
        if req.kind == MessageKind::DomainClose {
            let target = req
                .domain_target
                .ok_or(Error::Malformed("domain close target"))?;
            let reply = Reply::success();
            let encoded = codec::encode(&req, &reply, false, &[])?;
            apply(
                memory,
                vec![MemoryWrite {
                    address: ipc.tls,
                    bytes: encoded,
                }],
            )
            .map_err(|f| f.error)?;
            self.domains
                .get_mut(&ipc.object)
                .ok_or(Error::Malformed("domain close session"))?
                .objects
                .remove(&target);
            if object != ipc.object {
                kernel
                    .release_object(memory, object)
                    .map_err(Error::Kernel)?;
            }
            return self.cleanup(kernel, memory);
        }
        let family = Family::decode(kernel.session_service(object).map_err(Error::Kernel)?)?;
        let mut convert = false;
        let mut reply = if req.kind == MessageKind::Control {
            match req.command_id {
                0 => {
                    if self.domains.contains_key(&ipc.object) {
                        return Err(Error::Malformed("session already domain"));
                    }
                    convert = true;
                    Reply::data(1u32.to_le_bytes().to_vec())
                }
                2 | 4 => {
                    let alias = kernel
                        .duplicate_handle(ipc.session)
                        .map_err(Error::Kernel)?;
                    Reply {
                        move_handles: vec![alias],
                        ..Reply::default()
                    }
                }
                3 => Reply::data(
                    if family == Family::Nv { 0u16 } else { 0x500u16 }
                        .to_le_bytes()
                        .to_vec(),
                ),
                _ => {
                    return Err(Error::Unsupported {
                        object,
                        command: req.command_id,
                    });
                }
            }
        } else {
            let mut ctx = Context {
                kernel,
                memory,
                object,
                session: ipc.session,
                client: ClientId(1),
                now_ns: self.now_ns,
                tls: ipc.tls,
                nv: None,
            };
            match family {
                Family::Sm => dispatch_table(self, SM, &req, &mut ctx),
                Family::Platform => self.platform.dispatch(&req, &mut ctx),
                Family::Filesystem => self.filesystem.dispatch(&req, &mut ctx),
                Family::Nv => self.nv.dispatch(&req, &mut ctx),
                Family::Media => {
                    ctx.nv = Some(&self.nv);
                    self.media.dispatch(&req, &mut ctx)
                }
            }
            .map_err(|error| {
                Error::Message(format!(
                    "{family:?} object {object} command {}: {error}",
                    req.command_id
                ))
            })?
        };
        let domain = domain_request && req.kind == MessageKind::Request;
        let mut owned = reply.move_handles.clone();
        owned.extend(&reply.objects);
        let mut retained = Vec::new();
        let mut new_objects = Vec::new();
        let transaction = reply.transaction;
        let mut rollback_failed = false;
        let outcome = (|| {
            for h in &mut reply.copy_handles {
                *h = kernel.duplicate_handle(*h).map_err(Error::Kernel)?;
                owned.push(*h);
            }
            let mut ids = Vec::new();
            if domain {
                let d = self
                    .domains
                    .get(&ipc.object)
                    .ok_or(Error::Malformed("domain disappeared"))?;
                let mut next = d.next;
                for handle in &reply.objects {
                    let id = kernel.retain_object(*handle).map_err(Error::Kernel)?;
                    retained.push(id);
                    next = next
                        .checked_add(1)
                        .ok_or(Error::Malformed("domain ID exhaustion"))?;
                    ids.push(next);
                    new_objects.push((next, id));
                }
            } else {
                reply.move_handles.extend(&reply.objects);
            }
            let encoded = codec::encode(&req, &reply, domain, &ids)?;
            let mut writes = std::mem::take(&mut reply.memory_writes);
            for write in &reply.writes {
                let buffer = req
                    .buffers
                    .iter()
                    .find(|b| b.index == write.buffer_index)
                    .ok_or(Error::Malformed("unknown output descriptor"))?;
                if !matches!(
                    buffer.kind,
                    BufferKind::Receive | BufferKind::Exchange | BufferKind::ReceiveList
                ) {
                    return Err(Error::Malformed("write to input descriptor"));
                }
                let len = u64::try_from(write.bytes.len())
                    .map_err(|_| Error::Malformed("output size"))?;
                if write
                    .offset
                    .checked_add(len)
                    .is_none_or(|end| end > buffer.len)
                {
                    return Err(Error::Malformed("output descriptor overflow"));
                }
                let address = buffer
                    .address
                    .checked_add(write.offset)
                    .ok_or(Error::Malformed("output address"))?;
                for previous in &writes {
                    if overlaps(
                        address,
                        write.bytes.len(),
                        previous.address,
                        previous.bytes.len(),
                    )? {
                        return Err(Error::Malformed("overlapping output descriptors"));
                    }
                }
                writes.push(MemoryWrite {
                    address,
                    bytes: write.bytes.clone(),
                });
            }
            for write in &writes {
                if overlaps(write.address, write.bytes.len(), ipc.tls, 256)? {
                    return Err(Error::Malformed("service output overlaps TLS"));
                }
            }
            writes.push(MemoryWrite {
                address: ipc.tls,
                bytes: encoded,
            });
            apply(memory, writes).map_err(|failure| {
                rollback_failed = failure.rollback_failed;
                failure.error
            })
        })();
        if let Some(token) = transaction {
            let mut ctx = Context {
                kernel,
                memory,
                object,
                session: ipc.session,
                client: ClientId(1),
                now_ns: self.now_ns,
                tls: ipc.tls,
                nv: None,
            };
            self.nv
                .finish_transaction(token, outcome.is_ok(), rollback_failed, &mut ctx)?;
        }
        if let Err(error) = outcome {
            for id in retained {
                kernel.release_object(memory, id).map_err(Error::Kernel)?;
            }
            for handle in owned {
                kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
            }
            self.cleanup(kernel, memory)?;
            return Err(error);
        }
        if convert {
            self.domains.insert(
                ipc.object,
                Domain {
                    next: 1,
                    objects: BTreeMap::from([(1, ipc.object)]),
                },
            );
        }
        if domain {
            let d = self
                .domains
                .get_mut(&ipc.object)
                .ok_or(Error::Malformed("domain disappeared"))?;
            for (id, object) in new_objects {
                d.next = id;
                d.objects.insert(id, object);
            }
            for handle in reply.objects {
                kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
            }
        }
        for handle in req.move_handles {
            kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
        }
        self.cleanup(kernel, memory)
    }
}
struct ApplyFailure {
    error: Error,
    rollback_failed: bool,
}
fn overlaps(a: u64, n: usize, b: u64, m: usize) -> Result<bool, Error> {
    let ae = a
        .checked_add(n as u64)
        .ok_or(Error::Malformed("write overflow"))?;
    let be = b
        .checked_add(m as u64)
        .ok_or(Error::Malformed("write overflow"))?;
    Ok(n != 0 && m != 0 && a < be && b < ae)
}
fn apply(memory: &mut dyn GuestMemory, writes: Vec<MemoryWrite>) -> Result<(), ApplyFailure> {
    let prepare = (|| {
        let mut undo = Vec::new();
        let mut total = 0usize;
        for write in &writes {
            total = total
                .checked_add(write.bytes.len())
                .ok_or(Error::Malformed("reply size overflow"))?;
            if total > MAX_BUFFER {
                return Err(Error::Malformed("reply write limit"));
            }
            memory.check_access(write.address, write.bytes.len(), 3)?;
            let mut bytes = vec![0; write.bytes.len()];
            memory.read(write.address, &mut bytes)?;
            undo.push(MemoryWrite {
                address: write.address,
                bytes,
            });
        }
        Ok(undo)
    })();
    let undo = prepare.map_err(|error| ApplyFailure {
        error,
        rollback_failed: false,
    })?;
    for (index, write) in writes.iter().enumerate() {
        if let Err(error) = memory.write(write.address, &write.bytes) {
            let mut rollback_failed = false;
            for old in undo[..=index].iter().rev() {
                if memory.write(old.address, &old.bytes).is_err() {
                    rollback_failed = true;
                }
            }
            return Err(ApplyFailure {
                error: error.into(),
                rollback_failed,
            });
        }
    }
    Ok(())
}
#[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
impl crate::runtime::IpcHandler for Hub {
    fn frame_ready(&self) -> bool {
        self.media.frame_ready()
    }
    fn dispatch(
        &mut self,
        request: &IpcRequest,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
    ) -> Result<crate::runtime::IpcReply, String> {
        self.dispatch_ipc(request, kernel, memory)
            .map_err(|e| e.to_string())?;
        Ok(crate::runtime::IpcReply::Complete(vec![0]))
    }
    fn poll(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now: u64,
    ) -> Result<(), String> {
        self.poll_services(kernel, memory, now)
            .map_err(|e| e.to_string())
    }
}

// Appended NRO assets are file data, never executable mappings.
fn nro_control(bytes: &[u8]) -> Option<Vec<u8>> {
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    let asset = word(0x18)?;
    if bytes.get(asset..asset.checked_add(4)?)? != b"ASET" {
        return None;
    }
    let wide =
        |at| usize::try_from(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?)).ok();
    let offset = wide(asset.checked_add(0x18)?)?;
    let len = wide(asset.checked_add(0x20)?)?;
    if len != 0x4000 {
        return None;
    }
    let start = asset.checked_add(offset)?;
    Some(bytes.get(start..start.checked_add(len)?)?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        kernel::MAIN_THREAD,
        memory::{Error as MemoryError, Mapping, MemoryImage, PAGE},
        nro::{BASE, Homebrew, Region, TLS},
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> (Hub, Kernel, MemoryImage, std::path::PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../dist/router-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let nro = root.join("synthetic.nro");
        std::fs::write(&nro, b"synthetic IPC fixture").unwrap();
        let hub = Hub::new(&nro, &root.join("home")).unwrap();
        let boot = Homebrew {
            regions: vec![
                Region {
                    address: BASE,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
                Region {
                    address: TLS,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
            ],
            pc: BASE,
            sp: BASE + PAGE as u64,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: TLS,
        };
        (
            hub,
            Kernel::new(&boot),
            MemoryImage::new(&boot).unwrap(),
            root,
        )
    }
    fn wire(kind: u32, cmd: u32, data: &[u8], domain: Option<u32>) -> Vec<u8> {
        let mut bytes = vec![0; 256];
        bytes[..4].copy_from_slice(&kind.to_le_bytes());
        let offset = if let Some(id) = domain {
            bytes[16] = 1;
            bytes[18..20].copy_from_slice(&((16 + data.len()) as u16).to_le_bytes());
            bytes[20..24].copy_from_slice(&id.to_le_bytes());
            32
        } else {
            16
        };
        bytes[offset..offset + 4].copy_from_slice(b"SFCI");
        bytes[offset + 8..offset + 12].copy_from_slice(&cmd.to_le_bytes());
        bytes[offset + 16..offset + 16 + data.len()].copy_from_slice(data);
        let words = (offset + 16 + data.len() - 8).div_ceil(4) as u32;
        bytes[4..8].copy_from_slice(&words.to_le_bytes());
        bytes
    }
    fn ipc(kernel: &Kernel, handle: u32) -> IpcRequest {
        IpcRequest {
            thread: MAIN_THREAD,
            session: handle,
            object: kernel.object_id(handle).unwrap(),
            service: 0,
            tls: TLS,
        }
    }
    #[test]
    fn domain_clone_child_lifetime_and_final_session_cleanup() {
        let (mut hub, mut kernel, mut memory, root) = fixture();
        let handle = kernel.register_session(0).unwrap();
        let request = ipc(&kernel, handle);
        memory.write(TLS, &wire(5, 0, &[], None)).unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        assert_eq!(hub.domains[&request.object].objects.len(), 1);
        memory
            .write(TLS, &wire(4, 1, b"set:sys\0", Some(1)))
            .unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        let child = hub.domains[&request.object].objects[&2];
        assert_eq!(
            kernel.session_service(child).unwrap(),
            Family::Platform.code()
        );
        memory.write(TLS, &wire(5, 2, &[], None)).unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        let mut response = [0; 256];
        memory.read(TLS, &mut response).unwrap();
        let alias = read_u32(&response, 12).unwrap();
        assert_ne!(alias, handle);
        assert_eq!(kernel.object_id(alias).unwrap(), request.object);
        kernel.close_handle(&mut memory, handle).unwrap();
        hub.poll_services(&mut kernel, &mut memory, 0).unwrap();
        assert!(kernel.session_service(child).is_ok());
        kernel.close_handle(&mut memory, alias).unwrap();
        hub.poll_services(&mut kernel, &mut memory, 0).unwrap();
        assert!(kernel.session_service(child).is_err());
        assert!(hub.domains.is_empty());
        drop(hub);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_tls_publication_releases_new_session_and_domain_reference() {
        let (mut hub, mut kernel, mut memory, root) = fixture();
        let handle = kernel.register_session(0).unwrap();
        let request = ipc(&kernel, handle);
        memory.write(TLS, &wire(5, 0, &[], None)).unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        memory
            .write(TLS, &wire(4, 1, b"set:sys\0", Some(1)))
            .unwrap();
        memory.protect(TLS, PAGE, 1).unwrap();
        assert!(
            hub.dispatch_ipc(&request, &mut kernel, &mut memory)
                .is_err()
        );
        assert_eq!(hub.domains[&request.object].objects.len(), 1);
        memory.protect(TLS, PAGE, 3).unwrap();
        memory
            .write(TLS, &wire(4, 1, b"set:sys\0", Some(1)))
            .unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        assert_eq!(hub.domains[&request.object].objects.len(), 2);
        drop(hub);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn process_copy_pseudo_handle_is_accepted_but_cannot_be_moved() {
        let (mut hub, mut kernel, mut memory, root) = fixture();
        let handle = kernel.register_session(0).unwrap();
        let request = ipc(&kernel, handle);
        let mut bytes = wire(4, 0, &[], None);
        let header = read_u32(&bytes, 4).unwrap() | (1 << 31);
        bytes[4..8].copy_from_slice(&header.to_le_bytes());
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&0xffff8001u32.to_le_bytes());
        memory.write(TLS, &bytes).unwrap();
        hub.dispatch_ipc(&request, &mut kernel, &mut memory)
            .unwrap();
        bytes[8..12].copy_from_slice(&32u32.to_le_bytes());
        memory.write(TLS, &bytes).unwrap();
        assert!(
            hub.dispatch_ipc(&request, &mut kernel, &mut memory)
                .is_err()
        );
        drop(hub);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn many_ordered_gpu_chunks_do_not_require_pairwise_overlap_checks() {
        let mut m = FaultMemory {
            bytes: [9; 32],
            writes: 0,
            fail: 0,
            rollback_fail: false,
        };
        let writes = (0..65_536)
            .map(|i| MemoryWrite {
                address: 0,
                bytes: vec![(i % 256) as u8],
            })
            .collect();
        assert!(apply(&mut m, writes).is_ok());
        assert_eq!(m.writes, 65_536);
        assert_eq!(m.bytes[0], 255);
    }
    struct FaultMemory {
        bytes: [u8; 32],
        writes: usize,
        fail: usize,
        rollback_fail: bool,
    }
    impl GuestMemory for FaultMemory {
        fn map_zeroed(&mut self, _: u64, _: usize, _: u8) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn unmap(&mut self, _: u64, _: usize) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn protect(&mut self, _: u64, _: usize, _: u8) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn alias(&mut self, _: u64, _: u64, _: usize, _: u8) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn attribute(&mut self, _: u64, _: usize, _: u32, _: u32) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn read(&self, a: u64, o: &mut [u8]) -> Result<(), MemoryError> {
            self.check_access(a, o.len(), 1)?;
            o.copy_from_slice(&self.bytes[a as usize..a as usize + o.len()]);
            Ok(())
        }
        fn write(&mut self, a: u64, b: &[u8]) -> Result<(), MemoryError> {
            self.check_access(a, b.len(), 2)?;
            self.writes += 1;
            if self.writes == self.fail || (self.rollback_fail && self.writes > self.fail) {
                return Err(MemoryError::Access);
            }
            self.bytes[a as usize..a as usize + b.len()].copy_from_slice(b);
            Ok(())
        }
        fn write_kernel(&mut self, a: u64, b: &[u8]) -> Result<(), MemoryError> {
            self.write(a, b)
        }
        fn check_access(&self, a: u64, n: usize, _: u8) -> Result<(), MemoryError> {
            if a.checked_add(n as u64).is_none_or(|end| end > 32) {
                Err(MemoryError::Access)
            } else {
                Ok(())
            }
        }
        fn set_state(&mut self, _: u64, _: usize, _: u32) -> Result<(), MemoryError> {
            Err(MemoryError::Access)
        }
        fn query(&self, _: u64) -> Result<(u64, u64, Option<Mapping>), MemoryError> {
            Err(MemoryError::Access)
        }
    }
    fn writes() -> Vec<MemoryWrite> {
        vec![
            MemoryWrite {
                address: 0,
                bytes: vec![1; 8],
            },
            MemoryWrite {
                address: 4,
                bytes: vec![2; 8],
            },
            MemoryWrite {
                address: 16,
                bytes: vec![3; 4],
            },
        ]
    }
    #[test]
    fn overlapping_gpu_writes_commit_in_order_and_rollback_restores_original_bytes() {
        let mut m = FaultMemory {
            bytes: [9; 32],
            writes: 0,
            fail: 0,
            rollback_fail: false,
        };
        assert!(apply(&mut m, writes()).is_ok());
        assert_eq!(&m.bytes[..12], &[1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2]);
        let mut m = FaultMemory {
            bytes: [9; 32],
            writes: 0,
            fail: 3,
            rollback_fail: false,
        };
        let error = apply(&mut m, writes()).err().unwrap();
        assert!(!error.rollback_failed);
        assert_eq!(m.bytes, [9; 32]);
        let mut m = FaultMemory {
            bytes: [9; 32],
            writes: 0,
            fail: 3,
            rollback_fail: true,
        };
        assert!(apply(&mut m, writes()).err().unwrap().rollback_failed);
    }
    #[test]
    fn reply_preflight_does_not_modify_any_memory_on_late_invalid_range() {
        let mut m = FaultMemory {
            bytes: [9; 32],
            writes: 0,
            fail: 0,
            rollback_fail: false,
        };
        let mut writes = writes();
        writes.push(MemoryWrite {
            address: 31,
            bytes: vec![4; 2],
        });
        assert!(apply(&mut m, writes).is_err());
        assert_eq!(m.bytes, [9; 32]);
        assert_eq!(m.writes, 0);
    }
}
