use super::*;
use std::collections::BTreeSet;
pub(super) fn read_pcm(req: &Request, ctx: &Context<'_>) -> Result<Vec<u8>, Error> {
    let buffer = req.input()?;
    if buffer.len < 40 {
        return Err(Error::Malformed("audio buffer descriptor"));
    }
    let mut address = buffer.address;
    let mut visited = BTreeSet::new();
    let mut pcm = Vec::new();
    loop {
        if !visited.insert(address) || visited.len() > 64 {
            return Err(Error::Malformed("audio buffer list cycle or limit"));
        }
        let mut descriptor = [0; 40];
        ctx.memory.read(address, &mut descriptor)?;
        let next = read_u64(&descriptor, 0)?;
        let data = read_u64(&descriptor, 8)?;
        let capacity = read_u64(&descriptor, 16)?;
        let len = read_u64(&descriptor, 24)?;
        let offset = read_u64(&descriptor, 32)?;
        let legacy = data % 4096 == 0 && capacity > 0 && capacity.checked_mul(8) == Some(len);
        if offset != 0 || (len > capacity && !legacy) || len % 4 != 0 || len > MAX_PCM as u64 {
            return Err(Error::Malformed("audio data layout"));
        }
        let length = usize::try_from(len).map_err(|_| Error::Malformed("PCM size"))?;
        if pcm.len() + length > MAX_PCM {
            return Err(Error::Malformed("audio PCM limit"));
        }
        let start = pcm.len();
        pcm.resize(start + length, 0);
        ctx.memory.read(data, &mut pcm[start..])?;
        if next == 0 {
            break;
        }
        address = next;
    }
    Ok(pcm)
}
