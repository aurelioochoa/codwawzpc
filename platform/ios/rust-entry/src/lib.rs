/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
//! codwawzpc as an emulator core for the Applesauce iOS host. The host
//! dlopens this dylib and calls the same `touchhle_ios_*` entry points it uses
//! for its other cores.
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uchar};
use std::path::{Path, PathBuf};

pub struct TouchHLEIOSGameMetadata {
    display_name: CString,
    bundle_identifier: CString,
    orientation_capabilities: u32,
    icon_rgba: Vec<u8>,
    icon_width: u32,
    icon_height: u32,
}

fn c_string(value: String) -> CString {
    CString::new(value.replace('\0', "")).unwrap()
}

/// The host ships this core's own touchHLE_dylibs and touchHLE_fonts in
/// `codwawzpc_resources`, since its other cores bundle different builds.
fn resources() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = exe
        .parent()
        .ok_or("The app executable has no parent directory")?;
    Ok(bundle.join("codwawzpc_resources"))
}

fn documents() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Documents"))
        .ok_or_else(|| "The app container has no HOME path".into())
}

fn run_game(ipa: &Path) -> i32 {
    touchHLE::clear_host_exit_request();

    // The emulator runs on the main thread and this is called from
    // Objective-C. A panic unwinding past here is undefined behaviour at the
    // FFI boundary, so turn it into an error return the host can report.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        touchHLE::codwawzpc::run_ios(ipa, &documents()?, &resources()?)
    }));

    match outcome {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("codwawzpc failed: {error}");
            1
        }
        Err(_) => {
            eprintln!("codwawzpc panicked; see the log above for the cause");
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn touchhle_ios_request_exit() {
    touchHLE::request_host_exit();
}

#[no_mangle]
pub extern "C" fn touchhle_ios_current_fps() -> f32 {
    touchHLE::host_fps()
}

/// The touchHLE app picker isn't used; the host always launches a game.
#[no_mangle]
pub extern "C" fn touchhle_ios_run() -> i32 {
    eprintln!("codwawzpc failed: no game was selected");
    1
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_create(
    path: *const c_char,
) -> *mut TouchHLEIOSGameMetadata {
    if path.is_null() {
        return std::ptr::null_mut();
    }

    let path = match CStr::from_ptr(path).to_str() {
        Ok(path) => Path::new(path),
        Err(_) => return std::ptr::null_mut(),
    };
    let metadata = match std::panic::catch_unwind(|| touchHLE::inspect_host_app(path)) {
        Ok(Ok(metadata)) => metadata,
        Ok(Err(error)) => {
            eprintln!("Could not inspect game metadata: {error}");
            return std::ptr::null_mut();
        }
        Err(_) => {
            eprintln!("Could not inspect game metadata: the bundle parser panicked");
            return std::ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(TouchHLEIOSGameMetadata {
        display_name: c_string(metadata.display_name),
        bundle_identifier: c_string(metadata.bundle_identifier),
        orientation_capabilities: metadata.orientation_capabilities,
        icon_rgba: metadata.icon_rgba,
        icon_width: metadata.icon_width,
        icon_height: metadata.icon_height,
    }))
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_display_name(
    metadata: *const TouchHLEIOSGameMetadata,
) -> *const c_char {
    metadata
        .as_ref()
        .map_or(std::ptr::null(), |metadata| metadata.display_name.as_ptr())
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_bundle_identifier(
    metadata: *const TouchHLEIOSGameMetadata,
) -> *const c_char {
    metadata.as_ref().map_or(std::ptr::null(), |metadata| {
        metadata.bundle_identifier.as_ptr()
    })
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_orientation_capabilities(
    metadata: *const TouchHLEIOSGameMetadata,
) -> u32 {
    metadata
        .as_ref()
        .map_or(1, |metadata| metadata.orientation_capabilities)
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_icon_rgba(
    metadata: *const TouchHLEIOSGameMetadata,
) -> *const c_uchar {
    metadata
        .as_ref()
        .map_or(std::ptr::null(), |metadata| metadata.icon_rgba.as_ptr())
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_icon_width(
    metadata: *const TouchHLEIOSGameMetadata,
) -> u32 {
    metadata.as_ref().map_or(0, |metadata| metadata.icon_width)
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_icon_height(
    metadata: *const TouchHLEIOSGameMetadata,
) -> u32 {
    metadata.as_ref().map_or(0, |metadata| metadata.icon_height)
}

#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_game_metadata_free(metadata: *mut TouchHLEIOSGameMetadata) {
    if !metadata.is_null() {
        drop(Box::from_raw(metadata));
    }
}

/// Starts the game at `path`, an IPA in the host's library. The host's scale,
/// orientation, network and tilt arguments are ignored: codwawzpc's own
/// settings decide those for this game.
#[no_mangle]
pub unsafe extern "C" fn touchhle_ios_run_game(
    path: *const c_char,
    _scale_hack: i32,
    _orientation: i32,
    _network_access: i32,
    _analog_stick_tilt_controls: i32,
) -> i32 {
    if path.is_null() {
        eprintln!("codwawzpc failed: game path was null");
        return 1;
    }
    match CStr::from_ptr(path).to_str() {
        Ok(path) => run_game(Path::new(path)),
        Err(error) => {
            eprintln!("codwawzpc failed: game path was not UTF-8: {error}");
            1
        }
    }
}
