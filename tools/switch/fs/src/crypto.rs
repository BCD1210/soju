//! AES modes used by Switch containers, built on the `aes` block cipher.
//!
//! - ECB (key area and title key unwrapping)
//! - XTS with the Switch's big-endian sector tweak (NCA header, 0x200-byte sectors)
//! - CTR with a 16-byte big-endian counter (NCA sections)

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;

pub fn ecb_decrypt(key: &[u8; 16], data: &mut [u8]) {
    let c = Aes128::new(GenericArray::from_slice(key));
    for block in data.chunks_exact_mut(16) {
        c.decrypt_block(GenericArray::from_mut_slice(block));
    }
}

pub fn ecb_encrypt(key: &[u8; 16], data: &mut [u8]) {
    let c = Aes128::new(GenericArray::from_slice(key));
    for block in data.chunks_exact_mut(16) {
        c.encrypt_block(GenericArray::from_mut_slice(block));
    }
}

/// Multiply the XTS tweak by x in GF(2^128) (IEEE 1619 little-endian convention).
fn gf_double(t: &mut [u8; 16]) {
    let mut carry = 0u8;
    for b in t.iter_mut() {
        let next = *b >> 7;
        *b = (*b << 1) | carry;
        carry = next;
    }
    if carry != 0 {
        t[0] ^= 0x87;
    }
}

fn xts(key: &[u8; 32], data: &mut [u8], sector_size: usize, first_sector: u64, big_endian_tweak: bool, encrypt: bool) {
    let k1 = Aes128::new(GenericArray::from_slice(&key[..16]));
    let k2 = Aes128::new(GenericArray::from_slice(&key[16..]));
    for (i, sector) in data.chunks_mut(sector_size).enumerate() {
        let n = first_sector + i as u64;
        let mut t = [0u8; 16];
        if big_endian_tweak {
            t[8..].copy_from_slice(&n.to_be_bytes());
        } else {
            t[..8].copy_from_slice(&n.to_le_bytes());
        }
        k2.encrypt_block(GenericArray::from_mut_slice(&mut t));
        for block in sector.chunks_exact_mut(16) {
            for (b, x) in block.iter_mut().zip(t.iter()) {
                *b ^= x;
            }
            let g = GenericArray::from_mut_slice(block);
            if encrypt {
                k1.encrypt_block(g);
            } else {
                k1.decrypt_block(g);
            }
            for (b, x) in block.iter_mut().zip(t.iter()) {
                *b ^= x;
            }
            gf_double(&mut t);
        }
    }
}

/// Switch NCA header XTS: sector numbers are written big-endian into the tweak.
pub fn nintendo_xts_decrypt(key: &[u8; 32], data: &mut [u8], sector_size: usize, first_sector: u64) {
    xts(key, data, sector_size, first_sector, true, false)
}

pub fn nintendo_xts_encrypt(key: &[u8; 32], data: &mut [u8], sector_size: usize, first_sector: u64) {
    xts(key, data, sector_size, first_sector, true, true)
}

/// AES-CTR where `iv` is the counter for byte offset `offset` (must be 16-aligned):
/// the low 8 bytes hold (offset / 16) big-endian. Encryption and decryption are the same.
pub fn ctr_apply(key: &[u8; 16], upper: [u8; 8], offset: u64, data: &mut [u8]) {
    assert_eq!(offset % 16, 0, "CTR offset must be 16-byte aligned");
    ctr_apply_blocks(key, upper, offset / 16, data)
}

/// Same as `ctr_apply`, starting from counter block `block_index`.
pub fn ctr_apply_blocks(key: &[u8; 16], upper: [u8; 8], mut block_index: u64, data: &mut [u8]) {
    let c = Aes128::new(GenericArray::from_slice(key));
    for chunk in data.chunks_mut(16) {
        let mut ks = [0u8; 16];
        ks[..8].copy_from_slice(&upper);
        ks[8..].copy_from_slice(&block_index.to_be_bytes());
        c.encrypt_block(GenericArray::from_mut_slice(&mut ks));
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
        block_index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        crate::keys::unhex(s).unwrap()
    }

    #[test]
    fn xts_ieee1619_vector_1() {
        // IEEE 1619-2007 vector 1: zero keys, zero data, sector 0 (both tweak byte orders agree).
        let mut d = [0u8; 32];
        xts(&[0u8; 32], &mut d, 32, 0, false, true);
        assert_eq!(d.to_vec(), h("917cf69ebd68b2ec9b9fe9a3eadda692cd43d2f59598ed858c02c2652fbf922e"));
        let mut e = [0u8; 32];
        nintendo_xts_encrypt(&[0u8; 32], &mut e, 32, 0);
        assert_eq!(d, e);
    }

    #[test]
    fn xts_ieee1619_vector_2() {
        // Vector 2: key1=11.., key2=22.., data unit 0x3333333333, plaintext 44.. (32 bytes).
        let mut key = [0x11u8; 32];
        key[16..].fill(0x22);
        let mut d = [0x44u8; 32];
        xts(&key, &mut d, 32, 0x3333333333, false, true);
        assert_eq!(d.to_vec(), h("c454185e6a16936e39334038acef838bfb186fff7480adc4289382ecd6d394f0"));
    }

    #[test]
    fn nintendo_xts_round_trip_across_sectors() {
        let key = [7u8; 32];
        let plain: Vec<u8> = (0..0xC00u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut d = plain.clone();
        nintendo_xts_encrypt(&key, &mut d, 0x200, 0);
        assert_ne!(d, plain);
        // A different sector number gives different ciphertext for the same data.
        let mut other = plain[..0x200].to_vec();
        nintendo_xts_encrypt(&key, &mut other, 0x200, 1);
        assert_ne!(other, d[..0x200].to_vec());
        nintendo_xts_decrypt(&key, &mut d, 0x200, 0);
        assert_eq!(d, plain);
    }

    #[test]
    fn ctr_nist_sp800_38a_f51() {
        // NIST SP 800-38A F.5.1 CTR-AES128: counter f0f1..fe ff, first block.
        let key: [u8; 16] = h("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let ctr = h("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
        let upper: [u8; 8] = ctr[..8].try_into().unwrap();
        let lower = u64::from_be_bytes(ctr[8..].try_into().unwrap());
        let mut d = h("6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51");
        ctr_apply_blocks(&key, upper, lower, &mut d);
        assert_eq!(d, h("874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff"));
    }
}
