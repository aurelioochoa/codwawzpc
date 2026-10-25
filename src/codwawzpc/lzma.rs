/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Native LZMA decoding for the game's archives.
//!
//! Every `.dz` archive is one LZMA stream, which the game decodes with the
//! LZMA SDK's `LzmaDec.c`, statically linked into its executable. Under
//! emulation the SDK's inner loop, `LzmaDec_DecodeReal`, takes over half of
//! a level load. [install] redirects that one function to [decode_real], a
//! port of the same routine (LZMA SDK 9.x, public domain) that works in place
//! on the game's `CLzmaDec` and buffers. The output is identical: decoding is
//! fully determined by the input. The rest of the SDK, which frames the
//! stream and handles its tail, still runs as guest code.

use crate::dyld::HostFunction;
use crate::mem::{GuestUSize, MutPtr};
use crate::Environment;
use std::sync::atomic::{AtomicBool, Ordering};

/// `LzmaDec_DecodeReal(CLzmaDec *p, SizeT limit, const Byte *bufLimit)`, ARM.
const DECODE_REAL: u32 = 0x21d0;
/// Its first instructions in the supported executable:
/// `push {r4-r7, lr}; add r7, sp, #12; push {r8, r10, r11}; sub sp, sp, #0x4c`.
const DECODE_REAL_PROLOGUE: [u32; 4] = [0xe92d40f0, 0xe28d700c, 0xe92d0d00, 0xe24dd04c];
/// `ldr pc, [pc, #-4]`: jump to the address in the next word.
const A32_LDR_PC_NEXT_WORD: u32 = 0xe51ff004;

/// Redirect the game's `LzmaDec_DecodeReal` to [decode_real], once. Called
/// when the game first opens an archive: the loader hashes its own code at
/// startup to decode its data, so the code must not change before then.
pub(crate) fn install(env: &mut Environment) {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    if std::env::var_os("CODWAWZPC_GUEST_LZMA").is_some() {
        log!("Using the game's own LZMA decoder (CODWAWZPC_GUEST_LZMA)");
        return;
    }
    let entry: MutPtr<u32> = MutPtr::from_bits(DECODE_REAL);
    let found: Vec<u32> = (0..4).map(|i| env.mem.read(entry + i)).collect();
    if found != DECODE_REAL_PROLOGUE {
        log!("Not replacing the LZMA decoder: unexpected code {found:x?}");
        return;
    }
    const HOST: HostFunction = &(decode_real as fn(&mut Environment, u32, u32, u32) -> i32);
    let host = env
        .dyld
        .create_guest_function(&mut env.mem, "LzmaDec_DecodeReal", HOST);
    env.mem.write(entry, A32_LDR_PC_NEXT_WORD);
    env.mem.write(entry + 1, host.addr_with_thumb_bit());
    env.cpu.invalidate_cache_range(DECODE_REAL, 8);
    log!("LZMA decoding runs natively");
}

fn decode_real(env: &mut Environment, p: u32, limit: u32, buf_limit: u32) -> i32 {
    let mem = env.mem.bytes_whole_mut();
    decode(mem, p, limit, buf_limit)
}

type Bytes = [u8; 1 << 32];

const SZ_OK: i32 = 0;
const SZ_ERROR_DATA: i32 = 1;

const TOP_VALUE: u32 = 1 << 24;
const BIT_MODEL_TOTAL: u32 = 1 << 11;
const MOVE_BITS: u32 = 5;

const NUM_POS_BITS_MAX: u32 = 4;
const LEN_NUM_LOW_BITS: u32 = 3;
const LEN_NUM_LOW_SYMBOLS: u32 = 1 << LEN_NUM_LOW_BITS;
const LEN_NUM_MID_BITS: u32 = 3;
const LEN_NUM_MID_SYMBOLS: u32 = 1 << LEN_NUM_MID_BITS;
const LEN_NUM_HIGH_BITS: u32 = 8;
const LEN_NUM_HIGH_SYMBOLS: u32 = 1 << LEN_NUM_HIGH_BITS;
const LEN_CHOICE: u32 = 0;
const LEN_CHOICE_2: u32 = LEN_CHOICE + 1;
const LEN_LOW: u32 = LEN_CHOICE_2 + 1;
const LEN_MID: u32 = LEN_LOW + ((1 << NUM_POS_BITS_MAX) << LEN_NUM_LOW_BITS);
const LEN_HIGH: u32 = LEN_MID + ((1 << NUM_POS_BITS_MAX) << LEN_NUM_MID_BITS);
const NUM_LEN_PROBS: u32 = LEN_HIGH + LEN_NUM_HIGH_SYMBOLS;

const NUM_STATES: u32 = 12;
const NUM_LIT_STATES: u32 = 7;
const START_POS_MODEL_INDEX: u32 = 4;
const END_POS_MODEL_INDEX: u32 = 14;
const NUM_FULL_DISTANCES: u32 = 1 << (END_POS_MODEL_INDEX >> 1);
const NUM_POS_SLOT_BITS: u32 = 6;
const NUM_LEN_TO_POS_STATES: u32 = 4;
const NUM_ALIGN_BITS: u32 = 4;
const ALIGN_TABLE_SIZE: u32 = 1 << NUM_ALIGN_BITS;
const MATCH_MIN_LEN: u32 = 2;
const MATCH_SPEC_LEN_START: u32 =
    MATCH_MIN_LEN + LEN_NUM_LOW_SYMBOLS + LEN_NUM_MID_SYMBOLS + LEN_NUM_HIGH_SYMBOLS;

const IS_MATCH: u32 = 0;
const IS_REP: u32 = IS_MATCH + (NUM_STATES << NUM_POS_BITS_MAX);
const IS_REP_G0: u32 = IS_REP + NUM_STATES;
const IS_REP_G1: u32 = IS_REP_G0 + NUM_STATES;
const IS_REP_G2: u32 = IS_REP_G1 + NUM_STATES;
const IS_REP0_LONG: u32 = IS_REP_G2 + NUM_STATES;
const POS_SLOT: u32 = IS_REP0_LONG + (NUM_STATES << NUM_POS_BITS_MAX);
const SPEC_POS: u32 = POS_SLOT + (NUM_LEN_TO_POS_STATES << NUM_POS_SLOT_BITS);
const ALIGN: u32 = SPEC_POS + NUM_FULL_DISTANCES - END_POS_MODEL_INDEX;
const LEN_CODER: u32 = ALIGN + ALIGN_TABLE_SIZE;
const REP_LEN_CODER: u32 = LEN_CODER + NUM_LEN_PROBS;
const LITERAL: u32 = REP_LEN_CODER + NUM_LEN_PROBS;
const LIT_SIZE: u32 = 0x300;

/// Word offsets of `CLzmaDec` fields (LZMA SDK 9.x, 32-bit).
mod field {
    pub const LC: u32 = 0;
    pub const LP: u32 = 1;
    pub const PB: u32 = 2;
    pub const PROBS: u32 = 4;
    pub const DIC: u32 = 5;
    pub const BUF: u32 = 6;
    pub const RANGE: u32 = 7;
    pub const CODE: u32 = 8;
    pub const DIC_POS: u32 = 9;
    pub const DIC_BUF_SIZE: u32 = 10;
    pub const PROCESSED_POS: u32 = 11;
    pub const CHECK_DIC_SIZE: u32 = 12;
    pub const STATE: u32 = 13;
    pub const REPS: u32 = 14;
    pub const REMAIN_LEN: u32 = 18;
}

#[inline(always)]
fn rd8(m: &Bytes, addr: u32) -> u32 {
    m[addr as usize].into()
}
#[inline(always)]
fn wr8(m: &mut Bytes, addr: u32, value: u32) {
    m[addr as usize] = value as u8;
}
#[inline(always)]
fn rd16(m: &Bytes, addr: u32) -> u32 {
    match m.get(addr as usize..addr as usize + 2) {
        Some(b) => u16::from_le_bytes([b[0], b[1]]).into(),
        None => m[addr as usize].into(),
    }
}
#[inline(always)]
fn wr16(m: &mut Bytes, addr: u32, value: u32) {
    match m.get_mut(addr as usize..addr as usize + 2) {
        Some(b) => b.copy_from_slice(&(value as u16).to_le_bytes()),
        None => m[addr as usize] = value as u8,
    }
}
#[inline(always)]
fn rd32(m: &Bytes, addr: u32) -> u32 {
    u32::from_le_bytes(std::array::from_fn(|i| {
        m[addr.wrapping_add(i as u32) as usize]
    }))
}
#[inline(always)]
fn wr32(m: &mut Bytes, addr: u32, value: u32) {
    for (i, b) in value.to_le_bytes().into_iter().enumerate() {
        m[addr.wrapping_add(i as u32) as usize] = b;
    }
}

/// The range decoder and the probabilities it adapts.
struct Rc {
    range: u32,
    code: u32,
    buf: u32,
    probs: u32,
}

impl Rc {
    #[inline(always)]
    fn normalize(&mut self, m: &Bytes) {
        if self.range < TOP_VALUE {
            self.range <<= 8;
            self.code = (self.code << 8) | rd8(m, self.buf);
            self.buf = self.buf.wrapping_add(1);
        }
    }

    /// Decode one bit with the probability at index `i`.
    #[inline(always)]
    fn bit(&mut self, m: &mut Bytes, i: u32) -> u32 {
        let addr = self.probs.wrapping_add(i.wrapping_mul(2));
        let ttt = rd16(m, addr);
        self.normalize(m);
        let bound = (self.range >> 11) * ttt;
        if self.code < bound {
            self.range = bound;
            wr16(m, addr, ttt + ((BIT_MODEL_TOTAL - ttt) >> MOVE_BITS));
            0
        } else {
            self.range -= bound;
            self.code -= bound;
            wr16(m, addr, ttt - (ttt >> MOVE_BITS));
            1
        }
    }

    /// Decode a bit tree of `limit` leaves rooted at index `base`.
    #[inline(always)]
    fn tree(&mut self, m: &mut Bytes, base: u32, limit: u32) -> u32 {
        let mut i = 1;
        while i < limit {
            i = (i << 1) | self.bit(m, base.wrapping_add(i));
        }
        i - limit
    }
}

/// `LzmaDec_DecodeReal`: decode until `dicPos` reaches `limit` or the input
/// reaches `buf_limit`, updating the `CLzmaDec` at `p`.
fn decode(m: &mut Bytes, p: u32, limit: u32, buf_limit: u32) -> i32 {
    let f = |i: u32| p.wrapping_add(i * 4);
    let mut rc = Rc {
        range: rd32(m, f(field::RANGE)),
        code: rd32(m, f(field::CODE)),
        buf: rd32(m, f(field::BUF)),
        probs: rd32(m, f(field::PROBS)),
    };
    let mut state = rd32(m, f(field::STATE));
    let mut rep0 = rd32(m, f(field::REPS));
    let mut rep1 = rd32(m, f(field::REPS + 1));
    let mut rep2 = rd32(m, f(field::REPS + 2));
    let mut rep3 = rd32(m, f(field::REPS + 3));
    let pb_mask = (1u32 << rd32(m, f(field::PB))) - 1;
    let lp_mask = (1u32 << rd32(m, f(field::LP))) - 1;
    let lc = rd32(m, f(field::LC));
    let dic = rd32(m, f(field::DIC));
    let dic_buf_size = rd32(m, f(field::DIC_BUF_SIZE));
    let mut dic_pos = rd32(m, f(field::DIC_POS));
    let mut processed_pos = rd32(m, f(field::PROCESSED_POS));
    let check_dic_size = rd32(m, f(field::CHECK_DIC_SIZE));
    let mut len: u32 = 0;
    // Distance back to rep0 from the current position, wrapping in the
    // dictionary's ring buffer.
    let back = |dic_pos: u32, rep0: u32| {
        dic_pos
            .wrapping_sub(rep0)
            .wrapping_add(if dic_pos < rep0 { dic_buf_size } else { 0 })
    };

    'main: loop {
        let pos_state = processed_pos & pb_mask;
        'symbol: {
            if rc.bit(m, IS_MATCH + (state << NUM_POS_BITS_MAX) + pos_state) == 0 {
                // Literal.
                let mut prob = LITERAL;
                if check_dic_size != 0 || processed_pos != 0 {
                    let prev_pos = if dic_pos == 0 { dic_buf_size } else { dic_pos } - 1;
                    let prev = rd8(m, dic.wrapping_add(prev_pos));
                    prob += LIT_SIZE * (((processed_pos & lp_mask) << lc) + (prev >> (8 - lc)));
                }
                let mut symbol: u32 = 1;
                if state < NUM_LIT_STATES {
                    state -= if state < 4 { state } else { 3 };
                    while symbol < 0x100 {
                        symbol = (symbol << 1) | rc.bit(m, prob + symbol);
                    }
                } else {
                    let mut match_byte = rd8(m, dic.wrapping_add(back(dic_pos, rep0)));
                    let mut offs: u32 = 0x100;
                    state -= if state < 10 { 3 } else { 6 };
                    while symbol < 0x100 {
                        match_byte <<= 1;
                        let bit = match_byte & offs;
                        if rc.bit(m, prob + offs + bit + symbol) == 0 {
                            symbol <<= 1;
                            offs &= !bit;
                        } else {
                            symbol = (symbol << 1) + 1;
                            offs &= bit;
                        }
                    }
                }
                wr8(m, dic.wrapping_add(dic_pos), symbol);
                dic_pos = dic_pos.wrapping_add(1);
                processed_pos = processed_pos.wrapping_add(1);
                break 'symbol;
            }

            let len_coder;
            if rc.bit(m, IS_REP + state) == 0 {
                state += NUM_STATES;
                len_coder = LEN_CODER;
            } else {
                if check_dic_size == 0 && processed_pos == 0 {
                    return SZ_ERROR_DATA;
                }
                if rc.bit(m, IS_REP_G0 + state) == 0 {
                    if rc.bit(m, IS_REP0_LONG + (state << NUM_POS_BITS_MAX) + pos_state) == 0 {
                        // Short rep: one byte from rep0.
                        let byte = rd8(m, dic.wrapping_add(back(dic_pos, rep0)));
                        wr8(m, dic.wrapping_add(dic_pos), byte);
                        dic_pos = dic_pos.wrapping_add(1);
                        processed_pos = processed_pos.wrapping_add(1);
                        state = if state < NUM_LIT_STATES { 9 } else { 11 };
                        break 'symbol;
                    }
                } else {
                    let distance;
                    if rc.bit(m, IS_REP_G1 + state) == 0 {
                        distance = rep1;
                    } else {
                        if rc.bit(m, IS_REP_G2 + state) == 0 {
                            distance = rep2;
                        } else {
                            distance = rep3;
                            rep3 = rep2;
                        }
                        rep2 = rep1;
                    }
                    rep1 = rep0;
                    rep0 = distance;
                }
                state = if state < NUM_LIT_STATES { 8 } else { 11 };
                len_coder = REP_LEN_CODER;
            }

            let (base, limit_bits, offset) = if rc.bit(m, len_coder + LEN_CHOICE) == 0 {
                (
                    len_coder + LEN_LOW + (pos_state << LEN_NUM_LOW_BITS),
                    1 << LEN_NUM_LOW_BITS,
                    0,
                )
            } else if rc.bit(m, len_coder + LEN_CHOICE_2) == 0 {
                (
                    len_coder + LEN_MID + (pos_state << LEN_NUM_MID_BITS),
                    1 << LEN_NUM_MID_BITS,
                    LEN_NUM_LOW_SYMBOLS,
                )
            } else {
                (
                    len_coder + LEN_HIGH,
                    1 << LEN_NUM_HIGH_BITS,
                    LEN_NUM_LOW_SYMBOLS + LEN_NUM_MID_SYMBOLS,
                )
            };
            len = rc.tree(m, base, limit_bits) + offset;

            if state >= NUM_STATES {
                let len_state = len.min(NUM_LEN_TO_POS_STATES - 1);
                let mut distance = rc.tree(
                    m,
                    POS_SLOT + (len_state << NUM_POS_SLOT_BITS),
                    1 << NUM_POS_SLOT_BITS,
                );
                if distance >= START_POS_MODEL_INDEX {
                    let pos_slot = distance;
                    let mut num_direct_bits = (distance >> 1) - 1;
                    distance = 2 | (distance & 1);
                    if pos_slot < END_POS_MODEL_INDEX {
                        distance <<= num_direct_bits;
                        let base = (SPEC_POS + distance).wrapping_sub(pos_slot + 1);
                        let mut mask = 1;
                        let mut i = 1;
                        loop {
                            if rc.bit(m, base.wrapping_add(i)) == 0 {
                                i += i;
                            } else {
                                i = 2 * i + 1;
                                distance |= mask;
                            }
                            mask <<= 1;
                            num_direct_bits -= 1;
                            if num_direct_bits == 0 {
                                break;
                            }
                        }
                    } else {
                        num_direct_bits -= NUM_ALIGN_BITS;
                        loop {
                            rc.normalize(m);
                            rc.range >>= 1;
                            rc.code = rc.code.wrapping_sub(rc.range);
                            let t = 0u32.wrapping_sub(rc.code >> 31);
                            distance = (distance << 1).wrapping_add(t.wrapping_add(1));
                            rc.code = rc.code.wrapping_add(rc.range & t);
                            num_direct_bits -= 1;
                            if num_direct_bits == 0 {
                                break;
                            }
                        }
                        distance <<= NUM_ALIGN_BITS;
                        let mut i = 1;
                        for flag in [1, 2, 4, 8] {
                            if rc.bit(m, ALIGN + i) == 0 {
                                i += i;
                            } else {
                                i = 2 * i + 1;
                                distance |= flag;
                            }
                        }
                        if distance == u32::MAX {
                            // End marker.
                            len += MATCH_SPEC_LEN_START;
                            state -= NUM_STATES;
                            break 'main;
                        }
                    }
                }
                rep3 = rep2;
                rep2 = rep1;
                rep1 = rep0;
                rep0 = distance.wrapping_add(1);
                if check_dic_size == 0 {
                    if distance >= processed_pos {
                        return SZ_ERROR_DATA;
                    }
                } else if distance >= check_dic_size {
                    return SZ_ERROR_DATA;
                }
                state = if state < NUM_STATES + NUM_LIT_STATES {
                    NUM_LIT_STATES
                } else {
                    NUM_LIT_STATES + 3
                };
            }

            len += MATCH_MIN_LEN;
            if limit == dic_pos {
                return SZ_ERROR_DATA;
            }
            let rem = limit.wrapping_sub(dic_pos);
            let cur_len = rem.min(len);
            let mut pos = back(dic_pos, rep0);
            processed_pos = processed_pos.wrapping_add(cur_len);
            len -= cur_len;
            let (src, dest) = (
                dic.wrapping_add(pos) as usize,
                dic.wrapping_add(dic_pos) as usize,
            );
            if pos + cur_len <= dic_buf_size && src.max(dest) + (cur_len as usize) <= m.len() {
                // A source ahead of the destination (wrapped back from the
                // buffer's end) reads only bytes not yet overwritten.
                if src > dest || dest - src >= cur_len as usize {
                    m.copy_within(src..src + cur_len as usize, dest);
                } else {
                    // Overlapping: each byte may repeat one just written.
                    for i in 0..cur_len as usize {
                        m[dest + i] = m[src + i];
                    }
                }
                dic_pos = dic_pos.wrapping_add(cur_len);
            } else {
                for _ in 0..cur_len {
                    let byte = rd8(m, dic.wrapping_add(pos));
                    wr8(m, dic.wrapping_add(dic_pos), byte);
                    dic_pos = dic_pos.wrapping_add(1);
                    pos += 1;
                    if pos == dic_buf_size {
                        pos = 0;
                    }
                }
            }
        }
        if !(dic_pos < limit && rc.buf < buf_limit) {
            break;
        }
    }
    rc.normalize(m);
    wr32(m, f(field::BUF), rc.buf);
    wr32(m, f(field::RANGE), rc.range);
    wr32(m, f(field::CODE), rc.code);
    wr32(m, f(field::REMAIN_LEN), len);
    wr32(m, f(field::DIC_POS), dic_pos);
    wr32(m, f(field::PROCESSED_POS), processed_pos);
    for (i, rep) in [rep0, rep1, rep2, rep3].into_iter().enumerate() {
        wr32(m, f(field::REPS + i as GuestUSize), rep);
    }
    wr32(m, f(field::STATE), state);
    SZ_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a `.lzma` ("LZMA alone") stream the way `LzmaDec_DecodeToDic`
    /// drives `LzmaDec_DecodeReal` when all input is available, with the
    /// decoder state, probabilities, input and output in a guest address
    /// space. `out_len` is the decoded size (the header may leave it unknown).
    fn decode_alone(stream: &[u8], out_len: u32) -> Vec<u8> {
        let props = stream[0] as u32;
        let (lc, lp, pb) = (props % 9, (props / 9) % 5, props / 45);
        let packed = &stream[13..];

        let mut mem = crate::mem::Mem::new();
        let m = mem.bytes_whole_mut();
        let (p, probs, input, dic) = (0x1000u32, 0x2000u32, 0x10_0000u32, 0x100_0000u32);
        let num_probs = LITERAL + (LIT_SIZE << (lc + lp));
        for i in 0..num_probs {
            wr16(m, probs + i * 2, BIT_MODEL_TOTAL >> 1);
        }
        m[input as usize..][..packed.len()].copy_from_slice(packed);
        let code = u32::from_be_bytes(packed[1..5].try_into().unwrap());
        let fields = [
            (field::LC, lc),
            (field::LP, lp),
            (field::PB, pb),
            (field::PROBS, probs),
            (field::DIC, dic),
            (field::BUF, input + 5),
            (field::RANGE, u32::MAX),
            (field::CODE, code),
            (field::DIC_POS, 0),
            (field::DIC_BUF_SIZE, out_len),
            (field::PROCESSED_POS, 0),
            (field::CHECK_DIC_SIZE, 0),
            (field::STATE, 0),
            (field::REPS, 1),
            (field::REPS + 1, 1),
            (field::REPS + 2, 1),
            (field::REPS + 3, 1),
        ];
        for (i, value) in fields {
            wr32(m, p + i * 4, value);
        }
        // The input is followed by zeroed memory, so reading ahead at the end
        // is harmless.
        let buf_limit = input + packed.len() as u32;
        while rd32(m, p + field::DIC_POS * 4) < out_len {
            assert_eq!(decode(m, p, out_len, buf_limit), SZ_OK);
            assert_eq!(rd32(m, p + field::REMAIN_LEN * 4), 0);
        }
        m[dic as usize..][..out_len as usize].to_vec()
    }

    /// Deterministic data with literals, short and long matches and reps.
    fn sample(len: usize) -> Vec<u8> {
        let mut seed: u32 = 12345;
        let mut next = || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            seed >> 16
        };
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let r = next();
            if r % 3 == 0 || out.len() < 64 {
                out.push(next() as u8);
            } else {
                let dist = 1 + (next() as usize % out.len().min(70000));
                let count = 2 + next() as usize % 300;
                for _ in 0..count {
                    out.push(out[out.len() - dist]);
                }
            }
        }
        out.truncate(len);
        out
    }

    #[test]
    fn decodes_reference_stream() {
        // Made by `lzma.compress(sample(300000), format=lzma.FORMAT_ALONE)`
        // in Python (liblzma), with the same sample() generator.
        let stream = include_bytes!("lzma_sample.lzma");
        assert_eq!(decode_alone(stream, 300_000), sample(300_000));
    }
}
