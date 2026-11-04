//! Empty USB host bus: session binding, enumeration and unsignaled hotplug events.
use super::{tables::c, *};

const INVALID_ARGUMENT: u32 = 0xc88c;
const INVALID_STATE: u32 = 0x108c;
#[derive(Default)]
pub(super) struct Session {
    available: [Option<([u8; 16], Handle)>; 3],
    changed: Option<Handle>,
}

pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    c(0, 0, bind),
    c(1, 16, enumerate),
    c(2, 16, enumerate),
    c(3, 0, enumerate),
    c(4, 18, create_event),
    c(5, 1, destroy_event),
    c(6, 0, changed_event),
    c(7, 4, acquire),
];

fn bind(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.copy_handles != [0xffff8001] {
        return Ok(Reply::result(INVALID_ARGUMENT));
    }
    if s.usb.contains_key(&x.object) {
        return Ok(Reply::result(INVALID_STATE));
    }
    s.usb.insert(x.object, Session::default());
    Ok(Reply::success())
}

fn filter(bytes: &[u8], unique: bool) -> bool {
    let flags = u16::from_le_bytes([bytes[0], bytes[1]]);
    flags & !0x3ff == 0 && (!unique || flags != 0)
}

fn enumerate(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if !s.usb.contains_key(&x.object) {
        return Ok(Reply::result(INVALID_STATE));
    }
    if r.command_id != 3 && !filter(&r.data[..16], false) {
        return Ok(Reply::result(INVALID_ARGUMENT));
    }
    let out = r.buffer(BufferKind::Receive, 0)?;
    if out.len > MAX_BUFFER as u64 || !out.len.is_multiple_of(0x228) {
        return Ok(Reply::result(0xcc8c));
    }
    x.memory.check_access(out.address, out.len as usize, 2)?;
    Ok(Reply::data(0u32.to_le_bytes().to_vec()))
}

fn create_event(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let index = usize::from(r.data[0]);
    let wanted: [u8; 16] = r.data[2..18].try_into().unwrap();
    if index >= 3 || !filter(&wanted, true) {
        return Ok(Reply::result(INVALID_ARGUMENT));
    }
    let Some(session) = s.usb.get(&x.object) else {
        return Ok(Reply::result(INVALID_STATE));
    };
    if session.available[index].is_some()
        || s.usb
            .values()
            .any(|v| v.available.iter().flatten().any(|(f, _)| *f == wanted))
    {
        return Ok(Reply::result(INVALID_STATE));
    }
    let reply = s.event(r, x)?;
    s.usb.get_mut(&x.object).unwrap().available[index] = Some((wanted, reply.copy_handles[0]));
    Ok(reply)
}

fn destroy_event(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let index = usize::from(r.data[0]);
    if index >= 3 {
        return Ok(Reply::result(INVALID_ARGUMENT));
    }
    let Some(session) = s.usb.get_mut(&x.object) else {
        return Ok(Reply::result(INVALID_STATE));
    };
    if let Some((_, event)) = session.available[index].take() {
        if let Some(events) = s.owned_events.get_mut(&x.object) {
            events.retain(|h| *h != event);
        }
        s.retired_events.push(event);
    }
    Ok(Reply::success())
}

fn changed_event(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let Some(session) = s.usb.get(&x.object) else {
        return Ok(Reply::result(INVALID_STATE));
    };
    if let Some(event) = session.changed {
        return Ok(Reply::copy(event));
    }
    let reply = s.event(r, x)?;
    s.usb.get_mut(&x.object).unwrap().changed = Some(reply.copy_handles[0]);
    Ok(reply)
}

fn acquire(s: &mut Platform, _: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    // No interface IDs can be issued by this bus, so no interface can be acquired.
    Ok(Reply::result(if s.usb.contains_key(&x.object) {
        0x28c
    } else {
        INVALID_STATE
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };

    fn open(h: &mut Harness) -> Handle {
        let s = h.open("usb:hs");
        let mut r = req(0, vec![]);
        assert_eq!(h.call(s, r.clone()).unwrap().result, INVALID_ARGUMENT);
        r.copy_handles.push(0xffff8001);
        assert_eq!(h.call(s, r.clone()).unwrap().result, 0);
        assert_eq!(h.call(s, r).unwrap().result, INVALID_STATE);
        s
    }

    #[test]
    fn empty_bus_does_not_create_interfaces_or_write_output() {
        let mut h = Harness::new();
        let s = open(&mut h);
        for id in 1..=3 {
            let mut r = req(id, vec![0; 16]);
            r.buffers.push(Buffer {
                kind: BufferKind::Receive,
                address: TLS + 0x1000,
                len: 0x228,
                index: 0,
            });
            let reply = h.call(s, r.clone()).unwrap();
            assert_eq!(reply.data, [0; 4]);
            assert!(reply.writes.is_empty());
            r.buffers[0].len -= 1;
            assert_eq!(h.call(s, r).unwrap().result, 0xcc8c);
        }
        assert_eq!(h.call(s, req(7, vec![0; 4])).unwrap().result, 0x28c);
    }

    #[test]
    fn event_slots_are_unique_reusable_and_cleaned_up() {
        let mut h = Harness::new();
        let s = open(&mut h);
        let other = open(&mut h);
        let mut r = req(4, vec![0; 18]);
        r.data[2] = 1;
        let event = h.call(s, r.clone()).unwrap().copy_handles[0];
        let alias = h.k.duplicate_handle(event).unwrap();
        assert_eq!(h.call(other, r.clone()).unwrap().result, INVALID_STATE);
        assert_eq!(h.call(s, r.clone()).unwrap().result, INVALID_STATE);
        h.call(s, req(5, vec![0])).unwrap();
        h.p.poll(&mut h.k, &mut h.m, 0).unwrap();
        assert!(h.k.object_id(event).is_err());
        assert!(h.k.object_id(alias).is_ok());
        let event = h.call(other, r.clone()).unwrap().copy_handles[0];
        h.p.close_object(h.k.object_id(other).unwrap());
        h.p.poll(&mut h.k, &mut h.m, 0).unwrap();
        assert!(h.k.object_id(event).is_err());
        r.data[0] = 3;
        assert_eq!(h.call(s, r).unwrap().result, INVALID_ARGUMENT);
        let a = h.call(s, req(6, vec![])).unwrap().copy_handles[0];
        let b = h.call(s, req(6, vec![])).unwrap().copy_handles[0];
        assert_eq!(a, b);
    }
}
