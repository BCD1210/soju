//! HDLS work-buffer ownership and dedicated FullKey virtual controllers.
use super::tables::c;
use super::*;
use soju_switch_input::{NpadState, Stick};

struct Work {
    handle: Handle,
    address: u64,
    token: u64,
}
struct Device {
    owner: ObjectId,
    slot: usize,
    info: [u8; 20],
    state: [u8; 64],
}
#[derive(Default)]
pub(super) struct Debug {
    work: BTreeMap<ObjectId, Work>,
    devices: BTreeMap<u64, Device>,
    serial: u64,
    cleared_slots: std::collections::BTreeSet<usize>,
}
pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    c(323, 0, command),
    c(324, 8, command),
    c(325, 8, command),
    c(327, 8, command),
    c(330, 20, command),
    c(331, 8, command),
    c(332, 72, command),
];
impl Debug {
    pub(super) fn close(&mut self, owner: ObjectId) -> Option<Handle> {
        self.cleared_slots.extend(
            self.devices
                .values()
                .filter(|d| d.owner == owner)
                .map(|d| d.slot),
        );
        self.devices.retain(|_, d| d.owner != owner);
        self.work.remove(&owner).map(|w| w.handle)
    }
    pub(super) fn publish(
        &mut self,
        k: &mut Kernel,
        m: &mut dyn GuestMemory,
        shared: Handle,
        sample: u64,
    ) -> Result<(), Error> {
        let slots: std::collections::BTreeSet<_> = self
            .devices
            .values()
            .map(|d| d.slot)
            .chain(self.cleared_slots.iter().copied())
            .collect();
        for slot in slots {
            let base = 0x9a00 + slot * 0x5000;
            let mut header = vec![0; 0x28];
            let mut ring = vec![0; 0x50];
            let mut properties = [0; 0x20];
            if let Some(d) = self.devices.values().find(|d| d.slot == slot) {
                header[0] = 1; // FullKey style.
                header[12..16].copy_from_slice(&d.info[4..8]);
                header[16..20].copy_from_slice(&d.info[8..12]);
                let state = NpadState {
                    buttons: u64::from_le_bytes(d.state[8..16].try_into().unwrap())
                        & !((1 << 18) | (1 << 19)),
                    left: Stick {
                        x: word(&d.state, 16) as i32,
                        y: word(&d.state, 20) as i32,
                    },
                    right: Stick {
                        x: word(&d.state, 24) as i32,
                        y: word(&d.state, 28) as i32,
                    },
                }
                .with_stick_buttons();
                ring[8..16].copy_from_slice(&17u64.to_le_bytes());
                ring[24..32].copy_from_slice(&1u64.to_le_bytes());
                ring[32..80].copy_from_slice(&shared::hid_sample(sample, state));
                if d.info[1] == 3 {
                    ring[72] |= 2;
                }
                properties[..4].copy_from_slice(&1u32.to_le_bytes());
                let flags = word(&d.state, 4);
                properties[8] = (((flags & 1) << 3) | ((flags >> 1) & 1)) as u8;
                properties[20..24].copy_from_slice(&d.state[..4]);
            }
            k.write_shared(m, shared, base, &header)
                .map_err(Error::Kernel)?;
            k.write_shared(m, shared, base + 0x28, &ring)
                .map_err(Error::Kernel)?;
            k.write_shared(m, shared, base + 0x4188, &properties)
                .map_err(Error::Kernel)?;
        }
        self.cleared_slots.clear();
        Ok(())
    }
}
fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn command(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let dbg = &mut s.hid_debug;
    if r.command_id == 323 {
        // Legacy autopilot commands are unavailable on the advertised modern profile.
        // There are no legacy autopilot states; HDLS devices have their own lifecycle.
        return Ok(Reply::success());
    }
    if r.command_id == 324 {
        if dbg.work.contains_key(&x.object)
            || r.copy_handles.len() != 1
            || !r.move_handles.is_empty()
            || r.u64(0)? != 0x1000
        {
            return Err(Error::Malformed("HDLS work-buffer initialization"));
        }
        let id = x
            .kernel
            .object_id(r.copy_handles[0])
            .map_err(Error::Kernel)?;
        let (address, size) = x
            .kernel
            .transfer(id)
            .ok_or(Error::Malformed("HDLS transfer memory"))?;
        if size != 0x1000 {
            return Err(Error::Malformed("HDLS transfer size"));
        }
        x.memory.check_access(address, size, 3)?;
        let handle = x
            .kernel
            .duplicate_handle(r.copy_handles[0])
            .map_err(Error::Kernel)?;
        dbg.serial += 1;
        let token = dbg.serial;
        dbg.work.insert(
            x.object,
            Work {
                handle,
                address,
                token,
            },
        );
        return Ok(Reply::data(token.to_le_bytes().to_vec()));
    }
    let work = dbg
        .work
        .get(&x.object)
        .ok_or(Error::Malformed("HDLS work buffer not attached"))?;
    if matches!(r.command_id, 325 | 327) && work.token != r.u64(0)? {
        return Err(Error::Malformed("HDLS session token"));
    }
    match r.command_id {
        325 => {
            s.retired_events.extend(dbg.close(x.object));
        }
        327 => {
            let mut bytes = vec![0; 8 + 16 * 96];
            bytes[..4].copy_from_slice(&(dbg.devices.len() as u32).to_le_bytes());
            for (i, (handle, d)) in dbg.devices.iter().enumerate() {
                let at = 8 + i * 96;
                bytes[at..at + 8].copy_from_slice(&handle.to_le_bytes());
                bytes[at + 8..at + 28].copy_from_slice(&d.info);
                bytes[at + 32..at + 96].copy_from_slice(&d.state);
            }
            x.memory.write(work.address, &bytes)?;
        }
        330 => {
            if r.data[0] != 3 || !matches!(r.data[1], 1 | 3) {
                return Err(Error::Malformed("unsupported HDLS device type/interface"));
            }
            let slot = (1..8)
                .find(|slot| dbg.devices.values().all(|d| d.slot != *slot))
                .ok_or(Error::Malformed("HDLS controller slots exhausted"))?;
            dbg.serial += 1;
            let handle = dbg.serial;
            dbg.devices.insert(
                handle,
                Device {
                    owner: x.object,
                    slot,
                    info: r.data[..20].try_into().unwrap(),
                    state: [0; 64],
                },
            );
            return Ok(Reply::data(handle.to_le_bytes().to_vec()));
        }
        331 | 332 => {
            let handle = r.u64(0)?;
            let d = dbg
                .devices
                .get_mut(&handle)
                .filter(|d| d.owner == x.object)
                .ok_or(Error::Malformed("HDLS device ownership"))?;
            if r.command_id == 331 {
                dbg.cleared_slots.insert(d.slot);
                dbg.devices.remove(&handle);
            } else {
                let state: [u8; 64] = r.data[8..72].try_into().unwrap();
                if word(&state, 0) > 4 || word(&state, 4) & !3 != 0 || word(&state, 56) != 0 {
                    return Err(Error::Malformed(
                        "HDLS power or unsupported motion attributes",
                    ));
                }
                for at in [16, 20, 24, 28] {
                    if !(-32767..=32767).contains(&(word(&state, at) as i32)) {
                        return Err(Error::Malformed("HDLS stick range"));
                    }
                }
                d.state = state;
                let buttons =
                    u64::from_le_bytes(d.state[8..16].try_into().unwrap()) & 0xfffffffff00fffff;
                d.state[8..16].copy_from_slice(&buttons.to_le_bytes());
            }
        }
        _ => unreachable!(),
    }
    Ok(Reply::success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{req, Harness},
        kernel::{Dispatch, HEAP, MAIN_THREAD},
        nro::BASE,
    };
    fn svc(h: &mut Harness, id: u16, a: u64, b: u64, c: u64) -> Vec<u64> {
        let mut regs = [0; 31];
        regs[1] = a;
        regs[2] = b;
        regs[3] = c;
        match h.k.dispatch(&mut h.m, MAIN_THREAD, id, &regs, BASE, 0) {
            Dispatch::Return(v) => {
                assert_eq!(v[0], 0);
                v
            }
            v => panic!("unexpected {v:?}"),
        }
    }
    #[test]
    fn device_state_roundtrip_retains_transfer_and_rejects_foreign_handles() {
        let mut h = Harness::new();
        svc(&mut h, 1, 0x200000, 0, 0);
        let transfer = svc(&mut h, 21, HEAP, 0x1000, 3)[1] as u32;
        let transfer_id = h.k.object_id(transfer).unwrap();
        let session = h.open("hid:dbg");
        let mut attach = req(324, 0x1000u64.to_le_bytes().to_vec());
        attach.copy_handles = vec![transfer];
        let token = h.call(session, attach.clone()).unwrap().data;
        assert!(h.call(session, attach).is_err());
        h.k.close_handle(&mut h.m, transfer).unwrap();
        assert!(h.k.transfer(transfer_id).is_some());
        let mut info = vec![0; 20];
        info[0] = 3;
        info[1] = 1;
        let handle = h.call(session, req(330, info.clone())).unwrap().data;
        let mut state = vec![0; 64];
        state[..4].copy_from_slice(&4u32.to_le_bytes());
        state[8..16].copy_from_slice(&0x100u64.to_le_bytes());
        state[16..20].copy_from_slice(&0x1234i32.to_le_bytes());
        state[20..24].copy_from_slice(&(-0x1234i32).to_le_bytes());
        let mut input = handle.clone();
        input.extend(&state);
        h.call(session, req(332, input.clone())).unwrap();
        let hid = h.open("hid");
        let resource = h.child(hid, 0);
        let shared = h.call(resource, req(0, vec![])).unwrap().copy_handles[0];
        let mut regs = [0; 31];
        regs[..4].copy_from_slice(&[shared as u64, 0x80000000, 0x40000, 1]);
        assert!(
            matches!(h.k.dispatch(&mut h.m, MAIN_THREAD, 0x13, &regs, BASE, 0), Dispatch::Return(v) if v == [0])
        );
        h.p.poll(&mut h.k, &mut h.m, 1).unwrap();
        let mut published = [0; 48];
        h.m.read(0x80000000 + 0x9a48 + 0x5000, &mut published)
            .unwrap();
        assert_eq!(&published[16..24], &state[8..16]);
        assert_eq!(&published[24..40], &state[16..32]);
        assert_eq!(published[40], 1);
        let other = h.open("hid:dbg");
        assert!(h.call(other, req(332, input)).is_err());
        h.call(session, req(327, token.clone())).unwrap();
        let mut bytes = vec![0; 104];
        h.m.read(HEAP, &mut bytes).unwrap();
        assert_eq!(word(&bytes, 0), 1);
        assert_eq!(&bytes[8..16], &handle);
        assert_eq!(&bytes[16..36], &info);
        assert_eq!(&bytes[40..104], &state);
        h.call(session, req(331, handle.clone())).unwrap();
        assert!(h.call(session, req(331, handle)).is_err());
        h.call(session, req(327, token.clone())).unwrap();
        h.m.read(HEAP, &mut bytes).unwrap();
        assert_eq!(word(&bytes, 0), 0);
        h.call(session, req(325, token)).unwrap();
        h.p.poll(&mut h.k, &mut h.m, 0).unwrap();
        assert!(h.k.transfer(transfer_id).is_none());
        let mut cleared = [1; 4];
        h.m.read(0x80000000 + 0x9a00 + 0x5000, &mut cleared)
            .unwrap();
        assert_eq!(cleared, [0; 4]);
    }
}
