//! Checked wire layouts from Switchbrew HIPC and libnx sf/{hipc,cmif,tipc}.h.
use super::types::{
    Buffer, BufferKind, Error, MAX_BUFFER, MessageKind, Protocol, Reply, Request, read_u32,
    read_u64,
};

const TLS_SIZE: usize = 256;
const ADDRESS_LIMIT: u64 = 1 << 39;
const SFCI: u32 = 0x4943_4653;
const SFCO: u32 = 0x4f43_4653;

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl Cursor<'_> {
    fn take(&mut self, size: usize) -> Result<&[u8], Error> {
        let end = self
            .offset
            .checked_add(size)
            .ok_or(Error::Malformed("IPC offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::Malformed("truncated IPC"))?;
        self.offset = end;
        Ok(bytes)
    }
    fn word(&mut self) -> Result<u32, Error> {
        read_u32(self.take(4)?, 0)
    }
    fn handles(&mut self, count: u32) -> Result<Vec<u32>, Error> {
        (0..count).map(|_| self.word()).collect()
    }
}

fn buffer(
    buffers: &mut Vec<Buffer>,
    kind: BufferKind,
    address: u64,
    len: u64,
) -> Result<(), Error> {
    if len > MAX_BUFFER as u64
        || address >= ADDRESS_LIMIT
        || address
            .checked_add(len)
            .is_none_or(|end| end > ADDRESS_LIMIT)
    {
        return Err(Error::Malformed("IPC buffer outside guest bounds"));
    }
    buffers.push(Buffer {
        kind,
        address,
        len,
        index: u16::try_from(buffers.len()).map_err(|_| Error::Malformed("buffer index"))?,
    });
    Ok(())
}

/// Decode one bounded TLS command. Non-domain CMIF does not encode a byte count
/// for its parameters; its `data` consequently includes trailing raw padding.
pub fn decode(bytes: &[u8], domain: bool) -> Result<Request, Error> {
    if bytes.len() > TLS_SIZE {
        return Err(Error::Malformed("IPC exceeds TLS command area"));
    }
    let mut cursor = Cursor { bytes, offset: 0 };
    let header0 = cursor.word()?;
    let header1 = cursor.word()?;
    if header1 & 0x000f_c000 != 0 {
        return Err(Error::Malformed("HIPC reserved bits"));
    }
    let message_type = header0 & 0xffff;
    let (protocol, kind, command_id) = match message_type {
        2 => (Protocol::Cmif, MessageKind::Close, 0),
        4 | 6 => (Protocol::Cmif, MessageKind::Request, 0),
        5 | 7 => (Protocol::Cmif, MessageKind::Control, 0),
        15 => (Protocol::Tipc, MessageKind::Close, 0),
        16..=65535 => (Protocol::Tipc, MessageKind::Request, message_type - 16),
        _ => return Err(Error::Malformed("unsupported HIPC message type")),
    };
    if domain && protocol == Protocol::Tipc {
        return Err(Error::Malformed("TIPC domain"));
    }
    let mut request = Request {
        protocol,
        kind,
        command_id,
        pid: None,
        buffers: Vec::new(),
        data: Vec::new(),
        copy_handles: Vec::new(),
        move_handles: Vec::new(),
        domain_objects: Vec::new(),
        objects: Vec::new(),
        domain_target: None,
        token: 0,
    };
    if header1 >> 31 != 0 {
        let special = cursor.word()?;
        if special & !0x1ff != 0 {
            return Err(Error::Malformed("HIPC special reserved bits"));
        }
        if special & 1 != 0 {
            request.pid = Some(read_u64(cursor.take(8)?, 0)?);
        }
        request.copy_handles = cursor.handles((special >> 1) & 15)?;
        request.move_handles = cursor.handles((special >> 5) & 15)?;
    }
    for _ in 0..((header0 >> 16) & 15) {
        let descriptor = cursor.take(8)?;
        let upper = read_u32(descriptor, 0)?;
        let address = u64::from(read_u32(descriptor, 4)?)
            | (u64::from((upper >> 12) & 15) << 32)
            | (u64::from((upper >> 6) & 63) << 36);
        buffer(
            &mut request.buffers,
            BufferKind::Pointer,
            address,
            u64::from(upper >> 16),
        )?;
    }
    for (shift, kind) in [
        (20, BufferKind::Send),
        (24, BufferKind::Receive),
        (28, BufferKind::Exchange),
    ] {
        for _ in 0..((header0 >> shift) & 15) {
            let descriptor = cursor.take(12)?;
            let upper = read_u32(descriptor, 8)?;
            if upper & 3 == 2 {
                return Err(Error::Malformed("invalid map buffer mode"));
            }
            let address = u64::from(read_u32(descriptor, 4)?)
                | (u64::from(upper >> 28) << 32)
                | (u64::from((upper >> 2) & 0x3f_ffff) << 36);
            let len = u64::from(read_u32(descriptor, 0)?) | (u64::from((upper >> 24) & 15) << 32);
            buffer(&mut request.buffers, kind, address, len)?;
        }
    }
    let raw_start = cursor.offset;
    let raw_len = (header1 & 0x3ff) as usize * 4;
    cursor.take(raw_len)?;
    let raw_end = cursor.offset;
    let recv_mode = (header1 >> 10) & 15;
    let recv_count = match recv_mode {
        0 => 0,
        1 => return Err(Error::Malformed("inline receive list unsupported")),
        2 => 1,
        n => n - 2,
    };
    // ReceiveListOffset is unused by libnx; accept only its canonical location.
    let recv_offset = ((header1 >> 20) & 0x7ff) as usize * 4;
    if recv_offset != 0 && recv_offset != raw_end {
        return Err(Error::Malformed("receive list offset"));
    }
    for _ in 0..recv_count {
        let descriptor = cursor.take(8)?;
        let upper = read_u32(descriptor, 4)?;
        let address = u64::from(read_u32(descriptor, 0)?) | (u64::from(upper & 0xffff) << 32);
        buffer(
            &mut request.buffers,
            BufferKind::ReceiveList,
            address,
            u64::from(upper >> 16),
        )?;
    }
    if kind == MessageKind::Close {
        if raw_len != 0
            || !request.buffers.is_empty()
            || request.pid.is_some()
            || !request.copy_handles.is_empty()
            || !request.move_handles.is_empty()
        {
            return Err(Error::Malformed("close message payload"));
        }
        return Ok(request);
    }
    if protocol == Protocol::Tipc {
        request.data = bytes[raw_start..raw_end].to_vec();
        return Ok(request);
    }
    let aligned = (raw_start + 15) & !15;
    let mut payload = Cursor {
        bytes: bytes
            .get(aligned..raw_end)
            .ok_or(Error::Malformed("CMIF alignment"))?,
        offset: 0,
    };
    let domain_message = domain && kind == MessageKind::Request;
    let cmif: &[u8];
    if domain_message {
        let header = payload.take(16)?;
        let domain_kind = header[0];
        let object_count = usize::from(header[1]);
        let data_len = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let object_id = read_u32(header, 4)?;
        if object_id == 0 || read_u32(header, 8)? != 0 {
            return Err(Error::Malformed("domain header"));
        }
        request.domain_target = Some(object_id);
        request.token = read_u32(header, 12)?;
        match domain_kind {
            2 if data_len == 0 && object_count == 0 => {
                request.kind = MessageKind::DomainClose;
                return Ok(request);
            }
            1 => {}
            _ => return Err(Error::Malformed("domain command")),
        }
        let cmif_start = payload.offset;
        payload.take(data_len)?;
        let cmif_end = payload.offset;
        request.domain_objects = payload.handles(object_count as u32)?;
        if request.domain_objects.contains(&0) {
            return Err(Error::Malformed("null domain input object"));
        }
        cmif = &payload.bytes[cmif_start..cmif_end];
    } else {
        cmif = payload.bytes;
    }
    if read_u32(cmif, 0)? != SFCI || read_u32(cmif, 4)? > 1 {
        return Err(Error::Malformed("CMIF request header"));
    }
    request.command_id = read_u32(cmif, 8)?;
    let token = read_u32(cmif, 12)?;
    if !domain_message {
        request.token = token;
    }
    request.data = cmif
        .get(16..)
        .ok_or(Error::Malformed("CMIF payload"))?
        .to_vec();
    Ok(request)
}

fn push_word(bytes: &mut Vec<u8>, word: u32) {
    bytes.extend_from_slice(&word.to_le_bytes());
}

/// Encode finalized reply handles and domain IDs. Descriptor writes are performed
/// by the router, which also owns handle duplication and transactional rollback.
pub fn encode(
    request: &Request,
    reply: &Reply,
    domain: bool,
    domain_ids: &[u32],
) -> Result<Vec<u8>, Error> {
    if reply.copy_handles.len() > 15
        || reply.move_handles.len() > 15
        || reply.data.len() > TLS_SIZE
        || domain_ids.len() > TLS_SIZE / 4
    {
        return Err(Error::Malformed("reply size"));
    }
    let domain_message = domain
        && request.protocol == Protocol::Cmif
        && matches!(
            request.kind,
            MessageKind::Request | MessageKind::DomainClose
        );
    if !domain_message && !domain_ids.is_empty() {
        return Err(Error::Malformed("domain objects in session reply"));
    }
    if domain_ids.contains(&0) {
        return Err(Error::Malformed("null reply domain object"));
    }
    let special = !reply.copy_handles.is_empty() || !reply.move_handles.is_empty();
    let mut bytes = vec![0; 8];
    if special {
        push_word(
            &mut bytes,
            ((reply.copy_handles.len() as u32) << 1) | ((reply.move_handles.len() as u32) << 5),
        );
        for handle in reply.copy_handles.iter().chain(&reply.move_handles) {
            push_word(&mut bytes, *handle);
        }
    }
    let raw_start = bytes.len();
    if request.protocol == Protocol::Cmif {
        bytes.resize((raw_start + 15) & !15, 0);
        if domain_message {
            push_word(&mut bytes, domain_ids.len() as u32);
            bytes.resize(bytes.len() + 12, 0);
        }
        push_word(&mut bytes, SFCO);
        push_word(&mut bytes, u32::from(request.token != 0));
        push_word(&mut bytes, reply.result);
        push_word(&mut bytes, request.token);
    } else {
        push_word(&mut bytes, reply.result);
    }
    bytes.extend_from_slice(&reply.data);
    for id in domain_ids {
        push_word(&mut bytes, *id);
    }
    if request.protocol == Protocol::Cmif {
        // CMIF reserves sixteen total padding bytes, split around the payload.
        let front_padding = ((raw_start + 15) & !15) - raw_start;
        bytes.resize(bytes.len() + 16 - front_padding, 0);
    }
    bytes.resize((bytes.len() + 3) & !3, 0);
    if bytes.len() > TLS_SIZE {
        return Err(Error::Malformed("reply exceeds TLS command area"));
    }
    let header1 = ((bytes.len() - raw_start) / 4) as u32 | if special { 1 << 31 } else { 0 };
    bytes[4..8].copy_from_slice(&header1.to_le_bytes());
    bytes.resize(TLS_SIZE, 0);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn request(message_type: u32, domain: bool, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 16];
        word(&mut bytes, 0, message_type);
        if domain {
            push_word(&mut bytes, 1 | (((16 + data.len()) as u32) << 16));
            push_word(&mut bytes, 7);
            push_word(&mut bytes, 0);
            push_word(&mut bytes, 99);
        }
        push_word(&mut bytes, SFCI);
        push_word(&mut bytes, 1);
        push_word(&mut bytes, 42);
        push_word(&mut bytes, if domain { 0 } else { 99 });
        bytes.extend_from_slice(data);
        bytes.resize((bytes.len() + 3) & !3, 0);
        let words = ((bytes.len() - 8) / 4) as u32;
        word(&mut bytes, 4, words);
        bytes
    }

    #[test]
    fn cmif_and_control_decode_with_context() {
        for message_type in [4, 6, 5, 7] {
            let bytes = request(message_type, false, &[1, 2, 3, 4]);
            let decoded = decode(&bytes, message_type == 5 || message_type == 7).unwrap();
            assert_eq!(decoded.protocol, Protocol::Cmif);
            assert_eq!(decoded.command_id, 42);
            assert_eq!(decoded.token, 99);
            assert_eq!(decoded.data, [1, 2, 3, 4]);
            assert_eq!(
                decoded.kind,
                if message_type == 4 || message_type == 6 {
                    MessageKind::Request
                } else {
                    MessageKind::Control
                }
            );
        }
    }

    #[test]
    fn truncation_and_declared_size_rejected() {
        let bytes = request(4, false, &[1, 2, 3, 4]);
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len], false).is_err(), "length {len}");
        }
        let mut oversized = bytes.clone();
        word(&mut oversized, 4, 0x3ff);
        assert!(decode(&oversized, false).is_err());
        assert!(decode(&vec![0; TLS_SIZE + 1], false).is_err());
        let mut magic = bytes;
        word(&mut magic, 16, SFCO);
        assert!(decode(&magic, false).is_err());
    }

    #[test]
    fn tipc_pid_handles_and_unaligned_raw() {
        let mut bytes = vec![0; 8];
        word(&mut bytes, 0, 19);
        word(&mut bytes, 4, (1 << 31) | 1);
        push_word(&mut bytes, 1 | (1 << 1) | (1 << 5));
        bytes.extend_from_slice(&0x1234_5678_9abc_def0_u64.to_le_bytes());
        push_word(&mut bytes, 123);
        push_word(&mut bytes, 456);
        push_word(&mut bytes, 0xaabb_ccdd);
        let decoded = decode(&bytes, false).unwrap();
        assert_eq!(decoded.protocol, Protocol::Tipc);
        assert_eq!(decoded.command_id, 3);
        assert_eq!(decoded.pid, Some(0x1234_5678_9abc_def0));
        assert_eq!(decoded.copy_handles, [123]);
        assert_eq!(decoded.move_handles, [456]);
        assert_eq!(decoded.u32(0).unwrap(), 0xaabb_ccdd);
        assert!(decode(&bytes, true).is_err());
    }

    #[test]
    fn all_descriptors_have_distinct_indices_and_39_bit_addresses() {
        let address = 0x71_2345_6000_u64;
        let mut bytes = vec![0; 8];
        word(
            &mut bytes,
            0,
            16 | (1 << 16) | (1 << 20) | (1 << 24) | (1 << 28),
        );
        word(&mut bytes, 4, 3 << 10);
        let high = ((address >> 36) as u32) << 6 | (((address >> 32) as u32) & 15) << 12;
        push_word(&mut bytes, high | (8 << 16));
        push_word(&mut bytes, address as u32);
        for mode in [0, 1, 3] {
            push_word(&mut bytes, 16);
            push_word(&mut bytes, address as u32);
            push_word(
                &mut bytes,
                mode | (((address >> 36) as u32) << 2) | ((((address >> 32) as u32) & 15) << 28),
            );
        }
        push_word(&mut bytes, address as u32);
        push_word(&mut bytes, ((address >> 32) as u32) | (32 << 16));
        let decoded = decode(&bytes, false).unwrap();
        assert_eq!(decoded.buffers.len(), 5);
        for (index, descriptor) in decoded.buffers.iter().enumerate() {
            assert_eq!(usize::from(descriptor.index), index);
            assert_eq!(descriptor.address, address);
        }
        assert_eq!(
            decoded.buffers.iter().map(|b| b.kind).collect::<Vec<_>>(),
            [
                BufferKind::Pointer,
                BufferKind::Send,
                BufferKind::Receive,
                BufferKind::Exchange,
                BufferKind::ReceiveList
            ]
        );
        assert_eq!(
            decoded.buffers.iter().map(|b| b.len).collect::<Vec<_>>(),
            [8, 16, 16, 16, 32]
        );
        // A descriptor crossing the 39-bit address ceiling must not wrap.
        word(&mut bytes, 20, 0xffff_ffff);
        word(&mut bytes, 24, (7 << 2) | (15 << 28));
        assert!(decode(&bytes, false).is_err());
        word(&mut bytes, 20, address as u32);
        word(&mut bytes, 24, 8 << 2);
        assert!(decode(&bytes, false).is_err());
    }

    #[test]
    fn receive_list_modes_and_descriptor_limits() {
        let mut bytes = vec![0; 16];
        word(&mut bytes, 0, 16);
        word(&mut bytes, 4, 2 << 10);
        word(&mut bytes, 8, 0x1000);
        word(&mut bytes, 12, 32 << 16);
        assert_eq!(decode(&bytes, false).unwrap().buffers[0].len, 32);
        word(&mut bytes, 4, 1 << 10);
        assert!(matches!(
            decode(&bytes, false),
            Err(Error::Malformed("inline receive list unsupported"))
        ));
        word(&mut bytes, 0, 16 | (1 << 20));
        word(&mut bytes, 4, 0);
        bytes.resize(20, 0);
        word(&mut bytes, 8, MAX_BUFFER as u32 + 1);
        assert!(decode(&bytes, false).is_err());
        word(&mut bytes, 8, 4);
        word(&mut bytes, 16, 2);
        assert!(decode(&bytes, false).is_err());
    }

    #[test]
    fn domain_payload_objects_and_close() {
        let mut bytes = request(4, true, &[1, 2, 3]);
        // Object IDs immediately follow exact data_size, even before word padding.
        bytes.truncate(51);
        bytes[17] = 2;
        push_word(&mut bytes, 12);
        push_word(&mut bytes, 13);
        bytes.resize(60, 0);
        word(&mut bytes, 4, 13);
        let decoded = decode(&bytes, true).unwrap();
        assert_eq!(decoded.domain_target, Some(7));
        assert_eq!(decoded.domain_objects, [12, 13]);
        assert_eq!(decoded.data, [1, 2, 3]);
        assert_eq!(decoded.token, 99);
        bytes[17] = 3;
        assert!(decode(&bytes, true).is_err());
        let mut close = vec![0; 32];
        word(&mut close, 0, 4);
        word(&mut close, 4, 6);
        word(&mut close, 16, 2);
        word(&mut close, 20, 7);
        assert_eq!(decode(&close, true).unwrap().kind, MessageKind::DomainClose);
    }

    #[test]
    fn closes_are_explicit_and_reject_payloads() {
        for message_type in [2, 15] {
            let mut bytes = vec![0; 8];
            word(&mut bytes, 0, message_type);
            assert_eq!(decode(&bytes, false).unwrap().kind, MessageKind::Close);
            bytes.resize(12, 0);
            word(&mut bytes, 4, 1);
            assert!(decode(&bytes, false).is_err());
        }
    }

    #[test]
    fn response_cmif_handles_padding_and_domain_objects() {
        let req = decode(&request(4, false, &[]), false).unwrap();
        let reply = Reply {
            result: 0x123,
            data: vec![1, 2, 3],
            copy_handles: vec![11],
            move_handles: vec![12, 13],
            ..Reply::default()
        };
        let bytes = encode(&req, &reply, false, &[]).unwrap();
        assert_eq!(bytes.len(), TLS_SIZE);
        assert_eq!(read_u32(&bytes, 4).unwrap(), (1 << 31) | 9);
        assert_eq!(read_u32(&bytes, 8).unwrap(), (1 << 1) | (2 << 5));
        assert_eq!(read_u32(&bytes, 12).unwrap(), 11);
        assert_eq!(read_u32(&bytes, 16).unwrap(), 12);
        assert_eq!(read_u32(&bytes, 20).unwrap(), 13);
        assert_eq!(read_u32(&bytes, 32).unwrap(), SFCO);
        assert_eq!(read_u32(&bytes, 40).unwrap(), 0x123);
        assert_eq!(read_u32(&bytes, 44).unwrap(), 99);
        assert_eq!(&bytes[48..51], &[1, 2, 3]);
        assert!(bytes[51..].iter().all(|byte| *byte == 0));
        let domain = encode(&req, &Reply::data(vec![9, 8, 7]), true, &[5, 6]).unwrap();
        assert_eq!(read_u32(&domain, 16).unwrap(), 2);
        assert_eq!(read_u32(&domain, 32).unwrap(), SFCO);
        assert_eq!(read_u32(&domain, 51).unwrap(), 5);
        assert_eq!(read_u32(&domain, 55).unwrap(), 6);
        assert!(encode(&req, &reply, false, &[1]).is_err());
    }

    #[test]
    fn tipc_response_is_not_cmif_aligned_and_sizes_are_bounded() {
        let mut bytes = vec![0; 8];
        word(&mut bytes, 0, 17);
        let req = decode(&bytes, false).unwrap();
        let reply = Reply {
            result: 0x456,
            data: vec![7, 8, 9, 10],
            ..Reply::default()
        };
        let encoded = encode(&req, &reply, false, &[]).unwrap();
        assert_eq!(read_u32(&encoded, 4).unwrap(), 2);
        assert_eq!(read_u32(&encoded, 8).unwrap(), 0x456);
        assert_eq!(&encoded[12..16], &[7, 8, 9, 10]);
        assert!(encode(&req, &Reply::data(vec![0; TLS_SIZE]), false, &[]).is_err());
        let too_many = Reply {
            copy_handles: vec![1; 16],
            ..Reply::default()
        };
        assert!(encode(&req, &too_many, false, &[]).is_err());
    }
}
