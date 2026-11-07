/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Native desktop input: the game's own input events instead of touches.
//!
//! COD Zombies keeps the keyboard controller of its desktop builds
//! (`CKeyboardInput`, disabled unless its config sets `EnableKeyboardInput`).
//! It and the touch HUD drive the player the same way: they post
//! `Event_AnaloguePadPosition` for movement and `Event_PlayerAction` for
//! firing, reloading and the other actions to the local player entity. Once
//! per presented frame, [frame] posts those events through the keyboard
//! controller's own functions, writes mouse look into the player's input
//! controller, and hides the on-screen touch controls.
//!
//! Every address belongs to the one executable that `import` accepts (see
//! [super::profile]), and objects are checked by vtable before use.

use crate::abi::{CallFromHost, GuestFunction};
use crate::codwawz_controls::Frame;
use crate::mem::{MutVoidPtr, Ptr};
use crate::Environment;

/// Code and global variables of the supported executable.
mod addr {
    /// `(unused, const int *state, const short vector[2], bool look)`:
    /// posts `Event_AnaloguePadPosition` to the player. State 2 posts the
    /// vector as given.
    pub const POST_PAD_POSITION: u32 = 0x158664;
    /// `(unused, int action)`: posts `Event_PlayerAction` to the player.
    pub const POST_PLAYER_ACTION: u32 = 0x15893c;
    /// `CGame *`.
    pub const GAME: u32 = 0xa15080;
    /// `CLevelManager *`; at +8 is the local player `CEnt`, which receives
    /// input events.
    pub const LEVEL_MANAGER: u32 = 0xa1a54c;
    /// Game options; at +0x84 are the control settings.
    pub const OPTIONS: u32 = 0xa1a558;
    /// `CIwGuiManager *`.
    pub const GUI_MANAGER: u32 = 0xa1a548;
    /// Camera settings, with a look range per player state.
    pub const CAMERAS: u32 = 0xa1a5b4;
}

mod vtable {
    pub const GAME: u32 = 0x2b4784;
    pub const GAME_STATE_INGAME: u32 = 0x2a9754;
    pub const ENT: u32 = 0x2b46dc;
    pub const PLAYER_LOCAL: u32 = 0x2a4c98;
    pub const PLAYER_INPUT_CONTROLLER_LOCAL: u32 = 0x2b3204;
    pub const GUI_FRAME: u32 = 0x2a3550;
    pub const GUI_FRAME_HUD: u32 = 0x2a402c;
    pub const GUI_TOUCH_ANALOGUE_PAD: u32 = 0x2ae414;
    pub const GUI_TOUCH_BUTTON_ICON: u32 = 0x2ad1d8;
    pub const GUI_TOUCH_GESTURE_AREA: u32 = 0x2ae4a8;
}

/// `CGame` top-level state while in a level.
const GAME_STATE_INGAME: u32 = 2;
/// `CGameStateIngame` sub-state while playing (not paused, not game over).
const INGAME_STATE_PLAYING: u32 = 0;
/// `CPlayerLocal` states that matter here (others: knife, grenade, …).
const PLAYER_STATE_WALK: u32 = 0;
const PLAYER_STATE_RUN: u32 = 1;
const PLAYER_STATE_AIM: u32 = 2;
/// `Event_PlayerAction` numbers not covered by [crate::codwawz_controls::Action].
const ACTION_FIRE: u32 = 3;
const ACTION_FIRE_HELD: u32 = 4;
const ACTION_TOGGLE_AIM: u32 = 5;
const ACTION_RUN: u32 = 6;
/// Full deflection of an analogue pad.
const PAD_FULL: f32 = 4096.0;
/// `CIwGuiElement` flag: drawn.
const GUI_VISIBLE: u32 = 0x8;

/// Host-side state kept between frames.
#[derive(Default)]
pub(crate) struct State {
    /// Guest scratch memory for [addr::POST_PAD_POSITION]'s arguments.
    scratch: Option<MutVoidPtr>,
    /// Sub-unit look not yet applied.
    look_remainder: (f32, f32),
    menu: super::menu::State,
}

/// The local player's components.
#[derive(Clone, Copy)]
struct Player {
    controller: u32,
    local: u32,
}

/// Apply desktop input for one frame. Called when the game presents a
/// frame, so per-frame input (movement, held fire) is posted once per game
/// update.
pub(crate) fn frame(env: &mut Environment) {
    log_load_milestones(env);
    if !env.options.desktop_controls {
        return;
    }
    let player = playing_player(env);
    if let Some(player) = player {
        log_dbg!(
            "Player state {:?}, pitch {:?}",
            peek(env, player.local + 0xf0),
            peek(env, player.controller + 0x20).map(|p| p as i32)
        );
    }
    let Some(window) = env.window.as_mut() else {
        return;
    };
    let input = window.desktop_frame(player.is_some());
    let mut state = std::mem::take(&mut window.native_state);
    if let Some(player) = player {
        hide_touch_controls(env);
        if let Some(input) = input {
            apply(env, &mut state, player, input);
        }
    } else {
        state.look_remainder = (0.0, 0.0);
        super::menu::frame(env, &mut state.menu);
    }
    env.window.as_mut().unwrap().native_state = state;
}

fn apply(env: &mut Environment, state: &mut State, player: Player, input: Frame) {
    let player_state = peek(env, player.local + 0xf0).unwrap_or(PLAYER_STATE_WALK);
    // Keys give full deflection, diagonals included; a stick gives its own.
    let (x, y) = movement_for_state(player_state, &input);
    let magnitude = (x * x + y * y).sqrt();
    if magnitude > 0.0 || player_state == PLAYER_STATE_RUN {
        let scale = PAD_FULL / magnitude.max(1.0);
        post_pad_position(env, state, x * scale, y * scale);
    }

    if input.fire_pressed {
        post_action(env, ACTION_FIRE);
    } else if input.fire_held {
        post_action(env, ACTION_FIRE_HELD);
    }

    // Events dispatch synchronously and set animation inputs for the next
    // player update. A weapon animation can reject a stance change: retry
    // on the next frame, without adding a host-side cooldown. Reposting
    // the same animation input does not queue another transition.
    // Running needs forward movement fast enough, which the game flags.
    let can_run = peek_u8(env, player.local + 0xd6) == Some(1);
    if let Some(action) = stance_action(player_state, can_run, &input) {
        post_action(env, action);
    }

    for action in input.actions {
        post_action(env, action as u32);
    }

    look(env, state, player, input.look);
}

fn want_run(input: &Frame) -> bool {
    input.sprint && input.movement.1 > 0.0 && !input.aim
}

/// Action 6 starts sprint; it does not stop it. The original game exits
/// sprint when the movement pad is released. Deliver that release until
/// it leaves RUN, then restore the held direction at walking/aiming speed.
fn movement_for_state(player_state: u32, input: &Frame) -> (f32, f32) {
    if player_state == PLAYER_STATE_RUN && !want_run(input) {
        (0.0, 0.0)
    } else {
        input.movement
    }
}

/// Aim takes priority over sprint. Wait for the original game to leave RUN
/// through the movement release before requesting aim.
fn stance_action(player_state: u32, can_run: bool, input: &Frame) -> Option<u32> {
    let aiming = player_state == PLAYER_STATE_AIM;
    let can_aim = player_state == PLAYER_STATE_WALK;
    if (input.aim && can_aim) || (!input.aim && aiming) {
        return Some(ACTION_TOGGLE_AIM);
    }
    if want_run(input) && can_run && player_state == PLAYER_STATE_WALK {
        return Some(ACTION_RUN);
    }
    None
}

/// Mouse look. Yaw is a per-frame delta the player update consumes; pitch is
/// absolute and limited to the camera's range for the player state, as the
/// game's own look handler does.
fn look(env: &mut Environment, state: &mut State, player: Player, (yaw, pitch): (f32, f32)) {
    let invert = game_inverts_look(env);
    let total = (
        state.look_remainder.0 + yaw,
        state.look_remainder.1 + if invert { -pitch } else { pitch },
    );
    let whole = (total.0.trunc(), total.1.trunc());
    state.look_remainder = (total.0 - whole.0, total.1 - whole.1);
    let (yaw, pitch) = (whole.0 as i32, whole.1 as i32);

    if yaw != 0 {
        if let Some(current) = peek(env, player.controller + 0x24) {
            poke(
                env,
                player.controller + 0x24,
                (current as i32).wrapping_add(yaw) as u32,
            );
        }
    }
    if pitch != 0 {
        let Some(current) = peek(env, player.controller + 0x20).map(|v| v as i32) else {
            return;
        };
        let player_state = peek(env, player.local + 0xf0).unwrap_or(PLAYER_STATE_WALK);
        let Some((low, high)) = pitch_range(env, player_state) else {
            return;
        };
        let new = if pitch > 0 {
            (current < high).then(|| (current + pitch).min(high))
        } else {
            (current > low).then(|| (current + pitch).max(low))
        };
        if let Some(new) = new {
            poke(env, player.controller + 0x20, new as u32);
        } else {
            state.look_remainder.1 = 0.0;
        }
    }
}

/// Log `CGame` state changes and the first playable frame of each level, to
/// measure loading.
fn log_load_milestones(env: &Environment) {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    static GAME_STATE: AtomicU32 = AtomicU32::new(u32::MAX);
    static PLAYING: AtomicBool = AtomicBool::new(false);
    let game = peek(env, addr::GAME).filter(|&g| vtable_is(env, g, vtable::GAME));
    let state = game.and_then(|g| peek(env, g + 0x20)).unwrap_or(u32::MAX);
    if GAME_STATE.swap(state, Ordering::Relaxed) != state {
        crate::codwawzpc::metrics::milestone(format_args!("game state {state}"));
    }
    let playing = playing_player(env).is_some();
    if playing && !PLAYING.load(Ordering::Relaxed) {
        crate::codwawzpc::metrics::milestone(format_args!("playing"));
    }
    if state != GAME_STATE_INGAME || playing {
        PLAYING.store(playing, Ordering::Relaxed);
    }
}

/// The local player, if the game is in a level and playing.
fn playing_player(env: &Environment) -> Option<Player> {
    let game = peek(env, addr::GAME).filter(|&g| vtable_is(env, g, vtable::GAME))?;
    if peek(env, game + 0x20)? != GAME_STATE_INGAME {
        return None;
    }
    // The game's states are (wrapper, id) pairs; each wrapper holds its
    // state object at +0x10.
    let ingame = (0..8).find_map(|i| {
        let entry = game + 0xb4 + i * 8;
        let wrapper = peek(env, entry)?;
        (wrapper != 0 && peek(env, entry + 4)? == GAME_STATE_INGAME).then_some(wrapper + 0x10)
    })?;
    if !vtable_is(env, ingame, vtable::GAME_STATE_INGAME)
        || peek(env, ingame + 0x10)? != INGAME_STATE_PLAYING
    {
        return None;
    }
    let level = peek(env, addr::LEVEL_MANAGER).filter(|&l| l != 0)?;
    let ent = peek(env, level + 8).filter(|&e| vtable_is(env, e, vtable::ENT))?;
    let controller = peek(env, ent + 0xa8)
        .filter(|&c| vtable_is(env, c, vtable::PLAYER_INPUT_CONTROLLER_LOCAL))?;
    let local = peek(env, ent + 0x98).filter(|&p| vtable_is(env, p, vtable::PLAYER_LOCAL))?;
    Some(Player { controller, local })
}

/// Stop drawing the sticks and touch buttons of the current HUD layout.
/// The interaction prompt, grenade count and the rest of the HUD stay.
fn hide_touch_controls(env: &mut Environment) {
    let Some(layout) = current_hud_layout(env) else {
        return;
    };
    let mut pending = vec![(layout, 0)];
    while let Some((frame, depth)) = pending.pop() {
        let (Some(children), Some(count)) = (peek(env, frame + 0x50), peek(env, frame + 0x54))
        else {
            continue;
        };
        for i in 0..count.min(64) {
            let Some(child) = peek(env, children + i * 4) else {
                break;
            };
            let Some(class) = peek(env, child) else {
                continue;
            };
            match class {
                vtable::GUI_TOUCH_ANALOGUE_PAD
                | vtable::GUI_TOUCH_BUTTON_ICON
                | vtable::GUI_TOUCH_GESTURE_AREA => {
                    if let Some(flags) = peek(env, child + 0xc) {
                        if flags & GUI_VISIBLE != 0 {
                            poke(env, child + 0xc, flags & !GUI_VISIBLE);
                        }
                    }
                }
                vtable::GUI_FRAME if depth < 3 => pending.push((child, depth + 1)),
                _ => (),
            }
        }
    }
}

/// The touch control frame the HUD currently shows (dual stick A or B, or
/// accelerometer, as chosen in the game's options).
fn current_hud_layout(env: &Environment) -> Option<u32> {
    // The root GUI element is embedded in the manager.
    let root = peek(env, addr::GUI_MANAGER).filter(|&m| m != 0)? + 0xb8;
    let (frames, count) = (peek(env, root + 0x50)?, peek(env, root + 0x54)?);
    let hud = (0..count.min(256)).find_map(|i| {
        let frame = peek(env, frames + i * 4)?;
        vtable_is(env, frame, vtable::GUI_FRAME_HUD).then_some(frame)
    })?;
    peek(env, hud + 0x9c).filter(|&layout| layout != 0)
}

/// The pitch limits of the camera for a player state.
fn pitch_range(env: &Environment, player_state: u32) -> Option<(i32, i32)> {
    let cameras = peek(env, addr::CAMERAS).filter(|&c| c != 0)?;
    let camera = peek(env, cameras + 0x28 + player_state.min(16) * 4).filter(|&c| c != 0)?;
    let (points, count) = (peek(env, camera + 0x10)?, peek(env, camera + 0x14)?);
    let pitches: Option<Vec<i32>> = (0..count.min(16))
        .map(|i| peek(env, points + i * 8 + 4).map(|p| p as i32))
        .collect();
    let pitches = pitches?;
    Some((*pitches.iter().min()?, *pitches.iter().max()?))
}

/// The "Invert Y Axis" setting of the game's own options.
fn game_inverts_look(env: &Environment) -> bool {
    (|| {
        let controls = peek(env, addr::OPTIONS).filter(|&o| o != 0)? + 0x84;
        let mut scheme = peek_u8(env, controls + 9)?;
        if scheme == 3 {
            scheme = 2;
        }
        let table = peek(env, controls + 0x24)?;
        Some(peek_u8(env, table + u32::from(scheme) * 10 + 8)? != 0)
    })()
    .unwrap_or(false)
}

fn post_pad_position(env: &mut Environment, state: &mut State, x: f32, y: f32) {
    let scratch = *state.scratch.get_or_insert_with(|| env.mem.alloc(8));
    let base = scratch.to_bits();
    env.mem.write(Ptr::from_bits(base), 2i32);
    env.mem.write(Ptr::from_bits(base + 4), x.round() as i16);
    env.mem.write(Ptr::from_bits(base + 6), y.round() as i16);
    let post = GuestFunction::from_addr_with_thumb_bit(addr::POST_PAD_POSITION);
    let () = post.call_from_host(env, (0u32, base, base + 4, false));
}

fn post_action(env: &mut Environment, action: u32) {
    if action != ACTION_FIRE_HELD {
        log_dbg!("Player action {}", action);
    }
    let post = GuestFunction::from_addr_with_thumb_bit(addr::POST_PLAYER_ACTION);
    let () = post.call_from_host(env, (0u32, action));
}

fn vtable_is(env: &Environment, object: u32, vtable: u32) -> bool {
    object != 0 && peek(env, object) == Some(vtable)
}

/// Read a guest word, or [None] if the address is not readable.
pub(super) fn peek(env: &Environment, addr: u32) -> Option<u32> {
    let bytes = env.mem.get_bytes_fallible(Ptr::from_bits(addr), 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn peek_u8(env: &Environment, addr: u32) -> Option<u8> {
    env.mem
        .get_bytes_fallible(Ptr::from_bits(addr), 1)
        .map(|bytes| bytes[0])
}

pub(super) fn poke(env: &mut Environment, addr: u32, value: u32) {
    if let Some(bytes) = env.mem.get_bytes_fallible_mut(Ptr::from_bits(addr), 4) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_aim_retries_after_a_rejected_weapon_transition() {
        let mut input = Frame {
            aim: true,
            ..Frame::default()
        };
        // A shot's animation can leave the player walking after an aim
        // request. The next frame must retry, with no one-second delay.
        for _ in 0..3 {
            assert_eq!(
                stance_action(PLAYER_STATE_WALK, false, &input),
                Some(ACTION_TOGGLE_AIM)
            );
        }
        assert_eq!(stance_action(PLAYER_STATE_AIM, false, &input), None);
        input.aim = false;
        assert_eq!(
            stance_action(PLAYER_STATE_AIM, false, &input),
            Some(ACTION_TOGGLE_AIM)
        );
        assert_eq!(stance_action(PLAYER_STATE_WALK, false, &input), None);
        input.aim = true;
        assert_eq!(
            stance_action(PLAYER_STATE_WALK, false, &input),
            Some(ACTION_TOGGLE_AIM)
        );
    }

    #[test]
    fn aiming_from_sprint_stops_running_before_requesting_aim() {
        let input = Frame {
            aim: true,
            sprint: true,
            movement: (0.0, 1.0),
            ..Frame::default()
        };
        assert_eq!(movement_for_state(PLAYER_STATE_RUN, &input), (0.0, 0.0));
        assert_eq!(stance_action(PLAYER_STATE_RUN, true, &input), None);
        assert_eq!(movement_for_state(PLAYER_STATE_WALK, &input), (0.0, 1.0));
        assert_eq!(
            stance_action(PLAYER_STATE_WALK, true, &input),
            Some(ACTION_TOGGLE_AIM)
        );
        assert_eq!(stance_action(PLAYER_STATE_AIM, true, &input), None);
    }

    #[test]
    fn sprint_waits_for_forward_movement_and_the_game_permission() {
        let mut input = Frame {
            sprint: true,
            ..Frame::default()
        };
        assert_eq!(stance_action(PLAYER_STATE_WALK, true, &input), None);
        input.movement.1 = 1.0;
        assert_eq!(stance_action(PLAYER_STATE_WALK, false, &input), None);
        assert_eq!(
            stance_action(PLAYER_STATE_WALK, true, &input),
            Some(ACTION_RUN)
        );
        assert_eq!(stance_action(PLAYER_STATE_RUN, true, &input), None);
        assert_eq!(movement_for_state(PLAYER_STATE_RUN, &input), (0.0, 1.0));
        input.sprint = false;
        assert_eq!(stance_action(PLAYER_STATE_RUN, true, &input), None);
        assert_eq!(movement_for_state(PLAYER_STATE_RUN, &input), (0.0, 0.0));
        assert_eq!(movement_for_state(PLAYER_STATE_WALK, &input), (0.0, 1.0));
        // Knife, grenade and other game states keep their own rules.
        for state in 3..=8 {
            assert_eq!(stance_action(state, true, &input), None);
            input.aim = true;
            assert_eq!(stance_action(state, true, &input), None);
            input.aim = false;
        }
    }
}
