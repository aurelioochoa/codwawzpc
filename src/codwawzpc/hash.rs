/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Streaming SHA-256 (FIPS 180-4). Kept local to avoid changing upstream locks.
use std::io::{self, Read};

pub fn sha256(mut input: impl Read) -> io::Result<String> {
    // Hash whole blocks straight from a large buffer: an installation check
    // shouldn't issue one host read call per 64-byte block.
    let compress = compressor();
    let mut state = INITIAL;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut used = 0;
    let mut length = 0u64;
    loop {
        let n = match input.read(&mut buffer[used..]) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            break;
        }
        used += n;
        length = length
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("input too long"))?;
        let whole = used / 64 * 64;
        for block in buffer[..whole].chunks_exact(64) {
            compress(&mut state, block.try_into().unwrap());
        }
        buffer.copy_within(whole..used, 0);
        used -= whole;
    }
    let mut block = [0u8; 64];
    block[..used].copy_from_slice(&buffer[..used]);
    block[used] = 0x80;
    if used >= 56 {
        compress(&mut state, &block);
        block.fill(0);
    }
    block[56..].copy_from_slice(&length.wrapping_mul(8).to_be_bytes());
    compress(&mut state, &block);
    Ok(state.iter().map(|x| format!("{x:08x}")).collect())
}

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

type Compress = fn(&mut [u32; 8], &[u8; 64]);

/// The x86 SHA extensions are several times faster than the portable rounds.
/// Both produce identical digests; the tests compare them.
fn compressor() -> Compress {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("sha")
        && is_x86_feature_detected!("ssse3")
        && is_x86_feature_detected!("sse4.1")
    {
        return |state, block| unsafe { x86::compress(state, block) };
    }
    compress
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::K;
    use std::arch::x86_64::*;

    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    pub(super) unsafe fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let order = _mm_set_epi64x(0x0c0d0e0f08090a0b, 0x0405060700010203);
        // The instructions take the state as ABEF and CDGH lane groups.
        let dcba = _mm_loadu_si128(state.as_ptr().cast());
        let hgfe = _mm_loadu_si128(state.as_ptr().add(4).cast());
        let cdab = _mm_shuffle_epi32(dcba, 0xb1);
        let efgh = _mm_shuffle_epi32(hgfe, 0x1b);
        let mut abef = _mm_alignr_epi8(cdab, efgh, 8);
        let mut cdgh = _mm_blend_epi16(efgh, cdab, 0xf0);
        let (abef_start, cdgh_start) = (abef, cdgh);
        // Four schedule words per group; group i + 4 replaces group i.
        let mut w = [_mm_setzero_si128(); 4];
        for (i, group) in w.iter_mut().enumerate() {
            *group = _mm_shuffle_epi8(_mm_loadu_si128(block.as_ptr().add(i * 16).cast()), order);
        }
        for i in 0..16 {
            let words = _mm_add_epi32(w[i % 4], _mm_loadu_si128(K.as_ptr().add(i * 4).cast()));
            cdgh = _mm_sha256rnds2_epu32(cdgh, abef, words);
            abef = _mm_sha256rnds2_epu32(abef, cdgh, _mm_shuffle_epi32(words, 0x0e));
            if i < 12 {
                let next = _mm_add_epi32(
                    _mm_sha256msg1_epu32(w[i % 4], w[(i + 1) % 4]),
                    _mm_alignr_epi8(w[(i + 3) % 4], w[(i + 2) % 4], 4),
                );
                w[i % 4] = _mm_sha256msg2_epu32(next, w[(i + 3) % 4]);
            }
        }
        abef = _mm_add_epi32(abef, abef_start);
        cdgh = _mm_add_epi32(cdgh, cdgh_start);
        let feba = _mm_shuffle_epi32(abef, 0x1b);
        let dchg = _mm_shuffle_epi32(cdgh, 0xb1);
        _mm_storeu_si128(state.as_mut_ptr().cast(), _mm_blend_epi16(feba, dchg, 0xf0));
        _mm_storeu_si128(
            state.as_mut_ptr().add(4).cast(),
            _mm_alignr_epi8(dchg, feba, 8),
        );
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, bytes) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes(bytes.try_into().unwrap());
    }
    for i in 16..64 {
        let x = w[i - 15];
        let y = w[i - 2];
        let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nist_vectors() {
        assert_eq!(
            sha256(&b""[..]).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(&b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(&vec![b'a'; 1_000_000][..]).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            sha256(&b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..]).unwrap(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// Reads return few bytes at a time, so blocks straddle read boundaries.
    struct Trickle<'a>(&'a [u8], usize);
    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.0.len().min(out.len()).min(self.1);
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            self.1 = self.1 % 97 + 1;
            Ok(n)
        }
    }

    #[test]
    fn accelerated_rounds_match_portable_rounds() {
        let mut seed = 0x2545f491u32;
        let data: Vec<u8> = (0..300_000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        for block in data.chunks_exact(64).take(2000) {
            let block: &[u8; 64] = block.try_into().unwrap();
            let (mut portable, mut chosen) = ([seed; 8], [seed; 8]);
            for (i, (a, b)) in portable.iter_mut().zip(chosen.iter_mut()).enumerate() {
                *a ^= INITIAL[i];
                *b ^= INITIAL[i];
            }
            compress(&mut portable, block);
            compressor()(&mut chosen, block);
            assert_eq!(portable, chosen);
        }
        for length in [
            0, 1, 55, 56, 63, 64, 65, 119, 120, 128, 65_535, 65_536, 65_600, 300_000,
        ] {
            assert_eq!(
                sha256(&data[..length]).unwrap(),
                sha256(Trickle(&data[..length], 1)).unwrap()
            );
        }
        assert_eq!(
            sha256(&data[..]).unwrap(),
            "cf57206a1caaa8bc5a8d6ac9d4b9d0e3ad5a855d143fe25225af1ca233fc5824"
        );
    }
}
