//! Ordered in-process command ownership transfer. Fences complete GPU execution.
use crate::{
    protocol::{Array, Command as DrawCommand},
    Error, Result,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
};

pub struct Fence(Receiver<Result<String>>);
impl Fence {
    pub fn wait(self) -> Result<String> {
        self.0.recv().map_err(error)?
    }
}
type Batch = (Vec<DrawCommand>, Vec<Vec<u8>>, Vec<u8>);
struct Job {
    directory: PathBuf,
    batch: Option<Batch>,
    reply: mpsc::Sender<Result<String>>,
}
const CAPACITY: usize = 8;
struct Ring {
    slots: [AtomicPtr<Job>; CAPACITY],
    closed: AtomicBool,
}
impl Drop for Ring {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            let pointer = *slot.get_mut();
            if !pointer.is_null() {
                // Last Arc owner; no consumer can still access these remaining jobs.
                unsafe {
                    drop(Box::from_raw(pointer));
                }
            }
        }
    }
}
pub(crate) struct Worker {
    ring: Arc<Ring>,
    next: usize,
    thread: thread::JoinHandle<()>,
}
fn error(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
impl Worker {
    pub fn new() -> Self {
        let ring = Arc::new(Ring {
            slots: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            closed: AtomicBool::new(false),
        });
        let receiver = ring.clone();
        let thread = thread::Builder::new()
            .name("soju-gpu".into())
            .spawn(move || {
                let mut native = None;
                let mut failed = None;
                let mut next = 0;
                loop {
                    let pointer =
                        receiver.slots[next].swap(std::ptr::null_mut(), Ordering::Acquire);
                    if pointer.is_null() {
                        if receiver.closed.load(Ordering::Acquire)
                            && receiver.slots[next].load(Ordering::Acquire).is_null()
                        {
                            break;
                        }
                        thread::park_timeout(std::time::Duration::from_millis(1));
                        continue;
                    }
                    next = (next + 1) % CAPACITY;
                    // Release publication transferred exclusive Box ownership to this thread.
                    let job = unsafe { Box::from_raw(pointer) };
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
        Self {
            ring,
            next: 0,
            thread,
        }
    }
    fn enqueue(&mut self, directory: PathBuf, batch: Option<Batch>) -> Result<Fence> {
        let (reply, rx) = mpsc::channel();
        let job = Box::into_raw(Box::new(Job {
            directory,
            batch,
            reply,
        }));
        let slot = &self.ring.slots[self.next];
        // A single producer is enforced by &mut self; a full ring applies backpressure.
        while slot
            .compare_exchange(
                std::ptr::null_mut(),
                job,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_err()
        {
            if self.thread.is_finished() {
                // Publication failed, so this allocation still belongs to the producer.
                unsafe {
                    drop(Box::from_raw(job));
                }
                return Err(Error("GPU worker stopped".into()));
            }
            thread::yield_now();
        }
        self.next = (self.next + 1) % CAPACITY;
        self.thread.thread().unpark();
        Ok(Fence(rx))
    }
    pub fn submit(
        &mut self,
        directory: PathBuf,
        commands: Vec<DrawCommand>,
        files: Vec<Vec<u8>>,
        constants: Vec<u8>,
    ) -> Result<Fence> {
        self.enqueue(directory, Some((commands, files, constants)))
    }
    pub fn request(&mut self, _operation: &'static str, directory: PathBuf) -> Result<Fence> {
        self.enqueue(directory, None)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.ring.closed.store(true, Ordering::Release);
        self.thread.thread().unpark();
    }
}

#[cfg(target_os = "macos")]
struct Native {
    instance: *mut std::ffi::c_void,
    run: Run,
    readback: Readback,
    destroy: Destroy,
}
#[cfg(target_os = "macos")]
type Run = unsafe extern "C" fn(
    *mut std::ffi::c_void,
    *const DrawCommand,
    usize,
    *const Array<u8>,
    usize,
    *const Array<u8>,
) -> *mut std::ffi::c_char;
#[cfg(target_os = "macos")]
type Readback =
    unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_char) -> *mut std::ffi::c_char;
#[cfg(target_os = "macos")]
type Destroy = unsafe extern "C" fn(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dlopen(path: *const std::ffi::c_char, mode: i32) -> *mut std::ffi::c_void;
    fn dlsym(handle: *mut std::ffi::c_void, name: *const std::ffi::c_char)
        -> *mut std::ffi::c_void;
    fn free(pointer: *mut std::ffi::c_void);
}
#[cfg(target_os = "macos")]
impl Native {
    fn start(directory: &std::path::Path) -> Result<Self> {
        use std::{ffi::CString, process::Command};
        static LIBRARY: std::sync::OnceLock<Result<usize>> = std::sync::OnceLock::new();
        let library = *LIBRARY
            .get_or_init(|| {
                let source = directory.join("worker.swift");
                let header = directory.join("protocol.h");
                let binary = directory.join("metal-worker.dylib");
                std::fs::write(
                    &source,
                    concat!(
                        include_str!("../native/protocol.swift"),
                        "\n",
                        include_str!("../native/worker.swift")
                    ),
                )
                .map_err(error)?;
                std::fs::write(&header, include_str!("../native/protocol.h")).map_err(error)?;
                let output = Command::new("xcrun")
                    .args(["swiftc", "-O", "-emit-library", "-import-objc-header"])
                    .arg(header)
                    .args(["-framework", "Metal", "-framework", "Foundation"])
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
                let path = CString::new(binary.as_os_str().as_encoded_bytes()).map_err(error)?;
                // Swift runtime metadata remains registered for the process lifetime.
                let library = unsafe { dlopen(path.as_ptr(), 2) };
                if library.is_null() {
                    Err(Error("cannot load Metal worker".into()))
                } else {
                    Ok(library as usize)
                }
            })
            .as_ref()
            .map_err(Clone::clone)? as *mut std::ffi::c_void;
        // Symbols come from the embedded sources compiled with the matching C header.
        unsafe {
            let symbols = [
                c"soju_create",
                c"soju_run",
                c"soju_readback",
                c"soju_destroy",
            ]
            .map(|name| dlsym(library, name.as_ptr()));
            if symbols.iter().any(|p| p.is_null()) {
                return Err(Error("Metal worker ABI symbols missing".into()));
            }
            let create: unsafe extern "C" fn() -> *mut std::ffi::c_void =
                std::mem::transmute(symbols[0]);
            let instance = create();
            if instance.is_null() {
                return Err(Error("no default Metal device/queue".into()));
            }
            Ok(Self {
                instance,
                run: std::mem::transmute::<*mut std::ffi::c_void, Run>(symbols[1]),
                readback: std::mem::transmute::<*mut std::ffi::c_void, Readback>(symbols[2]),
                destroy: std::mem::transmute::<*mut std::ffi::c_void, Destroy>(symbols[3]),
            })
        }
    }
    fn request(&mut self, job: &Job) -> Result<String> {
        // The native call is synchronous through GPU completion. Every borrowed allocation
        // and command stays owned by this job until the call returns.
        let pointer = unsafe {
            if let Some((commands, files, constants)) = &job.batch {
                let resources = files.iter().map(|f| Array::borrowed(f)).collect::<Vec<_>>();
                let constant_view = Array::borrowed(constants);
                (self.run)(
                    self.instance,
                    commands.as_ptr(),
                    commands.len(),
                    resources.as_ptr().cast(),
                    resources.len(),
                    (&constant_view as *const crate::protocol::View<'_, u8>).cast(),
                )
            } else {
                let path = std::ffi::CString::new(job.directory.as_os_str().as_encoded_bytes())
                    .map_err(error)?;
                (self.readback)(self.instance, path.as_ptr())
            }
        };
        if pointer.is_null() {
            return Err(Error("native result allocation failed".into()));
        }
        // strdup returns an owned NUL-terminated string, released with the matching allocator.
        let message = unsafe {
            let s = std::ffi::CStr::from_ptr(pointer)
                .to_string_lossy()
                .into_owned();
            free(pointer.cast());
            s
        };
        match message.strip_prefix("OK ") {
            Some(value) => Ok(value.into()),
            None => Err(Error(message)),
        }
    }
}
#[cfg(target_os = "macos")]
impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.instance);
        }
    }
}
#[cfg(not(target_os = "macos"))]
struct Native;
#[cfg(not(target_os = "macos"))]
impl Native {
    fn start(_: &std::path::Path) -> Result<Self> {
        Err(Error("Metal requires macOS".into()))
    }
    fn request(&mut self, _: &Job) -> Result<String> {
        unreachable!()
    }
}
