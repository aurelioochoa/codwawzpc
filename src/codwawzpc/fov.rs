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
//! and height grows, which would narrow the vertical view of every angle the
//! game sets: the view, the zoom of aiming down sights and the teleporter's.
//! All of them reach the viewport through one setter, which [install]
//! redirects to [set_view_fov]: it converts the angle to the one with the
//! same vertical view on the wider screen, then runs the game's own setter.
//! The console variable then keeps its 3:2 meaning. Should the setter not be
//! found, the variable itself is widened instead, which leaves aiming
//! narrower than on the original screen.

use super::native::{peek, poke};
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::HostFunction;
use crate::mem::MutPtr;
use crate::Environment;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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

/// `CIwViewPort::SetFOV(CIwViewPort *, iwangle fov)`, A32: stores
/// `cot(fov / 2) / 2` and marks the projection for rebuilding. The player
/// calls it every frame while its field of view eases between the
/// variable's and a weapon's `aimfov`, and when teleporting.
const SET_FOV: u32 = 0x11a840;
/// `push {r4-r6, lr}; mov r4, r0; mov r5, r1; asr r0, r1, #1`.
const SET_FOV_CODE: [u32; 4] = [0xe92d4070, 0xe1a04000, 0xe1a05001, 0xe1a000c1];
/// `ldr pc, [pc, #-4]`: jump to the address in the next word.
const A32_LDR_PC_NEXT_WORD: u32 = 0xe51ff004;
/// Full circle in the game's angle unit (`iwangle`).
const IW_ANGLE_TURN: f64 = 4096.0;

/// Whether [set_view_fov] keeps the vertical view on a wider screen.
static SETTER_CONVERTS: AtomicBool = AtomicBool::new(false);
/// The trampoline that runs the game's original setter.
static ORIGINAL_SET_FOV: AtomicU32 = AtomicU32::new(0);

/// On a wider screen, redirect the viewport's field-of-view setter to
/// [set_view_fov], once. Called when the game first opens an archive, after
/// the loader has hashed its own code.
pub(crate) fn install(env: &mut Environment) {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if !env
        .options
        .iphone_long_side
        .is_some_and(|width| width > 480)
        || INSTALLED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    let entry: MutPtr<u32> = MutPtr::from_bits(SET_FOV);
    let found: Vec<u32> = (0..4).map(|i| env.mem.read(entry + i)).collect();
    if found != SET_FOV_CODE {
        log!("Not converting the field of view of aiming: unexpected code {found:x?}");
        return;
    }
    // The two overwritten instructions, then a jump back to the third.
    let trampoline: MutPtr<u32> = env.mem.alloc(16).cast();
    for (i, word) in [
        SET_FOV_CODE[0],
        SET_FOV_CODE[1],
        A32_LDR_PC_NEXT_WORD,
        SET_FOV + 8,
    ]
    .into_iter()
    .enumerate()
    {
        env.mem.write(trampoline + i as u32, word);
    }
    ORIGINAL_SET_FOV.store(trampoline.to_bits(), Ordering::Relaxed);
    const HOST: HostFunction = &(set_view_fov as fn(&mut Environment, u32, i32));
    let host = env
        .dyld
        .create_guest_function(&mut env.mem, "codwawzpc_set_view_fov", HOST);
    env.mem.write(entry, A32_LDR_PC_NEXT_WORD);
    env.mem.write(entry + 1, host.addr_with_thumb_bit());
    env.cpu.invalidate_cache_range(SET_FOV, 8);
    SETTER_CONVERTS.store(true, Ordering::Relaxed);
    log_dbg!("Field of view angles keep the 3:2 vertical view");
}

/// Replaces the viewport's field-of-view setter on a wider screen.
fn set_view_fov(env: &mut Environment, viewport: u32, fov: i32) {
    let screen_width = env.options.iphone_long_side.unwrap_or(480);
    let converted = widescreen_angle(fov, screen_width);
    let original =
        GuestFunction::from_addr_with_thumb_bit(ORIGINAL_SET_FOV.load(Ordering::Relaxed));
    let () = original.call_from_host(env, (viewport, converted));
}

/// Apply the configured field of view. Called once per presented frame; the
/// variable is replaced whenever it holds the game's own value, so the
/// setting survives the game resetting it.
pub(crate) fn frame(env: &mut Environment) {
    let Some(fov) = env.options.game_fov else {
        return;
    };
    // The setter converts every angle itself; otherwise widen the variable.
    let screen_width = if SETTER_CONVERTS.load(Ordering::Relaxed) {
        480
    } else {
        env.options.iphone_long_side.unwrap_or(480)
    };
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

/// The angle (in `iwangle`) with the same vertical view on a screen
/// `screen_width` points wide (and 320 high) as `fov` on the 3:2 screen.
/// It is even, since the setter halves it with a shift. Angles outside
/// (0, 180) degrees are returned unchanged.
pub(crate) fn widescreen_angle(fov: i32, screen_width: u32) -> i32 {
    if screen_width == 480 || fov <= 0 || f64::from(fov) >= IW_ANGLE_TURN / 2.0 {
        return fov;
    }
    let span = f64::from(screen_width + 320) / 2.0;
    let half = std::f64::consts::PI * f64::from(fov) / IW_ANGLE_TURN;
    let half = (half.tan() * span / 400.0).atan();
    2 * (half * IW_ANGLE_TURN / (2.0 * std::f64::consts::PI)).round() as i32
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

    #[test]
    fn widescreen_angles_keep_the_vertical_view() {
        // The game's 50 degrees, and a pistol's aimfov (about 47 degrees).
        let fov_50 = (50.0 * IW_ANGLE_TURN / 360.0) as i32;
        assert_eq!(widescreen_angle(fov_50, 480), fov_50);
        assert_eq!(widescreen_angle(534, 480), 534);
        // Agrees with the degree conversion of the variable.
        let deg = |a: i32| f64::from(a) * 360.0 / IW_ANGLE_TURN;
        assert_eq!(deg(widescreen_angle(fov_50, 568)).round() as u32, 55);
        assert_eq!(deg(widescreen_angle(fov_50, 512)).round() as u32, 52);
        // Tangent of half the angle scales with the mean of width and height.
        for (fov, width) in [(534, 568), (300, 568), (534, 512), (1200, 568)] {
            let half = |a: i32| (std::f64::consts::PI * f64::from(a) / IW_ANGLE_TURN).tan();
            let ratio = half(widescreen_angle(fov, width)) / half(fov);
            let expected = f64::from(width + 320) / 800.0;
            assert!(
                (ratio - expected).abs() < 0.005,
                "{fov} at {width}: {ratio}"
            );
            assert_eq!(widescreen_angle(fov, width) % 2, 0);
        }
        assert_eq!(widescreen_angle(0, 568), 0);
        assert_eq!(widescreen_angle(2048, 568), 2048);
    }
}
