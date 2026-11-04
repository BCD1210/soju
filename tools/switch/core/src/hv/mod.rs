//! Apple Silicon EL0 execution with 4 KiB stage-1 permissions and an EL1 bridge.
mod ffi;

use crate::memory::{MemoryImage, PHYSICAL_BASE, ROOT};
use crate::nro::{Homebrew, Region};

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::fmt;
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

pub const TIMER_HZ: u64 = 19_200_000;
const GUEST: u64 = 0x10000;
const VECTOR_SYNC: u64 = GUEST + 0x400;
static VM_LOCK: Mutex<()> = Mutex::new(());

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

/// Owns one process VM and a thread-affine vCPU. It cannot be sent between threads.
/// Only trusted synthetic instruction sequences are supported at this milestone.
pub struct Cpu {
    vcpu: Option<u64>,
    exit: *const ffi::Exit,
    arena: NonNull<u8>,
    layout: Layout,
    mapped: bool,
    vm: bool,
    start: Instant,
    _lock: MutexGuard<'static, ()>,
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
        let image = MemoryImage::new(boot).map_err(Error::Memory)?;
        let lock = VM_LOCK.lock().map_err(|_| Error::Lock)?;
        let layout =
            Layout::from_size_align(image.bytes.len(), 0x4000).map_err(|_| Error::Allocation)?;
        // SAFETY: nonzero valid layout, retained until the VM mapping is removed.
        let arena = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(Error::Allocation)?;
        let mut cpu = Self {
            vcpu: None,
            exit: ptr::null(),
            arena,
            layout,
            mapped: false,
            vm: false,
            start: Instant::now(),
            _lock: lock,
            _thread: PhantomData,
        };
        // SAFETY: arena was allocated for exactly image.bytes.len() bytes;
        // allocations do not overlap and no vCPU or mapping exists yet.
        unsafe {
            ptr::copy_nonoverlapping(image.bytes.as_ptr(), cpu.arena.as_ptr(), image.bytes.len());
        }
        // SAFETY: SDK calls receive valid pointers; Cpu tracks successful acquisition
        // so every early return destroys vCPU, mapping, and VM in that order.
        unsafe {
            check("hv_vm_create", ffi::hv_vm_create(ptr::null()))?;
            cpu.vm = true;
            check(
                "hv_vm_map",
                ffi::hv_vm_map(
                    cpu.arena.as_ptr().cast(),
                    PHYSICAL_BASE,
                    cpu.layout.size(),
                    7,
                ),
            )?;
            cpu.mapped = true;
            let mut id = 0;
            check(
                "hv_vcpu_create",
                ffi::hv_vcpu_create(&mut id, &mut cpu.exit, ptr::null()),
            )?;
            cpu.vcpu = Some(id);
        }
        cpu.set_sys(ffi::MAIR_EL1, 0xff)?; // normal write-back memory, AttrIndx=0
        // T0SZ=25 (39 bits), 4 KiB, inner-shareable WB walks, TTBR1 disabled,
        // IPS=40 bits. The stage-2 arena contains only low physical addresses.
        cpu.set_sys(
            ffi::TCR_EL1,
            25 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23) | (2 << 32),
        )?;
        cpu.set_sys(ffi::TTBR0_EL1, ROOT)?;
        cpu.set_sys(ffi::SCTLR_EL1, 0x30d00801)?;
        cpu.set_sys(ffi::CPACR_EL1, 3 << 20)?; // guest SIMD/FP
        cpu.set_sys(ffi::VBAR_EL1, GUEST)?;
        cpu.set_sys(ffi::CNTKCTL_EL1, 0)?; // trap EL0 CNTFRQ/CNTVCT reads to EL1
        cpu.set_sys(ffi::SP_EL0, boot.sp)?;
        // The register-preserving EL1 bridge uses no stack.
        cpu.set_sys(ffi::SP_EL1, 0)?;
        cpu.set_sys(ffi::TPIDRRO_EL0, boot.tpidrro_el0)?;
        cpu.set_reg(ffi::PC, boot.pc)?;
        cpu.set_reg(0, boot.x0)?;
        cpu.set_reg(1, boot.x1)?;
        cpu.set_reg(30, boot.x30)?;
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
    /// Run until one SVC, with a two-second watchdog and bounded host exits.
    pub fn run_until_svc(&mut self) -> Result<SvcExit, Error> {
        let id = self.id();
        let (done, wait) = mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if wait.recv_timeout(Duration::from_secs(2)) == Err(mpsc::RecvTimeoutError::Timeout) {
                // SAFETY: hv_vcpus_exit is explicitly cross-thread. The owner joins
                // this watchdog before returning or destroying the vCPU.
                unsafe { ffi::hv_vcpus_exit(&id, 1) };
                true
            } else {
                false
            }
        });
        let result = self.run_exits();
        let _ = done.send(());
        if watchdog.join().unwrap_or(true) {
            Err(Error::Timeout)
        } else {
            result
        }
    }
    fn run_exits(&mut self) -> Result<SvcExit, Error> {
        let mut timer_reads = 0;
        for _ in 0..64 {
            // SAFETY: same owning thread; mapping remains live until Drop.
            check("hv_vcpu_run", unsafe { ffi::hv_vcpu_run(self.id()) })?;
            // SAFETY: framework-provided exit storage lives until vCPU destruction.
            let exit = unsafe { *self.exit };
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
                    let mut registers = [0; 31];
                    for (index, value) in registers.iter_mut().enumerate() {
                        *value = self.reg(index as u32)?;
                    }
                    self.set_reg(ffi::PC, VECTOR_SYNC + 4)?;
                    return Ok(SvcExit {
                        immediate: syndrome as u16,
                        registers,
                        guest_pc: elr,
                        timer_reads,
                    });
                }
                0x18 => {
                    // ESR system-register ISS: Op0[21:20], Op2[19:17], Op1[16:14],
                    // CRn[13:10], Rt[9:5], CRm[4:1], direction[0].
                    let iss = syndrome & 0x1ffffff;
                    let op0 = (iss >> 20) & 3;
                    let op2 = (iss >> 17) & 7;
                    let op1 = (iss >> 14) & 7;
                    let crn = (iss >> 10) & 15;
                    let rt = ((iss >> 5) & 31) as u32;
                    let crm = (iss >> 1) & 15;
                    if (op0, op1, crn, crm, iss & 1) != (3, 3, 14, 0, 1) || ![0, 2].contains(&op2) {
                        return Err(Error::UnexpectedGuestException(syndrome));
                    }
                    let value = if op2 == 0 {
                        TIMER_HZ
                    } else {
                        ticks(self.start.elapsed())
                    };
                    if rt != 31 {
                        self.set_reg(rt, value)?;
                    }
                    timer_reads += 1;
                    self.set_sys(ffi::ELR_EL1, elr + 4)?;
                    self.set_reg(ffi::PC, VECTOR_SYNC + 4)?;
                }
                _ => return Err(Error::UnexpectedGuestException(syndrome)),
            }
        }
        Err(Error::ExitBudget)
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
            if self.mapped && ffi::hv_vm_unmap(PHYSICAL_BASE, self.layout.size()) != 0 {
                std::process::abort();
            }
            if self.vm && ffi::hv_vm_destroy() != 0 {
                std::process::abort();
            }
            dealloc(self.arena.as_ptr(), self.layout);
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
