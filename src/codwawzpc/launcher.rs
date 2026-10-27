/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Locate and launch the native Python front end without a cwd dependency.
use std::path::PathBuf;

pub fn open() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("Executable has no parent")?;
    if parent.join(".codwawzpc-portable").is_file() {
        let status = std::process::Command::new(parent.join("Launch"))
            .status()
            .map_err(|e| format!("Cannot start portable launcher: {e}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("Launcher exited with {status}"))
        };
    }
    let script = [
        parent.join("launcher/main.py"),
        parent.join("../share/codwawzpc/launcher/main.py"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("launcher/main.py"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or("Launcher resources were not found. Install launcher/ beside the executable or under share/codwawzpc/.")?;
    let python = std::env::var_os("CODWAWZPC_PYTHON").unwrap_or_else(|| "python3".into());
    let status = std::process::Command::new(python)
        .arg(script)
        .arg("--binary")
        .arg(exe)
        .status()
        .map_err(|e| format!("Cannot start launcher: {e}. Install Python 3, Tk and Pillow."))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Launcher exited with {status}"))
    }
}
