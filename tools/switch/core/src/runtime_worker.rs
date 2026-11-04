//! Thread-affine vCPU worker and context handoff.
use super::Error;
use crate::hv::{Context, Cpu, Error as HvError, Process, RunBudget, RunOutcome, ThreadState};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
pub(super) enum Command {
    Run {
        thread: crate::kernel::ThreadId,
        context: Box<Context>,
        results: Option<Vec<u64>>,
        budget: RunBudget,
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
            }) = receive.recv()
            {
                let result = (|| {
                    let mut guard = process.lock()?;
                    if active_thread != Some((thread, context.revision)) {
                        cpu.load_context(&guard, &context)?;
                    }
                    if let Some(values) = results {
                        cpu.set_svc_results(&mut guard, &values)?;
                    }
                    let outcome = cpu.run_quantum(&mut guard, budget)?;
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
    pub(super) fn run(&self, command: Command) -> Result<(Box<Context>, RunOutcome), Error> {
        self.commands.send(command).map_err(|_| Error::Worker)?;
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
