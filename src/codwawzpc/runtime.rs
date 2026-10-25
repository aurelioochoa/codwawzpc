/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Typed, cwd-independent entry to the inherited runtime.
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub struct GameSettings {
    pub fullscreen: bool,
    /// Original touch input only (mouse clicks are touches).
    pub touch_mode: bool,
    /// Mouse look speed multiplier.
    pub sensitivity: f32,
    pub invert_y: bool,
    /// Master volume, from 0 (muted) to 1.
    pub volume: f32,
    pub print_fps: bool,
    /// Gameplay frame-rate cap: 60, or the original 45.
    pub frame_rate: u32,
    /// Field of view in the game's degrees (see [super::fov]).
    pub fov: u32,
    /// Auto-aim strength, replacing the in-game option (see [super::auto_aim]).
    pub auto_aim: super::auto_aim::AutoAim,
    /// Save a real presented game frame for each new achievement.
    pub achievement_screenshots: bool,
    /// Record a short real gameplay clip around each new achievement.
    pub achievement_clips: bool,
    pub achievement_capture: super::capture_settings::CaptureSettings,
    /// Show the publisher logos at startup (see [super::splash]).
    pub intro_logos: bool,
    /// The game's language at startup (see [super::language]).
    pub language: super::language::Language,
    /// Relay endpoint as HOST:PORT; empty disables co-op.
    pub coop_server: String,
    /// Display name requested from the relay (which makes it unique).
    pub player_name: String,
}

/// Frame-rate caps `--frame-rate` accepts.
pub const FRAME_RATES: [u32; 2] = [60, 45];

/// Fields of view `--fov` accepts, in degrees; the game's own is 50.
pub const FOV_RANGE: std::ops::RangeInclusive<u32> = 40..=100;

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            touch_mode: false,
            sensitivity: 1.0,
            invert_y: false,
            volume: 1.0,
            print_fps: false,
            frame_rate: 60,
            fov: super::fov::ORIGINAL_FOV,
            auto_aim: super::auto_aim::AutoAim::Off,
            achievement_screenshots: true,
            achievement_clips: false,
            achievement_capture: super::capture_settings::CaptureSettings::default(),
            intro_logos: false,
            language: super::language::Language::Game,
            coop_server: String::new(),
            player_name: default_player_name(),
        }
    }
}

pub const SENSITIVITY_RANGE: std::ops::RangeInclusive<f32> = 0.05..=20.0;

/// Prefer the OS account over environment variables, which may be unset in a
/// desktop launch. Keep a usable name even when the account lookup fails.
fn default_player_name() -> String {
    #[cfg(unix)]
    {
        let mut entry = std::mem::MaybeUninit::<::libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0u8; 16 * 1024];
        // getpwuid_r fills both the entry and its strings in our owned buffer.
        let status = unsafe {
            ::libc::getpwuid_r(
                ::libc::geteuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == 0 && !result.is_null() {
            let name = unsafe { std::ffi::CStr::from_ptr((*result).pw_name) }
                .to_string_lossy()
                .into_owned();
            if !name.trim().is_empty() {
                return name;
            }
        }
    }
    ["LOGNAME", "USER", "LNAME", "USERNAME"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "Player".into())
}

/// Validate syntax without resolving DNS or contacting a relay. IPv6 literals
/// need brackets so the port is unambiguous, e.g. `[::1]:27960`.
pub fn validate_coop_server(server: &str) -> Result<(), String> {
    let invalid = || {
        "CoopServer must be HOST:PORT (port 1–65535), [IPv6]:PORT, or empty to disable co-op"
            .to_string()
    };
    if server.is_empty() {
        return Ok(());
    }
    if server.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    let (host, port) = if let Some(rest) = server.strip_prefix('[') {
        let (host, port) = rest.split_once("]:").ok_or_else(invalid)?;
        host.parse::<std::net::Ipv6Addr>().map_err(|_| invalid())?;
        (host, port)
    } else {
        let (host, port) = server.rsplit_once(':').ok_or_else(invalid)?;
        let dns = host.strip_suffix('.').unwrap_or(host);
        if dns.is_empty()
            || dns.len() > 253
            || dns.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || !label.as_bytes()[0].is_ascii_alphanumeric()
                    || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
            || (dns.contains('.')
                && dns.bytes().all(|c| c.is_ascii_digit() || c == b'.')
                && dns.parse::<std::net::Ipv4Addr>().is_err())
        {
            return Err(invalid());
        }
        (host, port)
    };
    if host.is_empty()
        || port.is_empty()
        || !port.bytes().all(|c| c.is_ascii_digit())
        || !matches!(port.parse::<u16>(), Ok(1..=u16::MAX))
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_player_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("PlayerName must not be empty".into())
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub app_path: PathBuf,
    pub resource_directory: PathBuf,
    /// Saves live below this directory in `touchHLE_sandbox/<bundle id>`.
    pub data_directory: PathBuf,
    pub config_directory: PathBuf,
    pub log_directory: PathBuf,
    /// SHA-256 of the imported IPA, for logs and bug reports.
    pub ipa_sha256: String,
    pub settings: GameSettings,
}

pub(crate) static CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

/// Hold an OS lease for the lifetime of the guest. A second runtime must not
/// snapshot the sandbox while the first one rotates save files at shutdown.
#[cfg(unix)]
pub(super) struct RunLease(std::fs::File);
#[cfg(unix)]
impl RunLease {
    pub(super) fn acquire(data: &std::path::Path) -> Result<Self, String> {
        use std::os::fd::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data.join("run.lock"))
            .map_err(|e| e.to_string())?;
        if unsafe { ::libc::flock(file.as_raw_fd(), ::libc::LOCK_EX | ::libc::LOCK_NB) } != 0 {
            return Err(format!(
                "Another codwawzpc runtime is using this save directory: {} ({})",
                data.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self(file))
    }
}
#[cfg(unix)]
impl Drop for RunLease {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            ::libc::flock(self.0.as_raw_fd(), ::libc::LOCK_UN);
        }
    }
}

/// Movie audio and OpenAL share the application's master gain.
pub fn volume() -> f32 {
    CONFIG.get().map_or(1.0, |c| c.settings.volume)
}

pub fn validate_resources(path: &std::path::Path) -> Result<(), String> {
    for file in [
        "touchHLE_default_options.txt",
        "touchHLE_dylibs/COPYING.libgcc",
        "touchHLE_dylibs/COPYING.libsqlite3",
        "touchHLE_dylibs/COPYING.libstdcxx",
        "touchHLE_dylibs/COPYING.libxml2",
        "touchHLE_dylibs/COPYING.libz",
        "touchHLE_dylibs/README.md",
        "touchHLE_dylibs/libgcc_s.1.dylib",
        "touchHLE_dylibs/libsqlite3.dylib",
        "touchHLE_dylibs/libstdc++.6.0.9.dylib",
        "touchHLE_dylibs/libxml2.2.dylib",
        "touchHLE_dylibs/libz.1.2.3.dylib",
        "touchHLE_fonts/LICENSE.liberation",
        "touchHLE_fonts/LICENSE.noto",
        "touchHLE_fonts/LiberationMono-Bold.ttf",
        "touchHLE_fonts/LiberationMono-BoldItalic.ttf",
        "touchHLE_fonts/LiberationMono-Italic.ttf",
        "touchHLE_fonts/LiberationMono-Regular.ttf",
        "touchHLE_fonts/LiberationSans-Bold.ttf",
        "touchHLE_fonts/LiberationSans-BoldItalic.ttf",
        "touchHLE_fonts/LiberationSans-Italic.ttf",
        "touchHLE_fonts/LiberationSans-Regular.ttf",
        "touchHLE_fonts/LiberationSerif-Bold.ttf",
        "touchHLE_fonts/LiberationSerif-BoldItalic.ttf",
        "touchHLE_fonts/LiberationSerif-Italic.ttf",
        "touchHLE_fonts/LiberationSerif-Regular.ttf",
        "touchHLE_fonts/NotoSansJP-Bold.otf",
        "touchHLE_fonts/NotoSansJP-Regular.otf",
        "touchHLE_fonts/README.md",
    ] {
        if !path.join(file).is_file() {
            return Err(format!(
                "Missing runtime resource {}",
                path.join(file).display()
            ));
        }
    }
    Ok(())
}

/// Runs one game per process, on the host's main thread. No cwd/env mutation.
pub fn run(mut config: RuntimeConfig) -> Result<(), String> {
    validate_coop_server(&config.settings.coop_server)?;
    validate_player_name(&config.settings.player_name)?;
    if !config.settings.volume.is_finite() || !(0.0..=1.0).contains(&config.settings.volume) {
        return Err("Volume must be between 0 and 1".into());
    }
    if !SENSITIVITY_RANGE.contains(&config.settings.sensitivity) {
        return Err("Sensitivity is out of range".into());
    }
    config.app_path = config.app_path.canonicalize().map_err(|e| e.to_string())?;
    config.resource_directory = config
        .resource_directory
        .canonicalize()
        .map_err(|e| e.to_string())?;
    validate_resources(&config.resource_directory)?;
    for path in [
        &mut config.data_directory,
        &mut config.config_directory,
        &mut config.log_directory,
    ] {
        std::fs::create_dir_all(&*path).map_err(|e| e.to_string())?;
        *path = path.canonicalize().map_err(|e| e.to_string())?;
    }
    let app = config.app_path.clone();
    let settings = config.settings.clone();
    #[cfg(unix)]
    let _lease = RunLease::acquire(&config.data_directory)?;
    // Keep the previous run's log, which may describe a crash.
    let log_path = config.log_directory.join("codwawzpc.log");
    if log_path.exists() {
        let _ = std::fs::rename(&log_path, log_path.with_extension("log.1"));
    }
    CONFIG
        .set(config)
        .map_err(|_| "A runtime has already been initialized in this process")?;
    log_identity(&settings);
    super::metrics::start();
    super::profiler::start();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        echo_no_panic!("codwawzpc stopped because of an error: {info}");
        default_hook(info);
        eprintln!(
            "codwawzpc: the game stopped because of an error. The log at {} \
             describes it; please include it in bug reports.",
            log_path.display()
        );
    }));
    super::coop::start(&settings.coop_server, &settings.player_name);
    let options = crate::options::Options::default();
    crate::launch_app(app, options, Vec::new(), None, false, Some(settings))
}

/// Host, build and game identity at the top of every log.
fn log_identity(settings: &GameSettings) {
    let config = CONFIG.get().unwrap();
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    let os = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    echo!("{}", super::version());
    echo!(
        "Host: {} {} ({}), kernel {}",
        os,
        std::env::consts::ARCH,
        std::env::consts::OS,
        read("/proc/sys/kernel/osrelease").trim()
    );
    echo!(
        "Game: {} {} (executable {}), IPA SHA-256 {}",
        super::profile::BUNDLE_ID,
        super::profile::VERSION,
        &super::profile::PAYLOAD[0].1[..16],
        config.ipa_sha256
    );
    echo!("App: {}", config.app_path.display());
    echo!(
        "Settings: controls={}, sensitivity={}, invert_y={}, fullscreen={}, frame_rate={}, fov={}, auto_aim={}, language={}",
        if settings.touch_mode {
            "touch"
        } else {
            "desktop"
        },
        settings.sensitivity,
        settings.invert_y,
        settings.fullscreen,
        settings.frame_rate,
        settings.fov,
        settings.auto_aim.name(),
        settings.language.name()
    );
    echo!();
}

pub(crate) fn apply_settings(options: &mut crate::options::Options, settings: GameSettings) {
    options.fullscreen = settings.fullscreen;
    options.desktop_controls = !settings.touch_mode;
    // Desktop controls read the controller themselves; the left stick is
    // not a tilt sensor.
    options.analog_stick_tilt_controls = settings.touch_mode;
    options.game_frame_rate = Some(settings.frame_rate);
    options.game_fov = Some(settings.fov);
    options.game_auto_aim = Some(settings.auto_aim);
    options.desktop_look = crate::codwawz_controls::LookSettings {
        sensitivity: settings.sensitivity,
        invert_y: settings.invert_y,
    };
    options.resizable_window = true;
    // Orientation comes from the app's Info.plist (landscape right).
    options.device_family = Some(crate::window::DeviceFamily::iPhone);
    options.scale_hack = std::num::NonZeroU32::new(1).unwrap();
    options.print_fps = settings.print_fps;
    // Airplay's loader waits on a semaphore posted from MainView's drawRect:,
    // which the fullscreen CAEAGLLayer fast path never calls.
    options.force_composition = true;
    options.popup_errors = false;
    options.preferred_languages = Some(vec!["en".into()]);
    // Only an active relay session may expose networking to the guest.
    options.network_access = super::coop::session().is_some_and(super::coop::Session::is_connected);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn runtime_lease_prevents_overlapping_save_rotation_and_releases() {
        let directory =
            std::env::temp_dir().join(format!("codwawzpc-lease-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let first = RunLease::acquire(&directory).unwrap();
        assert!(RunLease::acquire(&directory).is_err());
        drop(first);
        let next = RunLease::acquire(&directory).unwrap();
        drop(next);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
