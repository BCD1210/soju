//! Bounded application alarm records. The desktop launch has no wake notification.
use super::{tables::c, *};

#[derive(Default)]
pub(super) struct Notifications {
    initialized: std::collections::BTreeSet<ObjectId>,
    alarms: BTreeMap<u16, (Vec<u8>, Vec<u8>)>,
}
impl Notifications {
    pub(super) fn close(&mut self, object: ObjectId) {
        self.initialized.remove(&object);
    }
}
pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    c(500, 0, command),
    c(510, 0, command),
    c(520, 0, command),
    c(530, 2, command),
    c(540, 2, command),
    c(1000, 8, command),
];
fn id(r: &Request) -> u16 {
    u16::from_le_bytes(r.data[..2].try_into().unwrap())
}
fn setting(bytes: &[u8]) -> bool {
    bytes.len() == 0x40
        && bytes[2] == 0
        && bytes[0x32..]
            .chunks_exact(2)
            .all(|v| v == [255, 255] || (v[0] < 60 && v[1] < 24))
}
fn command(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.command_id == 1000 {
        if r.pid.is_none() {
            return Err(Error::Malformed("alarm initialization requires PID"));
        }
        s.notifications.initialized.insert(x.object);
        return Ok(Reply::success());
    }
    if !s.notifications.initialized.contains(&x.object) {
        return Err(Error::Malformed("alarm service not initialized"));
    }
    match r.command_id {
        500 | 510 => {
            let descriptor = r.buffer(BufferKind::Pointer, 0)?;
            let mut record = x.read_buffer(descriptor, 0x40)?;
            if !setting(&record) {
                return Err(Error::Malformed("alarm setting size, kind or schedule"));
            }
            let parameter = x.read_buffer(r.buffer(BufferKind::Send, 0)?, 0x400)?;
            let alarms = &mut s.notifications.alarms;
            let id = if r.command_id == 500 {
                if alarms.len() >= 8 {
                    return Err(Error::Malformed("application alarm limit reached"));
                }
                (1..=u16::MAX).find(|id| !alarms.contains_key(id)).unwrap()
            } else {
                let id = u16::from_le_bytes(record[..2].try_into().unwrap());
                if !alarms.contains_key(&id) {
                    return Err(Error::Malformed("unknown alarm setting"));
                }
                id
            };
            record[..2].copy_from_slice(&id.to_le_bytes());
            record[0x18..0x20].copy_from_slice(&s.program_id.to_le_bytes());
            alarms.insert(id, (record, parameter));
            Ok(if r.command_id == 500 {
                Reply::data(id.to_le_bytes().to_vec())
            } else {
                Reply::success()
            })
        }
        520 => {
            let out = r.buffer(BufferKind::Receive, 0)?;
            if out.len > MAX_BUFFER as u64 || !out.len.is_multiple_of(0x40) {
                return Err(Error::Malformed("alarm listing array size"));
            }
            x.memory.check_access(out.address, out.len as usize, 2)?;
            let count = s.notifications.alarms.len().min(out.len as usize / 0x40);
            let bytes = s
                .notifications
                .alarms
                .values()
                .take(count)
                .flat_map(|(record, _)| record.iter().copied())
                .collect();
            Reply::data((count as u32).to_le_bytes().to_vec()).write(out, bytes)
        }
        530 => {
            let out = r.buffer(BufferKind::Receive, 0)?;
            let (_, parameter) = s
                .notifications
                .alarms
                .get(&id(r))
                .ok_or(Error::Malformed("unknown alarm parameter"))?;
            if out.len < parameter.len() as u64 {
                return Err(Error::Malformed("short alarm parameter buffer"));
            }
            Reply::data((parameter.len() as u32).to_le_bytes().to_vec())
                .write(out, parameter.clone())
        }
        540 => {
            if s.notifications.alarms.remove(&id(r)).is_none() {
                return Err(Error::Malformed("unknown alarm setting"));
            }
            Ok(Reply::success())
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };
    #[test]
    fn alarm_crud_preserves_parameters_and_validates_schedule_and_capacity() {
        let mut h = Harness::new();
        let session = h.open("notif:a");
        let mut init = req(1000, vec![0; 8]);
        init.pid = Some(1);
        h.call(session, init).unwrap();
        let mut record = vec![0; 64];
        record[0x32..].fill(255);
        record[0x32] = 30;
        record[0x33] = 12;
        h.m.write(TLS, &record).unwrap();
        h.m.write(TLS + 64, b"parameter").unwrap();
        let mut request = req(500, vec![]);
        request.buffers = vec![
            Buffer {
                kind: BufferKind::Pointer,
                address: TLS,
                len: 64,
                index: 0,
            },
            Buffer {
                kind: BufferKind::Send,
                address: TLS + 64,
                len: 9,
                index: 0,
            },
        ];
        for id in 1u16..=8 {
            assert_eq!(
                h.call(session, request.clone()).unwrap().data,
                id.to_le_bytes()
            );
        }
        assert!(h.call(session, request.clone()).is_err());
        let mut load = req(530, 1u16.to_le_bytes().to_vec());
        load.buffers.push(Buffer {
            kind: BufferKind::Receive,
            address: TLS + 128,
            len: 16,
            index: 0,
        });
        assert_eq!(h.call(session, load).unwrap().writes[0].bytes, b"parameter");
        h.call(session, req(540, 1u16.to_le_bytes().to_vec()))
            .unwrap();
        record[0x33] = 24;
        h.m.write(TLS, &record).unwrap();
        assert!(h.call(session, request.clone()).is_err());
        record[0x33] = 12;
        h.m.write(TLS, &record).unwrap();
        assert_eq!(h.call(session, request).unwrap().data, 1u16.to_le_bytes());
    }
}
