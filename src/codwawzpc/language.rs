/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's languages, including three it never shipped.
//!
//! `_main.dz` has a string table for each of the game's six languages:
//! `english_us`, `english_uk`, `french`, `italian`, `german` and `spanish`
//! (ids 0 to 5, in the order of the game's name table). Only English and
//! French were translated: the Italian, German and Spanish tables are the
//! UK English one with its first string, "Boilerplate UK", changed to "IT",
//! "DE" or "ES". `CLocalise::SetLanguage` refuses ids above 2 on iPhone, so
//! the game never loads them, although it already maps the device language
//! to all six and its per-language menu variants are looked up by these
//! names.
//!
//! [install] makes the three usable, with this project's translations
//! (`localisation/*.txt` beside this file):
//! - SetLanguage accepts ids 3 to 5.
//! - The string table loader is replaced. For Italian, German and Spanish the
//!   host copies its translation into the game's table, growing the game's
//!   string buffer when the translation is longer than the 47,494 bytes the
//!   game allocates; any other table is loaded by the game's own loader.
//! - Pictures with text in them are `CGuiFrameLocalised` frames, which hold
//!   one image per translated language (`english_us`, `english_uk`, `french`)
//!   and show the one named after the current language. For the new languages
//!   they show the English picture.
//! - The `Language` setting (`--language`) replaces the language in the saved
//!   game's options the first time the frontend starts, which is where the
//!   game applies its saved language. The in-game Language screen still lists
//!   only English and French, and choosing one there lasts until the next
//!   launch with a `Language` setting.

use super::native::{peek, poke};
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::HostFunction;
use crate::mem::{MutPtr, Ptr};
use crate::Environment;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The `Language` setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// The game's own choice: its saved language, or the device language
    /// (English) for a new save.
    #[default]
    Game,
    English,
    French,
    German,
    Italian,
    Spanish,
}

impl Language {
    pub const NAMES: [(&'static str, Language); 6] = [
        ("game", Language::Game),
        ("english", Language::English),
        ("french", Language::French),
        ("german", Language::German),
        ("italian", Language::Italian),
        ("spanish", Language::Spanish),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES.iter().find(|(_, v)| *v == self).unwrap().0
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, v)| v)
    }

    /// The game's language id. English is the game's own English, UK.
    fn id(self) -> Option<u32> {
        match self {
            Language::Game => None,
            Language::English => Some(1),
            Language::French => Some(2),
            Language::Italian => Some(3),
            Language::German => Some(4),
            Language::Spanish => Some(5),
        }
    }
}

/// The game's language names by id, as in its name table.
const NAMES: [&str; 6] = [
    "english_us",
    "english_uk",
    "french",
    "italian",
    "german",
    "spanish",
];
/// The id whose `CGuiFrameLocalised` variants the new languages show.
const VARIANT_FALLBACK: usize = 1;

/// This project's translations, by name. See `localisation/README.md`.
const TRANSLATIONS: [(&str, &str); 3] = [
    ("italian", include_str!("localisation/italian.txt")),
    ("german", include_str!("localisation/german.txt")),
    ("spanish", include_str!("localisation/spanish.txt")),
];

/// `CLocalise::SetLanguage(this, id)`, A32. Its iPhone check
/// `cmp r5, #0; cmpne r5, #1; cmpne r5, #2; bne refuse` ends at
/// [SET_LANGUAGE_REFUSE], which becomes a `nop`; ids above 5 are still
/// refused before it.
const SET_LANGUAGE: u32 = 0x222d2c;
const SET_LANGUAGE_REFUSE: u32 = 0x222d80;
const BNE_REFUSE: u32 = 0x1a000001;
const A32_NOP: u32 = 0xe1a00000;
/// The string table loader, `load(path)`: an A32 veneer
/// (`adr r12, …; bx r12`) to the Thumb function after it, which reads the
/// file into [STRINGS].
const LOAD_STRINGS: u32 = 0xd6e34;
const LOAD_STRINGS_VENEER: [u32; 2] = [0xe28fc001, 0xe12fff1c];
const LOAD_STRINGS_THUMB: u32 = 0xd6e3d;
/// The literal that the name table getter used by `CGuiFrameLocalised`
/// (`ldr r0, [pc, #0]; bx lr` at 0x222e54) returns. It is the only user of
/// that getter; file names and the name-to-id lookup read the table
/// directly.
const VARIANT_TABLE_LITERAL: u32 = 0x222e5c;
const NAME_TABLE: u32 = 0xa16720;
/// "Apply the saved language" (`SetLanguage(localise, options->language)`),
/// A32, called when the frontend state starts.
const APPLY_SAVED_LANGUAGE: u32 = 0x14b5f8;
const APPLY_SAVED_LANGUAGE_CODE: [u32; 2] = [0xe59f0010, 0xe5900000];
/// `ldr pc, [pc, #-4]`: jump to the next word.
const A32_LDR_PC_NEXT_WORD: u32 = 0xe51ff004;

/// `CLocalise *`.
const LOCALISE: u32 = 0xa1a5c0;
/// The saved game's options and their language id.
const OPTIONS: u32 = 0xa1a558;
const OPTIONS_LANGUAGE: u32 = 0x9c;
/// The string table: string count, string buffer capacity, the hash list
/// and the matching string indices (`u16`) from `LocaliseHashes.dat`, then
/// the string buffer and the offset of each string in it.
const STRINGS: u32 = 0xa17d28;
const STRINGS_COUNT: u32 = 0x0;
const STRINGS_CAPACITY: u32 = 0x4;
const STRINGS_HASHES: u32 = 0x8;
const STRINGS_INDICES: u32 = 0xc;
const STRINGS_BUFFER: u32 = 0x14;
const STRINGS_OFFSETS: u32 = 0x18;
/// The game's string count.
const STRING_COUNT: usize = 732;

/// A translation: each string's id (from `LocaliseHashes.dat`) and text, in
/// the game's string order.
struct Translation {
    hashes: Vec<u32>,
    /// The strings, each followed by a NUL.
    data: Vec<u8>,
    offsets: Vec<u32>,
}

/// Parse a translation file: comment lines starting with `#`, then one line
/// per string, `<id in hex>\t<text>`, where `\n` is a line break and `\\` a
/// backslash.
fn parse(text: &str) -> Result<Translation, String> {
    let mut t = Translation {
        hashes: Vec::new(),
        data: Vec::new(),
        offsets: Vec::new(),
    };
    for (number, line) in text.lines().enumerate() {
        let line_error = |what: &str| format!("line {}: {what}", number + 1);
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (hash, value) = line.split_once('\t').ok_or_else(|| line_error("no tab"))?;
        let hash = u32::from_str_radix(hash, 16).map_err(|_| line_error("bad id"))?;
        let mut unescaped = String::with_capacity(value.len());
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                unescaped.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => unescaped.push('\n'),
                Some('\\') => unescaped.push('\\'),
                _ => return Err(line_error("bad escape")),
            }
        }
        if unescaped.contains('\0') {
            return Err(line_error("NUL"));
        }
        t.hashes.push(hash);
        t.offsets.push(t.data.len() as u32);
        t.data.extend_from_slice(unescaped.as_bytes());
        t.data.push(0);
    }
    if t.hashes.len() != STRING_COUNT {
        return Err(format!(
            "{} strings instead of {STRING_COUNT}",
            t.hashes.len()
        ));
    }
    Ok(t)
}

fn translation(name: &str) -> Option<&'static Translation> {
    static PARSED: OnceLock<Vec<(&'static str, Option<Translation>)>> = OnceLock::new();
    let parsed = PARSED.get_or_init(|| {
        TRANSLATIONS
            .iter()
            .map(|&(name, text)| {
                let t = parse(text);
                if let Err(e) = &t {
                    log!("The {name} translation is invalid: {e}");
                }
                (name, t.ok())
            })
            .collect()
    });
    parsed
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, t)| t.as_ref())
}

/// Guest copies of [NAMES] for [VARIANT_TABLE_LITERAL]: the table, then
/// each name, then an empty name.
static VARIANT_TABLE: OnceLock<(u32, [u32; 6], u32)> = OnceLock::new();
/// The `Language` setting has been applied (or there was none).
static SETTING_APPLIED: AtomicBool = AtomicBool::new(false);

fn read_u32(env: &Environment, at: u32) -> u32 {
    env.mem.read(Ptr::<u32, false>::from_bits(at))
}

/// Patch the game, once. Called when the game opens an archive, after its
/// loader has hashed its code.
pub(crate) fn install(env: &mut Environment) {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let veneer = [read_u32(env, LOAD_STRINGS), read_u32(env, LOAD_STRINGS + 4)];
        let apply = [
            read_u32(env, APPLY_SAVED_LANGUAGE),
            read_u32(env, APPLY_SAVED_LANGUAGE + 4),
        ];
        if read_u32(env, SET_LANGUAGE_REFUSE) != BNE_REFUSE
            || veneer != LOAD_STRINGS_VENEER
            || read_u32(env, VARIANT_TABLE_LITERAL) != NAME_TABLE
            || apply != APPLY_SAVED_LANGUAGE_CODE
        {
            log!("Not adding languages: unexpected code");
            return;
        }
        env.mem
            .write(MutPtr::<u32>::from_bits(SET_LANGUAGE_REFUSE), A32_NOP);
        env.cpu.invalidate_cache_range(SET_LANGUAGE_REFUSE, 4);

        let names = NAMES.map(|name| env.mem.alloc_and_write_cstr(name.as_bytes()).to_bits());
        let empty = env.mem.alloc_and_write_cstr(b"").to_bits();
        let table = env.mem.alloc(4 * 6).to_bits();
        let _ = VARIANT_TABLE.set((table, names, empty));
        set_variant_table(env, None);
        env.mem
            .write(MutPtr::<u32>::from_bits(VARIANT_TABLE_LITERAL), table);

        const LOAD: HostFunction = &(load_strings as fn(&mut Environment, u32));
        const APPLY: HostFunction = &(apply_saved_language as fn(&mut Environment));
        for (at, name, host) in [
            (LOAD_STRINGS, "codwawzpc_load_strings", LOAD),
            (
                APPLY_SAVED_LANGUAGE,
                "codwawzpc_apply_saved_language",
                APPLY,
            ),
        ] {
            let host = env.dyld.create_guest_function(&mut env.mem, name, host);
            env.mem
                .write(MutPtr::<u32>::from_bits(at), A32_LDR_PC_NEXT_WORD);
            env.mem
                .write(MutPtr::<u32>::from_bits(at + 4), host.addr_with_thumb_bit());
            env.cpu.invalidate_cache_range(at, 8);
        }
        log_dbg!("Italian, German and Spanish are available");
    });
}

/// Fill the variant name table for a new language (`Some(id)`), whose
/// variants are those of [VARIANT_FALLBACK], or for any other.
fn set_variant_table(env: &mut Environment, new_language: Option<usize>) {
    let Some(&(table, names, empty)) = VARIANT_TABLE.get() else {
        return;
    };
    for (i, &name) in names.iter().enumerate() {
        let entry = match new_language {
            Some(id) if i == id => names[VARIANT_FALLBACK],
            Some(_) if i == VARIANT_FALLBACK => empty,
            _ => name,
        };
        env.mem
            .write(MutPtr::<u32>::from_bits(table + 4 * i as u32), entry);
    }
}

/// Replaces the game's string table loader.
fn load_strings(env: &mut Environment, path: u32) {
    let path_str =
        String::from_utf8_lossy(env.mem.cstr_at(Ptr::<u8, false>::from_bits(path))).into_owned();
    let name = path_str
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".dat"))
        .unwrap_or_default();
    let id = NAMES.iter().position(|&n| n == name);
    let new_language = id.filter(|&id| id >= 3);
    set_variant_table(env, new_language);
    if let Some(t) = new_language.and_then(|_| translation(name)) {
        match copy_translation(env, t) {
            Ok(()) => {
                log_dbg!("Loaded the {name} translation");
                return;
            }
            Err(e) => {
                log!("Not using the {name} translation: {e}");
            }
        }
    }
    let load = GuestFunction::from_addr_with_thumb_bit(LOAD_STRINGS_THUMB);
    let () = load.call_from_host(env, (path,));
}

/// Copy a translation into the game's string table, as the game's loader
/// would have.
fn copy_translation(env: &mut Environment, t: &Translation) -> Result<(), String> {
    let count = read_u32(env, STRINGS + STRINGS_COUNT);
    if count != 0 && count as usize != t.hashes.len() {
        return Err(format!("the game has {count} strings"));
    }
    // Once LocaliseHashes.dat is loaded, check each string's id.
    let (hashes, indices) = (
        read_u32(env, STRINGS + STRINGS_HASHES),
        read_u32(env, STRINGS + STRINGS_INDICES),
    );
    if hashes != 0 && indices != 0 {
        for k in 0..t.hashes.len() as u32 {
            let hash = read_u32(env, hashes + 4 * k);
            let index = env.mem.read(Ptr::<u16, false>::from_bits(indices + 2 * k));
            if t.hashes.get(usize::from(index)) != Some(&hash) {
                return Err(format!("string {index} has another id"));
            }
        }
    }
    let len = t.data.len() as u32;
    let mut offsets = read_u32(env, STRINGS + STRINGS_OFFSETS);
    if offsets == 0 {
        offsets = env.mem.alloc(4 * t.offsets.len() as u32).to_bits();
        poke(env, STRINGS + STRINGS_OFFSETS, offsets);
    }
    let mut buffer = read_u32(env, STRINGS + STRINGS_BUFFER);
    let capacity = read_u32(env, STRINGS + STRINGS_CAPACITY);
    if buffer == 0 || capacity < len {
        // The game's loader always reads its own capacity (47,494 bytes)
        // into the buffer, so it never gets smaller than that. The game's
        // buffer is not freed: strings in it may still be referenced.
        let new_capacity = len.max(capacity).max(64 * 1024);
        buffer = env.mem.alloc(new_capacity).to_bits();
        poke(env, STRINGS + STRINGS_BUFFER, buffer);
        poke(env, STRINGS + STRINGS_CAPACITY, new_capacity);
    }
    poke(env, STRINGS + STRINGS_COUNT, t.offsets.len() as u32);
    for (i, &offset) in t.offsets.iter().enumerate() {
        poke(env, offsets + 4 * i as u32, offset);
    }
    env.mem
        .bytes_at_mut(MutPtr::from_bits(buffer), len)
        .copy_from_slice(&t.data);
    Ok(())
}

/// Replaces "apply the saved language": the first time, the `Language`
/// setting becomes the saved language.
fn apply_saved_language(env: &mut Environment) {
    let options = peek(env, OPTIONS).unwrap_or(0);
    let localise = peek(env, LOCALISE).unwrap_or(0);
    if options == 0 || localise == 0 {
        return;
    }
    if !SETTING_APPLIED.swap(true, Ordering::Relaxed) {
        let setting = super::runtime::CONFIG
            .get()
            .map_or(Language::Game, |c| c.settings.language);
        if let Some(id) = setting.id() {
            log_dbg!("Language: {}", setting.name());
            poke(env, options + OPTIONS_LANGUAGE, id);
        }
    }
    let id = peek(env, options + OPTIONS_LANGUAGE).unwrap_or(1);
    let set = GuestFunction::from_addr_with_thumb_bit(SET_LANGUAGE);
    let () = set.call_from_host(env, (localise, id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (name, language) in Language::NAMES {
            assert_eq!(Language::from_name(name), Some(language));
            assert_eq!(language.name(), name);
        }
        assert_eq!(Language::from_name("German"), None);
        assert_eq!(Language::default(), Language::Game);
        // The ids follow the game's name table.
        for language in [
            Language::English,
            Language::French,
            Language::German,
            Language::Italian,
            Language::Spanish,
        ] {
            let name = NAMES[language.id().unwrap() as usize];
            assert!(name.starts_with(language.name()), "{name}");
        }
    }

    #[test]
    fn translations_parse() {
        let parsed: Vec<_> = TRANSLATIONS
            .iter()
            .map(|(name, text)| parse(text).unwrap_or_else(|e| panic!("{name}: {e}")))
            .collect();
        for (t, (name, _)) in parsed.iter().zip(TRANSLATIONS) {
            assert_eq!(t.hashes, parsed[0].hashes, "{name}: ids or order differ");
            // The game's fonts have Latin-1 and the few other characters
            // its own strings use; anything else is drawn as a box.
            let text = std::str::from_utf8(&t.data).unwrap();
            let missing: String = text
                .chars()
                .filter(|&c| c > '\u{ff}' && !"•€".contains(c))
                .collect();
            assert!(missing.is_empty(), "{name}: no glyphs for {missing:?}");
        }
    }

    #[test]
    fn parse_rejects_bad_lines() {
        let line = |s: &str| format!("{}{s}\n", "0\tx\n".repeat(STRING_COUNT - 1));
        assert!(parse(&line("0\ta\\nb")).is_ok());
        assert_eq!(
            parse(&line("0\ta\\nb\\\\c"))
                .unwrap()
                .data
                .rsplit(|&b| b == 0)
                .nth(1),
            Some(&b"a\nb\\c"[..])
        );
        for bad in ["0 a", "zz\ta", "0\ta\\t", "0\ta\\"] {
            assert!(parse(&line(bad)).is_err(), "{bad}");
        }
        assert!(parse("0\ta\n").is_err());
    }
}
