//! Complete movable guest context; host vCPU ownership never moves.
use super::{Cpu, Error, ProcessGuard, ThreadState, check};
#[derive(Debug)]
pub struct Context {
    pub(crate) revision: u64,
    registers: [u64; 35],
    system: [u64; 7],
    vectors: [u8; 512],
    pending_svc: bool,
}
impl From<ThreadState> for Context {
    fn from(state: ThreadState) -> Self {
        let mut registers = [0; 35];
        registers[..31].copy_from_slice(&state.registers);
        registers[31] = state.pc;
        registers[34] = 0x3c0;
        Self {
            revision: 0,
            registers,
            system: [state.sp, 0, state.tpidrro_el0, 0, 0, 0, 0],
            vectors: [0; 512],
            pending_svc: false,
        }
    }
}
unsafe extern "C" {
    fn soju_context_save(cpu: u64, registers: *mut u64, system: *mut u64, vectors: *mut u8) -> i32;
    fn soju_context_load(
        cpu: u64,
        registers: *const u64,
        system: *const u64,
        vectors: *const u8,
    ) -> i32;
}
impl Cpu {
    pub fn load_context(&mut self, guard: &ProcessGuard<'_>, state: &Context) -> Result<(), Error> {
        self.validate(guard)?;
        // SAFETY: FFI/bounds: C reads exactly 35/7/512 initialized elements; the
        // guard excludes execution and this Cpu remains on its creating thread.
        check("context load", unsafe {
            soju_context_load(
                self.id(),
                state.registers.as_ptr(),
                state.system.as_ptr(),
                state.vectors.as_ptr(),
            )
        })?;
        self.pending_svc = state.pending_svc;
        // Runs the CLREX maintenance bridge before another thread enters EL0.
        self.generation = u64::MAX;
        Ok(())
    }
    pub fn save_context(&self, guard: &ProcessGuard<'_>, state: &mut Context) -> Result<(), Error> {
        self.validate(guard)?;
        // SAFETY: FFI/bounds: C writes exactly 35/7/512 elements into exclusive
        // live arrays; no guest is running while this guard is held.
        check("context save", unsafe {
            soju_context_save(
                self.id(),
                state.registers.as_mut_ptr(),
                state.system.as_mut_ptr(),
                state.vectors.as_mut_ptr(),
            )
        })?;
        state.pending_svc = self.pending_svc;
        state.revision = state.revision.wrapping_add(1);
        Ok(())
    }
}
