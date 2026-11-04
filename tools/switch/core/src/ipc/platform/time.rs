use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
pub(super) enum Clock {
    Host,
    Fixed(i64),
}
impl Clock {
    pub(super) fn seconds(&self, now_ns: u64) -> Result<i64, Error> {
        match self {
            Self::Host => i64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| Error::Malformed("host clock before epoch"))?
                    .as_secs(),
            )
            .map_err(|_| Error::Malformed("host clock overflow")),
            Self::Fixed(epoch) => epoch
                .checked_add(
                    i64::try_from(now_ns / 1_000_000_000)
                        .map_err(|_| Error::Malformed("clock overflow"))?,
                )
                .ok_or(Error::Malformed("clock overflow")),
        }
    }
}
impl Platform {
    pub(super) fn clock_read(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        Ok(Reply::data(
            self.clock.seconds(ctx.now_ns)?.to_le_bytes().to_vec(),
        ))
    }
}
pub(super) struct Timezone {
    name: String,
    bytes: Vec<u8>,
    zone: soju_switch_fs::tzif::Zone,
}
impl Timezone {
    pub(super) fn host() -> Result<Self, Error> {
        let path = std::fs::canonicalize("/etc/localtime")?;
        let path = path
            .to_str()
            .ok_or(Error::Malformed("host timezone path"))?;
        let name = path
            .split_once("/zoneinfo/")
            .map(|(_, name)| name)
            .ok_or(Error::Malformed("host timezone location"))?;
        Self::load(name, std::fs::read("/etc/localtime")?)
    }
    fn load(name: &str, bytes: Vec<u8>) -> Result<Self, Error> {
        if name.len() >= 36 || bytes.len() > 0x3ff0 {
            return Err(Error::Malformed("host timezone size"));
        }
        let zone = soju_switch_fs::tzif::parse(&bytes)
            .map_err(|e| Error::Message(format!("host timezone: {e}")))?;
        Ok(Self {
            name: name.into(),
            bytes,
            zone,
        })
    }
    fn location(&self) -> Vec<u8> {
        let mut out = vec![0; 36];
        out[..self.name.len()].copy_from_slice(self.name.as_bytes());
        out
    }
    // TimeZoneRule is opaque to libnx: carry only our host TZif snapshot, using the host database.
    fn rule(&self) -> Vec<u8> {
        let mut out = vec![0; 0x4000];
        out[..8].copy_from_slice(b"SOJUTZ01");
        out[8..12].copy_from_slice(&(self.bytes.len() as u32).to_le_bytes());
        out[16..16 + self.bytes.len()].copy_from_slice(&self.bytes);
        out
    }
}
pub(super) fn location(s: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(s.timezone.location()))
}
pub(super) fn load_rule(
    s: &mut Platform,
    r: &Request,
    _: &mut Context<'_>,
) -> Result<Reply, Error> {
    if r.data[..36] != s.timezone.location() {
        return Err(Error::Malformed("unavailable timezone location"));
    }
    let out = r.output()?;
    if out.len != 0x4000 {
        return Err(Error::Malformed("timezone rule size"));
    }
    Reply::success().write(out, s.timezone.rule())
}
// Proleptic Gregorian civil conversion after applying the selected host rules.
pub(super) fn calendar(
    platform: &mut Platform,
    req: &Request,
    ctx: &mut Context<'_>,
) -> Result<Reply, Error> {
    let timestamp = i64::from_le_bytes(req.u64(0)?.to_le_bytes());
    if req.command_id == 100 {
        let bytes = ctx.read_buffer(req.input()?, 0x4000)?;
        if bytes != platform.timezone.rule() {
            return Err(Error::Malformed("unrecognized timezone rule"));
        }
    }
    let zone = &platform.timezone.zone;
    if !(-62_135_596_800..=253_402_300_799).contains(&timestamp) {
        return Err(Error::Malformed("calendar timestamp"));
    }
    let (offset, dst) = zone.offset_at(timestamp);
    let seconds = timestamp
        .checked_add(i64::from(offset))
        .ok_or(Error::Malformed("calendar offset overflow"))?;
    if !(-62_135_596_800..=253_402_300_799).contains(&seconds) {
        return Err(Error::Malformed("calendar timestamp"));
    }
    let days = seconds.div_euclid(86400);
    let day_seconds = seconds.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_index = usize::try_from(month - 1).map_err(|_| Error::Malformed("calendar month"))?;
    let year_day = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month_index] + day - 1
        + i64::from(leap && month > 2);
    let mut out = vec![0; 32];
    out[..2].copy_from_slice(
        &u16::try_from(year)
            .map_err(|_| Error::Malformed("calendar year"))?
            .to_le_bytes(),
    );
    for (index, value) in [
        month,
        day,
        day_seconds / 3600,
        day_seconds / 60 % 60,
        day_seconds % 60,
    ]
    .into_iter()
    .enumerate()
    {
        out[index + 2] = u8::try_from(value).map_err(|_| Error::Malformed("calendar field"))?;
    }
    out[8..12].copy_from_slice(
        &u32::try_from((days + 4).rem_euclid(7))
            .map_err(|_| Error::Malformed("weekday"))?
            .to_le_bytes(),
    );
    out[12..16].copy_from_slice(
        &u32::try_from(year_day)
            .map_err(|_| Error::Malformed("year day"))?
            .to_le_bytes(),
    );
    let abbreviation = if zone
        .transitions
        .last()
        .is_none_or(|&last| timestamp >= last)
    {
        soju_switch_fs::tzif::PosixRule::parse(&zone.footer).map(|r| {
            if dst {
                r.dst.map(|v| v.0).unwrap_or(r.std_abbr)
            } else {
                r.std_abbr
            }
        })
    } else {
        None
    }
    .unwrap_or_else(|| zone.abbreviation(zone.local_type(timestamp)));
    let abbreviation = abbreviation.as_bytes();
    let len = abbreviation.len().min(7);
    out[16..16 + len].copy_from_slice(&abbreviation[..len]);
    out[24..28].copy_from_slice(&u32::from(dst).to_le_bytes());
    out[28..32].copy_from_slice(&offset.to_le_bytes());
    Ok(Reply::data(out))
}

#[cfg(test)]
impl Timezone {
    pub(super) fn utc_fixture() -> Self {
        let mut bytes = vec![0; 44];
        bytes[..4].copy_from_slice(b"TZif");
        bytes[39] = 1;
        bytes[43] = 4;
        bytes.extend_from_slice(&[0; 6]);
        bytes.extend_from_slice(b"UTC\0");
        Self::load("Etc/UTC", bytes).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Harness, req};
    use super::*;

    #[test]
    fn pacific_calendar_tracks_dst_and_reports_location_and_rule() {
        let mut h = Harness::new();
        h.p.timezone = Timezone::load(
            "America/Los_Angeles",
            std::fs::read("/usr/share/zoneinfo/America/Los_Angeles").unwrap(),
        )
        .unwrap();
        let root = h.open("time:u");
        let zone = h.child(root, 3);
        let location = h.call(zone, req(0, vec![])).unwrap().data;
        assert_eq!(&location[..19], b"America/Los_Angeles");
        let mut load = req(4, location);
        load.buffers.push(Buffer {
            kind: BufferKind::Receive,
            address: 0,
            len: 0x4000,
            index: 0,
        });
        let rule = h.call(zone, load).unwrap().writes.remove(0).bytes;
        assert_eq!(&rule[..8], b"SOJUTZ01");
        // One second across each 2024 Pacific DST transition, plus a footer-only year.
        for (stamp, hour, minute, dst, offset, abbr) in [
            (1710064799i64, 1, 59, 0u32, -28800i32, b"PST"),
            (1710064800, 3, 0, 1, -25200, b"PDT"),
            (1730624399, 1, 59, 1, -25200, b"PDT"),
            (1730624400, 1, 0, 0, -28800, b"PST"),
            (2224929600, 5, 0, 1, -25200, b"PDT"),
        ] {
            let data = h
                .call(zone, req(101, stamp.to_le_bytes().to_vec()))
                .unwrap()
                .data;
            assert_eq!((data[4], data[5]), (hour, minute), "{stamp}");
            assert_eq!(&data[16..19], abbr);
            assert_eq!(&data[24..28], &dst.to_le_bytes());
            assert_eq!(&data[28..32], &offset.to_le_bytes());
        }
    }
}
