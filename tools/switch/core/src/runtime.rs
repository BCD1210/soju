//! Four permanent core workers with saved guest contexts and deterministic scheduling.
//! Only the selected worker receives a run command. Every quantum and host memory
//! operation holds the shared ProcessGuard; channel waits and sleeping never do.
use crate::hv::{Context, Error as HvError, Process, RunBudget, RunOutcome, ThreadState};
use crate::kernel::{Dispatch, IpcRequest, Kernel, MAIN_THREAD, ThreadId, ThreadStatus};
use crate::memory::GuestMemory;
use crate::nro::Homebrew;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
#[path = "runtime_worker.rs"]
mod worker;
use std::time::{Duration, Instant};
use worker::{Command, Worker};

#[derive(Debug)]
pub enum Error {
    Hypervisor(HvError),
    Worker,
    Kernel(u64),
    Ipc(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<HvError> for Error {
    fn from(e: HvError) -> Self {
        Self::Hypervisor(e)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    ProcessExited(u32),
    AllThreadsExited,
    Break {
        reason: u32,
        address: u64,
        size: u64,
    },
    Unsupported {
        thread: ThreadId,
        svc: u16,
        pc: u64,
    },
    IpcRequired(IpcRequest),
    BudgetExpired,
}
#[derive(Debug)]
pub enum IpcReply {
    Complete(Vec<u64>),
    Deferred,
    Unhandled,
}
/// Handlers use the borrow-only interface, never ownership of the mapped arena.
/// Deferred requests must eventually call Kernel::complete_ipc from poll().
pub trait IpcHandler {
    fn frame_ready(&self) -> bool {
        false
    }
    fn dispatch(
        &mut self,
        request: &IpcRequest,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
    ) -> Result<IpcReply, String>;
    fn poll(
        &mut self,
        _kernel: &mut Kernel,
        _memory: &mut dyn GuestMemory,
        _now: u64,
    ) -> Result<(), String> {
        Ok(())
    }
}
pub struct NoIpc;
impl IpcHandler for NoIpc {
    fn dispatch(
        &mut self,
        _request: &IpcRequest,
        _kernel: &mut Kernel,
        _memory: &mut dyn GuestMemory,
    ) -> Result<IpcReply, String> {
        Ok(IpcReply::Unhandled)
    }
}
#[derive(Debug, Default)]
pub struct RuntimeProfile {
    pub worker: Duration,
    pub svc: Duration,
    pub ipc: Duration,
    pub poll: Duration,
    pub sleep: Duration,
}
pub struct Runtime {
    pub profile: RuntimeProfile,
    pub yield_on_frame: bool,
    process: Arc<Process>,
    kernel: Kernel,
    workers: BTreeMap<ThreadId, Box<Context>>,
    cores: Vec<Worker>,
    pub quantum: RunBudget,
    pub schedule: std::collections::VecDeque<ThreadId>,
    pending_ipc: Option<IpcRequest>,
}
impl Runtime {
    pub fn new(boot: &Homebrew) -> Result<Self, Error> {
        let process = Process::new(boot)?;
        let mut workers = BTreeMap::new();
        workers.insert(
            MAIN_THREAD,
            Box::new(Context::from(ThreadState::from_homebrew(boot))),
        );
        let mut cores = Vec::with_capacity(4);
        for _ in 0..4 {
            cores.push(Worker::new(
                Arc::clone(&process),
                ThreadState::from_homebrew(boot),
            )?);
        }
        Ok(Self {
            profile: RuntimeProfile::default(),
            yield_on_frame: false,
            process,
            kernel: Kernel::new(boot),
            workers,
            cores,
            quantum: RunBudget::default(),
            schedule: std::collections::VecDeque::new(),
            pending_ipc: None,
        })
    }
    pub fn from_program(program: &crate::program::Program) -> Result<Self, Error> {
        let mut runtime = Self::new(&program.boot)?;
        runtime.kernel.configure_program(program);
        Ok(runtime)
    }
    pub fn process(&self) -> Arc<Process> {
        Arc::clone(&self.process)
    }
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }
    pub fn kernel_mut(&mut self) -> &mut Kernel {
        &mut self.kernel
    }
    fn now(&self) -> u64 {
        self.process.elapsed_ns()
    }
    fn sync_workers(&mut self) -> Result<(), Error> {
        let new: Vec<_> = self
            .kernel
            .threads()
            .filter(|(id, t)| !self.workers.contains_key(id) && t.status != ThreadStatus::Exited)
            .map(|(id, t)| (id, t.spec.clone()))
            .collect();
        for (id, spec) in new {
            let mut registers = [0; 31];
            registers[0] = spec.argument;
            registers[30] = spec.link;
            let state = ThreadState {
                registers,
                pc: spec.entry,
                sp: spec.stack,
                tpidrro_el0: spec.tls,
            };
            self.workers.insert(id, Box::new(Context::from(state)));
        }
        let exited: Vec<_> = self
            .workers
            .keys()
            .copied()
            .filter(|id| {
                self.kernel
                    .thread(*id)
                    .is_none_or(|t| t.status == ThreadStatus::Exited)
            })
            .collect();
        for id in exited {
            self.workers.remove(&id);
        }
        Ok(())
    }
    fn ipc(
        &mut self,
        handler: &mut dyn IpcHandler,
        request: IpcRequest,
    ) -> Result<Option<Stop>, Error> {
        let mut guard = self.process.lock()?;
        let started = Instant::now();
        let reply = handler
            .dispatch(&request, &mut self.kernel, guard.memory())
            .map_err(Error::Ipc)?;
        self.profile.ipc += started.elapsed();
        match reply {
            IpcReply::Complete(results) => {
                self.kernel
                    .complete_ipc(request.thread, results)
                    .map_err(Error::Kernel)?;
                Ok(None)
            }
            IpcReply::Deferred => Ok(None),
            IpcReply::Unhandled => {
                self.pending_ipc = Some(request.clone());
                Ok(Some(Stop::IpcRequired(request)))
            }
        }
    }
    /// A budget stop is explicit and resumable; it never means guest success.
    pub fn run(
        &mut self,
        handler: &mut dyn IpcHandler,
        max_duration: Duration,
        max_quanta: u64,
    ) -> Result<Stop, Error> {
        let deadline = Instant::now() + max_duration;
        let mut quanta = 0;
        if let Some(request) = self.pending_ipc.take()
            && let Some(stop) = self.ipc(handler, request)?
        {
            return Ok(stop);
        }
        loop {
            if Instant::now() >= deadline
                || quanta >= max_quanta
                || (self.yield_on_frame && handler.frame_ready())
            {
                return Ok(Stop::BudgetExpired);
            }
            let now = self.now();
            let poll_started = Instant::now();
            {
                let mut guard = self.process.lock()?;
                handler
                    .poll(&mut self.kernel, guard.memory(), now)
                    .map_err(Error::Ipc)?;
                self.kernel
                    .advance(guard.memory(), now)
                    .map_err(Error::Kernel)?;
            }
            self.profile.poll += poll_started.elapsed();
            self.sync_workers()?;
            if self.kernel.all_exited() {
                return Ok(Stop::AllThreadsExited);
            }
            let Some(id) = self.kernel.next_runnable() else {
                let wait = self
                    .kernel
                    .next_deadline()
                    .map_or(1_000_000, |d| d.saturating_sub(now).min(1_000_000));
                let started = Instant::now();
                thread::sleep(Duration::from_nanos(wait.max(1)));
                self.profile.sleep += started.elapsed();
                continue;
            };
            let results = self.kernel.take_results(id);
            if self.schedule.len() == 1024 {
                self.schedule.pop_front();
            }
            self.schedule.push_back(id);
            quanta += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let budget = RunBudget {
                max_duration: self.quantum.max_duration.min(remaining),
                max_exits: self.quantum.max_exits,
            };
            let started = Instant::now();
            let thread = self.kernel.thread(id).ok_or(Error::Worker)?;
            let core = if thread.ideal_core >= 0 {
                usize::try_from(thread.ideal_core).map_err(|_| Error::Worker)?
            } else {
                usize::try_from(thread.affinity.trailing_zeros()).map_err(|_| Error::Worker)?
            };
            let context = self.workers.remove(&id).ok_or(Error::Worker)?;
            let (context, outcome) =
                self.cores
                    .get(core)
                    .ok_or(Error::Worker)?
                    .run(Command::Run {
                        context,
                        results,
                        budget,
                    })?;
            self.workers.insert(id, context);
            self.profile.worker += started.elapsed();
            match outcome {
                RunOutcome::Yield(_) => self.kernel.requeue(id),
                RunOutcome::Svc(exit) => {
                    let now = self.now();
                    let started = Instant::now();
                    let dispatch = {
                        let mut guard = self.process.lock()?;
                        self.kernel.dispatch(
                            guard.memory(),
                            id,
                            exit.immediate,
                            &exit.registers,
                            exit.guest_pc,
                            now,
                        )
                    };
                    self.profile.svc += started.elapsed();
                    match dispatch {
                        Dispatch::Return(mut results) => {
                            if exit.immediate == 30 {
                                results = vec![self.process.ticks()];
                            }
                            self.kernel
                                .set_results(id, results)
                                .map_err(Error::Kernel)?;
                            self.kernel.requeue(id);
                        }
                        Dispatch::Blocked | Dispatch::ThreadExited => {}
                        Dispatch::ProcessExited(code) => return Ok(Stop::ProcessExited(code)),
                        Dispatch::Break {
                            reason,
                            address,
                            size,
                        } => {
                            return Ok(Stop::Break {
                                reason,
                                address,
                                size,
                            });
                        }
                        Dispatch::Unsupported(svc) => {
                            return Ok(Stop::Unsupported {
                                thread: id,
                                svc,
                                pc: exit.guest_pc,
                            });
                        }
                        Dispatch::Ipc(request) => {
                            if let Some(stop) = self.ipc(handler, request)? {
                                return Ok(stop);
                            }
                        }
                    }
                }
            }
        }
    }
}
