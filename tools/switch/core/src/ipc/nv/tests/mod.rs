use super::*;
use crate::{
    kernel::Kernel,
    memory::{MemoryImage, PAGE},
    nro::{BASE, Homebrew, Region, STACK, TLS},
};
mod commit;
mod ioctls;
mod lifetime;
#[cfg(target_os = "macos")]
mod metal;
mod submission;
struct Fixture {
    nv: Nv,
    kernel: Kernel,
    memory: MemoryImage,
    session: Handle,
}
impl Fixture {
    fn new() -> Self {
        let boot = Homebrew {
            regions: vec![
                Region {
                    address: BASE,
                    bytes: vec![0; PAGE * 4],
                    permissions: 3,
                },
                Region {
                    address: TLS,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
                Region {
                    address: STACK,
                    bytes: vec![0; PAGE],
                    permissions: 3,
                },
                Region {
                    address: crate::kernel::HEAP,
                    bytes: vec![0; PAGE * 64],
                    permissions: 3,
                },
            ],
            pc: BASE,
            sp: STACK + 4096,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: TLS,
        };
        let mut kernel = Kernel::new(&boot);
        let mut memory = MemoryImage::new(&boot).unwrap();
        let mut nv = Nv::default();
        let session = nv
            .open(
                "nvdrv:a",
                &mut Context {
                    kernel: &mut kernel,
                    memory: &mut memory,
                    object: 0,
                    session: 0,
                    client: ClientId(1),
                    now_ns: 0,
                    tls: TLS,
                    nv: None,
                },
            )
            .unwrap();
        Self {
            nv,
            kernel,
            memory,
            session,
        }
    }
    fn request(command_id: u32, data: Vec<u8>) -> Request {
        Request {
            protocol: Protocol::Cmif,
            kind: MessageKind::Request,
            command_id,
            pid: None,
            buffers: vec![],
            data,
            copy_handles: vec![],
            move_handles: vec![],
            domain_objects: vec![],
            objects: vec![],
            domain_target: None,
            token: 0,
        }
    }
    fn call(&mut self, r: &Request) -> Result<Reply, Error> {
        let object = self.kernel.object_id(self.session).unwrap();
        self.nv.dispatch(
            r,
            &mut Context {
                kernel: &mut self.kernel,
                memory: &mut self.memory,
                object,
                session: self.session,
                client: ClientId(1),
                now_ns: 0,
                tls: TLS,
                nv: None,
            },
        )
    }
    fn open(&mut self, name: &str) -> u32 {
        self.memory.write(BASE, name.as_bytes()).unwrap();
        let mut r = Self::request(0, vec![]);
        r.buffers.push(Buffer {
            kind: BufferKind::Send,
            address: BASE,
            len: u64::try_from(name.len()).unwrap(),
            index: 0,
        });
        read_u32(&self.call(&r).unwrap().data, 0).unwrap()
    }
    fn ioctl_request(&mut self, fd: u32, command: u32, data: &[u8], out: usize) -> Request {
        self.memory.write(BASE, data).unwrap();
        let mut payload = fd.to_le_bytes().to_vec();
        payload.extend(command.to_le_bytes());
        let mut r = Self::request(1, payload);
        r.buffers = vec![
            Buffer {
                kind: BufferKind::Pointer,
                address: 0,
                len: 0,
                index: 0,
            },
            Buffer {
                kind: BufferKind::Send,
                address: BASE,
                len: u64::try_from(data.len()).unwrap(),
                index: 1,
            },
            Buffer {
                kind: BufferKind::Receive,
                address: BASE + 4096,
                len: u64::try_from(out).unwrap(),
                index: 2,
            },
        ];
        r
    }
    fn ioctl(&mut self, fd: u32, command: u32, data: &[u8], out: usize) -> Result<Vec<u8>, Error> {
        let r = self.ioctl_request(fd, command, data, out);
        let reply = self.call(&r)?;
        Ok(reply.writes.into_iter().next().unwrap().bytes)
    }
    fn allocation(&mut self, fd: u32, size: u32) -> u32 {
        let mut d = vec![0; 8];
        put32(&mut d, 0, size);
        let id = read_u32(&self.ioctl(fd, 0xc0080101, &d, 8).unwrap(), 4).unwrap();
        let mut d = vec![0; 32];
        put32(&mut d, 0, id);
        put32(&mut d, 8, 2);
        put32(&mut d, 12, 4096);
        put64(&mut d, 24, crate::kernel::HEAP);
        self.ioctl(fd, 0xc0200104, &d, 32).unwrap();
        id
    }
    fn setup(&mut self) -> (u32, u32, u64) {
        let map = self.open("/dev/nvmap");
        let id = self.allocation(map, 0x40000);
        let space = self.open("/dev/nvhost-as-gpu");
        let channel = self.open("/dev/nvhost-gpu");
        let mut d = vec![0; 40];
        put32(&mut d, 0, 1);
        put32(&mut d, 8, 65536);
        self.ioctl(space, 0x40284109, &d, 0).unwrap();
        self.ioctl(space, 0x40044101, &channel.to_le_bytes(), 0)
            .unwrap();
        self.ioctl(channel, 0x40044801, &map.to_le_bytes(), 0)
            .unwrap();
        let mut d = vec![0; 32];
        put32(&mut d, 0, 2048);
        put32(&mut d, 4, 1);
        let reply = self.ioctl(channel, 0xc020481a, &d, 32).unwrap();
        assert_eq!(read_u32(&reply, 12).unwrap(), channel);
        let mut d = vec![0; 40];
        put32(&mut d, 8, id);
        put32(&mut d, 12, 65536);
        let reply = self.ioctl(space, 0xc0284106, &d, 40).unwrap();
        let va = read_u64(&reply, 32).unwrap();
        (channel, space, va)
    }
}
