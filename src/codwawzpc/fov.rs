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
//!
//! On a wider screen (see [super::graphics::Widescreen]) the mean of width
//! and height grows, which would narrow the vertical view, so the angle is
//! widened to keep the vertical view of the original screen.

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
    let screen_width = env.options.iphone_long_side.unwrap_or(480);
    let fov = viewport_fov(fov, screen_width);
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

/// The game's field of view for a 3:2 field of view `fov` on a screen
/// `screen_width` points wide (and 320 high), with the same vertical view.
pub(crate) fn viewport_fov(fov: u32, screen_width: u32) -> u32 {
    let span = (screen_width + 320) as f64 / 2.0;
    let half = (fov as f64 / 2.0).to_radians().tan() * span / 400.0;
    (2.0 * half.atan().to_degrees()).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widescreen_keeps_the_vertical_view() {
        assert_eq!(viewport_fov(50, 480), 50);
        assert_eq!(viewport_fov(90, 480), 90);
        assert_eq!(viewport_fov(50, 568), 55);
        assert_eq!(viewport_fov(50, 512), 52);
        assert_eq!(viewport_fov(100, 568), 106);
    }
}
