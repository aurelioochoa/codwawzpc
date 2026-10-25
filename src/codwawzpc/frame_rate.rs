/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's frame-rate cap.
//!
//! The game paces itself with IwGame's `CIwGameClock`: a frame starts only
//! once the clock's minimum frame time has passed since the previous one,
//! and each update is given the measured time, clamped to the clock's
//! maximum step. The game's data sets 5 to 45 FPS, so the minimum frame time
//! is 22 ms and play runs at 45 FPS while the clock busy-waits on
//! `s3eDeviceYield(0)` and `s3eTimerGetMs()` for the rest of each frame.
//!
//! Updates are driven by the measured time, so a shorter minimum frame time
//! raises the frame rate without speeding the game up. touchHLE's 60 Hz
//! presentation limit then paces frames instead of the busy-wait.

use super::native::{peek, poke};
use crate::Environment;

/// `CIwGame::m_Clock`, the game's `CIwGameClock *`.
const CLOCK: u32 = 0xa15088;
const CLOCK_VTABLE: u32 = 0x2a476c;
/// `CIwGameClock` minimum frame time in milliseconds.
const CLOCK_MIN_FRAME_MS: u32 = 0x10;
/// The minimum frame time the game's data sets: 1000 / 45.
const ORIGINAL_MIN_FRAME_MS: u32 = 22;

/// Apply the configured cap. Called once per presented frame; the game sets
/// its clock when it starts and when a level or menu changes it, so the
/// value is replaced whenever it is the game's own.
pub(crate) fn frame(env: &mut Environment) {
    let Some(rate) = env.options.game_frame_rate else {
        return;
    };
    let min_frame_ms = 1000 / rate;
    if min_frame_ms == ORIGINAL_MIN_FRAME_MS {
        return;
    }
    let Some(clock) = peek(env, CLOCK).filter(|&c| c != 0 && peek(env, c) == Some(CLOCK_VTABLE))
    else {
        return;
    };
    if peek(env, clock + CLOCK_MIN_FRAME_MS) == Some(ORIGINAL_MIN_FRAME_MS) {
        poke(env, clock + CLOCK_MIN_FRAME_MS, min_frame_ms);
        log_dbg!("Game clock minimum frame time set to {} ms", min_frame_ms);
    }
}
