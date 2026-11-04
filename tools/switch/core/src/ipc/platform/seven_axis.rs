//! Stationary seven-six-axis fusion backed by retained transfer memory.
use super::*;

pub(super) struct Sensor {
    owner: (u64, u64),
    pub(super) handles: [Handle; 2],
    address: u64,
    running: bool,
    strength: f32,
    origin: u64,
    last: u64,
    sample: u64,
    ring: Vec<u8>,
}

pub(super) fn command(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let owner = (
        r.pid.ok_or(Error::Malformed("seven-axis PID"))?,
        r.u64(if r.command_id == 308 { 8 } else { 0 })?,
    );
    if r.command_id == 306 {
        if s.seven_axis.contains_key(&x.object)
            || r.copy_handles.len() != 2
            || !r.move_handles.is_empty()
        {
            return Err(Error::Malformed("seven-axis initialization state/handles"));
        }
        let mut transfers = Vec::new();
        for (i, size) in [0x1000, 0x7f000].into_iter().enumerate() {
            let id = x
                .kernel
                .object_id(r.copy_handles[i])
                .map_err(Error::Kernel)?;
            let t = x
                .kernel
                .transfer(id)
                .ok_or(Error::Malformed("seven-axis transfer handle"))?;
            if t.1 != size || r.u64(8 + i * 8)? != size as u64 {
                return Err(Error::Malformed("seven-axis transfer size"));
            }
            transfers.push(t);
        }
        if transfers[0].0 < transfers[1].0 + transfers[1].1 as u64
            && transfers[1].0 < transfers[0].0 + transfers[0].1 as u64
        {
            return Err(Error::Malformed("overlapping seven-axis transfers"));
        }
        let a = x
            .kernel
            .duplicate_handle(r.copy_handles[0])
            .map_err(Error::Kernel)?;
        let b = match x.kernel.duplicate_handle(r.copy_handles[1]) {
            Ok(b) => b,
            Err(e) => {
                x.kernel.close_handle(x.memory, a).map_err(Error::Kernel)?;
                return Err(Error::Kernel(e));
            }
        };
        let mut ring = vec![0; 0x1000];
        ring[8..16].copy_from_slice(&33u64.to_le_bytes());
        x.memory.write_kernel(transfers[0].0, &ring)?;
        s.seven_axis.insert(
            x.object,
            Sensor {
                owner,
                handles: [a, b],
                address: transfers[0].0,
                running: false,
                strength: 1.,
                origin: x.now_ns,
                last: x.now_ns,
                sample: 0,
                ring,
            },
        );
        return Ok(Reply::success());
    }
    let sensor = s
        .seven_axis
        .get_mut(&x.object)
        .ok_or(Error::Malformed("seven-axis not initialized"))?;
    if sensor.owner != owner {
        return Err(Error::Malformed("seven-axis owner mismatch"));
    }
    match r.command_id {
        303 => {} // The initialized stationary fusion source is available.
        304 => {
            sensor.running = true;
            sensor.last = x.now_ns.saturating_sub(5_000_000);
        }
        305 => sensor.running = false,
        307 => {
            let sensor = s.seven_axis.remove(&x.object).unwrap();
            s.retired_events.extend(sensor.handles);
        }
        308 => {
            let strength = f32::from_bits(r.u32(0)?);
            if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
                return Err(Error::Malformed("seven-axis fusion strength"));
            }
            sensor.strength = strength;
        }
        309 => return Ok(Reply::data(sensor.strength.to_le_bytes().to_vec())),
        310 => sensor.origin = x.now_ns,
        _ => unreachable!(),
    }
    Ok(Reply::success())
}

impl Sensor {
    pub(super) fn poll(&mut self, memory: &mut dyn GuestMemory, now: u64) -> Result<bool, Error> {
        if !self.running || now.saturating_sub(self.last) < 5_000_000 {
            return Ok(false);
        }
        self.last = now;
        self.sample = self.sample.wrapping_add(1);
        let slot = (self.sample - 1) % 33;
        let start = 0x20 + slot as usize * 0x50;
        let entry = &mut self.ring[start..start + 0x50];
        entry.fill(0);
        entry[..8].copy_from_slice(&self.sample.to_le_bytes());
        entry[0x10..0x18].copy_from_slice(&now.saturating_sub(self.origin).to_le_bytes());
        entry[0x18..0x20].copy_from_slice(&self.sample.to_le_bytes());
        entry[0x30..0x34].copy_from_slice(&1f32.to_le_bytes()); // +Z gravity
        entry[0x4c..0x50].copy_from_slice(&1f32.to_le_bytes()); // identity quaternion
        self.ring[0x10..0x18].copy_from_slice(&slot.to_le_bytes());
        self.ring[0x18..0x20].copy_from_slice(&self.sample.min(33).to_le_bytes());
        memory.write_kernel(self.address, &self.ring)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::platform::tests::{Harness, req},
        kernel::{Dispatch, HEAP, MAIN_THREAD},
        nro::BASE,
    };
    fn invoke(h: &mut Harness, svc: u16, a: u64, b: u64, c: u64) -> Vec<u64> {
        let mut regs = [0; 31];
        regs[1] = a;
        regs[2] = b;
        regs[3] = c;
        match h.k.dispatch(&mut h.m, MAIN_THREAD, svc, &regs, BASE, 0) {
            Dispatch::Return(v) => {
                assert_eq!(v[0], 0);
                v
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    fn call(h: &mut Harness, session: Handle, id: u32, data: Vec<u8>) -> Result<Reply, Error> {
        let mut r = req(id, data);
        r.pid = Some(1);
        h.call(session, r)
    }
    #[test]
    fn fusion_ring_wraps_and_retains_both_transfers_until_finalize() {
        let mut h = Harness::new();
        invoke(&mut h, 1, 0x200000, 0, 0);
        let a = invoke(&mut h, 21, HEAP, 0x1000, 1)[1] as u32;
        let b = invoke(&mut h, 21, HEAP + 0x1000, 0x7f000, 0)[1] as u32;
        let ids = [h.k.object_id(a).unwrap(), h.k.object_id(b).unwrap()];
        let session = h.open("hid");
        let mut r = req(
            306,
            [0u64, 0x1000, 0x7f000]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect(),
        );
        r.pid = Some(1);
        r.copy_handles = vec![a, b];
        assert_eq!(h.call(session, r.clone()).unwrap().result, 0);
        assert!(h.call(session, r).is_err());
        h.k.close_handle(&mut h.m, a).unwrap();
        h.k.close_handle(&mut h.m, b).unwrap();
        assert!(ids.iter().all(|id| h.k.transfer(*id).is_some()));
        call(&mut h, session, 304, vec![0; 8]).unwrap();
        for i in 0..40 {
            h.p.poll(&mut h.k, &mut h.m, 2_000_000_000 + i * 5_000_000)
                .unwrap();
        }
        let mut ring = vec![0; 0x1000];
        h.m.read(HEAP, &mut ring).unwrap();
        assert_eq!(u64::from_le_bytes(ring[0x10..0x18].try_into().unwrap()), 6);
        assert_eq!(u64::from_le_bytes(ring[0x18..0x20].try_into().unwrap()), 33);
        let slot = 0x20 + 6 * 0x50;
        assert_eq!(
            u64::from_le_bytes(ring[slot..slot + 8].try_into().unwrap()),
            40
        );
        assert_eq!(&ring[slot + 0x30..slot + 0x34], &1f32.to_le_bytes());
        assert_eq!(&ring[slot + 0x4c..slot + 0x50], &1f32.to_le_bytes());
        call(&mut h, session, 305, vec![0; 8]).unwrap();
        h.p.poll(&mut h.k, &mut h.m, 3_000_000_000).unwrap();
        let mut stopped = vec![0; 0x1000];
        h.m.read(HEAP, &mut stopped).unwrap();
        assert_eq!(stopped, ring);
        for strength in [0f32, 0.25, 1.] {
            let mut data = vec![0; 16];
            data[..4].copy_from_slice(&strength.to_le_bytes());
            call(&mut h, session, 308, data).unwrap();
            assert_eq!(
                call(&mut h, session, 309, vec![0; 8]).unwrap().data,
                strength.to_le_bytes()
            );
        }
        call(&mut h, session, 307, vec![0; 8]).unwrap();
        h.p.poll(&mut h.k, &mut h.m, 3_000_000_000).unwrap();
        assert!(ids.iter().all(|id| h.k.transfer(*id).is_none()));
        assert!(h.m.check_access(HEAP, 0x80000, 3).is_ok());
    }
}
