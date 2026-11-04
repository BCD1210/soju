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
// Proleptic Gregorian civil conversion, with UTC and Python datetime's year bounds.
pub(super) fn calendar(
    _: &mut Platform,
    req: &Request,
    _: &mut Context<'_>,
) -> Result<Reply, Error> {
    let seconds = i64::from_le_bytes(req.u64(0)?.to_le_bytes());
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
    out[16..19].copy_from_slice(b"UTC");
    Ok(Reply::data(out))
}
