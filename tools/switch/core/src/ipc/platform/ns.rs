//! NS read-only application control data from the current launch's control property.
use super::{tables::c, *};

pub(super) const CONTROL: &[CommandSpec<Platform>] = &[c(0, 16, control)];

fn control(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.data[0] > 2 {
        return Err(Error::Malformed("application control source"));
    }
    let out = r.buffer(BufferKind::Receive, 0)?;
    if out.len > MAX_BUFFER as u64 {
        return Err(Error::Malformed("application control buffer limit"));
    }
    x.memory.check_access(out.address, out.len as usize, 2)?;
    // A launch exposes only its own supplied property; there is no installed-title database.
    if r.u64(8)? != s.program_id || s.control.is_none() {
        return Ok(Reply::result(0x410));
    }
    let property = s.control.as_ref().unwrap();
    Reply::data((property.len() as u32).to_le_bytes().to_vec()).write(out, property.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };

    #[test]
    fn control_lookup_checks_identity_source_and_output_capacity() {
        let mut h = Harness::new();
        h.p.program_id = 123;
        h.p.set_control(vec![0x42; 0x4000]).unwrap();
        let root = h.open("ns:am2");
        let child = h.child(root, 7989);
        let mut r = req(0, [1u64.to_le_bytes(), 456u64.to_le_bytes()].concat());
        r.buffers.push(Buffer {
            kind: BufferKind::Receive,
            address: TLS,
            len: 0x4000,
            index: 0,
        });
        let missing = h.call(child, r.clone()).unwrap();
        assert_eq!(missing.result, 0x410);
        assert!(missing.writes.is_empty());
        r.data[8..16].copy_from_slice(&123u64.to_le_bytes());
        for source in 0..3 {
            r.data[0] = source;
            let reply = h.call(child, r.clone()).unwrap();
            assert_eq!(reply.data, 0x4000u32.to_le_bytes());
            assert_eq!(reply.writes[0].bytes, vec![0x42; 0x4000]);
        }
        r.data[0] = 3;
        assert!(h.call(child, r.clone()).is_err());
        r.data[0] = 1;
        r.buffers[0].len = 0x3fff;
        assert!(h.call(child, r.clone()).is_err());
        r.buffers[0].len = 0x4000;
        h.p.control = None;
        assert_eq!(h.call(child, r).unwrap().result, 0x410);
        assert!(matches!(
            h.call(root, req(0, vec![])),
            Err(Error::Unsupported { .. })
        ));
    }
}
