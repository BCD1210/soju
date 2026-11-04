use super::{Error, invalid};
pub(super) fn clear(
    address: u64,
    width: u64,
    height: u64,
    block_log: u32,
    pixel: [u8; 4],
) -> Result<Vec<(u64, Vec<u8>)>, Error> {
    if !address.is_multiple_of(4)
        || width == 0
        || width > 4096
        || !width.is_multiple_of(16)
        || height == 0
        || height > 2160
        || !height.is_multiple_of(8)
        || block_log > 5
    {
        return Err(invalid("block linear clear geometry"));
    }
    let pitch = width * 4;
    let gobs = 1 << block_log;
    let rows = 8 * gobs;
    let padded = height.div_ceil(rows) * rows;
    let span = pitch * padded;
    if span > 32 * 1024 * 1024 || address.checked_add(span).is_none_or(|n| n > 1 << 40) {
        return Err(invalid("block linear clear limit"));
    }
    let full = height / rows;
    let remainder = height % rows;
    let mut output = Vec::new();
    if full > 0 {
        output.push((
            address,
            pixel.repeat(
                usize::try_from(full * pitch * rows / 4).map_err(|_| invalid("clear size"))?,
            ),
        ));
    }
    if remainder > 0 {
        for column in 0..pitch / 64 {
            output.push((
                address + full * pitch * rows + column * gobs * 512,
                pixel.repeat(
                    usize::try_from(remainder / 8 * 512 / 4).map_err(|_| invalid("clear size"))?,
                ),
            ));
        }
    }
    Ok(output)
}
pub(super) fn copy(
    source: u64,
    target: u64,
    width: u64,
    height: u64,
    source_pitch: u64,
    block_log: u32,
) -> Result<Vec<(u64, u64, usize)>, Error> {
    if width == 0
        || width > 4096
        || height == 0
        || height > 2160
        || width * height * 4 > 8 * 1024 * 1024
        || source_pitch < width * 4
        || block_log > 5
    {
        return Err(invalid("pitch to tiled geometry"));
    }
    let pitch = (width * 4).div_ceil(64) * 64;
    let gobs = 1 << block_log;
    let rows = 8 * gobs;
    let span = pitch * height.div_ceil(rows) * rows;
    let source_span = (height - 1)
        .checked_mul(source_pitch)
        .and_then(|n| n.checked_add(width * 4))
        .ok_or(invalid("copy range overflow"))?;
    if target.checked_add(span).is_none_or(|n| n > 1 << 40)
        || source.checked_add(source_span).is_none_or(|n| n > 1 << 40)
    {
        return Err(invalid("copy range overflow"));
    }
    let mut copies = Vec::new();
    for y in 0..height {
        let base =
            (y / rows) * pitch * rows + ((y / 8) % gobs) * 512 + ((y % 8) / 2) * 64 + (y % 2) * 16;
        for x in (0..width * 4).step_by(16) {
            let offset =
                base + (x / 64) * 512 * gobs + ((x % 64) / 32) * 256 + ((x % 32) / 16) * 32;
            copies.push((
                source + y * source_pitch + x,
                target + offset,
                usize::try_from((width * 4 - x).min(16)).map_err(|_| invalid("copy chunk"))?,
            ));
            if copies.len() > 65536 {
                return Err(invalid("copy operation limit"));
            }
        }
    }
    Ok(copies)
}
