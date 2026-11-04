//! TZif time zone files (RFC 8536), the format of the zone rules inside the firmware's
//! TimeZoneBinary archive. The runner's `time` service uses this to convert POSIX time
//! to calendar time for the zone the player picks.

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalType {
    pub utc_offset: i32,
    pub is_dst: bool,
    pub abbreviation_index: u8,
}

#[derive(Debug, Clone)]
pub struct Zone {
    pub transitions: Vec<i64>,
    pub transition_types: Vec<u8>,
    pub types: Vec<LocalType>,
    pub abbreviations: Vec<u8>,
    /// POSIX TZ string footer (v2+), used after the last transition, e.g. "PST8PDT,M3.2.0,M11.1.0".
    pub footer: String,
}

fn be32(b: &[u8], at: usize) -> Result<i32> {
    b.get(at..at + 4).map(|s| i32::from_be_bytes(s.try_into().unwrap())).ok_or(Error::Truncated)
}

fn be64(b: &[u8], at: usize) -> Result<i64> {
    b.get(at..at + 8).map(|s| i64::from_be_bytes(s.try_into().unwrap())).ok_or(Error::Truncated)
}

struct Counts {
    isut: usize,
    isstd: usize,
    leap: usize,
    time: usize,
    typ: usize,
    chars: usize,
}

fn header(b: &[u8], at: usize) -> Result<(u8, Counts)> {
    let h = b.get(at..at + 44).ok_or(Error::Truncated)?;
    if &h[..4] != b"TZif" {
        return Err(Error::BadMagic("TZif"));
    }
    let c = |i: usize| -> Result<usize> {
        let v = be32(h, 20 + i * 4)?;
        if !(0..=1_000_000).contains(&v) {
            return Err(Error::Format("TZif count out of range"));
        }
        Ok(v as usize)
    };
    let counts = Counts { isut: c(0)?, isstd: c(1)?, leap: c(2)?, time: c(3)?, typ: c(4)?, chars: c(5)? };
    if counts.typ == 0 || counts.typ > 256 {
        return Err(Error::Format("TZif has no local time types"));
    }
    Ok((h[4], counts))
}

fn block_len(c: &Counts, time_size: usize) -> usize {
    c.time * time_size + c.time + c.typ * 6 + c.chars + c.leap * (time_size + 4) + c.isstd + c.isut
}

pub fn parse(b: &[u8]) -> Result<Zone> {
    let (version, c1) = header(b, 0)?;
    let v1_len = block_len(&c1, 4);
    let (c, data, wide) = if version >= b'2' {
        let at = 44 + v1_len;
        let (_, c2) = header(b, at)?;
        (c2, at + 44, true)
    } else {
        (c1, 44, false)
    };
    let ts = if wide { 8 } else { 4 };
    let need = data + block_len(&c, ts);
    if b.len() < need {
        return Err(Error::Truncated);
    }
    let mut p = data;
    let mut transitions = Vec::with_capacity(c.time);
    for i in 0..c.time {
        transitions.push(if wide { be64(b, p + i * 8)? } else { be32(b, p + i * 4)? as i64 });
    }
    p += c.time * ts;
    let transition_types = b[p..p + c.time].to_vec();
    p += c.time;
    let mut types = Vec::with_capacity(c.typ);
    for i in 0..c.typ {
        let e = p + i * 6;
        types.push(LocalType { utc_offset: be32(b, e)?, is_dst: b[e + 4] != 0, abbreviation_index: b[e + 5] });
    }
    p += c.typ * 6;
    let abbreviations = b[p..p + c.chars].to_vec();
    if transition_types.iter().any(|&t| t as usize >= c.typ) || types.iter().any(|t| t.abbreviation_index as usize >= c.chars.max(1)) {
        return Err(Error::Format("TZif index out of range"));
    }
    if transitions.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Error::Format("TZif transitions out of order"));
    }
    let footer = if wide {
        let rest = &b[need..];
        if rest.first() == Some(&b'\n') {
            let end = rest[1..].iter().position(|&c| c == b'\n').map(|e| e + 1).unwrap_or(rest.len());
            String::from_utf8_lossy(&rest[1..end]).into_owned()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    Ok(Zone { transitions, transition_types, types, abbreviations, footer })
}

impl Zone {
    /// Local time type in effect at POSIX time `t` from the transition table. Times after the
    /// last transition need the footer rule (see `footer`); this returns the last type then.
    pub fn local_type(&self, t: i64) -> LocalType {
        match self.transitions.partition_point(|&x| x <= t) {
            0 => self.types.iter().copied().find(|ty| !ty.is_dst).unwrap_or(self.types[0]),
            i => self.types[self.transition_types[i - 1] as usize],
        }
    }

    pub fn abbreviation(&self, ty: LocalType) -> String {
        let s = &self.abbreviations[ty.abbreviation_index as usize..];
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..end]).into_owned()
    }
}

/// A POSIX TZ rule such as "PST8PDT,M3.2.0,M11.1.0" or "KST-9" (the TZif footer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosixRule {
    pub std_abbr: String,
    /// UTC offset in seconds (east positive, unlike the POSIX string's sign).
    pub std_offset: i32,
    pub dst: Option<(String, i32, DateRule, i32, DateRule, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateRule {
    /// Mm.w.d: day d (0 = Sunday) of week w (5 = last) of month m.
    Month(u8, u8, u8),
    /// Jn: day n of the year, 1..=365, February 29 never counted.
    Julian1(u16),
    /// n: day n of the year, 0..=365, counting February 29.
    Julian0(u16),
}

fn take_abbr(s: &str) -> Option<(String, &str)> {
    if let Some(rest) = s.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    let end = s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len());
    if end < 3 {
        return None;
    }
    Some((s[..end].to_string(), &s[end..]))
}

/// "[+-]hh[:mm[:ss]]" -> seconds.
fn take_time(s: &str) -> Option<(i32, &str)> {
    let (sign, s) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    let end = s.find(|c: char| !(c.is_ascii_digit() || c == ':')).unwrap_or(s.len());
    let mut parts = s[..end].split(':').map(|p| p.parse::<i32>().ok());
    let h = parts.next()??;
    let m = parts.next().unwrap_or(Some(0))?;
    let sec = parts.next().unwrap_or(Some(0))?;
    if !(0..=167).contains(&h) || !(0..60).contains(&m) || !(0..60).contains(&sec) {
        return None;
    }
    Some((sign * (h * 3600 + m * 60 + sec), &s[end..]))
}

fn take_date(s: &str) -> Option<(DateRule, i32, &str)> {
    let (rule, rest) = if let Some(r) = s.strip_prefix('M') {
        let end = r.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(r.len());
        let v: Vec<u8> = r[..end].split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
        if v.len() != 3 || !(1..=12).contains(&v[0]) || !(1..=5).contains(&v[1]) || v[2] > 6 {
            return None;
        }
        (DateRule::Month(v[0], v[1], v[2]), &r[end..])
    } else if let Some(r) = s.strip_prefix('J') {
        let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
        let n: u16 = r[..end].parse().ok().filter(|n| (1..=365).contains(n))?;
        (DateRule::Julian1(n), &r[end..])
    } else {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let n: u16 = s[..end].parse().ok().filter(|n| *n <= 365)?;
        (DateRule::Julian0(n), &s[end..])
    };
    let (time, rest) = match rest.strip_prefix('/') {
        Some(t) => take_time(t)?,
        None => (7200, rest),
    };
    Some((rule, time, rest))
}

impl PosixRule {
    pub fn parse(s: &str) -> Option<Self> {
        let (std_abbr, rest) = take_abbr(s)?;
        let (off, rest) = take_time(rest)?;
        if rest.is_empty() {
            return Some(PosixRule { std_abbr, std_offset: -off, dst: None });
        }
        let (dst_abbr, mut rest) = take_abbr(rest)?;
        let mut dst_off = -off + 3600;
        if !rest.starts_with(',') && !rest.is_empty() {
            let (o, r) = take_time(rest)?;
            dst_off = -o;
            rest = r;
        }
        let rest = rest.strip_prefix(',')?;
        let (start, st, rest) = take_date(rest)?;
        let rest = rest.strip_prefix(',')?;
        let (end, et, rest) = take_date(rest)?;
        if !rest.is_empty() {
            return None;
        }
        Some(PosixRule { std_abbr, std_offset: -off, dst: Some((dst_abbr, dst_off, start, st, end, et)) })
    }

    /// (UTC offset, is DST) at POSIX time t.
    pub fn offset_at(&self, t: i64) -> (i32, bool) {
        let Some((_, dst_off, start, st, end, et)) = &self.dst else { return (self.std_offset, false) };
        let year = civil_from_days((t + self.std_offset as i64).div_euclid(86400)).0;
        // Transition instants in UTC: start is given in standard local time, end in DST local time.
        let begin = days_from_rule(year, *start) * 86400 + *st as i64 - self.std_offset as i64;
        let finish = days_from_rule(year, *end) * 86400 + *et as i64 - *dst_off as i64;
        let in_dst = if begin < finish { t >= begin && t < finish } else { t >= begin || t < finish };
        if in_dst { (*dst_off, true) } else { (self.std_offset, false) }
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn days_from_rule(year: i64, r: DateRule) -> i64 {
    match r {
        DateRule::Julian0(n) => days_from_civil(year, 1, 1) + n as i64,
        DateRule::Julian1(n) => {
            let extra = (is_leap(year) && n >= 60) as i64;
            days_from_civil(year, 1, 1) + n as i64 - 1 + extra
        }
        DateRule::Month(m, w, d) => {
            let first = days_from_civil(year, m as u32, 1);
            let wd_first = (first + 4).rem_euclid(7); // 1970-01-01 was a Thursday
            let mut day = first + (d as i64 - wd_first).rem_euclid(7) + (w as i64 - 1) * 7;
            let next_month = if m == 12 { days_from_civil(year + 1, 1, 1) } else { days_from_civil(year, m as u32 + 1, 1) };
            while day >= next_month {
                day -= 7;
            }
            day
        }
    }
}

impl Zone {
    /// UTC offset and DST flag at any time, using the footer rule after the last transition.
    pub fn offset_at(&self, t: i64) -> (i32, bool) {
        if self.transitions.last().is_none_or(|&last| t >= last) {
            if let Some(rule) = PosixRule::parse(&self.footer) {
                return rule.offset_at(t);
            }
        }
        let ty = self.local_type(t);
        (ty.utc_offset, ty.is_dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(zone: &str) -> Option<Vec<u8>> {
        std::fs::read(format!("/usr/share/zoneinfo/{zone}")).ok()
    }

    #[test]
    fn los_angeles_and_seoul_from_the_host_database() {
        let Some(la) = host("America/Los_Angeles") else { return };
        let z = parse(&la).unwrap();
        // 2024-01-15 12:00 UTC is PST (-8 h); 2024-07-15 12:00 UTC is PDT (-7 h).
        let winter = z.local_type(1_705_320_000);
        let summer = z.local_type(1_721_044_800);
        // Hosts that ship slim TZif files rely on the footer for recent years; accept either.
        if z.transitions.last().is_some_and(|&t| t > 1_721_044_800) {
            assert_eq!((winter.utc_offset, winter.is_dst, z.abbreviation(winter).as_str()), (-28800, false, "PST"));
            assert_eq!((summer.utc_offset, summer.is_dst, z.abbreviation(summer).as_str()), (-25200, true, "PDT"));
        }
        assert!(z.footer.starts_with("PST8PDT"), "{}", z.footer);
        // 1950 (before 1967 US rules) is still a Pacific offset.
        assert_eq!(z.local_type(-631_152_000).utc_offset, -28800);
        if let Some(seoul) = host("Asia/Seoul") {
            let s = parse(&seoul).unwrap();
            assert_eq!(s.local_type(1_721_044_800).utc_offset, 9 * 3600);
            assert_eq!(s.abbreviation(s.local_type(1_721_044_800)), "KST");
        }
    }

    #[test]
    fn every_host_zone_parses() {
        let Ok(root) = std::fs::read_dir("/usr/share/zoneinfo/America") else { return };
        let mut n = 0;
        for e in root.flatten() {
            if e.path().is_file() {
                let b = std::fs::read(e.path()).unwrap();
                if b.starts_with(b"TZif") {
                    parse(&b).unwrap_or_else(|err| panic!("{:?}: {err}", e.path()));
                    n += 1;
                }
            }
        }
        assert!(n > 50, "only {n} zones");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"nope").is_err());
        let mut b = std::fs::read("/usr/share/zoneinfo/UTC").unwrap_or_else(|_| {
            let mut v = b"TZif2".to_vec();
            v.resize(44, 0);
            v
        });
        b.truncate(b.len().min(50));
        assert!(parse(&b).is_err());
    }

    #[test]
    fn posix_rules_parse() {
        let r = PosixRule::parse("PST8PDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(r.std_offset, -28800);
        assert_eq!(r.dst.as_ref().unwrap().1, -25200);
        assert_eq!(PosixRule::parse("KST-9").unwrap().std_offset, 32400);
        assert_eq!(PosixRule::parse("<+0530>-5:30").unwrap().std_offset, 19800);
        let au = PosixRule::parse("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(au.offset_at(1_705_320_000), (39600, true)); // January is summer in Sydney
        assert!(PosixRule::parse("X1").is_none());
        assert!(PosixRule::parse("PST8PDT,M13.2.0,M11.1.0").is_none());
    }

    #[test]
    fn footer_rule_matches_the_table_hour_by_hour() {
        for zone in ["America/Los_Angeles", "America/New_York", "Europe/London", "Australia/Sydney", "America/Santiago"] {
            let Some(b) = host(zone) else { continue };
            let z = parse(&b).unwrap();
            let Some(rule) = PosixRule::parse(&z.footer) else { continue };
            let last = *z.transitions.last().unwrap();
            // Compare 2025-01-01 .. the last tabulated transition, every hour.
            let mut t = 1_735_689_600i64;
            let mut checked = 0;
            while t < last {
                let ty = z.local_type(t);
                assert_eq!(rule.offset_at(t), (ty.utc_offset, ty.is_dst), "{zone} at {t}");
                t += 3600;
                checked += 1;
            }
            assert!(checked > 1000, "{zone}: {checked}");
        }
    }

    #[test]
    fn calendar_helpers() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2040, 2, 29)), (2040, 2, 29));
        assert_eq!(days_from_rule(2024, DateRule::Month(3, 2, 0)), days_from_civil(2024, 3, 10));
        assert_eq!(days_from_rule(2024, DateRule::Month(11, 1, 0)), days_from_civil(2024, 11, 3));
        assert_eq!(days_from_rule(2024, DateRule::Month(10, 5, 0)), days_from_civil(2024, 10, 27));
    }
}
