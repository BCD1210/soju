//! Rendezvous between concurrent hardware execution and exclusive host access.
use super::*;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, MutexGuard};
unsafe extern "C" {
    fn soju_preemption_timer(cpu: u64, nanoseconds: u64) -> i32;
}

#[derive(Default)]
struct State {
    host: bool,
    waiters: usize,
    active: Vec<u64>,
    generation: u64,
}
#[derive(Default)]
pub(super) struct Execution {
    state: Mutex<State>,
    changed: Condvar,
}
pub(super) struct MemoryLease<'a>(pub Option<MutexGuard<'a, MemoryImage>>);
impl Deref for MemoryLease<'_> {
    type Target = MemoryImage;
    fn deref(&self) -> &MemoryImage {
        self.0.as_deref().expect("host memory lease")
    }
}
impl DerefMut for MemoryLease<'_> {
    fn deref_mut(&mut self) -> &mut MemoryImage {
        self.0.as_deref_mut().expect("host memory lease")
    }
}
impl Execution {
    pub(super) fn acquire<'a>(
        &self,
        process: &'a Process,
    ) -> Result<MutexGuard<'a, MemoryImage>, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Lock)?;
        state.waiters += 1;
        // One exit request per rendezvous. Repeating it after an exited core
        // wakes the condition variable can cancel that core's next quantum.
        if state.waiters == 1 && !state.active.is_empty() {
            // SAFETY: active IDs cannot be destroyed until their run returns.
            let result =
                unsafe { ffi::hv_vcpus_exit(state.active.as_ptr(), state.active.len() as u32) };
            if let Err(error) = check("rendezvous exit", result) {
                state.waiters -= 1;
                self.changed.notify_all();
                return Err(error);
            }
        }
        while state.host || !state.active.is_empty() {
            state = self.changed.wait(state).map_err(|_| Error::Lock)?;
        }
        state.waiters -= 1;
        let memory = process.memory.lock().map_err(|_| Error::Lock)?;
        state.host = true;
        Ok(memory)
    }
}
impl ProcessGuard<'_> {
    fn release(&mut self) {
        if !self.held {
            return;
        }
        let execution = &self.process.execution;
        let mut state = execution.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = self.memory.generation();
        self.memory.0.take();
        self.held = false;
        state.host = false;
        execution.changed.notify_all();
    }
    pub(super) fn wait_barrier(&mut self, barrier: &std::sync::Barrier) -> Result<(), Error> {
        self.release();
        barrier.wait();
        self.memory.0 = Some(self.process.execution.acquire(self.process)?);
        self.held = true;
        Ok(())
    }
    pub(super) fn run_vcpu(
        &mut self,
        id: u64,
        generation: u64,
        barrier: Option<&std::sync::Barrier>,
        duration: Duration,
    ) -> Result<bool, Error> {
        self.release();
        if let Some(barrier) = barrier {
            barrier.wait();
        }
        let execution = &self.process.execution;
        let mut state = execution.state.lock().map_err(|_| Error::Lock)?;
        while state.host || state.waiters != 0 {
            state = execution.changed.wait(state).map_err(|_| Error::Lock)?;
        }
        let current = state.generation == generation;
        let result = if current {
            state.active.push(id);
            self.process
                .profile
                .max_parallel
                .fetch_max(state.active.len() as u64, Ordering::Relaxed);
            drop(state);
            // SAFETY: owner thread only; the active lease excludes all host arena access.
            let result = (|| {
                // SAFETY: timer and vCPU are owned by the current host thread.
                check("preemption timer", unsafe {
                    soju_preemption_timer(id, duration.as_nanos().min(u64::MAX as u128) as u64)
                })?;
                let result = check("hv_vcpu_run", unsafe { ffi::hv_vcpu_run(id) });
                check("disable timer", unsafe {
                    ffi::hv_vcpu_set_sys_reg(id, 0xdf19, 0)
                })?;
                result
            })();
            state = execution.state.lock().map_err(|_| Error::Lock)?;
            state.active.retain(|v| *v != id);
            execution.changed.notify_all();
            result
        } else {
            Ok(())
        };
        drop(state);
        self.memory.0 = Some(execution.acquire(self.process)?);
        self.held = true;
        result.map(|()| current)
    }
}
impl Drop for ProcessGuard<'_> {
    fn drop(&mut self) {
        self.release();
    }
}
