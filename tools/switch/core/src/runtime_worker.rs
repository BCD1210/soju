//! Thread-affine vCPU worker and context handoff.
use super::Error;
use crate::hv::{Context, Cpu, Error as HvError, Process, RunBudget, RunOutcome, ThreadState};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
unsafe extern "C" {
    fn soju_worker_qos() -> i32;
}
pub(super) enum Command {
    Run {
        thread: crate::kernel::ThreadId,
        context: Box<Context>,
        results: Option<Vec<u64>>,
        budget: RunBudget,
        barrier: Arc<std::sync::Barrier>,
    },
    Stop,
}
pub(super) struct Worker {
    commands: mpsc::Sender<Command>,
    results: mpsc::Receiver<Result<(Box<Context>, RunOutcome), HvError>>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub(super) fn new(process: Arc<Process>, state: ThreadState) -> Result<Self, Error> {
        let (commands, receive) = mpsc::channel();
        let (send, results) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let join = thread::spawn(move || {
            // SAFETY: adjusts only the calling worker's scheduling class.
            if unsafe { soju_worker_qos() } != 0 {
                let _ = started.send(Err(HvError::InvalidState));
                return;
            }
            let mut cpu = match Cpu::attach(Arc::clone(&process), state) {
                Ok(cpu) => {
                    let _ = started.send(Ok(()));
                    cpu
                }
                Err(e) => {
                    let _ = started.send(Err(e));
                    return;
                }
            };
            let mut active_thread = None;
            while let Ok(Command::Run {
                thread,
                mut context,
                results,
                budget,
                barrier,
            }) = receive.recv()
            {
                let result = (|| {
                    let mut guard = match process.lock() {
                        Ok(guard) => guard,
                        Err(error) => {
                            barrier.wait();
                            return Err(error);
                        }
                    };
                    let prepared = (|| {
                        if active_thread != Some((thread, context.revision)) {
                            cpu.load_context(&guard, &context)?;
                        }
                        if let Some(values) = results {
                            cpu.set_svc_results(&mut guard, &values)?;
                        }
                        Ok::<_, HvError>(())
                    })();
                    if let Err(error) = prepared {
                        drop(guard);
                        barrier.wait();
                        return Err(error);
                    }
                    let outcome = cpu.run_quantum_batch(&mut guard, budget, Some(&barrier))?;
                    cpu.save_context(&guard, &mut context)?;
                    active_thread = Some((thread, context.revision));
                    Ok((context, outcome))
                })();
                let failed = result.is_err();
                if send.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let ready = match ready.recv() {
            Ok(result) => result,
            Err(_) => {
                let _ = join.join();
                return Err(Error::Worker);
            }
        };
        match ready {
            Ok(()) => Ok(Self {
                commands,
                results,
                join: Some(join),
            }),
            Err(error) => {
                let _ = join.join();
                Err(Error::Hypervisor(error))
            }
        }
    }
    pub(super) fn submit(&self, command: Command) -> Result<(), Error> {
        self.commands.send(command).map_err(|_| Error::Worker)
    }
    pub(super) fn receive(&self) -> Result<(Box<Context>, RunOutcome), Error> {
        self.results
            .recv()
            .map_err(|_| Error::Worker)?
            .map_err(Error::Hypervisor)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
