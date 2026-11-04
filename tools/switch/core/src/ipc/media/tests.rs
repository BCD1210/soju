use super::*;
use crate::{
    kernel::{Dispatch, INVALID_STATE, MAIN_THREAD},
    memory::{MemoryImage, PAGE},
    nro::{BASE, Homebrew, Region, TLS},
};
use std::cell::Cell;
const INPUT: u64 = BASE + 0x1000;
const OUTPUT_ADDRESS: u64 = BASE + 0x2000;
const PIXELS: u64 = BASE + 0x4000;
struct Nv {
    generation: Cell<u64>,
    alive: Cell<bool>,
}
impl NvBufferLookup for Nv {
    fn resolve(&self, client: ClientId, id: u32) -> Result<NvBufferView, Error> {
        if client != ClientId(1) || id != 7 || !self.alive.get() {
            return Err(Error::Malformed("NV allocation"));
        }
        Ok(NvBufferView {
            allocation: 7,
            generation: self.generation.get(),
            guest_address: PIXELS,
            size: 4096,
            alignment: 4096,
            kind: 0,
        })
    }
}
struct Fixture {
    media: Media,
    kernel: Kernel,
    memory: MemoryImage,
    nv: Nv,
    now: u64,
    client: ClientId,
}
impl Fixture {
    fn new() -> Self {
        let boot = Homebrew {
            regions: vec![Region {
                address: BASE,
                bytes: vec![0; PAGE * 8],
                permissions: 3,
            }],
            pc: BASE,
            sp: 0,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: TLS,
        };
        Self {
            media: Media::default(),
            kernel: Kernel::new(&boot),
            memory: MemoryImage::new(&boot).unwrap(),
            nv: Nv {
                generation: Cell::new(1),
                alive: Cell::new(true),
            },
            now: 0,
            client: ClientId(1),
        }
    }
    fn open(&mut self, name: &str) -> Handle {
        self.media
            .open(
                name,
                &mut Context {
                    kernel: &mut self.kernel,
                    memory: &mut self.memory,
                    object: 0,
                    session: 0,
                    client: self.client,
                    now_ns: self.now,
                    tls: TLS,
                    nv: Some(&self.nv),
                },
            )
            .unwrap()
    }
    fn call(&mut self, handle: Handle, request: &Request) -> Result<Reply, Error> {
        let object = self.kernel.object_id(handle).unwrap();
        self.media.dispatch(
            request,
            &mut Context {
                kernel: &mut self.kernel,
                memory: &mut self.memory,
                object,
                session: handle,
                client: self.client,
                now_ns: self.now,
                tls: TLS,
                nv: Some(&self.nv),
            },
        )
    }
    fn poll(&mut self, now: u64) {
        self.now = now;
        self.media
            .poll(&mut self.kernel, &mut self.memory, now)
            .unwrap();
    }
    fn binder(&mut self) -> Handle {
        let vi = self.open("vi:m");
        let display = self.call(vi, &req(2, words(&[0]))).unwrap().objects[0];
        self.call(display, &req(100, vec![])).unwrap().objects[0]
    }
    fn transaction(&mut self, binder: Handle, id: u32, body: &[u8]) -> Result<Reply, Error> {
        let raw = parcel_input(body);
        self.memory.write(INPUT, &raw).unwrap();
        let mut request = req(3, words(&[0, id, 0]));
        request.buffers = vec![
            Buffer {
                kind: BufferKind::Pointer,
                address: INPUT,
                len: raw.len() as u64,
                index: 0,
            },
            Buffer {
                kind: BufferKind::ReceiveList,
                address: OUTPUT_ADDRESS,
                len: 1024,
                index: 1,
            },
        ];
        self.call(binder, &request)
    }
    fn preallocate(&mut self, binder: Handle, slot: u32) -> Result<Reply, Error> {
        let mut body = vec![0; 148];
        put(&mut body, 0, slot);
        body[16..20].copy_from_slice(b"RFBG");
        for (offset, value) in [(32, 1), (60, 7), (108, 16), (112, 2), (128, 64)] {
            put(&mut body, offset, value);
        }
        self.transaction(binder, 14, &body)
    }
    fn audio(&mut self) -> Handle {
        let service = self.open("audout:u");
        let mut request = req(1, words(&[48000, 2]));
        request.buffers = io_buffers(256);
        self.call(service, &request).unwrap().objects[0]
    }
    fn append(&mut self, output: Handle, tag: u64, len: u64) -> Result<Reply, Error> {
        let bytes: Vec<u8> = [0, PIXELS, len, len, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        self.memory.write(INPUT, &bytes).unwrap();
        let mut request = req(3, tag.to_le_bytes().to_vec());
        request.buffers = io_buffers(40);
        self.call(output, &request)
    }
    fn reset_signal(&mut self, handle: Handle) -> u64 {
        let mut registers = [0; 31];
        registers[0] = u64::from(handle);
        match self.kernel.dispatch(
            &mut self.memory,
            MAIN_THREAD,
            23,
            &registers,
            BASE,
            self.now,
        ) {
            Dispatch::Return(values) => values[0],
            other => panic!("unexpected {other:?}"),
        }
    }
}
fn req(command_id: u32, data: Vec<u8>) -> Request {
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
fn io_buffers(len: u64) -> Vec<Buffer> {
    vec![
        Buffer {
            kind: BufferKind::Send,
            address: INPUT,
            len,
            index: 0,
        },
        Buffer {
            kind: BufferKind::Receive,
            address: OUTPUT_ADDRESS,
            len: 256,
            index: 1,
        },
    ]
}
fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn parcel_input(body: &[u8]) -> Vec<u8> {
    let name = "android.gui.IGraphicBufferProducer";
    let mut payload = words(&[0x100, name.len() as u32]);
    payload.extend(name.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
    payload.resize(payload.len().next_multiple_of(4), 0);
    payload.extend_from_slice(body);
    let mut raw = words(&[payload.len() as u32, 16, 0, 16 + payload.len() as u32]);
    raw.extend(payload);
    raw
}
#[test]
fn binder_exact_parcels_alternate_slots_and_produce_rgba() {
    let mut f = Fixture::new();
    let binder = f.binder();
    assert_eq!(
        f.transaction(binder, 10, &words(&[0, 2, 0]))
            .unwrap()
            .writes[0]
            .bytes,
        words(&[20, 16, 0, 36, 1280, 720, 0, 0, 0])
    );
    assert_eq!(
        f.preallocate(binder, 0).unwrap().writes[0].bytes,
        words(&[0, 16, 0, 16])
    );
    f.preallocate(binder, 1).unwrap();
    let raw: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
    f.memory.write(PIXELS, &raw).unwrap();
    for slot in [0, 1, 0, 1] {
        let dequeue = f.transaction(binder, 3, &words(&[0, 16, 2, 1, 0])).unwrap();
        let mut expected = words(&[56, 16, 0, 72, slot, 1, 36, 0]);
        expected.extend([0; 40]);
        assert_eq!(dequeue.writes[0].bytes, expected);
        assert!(f.preallocate(binder, slot).is_err());
        assert_eq!(
            f.transaction(binder, 1, &words(&[slot])).unwrap().writes[0].bytes,
            words(&[8, 16, 0, 24, 0, 0])
        );
        assert_eq!(
            f.transaction(binder, 7, &words(&[slot])).unwrap().writes[0].bytes,
            words(&[20, 16, 0, 36, 16, 2, 0, 0, 0])
        );
        let frame = f.media.take_frame().unwrap();
        let expected: Vec<u8> = [0, 32, 256, 288, 16, 48, 272, 304]
            .iter()
            .flat_map(|offset| raw[*offset..*offset + 16].iter().copied())
            .collect();
        assert_eq!((frame.width, frame.height, frame.pixels), (16, 2, expected));
        assert!(f.transaction(binder, 7, &words(&[slot])).is_err());
    }
}
#[test]
fn queue_rejects_freed_replaced_or_wrong_client_allocation_without_releasing_slot() {
    for change in 0..3 {
        let mut f = Fixture::new();
        let binder = f.binder();
        f.preallocate(binder, 0).unwrap();
        f.transaction(binder, 3, &words(&[0])).unwrap();
        match change {
            0 => f.nv.alive.set(false),
            1 => f.nv.generation.set(2),
            _ => f.client = ClientId(2),
        }
        assert!(f.transaction(binder, 7, &words(&[0])).is_err());
        assert!(f.media.take_frame().is_none());
        f.nv.alive.set(true);
        f.nv.generation.set(1);
        f.client = ClientId(1);
        f.transaction(binder, 7, &words(&[0])).unwrap();
    }
}
#[test]
fn malformed_parcels_and_output_bounds_do_not_consume_slots() {
    let mut f = Fixture::new();
    let binder = f.binder();
    f.preallocate(binder, 0).unwrap();
    let mut raw = parcel_input(&words(&[0]));
    put(&mut raw, 0, 1024);
    f.memory.write(INPUT, &raw).unwrap();
    let mut request = req(3, words(&[0, 3, 0]));
    request.buffers = io_buffers(raw.len() as u64);
    assert!(f.call(binder, &request).is_err());
    let payload_len = (raw.len() - 16) as u32;
    put(&mut raw, 0, payload_len);
    f.memory.write(INPUT, &raw).unwrap();
    request.buffers[1].len = 16;
    assert!(f.call(binder, &request).is_err());
    assert_eq!(
        read_u32(
            &f.transaction(binder, 3, &words(&[0])).unwrap().writes[0].bytes,
            16
        )
        .unwrap(),
        0
    );
}
#[test]
fn framebuffer_rejects_out_of_allocation_and_swizzles_bgra() {
    let mut f = Fixture::new();
    let allocation = f.nv.resolve(ClientId(1), 7).unwrap();
    let mut buffer = frame::GraphicBuffer {
        id: 7,
        allocation,
        width: 16,
        height: 2,
        pitch: 64,
        plane: 4090,
        block: 0,
        format: 1,
    };
    assert!(buffer.size().is_err());
    buffer.plane = 0;
    buffer.format = 5;
    f.memory.write(PIXELS, &[7, 11, 19, 255]).unwrap();
    let ctx = Context {
        kernel: &mut f.kernel,
        memory: &mut f.memory,
        object: 0,
        session: 0,
        client: ClientId(1),
        now_ns: 0,
        tls: TLS,
        nv: Some(&f.nv),
    };
    assert_eq!(&buffer.pixels(&ctx).unwrap().pixels[..4], &[19, 11, 7, 255]);
    buffer.block = 6;
    assert!(buffer.size().is_err());
}
#[test]
fn display_vsync_and_binder_availability_are_real_shared_events() {
    let mut f = Fixture::new();
    let vi = f.open("vi:m");
    let display = f.call(vi, &req(2, words(&[0]))).unwrap().objects[0];
    let event = f
        .call(display, &req(5202, 1u64.to_le_bytes().to_vec()))
        .unwrap()
        .copy_handles[0];
    let alias = f.kernel.duplicate_handle(event).unwrap();
    f.poll(16_666_666);
    assert_eq!(f.reset_signal(alias), INVALID_STATE);
    f.poll(16_666_667);
    assert_eq!(f.reset_signal(alias), 0);
    let binder = f.call(display, &req(100, vec![])).unwrap().objects[0];
    f.preallocate(binder, 0).unwrap();
    let event = f
        .call(binder, &req(2, words(&[0, 0])))
        .unwrap()
        .copy_handles[0];
    let alias = f.kernel.duplicate_handle(event).unwrap();
    assert_eq!(f.reset_signal(alias), 0);
    f.transaction(binder, 3, &words(&[0])).unwrap();
    assert_eq!(f.reset_signal(alias), INVALID_STATE);
    f.transaction(binder, 7, &words(&[0])).unwrap();
    assert_eq!(f.reset_signal(alias), 0);
}

#[path = "audio_tests.rs"]
mod audio_tests;

#[test]
fn vi_layer_parcel_and_close_order_preserve_display_lifecycle() {
    let mut f = Fixture::new();
    let vi = f.open("vi:m");
    let display = f.call(vi, &req(2, words(&[0]))).unwrap().objects[0];
    let mut name = vec![0; 64];
    name[..7].copy_from_slice(b"Default");
    assert_eq!(
        f.call(display, &req(1010, name.clone())).unwrap().data,
        1u64.to_le_bytes()
    );
    name.resize(80, 0);
    let mut layer = req(2020, name);
    layer.buffers = io_buffers(256);
    let reply = f.call(display, &layer).unwrap();
    assert_eq!(reply.data, 28u64.to_le_bytes());
    assert_eq!(reply.writes[0].bytes, words(&[12, 16, 0, 28, 0, 0, 1]));
    assert!(
        f.call(display, &req(1020, 1u64.to_le_bytes().to_vec()))
            .is_err()
    );
    f.call(display, &req(2021, 1u64.to_le_bytes().to_vec()))
        .unwrap();
    f.call(display, &req(1020, 1u64.to_le_bytes().to_vec()))
        .unwrap();
    assert!(f.call(display, &layer).is_err());
}
