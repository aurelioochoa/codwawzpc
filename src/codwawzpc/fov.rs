/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's field of view.
//!
//! The game's `CIwViewPort` holds its field of view in the console variable
//! `ViewportFOV`: 50 degrees across the mean of the screen's width and
//! height, which on the 3:2 screen is about 58 degrees horizontally and 41
//! vertically. The local player reads it when a level starts and whenever
//! aiming down sights ends, and eases the viewport towards it; aiming eases
//! towards the weapon's own `aimfov` instead, so that zoom is left as the
//! game sets it.

use super::native::{peek, poke};
use crate::Environment;

/// The game's `CIwViewPort *`.
const VIEWPORT: u32 = 0xa1a52c;
const VIEWPORT_VTABLE: u32 = 0x2a39d0;
/// `CIwViewPort` field of view, a `CIwConsoleVarInt *` named `ViewportFOV`.
const VIEWPORT_FOV_VAR: u32 = 0xc0;
const CONSOLE_VAR_INT_VTABLE: u32 = 0x2a9bd4;
/// `CIwConsoleVarInt` value.
const CONSOLE_VAR_INT_VALUE: u32 = 0x10;
/// The field of view the game's data sets, in degrees.
pub(crate) const ORIGINAL_FOV: u32 = 50;

/// Apply the configured field of view. Called once per presented frame; the
/// variable is replaced whenever it holds the game's own value, so the
/// setting survives the game resetting it.
pub(crate) fn frame(env: &mut Environment) {
    let Some(fov) = env.options.game_fov else {
        return;
    };
    if fov == ORIGINAL_FOV {
        return;
    }
    let Some(var) = peek(env, VIEWPORT)
        .filter(|&v| v != 0 && peek(env, v) == Some(VIEWPORT_VTABLE))
        .and_then(|v| peek(env, v + VIEWPORT_FOV_VAR))
        .filter(|&var| var != 0 && peek(env, var) == Some(CONSOLE_VAR_INT_VTABLE))
    else {
        return;
    };
    if peek(env, var + CONSOLE_VAR_INT_VALUE) == Some(ORIGINAL_FOV) {
        poke(env, var + CONSOLE_VAR_INT_VALUE, fov);
        log_dbg!("Viewport field of view set to {} degrees", fov);
    }
}
