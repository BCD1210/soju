use super::{Error, Method, invalid};
pub(crate) fn decode_entry(low: u32, high: u32) -> Result<Option<(u64, usize)>, Error> {
    if low & 3 != 0 || high & 0x100 != 0 {
        return Err(invalid("conditional or privileged GPFIFO entry"));
    }
    let count = (high >> 10) & 0x1fffff;
    if count == 0 {
        return if low == 0 && high == 0 {
            Ok(None)
        } else {
            Err(invalid("GPFIFO control entry"))
        };
    }
    if count > 1_048_576 {
        return Err(invalid("GPFIFO word limit"));
    }
    let address = u64::from(low & 0xfffffffc) | (u64::from(high & 255) << 32);
    if address + u64::from(count) * 4 > 1 << 40 {
        return Err(invalid("GPFIFO address overflow"));
    }
    Ok(Some((
        address,
        usize::try_from(count).map_err(|_| invalid("GPFIFO count"))?,
    )))
}
pub(crate) fn decode_packets(address: u64, words: &[u32]) -> Result<Vec<Method>, Error> {
    if words.len() > 1_048_576 {
        return Err(invalid("GPU stream word limit"));
    }
    let mut output = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let header = words[index];
        index += 1;
        if header == 0 {
            continue;
        }
        let method = header & 0x1fff;
        if method & 0x1000 != 0 {
            return Err(invalid("extended GPU method address"));
        }
        let subchannel = (header >> 13) & 7;
        let argument = (header >> 16) & 0x1fff;
        let mode = header >> 29;
        if mode == 4 {
            output.push(Method {
                address,
                subchannel,
                method: method * 4,
                value: argument,
            });
        } else {
            if !matches!(mode, 1 | 3 | 5) {
                return Err(invalid("unsupported GPU packet mode"));
            }
            let count = usize::try_from(argument).map_err(|_| invalid("GPU packet count"))?;
            if count > words.len() - index {
                return Err(invalid("truncated GPU packet"));
            }
            for (n, &value) in words[index..index + count].iter().enumerate() {
                let n = u32::try_from(n).map_err(|_| invalid("GPU method count"))?;
                let destination = method
                    + match mode {
                        1 => n,
                        5 => n.min(1),
                        _ => 0,
                    };
                if destination > 0xfff {
                    return Err(invalid("GPU method overflow"));
                }
                output.push(Method {
                    address,
                    subchannel,
                    method: destination * 4,
                    value,
                });
            }
            index += count;
        }
        if output.len() > 1_048_576 {
            return Err(invalid("GPU method count limit"));
        }
    }
    Ok(output)
}
