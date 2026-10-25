/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Export and import of the game's save file. The format and slot rules were
//! reverse-engineered from the 1.5.0 executable; see docs/saves.md.
use super::profile::BUNDLE_ID;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The game loads the first slot that exists. It writes the other one, then
/// deletes the one it loaded, so normally exactly one exists.
pub const SLOTS: [&str; 2] = ["savedata1.i3d", "savedata2.i3d"];
/// Real saves are about 40 KB.
const MAX_SAVE: u64 = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct Save {
    pub bytes: Vec<u8>,
    /// The game's SaveDataVersion, major.minor.patch.
    pub version: (u8, u8, u8),
    pub checksum: u32,
}

impl Save {
    /// A save is a little-endian Adler-32 of the rest of the file, then the
    /// serialised save manager, which starts with its version (0x00MMmmpp).
    pub fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < 8 {
            return Err(format!(
                "Not a COD Zombies save: {} bytes is too short",
                bytes.len()
            ));
        }
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let (stored, actual) = (word(0), adler32(&bytes[4..]));
        if stored != actual {
            return Err(format!(
                "Not a valid COD Zombies save: checksum {actual:08x} does not match {stored:08x}"
            ));
        }
        let version = word(4);
        if version >> 24 != 0 {
            return Err(format!(
                "Not a valid COD Zombies save: unknown version {version:#x}"
            ));
        }
        Ok(Self {
            version: ((version >> 16) as u8, (version >> 8) as u8, version as u8),
            checksum: stored,
            bytes,
        })
    }

    pub fn describe(&self) -> String {
        let (major, minor, patch) = self.version;
        format!(
            "save format {major}.{minor}.{patch}, {} bytes, Adler-32 {:08x}",
            self.bytes.len(),
            self.checksum
        )
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the longest run that cannot overflow before the modulo.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

pub fn documents(data: &Path) -> PathBuf {
    data.join("touchHLE_sandbox")
        .join(BUNDLE_ID)
        .join("Documents")
}

/// The slot the game would load.
pub fn active(documents: &Path) -> Option<PathBuf> {
    SLOTS
        .iter()
        .map(|slot| documents.join(slot))
        .find(|path| path.is_file())
}

fn read(path: &Path) -> Result<Save, String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    if !file.metadata().map_err(error)?.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_SAVE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_SAVE {
        return Err(format!(
            "{} is larger than any COD Zombies save",
            path.display()
        ));
    }
    Save::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write beside the destination, then rename over it.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let temporary = parent.join(format!(".codwawzpc-save-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The game rewrites its save while it runs; hold the runtime's lease.
fn lease(data: &Path) -> Result<impl Sized, String> {
    fs::create_dir_all(data).map_err(error)?;
    #[cfg(unix)]
    return super::runtime::RunLease::acquire(data)
        .map_err(|_| "The game is running. Close it, then try again.".to_string());
    #[cfg(not(unix))]
    Ok(())
}

#[derive(Debug)]
pub struct Exported {
    pub save: Save,
    pub from: PathBuf,
}

/// Copy the save the game would load to `destination`.
pub fn export(data: &Path, destination: &Path) -> Result<Exported, String> {
    let _lease = lease(data)?;
    let from = active(&documents(data))
        .ok_or("No saved game yet. Play until the game saves your progress, then export again.")?;
    let save = read(&from)?;
    write_atomic(destination, &save.bytes)?;
    Ok(Exported { save, from })
}

#[derive(Debug)]
pub struct Imported {
    pub save: Save,
    pub to: PathBuf,
    /// Where the replaced save files were copied, if there were any.
    pub backup: Option<PathBuf>,
}

/// Replace the current save with `source`. The current slots are copied to
/// `save-backups/<UTC time>` first. The import becomes slot 1, which the game
/// loads first, and slot 2 is removed so a stale save cannot come back.
pub fn import(data: &Path, source: &Path) -> Result<Imported, String> {
    let save = read(source)?;
    let _lease = lease(data)?;
    let documents = documents(data);
    fs::create_dir_all(&documents).map_err(error)?;
    let existing: Vec<PathBuf> = SLOTS
        .iter()
        .map(|slot| documents.join(slot))
        .filter(|path| path.is_file())
        .collect();
    let backup = if existing.is_empty() {
        None
    } else {
        let directory = backup_directory(&data.join("save-backups"))?;
        for path in &existing {
            let copy = directory.join(path.file_name().unwrap());
            let bytes =
                fs::read(path).map_err(|e| format!("Cannot back up {}: {e}", path.display()))?;
            write_atomic(&copy, &bytes)?;
        }
        Some(directory)
    };
    let to = documents.join(SLOTS[0]);
    write_atomic(&to, &save.bytes)?;
    let stale = documents.join(SLOTS[1]);
    if stale.exists() {
        fs::remove_file(&stale).map_err(|e| format!("Cannot remove {}: {e}", stale.display()))?;
        File::open(&documents)
            .and_then(|f| f.sync_all())
            .map_err(error)?;
    }
    Ok(Imported { save, to, backup })
}

fn backup_directory(parent: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(parent).map_err(error)?;
    let stamp = utc_stamp(SystemTime::now());
    for n in 0..100 {
        let name = if n == 0 {
            stamp.clone()
        } else {
            format!("{stamp}-{n}")
        };
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", path.display())),
        }
    }
    Err(format!(
        "Cannot create a backup folder in {}",
        parent.display()
    ))
}

/// YYYY-MM-DD_HH-MM-SS in UTC, safe in file names.
fn utc_stamp(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (seconds / 86400, seconds % 86400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(payload: &[u8]) -> Vec<u8> {
        let mut bytes = adler32(payload).to_le_bytes().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    fn temporary() -> PathBuf {
        let path = std::env::temp_dir().join(format!("codwawzpc-saves-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn adler32_matches_zlib() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        // Long runs of 0xff exercise the deferred modulo.
        let long = vec![0xffu8; 100_000];
        let (mut a, mut b) = (1u64, 0u64);
        for &x in &long {
            a = (a + u64::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        assert_eq!(adler32(&long), (b << 16 | a) as u32);
    }

    #[test]
    fn parse_checks_checksum_and_version() {
        let good = save(&[0x00, 0x08, 0x01, 0x00, 1, 2, 3]);
        let parsed = Save::parse(good.clone()).unwrap();
        assert_eq!(parsed.version, (1, 8, 0));
        assert!(parsed.describe().starts_with("save format 1.8.0, 11 bytes"));
        let mut corrupt = good.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(Save::parse(corrupt).unwrap_err().contains("checksum"));
        assert!(Save::parse(save(&[0, 0, 0, 1]))
            .unwrap_err()
            .contains("version"));
        assert!(Save::parse(good[..7].to_vec())
            .unwrap_err()
            .contains("too short"));
    }

    #[test]
    fn slots_follow_the_game() {
        let data = temporary();
        let documents = documents(&data);
        fs::create_dir_all(&documents).unwrap();
        assert_eq!(active(&documents), None);
        fs::write(documents.join("savedata2.i3d"), b"2").unwrap();
        assert_eq!(active(&documents), Some(documents.join("savedata2.i3d")));
        fs::write(documents.join("savedata1.i3d"), b"1").unwrap();
        assert_eq!(active(&documents), Some(documents.join("savedata1.i3d")));
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn import_backs_up_replaces_and_exports() {
        let data = temporary();
        let documents = documents(&data);
        fs::create_dir_all(&documents).unwrap();
        let old = save(&[0, 8, 1, 0, b'o', b'l', b'd']);
        fs::write(documents.join("savedata2.i3d"), &old).unwrap();
        let new = save(&[0, 8, 1, 0, b'n', b'e', b'w']);
        let source = data.join("import.i3d");
        fs::write(&source, &new).unwrap();

        let imported = import(&data, &source).unwrap();
        assert_eq!(imported.to, documents.join("savedata1.i3d"));
        assert_eq!(fs::read(documents.join("savedata1.i3d")).unwrap(), new);
        assert!(!documents.join("savedata2.i3d").exists());
        let backup = imported.backup.unwrap();
        assert!(backup.starts_with(data.join("save-backups")));
        assert_eq!(fs::read(backup.join("savedata2.i3d")).unwrap(), old);

        let destination = data.join("exported.i3d");
        let exported = export(&data, &destination).unwrap();
        assert_eq!(exported.from, documents.join("savedata1.i3d"));
        assert_eq!(fs::read(&destination).unwrap(), new);
        // No temporary files are left behind.
        let names: Vec<_> = fs::read_dir(&documents)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["savedata1.i3d"]);
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn rejected_import_changes_nothing() {
        let data = temporary();
        let documents = documents(&data);
        fs::create_dir_all(&documents).unwrap();
        let old = save(&[0, 8, 1, 0, 1]);
        fs::write(documents.join("savedata1.i3d"), &old).unwrap();
        let source = data.join("bad.i3d");
        let mut bad = save(&[0, 8, 1, 0, 2]);
        bad[0] ^= 1;
        fs::write(&source, &bad).unwrap();
        assert!(import(&data, &source).is_err());
        assert_eq!(fs::read(documents.join("savedata1.i3d")).unwrap(), old);
        assert!(!data.join("save-backups").exists());
        assert!(export(&data, &data.join("missing-dir/out.i3d")).is_err());
        fs::remove_dir_all(data).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn running_game_blocks_import_and_export() {
        let data = temporary();
        let source = data.join("save.i3d");
        fs::write(&source, save(&[0, 8, 1, 0])).unwrap();
        let running = super::super::runtime::RunLease::acquire(&data).unwrap();
        assert!(import(&data, &source).unwrap_err().contains("running"));
        assert!(export(&data, &data.join("out.i3d"))
            .unwrap_err()
            .contains("running"));
        drop(running);
        import(&data, &source).unwrap();
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn utc_stamp_is_civil_time() {
        let at = |s| utc_stamp(UNIX_EPOCH + std::time::Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01_00-00-00");
        assert_eq!(at(951_782_400), "2000-02-29_00-00-00");
        assert_eq!(at(1_792_620_245), "2026-10-21_22-04-05");
    }
}
