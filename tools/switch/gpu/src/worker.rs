//! Ordered worker-thread transport. A fence completes GPU execution, not CPU readback.
use crate::{Error, Result};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
};

pub struct Fence(Receiver<Result<String>>);
impl Fence {
    pub fn wait(self) -> Result<String> {
        self.0.recv().map_err(|e| Error(e.to_string()))?
    }
}
struct Job {
    operation: &'static str,
    directory: PathBuf,
    reply: mpsc::Sender<Result<String>>,
}
pub(crate) struct Worker(SyncSender<Job>);
struct Native {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn error(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
impl Native {
    fn start(directory: &std::path::Path) -> Result<Self> {
        let source = directory.join("worker.swift");
        let binary = directory.join("metal-worker");
        std::fs::write(&source, include_str!("../native/worker.swift")).map_err(error)?;
        let output = Command::new("xcrun")
            .args([
                "swiftc",
                "-O",
                "-framework",
                "Metal",
                "-framework",
                "Foundation",
            ])
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .output()
            .map_err(error)?;
        if !output.status.success() {
            return Err(Error(format!(
                "Swift compilation: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let mut child = Command::new(binary)
            .arg("--service")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(error)?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Self {
            child,
            input,
            output,
        })
    }
    fn request(&mut self, job: &Job) -> Result<String> {
        writeln!(self.input, "{}\t{}", job.operation, job.directory.display()).map_err(error)?;
        self.input.flush().map_err(error)?;
        let mut line = String::new();
        self.output.read_line(&mut line).map_err(error)?;
        if let Some(ok) = line.strip_prefix("OK ") {
            Ok(ok.trim().to_string())
        } else {
            Err(Error(format!("Metal worker: {}", line.trim())))
        }
    }
}
impl Worker {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Job>(4);
        std::thread::Builder::new()
            .name("soju-gpu".into())
            .spawn(move || {
                let mut native = None;
                let mut failed = None;
                for job in rx {
                    let result = if let Some(e) = &failed {
                        Err(Error(format!("worker poisoned: {e}")))
                    } else {
                        if native.is_none() {
                            match Native::start(&job.directory) {
                                Ok(n) => native = Some(n),
                                Err(e) => failed = Some(e),
                            }
                        }
                        match &mut native {
                            Some(n) => n.request(&job),
                            None => Err(failed.clone().unwrap()),
                        }
                    };
                    if let Err(e) = &result {
                        failed = Some(e.clone());
                    }
                    let _ = job.reply.send(result);
                }
            })
            .expect("GPU worker thread");
        Self(tx)
    }
    pub fn request(&self, operation: &'static str, directory: PathBuf) -> Result<Fence> {
        let (tx, rx) = mpsc::channel();
        self.0
            .send(Job {
                operation,
                directory,
                reply: tx,
            })
            .map_err(error)?;
        Ok(Fence(rx))
    }
}
