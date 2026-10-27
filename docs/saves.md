# Saved games

COD Zombies 1.5.0 keeps all progress in one file: rank, per-map stats,
unlocks and the unlocked achievements (the save manager, `CSaveManager`,
`*0xa1a558`). This page describes the file, how the game reads and writes it
and how codwawzpc exports and imports it. Addresses are in the 1.5.0
executable `kiwi` (SHA-256 `4c940898…`).

## Files

The game's Documents folder
(`$XDG_DATA_HOME/codwawzpc/touchHLE_sandbox/com.activision.callofduty/Documents`)
contains:

| File | Contents |
| --- | --- |
| `savedata1.i3d` or `savedata2.i3d` | The saved game |
| `genericSink.dat`, `shipSink.dat`, `firstRunSink.dat` | Telemetry logs (session start, waves, device name); not progress |
| `data-iphone/` | Empty |

`Library/Preferences/com.activision.callofduty.plist` is an empty dictionary.

## Format

| Offset | Size | Contents |
| --- | --- | --- |
| 0 | 4 | Adler-32 of the rest of the file, little-endian |
| 4 | 4 | Save version, `0x00MMmmpp`; 1.5.0 writes 1.8.0 (`0x00010800`) |
| 8 | … | `CSaveManager` serialised with `IwSerialise` |

- **Checksum.** `0x199658` reads everything after the first word and
  compares it with `0xa47fc`, an Adler-32 (modulus 65521). When writing, it
  computes the checksum and writes it at offset 0. A real save's checksum
  matches `zlib.adler32` of bytes 4 onwards.
- **Version.** `CSaveManager` turns the config string `SaveDataVersion`
  (`x.y.z`) into `x << 16 | y << 8 | z` (`0x132a44`) and serialises it first
  (`0x151e18`). `0x132c24` treats a value with a nonzero top byte as invalid
  and otherwise compares versions. `AllowOldSaveData` is a related config key.
- **Contents.** `0x132d3c` serialises the manager's fields, the stats at
  `+0x400` and the unlocked achievement ids at `+0x478`
  (see [achievements](achievements.md)). Stat names such as
  `totalzombiekilled` and the player name appear as plain strings.
- **Not tied to a device.** The save has no device identifier; the
  telemetry logs do.

## Reading and writing

`CIsSaveStorageFile` (`0x199444`, `0x199548`) chooses the slot:

- **Load:** `savedata1.i3d` if it exists, otherwise `savedata2.i3d`. If
  neither exists there is no save. A checksum mismatch fails the load.
- **Save:** write the slot the game did not load, fix its checksum, then
  delete the other slot. If the checksum step fails, the new file is deleted
  instead. Normally only one slot exists, so a save is never half-written.

## Export, import and reset

```text
codwawzpc export-save <file>
codwawzpc import-save <file>
codwawzpc reset-save [--no-backup]
```

`export-save` copies the slot the game would load to `<file>`, after
checking its checksum. `import-save` checks the file's checksum and version,
then copies the existing slots to `save-backups/<UTC time>/` in the data
directory. It writes the file as `savedata1.i3d`, the slot the game loads
first, and removes `savedata2.i3d`. Both hold the runtime's lock
(`run.lock`), so they fail while the game runs. Writes go through a
temporary file and a rename.

`reset-save` starts over. It copies both slots and `achievements.plist` to
`save-backups/<UTC time>/` (skipped with `--no-backup`), then deletes them; with no slot the game creates
a fresh save, and the achievement report follows it on the next run. The
slots are not parsed, so a corrupt save can be reset. To undo a reset,
import the backed-up `savedata1.i3d` (or `savedata2.i3d`). It holds the same
lock and changes nothing when there is no save.

The launcher's Saved games window runs these commands (Export save…,
Import save…, Reset progress…). `achievements.plist` follows the imported save the next time
the game runs.

Checked: the real save above has a matching checksum and version 1.8.0. It
was imported into an empty data directory as `savedata1.i3d`, and the game
loaded it with its six achievements (`Achievements: 6 of 106 unlocked`).
Export after import returns the same bytes. Unit tests cover the checksum,
version, slot order, backups, rejected files and the lock.

## How this was found

String search and byte reads used [REA](https://github.com/morluto/rea)
(`rea search`, `rea read-bytes`) with Ghidra 12.1.4. REA's own fresh
import finds about 2,800 functions, which do not include the Marmalade game
code; that code is reached only through vtables. So the save routines were
decompiled from the existing Ghidra project, whose functions are seeded from
the vtables. The two tools agree on the string addresses. File-mode strings
(`0x27ec74`: `rb`, `r+b`, `wb`) show which code paths read and which write.
