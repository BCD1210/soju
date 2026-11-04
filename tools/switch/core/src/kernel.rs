//! Clean-room non-IPC Horizon SVC subset for homebrew. Time and dispatch are
//! explicit inputs, so scheduling and synchronization can be tested without HV.
use crate::memory::{self, GuestMemory, PAGE};
use crate::nro::{BASE, EXCEPTION_CONTEXT, Homebrew, LOADER_RETURN, STACK, STACK_SIZE};
use std::collections::{BTreeMap, VecDeque};
#[path = "kernel_threads.rs"]
mod threads;
#[path = "kernel_virtual_memory.rs"]
mod virtual_memory;

pub type Handle = u32;
pub type ThreadId = u32;
pub type ObjectId = u64;
pub const MAIN_THREAD: ThreadId = 0x101;
pub const PROCESS_HANDLE: Handle = 0x102;
pub const CURRENT_THREAD: Handle = 0xffff8000;
pub const HEAP: u64 = 0x400000000;
pub const HEAP_LIMIT: usize = 3 * 1024 * 1024 * 1024;
pub const SUCCESS: u64 = 0;
pub const INVALID_SIZE: u64 = 0xca01;
pub const INVALID_ADDRESS: u64 = 0xcc01;
pub const INVALID_HANDLE: u64 = 0xe401;
pub const INVALID_MEMORY_STATE: u64 = 0xd401;
pub const INVALID_STATE: u64 = 0xfa01;
pub const INVALID_ENUM: u64 = 0xf001;
pub const TIMED_OUT: u64 = 0xea01;
pub const CANCELLED: u64 = 0xec01;
pub const INVALID_COMBINATION: u64 = 0xe801;
pub const INVALID_PRIORITY: u64 = 0xe001;
pub const LIMIT_REACHED: u64 = 0x10801;
pub const INVALID_CURRENT_MEMORY: u64 = 0xd401;
const WAITERS: u32 = 0x40000000;
const MAX_THREADS: usize = 128;
const MAX_HANDLES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadSpec {
    pub entry: u64,
    pub argument: u64,
    pub stack: u64,
    pub tls: u64,
    pub link: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wait {
    Sleep,
    Objects(Vec<ObjectId>),
    Mutex {
        address: u64,
        owner: ThreadId,
        result: u64,
    },
    Condition {
        key: u64,
        mutex: u64,
    },
    Address(u64),
    Ipc,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadStatus {
    Suspended,
    Runnable,
    Waiting(Wait),
    Exited,
}
#[derive(Clone, Debug)]
pub struct Thread {
    pub spec: ThreadSpec,
    pub status: ThreadStatus,
    pub priority: u32,
    pub ideal_core: i32,
    pub affinity: u64,
    pub deadline: Option<u64>,
    pub pending: Option<Vec<u64>>,
    cancelled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpcRequest {
    pub thread: ThreadId,
    pub session: Handle,
    pub object: ObjectId,
    pub service: u64,
    pub tls: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    Return(Vec<u64>),
    Blocked,
    ThreadExited,
    ProcessExited(u32),
    Break {
        reason: u32,
        address: u64,
        size: u64,
    },
    Ipc(IpcRequest),
    Unsupported(u16),
}
#[derive(Clone, Debug)]
enum Object {
    Process,
    Thread(ThreadId),
    Event(bool),
    Session(u64),
    Shared {
        address: u64,
        size: usize,
        permissions: u8,
    },
    Code {
        address: u64,
        size: usize,
        owner: Option<u64>,
        slave: Option<u64>,
    },
    Transfer {
        address: u64,
        size: usize,
    },
}
#[derive(Clone, Debug)]
struct Entry {
    references: usize,
    object: Object,
}
#[derive(Debug)]
pub struct Kernel {
    policy: Option<crate::program::Policy>,
    stack_size: usize,
    threads: BTreeMap<ThreadId, Thread>,
    ready: VecDeque<ThreadId>,
    handles: BTreeMap<Handle, ObjectId>,
    objects: BTreeMap<ObjectId, Entry>,
    next_handle: Handle,
    next_object: ObjectId,
    ports: BTreeMap<Vec<u8>, u64>,
    aliases: BTreeMap<(u64, u64), usize>,
    physical_mappings: BTreeMap<u64, usize>,
    code_mappings: BTreeMap<(u64, u64), usize>,
    shared_mappings: BTreeMap<(ObjectId, u64), usize>,
    next_shared: u64,
    heap_size: usize,
    image_size: usize,
    pub program_id: u64,
    debug_output: Vec<Vec<u8>>,
    debug_bytes: usize,
    closed_sessions: Vec<ObjectId>,
    deferred_releases: Vec<ObjectId>,
    seed: [u64; 4],
}
fn ret(code: u64) -> Dispatch {
    Dispatch::Return(vec![code])
}
fn out(value: u64) -> Dispatch {
    Dispatch::Return(vec![0, value])
}
fn memory_result(result: Result<(), memory::Error>) -> Dispatch {
    ret(result.err().map_or(0, |e| match e {
        memory::Error::Alignment => INVALID_ADDRESS,
        memory::Error::Address => INVALID_ADDRESS,
        memory::Error::Capacity => LIMIT_REACHED,
        memory::Error::Permission => INVALID_COMBINATION,
        _ => INVALID_MEMORY_STATE,
    }))
}
fn word(memory: &dyn GuestMemory, address: u64) -> Result<u32, u64> {
    if !address.is_multiple_of(4) {
        return Err(INVALID_ADDRESS);
    }
    let mut bytes = [0; 4];
    memory
        .read(address, &mut bytes)
        .map_err(|_| INVALID_CURRENT_MEMORY)?;
    Ok(u32::from_le_bytes(bytes))
}
fn put_word(memory: &mut dyn GuestMemory, address: u64, value: u32) -> Result<(), u64> {
    memory
        .write(address, &value.to_le_bytes())
        .map_err(|_| INVALID_CURRENT_MEMORY)
}
fn deadline(now: u64, timeout: u64) -> Option<u64> {
    if timeout as i64 >= 0 {
        Some(now.saturating_add(timeout))
    } else {
        None
    }
}
impl Kernel {
    pub fn configure_program(&mut self, program: &crate::program::Program) {
        self.program_id = program.metadata.program_id;
        self.stack_size = program.metadata.main_thread_stack_size as usize;
        self.policy = Some(program.policy.clone());
        let main = self.threads.get_mut(&MAIN_THREAD).expect("main thread");
        main.priority = program.metadata.main_thread_priority.into();
        main.ideal_core = program.metadata.main_thread_core.into();
        main.affinity = 1 << program.metadata.main_thread_core;
    }
    pub fn new(boot: &Homebrew) -> Self {
        let main = Thread {
            spec: ThreadSpec {
                entry: boot.pc,
                argument: boot.x0,
                stack: boot.sp,
                tls: boot.tpidrro_el0,
                link: boot.x30,
            },
            status: ThreadStatus::Runnable,
            priority: 44,
            ideal_core: 0,
            affinity: 1,
            deadline: None,
            pending: None,
            cancelled: false,
        };
        let image_size = boot
            .regions
            .iter()
            .filter(|r| r.address >= BASE && r.address < STACK)
            .map(|r| r.bytes.len())
            .sum();
        let mut kernel = Self {
            policy: None,
            stack_size: STACK_SIZE,
            threads: BTreeMap::from([(MAIN_THREAD, main)]),
            ready: VecDeque::from([MAIN_THREAD]),
            handles: BTreeMap::new(),
            objects: BTreeMap::new(),
            next_handle: 0x500,
            next_object: 1,
            ports: BTreeMap::from([(b"sm:".to_vec(), 0)]),
            aliases: BTreeMap::new(),
            physical_mappings: BTreeMap::new(),
            code_mappings: BTreeMap::new(),
            shared_mappings: BTreeMap::new(),
            next_shared: 0x7000000000,
            heap_size: 0,
            image_size,
            program_id: 0,
            debug_output: Vec::new(),
            debug_bytes: 0,
            closed_sessions: Vec::new(),
            deferred_releases: Vec::new(),
            seed: [
                0x12a34b56c78d9012,
                0x91e8d7c6b5a40312,
                0x3c2d1e0f11223344,
                0xaabbccdd12345678,
            ],
        };
        let object = kernel
            .object(Object::Thread(MAIN_THREAD))
            .expect("initial object");
        kernel.handles.insert(MAIN_THREAD, object);
        let object = kernel
            .object(Object::Process)
            .expect("initial process object");
        kernel.handles.insert(PROCESS_HANDLE, object);
        kernel
    }
    fn allocation_available(&self) -> Result<(), u64> {
        if self.handles.len() >= MAX_HANDLES
            || self.objects.len() >= MAX_HANDLES
            || self.next_handle >= 0x3fffffff
            || self.next_object == u64::MAX
        {
            Err(LIMIT_REACHED)
        } else {
            Ok(())
        }
    }
    fn object(&mut self, object: Object) -> Result<ObjectId, u64> {
        self.allocation_available()?;
        let id = self.next_object;
        self.next_object += 1;
        self.objects.insert(
            id,
            Entry {
                references: if matches!(object, Object::Thread(_)) {
                    2
                } else {
                    1
                },
                object,
            },
        );
        Ok(id)
    }
    fn handle(&mut self, object: Object) -> Result<Handle, u64> {
        let id = self.object(object)?;
        let handle = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(handle, id);
        Ok(handle)
    }
    pub fn object_id(&self, handle: Handle) -> Result<ObjectId, u64> {
        self.handles.get(&handle).copied().ok_or(INVALID_HANDLE)
    }
    pub fn session_service(&self, id: ObjectId) -> Result<u64, u64> {
        match self.objects.get(&id).map(|e| &e.object) {
            Some(Object::Session(service)) => Ok(*service),
            _ => Err(INVALID_HANDLE),
        }
    }
    pub fn close_handle(
        &mut self,
        memory: &mut dyn GuestMemory,
        handle: Handle,
    ) -> Result<(), u64> {
        let id = self.object_id(handle)?;
        self.release_object(memory, id)?;
        self.handles.remove(&handle);
        Ok(())
    }
    fn lookup(&self, handle: Handle) -> Option<&Object> {
        self.objects
            .get(self.handles.get(&handle)?)
            .map(|e| &e.object)
    }
    pub fn thread(&self, id: ThreadId) -> Option<&Thread> {
        self.threads.get(&id)
    }
    pub fn threads(&self) -> impl Iterator<Item = (ThreadId, &Thread)> {
        self.threads.iter().map(|(id, t)| (*id, t))
    }
    fn resolve_thread(&self, handle: Handle, current: ThreadId) -> Option<ThreadId> {
        if handle == CURRENT_THREAD {
            return self.threads.contains_key(&current).then_some(current);
        }
        match self.lookup(handle) {
            Some(Object::Thread(id)) => Some(*id),
            _ => None,
        }
    }
    pub fn register_port(&mut self, name: &[u8], service: u64) -> Result<(), u64> {
        if name.is_empty() || name.len() > 11 || name.contains(&0) {
            return Err(INVALID_SIZE);
        }
        self.ports.insert(name.to_vec(), service);
        Ok(())
    }
    pub fn register_session(&mut self, service: u64) -> Result<Handle, u64> {
        self.handle(Object::Session(service))
    }
    pub fn register_event(&mut self, signaled: bool) -> Result<Handle, u64> {
        self.handle(Object::Event(signaled))
    }
    pub fn duplicate_handle(&mut self, handle: Handle) -> Result<Handle, u64> {
        self.allocation_available()?;
        let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
        self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?.references += 1;
        let alias = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(alias, id);
        Ok(alias)
    }
    pub fn retain_object(&mut self, handle: Handle) -> Result<ObjectId, u64> {
        let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
        self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?.references += 1;
        Ok(id)
    }
    pub fn release_object(
        &mut self,
        memory: &mut dyn GuestMemory,
        id: ObjectId,
    ) -> Result<(), u64> {
        let entry = self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?;
        if entry.references > 1 {
            entry.references -= 1;
            return Ok(());
        }
        match entry.object {
            Object::Code {
                address,
                size,
                owner,
                slave,
            } => {
                for mapped in [owner, slave].into_iter().flatten() {
                    memory
                        .unmap(mapped, size)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                }
                memory
                    .protect(address, size, 3)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
            }
            Object::Transfer { address, size } => memory
                .protect(address, size, 3)
                .map_err(|_| INVALID_MEMORY_STATE)?,
            Object::Shared { address, size, .. } => memory
                .unmap(address, size)
                .map_err(|_| INVALID_MEMORY_STATE)?,
            Object::Session(_) => {
                if self.closed_sessions.len() >= MAX_HANDLES {
                    return Err(LIMIT_REACHED);
                }
                self.closed_sessions.push(id);
            }
            Object::Thread(thread) => {
                if let Some(t) = self.threads.get(&thread) {
                    if thread != MAIN_THREAD {
                        memory
                            .unmap(t.spec.tls, PAGE)
                            .map_err(|_| INVALID_MEMORY_STATE)?;
                    }
                    self.threads.remove(&thread);
                    self.ready.retain(|id| *id != thread);
                }
            }
            _ => {}
        }
        self.objects.remove(&id);
        Ok(())
    }
    pub fn debug_output(&self) -> &[Vec<u8>] {
        &self.debug_output
    }
    pub fn take_debug_output(&mut self) -> Vec<Vec<u8>> {
        self.debug_bytes = 0;
        std::mem::take(&mut self.debug_output)
    }
    pub fn take_closed_sessions(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.closed_sessions)
    }
    pub fn transfer(&self, id: ObjectId) -> Option<(u64, usize)> {
        match self.objects.get(&id)?.object {
            Object::Transfer { address, size } => Some((address, size)),
            _ => None,
        }
    }
    pub fn register_shared(
        &mut self,
        memory: &mut dyn GuestMemory,
        bytes: &[u8],
        permissions: u8,
    ) -> Result<Handle, u64> {
        self.allocation_available()?;
        if bytes.is_empty() || !bytes.len().is_multiple_of(PAGE) {
            return Err(INVALID_SIZE);
        }
        if ![1, 3].contains(&permissions) {
            return Err(INVALID_COMBINATION);
        }
        let address = self.next_shared;
        let end = address
            .checked_add(bytes.len() as u64)
            .filter(|v| *v < 1 << 39)
            .ok_or(LIMIT_REACHED)?;
        memory
            .map_zeroed(address, bytes.len(), 0)
            .map_err(|_| LIMIT_REACHED)?;
        memory
            .write_kernel(address, bytes)
            .map_err(|_| INVALID_MEMORY_STATE)?;
        self.next_shared = end;
        self.handle(Object::Shared {
            address,
            size: bytes.len(),
            permissions,
        })
    }
    pub fn write_shared(
        &mut self,
        memory: &mut dyn GuestMemory,
        handle: Handle,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), u64> {
        let Some(Object::Shared { address, size, .. }) = self.lookup(handle) else {
            return Err(INVALID_HANDLE);
        };
        if offset.checked_add(bytes.len()).is_none_or(|v| v > *size) {
            return Err(INVALID_SIZE);
        }
        memory
            .write_kernel(*address + offset as u64, bytes)
            .map_err(|_| INVALID_MEMORY_STATE)
    }
    pub fn signal_event(&mut self, handle: Handle) -> Result<(), u64> {
        let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
        let Some(Entry {
            object: Object::Event(signaled),
            ..
        }) = self.objects.get_mut(&id)
        else {
            return Err(INVALID_HANDLE);
        };
        *signaled = true;
        self.poll_objects();
        Ok(())
    }
    pub fn clear_event(&mut self, handle: Handle) -> Result<(), u64> {
        let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
        match &mut self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?.object {
            Object::Event(signaled) => {
                *signaled = false;
                Ok(())
            }
            _ => Err(INVALID_HANDLE),
        }
    }
    fn runnable(&mut self, id: ThreadId, result: Vec<u64>) {
        let thread = self.threads.get_mut(&id).expect("existing thread");
        thread.status = ThreadStatus::Runnable;
        thread.deadline = None;
        thread.pending = Some(result);
        if !self.ready.contains(&id) {
            self.ready.push_back(id);
        }
    }
    fn block(&mut self, id: ThreadId, wait: Wait, until: Option<u64>) -> Dispatch {
        let thread = self.threads.get_mut(&id).expect("dispatch current thread");
        thread.status = ThreadStatus::Waiting(wait);
        thread.deadline = until;
        thread.pending = None;
        self.ready.retain(|h| *h != id);
        Dispatch::Blocked
    }
    pub fn requeue(&mut self, id: ThreadId) {
        if self
            .threads
            .get(&id)
            .is_some_and(|t| t.status == ThreadStatus::Runnable)
            && !self.ready.contains(&id)
        {
            self.ready.push_back(id);
        }
    }
    pub fn next_runnable(&mut self) -> Option<ThreadId> {
        let mut priorities: BTreeMap<_, _> = self
            .threads
            .iter()
            .map(|(id, t)| (*id, t.priority))
            .collect();
        for _ in 0..self.threads.len() {
            for (id, t) in &self.threads {
                if let ThreadStatus::Waiting(Wait::Mutex { owner, .. }) = t.status {
                    let priority = priorities[id];
                    if let Some(p) = priorities.get_mut(&owner) {
                        *p = (*p).min(priority);
                    }
                }
            }
        }
        let priority = self
            .ready
            .iter()
            .filter_map(|id| priorities.get(id))
            .min()?;
        let index = self
            .ready
            .iter()
            .position(|id| priorities[id] == *priority)?;
        self.ready.remove(index)
    }
    pub fn set_results(&mut self, id: ThreadId, results: Vec<u64>) -> Result<(), u64> {
        let thread = self.threads.get_mut(&id).ok_or(INVALID_HANDLE)?;
        if thread.status != ThreadStatus::Runnable {
            return Err(INVALID_STATE);
        }
        thread.pending = Some(results);
        Ok(())
    }
    pub fn take_results(&mut self, id: ThreadId) -> Option<Vec<u64>> {
        self.threads.get_mut(&id)?.pending.take()
    }
    pub fn next_deadline(&self) -> Option<u64> {
        self.threads.values().filter_map(|t| t.deadline).min()
    }
    pub fn all_exited(&self) -> bool {
        self.threads
            .values()
            .all(|t| matches!(t.status, ThreadStatus::Exited | ThreadStatus::Suspended))
    }
    fn object_ready(&self, id: ObjectId) -> bool {
        match self.objects.get(&id).map(|e| &e.object) {
            Some(Object::Event(signaled)) => *signaled,
            Some(Object::Thread(id)) => self
                .threads
                .get(id)
                .is_some_and(|t| t.status == ThreadStatus::Exited),
            _ => false,
        }
    }
    fn poll_objects(&mut self) {
        let ready: Vec<_> = self
            .threads
            .iter()
            .filter_map(|(id, t)| match &t.status {
                ThreadStatus::Waiting(Wait::Objects(objects)) => objects
                    .iter()
                    .position(|o| self.object_ready(*o))
                    .map(|index| (*id, index, objects.clone())),
                _ => None,
            })
            .collect();
        for (id, index, objects) in ready {
            self.runnable(id, vec![0, index as u64]);
            self.deferred_releases.extend(objects);
        }
    }
    fn waiters(&self, predicate: impl Fn(&Wait) -> bool) -> Vec<ThreadId> {
        let mut waiters: Vec<_> = self
            .threads
            .iter()
            .filter_map(|(id, t)| match &t.status {
                ThreadStatus::Waiting(w) if predicate(w) => Some(*id),
                _ => None,
            })
            .collect();
        waiters.sort_by_key(|id| (self.threads[id].priority, *id));
        waiters
    }
    fn release_mutex(
        &mut self,
        memory: &mut dyn GuestMemory,
        address: u64,
        current: ThreadId,
    ) -> Result<(), u64> {
        let value = word(memory, address)?;
        if value & !WAITERS != current {
            return Err(INVALID_STATE);
        }
        let waiters = self.waiters(|w| matches!(w,Wait::Mutex { address:a,.. } if *a==address));
        let next = waiters.first().copied();
        let next_value = next.unwrap_or(0) | if waiters.len() > 1 { WAITERS } else { 0 };
        put_word(memory, address, next_value)?;
        if let Some(id) = next {
            for waiter in waiters.iter().skip(1) {
                if let ThreadStatus::Waiting(Wait::Mutex { owner, .. }) =
                    &mut self.threads.get_mut(waiter).expect("waiter").status
                {
                    *owner = id;
                }
            }
            let ThreadStatus::Waiting(Wait::Mutex { result, .. }) = self.threads[&id].status else {
                unreachable!()
            };
            self.runnable(id, vec![result]);
        }
        Ok(())
    }
    fn reacquire(
        &mut self,
        memory: &mut dyn GuestMemory,
        id: ThreadId,
        mutex: u64,
        result: u64,
    ) -> Result<(), u64> {
        let value = word(memory, mutex)?;
        if value == 0 {
            put_word(memory, mutex, id)?;
            self.runnable(id, vec![result]);
        } else {
            put_word(memory, mutex, value | WAITERS)?;
            self.block(
                id,
                Wait::Mutex {
                    address: mutex,
                    owner: value & !WAITERS,
                    result,
                },
                None,
            );
        }
        Ok(())
    }
    pub fn advance(&mut self, memory: &mut dyn GuestMemory, now: u64) -> Result<(), u64> {
        self.poll_objects();
        for object in std::mem::take(&mut self.deferred_releases) {
            self.release_object(memory, object)?;
        }
        let expired: Vec<_> = self
            .threads
            .iter()
            .filter(|(_, t)| t.deadline.is_some_and(|d| d <= now))
            .map(|(id, t)| (*id, t.status.clone()))
            .collect();
        for (id, status) in expired {
            match status {
                ThreadStatus::Waiting(Wait::Condition { .. }) => self.runnable(id, vec![TIMED_OUT]),
                ThreadStatus::Waiting(Wait::Sleep) => self.runnable(id, vec![0]),
                ThreadStatus::Waiting(Wait::Objects(objects)) => {
                    self.runnable(id, vec![TIMED_OUT]);
                    for o in objects {
                        self.release_object(memory, o)?;
                    }
                }
                ThreadStatus::Waiting(_) => self.runnable(id, vec![TIMED_OUT]),
                _ => {}
            }
        }
        Ok(())
    }
    pub fn complete_ipc(&mut self, id: ThreadId, results: Vec<u64>) -> Result<(), u64> {
        if self
            .threads
            .get(&id)
            .is_none_or(|t| t.status != ThreadStatus::Waiting(Wait::Ipc))
        {
            return Err(INVALID_STATE);
        }
        self.runnable(id, results);
        Ok(())
    }
    pub fn dispatch(
        &mut self,
        memory: &mut dyn GuestMemory,
        current: ThreadId,
        svc: u16,
        r: &[u64; 31],
        pc: u64,
        now: u64,
    ) -> Dispatch {
        if self
            .policy
            .as_ref()
            .is_some_and(|p| !p.syscalls.get(svc as usize).copied().unwrap_or(false))
        {
            return Dispatch::Unsupported(svc);
        }
        if !self.threads.contains_key(&current) {
            return ret(INVALID_HANDLE);
        }
        match self.dispatch_inner(memory, current, svc, r, pc, now) {
            Ok(d) => d,
            Err(code) => ret(code),
        }
    }
    fn dispatch_inner(
        &mut self,
        memory: &mut dyn GuestMemory,
        current: ThreadId,
        svc: u16,
        r: &[u64; 31],
        pc: u64,
        now: u64,
    ) -> Result<Dispatch, u64> {
        Ok(match svc {
            1 => {
                let requested = usize::try_from(r[1]).map_err(|_| INVALID_SIZE)?;
                if requested > HEAP_LIMIT || !requested.is_multiple_of(0x200000) {
                    return Err(INVALID_SIZE);
                }
                if requested < self.heap_size {
                    let end = HEAP + requested as u64;
                    if self.aliases.iter().chain(self.code_mappings.iter()).any(|((_,source),size)|source+*size as u64>end) || self.objects.values().any(|e|matches!(e.object,Object::Transfer { address,size } | Object::Code { address,size,.. } if address+size as u64>end)) { return Err(INVALID_MEMORY_STATE); }
                    memory
                        .unmap(end, self.heap_size - requested)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                } else if requested > self.heap_size {
                    memory
                        .map_zeroed(HEAP + self.heap_size as u64, requested - self.heap_size, 3)
                        .map_err(|_| LIMIT_REACHED)?;
                }
                self.heap_size = requested;
                out(HEAP)
            }
            2 => {
                if r[0] < BASE
                    || r[0]
                        .checked_add(r[1])
                        .is_none_or(|end| end > BASE + self.image_size as u64)
                {
                    return Err(INVALID_MEMORY_STATE);
                }
                if ![0, 1, 3].contains(&r[2]) {
                    return Err(INVALID_COMBINATION);
                }
                memory_result(memory.protect(r[0], r[1] as usize, r[2] as u8))
            }
            3 => {
                let end = r[0].checked_add(r[1]).ok_or(INVALID_ADDRESS)?;
                if !((r[2] == 16
                    && r[3] == 16
                    && r[0] >= BASE
                    && end <= BASE + self.image_size as u64)
                    || (r[2] == 8
                        && [0, 8].contains(&r[3])
                        && r[0] >= HEAP
                        && end <= HEAP + self.heap_size as u64))
                {
                    return Err(INVALID_COMBINATION);
                }
                memory_result(memory.attribute(r[0], r[1] as usize, r[2] as u32, r[3] as u32))
            }
            4 => {
                let (dest, source, size) = (r[0], r[1], r[2] as usize);
                if !dest.is_multiple_of(4096) || !source.is_multiple_of(4096) {
                    return Err(INVALID_ADDRESS);
                }
                if size == 0 || !size.is_multiple_of(PAGE) {
                    return Err(INVALID_SIZE);
                }
                if dest < STACK
                    || dest
                        .checked_add(size as u64)
                        .is_none_or(|end| end > STACK + 0x10000000)
                {
                    return Err(0xdc01);
                }
                if source < HEAP
                    || source
                        .checked_add(size as u64)
                        .is_none_or(|end| end > HEAP + self.heap_size as u64)
                {
                    return Err(INVALID_MEMORY_STATE);
                }
                if self.objects.values().any(|e|matches!(e.object,Object::Transfer{address,size:n} if source<address+n as u64 && address<source+size as u64)) { return Err(INVALID_MEMORY_STATE); }
                memory
                    .check_access(source, size, 3)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                memory
                    .alias(dest, source, size, 3)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                if memory.protect(source, size, 0).is_err() {
                    memory.unmap(dest, size).map_err(|_| INVALID_MEMORY_STATE)?;
                    return Err(INVALID_MEMORY_STATE);
                }
                memory
                    .set_state(dest, size, 0xb)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                self.aliases.insert((dest, source), size);
                ret(0)
            }
            5 => {
                let key = (r[0], r[1]);
                let size = r[2] as usize;
                if self.aliases.get(&key) != Some(&size) {
                    return Err(INVALID_MEMORY_STATE);
                }
                memory
                    .protect(r[1], size, 3)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                memory.unmap(r[0], size).map_err(|_| INVALID_MEMORY_STATE)?;
                self.aliases.remove(&key);
                ret(0)
            }
            6 => {
                let (address, size, mapping) = memory.query(r[2]).map_err(|_| INVALID_ADDRESS)?;
                let mut info = [0; 40];
                info[..8].copy_from_slice(&address.to_le_bytes());
                info[8..16].copy_from_slice(&size.to_le_bytes());
                if let Some(m) = mapping {
                    info[16..20].copy_from_slice(&m.state.to_le_bytes());
                    info[20..24].copy_from_slice(&m.attribute.to_le_bytes());
                    info[24..28].copy_from_slice(&(m.permissions as u32).to_le_bytes());
                }
                memory
                    .write(r[0], &info)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                out(0)
            }
            7 => Dispatch::ProcessExited(0),
            0x2c | 0x2d | 0x4b | 0x4c | 0x73 | 0x77 | 0x78 => {
                self.dispatch_virtual_memory(memory, svc, r)?
            }
            8..=16 => self.dispatch_thread(memory, current, svc, r, now)?,
            17 => {
                self.signal_event(r[0] as u32)?;
                ret(0)
            }
            18 | 23 => {
                let id = *self.handles.get(&(r[0] as u32)).ok_or(INVALID_HANDLE)?;
                let Some(Entry {
                    object: Object::Event(signaled),
                    ..
                }) = self.objects.get_mut(&id)
                else {
                    return Err(INVALID_HANDLE);
                };
                if svc == 23 && !*signaled {
                    return Err(INVALID_STATE);
                }
                *signaled = false;
                ret(0)
            }
            19 => {
                let handle = r[0] as u32;
                let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
                let Some(Object::Shared {
                    address,
                    size,
                    permissions,
                }) = self.lookup(handle).cloned()
                else {
                    return Err(INVALID_HANDLE);
                };
                if r[2] != size as u64 {
                    return Err(INVALID_SIZE);
                }
                if ![1, 3].contains(&r[3]) || r[3] as u8 & !permissions != 0 {
                    return Err(INVALID_COMBINATION);
                }
                memory
                    .alias(r[1], address, size, r[3] as u8)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                memory
                    .set_state(r[1], size, 6)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                self.shared_mappings.insert((id, r[1]), size);
                self.objects.get_mut(&id).expect("shared").references += 1;
                ret(0)
            }
            20 => {
                let id = *self.handles.get(&(r[0] as u32)).ok_or(INVALID_HANDLE)?;
                let key = (id, r[1]);
                let size = r[2] as usize;
                if self.shared_mappings.get(&key) != Some(&size) {
                    return Err(INVALID_MEMORY_STATE);
                }
                memory.unmap(r[1], size).map_err(|_| INVALID_MEMORY_STATE)?;
                self.shared_mappings.remove(&key);
                self.release_object(memory, id)?;
                ret(0)
            }
            21 => {
                self.allocation_available()?;
                let (address, size) = (r[1], r[2] as usize);
                if ![0, 1, 3].contains(&r[3]) {
                    return Err(INVALID_COMBINATION);
                }
                if !address.is_multiple_of(4096) {
                    return Err(INVALID_ADDRESS);
                }
                if size == 0 || !size.is_multiple_of(PAGE) {
                    return Err(INVALID_SIZE);
                }
                if address < HEAP
                    || address
                        .checked_add(size as u64)
                        .is_none_or(|end| end > HEAP + self.heap_size as u64)
                {
                    return Err(INVALID_MEMORY_STATE);
                }
                if self.objects.values().any(|e|matches!(e.object,Object::Transfer {address:a,size:n} if address<a+n as u64 && a<address+size as u64)) { return Err(INVALID_MEMORY_STATE); }
                memory
                    .check_access(address, size, 3)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                memory
                    .protect(address, size, r[3] as u8)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                out(self.handle(Object::Transfer { address, size })? as u64)
            }
            22 => {
                let handle = r[0] as u32;
                self.close_handle(memory, handle)?;
                ret(0)
            }
            24 => {
                if r[2] == 0 || r[2] > 64 {
                    return Err(INVALID_SIZE);
                }
                let mut bytes = vec![0; r[2] as usize * 4];
                memory
                    .read(r[1], &mut bytes)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                let mut objects = Vec::new();
                for b in bytes.chunks_exact(4) {
                    let handle = u32::from_le_bytes(b.try_into().expect("handle"));
                    let id = *self.handles.get(&handle).ok_or(INVALID_HANDLE)?;
                    if !matches!(
                        self.objects[&id].object,
                        Object::Event(_) | Object::Thread(_)
                    ) {
                        return Err(INVALID_HANDLE);
                    }
                    objects.push(id);
                }
                if self.threads[&current].cancelled {
                    self.threads.get_mut(&current).expect("current").cancelled = false;
                    return Err(CANCELLED);
                }
                if let Some(index) = objects.iter().position(|o| self.object_ready(*o)) {
                    out(index as u64)
                } else if r[3] == 0 {
                    ret(TIMED_OUT)
                } else {
                    for o in &objects {
                        self.objects.get_mut(o).expect("validated").references += 1;
                    }
                    self.block(current, Wait::Objects(objects), deadline(now, r[3]))
                }
            }
            25 => {
                let id = self
                    .resolve_thread(r[0] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                if let ThreadStatus::Waiting(Wait::Objects(objects)) =
                    self.threads[&id].status.clone()
                {
                    self.runnable(id, vec![CANCELLED]);
                    for o in objects {
                        self.release_object(memory, o)?;
                    }
                } else {
                    self.threads.get_mut(&id).expect("resolved").cancelled = true;
                }
                ret(0)
            }
            26 => {
                let owner = r[0] as u32;
                let address = r[1];
                let tag = r[2] as u32;
                if tag != current || self.resolve_thread(owner, current).is_none() {
                    return Err(INVALID_HANDLE);
                }
                memory
                    .check_access(address, 4, 3)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                if word(memory, address)? == owner | WAITERS {
                    self.block(
                        current,
                        Wait::Mutex {
                            address,
                            owner,
                            result: 0,
                        },
                        None,
                    )
                } else {
                    ret(0)
                }
            }
            27 => {
                self.release_mutex(memory, r[0], current)?;
                ret(0)
            }
            28 => {
                if r[2] as u32 != current {
                    return Err(INVALID_HANDLE);
                }
                word(memory, r[1])?;
                memory
                    .check_access(r[1], 4, 3)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                self.release_mutex(memory, r[0], current)?;
                if r[3] == 0 {
                    ret(TIMED_OUT)
                } else {
                    self.block(
                        current,
                        Wait::Condition {
                            key: r[1],
                            mutex: r[0],
                        },
                        deadline(now, r[3]),
                    )
                }
            }
            29 => {
                word(memory, r[0])?;
                let count = r[1] as i32;
                let waiters = self.waiters(|w| matches!(w,Wait::Condition {key,..} if *key==r[0]));
                let count = if count <= 0 {
                    waiters.len()
                } else {
                    (count as usize).min(waiters.len())
                };
                for id in waiters.into_iter().take(count) {
                    let ThreadStatus::Waiting(Wait::Condition { mutex, .. }) =
                        self.threads[&id].status
                    else {
                        unreachable!()
                    };
                    self.reacquire(memory, id, mutex, 0)?;
                }
                ret(0)
            }
            30 => ret(((now as u128 * 19_200_000) / 1_000_000_000) as u64),
            31 => {
                let mut name = Vec::new();
                for i in 0..12 {
                    let mut b = [0];
                    memory
                        .read(r[1] + i, &mut b)
                        .map_err(|_| INVALID_CURRENT_MEMORY)?;
                    if b[0] == 0 {
                        break;
                    }
                    name.push(b[0]);
                }
                let service = *self.ports.get(&name).ok_or(0xf201u64)?;
                out(self.register_session(service)? as u64)
            }
            33 => {
                let session = r[0] as u32;
                let object = *self.handles.get(&session).ok_or(INVALID_HANDLE)?;
                let Some(Object::Session(service)) = self.lookup(session) else {
                    return Err(INVALID_HANDLE);
                };
                let request = IpcRequest {
                    thread: current,
                    session,
                    object,
                    service: *service,
                    tls: self.threads[&current].spec.tls,
                };
                self.block(current, Wait::Ipc, None);
                Dispatch::Ipc(request)
            }
            36 => {
                if r[1] as u32 != 0xffff8001 {
                    return Err(INVALID_HANDLE);
                }
                out(1)
            }
            37 => {
                let id = self
                    .resolve_thread(r[1] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                out(id as u64)
            }
            38 => {
                if r[0] & 0x80000000 != 0 {
                    ret(0)
                } else {
                    Dispatch::Break {
                        reason: r[0] as u32,
                        address: r[1],
                        size: r[2],
                    }
                }
            }
            39 => {
                if r[1] > 1024 * 1024 {
                    return Err(INVALID_SIZE);
                }
                let mut bytes = vec![0; r[1] as usize];
                memory
                    .read(r[0], &mut bytes)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                if !bytes.is_empty() {
                    while self.debug_bytes + bytes.len() > 1024 * 1024
                        || self.debug_output.len() >= 1024
                    {
                        self.debug_bytes -= self.debug_output.remove(0).len();
                    }
                    self.debug_bytes += bytes.len();
                    self.debug_output.push(bytes);
                }
                ret(0)
            }
            41 => {
                let id = r[1];
                let subtype = r[3];
                if id == 11 && subtype < 4 {
                    out(self.seed[subtype as usize])
                } else if subtype != 0 {
                    ret(INVALID_ENUM)
                } else {
                    let value = match id {
                        0 => self.policy.as_ref().map_or(15, |p| {
                            ((1u64 << (p.max_core + 1)) - 1) & !((1u64 << p.min_core) - 1)
                        }),
                        1 => self.policy.as_ref().map_or(u64::MAX, |p| {
                            (u64::MAX >> (63 - p.lowest_priority))
                                & (u64::MAX << p.highest_priority)
                        }),
                        2 => 0x500000000,
                        3 => 0x40000000,
                        4 => HEAP,
                        5 | 6 | 21 => HEAP_LIMIT as u64,
                        7 => (self.image_size + self.stack_size) as u64,
                        12 => {
                            if self.policy.is_some() {
                                0x200000
                            } else {
                                BASE
                            }
                        }
                        13 => {
                            if self.policy.is_some() {
                                (1 << 39) - 0x200000
                            } else {
                                0x600000000
                            }
                        }
                        14 => STACK,
                        15 => 0x10000000,
                        18 => self.program_id,
                        20 => EXCEPTION_CONTEXT,
                        22 => (self.image_size + self.stack_size + self.heap_size) as u64,
                        23 | 28 => 0,
                        24 => MAX_THREADS.saturating_sub(
                            self.threads
                                .values()
                                .filter(|t| t.status != ThreadStatus::Exited)
                                .count(),
                        ) as u64,
                        _ => return Err(INVALID_ENUM),
                    };
                    out(value)
                }
            }
            52 => {
                let value = word(memory, r[0])? as i32;
                let compare = r[2] as i32;
                let should_wait = match r[1] {
                    0 | 1 => value < compare,
                    2 => value == compare,
                    _ => return Err(INVALID_ENUM),
                };
                if !should_wait {
                    return Err(INVALID_STATE);
                }
                if r[1] == 1 {
                    put_word(memory, r[0], value.wrapping_sub(1) as u32)?;
                }
                if r[3] == 0 {
                    ret(TIMED_OUT)
                } else {
                    self.block(current, Wait::Address(r[0]), deadline(now, r[3]))
                }
            }
            53 => {
                if r[1] > 2 {
                    return Err(INVALID_ENUM);
                }
                let value = word(memory, r[0])?;
                let waiters = self.waiters(|w| matches!(w,Wait::Address(a) if *a==r[0]));
                let count = r[3] as i32;
                if r[1] != 0 {
                    if value != r[2] as u32 {
                        return Err(INVALID_STATE);
                    }
                    let delta = if r[1] == 1 || waiters.is_empty() {
                        1
                    } else if count <= 0 || waiters.len() <= count as usize {
                        -1
                    } else {
                        0
                    };
                    put_word(memory, r[0], value.wrapping_add_signed(delta))?;
                }
                let count = if count <= 0 {
                    waiters.len()
                } else {
                    (count as usize).min(waiters.len())
                };
                for id in waiters.into_iter().take(count) {
                    self.runnable(id, vec![0]);
                }
                ret(0)
            }
            0x45 => {
                if self.handles.len() + 2 > MAX_HANDLES {
                    return Err(LIMIT_REACHED);
                }
                let write = self.register_event(false)?;
                let read = self.duplicate_handle(write)?;
                Dispatch::Return(vec![0, u64::from(write), u64::from(read)])
            }
            0x7e if pc.checked_sub(4) == Some(LOADER_RETURN) => {
                Dispatch::ProcessExited(r[0] as u32)
            }
            _ => Dispatch::Unsupported(svc),
        })
    }
}
