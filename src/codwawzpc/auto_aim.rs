/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's auto-aim.
//!
//! The game's Options → Controls → Auto-Aim toggle steps a level kept in the
//! saved game's control settings through 0, 0.5 and 1 (shown as Low, Medium
//! and High), and sets the console
//! variables `AutoAimWeightAngle`, `AutoAimWeightRange` and
//! `AutoAimScoreRelease` from it, between their `…Min` and `…Max` values.
//! The game does not do this when a level starts, so it is done here for each
//! new local player and whenever the level is restored. `CAutoAim` scores
//! each enemy from those weights (lower is better), locks on to the best one
//! only when its score is below `AutoAimScoreAcquire`, and keeps it while the
//! score stays below `AutoAimScoreRelease`. The game has no off: Low only
//! makes locking on harder. [AutoAim::Off] is Low with the acquire and
//! release scores set below any possible score, so the game never locks on
//! and drops any target at once. Without a target `CAutoAim` eases its aim
//! point back to the screen centre and the player's lock-on spring does not
//! turn the view, so nothing assists aiming.

use super::native::{peek, poke};
use crate::Environment;
use std::sync::atomic::{AtomicU32, Ordering};

/// Auto-aim setting, applied instead of the in-game toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoAim {
    /// The game's Low, never locking on.
    Off,
    /// The game's own levels.
    Low,
    Medium,
    High,
}

impl AutoAim {
    pub const NAMES: [(&'static str, AutoAim); 4] = [
        ("off", AutoAim::Off),
        ("low", AutoAim::Low),
        ("medium", AutoAim::Medium),
        ("high", AutoAim::High),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES.iter().find(|(_, v)| *v == self).unwrap().0
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, v)| v)
    }

    /// The in-game toggle's level.
    fn level(self) -> f32 {
        match self {
            AutoAim::Off | AutoAim::Low => 0.0,
            AutoAim::Medium => 0.5,
            AutoAim::High => 1.0,
        }
    }
}

/// `CSaveManager *`; holds the saved control settings.
const SAVE_MANAGER: u32 = 0xa1a558;
/// Saved auto-aim level, a float.
const SAVE_MANAGER_AUTO_AIM_LEVEL: u32 = 0x98;
/// `CLevelManager *`; at +8 is the local player `CEnt`.
const LEVEL_MANAGER: u32 = 0xa1a54c;
const ENT_VTABLE: u32 = 0x2b46dc;
/// `CEnt` player component, a `CPlayerLocal *`.
const ENT_PLAYER: u32 = 0x98;
const PLAYER_LOCAL_VTABLE: u32 = 0x2a4c98;
/// `CPlayerLocal` console variables: the current, `…Max` and `…Min`
/// `AutoAimWeightAngle`, `AutoAimWeightRange` and `AutoAimScoreRelease`.
const PLAYER_AUTO_AIM_VARS: u32 = 0xac;
/// `CEnt` auto-aim component, a `CAutoAim *`.
const ENT_AUTO_AIM: u32 = 0x94;
const AUTO_AIM_VTABLE: u32 = 0x2b4f88;
/// `CAutoAim` lock-on thresholds, `CIwConsoleVarInt *`s named
/// `AutoAimScoreAcquire` and `AutoAimScoreRelease`.
const AUTO_AIM_SCORE_VARS: [u32; 2] = [0xe8, 0xec];
const CONSOLE_VAR_INT_VTABLE: u32 = 0x2a9bd4;
/// `CIwConsoleVarFixed`, which keeps its value where `CIwConsoleVarInt` does.
const CONSOLE_VAR_FIXED_VTABLE: u32 = 0x2ac55c;
/// `CIwConsoleVarInt` value.
const CONSOLE_VAR_INT_VALUE: u32 = 0x10;
/// Score threshold that no enemy's score is below (scores compare signed).
const NEVER: u32 = i32::MIN as u32;

/// The `CPlayerLocal` whose console variables were last set.
static PLAYER_SET: AtomicU32 = AtomicU32::new(0);

/// Apply the configured auto-aim. Called once per presented frame, so the
/// setting survives the game loading the saved level, each level setting its
/// own acquire score, and the game setting the release score.
pub(crate) fn frame(env: &mut Environment) {
    let Some(setting) = env.options.game_auto_aim else {
        return;
    };
    let mut restored = false;
    if let Some(save_manager) = peek(env, SAVE_MANAGER).filter(|&v| v != 0) {
        let level = save_manager + SAVE_MANAGER_AUTO_AIM_LEVEL;
        if peek(env, level).is_some_and(|v| v != setting.level().to_bits()) {
            poke(env, level, setting.level().to_bits());
            log_dbg!("Auto-aim level set to {:?}", setting);
            restored = true;
        }
    }
    let Some(ent) = peek(env, LEVEL_MANAGER)
        .filter(|&v| v != 0)
        .and_then(|v| peek(env, v + 8))
        .filter(|&ent| ent != 0 && peek(env, ent) == Some(ENT_VTABLE))
    else {
        return;
    };
    if let Some(player) =
        peek(env, ent + ENT_PLAYER).filter(|&p| p != 0 && peek(env, p) == Some(PLAYER_LOCAL_VTABLE))
    {
        if restored || PLAYER_SET.load(Ordering::Relaxed) != player {
            set_player_vars(env, player, setting.level());
        }
    }
    if setting != AutoAim::Off {
        return;
    }
    let Some(aim) = peek(env, ent + ENT_AUTO_AIM)
        .filter(|&aim| aim != 0 && peek(env, aim) == Some(AUTO_AIM_VTABLE))
    else {
        return;
    };
    for offset in AUTO_AIM_SCORE_VARS {
        let Some(var) = peek(env, aim + offset)
            .filter(|&var| var != 0 && peek(env, var) == Some(CONSOLE_VAR_INT_VTABLE))
        else {
            continue;
        };
        if peek(env, var + CONSOLE_VAR_INT_VALUE).is_some_and(|v| v != NEVER) {
            poke(env, var + CONSOLE_VAR_INT_VALUE, NEVER);
            log_dbg!("Auto-aim lock-on disabled");
        }
    }
}

/// Set the player's current weights and release score from `level`, as the
/// in-game toggle does.
fn set_player_vars(env: &mut Environment, player: u32, level: f32) {
    let mut vars = [0; 9];
    for (i, var) in vars.iter_mut().enumerate() {
        let Some(v) = peek(env, player + PLAYER_AUTO_AIM_VARS + 4 * i as u32).filter(|&v| {
            v != 0
                && matches!(
                    peek(env, v),
                    Some(CONSOLE_VAR_INT_VTABLE | CONSOLE_VAR_FIXED_VTABLE)
                )
        }) else {
            return;
        };
        *var = v;
    }
    let value =
        |env: &Environment, var: u32| peek(env, var + CONSOLE_VAR_INT_VALUE).unwrap() as i32;
    for i in 0..3 {
        let (max, min) = (value(env, vars[3 + i]), value(env, vars[6 + i]));
        let current = min + (level * max.wrapping_sub(min) as f32) as i32;
        poke(env, vars[i] + CONSOLE_VAR_INT_VALUE, current as u32);
    }
    PLAYER_SET.store(player, Ordering::Relaxed);
    log_dbg!("Auto-aim weights set for level {}", level);
}
