/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Game-specific host application. Guest compatibility modules stay upstream.
pub mod achievements;
pub(crate) mod archive;
pub mod auto_aim;
pub mod bindings;
pub mod capture_settings;
pub(crate) mod coop;
pub(crate) mod fov;
pub(crate) mod frame_rate;
pub mod graphics;
mod hash;
pub mod import;
pub mod language;
mod launcher;
pub(crate) mod lzma;
pub(crate) mod menu;
pub(crate) mod metrics;
pub(crate) mod native;
pub mod profile;
pub(crate) mod profiler;
pub mod runtime;
pub mod saves;
pub(crate) mod splash;
pub(crate) mod widescreen;

use auto_aim::AutoAim;
use graphics::{Picture, TextureFilter, Widescreen};
use language::Language;
use plist::{Dictionary, Value};
use runtime::{
    GameSettings, RuntimeConfig, FOV_RANGE, FRAME_RATES, RENDER_SCALE_RANGE, SENSITIVITY_RANGE,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const HELP: &str = "codwawzpc — Call of Duty: Zombies (iPhone 1.5.0) for Linux x86_64

Usage:
  codwawzpc launcher            Open the native Shi No Numa launcher
  codwawzpc import <game.ipa>   Validate and install your copy of the game
  codwawzpc check               Verify the installed game files
  codwawzpc run [options]       Play
  codwawzpc export-save <file>  Copy your saved game to a file
  codwawzpc import-save <file>  Replace your saved game with a file; the
                                current one is backed up first
  codwawzpc reset-save [--no-backup]
                                Delete your saved game and start over; it is
                                backed up first unless --no-backup is given
  codwawzpc --help
  codwawzpc --version
  codwawzpc --copyright         Runtime and dependency license notices

Run options (all but --fps are remembered):
  --fullscreen | --windowed
  --controls=desktop|touch      Keyboard and mouse (default), or the original
                                touch controls with the mouse as a finger
  --sensitivity=<number>        Mouse look speed, 0.05 to 20 (default 1)
  --invert-y | --no-invert-y    Invert vertical mouse look
  --frame-rate=60|45            Gameplay frame-rate cap (default 60; the
                                game's own cap is 45)
  --fov=<degrees>               Field of view, 40 to 100 (default 50, the
                                game's own); aiming keeps each weapon's zoom
  --render-scale=1|2|3|4        Internal rendering resolution as a multiple of
                                480x320 (default 1, the game's own); higher
                                is sharper and smoother when it exceeds the
                                window, and costs more GPU time
  --widescreen=off|16:10|16:9   Give the game a wider screen (default off,
                                the original 3:2); levels show more at the
                                sides and the HUD follows the edges; menu
                                backgrounds are widened to fit
  --texture-filter=original|trilinear|2x|4x|8x|16x
                                Filtering of the game's mipmapped surfaces
                                (default original); trilinear removes visible
                                mip bands, 2x-16x anisotropic also keeps
                                floors and walls sharp at an angle
  --gamma=<number>              Brightness of dark tones, 0.5 to 2 (default 1;
                                higher is brighter)
  --contrast=<number>           Contrast, 0.5 to 1.5 (default 1)
  --sharpening=<number>         Edge sharpening, 0 to 1 (default 0)
  --picture=original            Reset gamma, contrast and sharpening to the
                                game's own picture
  --auto-aim=off|low|medium|high
                                Aim assist (default off); low, medium and high
                                are the game's own Auto-Aim levels
  --logos | --no-logos          Show the publisher logos at startup (default
                                off: about 8 seconds faster to the title)
  --language=game|english|french|german|italian|spanish
                                Language at startup (default game: the one
                                chosen in the game's own Language screen);
                                German, Italian and Spanish are this
                                project's translations
  --fps                         Print measured frame rates
  --coop-server=HOST:PORT        Co-op relay (default off); use [IPv6]:PORT for
                                IPv6, or --coop-server= to disable co-op
  --player-name=NAME             Co-op name (default: your OS user name)

Default desktop controls (customize in launcher Settings & controls):
  WASD / arrows   move              Mouse          look
  Left click      fire (hold for automatic weapons)
  Right click     aim down sights (hold)            Shift   sprint (hold)
  E / F           use: buy, repair (hold)           R       reload
  G               grenade (hold to cook, release to throw)
  4               next special grenade              V       knife
  Wheel / Q       change weapon     Escape         pause (back in menus)
  Tab             release or capture the mouse; it is captured during play
                  and released in menus, where the pointer clicks
  F11 fullscreen, F10 screenshot

Data:   $XDG_DATA_HOME/codwawzpc (default ~/.local/share/codwawzpc)
Config: $XDG_CONFIG_HOME/codwawzpc (default ~/.config/codwawzpc)
Logs:   $XDG_STATE_HOME/codwawzpc/logs (default ~/.local/state/codwawzpc/logs)
Portable installations use userdata beside the executable.
No game files are included or downloaded. Import copies the files it needs;
the IPA is not needed afterwards.";

#[derive(Debug, Clone)]
pub struct Directories {
    pub data: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
}
impl Directories {
    pub fn discover() -> Result<Self, String> {
        // A marker beside the host identifies a portable installation. Resolve
        // it each time so the installation can be moved without rewriting paths.
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if let Some(root) = exe
            .parent()
            .filter(|p| p.join(".codwawzpc-portable").is_file())
        {
            return Ok(Self {
                data: root.join("userdata/data"),
                config: root.join("userdata/config"),
                logs: root.join("userdata/logs"),
            });
        }
        let home = std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or("HOME must be set to resolve XDG directories")?;
        if !home.is_absolute() {
            return Err("HOME must be absolute".into());
        }
        let base = |key: &str, default: &str| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(default))
                .join("codwawzpc")
        };
        Ok(Self {
            data: base("XDG_DATA_HOME", ".local/share"),
            config: base("XDG_CONFIG_HOME", ".config"),
            logs: base("XDG_STATE_HOME", ".local/state").join("logs"),
        })
    }
}

pub fn version() -> String {
    format!(
        "codwawzpc {} (touchHLE {})",
        env!("CARGO_PKG_VERSION"),
        crate::VERSION.trim()
    )
}

/// Executable-relative installed resources first; a source checkout is a
/// development fallback compiled into this particular binary.
fn resources() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("Executable has no parent")?;
    if parent.join(".codwawzpc-portable").is_file() {
        let path = parent.join("resources");
        runtime::validate_resources(&path)?;
        return path.canonicalize().map_err(|e| e.to_string());
    }
    for path in [
        parent.join("resources"),
        parent.join("../share/codwawzpc"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ] {
        if runtime::validate_resources(&path).is_ok() {
            return path.canonicalize().map_err(|e| e.to_string());
        }
    }
    Err(
        "Runtime resources (touchHLE_dylibs, touchHLE_fonts) were not found beside the executable."
            .into(),
    )
}

fn load_settings(config: &Path) -> Result<GameSettings, String> {
    let path = config.join("settings.plist");
    if !path.exists() {
        return Ok(GameSettings::default());
    }
    let v = Value::from_file(&path).map_err(|e| format!("Invalid {}: {e}", path.display()))?;
    let d = v
        .as_dictionary()
        .ok_or("Settings must be a property-list dictionary")?;
    let mut s = GameSettings::default();
    for (key, value) in d {
        let invalid = || format!("Invalid {key} setting in {}", path.display());
        if let Some(result) = s.achievement_capture.read(key, value) {
            result.map_err(|e| format!("{}: {e}", invalid()))?;
            continue;
        }
        match key.as_str() {
            "Fullscreen" => s.fullscreen = value.as_boolean().ok_or_else(invalid)?,
            "TouchMode" => s.touch_mode = value.as_boolean().ok_or_else(invalid)?,
            "InvertY" => s.invert_y = value.as_boolean().ok_or_else(invalid)?,
            "Sensitivity" => {
                s.sensitivity = value
                    .as_real()
                    .map(|v| v as f32)
                    .filter(|v| SENSITIVITY_RANGE.contains(v))
                    .ok_or_else(invalid)?
            }
            "FrameRate" => {
                s.frame_rate = value
                    .as_unsigned_integer()
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| FRAME_RATES.contains(v))
                    .ok_or_else(invalid)?
            }
            "FieldOfView" => {
                s.fov = value
                    .as_unsigned_integer()
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| FOV_RANGE.contains(v))
                    .ok_or_else(invalid)?
            }
            "RenderScale" => {
                s.render_scale = value
                    .as_unsigned_integer()
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| RENDER_SCALE_RANGE.contains(v))
                    .ok_or_else(invalid)?
            }
            "Gamma" => {
                s.picture.gamma = real_in(value, Picture::GAMMA_RANGE).ok_or_else(invalid)?
            }
            "Contrast" => {
                s.picture.contrast = real_in(value, Picture::CONTRAST_RANGE).ok_or_else(invalid)?
            }
            "Sharpening" => {
                s.picture.sharpening =
                    real_in(value, Picture::SHARPENING_RANGE).ok_or_else(invalid)?
            }
            "Widescreen" => {
                s.widescreen = value
                    .as_string()
                    .and_then(Widescreen::from_name)
                    .ok_or_else(invalid)?
            }
            "TextureFilter" => {
                s.texture_filter = value
                    .as_string()
                    .and_then(TextureFilter::from_name)
                    .ok_or_else(invalid)?
            }
            "AutoAim" => {
                s.auto_aim = value
                    .as_string()
                    .and_then(AutoAim::from_name)
                    .ok_or_else(invalid)?
            }
            "AchievementScreenshots" => {
                s.achievement_screenshots = value.as_boolean().ok_or_else(invalid)?
            }
            "AchievementClips" => s.achievement_clips = value.as_boolean().ok_or_else(invalid)?,
            "IntroLogos" => s.intro_logos = value.as_boolean().ok_or_else(invalid)?,
            "Language" => {
                s.language = value
                    .as_string()
                    .and_then(Language::from_name)
                    .ok_or_else(invalid)?
            }
            "CoopServer" => {
                let server = value.as_string().ok_or_else(invalid)?;
                runtime::validate_coop_server(server).map_err(|_| invalid())?;
                s.coop_server = server.into();
            }
            "PlayerName" => {
                let name = value.as_string().ok_or_else(invalid)?;
                runtime::validate_player_name(name).map_err(|_| invalid())?;
                s.player_name = name.into();
            }
            "Volume" => {
                s.volume = value
                    .as_real()
                    .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    .ok_or_else(invalid)? as f32
            }
            _ => return Err(format!("Unknown setting {key} in {}", path.display())),
        }
    }
    Ok(s)
}
fn save_settings(config: &Path, s: &GameSettings) -> Result<(), String> {
    let mut d = Dictionary::new();
    s.achievement_capture.write(&mut d);
    d.insert("Fullscreen".into(), Value::Boolean(s.fullscreen));
    d.insert("TouchMode".into(), Value::Boolean(s.touch_mode));
    d.insert("InvertY".into(), Value::Boolean(s.invert_y));
    d.insert("Sensitivity".into(), Value::Real(s.sensitivity as f64));
    d.insert("Volume".into(), Value::Real(s.volume as f64));
    d.insert("FrameRate".into(), Value::Integer(s.frame_rate.into()));
    d.insert("FieldOfView".into(), Value::Integer(s.fov.into()));
    d.insert("RenderScale".into(), Value::Integer(s.render_scale.into()));
    d.insert(
        "Widescreen".into(),
        Value::String(s.widescreen.name().into()),
    );
    d.insert("Gamma".into(), Value::Real(s.picture.gamma as f64));
    d.insert("Contrast".into(), Value::Real(s.picture.contrast as f64));
    d.insert(
        "Sharpening".into(),
        Value::Real(s.picture.sharpening as f64),
    );
    d.insert(
        "TextureFilter".into(),
        Value::String(s.texture_filter.name().into()),
    );
    d.insert("AutoAim".into(), Value::String(s.auto_aim.name().into()));
    d.insert(
        "AchievementScreenshots".into(),
        Value::Boolean(s.achievement_screenshots),
    );
    d.insert(
        "AchievementClips".into(),
        Value::Boolean(s.achievement_clips),
    );
    d.insert("IntroLogos".into(), Value::Boolean(s.intro_logos));
    d.insert("Language".into(), Value::String(s.language.name().into()));
    d.insert("CoopServer".into(), Value::String(s.coop_server.clone()));
    d.insert("PlayerName".into(), Value::String(s.player_name.clone()));
    import::write_plist_atomic(&config.join("settings.plist"), &Value::Dictionary(d))
}

/// A settings value that is a real number in `range`.
fn real_in(value: &Value, range: std::ops::RangeInclusive<f32>) -> Option<f32> {
    value
        .as_real()
        .map(|v| v as f32)
        .filter(|v| range.contains(v))
}

/// A run option value that is a number in `range`.
fn parse_in(value: &str, range: std::ops::RangeInclusive<f32>) -> Option<f32> {
    value.parse::<f32>().ok().filter(|v| range.contains(v))
}

/// Apply `run` options to the remembered settings.
fn parse_run_options(
    args: impl IntoIterator<Item = OsString>,
    settings: &mut GameSettings,
) -> Result<(), String> {
    for arg in args {
        let arg = arg
            .to_str()
            .ok_or_else(|| format!("Invalid run option {arg:?}"))?
            .to_string();
        let (name, value) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (arg.as_str(), None),
        };
        let unknown = || format!("Unknown run option {arg:?}. See codwawzpc --help");
        match (name, value) {
            ("--fullscreen", None) => settings.fullscreen = true,
            ("--windowed", None) => settings.fullscreen = false,
            ("--fps", None) => settings.print_fps = true,
            ("--invert-y", None) => settings.invert_y = true,
            ("--no-invert-y", None) => settings.invert_y = false,
            ("--logos", None) => settings.intro_logos = true,
            ("--no-logos", None) => settings.intro_logos = false,
            ("--controls", Some("desktop")) => settings.touch_mode = false,
            ("--controls", Some("touch")) => settings.touch_mode = true,
            ("--controls", _) => return Err("--controls must be desktop or touch".into()),
            ("--sensitivity", Some(value)) => {
                settings.sensitivity = value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| SENSITIVITY_RANGE.contains(v))
                    .ok_or("--sensitivity must be a number from 0.05 to 20")?
            }
            ("--frame-rate", Some(value)) => {
                settings.frame_rate = value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| FRAME_RATES.contains(v))
                    .ok_or("--frame-rate must be 60 or 45")?
            }
            ("--fov", Some(value)) => {
                settings.fov = value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| FOV_RANGE.contains(v))
                    .ok_or("--fov must be a whole number of degrees from 40 to 100")?
            }
            ("--render-scale", Some(value)) => {
                settings.render_scale = value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| RENDER_SCALE_RANGE.contains(v))
                    .ok_or("--render-scale must be 1, 2, 3 or 4")?
            }
            ("--gamma", Some(value)) => {
                settings.picture.gamma = parse_in(value, Picture::GAMMA_RANGE)
                    .ok_or("--gamma must be a number from 0.5 to 2")?
            }
            ("--contrast", Some(value)) => {
                settings.picture.contrast = parse_in(value, Picture::CONTRAST_RANGE)
                    .ok_or("--contrast must be a number from 0.5 to 1.5")?
            }
            ("--sharpening", Some(value)) => {
                settings.picture.sharpening = parse_in(value, Picture::SHARPENING_RANGE)
                    .ok_or("--sharpening must be a number from 0 to 1")?
            }
            ("--picture", Some("original")) => settings.picture = Picture::ORIGINAL,
            ("--picture", _) => return Err("--picture must be original".into()),
            ("--widescreen", Some(value)) => {
                settings.widescreen =
                    Widescreen::from_name(value).ok_or("--widescreen must be off, 16:10 or 16:9")?
            }
            ("--texture-filter", Some(value)) => {
                settings.texture_filter = TextureFilter::from_name(value)
                    .ok_or("--texture-filter must be original, trilinear, 2x, 4x, 8x or 16x")?
            }
            ("--auto-aim", Some(value)) => {
                settings.auto_aim = AutoAim::from_name(value)
                    .ok_or("--auto-aim must be off, low, medium or high")?
            }
            ("--language", Some(value)) => {
                settings.language = Language::from_name(value)
                    .ok_or("--language must be game, english, french, german, italian or spanish")?
            }
            ("--coop-server", Some(value)) => {
                runtime::validate_coop_server(value)?;
                settings.coop_server = value.into();
            }
            ("--player-name", Some(value)) => {
                runtime::validate_player_name(value)?;
                settings.player_name = value.into();
            }
            _ => return Err(unknown()),
        }
    }
    Ok(())
}

pub fn cli(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let mut args: Vec<_> = args.into_iter().collect();
    let only = |args: &[OsString], usage: &str| {
        if args.len() == 1 {
            Ok(())
        } else {
            Err(format!("Usage: {usage}"))
        }
    };
    match args.first().and_then(|a| a.to_str()) {
        None => {
            println!("{HELP}");
            return Ok(());
        }
        Some("--help" | "-h" | "help") => {
            only(&args, "codwawzpc --help")?;
            println!("{HELP}");
            return Ok(());
        }
        Some("--version" | "-V") => {
            only(&args, "codwawzpc --version")?;
            println!("{}", version());
            return Ok(());
        }
        Some("--copyright") => {
            only(&args, "codwawzpc --copyright")?;
            println!("{}", crate::licenses::get_text());
            return Ok(());
        }
        _ => {}
    }
    let command = args.remove(0);
    if command == "launcher" {
        if !args.is_empty() {
            return Err("Usage: codwawzpc launcher".into());
        }
        return launcher::open();
    }
    let dirs = Directories::discover()?;
    match command.to_str() {
        Some("import") => {
            if args.len() != 1 {
                return Err("Usage: codwawzpc import <game.ipa>".into());
            }
            let installed = import::import_ipa(Path::new(&args[0]), &dirs.data)?;
            println!(
                "Imported Call of Duty: Zombies {}: {} files\nIPA SHA-256: {}\nInstalled at: {}",
                profile::VERSION,
                installed.files,
                installed.ipa_sha256,
                installed.app.display()
            );
        }
        Some("check") => {
            if !args.is_empty() {
                return Err("Usage: codwawzpc check".into());
            }
            let installed = import::check(&dirs.data)?;
            let resources = resources()?;
            println!(
                "Installation verified: Call of Duty: Zombies {}, {} files\n\
                 Maps: {}\nIPA SHA-256: {}\nResources: {}\nSaves: {}\nLogs: {}",
                profile::VERSION,
                installed.files,
                profile::MAPS.map(|m| m.name).join(", "),
                installed.ipa_sha256,
                resources.display(),
                dirs.data.join("touchHLE_sandbox").display(),
                dirs.logs.display()
            );
        }
        Some("export-save") => {
            if args.len() != 1 {
                return Err("Usage: codwawzpc export-save <file>".into());
            }
            let exported = saves::export(&dirs.data, Path::new(&args[0]))?;
            println!(
                "Exported saved game ({})\nFrom: {}\nTo: {}",
                exported.save.describe(),
                exported.from.display(),
                Path::new(&args[0]).display()
            );
        }
        Some("import-save") => {
            if args.len() != 1 {
                return Err("Usage: codwawzpc import-save <file>".into());
            }
            let imported = saves::import(&dirs.data, Path::new(&args[0]))?;
            println!(
                "Imported saved game ({})\nInstalled as: {}\n{}",
                imported.save.describe(),
                imported.to.display(),
                imported
                    .backup
                    .map_or("There was no previous save to back up.".into(), |b| {
                        format!("Previous save backed up to: {}", b.display())
                    })
            );
        }
        Some("reset-save") => {
            let backup = match args.iter().map(|a| a.to_str()).collect::<Vec<_>>()[..] {
                [] => true,
                [Some("--no-backup")] => false,
                _ => return Err("Usage: codwawzpc reset-save [--no-backup]".into()),
            };
            let reset = saves::reset(&dirs.data, backup)?;
            if reset.removed.is_empty() {
                println!("There was no saved game to reset.");
            } else {
                println!(
                    "Saved game reset. The game starts over the next time you play.\n\
                     Removed: {}\n{}",
                    reset
                        .removed
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    reset.backup.map_or("No backup was made.".into(), |b| {
                        format!("Backed up to: {}", b.display())
                    })
                );
            }
        }
        Some("run") => {
            let mut settings = load_settings(&dirs.config)?;
            settings.bindings = bindings::Bindings::load(&dirs.config)?;
            settings.print_fps = false;
            parse_run_options(args, &mut settings)?;
            let installation = import::check(&dirs.data)?;
            let resources = resources()?;
            save_settings(&dirs.config, &settings)?;
            // Keep upstream app options separate from our typed settings.
            let upstream_options = dirs.config.join("touchHLE_options.txt");
            if !upstream_options.exists() {
                std::fs::write(
                    &upstream_options,
                    "# Additional touchHLE options; see OPTIONS_HELP.txt in the source tree.\n",
                )
                .map_err(|e| e.to_string())?;
            }
            println!(
                "{}\nLaunching Call of Duty: Zombies {} (payload {}); log: {}",
                version(),
                profile::VERSION,
                &installation.ipa_sha256[..16],
                dirs.logs.join("codwawzpc.log").display()
            );
            runtime::run(RuntimeConfig {
                app_path: installation.app,
                resource_directory: resources,
                data_directory: dirs.data,
                config_directory: dirs.config,
                log_directory: dirs.logs,
                ipa_sha256: installation.ipa_sha256,
                settings,
            })?;
        }
        _ => return Err(format!("Unknown command {command:?}. See codwawzpc --help")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preferences_default_migrate_and_reject_invalid_values() {
        let p = std::env::temp_dir().join(format!("capture-settings-{}", uuid::Uuid::new_v4()));
        let mut dictionary = Dictionary::new();
        dictionary.insert("AchievementClips".into(), Value::Boolean(true));
        import::write_plist_atomic(
            &p.join("settings.plist"),
            &Value::Dictionary(dictionary.clone()),
        )
        .unwrap();
        let settings = load_settings(&p).unwrap();
        assert!(settings.achievement_clips);
        assert_eq!(settings.achievement_capture.clip_fps, 60);
        assert_eq!(settings.achievement_capture.clip_resolution, "1080p");
        assert_eq!(settings.achievement_capture.screenshot_format, "png");
        assert_eq!(settings.achievement_capture.screenshot_resolution, "native");
        for (key, choices) in [
            ("AchievementScreenshotFormat", &["png", "bmp"][..]),
            (
                "AchievementScreenshotResolution",
                &["native", "720p", "1080p"][..],
            ),
            ("AchievementClipResolution", &["540p", "720p", "1080p"][..]),
            ("AchievementClipQuality", &["standard", "high", "ultra"][..]),
        ] {
            for choice in choices {
                let mut settings = GameSettings::default();
                settings
                    .achievement_capture
                    .read(key, &Value::String((*choice).into()))
                    .unwrap()
                    .unwrap();
                save_settings(&p, &settings).unwrap();
                assert_eq!(load_settings(&p).unwrap(), settings);
            }
            for value in [
                Value::String("invalid".into()),
                Value::Boolean(true),
                Value::Integer(1.into()),
            ] {
                dictionary.insert(key.into(), value);
                import::write_plist_atomic(
                    &p.join("settings.plist"),
                    &Value::Dictionary(dictionary.clone()),
                )
                .unwrap();
                assert!(load_settings(&p).is_err(), "{key}");
            }
            dictionary.remove(key);
        }
        for fps in [30, 60] {
            let mut settings = GameSettings::default();
            settings.achievement_capture.clip_fps = fps;
            save_settings(&p, &settings).unwrap();
            assert_eq!(load_settings(&p).unwrap(), settings);
        }
        for value in [
            Value::Integer(8.into()),
            Value::Integer(120.into()),
            Value::Real(60.0),
            Value::Boolean(true),
        ] {
            dictionary.insert("AchievementClipFps".into(), value);
            import::write_plist_atomic(
                &p.join("settings.plist"),
                &Value::Dictionary(dictionary.clone()),
            )
            .unwrap();
            assert!(load_settings(&p).is_err());
        }
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn settings_round_trip_and_rejection() {
        let p = std::env::temp_dir().join(format!("codwawzpc-settings-{}", uuid::Uuid::new_v4()));
        let settings = GameSettings {
            bindings: Default::default(),
            fullscreen: true,
            touch_mode: true,
            sensitivity: 2.5,
            invert_y: true,
            volume: 0.25,
            print_fps: false,
            frame_rate: 45,
            fov: 75,
            render_scale: 3,
            texture_filter: TextureFilter::Anisotropic(8),
            widescreen: Widescreen::W16x10,
            picture: Picture {
                gamma: 1.25,
                contrast: 0.75,
                sharpening: 0.5,
            },
            auto_aim: AutoAim::High,
            achievement_screenshots: false,
            achievement_clips: true,
            achievement_capture: capture_settings::CaptureSettings {
                screenshot_format: "bmp".into(),
                screenshot_resolution: "720p".into(),
                clip_resolution: "540p".into(),
                clip_fps: 30,
                clip_quality: "ultra".into(),
            },
            intro_logos: true,
            language: Language::Spanish,
            coop_server: "relay.example.org:27960".into(),
            player_name: "Test player".into(),
        };
        save_settings(&p, &settings).unwrap();
        assert_eq!(load_settings(&p).unwrap(), settings);
        std::fs::write(p.join("settings.plist"), "bad").unwrap();
        assert!(load_settings(&p).is_err());
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn run_options_parse_and_reject() {
        let mut s = GameSettings::default();
        let args = [
            "--fullscreen",
            "--controls=touch",
            "--sensitivity=0.5",
            "--invert-y",
            "--fps",
            "--frame-rate=45",
            "--fov=90",
            "--render-scale=2",
            "--texture-filter=16x",
            "--widescreen=16:9",
            "--gamma=1.5",
            "--contrast=1.25",
            "--sharpening=0.25",
            "--auto-aim=medium",
            "--logos",
            "--language=german",
        ];
        parse_run_options(args.map(OsString::from), &mut s).unwrap();
        assert!(s.fullscreen && s.touch_mode && s.invert_y && s.print_fps);
        assert_eq!(s.sensitivity, 0.5);
        assert_eq!(s.frame_rate, 45);
        assert_eq!(s.fov, 90);
        assert_eq!(s.render_scale, 2);
        assert_eq!(s.texture_filter, TextureFilter::Anisotropic(16));
        assert_eq!(s.widescreen, Widescreen::W16x9);
        assert_eq!(
            s.picture,
            Picture {
                gamma: 1.5,
                contrast: 1.25,
                sharpening: 0.25
            }
        );
        parse_run_options([OsString::from("--picture=original")], &mut s).unwrap();
        assert!(s.picture.is_original());
        assert_eq!(s.auto_aim, AutoAim::Medium);
        assert!(s.intro_logos);
        assert_eq!(s.language, Language::German);
        parse_run_options(
            ["--controls=desktop", "--windowed", "--no-logos"].map(OsString::from),
            &mut s,
        )
        .unwrap();
        assert!(!s.touch_mode && !s.fullscreen && !s.intro_logos);
        for bad in [
            "--controls=gamepad",
            "--sensitivity=0",
            "--sensitivity=fast",
            "--sensitivity=nan",
            "--volume=5",
            "--frame-rate=30",
            "--frame-rate",
            "--fov=39",
            "--fov=101",
            "--fov=75.5",
            "--fov",
            "--render-scale=0",
            "--render-scale=5",
            "--render-scale=1.5",
            "--render-scale",
            "--texture-filter=32x",
            "--texture-filter=Trilinear",
            "--texture-filter",
            "--gamma=0.4",
            "--gamma=nan",
            "--contrast=2",
            "--sharpening=-0.1",
            "--sharpening=1.1",
            "--picture=vivid",
            "--widescreen=21:9",
            "--widescreen=on",
            "--widescreen",
            "--picture",
            "--auto-aim=on",
            "--auto-aim",
            "--language=deutsch",
            "--language=German",
            "--language",
            "--fullscreen=yes",
            "run",
        ] {
            assert!(
                parse_run_options([OsString::from(bad)], &mut s).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn coop_options_parse_and_persist() {
        let p = std::env::temp_dir().join(format!("codwawzpc-coop-{}", uuid::Uuid::new_v4()));
        let mut settings = GameSettings::default();
        assert!(settings.coop_server.is_empty());
        assert!(!settings.player_name.trim().is_empty());
        for server in [
            "localhost:27960",
            "relay.example.org.:443",
            "127.0.0.1:1",
            "[::1]:27960",
            "[2001:db8::1]:65535",
            "",
        ] {
            parse_run_options(
                [
                    format!("--coop-server={server}"),
                    "--player-name=Joueur 日本語".into(),
                ]
                .map(OsString::from),
                &mut settings,
            )
            .unwrap();
            assert_eq!(settings.coop_server, server);
            assert_eq!(settings.player_name, "Joueur 日本語");
            save_settings(&p, &settings).unwrap();
            assert_eq!(load_settings(&p).unwrap(), settings);
        }
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn coop_default_keeps_network_disabled() {
        let mut options = crate::options::Options::default();
        options.network_access = true;
        runtime::apply_settings(&mut options, GameSettings::default());
        assert!(!options.network_access);
    }

    #[test]
    fn coop_server_rejects_missing_port_and_malformed_endpoints() {
        for server in [
            "host",
            ":27960",
            "host:",
            "host:0",
            "host:65536",
            "host:-1",
            "host:+1",
            "host:１２",
            "host:port",
            "host:1:2",
            "::1:27960",
            "[::1]",
            "[::1]:0",
            "[invalid]:27960",
            "[::1]extra:80",
            " host:27960",
            "host:27960 ",
            "bad host:27960",
            "https://host:27960",
            "host/path:80",
            "host@relay:80",
            "host\\relay:80",
            "host\n:80",
            ".:80",
            "-host:80",
            "host-:80",
            "relay..example:80",
            "999.999.999.999:80",
            "[fe80::1%eth0]:80",
        ] {
            let mut settings = GameSettings::default();
            assert!(
                parse_run_options(
                    [OsString::from(format!("--coop-server={server}"))],
                    &mut settings
                )
                .is_err(),
                "{server:?}"
            );
            assert!(settings.coop_server.is_empty());
        }
        for option in [
            "--coop-server",
            "--player-name",
            "--player-name=",
            "--player-name=  ",
        ] {
            assert!(
                parse_run_options([OsString::from(option)], &mut GameSettings::default()).is_err(),
                "{option}"
            );
        }
        assert!(runtime::validate_coop_server(&format!("{}:80", "a".repeat(64))).is_err());
    }

    #[test]
    fn coop_plist_rejects_invalid_values() {
        let p =
            std::env::temp_dir().join(format!("codwawzpc-coop-invalid-{}", uuid::Uuid::new_v4()));
        for (key, value) in [
            ("CoopServer", Value::String("host".into())),
            ("CoopServer", Value::Integer(27960.into())),
            ("PlayerName", Value::String("".into())),
            ("PlayerName", Value::String("  ".into())),
            ("PlayerName", Value::Boolean(false)),
        ] {
            let mut dictionary = Dictionary::new();
            dictionary.insert(key.into(), value);
            import::write_plist_atomic(&p.join("settings.plist"), &Value::Dictionary(dictionary))
                .unwrap();
            assert!(load_settings(&p).is_err(), "{key}");
        }
        std::fs::remove_dir_all(p).unwrap();
    }
}
