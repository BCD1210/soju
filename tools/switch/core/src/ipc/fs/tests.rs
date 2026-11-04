use super::*;
use crate::{
    kernel::Kernel,
    memory::{MemoryImage, PAGE},
    nro::{BASE, Homebrew, LOADER_RETURN, Region, STACK, TLS},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    fs: Filesystem,
    kernel: Kernel,
    memory: MemoryImage,
    home: PathBuf,
    nro: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../dist/fs-ipc-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let home = root.join("home");
        let nro = root.join("fixture.nro");
        std::fs::write(&nro, b"synthetic NRO\0ASET\0RomFS assets exact bytes\xff").unwrap();
        let boot = Homebrew {
            regions: vec![
                Region {
                    address: BASE,
                    bytes: vec![0; PAGE * 4],
                    permissions: 3,
                },
                Region {
                    address: STACK,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
                Region {
                    address: TLS,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
            ],
            pc: BASE,
            sp: STACK + PAGE as u64,
            x0: 0,
            x1: 0,
            x30: LOADER_RETURN,
            tpidrro_el0: TLS,
        };
        Self {
            fs: Filesystem::new(&nro, &home).unwrap(),
            kernel: Kernel::new(&boot),
            memory: MemoryImage::new(&boot).unwrap(),
            home,
            nro,
            root,
        }
    }
    fn open(&mut self) -> Handle {
        let mut ctx = Context {
            kernel: &mut self.kernel,
            memory: &mut self.memory,
            object: 0,
            session: 0,
            client: ClientId(1),
            now_ns: 0,
            tls: TLS,
            nv: None,
        };
        self.fs.open("fsp-srv", &mut ctx).unwrap()
    }
    fn call(
        &mut self,
        handle: Handle,
        command: u32,
        data: Vec<u8>,
        input: Option<&[u8]>,
        output: Option<usize>,
    ) -> Result<Reply, Error> {
        let mut buffers = vec![];
        if let Some(input) = input {
            self.memory.write(BASE, input).unwrap();
            buffers.push(Buffer {
                kind: BufferKind::Send,
                address: BASE,
                len: input.len() as u64,
                index: 7,
            });
        }
        if let Some(output) = output {
            buffers.push(Buffer {
                kind: BufferKind::Receive,
                address: BASE + PAGE as u64,
                len: output as u64,
                index: 11,
            });
        }
        let req = Request {
            protocol: Protocol::Cmif,
            kind: MessageKind::Request,
            command_id: command,
            pid: None,
            buffers,
            data,
            copy_handles: vec![],
            move_handles: vec![],
            objects: vec![],
            domain_objects: vec![],
            domain_target: None,
            token: 0,
        };
        let object = self.kernel.object_id(handle).unwrap();
        let mut ctx = Context {
            kernel: &mut self.kernel,
            memory: &mut self.memory,
            object,
            session: handle,
            client: ClientId(1),
            now_ns: 0,
            tls: TLS,
            nv: None,
        };
        self.fs.dispatch(&req, &mut ctx)
    }
    fn child(&mut self, h: Handle, command: u32, data: Vec<u8>, path: Option<&[u8]>) -> Handle {
        self.call(h, command, data, path, None).unwrap().objects[0]
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn rw(option: u32, offset: u64, size: u64) -> Vec<u8> {
    let mut v = option.to_le_bytes().to_vec();
    v.extend([0; 4]);
    v.extend(offset.to_le_bytes());
    v.extend(size.to_le_bytes());
    v
}
fn create(size: u64) -> Vec<u8> {
    let mut v = vec![0; 8];
    v.extend(size.to_le_bytes());
    v
}
fn number(reply: Reply) -> u64 {
    let mut b = [0; 8];
    b[..reply.data.len()].copy_from_slice(&reply.data);
    u64::from_le_bytes(b)
}
fn attribute(id: u8) -> [u8; 64] {
    let mut a = [0; 64];
    a[0] = id;
    a[8] = 1;
    a[32] = 1;
    a
}
fn create_save(f: &mut Fixture, proxy: Handle, id: u8) -> Handle {
    let a = attribute(id);
    let mut payload = a.to_vec();
    payload.extend(1024u64.to_le_bytes());
    payload.resize(101, 0);
    payload[100] = 1;
    f.call(proxy, 22, payload, None, None).unwrap();
    let mut payload = vec![1, 0, 0, 0, 0, 0, 0, 0];
    payload.extend(a);
    f.child(proxy, 51, payload, None)
}
#[test]
fn sd_write_flush_close_restart_and_readonly_complete_nro() {
    let mut f = Fixture::new();
    let proxy = f.open();
    f.call(proxy, 1, vec![0; 8], None, None).unwrap();
    let sd = f.child(proxy, 18, vec![], None);
    assert_eq!(
        f.call(sd, 2, vec![], Some(b"/switch/2048\0"), None)
            .unwrap()
            .result,
        0
    );
    f.call(sd, 0, create(0), Some(b"/switch/2048/state\0"), None)
        .unwrap();
    let file = f.child(
        sd,
        8,
        7u32.to_le_bytes().to_vec(),
        Some(b"/switch/2048/state\0"),
    );
    f.call(file, 1, rw(0, 0, 5), Some(b"state"), None).unwrap();
    assert_eq!(number(f.call(file, 4, vec![], None, None).unwrap()), 5);
    f.call(file, 2, vec![], None, None).unwrap();
    f.fs.close_object(f.kernel.object_id(file).unwrap());
    let self_file = f.child(
        sd,
        8,
        1u32.to_le_bytes().to_vec(),
        Some(b"/switch/soju-current.nro\0"),
    );
    let bytes = std::fs::read(&f.nro).unwrap();
    assert_eq!(
        number(f.call(self_file, 4, vec![], None, None).unwrap()),
        bytes.len() as u64
    );
    let reply = f
        .call(
            self_file,
            0,
            rw(0, 0, bytes.len() as u64),
            None,
            Some(bytes.len()),
        )
        .unwrap();
    assert_eq!(reply.writes[0].bytes, bytes);
    assert_eq!(reply.writes[0].buffer_index, 11);
    assert!(f.call(self_file, 1, rw(0, 0, 1), Some(b"x"), None).is_err());
    assert_eq!(
        f.call(
            sd,
            8,
            2u32.to_le_bytes().to_vec(),
            Some(b"/switch/soju-current.nro\0"),
            None
        )
        .unwrap()
        .result,
        0x202
    );
    let other = f.root.join("other");
    let replacement = Filesystem::new(&f.nro, &other).unwrap();
    drop(std::mem::replace(&mut f.fs, replacement));
    f.fs = Filesystem::new(&f.nro, &f.home).unwrap();
    let proxy = f.open();
    let sd = f.child(proxy, 18, vec![], None);
    let file = f.child(
        sd,
        8,
        1u32.to_le_bytes().to_vec(),
        Some(b"/switch/2048/state\0"),
    );
    assert_eq!(
        f.call(file, 0, rw(0, 0, 20), None, Some(20))
            .unwrap()
            .writes[0]
            .bytes,
        b"state"
    );
    assert_eq!(f.fs.game_hash().len(), 64);
    assert_eq!(
        f.fs.program_id(),
        0x01ff_0000_0000_0000 | u64::from_str_radix(&f.fs.game_hash()[..12], 16).unwrap()
    );
}
#[test]
fn file_modes_bounds_directory_cursor_and_path_validation() {
    let mut f = Fixture::new();
    let proxy = f.open();
    let sd = f.child(proxy, 18, vec![], None);
    f.call(sd, 0, create(4), Some(b"/switch/b\0"), None)
        .unwrap();
    f.call(sd, 2, vec![], Some(b"/switch/a\0"), None).unwrap();
    assert!(
        f.call(sd, 0, create(0), Some(b"/../outside\0"), None)
            .is_err()
    );
    assert!(
        f.call(sd, 7, vec![], Some(b"/switch/no-nul"), None)
            .is_err()
    );
    assert_eq!(
        number(f.call(sd, 7, vec![], Some(b"/switch/b\0"), None).unwrap()),
        1
    );
    assert_eq!(
        f.call(sd, 14, vec![], Some(b"/switch/b\0"), None)
            .unwrap()
            .result,
        0
    );
    assert_eq!(
        f.call(sd, 14, vec![], Some(b"/absent\0"), None)
            .unwrap()
            .result,
        0x202
    );
    let file = f.child(sd, 8, 3u32.to_le_bytes().to_vec(), Some(b"/switch/b\0"));
    assert!(f.call(file, 1, rw(0, 4, 1), Some(b"x"), None).is_err());
    assert!(f.call(file, 0, rw(0, u64::MAX, 1), None, Some(1)).is_err());
    assert!(f.call(file, 0, rw(0, 0, 8), None, Some(4)).is_err());
    f.call(file, 3, 2u64.to_le_bytes().to_vec(), None, None)
        .unwrap();
    assert_eq!(number(f.call(file, 4, vec![], None, None).unwrap()), 2);
    f.call(file, 1, rw(1, 0, 2), Some(b"ok"), None).unwrap();
    let readonly = f.child(sd, 8, 1u32.to_le_bytes().to_vec(), Some(b"/switch/b\0"));
    assert!(
        f.call(readonly, 3, 0u64.to_le_bytes().to_vec(), None, None)
            .is_err()
    );
    let directory = f.child(sd, 9, 3u32.to_le_bytes().to_vec(), Some(b"/switch\0"));
    assert_eq!(number(f.call(directory, 1, vec![], None, None).unwrap()), 3);
    assert_eq!(
        number(
            f.call(directory, 0, vec![], None, Some(DIRECTORY_ENTRY - 1))
                .unwrap()
        ),
        0
    );
    let first = f
        .call(directory, 0, vec![], None, Some(DIRECTORY_ENTRY))
        .unwrap();
    assert_eq!(first.writes[0].bytes[0], b'a');
    assert_eq!(first.writes[0].bytes[0x304], 0);
    let rest = f
        .call(directory, 0, vec![], None, Some(2 * DIRECTORY_ENTRY))
        .unwrap();
    assert_eq!(number(Reply::data(rest.data)), 2);
    assert_eq!(rest.writes[0].bytes[0], b'b');
    assert_eq!(rest.writes[0].bytes[0x304], 1);
    assert_eq!(
        number(
            f.call(directory, 0, vec![], None, Some(DIRECTORY_ENTRY))
                .unwrap()
        ),
        0
    );
    assert_eq!(number(f.call(directory, 1, vec![], None, None).unwrap()), 3);
}
#[test]
fn independent_account_commits_and_save_reader_cursor() {
    let mut f = Fixture::new();
    let proxy = f.open();
    let a = create_save(&mut f, proxy, 1);
    let b = create_save(&mut f, proxy, 2);
    for (volume, text) in [(a, b"A"), (b, b"B")] {
        f.call(volume, 0, create(1), Some(b"/data\0"), None)
            .unwrap();
        let file = f.child(volume, 8, 3u32.to_le_bytes().to_vec(), Some(b"/data\0"));
        f.call(file, 1, rw(1, 0, 1), Some(text), None).unwrap();
        f.call(file, 2, vec![], None, None).unwrap();
    }
    f.call(a, 10, vec![], None, None).unwrap();
    let disk = f.fs.store.load_saves().unwrap();
    assert_eq!(disk[&attribute(1)].tree.files["/data"], b"A");
    assert!(disk[&attribute(2)].tree.files.is_empty());
    f.call(b, 10, vec![], None, None).unwrap();
    assert_eq!(
        f.fs.store.load_saves().unwrap()[&attribute(2)].tree.files["/data"],
        b"B"
    );
    let reader = f.child(proxy, 61, vec![1], None);
    let first = f.call(reader, 0, vec![], None, Some(0x60)).unwrap();
    assert_eq!(number(Reply::data(first.data)), 1);
    assert_eq!(first.writes[0].bytes[8..10], [1, 1]);
    assert_eq!(
        number(f.call(reader, 0, vec![], None, Some(0x60)).unwrap()),
        1
    );
    assert_eq!(
        number(f.call(reader, 0, vec![], None, Some(0x60)).unwrap()),
        0
    );
    let invalid = f.call(proxy, 51, vec![0; 72], None, None);
    assert!(invalid.is_err());
}

#[test]
fn sd_finish_boundary_and_account_failed_commit_keep_previous_snapshot() {
    let mut f = Fixture::new();
    let proxy = f.open();
    let sd = f.child(proxy, 18, vec![], None);
    f.call(sd, 0, create(2), Some(b"/switch/finish\0"), None)
        .unwrap();
    let file = f.child(
        sd,
        8,
        7u32.to_le_bytes().to_vec(),
        Some(b"/switch/finish\0"),
    );
    f.call(file, 1, rw(0, 1, 1), Some(b"x"), None).unwrap();
    assert!(
        !f.fs
            .store
            .load_sd()
            .unwrap()
            .files
            .contains_key("/switch/finish")
    );
    f.fs.finish().unwrap();
    assert_eq!(
        f.fs.store.load_sd().unwrap().files["/switch/finish"],
        [0, b'x']
    );
    f.call(file, 1, rw(1, 2, 1), Some(b"y"), None).unwrap();
    assert_eq!(
        f.fs.store.load_sd().unwrap().files["/switch/finish"],
        [0, b'x', b'y']
    );
    let save = create_save(&mut f, proxy, 3);
    f.call(save, 0, create(1025), Some(b"/too-large\0"), None)
        .unwrap();
    assert!(f.call(save, 10, vec![], None, None).is_err());
    assert!(
        f.fs.store.load_saves().unwrap()[&attribute(3)]
            .tree
            .files
            .is_empty()
    );
    assert!(f.call(file, 0, vec![0; 23], None, Some(1)).is_err());
    assert!(f.call(sd, 99, vec![], None, None).is_err());
}

#[path = "../../../tests/common/program.rs"]
mod program_fixture;
#[test]
fn program_romfs_storage_is_readonly_and_bounded() {
    let mut f = Fixture::new();
    let (bytes, keys, expected) = program_fixture::package("nsp", true);
    let program = crate::program::Program::load(&bytes, "nsp", &keys).unwrap();
    f.fs = Filesystem::new_program(&program, &f.home.join("program")).unwrap();
    let proxy = f.open();
    let storage = f.child(proxy, 200, vec![], None);
    assert_eq!(
        number(f.call(storage, 4, vec![], None, None).unwrap()),
        expected.len() as u64
    );
    let range = |offset: u64, size: u64| [offset.to_le_bytes(), size.to_le_bytes()].concat();
    let read = f.call(storage, 0, range(4, 5), None, Some(5)).unwrap();
    assert_eq!(read.writes[0].bytes, expected[4..9]);
    assert!(
        f.call(
            storage,
            0,
            range(expected.len() as u64 - 1, 2),
            None,
            Some(2)
        )
        .is_err()
    );
    assert!(f.call(storage, 1, range(0, 1), Some(b"x"), None).is_err());
}
