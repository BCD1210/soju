//! Apple Silicon EL0 execution with 4 KiB stage-1 permissions and an EL1 bridge.
mod ffi;

use crate::memory::{MAX_ARENA, MemoryImage, PHYSICAL_BASE, ROOT};
use crate::nro::{Homebrew, Region};

use std::fmt;
use std::marker::PhantomData;
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

pub const TIMER_HZ: u64 = 19_200_000;
/// Deterministic 64-byte I/D lines, 64-byte reservation/writeback granules,
/// PIPT instruction cache. IDC/DIC remain clear: software must maintain caches.
pub const CACHE_TYPE: u64 = 0x8444_c004;
// DZE stays clear: an architectural DCZID read advertises DZP (prohibited).
// A DC ZVA instruction nevertheless traps and is handled as a bounded 64-byte
// checked write, without exposing host zero-block sizing to the write path.
const CACHE_ZERO_ID: u64 = 0x14;
const GUEST: u64 = 0x10000;
const VECTOR_SYNC: u64 = GUEST + 0x400;
static VM_LOCK: Mutex<bool> = Mutex::new(false);

#[derive(Debug)]
pub enum Error {
    Hypervisor(&'static str, i32),
    Allocation,
    InvalidProgram,
    Memory(crate::memory::Error),
    UnexpectedExit { reason: u32, syndrome: u64 },
    UnexpectedGuestException(u64),
    Timeout,
    ExitBudget,
    Lock,
    VmBusy,
    WrongProcess,
    InvalidState,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
fn check(operation: &'static str, result: i32) -> Result<(), Error> {
    if result == 0 {
        Ok(())
    } else {
        Err(Error::Hypervisor(operation, result))
    }
}

/// A monotonic 19.2 MHz counter starting at VM creation, independent of host Hz.
fn ticks(elapsed: Duration) -> u64 {
    (elapsed.as_nanos() * u128::from(TIMER_HZ) / 1_000_000_000) as u64
}

#[derive(Debug)]
pub struct SvcExit {
    pub immediate: u16,
    pub registers: [u64; 31],
    pub guest_pc: u64,
    pub timer_reads: usize,
}

/// VM exits; cache is a subset of system-register exits.
#[derive(Debug, Default)]
pub struct ExitProfile {
    pub canceled: AtomicU64,
    pub svc: AtomicU64,
    pub system: AtomicU64,
    pub cache: AtomicU64,
    pub maintenance: AtomicU64,
    pub lock_wait_ns: AtomicU64,
}

/// The single VM and its memory. The mutex serializes ALL guest execution and
/// host memory access; vCPUs hold an Arc until their thread-affine destruction.
pub struct Process {
    pub profile: ExitProfile,
    memory: Mutex<MemoryImage>,
    start: Instant,
    capture_tick: AtomicU64,
    mapped: bool,
    vm: bool,
}
pub struct ProcessGuard<'a> {
    process: &'a Process,
    memory: MutexGuard<'a, MemoryImage>,
}
impl Process {
    pub fn new(boot: &Homebrew) -> Result<Arc<Self>, Error> {
        let mut lease = VM_LOCK.lock().map_err(|_| Error::Lock)?;
        if *lease {
            return Err(Error::VmBusy);
        }
        let memory = MemoryImage::new(boot).map_err(Error::Memory)?;
        // SAFETY: global lease excludes any other VM owner.
        check("hv_vm_create", unsafe { ffi::hv_vm_create(ptr::null()) })?;
        *lease = true;
        drop(lease);
        let mut process = Self {
            profile: ExitProfile::default(),
            memory: Mutex::new(memory),
            start: Instant::now(),
            capture_tick: AtomicU64::new(u64::MAX),
            mapped: false,
            vm: true,
        };
        let pointer = process
            .memory
            .get_mut()
            .map_err(|_| Error::Lock)?
            .host_pointer();
        // SAFETY: aligned stable allocation stays live until Process::drop unmaps it.
        check("hv_vm_map", unsafe {
            ffi::hv_vm_map(pointer.cast(), PHYSICAL_BASE, MAX_ARENA, 7)
        })?;
        process.mapped = true;
        Ok(Arc::new(process))
    }
    pub fn lock(&self) -> Result<ProcessGuard<'_>, Error> {
        let started = Instant::now();
        let memory = self.memory.lock().map_err(|_| Error::Lock)?;
        self.profile
            .lock_wait_ns
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        Ok(ProcessGuard {
            process: self,
            memory,
        })
    }
    pub fn elapsed_ns(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
    /// Freeze the guest-visible counter for headless pose capture. Host scheduling
    /// deadlines continue to use elapsed_ns so sleeping service threads progress.
    pub fn set_capture_tick(&self, tick: u64) {
        self.capture_tick.store(tick, Ordering::Relaxed);
    }
    pub fn ticks(&self) -> u64 {
        match self.capture_tick.load(Ordering::Relaxed) {
            u64::MAX => ticks(self.start.elapsed()),
            tick => tick,
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: last Arc implies every owning CPU was destroyed. On failure
        // abort instead of freeing memory while the hypervisor still maps it.
        unsafe {
            if self.mapped && ffi::hv_vm_unmap(PHYSICAL_BASE, MAX_ARENA) != 0 {
                std::process::abort();
            }
            if self.vm && ffi::hv_vm_destroy() != 0 {
                std::process::abort();
            }
        }
        *VM_LOCK.lock().unwrap_or_else(|e| e.into_inner()) = false;
    }
}
impl ProcessGuard<'_> {
    pub fn memory(&mut self) -> &mut dyn crate::memory::GuestMemory {
        &mut *self.memory
    }
    pub fn map_zeroed(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error> {
        self.memory
            .map_zeroed(address, size, permissions)
            .map_err(Error::Memory)
    }
    pub fn protect(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error> {
        self.memory
            .protect(address, size, permissions)
            .map_err(Error::Memory)
    }
    pub fn unmap(&mut self, address: u64, size: usize) -> Result<(), Error> {
        self.memory.unmap(address, size).map_err(Error::Memory)
    }
    pub fn alias(
        &mut self,
        destination: u64,
        source: u64,
        size: usize,
        permissions: u8,
    ) -> Result<(), Error> {
        self.memory
            .alias(destination, source, size, permissions)
            .map_err(Error::Memory)
    }
    pub fn read(&self, address: u64, output: &mut [u8]) -> Result<(), Error> {
        self.memory.read(address, output).map_err(Error::Memory)
    }
    pub fn write(&mut self, address: u64, input: &[u8]) -> Result<(), Error> {
        self.memory.write(address, input).map_err(Error::Memory)
    }
}
#[derive(Clone, Debug)]
pub struct ThreadState {
    pub registers: [u64; 31],
    pub pc: u64,
    pub sp: u64,
    pub tpidrro_el0: u64,
}
impl ThreadState {
    pub fn from_homebrew(boot: &Homebrew) -> Self {
        let mut registers = [0; 31];
        registers[0] = boot.x0;
        registers[1] = boot.x1;
        registers[30] = boot.x30;
        Self {
            registers,
            pc: boot.pc,
            sp: boot.sp,
            tpidrro_el0: boot.tpidrro_el0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RunBudget {
    pub max_duration: Duration,
    pub max_exits: u32,
}
impl Default for RunBudget {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_millis(5),
            max_exits: 64,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum YieldReason {
    Deadline,
    ExitBudget,
}
#[derive(Debug)]
pub enum RunOutcome {
    Svc(Box<SvcExit>),
    Yield(YieldReason),
}
/// A vCPU permanently attached to its creating host thread, retaining shared VM ownership.
pub struct Cpu {
    vcpu: Option<u64>,
    exit: *const ffi::Exit,
    process: Arc<Process>,
    generation: u64,
    pending_svc: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Cpu {
    pub fn new(program: &[u32]) -> Result<Self, Error> {
        if program.is_empty() || program.len() > 0x1000 / 4 {
            return Err(Error::InvalidProgram);
        }
        let mut bytes = vec![0; 0x1000];
        for (index, instruction) in program.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&instruction.to_le_bytes());
        }
        Self::from_homebrew(&Homebrew {
            regions: vec![
                Region {
                    address: 0x14000,
                    bytes,
                    permissions: 5,
                },
                Region {
                    address: 0x18000,
                    bytes: vec![0; 0x1000],
                    permissions: 3,
                },
            ],
            pc: 0x14000,
            sp: 0x19000,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: 0,
        })
    }
    pub fn from_homebrew(boot: &Homebrew) -> Result<Self, Error> {
        Self::attach(Process::new(boot)?, ThreadState::from_homebrew(boot))
    }
    pub fn process(&self) -> Arc<Process> {
        Arc::clone(&self.process)
    }
    pub fn attach(process: Arc<Process>, state: ThreadState) -> Result<Self, Error> {
        let owner = Arc::clone(&process);
        let _guard = owner.lock()?;
        let mut cpu = Self {
            vcpu: None,
            exit: ptr::null(),
            process,
            generation: u64::MAX,
            pending_svc: false,
            _thread: PhantomData,
        };
        let mut id = 0;
        // SAFETY: called on this CPU's permanent owner thread; storage lives until destroy.
        check("hv_vcpu_create", unsafe {
            ffi::hv_vcpu_create(&mut id, &mut cpu.exit, ptr::null())
        })?;
        cpu.vcpu = Some(id);
        cpu.set_sys(ffi::MAIR_EL1, 0x44ff)?; // normal write-back memory, AttrIndx=0
        // T0SZ=25 (39 bits), 4 KiB, inner-shareable WB walks, TTBR1 disabled,
        // IPS=40 bits. The stage-2 arena contains only low physical addresses.
        cpu.set_sys(
            ffi::TCR_EL1,
            25 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23) | (2 << 32),
        )?;
        cpu.set_sys(ffi::TTBR0_EL1, ROOT)?;
        cpu.set_sys(ffi::SCTLR_EL1, 0x30d01805)?;
        cpu.set_sys(ffi::CPACR_EL1, 3 << 20)?; // guest SIMD/FP
        cpu.set_sys(ffi::VBAR_EL1, GUEST)?;
        cpu.set_sys(ffi::CNTKCTL_EL1, 0)?; // trap EL0 CNTFRQ/CNTVCT reads to EL1
        cpu.set_sys(ffi::SP_EL0, state.sp)?;
        // The register-preserving EL1 bridge uses no stack.
        cpu.set_sys(ffi::SP_EL1, 0)?;
        cpu.set_sys(ffi::TPIDRRO_EL0, state.tpidrro_el0)?;
        cpu.set_reg(ffi::PC, state.pc)?;
        for (index, value) in state.registers.into_iter().enumerate() {
            cpu.set_reg(index as u32, value)?;
        }
        cpu.set_reg(ffi::CPSR, 0x3c0)?; // EL0t, asynchronous exceptions masked
        Ok(cpu)
    }
    fn id(&self) -> u64 {
        self.vcpu.expect("initialized vCPU")
    }
    fn reg(&self, reg: u32) -> Result<u64, Error> {
        let mut value = 0;
        // SAFETY: vCPU is owned by this thread; output points to live storage.
        check("hv_vcpu_get_reg", unsafe {
            ffi::hv_vcpu_get_reg(self.id(), reg, &mut value)
        })?;
        Ok(value)
    }
    fn set_reg(&mut self, reg: u32, value: u64) -> Result<(), Error> {
        // SAFETY: vCPU is owned by this thread and is not running.
        check("hv_vcpu_set_reg", unsafe {
            ffi::hv_vcpu_set_reg(self.id(), reg, value)
        })
    }
    fn sys(&self, reg: u32) -> Result<u64, Error> {
        let mut value = 0;
        // SAFETY: vCPU is owned by this thread; output points to live storage.
        check("hv_vcpu_get_sys_reg", unsafe {
            ffi::hv_vcpu_get_sys_reg(self.id(), reg, &mut value)
        })?;
        Ok(value)
    }
    fn set_sys(&mut self, reg: u32, value: u64) -> Result<(), Error> {
        // SAFETY: vCPU is owned by this thread and is not running.
        check("hv_vcpu_set_sys_reg", unsafe {
            ffi::hv_vcpu_set_sys_reg(self.id(), reg, value)
        })
    }
    fn validate(&self, guard: &ProcessGuard<'_>) -> Result<(), Error> {
        if !ptr::eq(guard.process, Arc::as_ptr(&self.process)) {
            Err(Error::WrongProcess)
        } else {
            Ok(())
        }
    }
    pub fn set_svc_results(
        &mut self,
        guard: &mut ProcessGuard<'_>,
        values: &[u64],
    ) -> Result<(), Error> {
        self.validate(guard)?;
        if !self.pending_svc || values.len() > 31 {
            return Err(Error::InvalidState);
        }
        for (i, v) in values.iter().enumerate() {
            self.set_reg(i as u32, *v)?;
        }
        Ok(())
    }
    // Maintenance runs with EL1 MMU enabled, without changing GPRs, SIMD, ELR or
    // SPSR. Canceled exits are retried until the private HVC completes. The guest
    // cannot enter EL0 until both break-before-make phases have finished.
    fn invalidate(&mut self) -> Result<(), Error> {
        let pc = self.reg(ffi::PC)?;
        let cpsr = self.reg(ffi::CPSR)?;
        self.set_reg(ffi::PC, GUEST + 0x800)?;
        self.set_reg(ffi::CPSR, 0x3c5)?;
        let mut complete = false;
        for _ in 0..64 {
            // SAFETY: this owner thread holds the process execution guard.
            check("hv_vcpu_run", unsafe { ffi::hv_vcpu_run(self.id()) })?;
            // SAFETY: exit storage is valid while this CPU is live.
            let exit = unsafe { *self.exit };
            if exit.reason == 0 {
                self.process
                    .profile
                    .canceled
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            self.process
                .profile
                .maintenance
                .fetch_add(1, Ordering::Relaxed);
            if exit.reason != 1
                || exit.exception.syndrome >> 26 != 0x16
                || exit.exception.syndrome & 0xffff != 1
            {
                return Err(Error::UnexpectedExit {
                    reason: exit.reason,
                    syndrome: exit.exception.syndrome,
                });
            }
            complete = true;
            break;
        }
        if !complete {
            return Err(Error::ExitBudget);
        }
        self.set_reg(ffi::PC, pc)?;
        self.set_reg(ffi::CPSR, cpsr)
    }
    pub fn run_quantum(
        &mut self,
        guard: &mut ProcessGuard<'_>,
        budget: RunBudget,
    ) -> Result<RunOutcome, Error> {
        self.validate(guard)?;
        if budget.max_duration.is_zero() {
            return Ok(RunOutcome::Yield(YieldReason::Deadline));
        }
        if budget.max_exits == 0 {
            return Ok(RunOutcome::Yield(YieldReason::ExitBudget));
        }
        let started = Instant::now();
        let id = self.id();
        let duration = budget.max_duration;
        let (done, wait) = mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if wait.recv_timeout(duration) == Err(mpsc::RecvTimeoutError::Timeout) {
                // SAFETY: the owner joins this watchdog before CPU destruction.
                check("hv_vcpus_exit", unsafe { ffi::hv_vcpus_exit(&id, 1) })
            } else {
                Ok(())
            }
        });
        let result = (|| {
            if self.generation != guard.memory.generation() {
                self.invalidate()?;
                guard.memory.publish();
                self.invalidate()?;
                self.generation = guard.memory.generation();
            }
            self.pending_svc = false;
            if started.elapsed() >= duration {
                return Ok(RunOutcome::Yield(YieldReason::Deadline));
            }
            self.run_exits(guard, budget.max_exits, started + duration)
        })();
        let _ = done.send(());
        watchdog.join().map_err(|_| Error::Lock)??;
        result
    }
    /// Compatibility wrapper with the original two-second, 64-exit bound.
    pub fn run_until_svc(&mut self) -> Result<SvcExit, Error> {
        let process = Arc::clone(&self.process);
        let mut guard = process.lock()?;
        match self.run_quantum(
            &mut guard,
            RunBudget {
                max_duration: Duration::from_secs(2),
                max_exits: 64,
            },
        )? {
            RunOutcome::Svc(exit) => Ok(*exit),
            RunOutcome::Yield(YieldReason::Deadline) => Err(Error::Timeout),
            RunOutcome::Yield(YieldReason::ExitBudget) => Err(Error::ExitBudget),
        }
    }
    /// Coalesce a bounded, side-effect-free data-cache maintenance loop. The
    /// final ADD/CMP/branch execute in the guest, retaining exact NZCV semantics.
    /// Other instruction sequences and inaccessible ranges take the scalar path.
    fn batch_cache_loop(
        &mut self,
        guard: &mut ProcessGuard<'_>,
        pc: u64,
        rt: u32,
        address: u64,
    ) -> Result<bool, Error> {
        let mut code = [0; 12];
        if guard.memory.check_access(pc + 4, 12, 5).is_err()
            || guard.memory.read(pc + 4, &mut code).is_err()
        {
            return Ok(false);
        }
        let words: Vec<u32> = code
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let (add, cmp, branch) = (words[0], words[1], words[2]);
        if add & 0xffe0fc00 != 0x8b000000
            || add & 31 != rt
            || (add >> 5) & 31 != rt
            || (add >> 16) & 31 == rt
            || cmp & 0xffe0fc1f != 0xeb00001f
            || (cmp >> 5) & 31 != rt
            || (cmp >> 16) & 31 == rt
            || branch != 0x54ffffa3
        {
            return Ok(false);
        }
        let operand = |r| if r == 31 { Ok(0) } else { self.reg(r) };
        if operand((add >> 16) & 31)? != 64 {
            return Ok(false);
        }
        let end = operand((cmp >> 16) & 31)?;
        let count = end.saturating_sub(address).div_ceil(64).clamp(1, 4096);
        let Some(last) = address.checked_add((count - 1) * 64) else {
            return Ok(false);
        };
        if count < 2 || last.checked_add(64).is_none() {
            return Ok(false);
        }
        if !guard
            .memory
            .cache_data_range(address & !63, count as usize * 64)
            .map_err(Error::Memory)?
        {
            return Ok(false);
        }
        self.set_reg(rt, last)?;
        Ok(true)
    }
    fn run_exits(
        &mut self,
        guard: &mut ProcessGuard<'_>,
        max_exits: u32,
        deadline: Instant,
    ) -> Result<RunOutcome, Error> {
        let mut timer_reads = 0;
        for _ in 0..max_exits {
            if Instant::now() >= deadline {
                return Ok(RunOutcome::Yield(YieldReason::Deadline));
            }
            // SAFETY: same owning thread; mapping remains live until Drop.
            check("hv_vcpu_run", unsafe { ffi::hv_vcpu_run(self.id()) })?;
            // SAFETY: framework-provided exit storage lives until vCPU destruction.
            let exit = unsafe { *self.exit };
            if exit.reason == 0 {
                self.process
                    .profile
                    .canceled
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(RunOutcome::Yield(YieldReason::Deadline));
            }
            if exit.reason != 1 || exit.exception.syndrome >> 26 != 0x16 {
                return Err(Error::UnexpectedExit {
                    reason: exit.reason,
                    syndrome: exit.exception.syndrome,
                });
            }
            let pc = self.reg(ffi::PC)?;
            let syndrome = self.sys(ffi::ESR_EL1)?;
            // HVC may report its PC or the following instruction. Verify that the
            // bridge originated from EL0, not a fault in another EL1 vector.
            if !(VECTOR_SYNC..=VECTOR_SYNC + 4).contains(&pc) || self.sys(ffi::SPSR_EL1)? & 0xf != 0
            {
                return Err(Error::UnexpectedGuestException(syndrome));
            }
            let elr = self.sys(ffi::ELR_EL1)?;
            match syndrome >> 26 {
                0x15 => {
                    self.process.profile.svc.fetch_add(1, Ordering::Relaxed);
                    let mut registers = [0; 31];
                    for (index, value) in registers.iter_mut().enumerate() {
                        *value = self.reg(index as u32)?;
                    }
                    self.set_reg(ffi::PC, VECTOR_SYNC + 4)?;
                    self.pending_svc = true;
                    return Ok(RunOutcome::Svc(Box::new(SvcExit {
                        immediate: syndrome as u16,
                        registers,
                        guest_pc: elr,
                        timer_reads,
                    })));
                }
                0x18 => {
                    self.process.profile.system.fetch_add(1, Ordering::Relaxed);
                    // ESR system-register ISS: Op0[21:20], Op2[19:17], Op1[16:14],
                    // CRn[13:10], Rt[9:5], CRm[4:1], direction[0].
                    let iss = syndrome & 0x1ffffff;
                    let op0 = (iss >> 20) & 3;
                    let op2 = (iss >> 17) & 7;
                    let op1 = (iss >> 14) & 7;
                    let crn = (iss >> 10) & 15;
                    let rt = ((iss >> 5) & 31) as u32;
                    let crm = (iss >> 1) & 15;
                    match (op0, op1, crn, crm, op2, iss & 1) {
                        (3, 3, 14, 0, 0..=2, 1) => {
                            let value = if op2 == 0 {
                                TIMER_HZ
                            } else {
                                self.process.ticks()
                            };
                            if rt != 31 {
                                self.set_reg(rt, value)?;
                            }
                            timer_reads += 1;
                        }
                        (3, 3, 0, 0, 1, 1) | (3, 3, 0, 0, 7, 1) => {
                            if rt != 31 {
                                self.set_reg(
                                    rt,
                                    if op2 == 1 { CACHE_TYPE } else { CACHE_ZERO_ID },
                                )?;
                            }
                        }
                        // DC CVAC/CVAU/CIVAC and IC IVAU. The virtual operand is
                        // translated through checked metadata, never a host pointer.
                        (1, 3, 7, 10 | 11 | 14 | 5, 1, 0) => {
                            self.process.profile.cache.fetch_add(1, Ordering::Relaxed);
                            let address = if rt == 31 { 0 } else { self.reg(rt)? };
                            let batched = if crm != 5 && rt != 31 {
                                self.batch_cache_loop(guard, elr, rt, address)?
                            } else {
                                false
                            };
                            if !batched {
                                guard
                                    .memory
                                    .cache_line(address, crm == 5)
                                    .map_err(Error::Memory)?;
                            }
                        }
                        (1, 3, 7, 4, 1, 0) => {
                            let address = if rt == 31 { 0 } else { self.reg(rt)? };
                            guard
                                .memory
                                .write(address & !63, &[0; 64])
                                .map_err(Error::Memory)?;
                        }
                        _ => return Err(Error::UnexpectedGuestException(syndrome)),
                    }
                    self.set_sys(ffi::ELR_EL1, elr + 4)?;
                    self.set_reg(ffi::PC, VECTOR_SYNC + 4)?;
                    if self.generation != guard.memory.generation() {
                        self.invalidate()?;
                        self.generation = guard.memory.generation();
                    }
                }
                _ => return Err(Error::UnexpectedGuestException(syndrome)),
            }
        }
        Ok(RunOutcome::Yield(YieldReason::ExitBudget))
    }
}
impl Drop for Cpu {
    fn drop(&mut self) {
        // SAFETY: no vCPU runs and the watchdog has joined. Never free mapped
        // memory after a teardown failure; abort rather than leave a dangling map.
        unsafe {
            if let Some(id) = self.vcpu
                && ffi::hv_vcpu_destroy(id) != 0
            {
                std::process::abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_scales_to_tegra_frequency() {
        assert_eq!(ticks(Duration::ZERO), 0);
        assert_eq!(ticks(Duration::from_secs(1)), TIMER_HZ);
        assert_eq!(ticks(Duration::from_millis(125)), 2_400_000);
        assert_eq!(ticks(Duration::from_nanos(100)), 1);
    }
}
