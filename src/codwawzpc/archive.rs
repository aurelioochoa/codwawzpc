/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's archives, decompressed on the host.
//!
//! The game's data is in Marmalade `.dz` (DTRZ) archives: a header listing
//! the files, then one LZMA-compressed chunk per file. The game reads them
//! through Marmalade's derbh, which hands each chunk to a codec: `open`
//! positions it at a chunk, then each `read` returns the next block of
//! decompressed data. The game's LZMA codec decodes 1 KB blocks through the
//! s3eCompression API, and derbh reopens the chunk, decoding it again from
//! the start, whenever a file seeks backwards. A level load decodes nearly
//! three times its data in about 300,000 such reads.
//!
//! [install] replaces that codec's `open` and `read`. Chunks are decoded on
//! the host once per session, on other threads from when the game opens
//! their archive (it opens every level's at startup), and `read` returns the
//! whole chunk as one block in guest memory. derbh copies file data out of blocks
//! itself, so seeking within a chunk no longer reopens it.
//!
//! The patch is applied when the game first opens an archive, after its
//! loader has hashed its code. `CODWAWZPC_GUEST_LZMA` turns it off.

use crate::dyld::HostFunction;
use crate::mem::{GuestUSize, MutPtr, MutVoidPtr, Ptr};
use crate::Environment;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, OnceLock};

/// The codec's `open(codec, chunk)` and `read(codec, &block, &size)`, Thumb.
const CODEC_OPEN: u32 = 0x223460;
const CODEC_READ: u32 = 0x223548;
/// Their first instructions in the supported executable:
/// `push {r3-r7, lr}; mov r4, r0; mov r5, r1; bl …` and
/// `push {r4-r6, lr}; mov r4, r0; mov r6, r1; ldr r0, [r0, #0x1c]`.
const CODEC_OPEN_PROLOGUE: [u16; 4] = [0xb5f8, 0x4604, 0x460d, 0xf000];
const CODEC_READ_PROLOGUE: [u16; 4] = [0xb570, 0x4604, 0x460e, 0x69c0];
/// Thumb `bx pc; nop`, then A32 `ldr pc, [pc, #-4]`: jump to the next word.
const THUMB_TO_A32: [u16; 2] = [0x4778, 0x46c0];
const A32_LDR_PC_NEXT_WORD: u32 = 0xe51ff004;

/// Codec fields: the derbh archive and the bytes left in the chunk.
const CODEC_ARCHIVE: u32 = 0x10;
const CODEC_REMAINING: u32 = 0x1c;
/// derbh archive fields: the chunk count (`u16`) and the chunk table, as in
/// the file.
const ARCHIVE_CHUNK_COUNT: u32 = 0x2;
const ARCHIVE_CHUNKS: u32 = 0x8;

/// DTRZ chunk flags.
const CHUNK_LZMA: u16 = 0x200;
/// Size of a chunk table entry: offset, compressed size, decompressed size
/// (`u32` each), flags and volume (`u16` each).
const CHUNK_ENTRY: u32 = 16;

fn disabled() -> bool {
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| std::env::var_os("CODWAWZPC_GUEST_LZMA").is_some())
}

struct Archive {
    name: String,
    compressed: Box<[u8]>,
    /// The chunk table, which identifies the archive to [codec_open].
    table: std::ops::Range<usize>,
    /// By offset in the archive.
    chunks: HashMap<u32, Chunk>,
    background_started: AtomicBool,
}

struct Chunk {
    len: usize,
    /// Byte range of the LZMA stream in the archive.
    packed: std::ops::Range<usize>,
    data: OnceLock<Box<[u8]>>,
}

impl Archive {
    /// Parse an archive whose chunks are all LZMA in one volume, which is
    /// how all of the game's archives are made. Anything else is [None].
    fn parse(name: &str, compressed: Box<[u8]>) -> Option<Archive> {
        let b = &compressed[..];
        let u16_at = |o: usize| Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?));
        let u32_at = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
        if b.get(..4)? != b"DTRZ" {
            return None;
        }
        let (files, dirs) = (u16_at(4)? as usize, u16_at(6)? as usize);
        let mut o = 9;
        // File names, then directory names (the first directory is the root
        // and has none).
        for _ in 0..(files + dirs.checked_sub(1)?) {
            o += b.get(o..)?.iter().position(|&c| c == 0)? + 1;
        }
        // Each file's directory and chunk list, ended by 0xffff.
        for _ in 0..files {
            o += 2;
            while u16_at(o)? != 0xffff {
                o += 2;
            }
            o += 2;
        }
        let (volumes, count) = (u16_at(o)?, u16_at(o + 2)? as usize);
        if volumes != 1 {
            return None;
        }
        let table = o + 4;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let e = table + i * CHUNK_ENTRY as usize;
            let (offset, len, flags) = (u32_at(e)?, u32_at(e + 8)? as usize, u16_at(e + 12)?);
            if flags != CHUNK_LZMA || offset as usize > b.len() {
                return None;
            }
            entries.push((offset, len));
        }
        let chunks = entries
            .iter()
            .map(|&(offset, len)| {
                // Each stream runs to the next chunk's, or to the end.
                let end = entries
                    .iter()
                    .map(|&(o, _)| o as usize)
                    .filter(|&o| o > offset as usize)
                    .min()
                    .unwrap_or(b.len());
                let chunk = Chunk {
                    len,
                    packed: offset as usize..end,
                    data: OnceLock::new(),
                };
                (offset, chunk)
            })
            .collect();
        Some(Archive {
            name: name.to_owned(),
            table: table..table + count * CHUNK_ENTRY as usize,
            compressed,
            chunks,
            background_started: AtomicBool::new(false),
        })
    }

    fn chunk_data<'a>(&self, chunk: &'a Chunk) -> &'a [u8] {
        chunk.data.get_or_init(|| {
            let mut out = Vec::with_capacity(chunk.len);
            let options = lzma_rs::decompress::Options {
                unpacked_size: lzma_rs::decompress::UnpackedSize::ReadHeaderButUseProvided(Some(
                    chunk.len as u64,
                )),
                ..Default::default()
            };
            let mut input = &self.compressed[chunk.packed.clone()];
            if let Err(e) = lzma_rs::lzma_decompress_with_options(&mut input, &mut out, &options) {
                log!(
                    "{}: chunk at {:#x} does not decode: {e:?}",
                    self.name,
                    chunk.packed.start
                );
            }
            out.resize(chunk.len, 0);
            out.into_boxed_slice()
        })
    }

    /// Queue the archive's chunks, largest first, for the decoder threads.
    /// A chunk the game asks for meanwhile is decoded by whichever thread
    /// gets to it first; the other waits for it.
    fn decode_in_background(self: &Arc<Self>) {
        if self.background_started.swap(true, Ordering::Relaxed) {
            return;
        }
        let mut order: Vec<u32> = self.chunks.keys().copied().collect();
        order.sort_by_key(|offset| std::cmp::Reverse(self.chunks[offset].len));
        let (queue, ready) = &*DECODER_QUEUE;
        queue
            .lock()
            .unwrap()
            .extend(order.into_iter().map(|offset| (self.clone(), offset)));
        ready.notify_all();
        static STARTED: std::sync::Once = std::sync::Once::new();
        STARTED.call_once(|| {
            let threads = std::thread::available_parallelism()
                .map_or(2, |n| n.get().saturating_sub(2).clamp(1, 4));
            for _ in 0..threads {
                let _ = std::thread::Builder::new()
                    .name("archive decoder".into())
                    .spawn(decoder_thread);
            }
        });
    }
}

type DecoderQueue = (Mutex<VecDeque<(Arc<Archive>, u32)>>, Condvar);
static DECODER_QUEUE: LazyLock<DecoderQueue> = LazyLock::new(Default::default);

fn decoder_thread() {
    let (queue, ready) = &*DECODER_QUEUE;
    loop {
        let (archive, offset) = {
            let mut queue = queue.lock().unwrap();
            loop {
                match queue.pop_front() {
                    Some(next) => break next,
                    None => queue = ready.wait(queue).unwrap(),
                }
            }
        };
        archive.chunk_data(&archive.chunks[&offset]);
    }
}

#[derive(Default)]
struct State {
    /// Parsed archives by guest path.
    archives: HashMap<String, Option<Arc<Archive>>>,
    /// What each codec (by guest address) is positioned at.
    codecs: HashMap<u32, CodecState>,
}

struct CodecState {
    /// The archive's name and the chunk's offset in it.
    chunk: (String, u32),
    /// The chunk's data in guest memory.
    block: MutVoidPtr,
    len: GuestUSize,
    delivered: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    f(STATE.lock().unwrap().get_or_insert_with(State::default))
}

/// The game opened the archive at `path`; `file` is at its start and is left
/// there. Called for every `.dz` file opened.
pub(crate) fn opened(path: &str, file: &mut (impl Read + Seek)) {
    if disabled() {
        return;
    }
    with_state(|state| {
        state.archives.entry(path.to_owned()).or_insert_with(|| {
            let name = path.rsplit('/').next().unwrap_or(path);
            let mut compressed = Vec::new();
            let read = file.read_to_end(&mut compressed);
            let _ = file.seek(SeekFrom::Start(0));
            let archive = read
                .ok()
                .and_then(|_| Archive::parse(name, compressed.into_boxed_slice()));
            match &archive {
                Some(a) => {
                    log_dbg!("{name}: {} chunks decoded on the host", a.chunks.len());
                }
                None => {
                    log!("{name}: not a supported archive");
                }
            }
            let archive = archive.map(Arc::new);
            if let Some(archive) = &archive {
                archive.decode_in_background();
            }
            archive
        });
    });
}

/// Redirect derbh's LZMA codec to [codec_open] and [codec_read], once.
/// Returns whether it is redirected.
pub(crate) fn install(env: &mut Environment) -> bool {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    if disabled() {
        return false;
    }
    if let Some(&installed) = INSTALLED.get() {
        return installed;
    }
    let found = |env: &Environment, at: u32| -> Vec<u16> {
        (0..4)
            .map(|i| env.mem.read(Ptr::<u16, false>::from_bits(at + i * 2)))
            .collect()
    };
    if found(env, CODEC_OPEN) != CODEC_OPEN_PROLOGUE
        || found(env, CODEC_READ) != CODEC_READ_PROLOGUE
    {
        log!("Not replacing the archive codec: unexpected code");
        return *INSTALLED.get_or_init(|| false);
    }
    const OPEN: HostFunction = &(codec_open as fn(&mut Environment, u32, u32) -> u32);
    const READ: HostFunction = &(codec_read as fn(&mut Environment, u32, u32, u32) -> u32);
    for (at, name, host) in [
        (CODEC_OPEN, "derbh_codec_open", OPEN),
        (CODEC_READ, "derbh_codec_read", READ),
    ] {
        let host = env.dyld.create_guest_function(&mut env.mem, name, host);
        env.mem.write(MutPtr::from_bits(at), THUMB_TO_A32[0]);
        env.mem.write(MutPtr::from_bits(at + 2), THUMB_TO_A32[1]);
        env.mem
            .write(MutPtr::from_bits(at + 4), A32_LDR_PC_NEXT_WORD);
        env.mem
            .write(MutPtr::from_bits(at + 8), host.addr_with_thumb_bit());
        env.cpu.invalidate_cache_range(at, 12);
    }
    log!("Archives are decoded on the host");
    *INSTALLED.get_or_init(|| true)
}

fn read_u32(env: &Environment, at: u32) -> u32 {
    let bytes = env.mem.bytes_at(Ptr::<u8, false>::from_bits(at), 4);
    u32::from_le_bytes(bytes.try_into().unwrap())
}

fn read_u16(env: &Environment, at: u32) -> u16 {
    let bytes = env.mem.bytes_at(Ptr::<u8, false>::from_bits(at), 2);
    u16::from_le_bytes(bytes.try_into().unwrap())
}

/// `open(codec, chunk)`: 0, or 2 if the chunk cannot be read.
fn codec_open(env: &mut Environment, codec: u32, chunk: u32) -> u32 {
    let archive = read_u32(env, codec + CODEC_ARCHIVE);
    let count = u32::from(read_u16(env, archive + ARCHIVE_CHUNK_COUNT));
    let table = read_u32(env, archive + ARCHIVE_CHUNKS);
    let entry = table + chunk * CHUNK_ENTRY;
    let (offset, len) = (read_u32(env, entry), read_u32(env, entry + 8));
    let table = env
        .mem
        .bytes_at(Ptr::<u8, false>::from_bits(table), count * CHUNK_ENTRY);
    let found = with_state(|state| {
        state
            .archives
            .values()
            .flatten()
            .find(|a| a.compressed[a.table.clone()] == *table)
            .cloned()
    });
    let Some(archive) = found else {
        log!("Archive codec: chunk {chunk} is not in a known archive");
        return 2;
    };
    let Some(data) = archive
        .chunks
        .get(&offset)
        .filter(|c| c.len == len as usize)
        .map(|c| archive.chunk_data(c))
    else {
        log!("{}: no chunk at {offset:#x}", archive.name);
        return 2;
    };
    let key = (archive.name.clone(), offset);
    let state = match with_state(|state| state.codecs.remove(&codec)) {
        Some(mut same) if same.chunk == key => {
            log_dbg!(
                "{}: chunk {offset:#x} already in guest memory",
                archive.name
            );
            same.delivered = false;
            same
        }
        previous => {
            log_dbg!(
                "{}: chunk {offset:#x} copied to guest memory ({len} bytes)",
                archive.name
            );
            if let Some(previous) = previous {
                env.mem.free(previous.block);
            }
            let block = env.mem.alloc(len.max(1));
            env.mem
                .bytes_at_mut(block.cast(), len)
                .copy_from_slice(data);
            CodecState {
                chunk: key,
                block,
                len,
                delivered: false,
            }
        }
    };
    with_state(|s| s.codecs.insert(codec, state));
    env.mem
        .write(MutPtr::<u32>::from_bits(codec + CODEC_REMAINING), len);
    0
}

/// `read(codec, &block, &size)`: the whole chunk the first time, then an
/// empty block. Returns 0 (success).
fn codec_read(env: &mut Environment, codec: u32, block_out: u32, size_out: u32) -> u32 {
    let next = with_state(|state| {
        let s = state.codecs.get_mut(&codec)?;
        let next = (!s.delivered).then_some((s.block, s.len));
        s.delivered = true;
        next
    });
    let (block, len) = next.unwrap_or((MutVoidPtr::null(), 0));
    env.mem.write(MutPtr::<u32>::from_bits(size_out), len);
    if len != 0 {
        env.mem
            .write(MutPtr::<u32>::from_bits(block_out), block.to_bits());
        env.mem
            .write(MutPtr::<u32>::from_bits(codec + CODEC_REMAINING), 0);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-file archive with an LZMA chunk, built like the game's, and the
    /// offset of its chunk table entry.
    fn sample(data: &[u8]) -> (Vec<u8>, usize) {
        let mut packed = Vec::new();
        lzma_rs::lzma_compress(&mut &data[..], &mut packed).unwrap();
        let mut b = b"DTRZ".to_vec();
        b.extend_from_slice(&1u16.to_le_bytes()); // files
        b.extend_from_slice(&1u16.to_le_bytes()); // directories
        b.push(0); // version
        b.extend_from_slice(b"a.bin\0");
        b.extend_from_slice(&0u16.to_le_bytes()); // directory
        b.extend_from_slice(&0u16.to_le_bytes()); // chunk 0
        b.extend_from_slice(&0xffffu16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // volumes
        b.extend_from_slice(&1u16.to_le_bytes()); // chunks
        let entry = b.len();
        let offset = entry + CHUNK_ENTRY as usize;
        b.extend_from_slice(&(offset as u32).to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&CHUNK_LZMA.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&packed);
        (b, entry)
    }

    #[test]
    fn decodes_chunks() {
        let data: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let (bytes, entry) = sample(&data);
        let archive = Archive::parse("a.dz", bytes.into_boxed_slice()).unwrap();
        let chunk = &archive.chunks[&(entry as u32 + CHUNK_ENTRY)];
        assert_eq!(archive.chunk_data(chunk), &data[..]);
    }

    #[test]
    fn rejects_other_archives() {
        assert!(Archive::parse("a.dz", b"not an archive".to_vec().into()).is_none());
        let (mut bytes, entry) = sample(b"data");
        bytes[entry + 12..entry + 14].copy_from_slice(&8u16.to_le_bytes()); // zlib
        assert!(Archive::parse("a.dz", bytes.into_boxed_slice()).is_none());
    }
}
