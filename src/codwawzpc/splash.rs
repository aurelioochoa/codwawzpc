/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The publisher logos at startup.
//!
//! `CGameStateSplash` shows the Activision, Treyarch and Ideaworks logos one
//! after another, each for the `SplashScreenMs` setting. Nothing loads
//! meanwhile: they add about eight idle seconds to every launch. Unless the
//! `IntroLogos` setting (`--logos`) or `CODWAWZPC_LOGOS` asks for them, the
//! state is moved to its last logo with its timer expired, so the game's next
//! update finishes it as usual and goes on to the title screen.

use super::native::{peek, poke};
use crate::Environment;

/// `CGame *`; its states are (wrapper, id) pairs at +0xb4.
const GAME: u32 = 0xa15080;
const GAME_VTABLE: u32 = 0x2b4784;
const GAME_STATE_SPLASH: u32 = 4;
const SPLASH_VTABLE: u32 = 0x2a9798;
/// `CGameStateSplash` fields: milliseconds left for the current logo, the
/// current logo's index and the logo list, which counts its logos at +0x54.
const SPLASH_TIMER: u32 = 0x10;
const SPLASH_INDEX: u32 = 0x14;
const SPLASH_LIST: u32 = 0x18;
const LIST_COUNT: u32 = 0x54;

/// Called once per presented frame.
pub(crate) fn frame(env: &mut Environment) {
    let show = super::runtime::CONFIG
        .get()
        .is_some_and(|c| c.settings.intro_logos);
    if show || std::env::var_os("CODWAWZPC_LOGOS").is_some() {
        return;
    }
    let Some(game) = peek(env, GAME).filter(|&g| g != 0 && peek(env, g) == Some(GAME_VTABLE))
    else {
        return;
    };
    if peek(env, game + 0x20) != Some(GAME_STATE_SPLASH) {
        return;
    }
    let Some(splash) = (0..8).find_map(|i| {
        let entry = game + 0xb4 + i * 8;
        let wrapper = peek(env, entry).filter(|&w| w != 0)?;
        (peek(env, entry + 4)? == GAME_STATE_SPLASH).then_some(wrapper + 0x10)
    }) else {
        return;
    };
    if peek(env, splash) != Some(SPLASH_VTABLE) {
        return;
    }
    let (Some(index), Some(count)) = (
        peek(env, splash + SPLASH_INDEX).map(|i| i as i32),
        peek(env, splash + SPLASH_LIST)
            .filter(|&l| l != 0)
            .and_then(|list| peek(env, list + LIST_COUNT))
            .map(|c| c as i32),
    ) else {
        return;
    };
    if count > 0 && index < count - 1 {
        poke(env, splash + SPLASH_INDEX, (count - 1) as u32);
        poke(env, splash + SPLASH_TIMER, 0);
        log_dbg!("Skipping the startup logos ({count})");
    }
}
