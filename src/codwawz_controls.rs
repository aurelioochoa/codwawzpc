/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Desktop keyboard, mouse and game controller state for COD Zombies'
//! native input.
//!
//! The game has its own input events: `Event_AnaloguePadPosition` moves the
//! player and `Event_PlayerAction` carries actions such as firing or
//! reloading (see [crate::codwawzpc::native], which delivers them). This
//! module turns held keys, buttons, sticks and mouse motion into one [Frame]
//! of those inputs per game frame; mouse and stick look become yaw and pitch
//! in the game's angle units.
//!
//! The menus are touch-only, so outside play the keyboard and controller
//! produce [MenuCommand]s (see [crate::codwawzpc::menu], which turns them
//! into touches on the game's buttons), and the pause and back buttons are
//! touched through [Controls::pause] and [Controls::back].
//!
//! The state machine is independent of SDL and of the guest, and takes the
//! time as input, so it is testable on its own.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

pub(crate) type Coords = (f32, f32);

/// Menu buttons in points of the landscape screen, from its top left.
mod hud {
    use super::Coords;
    /// The score, which opens the pause menu.
    pub const PAUSE: Coords = (19.0, 20.0);
    /// The Back button shared by the game's menus.
    pub const BACK: Coords = (22.0, 296.0);
}

/// Game angle units in a full turn.
pub(crate) const FULL_TURN: f32 = 4096.0;
/// Degrees of look per mouse count at sensitivity 1.
const DEGREES_PER_COUNT: f32 = 0.05;
/// Degrees of look per second with the right stick fully deflected, at
/// sensitivity 1.
const STICK_DEGREES_PER_SECOND: f32 = 180.0;
/// Stick deflection ignored as noise (radial, 0–1).
const MOVE_DEADZONE: f32 = 0.2;
const LOOK_DEADZONE: f32 = 0.15;
/// Trigger travel that presses and releases fire or aim.
const TRIGGER_PRESS: f32 = 0.5;
const TRIGGER_RELEASE: f32 = 0.3;
/// Stick deflection that presses and releases a menu direction.
const NAV_PRESS: f32 = 0.6;
const NAV_RELEASE: f32 = 0.4;
/// Forward stick deflection below which a clicked-in sprint stops.
const SPRINT_RELEASE: f32 = 0.3;
/// A held menu direction repeats after this delay, at this interval.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);
/// Longest frame time stick look integrates over, so a stall does not spin
/// the view.
const MAX_FRAME_TIME: f32 = 0.1;
/// Duration of a menu tap.
const TAP: Duration = Duration::from_millis(60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum Key {
    Forward,
    Left,
    Back,
    Right,
    Sprint,
    Reload,
    Interact,
    Grenade,
    Special,
    Melee,
    NextWeapon,
}
const KEY_COUNT: usize = Key::NextWeapon as usize + 1;
const ALL_KEYS: [Key; KEY_COUNT] = [
    Key::Forward,
    Key::Left,
    Key::Back,
    Key::Right,
    Key::Sprint,
    Key::Reload,
    Key::Interact,
    Key::Grenade,
    Key::Special,
    Key::Melee,
    Key::NextWeapon,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum Button {
    Fire,
    Aim,
}

/// Game controller buttons, in SDL's standard (Xbox) layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PadButton {
    A,
    B,
    X,
    Y,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    Start,
    Back,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Game controller axes. Sticks are -1 to 1 (down and right positive),
/// triggers 0 to 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum PadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}
const AXIS_COUNT: usize = PadAxis::RightTrigger as usize + 1;

/// What a controller button asks the window to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PadResponse {
    None,
    /// Same as the Escape key: pause during play, Back in menus.
    Escape,
}

/// Where input comes from. Keys and buttons held from both are released
/// only when both let go.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum Source {
    Desktop,
    Pad,
}

/// One-shot player actions, with the game's `Event_PlayerAction` numbers.
/// They are the numbers its touch buttons send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum Action {
    /// The hand button pressed: start using what is in front of the player.
    InteractStart = 1,
    /// The hand button released.
    InteractEnd = 2,
    Reload = 7,
    /// The grenade button pressed: take out (cook) a grenade.
    GrenadeStart = 8,
    /// The grenade button released: throw it.
    GrenadeThrow = 9,
    /// Select the next special grenade (monkey bomb, Bouncing Betty).
    CycleSpecial = 10,
    Melee = 11,
    SwapWeapon = 12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum Dir {
    Up,
    Down,
    Left,
    Right,
}
const ALL_DIRS: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

/// Where a held menu direction comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum NavSource {
    Keyboard,
    DPad,
    Stick,
}
const NAV_SOURCES: usize = 3;

/// Keyboard and controller menu input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuCommand {
    /// Focus the next button in a direction, or adjust a slider.
    Move(Dir),
    /// Press the focused button.
    Activate,
}

/// Finger used for menu taps.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Finger {
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Touch {
    pub finger: Finger,
    pub phase: Phase,
    pub coords: Coords,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LookSettings {
    /// Multiplier for mouse and stick look speed.
    pub sensitivity: f32,
    pub invert_y: bool,
}
impl Default for LookSettings {
    fn default() -> Self {
        Self {
            sensitivity: 1.0,
            invert_y: false,
        }
    }
}

/// Input for one game frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    /// Strafe (right positive) and forward movement, within the unit
    /// circle once normalized: keys give -1, 0 or 1 per axis, the left stick
    /// anything in between.
    pub movement: (f32, f32),
    /// Look in game angle units ([FULL_TURN] per turn): yaw (right
    /// positive) and pitch (up positive).
    pub look: (f32, f32),
    /// Fire was pressed since the last frame.
    pub fire_pressed: bool,
    /// Fire is held (automatic weapons keep firing).
    pub fire_held: bool,
    /// Aim down the sights while true.
    pub aim: bool,
    /// Sprint while true.
    pub sprint: bool,
    /// One-shot actions, in input order.
    pub actions: Vec<Action>,
}

pub(crate) struct Controls {
    look: LookSettings,
    bindings: crate::codwawzpc::bindings::Bindings,
    physical_held: HashSet<String>,
    /// Play or menus: controller buttons mean different things.
    gameplay: bool,
    keys: [[bool; KEY_COUNT]; 2],
    buttons: [[bool; 2]; 2],
    fire_pressed: bool,
    pending_look: (f32, f32),
    actions: Vec<Action>,
    axes: [f32; AXIS_COUNT],
    /// Sprint clicked in with the left stick; lasts while moving forward.
    pad_sprint: bool,
    last_frame: Option<Instant>,
    nav_held: [[bool; NAV_SOURCES]; 4],
    nav_repeat: [Option<Instant>; 4],
    menu_commands: Vec<MenuCommand>,
    /// The keyboard or controller was used in the menus more recently than
    /// the mouse, so the focused button is shown.
    menu_focus_shown: bool,
    taps: VecDeque<(Instant, Touch)>,
}

impl Controls {
    pub fn new(look: LookSettings) -> Self {
        Self {
            look,
            bindings: Default::default(),
            physical_held: HashSet::new(),
            gameplay: false,
            keys: [[false; KEY_COUNT]; 2],
            buttons: [[false; 2]; 2],
            fire_pressed: false,
            pending_look: (0.0, 0.0),
            actions: Vec::new(),
            axes: [0.0; AXIS_COUNT],
            pad_sprint: false,
            last_frame: None,
            nav_held: [[false; NAV_SOURCES]; 4],
            nav_repeat: [None; 4],
            menu_commands: Vec::new(),
            menu_focus_shown: false,
            taps: VecDeque::new(),
        }
    }

    pub fn set_bindings(&mut self, bindings: crate::codwawzpc::bindings::Bindings) {
        self.bindings = bindings;
    }

    /// Aggregate aliases before changing an action, so releasing one held
    /// input never releases another input bound to the same action.
    pub fn physical(&mut self, token: &str, pressed: bool) -> bool {
        let Some(action) = self.bindings.action(token).map(str::to_owned) else {
            return false;
        };
        let changed = if pressed {
            self.physical_held.insert(token.to_owned())
        } else {
            self.physical_held.remove(token)
        };
        if !changed {
            return true;
        }
        if action == "Sprint" && token.starts_with("Pad:") {
            if pressed {
                self.pad_sprint = true;
            }
            return true;
        }
        let held = self.physical_held.iter().any(|input| {
            !(action == "Sprint" && input.starts_with("Pad:"))
                && self.bindings.action(input) == Some(action.as_str())
        });
        match action.as_str() {
            "Forward" => self.set_key(Source::Desktop, Key::Forward, held),
            "Left" => self.set_key(Source::Desktop, Key::Left, held),
            "Back" => self.set_key(Source::Desktop, Key::Back, held),
            "Right" => self.set_key(Source::Desktop, Key::Right, held),
            "Sprint" => self.set_key(Source::Desktop, Key::Sprint, held),
            "Reload" => self.set_key(Source::Desktop, Key::Reload, held),
            "Interact" => self.set_key(Source::Desktop, Key::Interact, held),
            "Grenade" => self.set_key(Source::Desktop, Key::Grenade, held),
            "Special" => self.set_key(Source::Desktop, Key::Special, held),
            "Melee" => self.set_key(Source::Desktop, Key::Melee, held),
            "NextWeapon" => self.set_key(Source::Desktop, Key::NextWeapon, held),
            "Fire" => self.set_button(Source::Desktop, Button::Fire, held),
            "Aim" => self.set_button(Source::Desktop, Button::Aim, held),
            _ => unreachable!(),
        }
        true
    }

    pub fn trigger(&mut self, token: &str, value: f32) {
        let held = self.physical_held.contains(token);
        if !held && value >= TRIGGER_PRESS {
            self.physical(token, true);
        } else if held && value <= TRIGGER_RELEASE {
            self.physical(token, false);
        }
    }

    /// Whether a level is being played (rather than a menu shown).
    pub fn set_gameplay(&mut self, gameplay: bool) {
        self.gameplay = gameplay;
        self.menu_commands.clear();
        for dir in ALL_DIRS {
            self.nav_held[dir as usize] = [false; NAV_SOURCES];
            self.nav_repeat[dir as usize] = None;
        }
    }

    #[cfg(test)]
    pub fn key(&mut self, key: Key, pressed: bool) {
        self.set_key(Source::Desktop, key, pressed);
    }

    fn key_held(&self, key: Key) -> bool {
        self.keys.iter().any(|keys| keys[key as usize])
    }

    fn set_key(&mut self, source: Source, key: Key, pressed: bool) {
        let was = self.key_held(key);
        self.keys[source as usize][key as usize] = pressed;
        let now = self.key_held(key);
        if was == now {
            return;
        }
        let action = match (key, now) {
            (Key::Reload, true) => Action::Reload,
            (Key::Interact, true) => Action::InteractStart,
            (Key::Interact, false) => Action::InteractEnd,
            (Key::Grenade, true) => Action::GrenadeStart,
            (Key::Grenade, false) => Action::GrenadeThrow,
            (Key::Special, true) => Action::CycleSpecial,
            (Key::Melee, true) => Action::Melee,
            (Key::NextWeapon, true) => Action::SwapWeapon,
            _ => return,
        };
        self.actions.push(action);
    }

    #[cfg(test)]
    pub fn button(&mut self, button: Button, pressed: bool) {
        self.set_button(Source::Desktop, button, pressed);
    }

    fn button_held(&self, button: Button) -> bool {
        self.buttons.iter().any(|buttons| buttons[button as usize])
    }

    fn set_button(&mut self, source: Source, button: Button, pressed: bool) {
        let was = self.button_held(button);
        self.buttons[source as usize][button as usize] = pressed;
        if button == Button::Fire && !was && pressed {
            self.fire_pressed = true;
        }
    }

    /// The game has two weapon slots, so either wheel direction swaps.
    #[cfg(test)]
    pub fn wheel(&mut self) {
        self.actions.push(Action::SwapWeapon);
    }

    /// Relative mouse motion in counts (y down positive).
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        let scale = DEGREES_PER_COUNT * self.look.sensitivity * FULL_TURN / 360.0;
        let dy = if self.look.invert_y { dy } else { -dy };
        self.pending_look.0 += dx as f32 * scale;
        self.pending_look.1 += dy as f32 * scale;
    }

    /// A controller button. During play it controls the player; in menus it
    /// navigates.
    pub fn pad_button(&mut self, button: PadButton, pressed: bool, now: Instant) -> PadResponse {
        let play_key = match button {
            PadButton::A => Some(Key::Interact),
            PadButton::X => Some(Key::Reload),
            PadButton::Y => Some(Key::NextWeapon),
            PadButton::B | PadButton::RightStick => Some(Key::Melee),
            PadButton::RightShoulder => Some(Key::Grenade),
            PadButton::LeftShoulder => Some(Key::Special),
            _ => None,
        };
        let nav_dir = match button {
            PadButton::DPadUp => Some(Dir::Up),
            PadButton::DPadDown => Some(Dir::Down),
            PadButton::DPadLeft => Some(Dir::Left),
            PadButton::DPadRight => Some(Dir::Right),
            _ => None,
        };
        if !pressed {
            // Release whatever the press did, even if play started or
            // stopped in between.
            if let Some(key) = play_key {
                self.set_key(Source::Pad, key, false);
            }
            if let Some(dir) = nav_dir {
                self.nav(dir, NavSource::DPad, false, now);
            }
            return PadResponse::None;
        }
        if matches!(button, PadButton::Start | PadButton::Back) {
            return PadResponse::Escape;
        }
        if self.gameplay {
            if let Some(key) = play_key {
                self.set_key(Source::Pad, key, true);
            } else if button == PadButton::LeftStick {
                self.pad_sprint = true;
            }
            PadResponse::None
        } else if let Some(dir) = nav_dir {
            self.nav(dir, NavSource::DPad, true, now);
            PadResponse::None
        } else if button == PadButton::A {
            self.activate();
            PadResponse::None
        } else if button == PadButton::B {
            PadResponse::Escape
        } else {
            PadResponse::None
        }
    }

    /// A controller axis moved.
    pub fn pad_axis(&mut self, axis: PadAxis, value: f32, now: Instant) {
        let value = value.clamp(-1.0, 1.0);
        self.axes[axis as usize] = value;
        match axis {
            PadAxis::LeftTrigger | PadAxis::RightTrigger => {
                let button = if axis == PadAxis::RightTrigger {
                    Button::Fire
                } else {
                    Button::Aim
                };
                let held = self.buttons[Source::Pad as usize][button as usize];
                if !held && value >= TRIGGER_PRESS {
                    self.set_button(Source::Pad, button, true);
                } else if held && value <= TRIGGER_RELEASE {
                    self.set_button(Source::Pad, button, false);
                }
            }
            PadAxis::LeftX | PadAxis::LeftY => {
                let (x, y) = (
                    self.axes[PadAxis::LeftX as usize],
                    self.axes[PadAxis::LeftY as usize],
                );
                let along = |dir: Dir| match dir {
                    Dir::Up => -y,
                    Dir::Down => y,
                    Dir::Left => -x,
                    Dir::Right => x,
                };
                // One direction at a time: the held one until it eases off,
                // else the dominant one once pushed far enough.
                let held = ALL_DIRS
                    .into_iter()
                    .find(|&d| self.nav_held[d as usize][NavSource::Stick as usize]);
                let wanted = held.filter(|&d| along(d) > NAV_RELEASE).or_else(|| {
                    ALL_DIRS
                        .into_iter()
                        .filter(|&d| along(d) >= NAV_PRESS)
                        .max_by(|&a, &b| along(a).total_cmp(&along(b)))
                });
                if wanted != held {
                    if let Some(dir) = held {
                        self.nav(dir, NavSource::Stick, false, now);
                    }
                    if let Some(dir) = wanted {
                        self.nav(dir, NavSource::Stick, true, now);
                    }
                }
            }
            PadAxis::RightX | PadAxis::RightY => (),
        }
    }

    /// A menu direction pressed or released. Ignored during play.
    pub fn nav(&mut self, dir: Dir, source: NavSource, pressed: bool, now: Instant) {
        let held = &mut self.nav_held[dir as usize];
        let was = held.iter().any(|&h| h);
        held[source as usize] = pressed && !self.gameplay;
        let is = held.iter().any(|&h| h);
        if is && !was {
            self.menu_commands.push(MenuCommand::Move(dir));
            self.nav_repeat[dir as usize] = Some(now + REPEAT_DELAY);
            self.menu_focus_shown = true;
        } else if !is {
            self.nav_repeat[dir as usize] = None;
        }
    }

    /// Press the focused menu button. Ignored during play.
    pub fn activate(&mut self) {
        if !self.gameplay {
            self.menu_commands.push(MenuCommand::Activate);
            self.menu_focus_shown = true;
        }
    }

    /// The mouse was used in the menus: hide the focused button.
    pub fn pointer_used(&mut self) {
        self.menu_focus_shown = false;
    }

    /// Whether the focused menu button is shown.
    pub fn menu_focus_shown(&self) -> bool {
        self.menu_focus_shown
    }

    /// Menu commands since the last call, including repeats of held
    /// directions that are due.
    pub fn take_menu(&mut self, now: Instant) -> Vec<MenuCommand> {
        for dir in ALL_DIRS {
            if let Some(due) = self.nav_repeat[dir as usize] {
                if due <= now {
                    self.menu_commands.push(MenuCommand::Move(dir));
                    self.nav_repeat[dir as usize] =
                        Some(due.max(now - REPEAT_INTERVAL) + REPEAT_INTERVAL);
                }
            }
        }
        std::mem::take(&mut self.menu_commands)
    }

    /// Input for the next game frame; one-shot input is consumed.
    pub fn take_frame(&mut self, now: Instant) -> Frame {
        let dt = self
            .last_frame
            .replace(now)
            .map_or(0.0, |last| (now - last).as_secs_f32().min(MAX_FRAME_TIME));
        let axis =
            |neg: Key, pos: Key| self.key_held(pos) as i8 as f32 - self.key_held(neg) as i8 as f32;
        let mut movement = (axis(Key::Left, Key::Right), axis(Key::Back, Key::Forward));
        let stick = |x: PadAxis, y: PadAxis, deadzone: f32| {
            deadzoned((self.axes[x as usize], -self.axes[y as usize]), deadzone)
        };
        let (move_x, move_y, look_x, look_y) = if self.bindings.swapped_sticks {
            (
                PadAxis::RightX,
                PadAxis::RightY,
                PadAxis::LeftX,
                PadAxis::LeftY,
            )
        } else {
            (
                PadAxis::LeftX,
                PadAxis::LeftY,
                PadAxis::RightX,
                PadAxis::RightY,
            )
        };
        let left = stick(move_x, move_y, MOVE_DEADZONE);
        if movement == (0.0, 0.0) {
            movement = left;
        }
        if left.1 < SPRINT_RELEASE {
            self.pad_sprint = false;
        }

        let (x, y) = stick(look_x, look_y, LOOK_DEADZONE);
        let magnitude = (x * x + y * y).sqrt();
        if magnitude > 0.0 {
            // Squared response: fine aim near the centre, fast turns at the
            // edge.
            let rate = STICK_DEGREES_PER_SECOND * self.look.sensitivity * FULL_TURN / 360.0
                * magnitude
                * dt;
            let y = if self.look.invert_y { -y } else { y };
            self.pending_look.0 += x * rate;
            self.pending_look.1 += y * rate;
        }

        Frame {
            movement,
            look: std::mem::take(&mut self.pending_look),
            fire_pressed: std::mem::take(&mut self.fire_pressed),
            fire_held: self.button_held(Button::Fire),
            aim: self.button_held(Button::Aim),
            sprint: self.key_held(Key::Sprint) || self.pad_sprint,
            actions: std::mem::take(&mut self.actions),
        }
    }

    /// Drop one-shot input that arrived while nothing could use it.
    pub fn discard_pending(&mut self) {
        self.fire_pressed = false;
        self.pending_look = (0.0, 0.0);
        self.actions.clear();
        self.last_frame = None;
    }

    /// Release everything held, as on focus loss. Releases that complete an
    /// action (using, throwing a grenade) are still delivered.
    pub fn release_all(&mut self) {
        self.physical_held.clear();
        for source in [Source::Desktop, Source::Pad] {
            for key in ALL_KEYS {
                self.set_key(source, key, false);
            }
            self.set_button(source, Button::Fire, false);
            self.set_button(source, Button::Aim, false);
        }
        self.axes = [0.0; AXIS_COUNT];
        self.pad_sprint = false;
        self.fire_pressed = false;
        self.pending_look = (0.0, 0.0);
        self.set_gameplay(self.gameplay);
    }

    /// Menu taps that are due.
    pub fn poll(&mut self, now: Instant) -> Vec<Touch> {
        let mut events = Vec::new();
        while let Some(&(due, touch)) = self.taps.front() {
            if due > now {
                break;
            }
            self.taps.pop_front();
            events.push(touch);
        }
        events
    }

    /// Menu touches are still to be delivered.
    pub fn touching(&self) -> bool {
        !self.taps.is_empty()
    }

    /// Queue a touch gesture: each step is a phase and point at a time after
    /// the gesture starts. Gestures run one after another.
    pub fn gesture(&mut self, steps: &[(Duration, Phase, Coords)], now: Instant) {
        let start = self.taps.back().map_or(now, |&(due, _)| due.max(now));
        for &(at, phase, coords) in steps {
            self.taps.push_back((
                start + at,
                Touch {
                    finger: Finger::Menu,
                    phase,
                    coords,
                },
            ));
        }
    }

    /// Queue a tap.
    pub fn tap(&mut self, coords: Coords, now: Instant) {
        self.gesture(
            &[
                (Duration::ZERO, Phase::Down, coords),
                (TAP, Phase::Up, coords),
            ],
            now,
        );
    }

    /// Escape during play: tap the score to open the pause menu.
    pub fn pause(&mut self, now: Instant) -> Vec<Touch> {
        self.release_all();
        self.tap(hud::PAUSE, now);
        self.poll(now)
    }

    /// Escape in menus: tap the Back button shared by the game's menus.
    pub fn back(&mut self, now: Instant) -> Vec<Touch> {
        self.tap(hud::BACK, now);
        self.poll(now)
    }
}

/// A stick position with a radial dead zone, rescaled so movement starts
/// from zero at its edge, and limited to the unit circle.
fn deadzoned((x, y): (f32, f32), deadzone: f32) -> (f32, f32) {
    let magnitude = (x * x + y * y).sqrt();
    if magnitude <= deadzone {
        return (0.0, 0.0);
    }
    let scaled = ((magnitude - deadzone) / (1.0 - deadzone)).min(1.0);
    (x / magnitude * scaled, y / magnitude * scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebound_aliases_release_only_after_last_input_and_focus_loss() {
        let mut c = controls();
        let now = Instant::now();
        c.physical("Key:E", true);
        c.physical("Pad:A", true);
        c.physical("Key:E", false);
        assert_eq!(c.take_frame(now).actions, vec![Action::InteractStart]);
        c.release_all();
        assert_eq!(c.take_frame(now).actions, vec![Action::InteractEnd]);
        c.physical("Pad:A", false);
        assert!(c.take_frame(now).actions.is_empty());
    }

    #[test]
    fn physical_fire_and_grenade_are_held_and_released_across_devices() {
        let mut c = controls();
        let now = Instant::now();
        c.physical("Mouse:Left", true);
        c.trigger("Pad:RightTrigger", 0.7);
        c.physical("Key:G", true);
        let frame = c.take_frame(now);
        assert!(frame.fire_pressed && frame.fire_held);
        assert_eq!(frame.actions, vec![Action::GrenadeStart]);
        c.physical("Mouse:Left", false);
        assert!(c.take_frame(now).fire_held);
        c.trigger("Pad:RightTrigger", 0.1);
        c.physical("Key:G", false);
        let frame = c.take_frame(now);
        assert!(!frame.fire_held);
        assert_eq!(frame.actions, vec![Action::GrenadeThrow]);
    }

    #[test]
    fn custom_keyboard_and_trigger_bindings_drive_actions() {
        let mut c = controls();
        let mut bindings = crate::codwawzpc::bindings::Bindings::default();
        for (action, inputs) in &mut bindings.entries {
            if action == "Reload" {
                *inputs = vec!["Key:T".into(), "Pad:RightTrigger".into()];
            }
            if action == "Fire" {
                *inputs = vec!["Mouse:Left".into(), "Pad:X".into()];
            }
        }
        c.set_bindings(bindings);
        let now = Instant::now();
        assert!(!c.physical("Key:R", true));
        assert!(c.physical("Key:T", true));
        assert_eq!(c.take_frame(now).actions, vec![Action::Reload]);
        c.physical("Key:T", false);
        c.trigger("Pad:RightTrigger", 0.6);
        assert_eq!(c.take_frame(now).actions, vec![Action::Reload]);
        c.trigger("Pad:RightTrigger", 0.2);
    }

    #[test]
    fn trigger_hysteresis_and_swapped_sticks() {
        let mut c = controls();
        let mut bindings = crate::codwawzpc::bindings::Bindings::default();
        bindings.swapped_sticks = true;
        c.set_bindings(bindings);
        let now = Instant::now();
        c.trigger("Pad:RightTrigger", 0.6);
        c.trigger("Pad:RightTrigger", 0.4);
        assert!(c.take_frame(now).fire_held);
        c.trigger("Pad:RightTrigger", 0.2);
        assert!(!c.take_frame(now).fire_held);
        c.pad_axis(PadAxis::RightY, -1.0, now);
        assert!(c.take_frame(now).movement.1 > 0.9);
    }

    fn controls() -> Controls {
        Controls::new(LookSettings::default())
    }

    fn playing() -> Controls {
        let mut c = controls();
        c.set_gameplay(true);
        c
    }

    fn frame(c: &mut Controls) -> Frame {
        c.take_frame(Instant::now())
    }

    #[test]
    fn movement_keys_combine_into_axes() {
        let mut c = controls();
        c.key(Key::Forward, true);
        c.key(Key::Left, true);
        assert_eq!(frame(&mut c).movement, (-1.0, 1.0));
        c.key(Key::Right, true);
        c.key(Key::Back, true);
        assert_eq!(frame(&mut c).movement, (0.0, 0.0));
        c.key(Key::Left, false);
        c.key(Key::Forward, false);
        assert_eq!(frame(&mut c).movement, (1.0, -1.0));
    }

    #[test]
    fn fire_press_is_reported_once_and_hold_persists() {
        let mut c = controls();
        c.button(Button::Fire, true);
        let f = frame(&mut c);
        assert!(f.fire_pressed && f.fire_held);
        let f = frame(&mut c);
        assert!(!f.fire_pressed && f.fire_held);
        c.button(Button::Fire, false);
        let f = frame(&mut c);
        assert!(!f.fire_pressed && !f.fire_held);
    }

    #[test]
    fn a_click_between_frames_still_fires() {
        let mut c = controls();
        c.button(Button::Fire, true);
        c.button(Button::Fire, false);
        let f = frame(&mut c);
        assert!(f.fire_pressed && !f.fire_held);
    }

    #[test]
    fn keys_send_the_touch_buttons_actions() {
        let mut c = controls();
        c.key(Key::Interact, true);
        c.key(Key::Interact, true); // key repeat
        c.key(Key::Reload, true);
        c.key(Key::Interact, false);
        c.key(Key::Grenade, true);
        c.key(Key::Grenade, false);
        c.key(Key::Melee, true);
        c.key(Key::Special, true);
        c.wheel();
        assert_eq!(
            frame(&mut c).actions,
            vec![
                Action::InteractStart,
                Action::Reload,
                Action::InteractEnd,
                Action::GrenadeStart,
                Action::GrenadeThrow,
                Action::Melee,
                Action::CycleSpecial,
                Action::SwapWeapon,
            ]
        );
        assert!(frame(&mut c).actions.is_empty());
        assert_eq!(Action::InteractStart as u32, 1);
        assert_eq!(Action::SwapWeapon as u32, 12);
    }

    #[test]
    fn mouse_look_scales_with_sensitivity_and_inverts() {
        let mut c = controls();
        c.mouse_motion(100, 0);
        c.mouse_motion(0, 100);
        let (yaw, pitch) = frame(&mut c).look;
        let expected = 100.0 * DEGREES_PER_COUNT * FULL_TURN / 360.0;
        assert!((yaw - expected).abs() < 1e-3);
        // Moving the mouse down looks down.
        assert!((pitch + expected).abs() < 1e-3);
        assert_eq!(frame(&mut c).look, (0.0, 0.0));

        let mut c = Controls::new(LookSettings {
            sensitivity: 2.0,
            invert_y: true,
        });
        c.mouse_motion(-10, 10);
        let (yaw, pitch) = frame(&mut c).look;
        let expected = 20.0 * DEGREES_PER_COUNT * FULL_TURN / 360.0;
        assert!((yaw + expected).abs() < 1e-3);
        assert!((pitch - expected).abs() < 1e-3);
    }

    #[test]
    fn focus_loss_releases_everything_and_completes_actions() {
        let mut c = playing();
        c.key(Key::Forward, true);
        c.key(Key::Sprint, true);
        c.key(Key::Interact, true);
        c.pad_button(PadButton::RightShoulder, true, Instant::now());
        c.button(Button::Fire, true);
        c.pad_axis(PadAxis::LeftTrigger, 1.0, Instant::now());
        c.pad_axis(PadAxis::LeftY, -1.0, Instant::now());
        c.mouse_motion(5, 5);
        let _ = frame(&mut c);
        c.mouse_motion(5, 5);
        c.release_all();
        let f = frame(&mut c);
        assert_eq!(f.movement, (0.0, 0.0));
        assert_eq!(f.look, (0.0, 0.0));
        assert!(!f.fire_held && !f.fire_pressed && !f.aim && !f.sprint);
        assert_eq!(f.actions, vec![Action::InteractEnd, Action::GrenadeThrow]);
    }

    #[test]
    fn discarding_keeps_held_state() {
        let mut c = controls();
        c.key(Key::Forward, true);
        c.button(Button::Aim, true);
        c.key(Key::Reload, true);
        c.mouse_motion(5, 0);
        c.discard_pending();
        let f = frame(&mut c);
        assert_eq!(f.movement, (0.0, 1.0));
        assert!(f.aim);
        assert!(f.actions.is_empty());
        assert_eq!(f.look, (0.0, 0.0));
    }

    #[test]
    fn pause_releases_input_and_taps_the_score() {
        let t0 = Instant::now();
        let mut c = controls();
        c.button(Button::Fire, true);
        let events = c.pause(t0);
        assert_eq!(
            events,
            vec![Touch {
                finger: Finger::Menu,
                phase: Phase::Down,
                coords: hud::PAUSE,
            }]
        );
        assert!(!frame(&mut c).fire_held);
        assert!(c.poll(t0 + TAP / 2).is_empty());
        assert_eq!(c.poll(t0 + TAP)[0].phase, Phase::Up);
    }

    #[test]
    fn taps_queue_one_after_another() {
        let t0 = Instant::now();
        let mut c = controls();
        assert_eq!(c.back(t0).len(), 1);
        assert!(c.back(t0).is_empty());
        assert!(c.touching());
        let later = c.poll(t0 + TAP);
        assert_eq!(later.len(), 2);
        assert_eq!(later[0].phase, Phase::Up);
        assert_eq!(later[1].phase, Phase::Down);
        assert_eq!(c.poll(t0 + TAP * 2)[0].coords, hud::BACK);
        assert!(!c.touching());
    }

    #[test]
    fn controller_plays_with_sticks_triggers_and_buttons() {
        let t0 = Instant::now();
        let mut c = playing();
        // Full forward and a little right: analog movement.
        c.pad_axis(PadAxis::LeftY, -1.0, t0);
        c.pad_axis(PadAxis::LeftX, 0.1, t0);
        c.pad_axis(PadAxis::RightTrigger, 0.8, t0);
        c.pad_axis(PadAxis::LeftTrigger, 0.6, t0);
        c.pad_button(PadButton::A, true, t0);
        c.pad_button(PadButton::LeftStick, true, t0);
        let f = c.take_frame(t0);
        assert!(f.movement.1 > 0.99 && f.movement.0 > 0.0 && f.movement.0 < 0.2);
        assert!(f.fire_pressed && f.fire_held && f.aim && f.sprint);
        assert_eq!(f.actions, vec![Action::InteractStart]);

        // Triggers release with hysteresis; slowing down ends the sprint.
        c.pad_axis(PadAxis::RightTrigger, 0.4, t0);
        c.pad_axis(PadAxis::LeftTrigger, 0.2, t0);
        c.pad_axis(PadAxis::LeftY, -0.1, t0);
        c.pad_button(PadButton::A, false, t0);
        let f = c.take_frame(t0);
        assert!(f.fire_held && !f.fire_pressed && !f.aim && !f.sprint);
        assert_eq!(f.movement, (0.0, 0.0));
        assert_eq!(f.actions, vec![Action::InteractEnd]);
    }

    #[test]
    fn keyboard_movement_wins_over_the_stick() {
        let mut c = playing();
        c.pad_axis(PadAxis::LeftX, 1.0, Instant::now());
        c.key(Key::Forward, true);
        assert_eq!(frame(&mut c).movement, (0.0, 1.0));
    }

    #[test]
    fn right_stick_look_integrates_over_time() {
        let t0 = Instant::now();
        let mut c = playing();
        c.pad_axis(PadAxis::RightX, 1.0, t0);
        c.pad_axis(PadAxis::RightY, -1.0, t0);
        assert_eq!(c.take_frame(t0).look, (0.0, 0.0));
        let (yaw, pitch) = c.take_frame(t0 + Duration::from_millis(50)).look;
        // Diagonal is limited to full deflection.
        let full = STICK_DEGREES_PER_SECOND * FULL_TURN / 360.0 * 0.05;
        let expected = full / 2f32.sqrt();
        assert!((yaw - expected).abs() < 0.5, "{yaw} {expected}");
        // Stick up looks up.
        assert!((pitch - expected).abs() < 0.5);
        // A stall does not spin the view.
        let (yaw, _) = c.take_frame(t0 + Duration::from_secs(5)).look;
        assert!(yaw <= full * 2.0 / 2f32.sqrt() + 0.5);
    }

    #[test]
    fn stick_deadzone_ignores_drift() {
        let mut c = playing();
        c.pad_axis(PadAxis::LeftX, 0.1, Instant::now());
        c.pad_axis(PadAxis::RightY, 0.1, Instant::now());
        let f = frame(&mut c);
        assert_eq!(f.movement, (0.0, 0.0));
        assert_eq!(frame(&mut c).look, (0.0, 0.0));
    }

    #[test]
    fn controller_start_and_back_escape() {
        let t0 = Instant::now();
        let mut c = playing();
        assert_eq!(
            c.pad_button(PadButton::Start, true, t0),
            PadResponse::Escape
        );
        c.set_gameplay(false);
        assert_eq!(c.pad_button(PadButton::B, true, t0), PadResponse::Escape);
        assert_eq!(c.pad_button(PadButton::Back, true, t0), PadResponse::Escape);
        assert_eq!(c.pad_button(PadButton::B, false, t0), PadResponse::None);
    }

    #[test]
    fn menus_navigate_with_keys_dpad_and_stick() {
        let t0 = Instant::now();
        let mut c = controls();
        assert!(!c.menu_focus_shown());
        c.nav(Dir::Down, NavSource::Keyboard, true, t0);
        c.nav(Dir::Down, NavSource::Keyboard, false, t0);
        c.pad_button(PadButton::DPadRight, true, t0);
        c.pad_button(PadButton::DPadRight, false, t0);
        c.pad_axis(PadAxis::LeftY, -0.9, t0);
        c.pad_axis(PadAxis::LeftY, -0.5, t0); // still held (hysteresis)
        c.pad_axis(PadAxis::LeftY, 0.0, t0);
        c.pad_button(PadButton::A, true, t0);
        assert!(c.menu_focus_shown());
        assert_eq!(
            c.take_menu(t0),
            vec![
                MenuCommand::Move(Dir::Down),
                MenuCommand::Move(Dir::Right),
                MenuCommand::Move(Dir::Up),
                MenuCommand::Activate,
            ]
        );
        c.pointer_used();
        assert!(!c.menu_focus_shown());
        // The stick's dominant axis wins on diagonals.
        c.pad_axis(PadAxis::LeftX, 0.7, t0);
        c.pad_axis(PadAxis::LeftY, 0.9, t0);
        assert_eq!(c.take_menu(t0), vec![MenuCommand::Move(Dir::Right)]);
    }

    #[test]
    fn held_directions_repeat() {
        let t0 = Instant::now();
        let mut c = controls();
        c.nav(Dir::Left, NavSource::Keyboard, true, t0);
        // A second source holding the same direction does not press again.
        c.nav(Dir::Left, NavSource::DPad, true, t0);
        assert_eq!(c.take_menu(t0).len(), 1);
        assert!(c.take_menu(t0 + REPEAT_DELAY / 2).is_empty());
        assert_eq!(c.take_menu(t0 + REPEAT_DELAY).len(), 1);
        assert!(c
            .take_menu(t0 + REPEAT_DELAY + REPEAT_INTERVAL / 2)
            .is_empty());
        assert_eq!(c.take_menu(t0 + REPEAT_DELAY + REPEAT_INTERVAL).len(), 1);
        c.nav(Dir::Left, NavSource::Keyboard, false, t0);
        assert_eq!(c.take_menu(t0 + REPEAT_DELAY * 4).len(), 1);
        c.nav(Dir::Left, NavSource::DPad, false, t0);
        assert!(c.take_menu(t0 + REPEAT_DELAY * 8).is_empty());
    }

    #[test]
    fn menu_input_is_ignored_during_play() {
        let t0 = Instant::now();
        let mut c = playing();
        c.nav(Dir::Down, NavSource::Keyboard, true, t0);
        c.activate();
        c.pad_button(PadButton::DPadUp, true, t0);
        assert!(c.take_menu(t0).is_empty());
        // Controller buttons control the player instead.
        c.pad_button(PadButton::X, true, t0);
        assert_eq!(frame(&mut c).actions, vec![Action::Reload]);
    }

    #[test]
    fn gestures_follow_queued_taps() {
        let t0 = Instant::now();
        let mut c = controls();
        c.tap((10.0, 10.0), t0);
        let ms = Duration::from_millis;
        c.gesture(
            &[
                (ms(0), Phase::Down, (1.0, 1.0)),
                (ms(20), Phase::Move, (2.0, 1.0)),
                (ms(40), Phase::Up, (3.0, 1.0)),
            ],
            t0,
        );
        assert_eq!(c.poll(t0).len(), 1);
        let rest = c.poll(t0 + TAP + ms(40));
        assert_eq!(
            rest.iter().map(|t| t.phase).collect::<Vec<_>>(),
            vec![Phase::Up, Phase::Down, Phase::Move, Phase::Up]
        );
    }
}
