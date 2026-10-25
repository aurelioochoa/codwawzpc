# Languages and voices: a field report

What the iPhone releases of Call of Duty: World at War: Zombies contain for
languages and recorded voices, what the code does with them, and how
codwawzpc added German, Italian and Spanish. Written from the game's own
bytes, executable and memory, not from recollection or outside sources.

Each statement is marked by how it is known:

- **Observed**: read directly from the files or the executable, reproducible
  with the method given.
- **Confirmed**: observed, and also seen to behave that way in the running game.
- **Inferred**: the most likely reading of what was observed, not proven.
- **Unknown**: not established; listed under [Open questions](#open-questions).

## Contents

- [Specimens](#specimens)
- [The string tables](#the-string-tables)
- [The three languages that never shipped](#the-three-languages-that-never-shipped)
- [1.4.1 to 1.5.0](#141-to-150)
- [Recovered key names](#recovered-key-names)
- [Where the text is used](#where-the-text-is-used)
- [Language in the code](#language-in-the-code)
- [Voices](#voices)
- [How codwawzpc introduced the translations](#how-codwawzpc-introduced-the-translations)
- [Open questions](#open-questions)
- [Method and tools](#method-and-tools)

## Specimens

| Release | Bundle | IPA SHA-256 | Shared archive |
| --- | --- | --- | --- |
| 1.5.0 | `com.activision.callofduty`, "COD Zombies" | `a2b6af38…432d782` | `_main.dz`, SHA-256 `261632ad…412e164e` |
| 1.4.1 | `com.activision.callofdutyshi`, "COD Zombies" | `56b6d6d8…35fdf9` | `main.dz`, SHA-256 `5d0e3e11…aa461d6` |

1.5.0 is the release codwawzpc runs: executable SHA-256 `4c940898…`, all four
maps (`_nacht`, `_verruckt`, `_shinonuma`, `_derriese`, each with a `_full`
archive). 1.4.1 is a separate app with a different bundle id and only the Shi
No Numa archives (`shinonuma.dz`, `shinonuma_full.dz`) beside `main.dz`. Both
declare `CFBundleDevelopmentRegion` English and no `CFBundleLocalizations`:
iOS was told nothing about other languages (observed).

The `.dz` files are Marmalade DTRZ archives with one LZMA chunk per file.

## The string tables

**Observed.** Both releases have the same seven files in `localisation/`:

| File | Game id | 1.4.1 bytes | 1.5.0 bytes |
| --- | ---: | ---: | ---: |
| `english_us.dat` | 0 | 45,585 | 45,825 |
| `english_uk.dat` | 1 | 45,565 | 45,805 |
| `french.dat` | 2 | 50,093 | 50,424 |
| `italian.dat` | 3 | 45,566 | 45,805 |
| `german.dat` | 4 | 45,566 | 45,805 |
| `spanish.dat` | 5 | 45,566 | 45,805 |
| `LocaliseHashes.dat` | | 4,380 | 4,398 |

The ids are the positions in the executable's language name table (see
[Language in the code](#language-in-the-code)).

A table (`LOC0`) is, in little-endian order:

| Field | 1.4.1 | 1.5.0 |
| --- | --- | --- |
| magic `LOC0` | | |
| `u32` buffer size | 50,107 in all six | 50,438 in all six |
| `u32` length of the string data | per file | per file |
| `u32` string count | 729 | 732 |
| `u32` offset of each string | | |
| encoding name, NUL-terminated | `utf-8` | `utf-8` |
| the strings, each NUL-terminated | | |

The buffer size is the same in all six files of a release. The game sizes its
string buffer from it once (buffer size − 4 × count − 16: 47,494 bytes in
1.5.0) and always reads that many bytes. The French data, the longest, is
47,474 bytes in 1.5.0 and 47,155 in 1.4.1, which leaves 20 and 36 bytes spare.
How the figure was chosen is unknown.

`LocaliseHashes.dat` is `LOC0`, a `u16` count, the strings' ids (`u32`,
sorted ascending) and the index of each id's string (`u16`). An id is the
**djb2 hash of the string's key name, lowercased** (Marmalade's
case-insensitive `IwHashString`). This is established by the 111 key names
recovered below: every one hashes to an id in the file, and the chance of a
false match is negligible (observed).

## The three languages that never shipped

**Observed.** In 1.5.0, `italian.dat`, `german.dat` and `spanish.dat` are
`english_uk.dat` with two bytes changed. Their first string, `Boilerplate UK`,
reads `Boilerplate IT`, `Boilerplate DE` and `Boilerplate ES`. The other 731
strings are UK English. Neither release has a German, Italian or Spanish
translation of any string, in these tables or anywhere else found; the only
German in the game is the German of the English and French text (Richtofen's
"Verdammt", map names). Only English and French were translated.

**Observed in 1.4.1.** The placeholders differ from UK English in three
strings, not one. Besides the boilerplate, two new Der Riese strings carry a
number that steps by language id, in all six tables:

| Id | Table | Index 694 | Index 704 ends with |
| ---: | --- | --- | --- |
| 0 | `english_us` | `40 Knives` | `Round 7` |
| 1 | `english_uk` | `41 Knives` | `Round 8` |
| 2 | `french` | `42 Knives` | `Round 9` |
| 3 | `italian` | `43 Knives` | `Round 10` |
| 4 | `german` | `44 Knives` | `Round 11` |
| 5 | `spanish` | `45 Knives` | `Round 12` |

The values are exactly 40 + id and 7 + id. In 1.4.1 these strings were not
yet translated even into French (`42 Knives` is English).

**Inferred.** The stepped numbers look like a deliberate marker: a way to see
at a glance which table a build had loaded. The real values (the "40 Knives"
achievement, "before Round 7") are the `english_us` ones, which 1.5.0 uses in
all six tables. That the German, Italian and Spanish tables were kept
current, numbered and labelled for two releases shows the slots were
maintained. Whether translations were ever commissioned is unknown.

The US table was maintained separately too. In 1.5.0, US and UK differ only
in the boilerplate and in string 713, an achievement description, where the
US table repeats the description of string 714 (observed).

## 1.4.1 to 1.5.0

**Observed**, comparing strings by id:

- 3 strings were added, at the end: the Der Riese teleporter messages
  (indices 729–731). None were removed, and the order is unchanged.
- 10 UK English strings changed: the markers above, spacing fixes in two
  "Buy Ammo" prompts, "(one time only)" added to the Bowie knife prompt, a
  shortened achievement description, and line changes in the tilt-control
  help and two credits blocks.
- French went from 116 strings identical to English to 70; 52 French strings
  changed. The Der Riese texts (about 679–731) were translated for 1.5.0.
  The 70 left identical in 1.5.0 are the credits; words that are the same in
  French ("Options", "Volume", "Pause", "Total"); names ("Nacht der Untoten",
  "Shi No Numa", "Der Riese", "Wunderkind", "Richtofen"); a few left in
  English ("Headshots", "The Playground", "Hong Kong Akimbo", "Type C");
  `oneShotRod` (606, an internal-looking name) and `THIS HAS BEEN REMOVED`
  (206).

## Recovered key names

**Observed.** 108 key names survive as text, all beginning `LS_`. They come
from the executable (its data segment is encoded on disk and was read from a
memory dump), from level Lua and from resource groups. Three more were
confirmed by hashing guesses that fill gaps in a numbered series:
`LS_TUTORIAL_DLG02` (index 94), `LS_TUTORIAL_DLG17` (240) and
`LS_TUTORIAL_DLG19_IPAD` (622). In all, 111 of 732 strings have their
original names. The rest are known only by id. The full list is in
[string-keys.md](string-keys.md).

The names fall into families: `LS_TUTORIAL_DLG01`–`22` (Richtofen's tutorial
lines), `LS_COOP_*` (co-op and connection messages), `LS_TELEPORTER_*`,
`LS_BUY_*` (map purchases), `LS_DEMONWARE_*` (online profile service),
`LS_RANK_FIRST`/`SECOND`/`THIRD`/`REST` (the ordinal suffixes 573–576),
`LS_ROUND_1`, `_LITE`, `_PREVIEW`, and account validation such as
`LS_USERNAME_NO_SWEARING`.

## Where the text is used

**Observed.** Searching every resource group and the executable for each
string's id finds 541 of the 732. They turn up in `frontend.group.bin`
(247), the in-game `ui.group.bin` (136), the achievement groups and the maps'
environment and dynamics groups. 98 strings have neither a known name nor an
id found anywhere.

That does not make them unused. "Touch screen to start" (26), "Back" (28)
and the touch diagram's Move/Aim/Shoot (344–346) are among the 98 and are on
screen in the running game. Some references are therefore stored in a form
this search does not find. A raw 32-bit search also matches by chance about
2% of the time in this much data, so a single found id is weak evidence on
its own.

What can be said:

- The tutorial script queues 20 of the 22 `LS_TUTORIAL_DLG` lines.
  `DLG02` ("That's it. Now come over here…") and `DLG17` (a pointer to the
  help section) are in every table but are never queued by the shipped
  tutorial script (observed). Whether native code shows them is unknown.
- The tables carry iPad wording (612–621, `LS_TUTORIAL_DLG19_IPAD`): help
  pages and calibration prompts that say "iPad", and a 2010 copyright line
  beside the 2009 one (observed). The embedded configuration also has an
  `iPadMode` section, the only place that sets `SingleLanguage=1` (observed).
  This suggests the tables were shared with an iPad build (inferred).

## Language in the code

All observed in the 1.5.0 executable, with addresses as in the dumps.

- **The name table** (`0xa16720`, in the encoded data segment) lists
  `english_us`, `english_uk`, `french`, `italian`, `german`, `spanish`. Four
  code literals point at it (`0x222ce0`, `0x222e48`, `0x222e5c`, `0x222f40`):
  the name-to-id lookup, `CLocalise::SetLanguage` and the initial load (both
  build `localisation/%s.dat`), and the getter used by `CGuiFrameLocalised`.
- **Device language.** `0x222ce4` maps the device language code to these
  ids: 2 → 2, 3 → 4, 4 → 5, 5 → 3, anything else → 1 (observed). With
  Marmalade's language codes (French 2, German 3, Spanish 4, Italian 5),
  that is French → `french`, German → `german`, Spanish → `spanish`,
  Italian → `italian`, and English or anything else → `english_uk`. The
  codes come from Marmalade's API, not from this executable (inferred). The
  game was ready to pick these languages from the phone's settings.
- **The refusal.** `CLocalise::SetLanguage` (`0x222d2c`, ARM) reads
  `SingleLanguage` from the configuration. If it is 1, only id 1 is accepted.
  Otherwise `cmp r5,#0; cmpne r5,#1; cmpne r5,#2; bne` (the branch at
  `0x222d80`) refuses everything but 0–2. Ids above 5 are refused before
  that. A refused id leaves the current language as it was, and the table
  is reloaded unchanged (observed in the code; not tested in the running
  game without the patch).
- **The menu.** Options → Language (`CFrontendStateLanguageSelect`,
  `CIngameStateLanguageSelect`) has two items. Its handler (`0x1d5908`)
  compares the item's hash with `english` and `french` and sets id 1 or 2.
  There are no `german`, `italian` or `spanish` items in the GUI data
  (their hashes appear nowhere) (observed).
- **The saved choice.** The handler also stores the id at offset `0x9c` of
  the saved options (`*0xa1a558`). Each time the frontend state starts,
  `0x14b5f8` re-applies it (observed; confirmed across launches).
- **Pictures with text.** `CGuiFrameLocalised` (vtable `0x2ac2f4`) frames
  hold children named `english_uk`, `english_us` and `french`, each with a
  `CIwTexture`. The frame shows the child named after the current language
  and hides the rest. The class appears 29 times in `frontend.group.bin`
  and 3 times in the in-game `ui.group.bin`, and 29 instances were found in
  a running game's memory at the main menu. In each, the UK and US children
  share one texture and the French child has its own (confirmed). A German,
  Italian or Spanish id would hide every child.
- **The string loader** (`0xd6e34`, an ARM veneer to Thumb `0xd6e3c`) reads
  the table described above into a structure at `0xa17d28`: count,
  capacity, the hash list and indices from `LocaliseHashes.dat`, the string
  buffer (`+0x14`) and the offsets (`+0x18`).
- **The news feed.** `CNewsReader` (`0x12002c`) reads a downloaded file of
  lines `<id>,<text>` or `A<id>,<n>,<text>` and keeps only the lines for the
  current language id. This is the only other language-aware system found.
  Its server no longer exists.
- **Nothing else.** The localiser (`*0xa1a5c0`) is referenced from 15
  functions: the ones above, the options screens, singleton plumbing and
  `CGame` event 9 (set a language by name). None is in sound code (observed).

## Voices

### What there is

**Observed**, by parsing every `CIwSoundData` and `CIwSoundSpec` resource in
1.5.0. All sound is IMA ADPCM: 256-byte blocks, 505 samples per block, mono.

| Group | Clips in the group | Length | Rate | Voice clips among the names recovered |
| --- | ---: | ---: | --- | --- |
| `ingame/sound/playervox` | 98 (104 specs) | 195 s | 11,025 Hz | no names recovered; clips 0.3–4.2 s |
| `derriese/sound/derriese_playervox` | 52 | 154 s | 11,025 Hz | no names recovered; 1.5–7.5 s |
| `shinonuma/sound/shinonuma_playervox` | 8 | 26 s | 11,025 Hz | no names recovered; 2.5–4.1 s |
| `derriese/sound/derriese_pa` | 28 | 55 s | 22,050 Hz | `pa_level_start` (6.3 s), `pa_power_on`, `pa_link_count_1`–`10`, `_15`, `_20` |
| `ingame/sound/sfx` | 193 | 301 s | 22,050–48,000 Hz | `hank_bust_some_heads`, `hank_back_in_the_dirt`, `hank_kickass_04`, `dog_start_vox` (11.6 s), `monkey_expl_vox1`–`3` |
| `derriese/sound/derriese_sfx` | 39 | 651 s | 8,000 and 22,050 Hz | `flytrap_found_vox1`–`3`, `message1`–`8` (27–105 s each) |
| `shinonuma/sound/shinonuma_sfx` | 13 | 238 s | 22,050 Hz | `message1`–`3` (34–89 s) |

The three `sfx` groups also hold sound effects; the last column lists only
their clips whose names suggest speech. In all, the game's 15 sound groups
hold 499 clips (about 37 minutes). There is one copy
of each voice, in groups named only by map. No sound group, file or name
refers to a language (observed).

Sound names are hashed like string keys (lowercased djb2) and stored only
as ids. 87 names (93 clips) resolved: from names the game still holds as
text (Lua constants, settings such as `HeadshotAnnounceSounds`), and from
numbered series where every member hashes correctly (`pa_link_count_1`…,
`message1`…).
Guessing also produces isolated chance matches, which are not counted.

### When voices play

- **Headshot announcer.** The setting `HeadshotAnnounceSounds` is
  `hank_bust_some_heads,hank_back_in_the_dirt,hank_kickass_04` (observed in
  `console.bin`). The code that plays them (`0x131874`) does so only when the
  level state manager's `+0x9c` flag is false.
- **Character voices.** That flag is set by `SetUsingPlayerVox` (Lua binding
  `0xbc7d8` writes `CLevelStateManager+0x9c`). Each map's script passes a
  constant: Nacht der Untoten and Verrückt `false`, Shi No Numa and Der Riese
  `true` (observed in the decoded bytecode: `LOADBOOL` before the call).
  By the code, Nacht der Untoten and Verrückt therefore get the announcer
  (not checked by ear). Shi No Numa and Der Riese use the `playervox` clips,
  which `0x192184` looks up by event id only when the flag is set
  (observed). Which character speaks which clip is unknown.
- **Radio messages.** Der Riese's script has triggers `Radio1`–`Radio8` and
  its sfx group has `message1`–`message8`, 27–105 s each. Shi No Numa has
  `message1`–`3`. That these are the radio messages is inferred from the
  names, counts and lengths.
- **Der Riese PA.** `pa_level_start`, `pa_power_on` and `pa_link_count_N`
  (observed names). That the counts belong to linking the teleporters is
  inferred from the names and the map's `LS_TELEPORTER_*` text.
- **The tutorial is silent.** Richtofen's tutorial lines are text only. The
  tutorial script (the same Lua chunk in every map's shared group) queues
  them with `QueueDialogue`. That binding (`0x9eeac`) hashes the key and
  appends `{id, duration, flag}` to the HUD's message queue (`0x1c7024`),
  with no sound. The only sound the tutorial script plays is `colt_fire`
  (observed).

### Language and voices

No voice depends on the language. There are no per-language sound
resources, no sound code reads the language, and the only language-indexed
loads are the string table, the picture variants and the news feed
(observed). Whatever language is chosen, the same recordings play.

The recordings are presumably English. That has not been checked by
listening or transcription: the clips decode to plausible audio (low
zero-crossing rate, quiet edges), but no one has listened to them for this
report.

### 1.4.1 to 1.5.0

**Observed.** 1.4.1's `playervox` has the same 98 clip names as 1.5.0's, at
22,050 Hz (197 s), and is 2.18 MB against 1.09 MB. No clip is byte-identical.
1.5.0 re-encoded the shared voice set at half the sample rate, which halved
its size. Why is not recorded; saving space is the likely reason (inferred).

## How codwawzpc introduced the translations

The game shipped **the slots, not the translations**: three tables of UK
English labelled `DE`, `IT` and `ES`, the code to pick those languages, and
a check that refuses them. The German, Italian and Spanish text is not
recovered material. It was written for this project in October 2026 by an AI
assistant (Claude), checked on screen in the game and against the English
for placeholders and fit, and has not yet been reviewed by native speakers.

### What was added

- **The translations**, 732 strings each, in
  `src/codwawzpc/localisation/{german,italian,spanish}.txt`. Each line is
  the string's id and its text, in the game's order. The files contain no
  English. [docs/localisation.md](localisation.md) describes their
  conventions.
- **Patches to the running game** (`src/codwawzpc/language.rs`), applied in
  memory when the game opens its first archive, after its loader has hashed
  its own code. Each is applied only if the original bytes are found:

| Address | Original | Replaced with | Effect |
| --- | --- | --- | --- |
| `0x222d80` | `bne` (`0x1a000001`) | `nop` (`0xe1a00000`) | `SetLanguage` accepts ids 3–5 |
| `0xd6e34` | `adr r12,…; bx r12` | `ldr pc,[pc,#-4]` + host function | the string loader goes through the host |
| `0x222e5c` | pointer to the name table | pointer to a host-managed copy | picture variants for the new languages |
| `0x14b5f8` | `ldr r0,…; ldr r0,[r0]` | `ldr pc,[pc,#-4]` + host function | the setting becomes the saved language once |

### What happens at run time

1. During startup, before the splash screens, the game loads the table of
   its current language and `LocaliseHashes.dat` with its own code
   (`0x222e84`).
2. When the frontend starts, the host's replacement for `0x14b5f8` writes the
   `Language` setting (if it is not `game`) into the saved options' `+0x9c`,
   then calls the game's `SetLanguage` with the saved value.
3. `SetLanguage` builds `localisation/german.dat` (for example) and calls
   the loader. The host sees a German, Italian or Spanish name and checks
   each translated string's id against the game's loaded
   `LocaliseHashes.dat`. It then copies the strings and offsets into the
   game's table. If they did not fit the 47,494-byte buffer it would give the
   game a larger one (they fit: 45.4–45.8 KB). Any other name goes to the
   game's own loader, so English and French are untouched.
4. For the new ids, the host's name table names the new language's slot
   `english_uk` and blanks the real `english_uk` slot. `CGuiFrameLocalised`
   then shows the English pictures instead of none.
5. The in-game Language screen still offers English and French. Choosing one
   works as before and is saved. The next launch with a `Language` setting
   writes the setting again.

### What was verified

All confirmed in scripted runs of the game under codwawzpc:

- German, Italian and Spanish load at startup. The title, main menu, options,
  help, level select, Single Player and the tutorial's subtitles show the
  translations.
- The fonts draw ä, ß, Ö, Ü, ñ, Ñ, ¿, ¡, ì, ò, ó, ú, à, è. They do not draw
  „, “ or –, which showed as boxes and were removed; a unit test now rejects
  any character outside Latin-1, `•` and `€`.
- Text that overflowed fixed layouts was shortened (the Single Player
  label and notes, the touch diagram labels).
- In a German run, the picture frame on screen had its `english_uk` child
  visible and the other two hidden.
- Launch order: German set → English chosen in game → `game` relaunch stays
  English → German set → `game` relaunch stays German.
- Nothing in the game's files changes. Translation ids are checked against
  the game, and `scripts/review_translation.py` checks the files against the
  player's own copy.

The voices were not changed: the same recordings play in every language, as
they always did.

## Open questions

- What the 158 character voice clips and the PA and radio messages say, and
  who speaks them. Answering this needs listening or transcription.
- The names of the remaining 621 strings and of most sounds.
- Whether native code shows `LS_TUTORIAL_DLG02` and `DLG17`, and how
  `LS_TUTORIAL_DLG19_IPAD` is selected.
- How the GUI refers to strings such as "Back" whose ids were not found.
- What the extra specs mean (104 specs for 98 clips in `playervox`, 196 for
  193 in `sfx`, 18 for 13 in Nacht's ambience).
- Whether German, Italian or Spanish translations were ever made.
- How the 50,438-byte buffer size was chosen.

## Method and tools

- Archives were unpacked with a small DTRZ reader (the format is described
  in `src/codwawzpc/archive.rs`). `scripts/review_translation.py` contains a
  self-contained reader for the string tables.
- The executable was studied in Ghidra 12.1.4, in a project seeded with the
  game's vtables. Thumb functions had to be disassembled with the Thumb mode
  forced, because the automatic analysis decodes them as ARM. A memory dump
  of the running game (made with `CODWAWZPC_MEM_DUMP`) supplied the decoded
  data segment: the name table, configuration text and string literals that
  are encoded on disk.
- Ids were matched by hashing candidate names with lowercased djb2
  (`h = 5381; h = h·33 + c` over the lowercased bytes, modulo 2³²).
- Resource groups were read as lists (class hash, count, then records with a
  size, class hash and name hash). `CIwSoundData` records hold the buffer
  size, sample count, data, sample rate, format, samples per block and block
  size, as its serialiser (`0x12e9c8`, ADPCM `0x18ca88`) writes them.
- Lua chunks are Lua 5.1 bytecode with 4-byte integer numbers and no debug
  information. They were read with a minimal chunk reader. Constants and the
  `LOADBOOL` before each call give the values passed.
- Runtime checks used codwawzpc's input scripts and frame captures in a
  private data directory, and `CODWAWZPC_TRACE=touchHLE::codwawzpc::language`
  for the host's log.

These research tools live in the project's research workspace, not in this
repository; the facts above are stated so they can be checked again with any
equivalent tools.
