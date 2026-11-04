//! Read-only HOS 17 query view of the runner's empty play-event journal.
//! No events are recorded by this runner; see SOURCES.md for the wire contract.
use super::{tables::c, *};

pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    c(0, 8, events),
    c(8, 4, events),
    c(9, 0, range),
    c(10, 4, events),
    c(12, 16, range),
    c(17, 1, applications),
    c(18, 1, applications),
    c(19, 24, applications),
];

fn flag(r: &Request) -> Result<(), Error> {
    if r.data[0] > 1 {
        return Err(Error::Malformed("play-history flag"));
    }
    Ok(())
}

fn output(r: &Request, x: &Context<'_>, stride: u64) -> Result<(), Error> {
    let b = r.buffer(BufferKind::Receive, 0)?;
    if b.len > MAX_BUFFER as u64 || !b.len.is_multiple_of(stride) {
        return Err(Error::Malformed("play-history output array"));
    }
    x.memory.check_access(b.address, b.len as usize, 2)?;
    Ok(())
}

fn events(_: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let (offset, stride) = if r.command_id == 0 {
        flag(r)?;
        (4, 0x28)
    } else {
        (0, 0x38)
    };
    if r.u32(offset)? > i32::MAX as u32 {
        return Err(Error::Malformed("negative play-history index"));
    }
    output(r, x, stride)?;
    // With no matching records, leave caller storage untouched and return count=0.
    Ok(Reply::data(0u32.to_le_bytes().to_vec()))
}

fn range(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(vec![0; 12]))
}

fn applications(_: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    flag(r)?;
    let input = r.buffer(BufferKind::Send, 0)?;
    if !input.len.is_multiple_of(8) {
        return Err(Error::Malformed("play-history application array"));
    }
    let ids = x.read_buffer(input, MAX_BUFFER)?;
    output(r, x, 0x18)?;
    if r.buffer(BufferKind::Receive, 0)?.len / 0x18 != (ids.len() / 8) as u64 {
        return Err(Error::Malformed("play-history array count mismatch"));
    }
    Ok(Reply::data(0u32.to_le_bytes().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };

    fn array(kind: BufferKind, len: u64, index: u16) -> Buffer {
        Buffer {
            kind,
            address: TLS + 0x1000 + u64::from(index) * 0x100,
            len,
            index,
        }
    }

    #[test]
    fn empty_events_preserve_buffers_and_validate_layouts() {
        let mut h = Harness::new();
        let s = h.open("pdm:qry");
        for (id, size, n) in [(0, 0x28, 8), (8, 0x38, 4), (10, 0x38, 4)] {
            let mut r = req(id, vec![0; n]);
            assert!(h.call(s, r.clone()).is_err());
            r.buffers.push(array(BufferKind::Receive, size, 0));
            let reply = h.call(s, r.clone()).unwrap();
            assert_eq!(reply.data, [0; 4]);
            assert!(reply.writes.is_empty());
            r.buffers[0].len -= 1;
            assert!(h.call(s, r).is_err());
        }
        assert_eq!(h.call(s, req(9, vec![])).unwrap().data, [0; 12]);
        assert!(matches!(
            h.call(s, req(999, vec![])),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn application_queries_validate_both_arrays() {
        let mut h = Harness::new();
        let s = h.open("pdm:qry");
        for (id, n) in [(17, 1), (18, 1), (19, 24)] {
            let mut r = req(id, vec![0; n]);
            r.buffers = vec![
                array(BufferKind::Send, 8, 0),
                array(BufferKind::Receive, 24, 1),
            ];
            assert_eq!(h.call(s, r.clone()).unwrap().data, [0; 4]);
            r.buffers[1].len = 48;
            assert!(h.call(s, r.clone()).is_err());
            r.buffers[1].len = 24;
            r.data[0] = 2;
            assert!(h.call(s, r).is_err());
        }
    }
}
