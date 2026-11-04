//! Physical-pad enumeration. Virtual Npad input has no physical UniquePadId.
use super::{tables::c, *};

pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[c(321, 4, pads), c(703, 0, pads)];

fn pads(_: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.command_id == 321 && !matches!(r.u32(0)?, 0..=7 | 0x10 | 0x20) {
        return Ok(Reply::result(0x58aca));
    }
    let out = r.buffer(BufferKind::ReceiveList, 0)?;
    if out.len > MAX_BUFFER as u64 || !out.len.is_multiple_of(8) {
        return Err(Error::Malformed("unique pad output array"));
    }
    x.memory.check_access(out.address, out.len as usize, 2)?;
    Ok(Reply::data(0u64.to_le_bytes().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };
    #[test]
    fn virtual_input_does_not_claim_a_physical_notification_led() {
        let mut h = Harness::new();
        let s = h.open("hid:sys");
        for id in [321, 703] {
            let mut r = req(id, vec![0; 4]);
            r.buffers.push(Buffer {
                kind: BufferKind::ReceiveList,
                address: TLS + 0x1000,
                len: 16,
                index: 0,
            });
            let reply = h.call(s, r.clone()).unwrap();
            assert_eq!(reply.data, [0; 8]);
            assert!(reply.writes.is_empty());
            r.buffers[0].len = 15;
            assert!(h.call(s, r).is_err());
        }
        assert_eq!(
            h.call(s, req(321, 8u32.to_le_bytes().to_vec()))
                .unwrap()
                .result,
            0x58aca
        );
        assert!(matches!(
            h.call(s, req(830, vec![0; 80])),
            Err(Error::Unsupported { .. })
        ));
    }
}
