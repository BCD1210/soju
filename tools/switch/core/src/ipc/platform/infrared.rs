//! IR service lifecycle with no attached camera; no synthetic image samples.
use super::{tables::c, *};

const NOT_READY: u32 = 0xf0cd;
const INVALID_HANDLE: u32 = 0x198cd;

#[derive(Default)]
pub(super) struct Session {
    owner: Option<(u64, u64)>,
    active: bool,
    pub(super) shared: Option<Handle>,
}

pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    c(302, 8, lifecycle),
    c(303, 8, lifecycle),
    c(304, 8, shared),
    c(305, 16, camera),
    c(308, 48, camera),
    c(309, 16, camera),
    c(311, 4, handle),
    c(313, 16, camera),
    c(314, 16, camera),
    c(318, 16, camera),
    c(319, 16, lifecycle),
];

fn owner(r: &Request, offset: usize) -> Result<(u64, u64), Error> {
    Ok((
        r.pid.ok_or(Error::Malformed("IR request requires PID"))?,
        r.u64(offset)?,
    ))
}

fn lifecycle(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let offset = if r.command_id == 319 { 8 } else { 0 };
    let who = owner(r, offset)?;
    let session = s.infrared.entry(x.object).or_default();
    if session.owner.is_some_and(|old| old != who) {
        return Err(Error::Malformed("IR resource owner changed"));
    }
    session.owner = Some(who);
    session.active = r.command_id != 303;
    Ok(Reply::success())
}

fn status_image() -> Vec<u8> {
    let mut bytes = vec![0; 0x8000];
    for player in 0..9 {
        // Unconnected camera, stopped processor, no samples in its ring.
        bytes[player * 0xe30..player * 0xe30 + 4].copy_from_slice(&2u32.to_le_bytes());
    }
    bytes
}

fn shared(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let who = owner(r, 0)?;
    let session = s.infrared.entry(x.object).or_default();
    if session.owner.is_some_and(|old| old != who) {
        return Err(Error::Malformed("IR resource owner changed"));
    }
    session.owner = Some(who);
    if session.shared.is_none() {
        session.shared = Some(
            x.kernel
                .register_shared(x.memory, &status_image(), 1)
                .map_err(Error::Kernel)?,
        );
    }
    Ok(Reply::copy(session.shared.unwrap()))
}

fn handle(_: &mut Platform, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let player = match r.u32(0)? {
        n @ 0..=7 => n,
        0x20 => 8,
        _ => return Ok(Reply::result(INVALID_HANDLE)),
    };
    Ok(Reply::data(player.to_le_bytes().to_vec()))
}

fn camera(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.u32(0)? > 8 {
        return Ok(Reply::result(INVALID_HANDLE));
    }
    let who = owner(r, 8)?;
    let Some(session) = s.infrared.get(&x.object) else {
        return Ok(Reply::result(NOT_READY));
    };
    if !session.active || session.owner != Some(who) {
        return Ok(Reply::result(NOT_READY));
    }
    if r.command_id == 309 {
        let out = r.buffer(BufferKind::Receive, 0)?;
        if out.len > MAX_BUFFER as u64 {
            return Err(Error::Malformed("IR image buffer size"));
        }
        x.memory.check_access(out.address, out.len as usize, 2)?;
    }
    if r.command_id == 308 {
        let size = r.u64(40)?;
        if r.copy_handles.len() != 1 || !r.move_handles.is_empty() {
            return Err(Error::Malformed("IR transfer memory handles"));
        }
        let id = x
            .kernel
            .object_id(r.copy_handles[0])
            .map_err(Error::Kernel)?;
        if x.kernel.transfer(id).map(|(_, len)| len as u64) != Some(size) {
            return Err(Error::Malformed("IR transfer memory size/handle"));
        }
    }
    // No camera can supply an image or satisfy a requested MCU version.
    // In particular, error replies must not modify the caller's image buffer.
    Ok(Reply::result(NOT_READY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        nro::TLS,
    };

    #[test]
    fn absent_camera_never_publishes_an_image_or_firmware_version() {
        let mut h = Harness::new();
        let s = h.open("irs");
        let image = status_image();
        for slot in image[..0x7fb0].chunks_exact(0xe30) {
            assert_eq!(&slot[..4], &2u32.to_le_bytes());
            assert!(slot[4..].iter().all(|b| *b == 0));
        }
        let mut r = req(319, vec![0; 16]);
        assert!(h.call(s, r.clone()).is_err());
        r.pid = Some(1);
        assert_eq!(h.call(s, r).unwrap().result, 0);
        for command in [309, 314] {
            let mut r = req(command, vec![0; 16]);
            r.pid = Some(1);
            r.buffers.push(Buffer {
                kind: BufferKind::Receive,
                address: TLS,
                len: 256,
                index: 0,
            });
            let reply = h.call(s, r.clone()).unwrap();
            assert_eq!(reply.result, NOT_READY);
            assert!(reply.writes.is_empty() && reply.data.is_empty());
            r.data[1] = 1;
            assert_eq!(h.call(s, r).unwrap().result, INVALID_HANDLE);
        }
        assert_eq!(
            h.call(s, req(311, 0x20u32.to_le_bytes().to_vec()))
                .unwrap()
                .data,
            8u32.to_le_bytes()
        );
        assert_eq!(
            h.call(s, req(311, 0x10u32.to_le_bytes().to_vec()))
                .unwrap()
                .result,
            INVALID_HANDLE
        );
    }
}
