//! LZ4 block decompression (the format NSO segments use), from the public LZ4 block spec.

use crate::{Error, Result};

pub fn decompress(src: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0usize;
    let len_ext = |i: &mut usize, mut n: usize| -> Result<usize> {
        if n == 15 {
            loop {
                let b = *src.get(*i).ok_or(Error::Truncated)?;
                *i += 1;
                n = n.checked_add(b as usize).ok_or(Error::Format("LZ4 length overflow"))?;
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    };
    while i < src.len() {
        let token = src[i];
        i += 1;
        let lit = len_ext(&mut i, (token >> 4) as usize)?;
        let lit_end = i.checked_add(lit).ok_or(Error::Truncated)?;
        out.extend_from_slice(src.get(i..lit_end).ok_or(Error::Truncated)?);
        i = lit_end;
        if out.len() > expected {
            return Err(Error::Format("LZ4 output larger than declared"));
        }
        if i == src.len() {
            break; // last sequence has literals only
        }
        let off = u16::from_le_bytes([*src.get(i).ok_or(Error::Truncated)?, *src.get(i + 1).ok_or(Error::Truncated)?]) as usize;
        i += 2;
        if off == 0 || off > out.len() {
            return Err(Error::Format("LZ4 match offset outside output"));
        }
        let mlen = len_ext(&mut i, (token & 15) as usize)? + 4;
        if out.len() + mlen > expected {
            return Err(Error::Format("LZ4 output larger than declared"));
        }
        let start = out.len() - off;
        for k in 0..mlen {
            out.push(out[start + k]); // overlapping copies are allowed
        }
    }
    if out.len() != expected {
        return Err(Error::Format("LZ4 output size differs from header"));
    }
    Ok(out)
}

/// A tiny LZ4 block encoder (greedy, 4-byte hash) for synthetic test fixtures.
pub fn compress(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table = vec![usize::MAX; 1 << 12];
    let mut anchor = 0;
    let mut i = 0;
    let push_len = |out: &mut Vec<u8>, mut n: usize| {
        while n >= 255 {
            out.push(255);
            n -= 255;
        }
        out.push(n as u8);
    };
    let limit = src.len().saturating_sub(12);
    while i < limit {
        let key = u32::from_le_bytes(src[i..i + 4].try_into().unwrap());
        let h = (key.wrapping_mul(2654435761) >> 20) as usize;
        let cand = table[h];
        table[h] = i;
        if cand != usize::MAX && i - cand <= 0xFFFF && src[cand..cand + 4] == src[i..i + 4] {
            let mut m = 4;
            while i + m < src.len() - 5 && src[cand + m] == src[i + m] {
                m += 1;
            }
            let lit = i - anchor;
            let token_pos = out.len();
            out.push((lit.min(15) as u8) << 4 | (m - 4).min(15) as u8);
            if lit >= 15 {
                push_len(&mut out, lit - 15);
            }
            out.extend_from_slice(&src[anchor..i]);
            out.extend_from_slice(&((i - cand) as u16).to_le_bytes());
            if m - 4 >= 15 {
                push_len(&mut out, m - 4 - 15);
            }
            let _ = token_pos;
            i += m;
            anchor = i;
        } else {
            i += 1;
        }
    }
    let lit = src.len() - anchor;
    out.push((lit.min(15) as u8) << 4);
    if lit >= 15 {
        push_len(&mut out, lit - 15);
    }
    out.extend_from_slice(&src[anchor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_block() {
        // "abcabcabcabcabcabc": 3 literals then a 15-byte match at offset 3.
        let block = [0x3b, b'a', b'b', b'c', 0x03, 0x00];
        assert_eq!(decompress(&block, 18).unwrap(), b"abcabcabcabcabcabc");
    }

    #[test]
    fn round_trip_with_long_runs() {
        let mut data = Vec::new();
        for i in 0..20000u32 {
            data.extend_from_slice(&(i / 37).to_le_bytes());
        }
        data.extend(std::iter::repeat_n(7u8, 1000));
        let c = compress(&data);
        assert!(c.len() < data.len());
        assert_eq!(decompress(&c, data.len()).unwrap(), data);
    }

    #[test]
    fn rejects_bad_offsets_and_sizes() {
        assert!(decompress(&[0x10, b'a', 0x05, 0x00], 10).is_err());
        assert!(decompress(&[0x10, b'a', 0x00, 0x00], 10).is_err());
        assert!(decompress(&[0x3b, b'a', b'b', b'c', 0x03, 0x00], 17).is_err());
        assert!(decompress(&[0xf0], 100).is_err());
    }
}
