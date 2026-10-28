/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Timed input scripts for reproducible smoke tests.
//!
//! `CODWAWZPC_INPUT_SCRIPT=<file>` replays one step per line. Each line is a
//! time in milliseconds since the window opened, then a command:
//!
//! ```text
//! 5000 tap 240 160          # touch and release after 80 ms
//! 5000 tap 240 160 400      # ... after 400 ms
//! 6000 down 1 30 215        # finger 1 down at (30, 215)
//! 6100 move 1 60 215
//! 7000 up 1 60 215
//! 7500 key w down           # desktop control key (SDL name, or lshift/rshift,
//!                           # lctrl/rctrl, lalt/ralt)
//! 8000 key w up
//! 8500 text Player1         # typed text for a focused text field
//! 9000 mouse 120 0          # relative mouse motion
//! 9100 button left down      # mouse button (left, right) down or up
//! 9200 wheel 1               # mouse wheel
//! 9250 pad a down             # controller button (SDL name: a, b, x, y,
//!                             # dpup, leftshoulder, start, …) down or up
//! 9260 axis lefty -1          # controller axis (leftx, lefty, rightx,
//!                             # righty, lefttrigger, righttrigger), -1 to 1
//! 9300 resize 1000 500       # resize the window
//! 9400 focus lost            # simulate focus loss (or: focus gained)
//! 9500 capture              # save a frame (see capture.rs)
//! 9600 wait menu             # hold the script until a menu screen settles
//! 9900 quit
//! ```
//!
//! Coordinates are logical points in the app's 480×320 landscape space
//! (wider with a widescreen game view). `#`
//! starts a comment.
//!
//! `wait movie`, `wait menu` and `wait playing` hold the script until the
//! intro movie plays, a menu screen has stayed the same for a third of a
//! second since the script last pressed anything (see
//! [crate::codwawzpc::menu]), or a level is being played. Later steps are
//! delayed by however long the wait took, so a script can keep fixed gaps
//! between menu presses whatever the load times.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Down(u8, (f32, f32)),
    Move(u8, (f32, f32)),
    Up(u8, (f32, f32)),
    Key(String, bool),
    Text(String),
    Mouse(i32, i32),
    Button(bool, bool),
    Wheel(i32),
    Pad(String, bool),
    Axis(String, f32),
    Resize(u32, u32),
    Focus(bool),
    Capture,
    Wait(Condition),
    Quit,
}

/// A state of the game a script can wait for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Condition {
    Movie,
    Menu,
    Playing,
}

pub(crate) struct InputScript {
    started: Instant,
    steps: VecDeque<(Duration, Step)>,
}

/// Finger IDs used by tap steps, distinct from explicit fingers 0–255.
const TAP_FINGER: u8 = 255;

pub(crate) fn parse(text: &str) -> Result<Vec<(Duration, Step)>, String> {
    let mut steps = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let error = |message: &str| format!("Input script line {}: {message}", number + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        let number = |i: usize| -> Result<f32, String> {
            words
                .get(i)
                .and_then(|w| w.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .ok_or_else(|| error("expected a number"))
        };
        let at = Duration::from_millis(number(0)? as u64);
        let finger = |i: usize| -> Result<u8, String> {
            words
                .get(i)
                .and_then(|w| w.parse::<u8>().ok())
                .filter(|&f| f != TAP_FINGER)
                .ok_or_else(|| error("expected a finger number 0–254"))
        };
        let expect_len = |n: usize| {
            if words.len() == n {
                Ok(())
            } else {
                Err(error(&format!("expected {} words", n)))
            }
        };
        match words.get(1).copied() {
            Some("tap") => {
                if !(4..=5).contains(&words.len()) {
                    return Err(error("tap needs x y [hold_ms]"));
                }
                let point = (number(2)?, number(3)?);
                let hold = if words.len() == 5 { number(4)? } else { 80.0 };
                steps.push((at, Step::Down(TAP_FINGER, point)));
                steps.push((
                    at + Duration::from_millis(hold as u64),
                    Step::Up(TAP_FINGER, point),
                ));
            }
            Some(kind @ ("down" | "move" | "up")) => {
                expect_len(5)?;
                let (f, point) = (finger(2)?, (number(3)?, number(4)?));
                steps.push((
                    at,
                    match kind {
                        "down" => Step::Down(f, point),
                        "move" => Step::Move(f, point),
                        _ => Step::Up(f, point),
                    },
                ));
            }
            Some("key") => {
                expect_len(4)?;
                let pressed = match words[3] {
                    "down" => true,
                    "up" => false,
                    _ => return Err(error("key state must be down or up")),
                };
                // SDL key names, plus single-word names for keys whose SDL
                // names contain spaces.
                let name = match words[2].to_ascii_lowercase().as_str() {
                    "lshift" => "left shift".to_string(),
                    "rshift" => "right shift".to_string(),
                    "lctrl" => "left ctrl".to_string(),
                    "rctrl" => "right ctrl".to_string(),
                    "lalt" => "left alt".to_string(),
                    "ralt" => "right alt".to_string(),
                    other => other.to_string(),
                };
                steps.push((at, Step::Key(name, pressed)));
            }
            Some("text") => {
                expect_len(3)?;
                steps.push((at, Step::Text(words[2].to_string())));
            }
            Some("mouse") => {
                expect_len(4)?;
                steps.push((at, Step::Mouse(number(2)? as i32, number(3)? as i32)));
            }
            Some("button") => {
                expect_len(4)?;
                let left = match words[2] {
                    "left" => true,
                    "right" => false,
                    _ => return Err(error("button must be left or right")),
                };
                let pressed = match words[3] {
                    "down" => true,
                    "up" => false,
                    _ => return Err(error("button state must be down or up")),
                };
                steps.push((at, Step::Button(left, pressed)));
            }
            Some("wheel") => {
                expect_len(3)?;
                steps.push((at, Step::Wheel(number(2)? as i32)));
            }
            Some("pad") => {
                expect_len(4)?;
                let pressed = match words[3] {
                    "down" => true,
                    "up" => false,
                    _ => return Err(error("pad button state must be down or up")),
                };
                steps.push((at, Step::Pad(words[2].to_ascii_lowercase(), pressed)));
            }
            Some("axis") => {
                expect_len(4)?;
                let value = number(3)?;
                if !(-1.0..=1.0).contains(&value) {
                    return Err(error("axis value must be from -1 to 1"));
                }
                steps.push((at, Step::Axis(words[2].to_ascii_lowercase(), value)));
            }
            Some("resize") => {
                expect_len(4)?;
                let (w, h) = (number(2)?, number(3)?);
                if w < 1.0 || h < 1.0 {
                    return Err(error("window size must be positive"));
                }
                steps.push((at, Step::Resize(w as u32, h as u32)));
            }
            Some("focus") => {
                expect_len(3)?;
                let gained = match words[2] {
                    "gained" => true,
                    "lost" => false,
                    _ => return Err(error("focus must be gained or lost")),
                };
                steps.push((at, Step::Focus(gained)));
            }
            Some("capture") => {
                expect_len(2)?;
                steps.push((at, Step::Capture));
            }
            Some("wait") => {
                expect_len(3)?;
                let condition = match words[2] {
                    "movie" => Condition::Movie,
                    "menu" => Condition::Menu,
                    "playing" => Condition::Playing,
                    _ => return Err(error("wait needs movie, menu or playing")),
                };
                steps.push((at, Step::Wait(condition)));
            }
            Some("quit") => {
                expect_len(2)?;
                steps.push((at, Step::Quit));
            }
            _ => return Err(error("unknown command")),
        }
    }
    // Stable, so steps at the same time keep their order.
    steps.sort_by_key(|(at, _)| *at);
    Ok(steps)
}

impl InputScript {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("CODWAWZPC_INPUT_SCRIPT")?;
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Cannot read input script {path:?}: {e}"));
        let steps = parse(&text).unwrap_or_else(|e| panic!("{e}"));
        log!(
            "Replaying {} input script steps from {:?}",
            steps.len(),
            path
        );
        Some(Self {
            started: Instant::now(),
            steps: steps.into(),
        })
    }

    /// Steps whose time has come, in order. A due wait holds back the steps
    /// after it until `holds` reports its condition, then shifts the clock so
    /// that they follow the wait by their scripted gaps.
    pub fn due(&mut self, holds: impl Fn(Condition) -> bool) -> Vec<Step> {
        let mut elapsed = self.started.elapsed();
        let mut due = Vec::new();
        while let Some(&(at, ref step)) = self.steps.front() {
            if at > elapsed {
                break;
            }
            if let &Step::Wait(condition) = step {
                if !holds(condition) {
                    break;
                }
                log!("Input script: {condition:?} after {:?}", elapsed - at);
                self.started += elapsed - at;
                elapsed = at;
                self.steps.pop_front();
                continue;
            }
            due.push(self.steps.pop_front().unwrap().1);
        }
        due
    }
}

/// Records desktop input as script steps (`CODWAWZPC_INPUT_RECORD=<file>`),
/// timed from the moment a level is first played, so a route walked by hand
/// can be replayed after a scripted `wait playing`.
pub(crate) struct InputRecorder {
    out: std::io::BufWriter<std::fs::File>,
    started: Option<Instant>,
}

impl InputRecorder {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("CODWAWZPC_INPUT_RECORD")?;
        let file = std::fs::File::create(&path)
            .unwrap_or_else(|e| panic!("Cannot create input recording {path:?}: {e}"));
        log!("Recording input to {:?} once a level is played", path);
        Some(Self {
            out: std::io::BufWriter::new(file),
            started: None,
        })
    }

    pub fn record(&mut self, playing: bool, event: &sdl2::event::Event) {
        use sdl2::event::Event as E;
        use std::io::Write;
        if self.started.is_none() && playing {
            self.started = Some(Instant::now());
        }
        let Some(started) = self.started else {
            return;
        };
        let command = match event {
            E::KeyDown {
                keycode: Some(sdl2::keyboard::Keycode::F10),
                ..
            } => "capture".to_string(),
            E::KeyDown {
                keycode: Some(k),
                repeat: false,
                ..
            } => format!("key {} down", key_name(*k)),
            E::KeyUp {
                keycode: Some(k), ..
            } => format!("key {} up", key_name(*k)),
            E::MouseMotion { xrel, yrel, .. } => format!("mouse {xrel} {yrel}"),
            E::MouseButtonDown { mouse_btn, .. } | E::MouseButtonUp { mouse_btn, .. } => {
                let button = match mouse_btn {
                    sdl2::mouse::MouseButton::Left => "left",
                    sdl2::mouse::MouseButton::Right => "right",
                    _ => return,
                };
                let state = if matches!(event, E::MouseButtonDown { .. }) {
                    "down"
                } else {
                    "up"
                };
                format!("button {button} {state}")
            }
            E::MouseWheel { y, .. } => format!("wheel {y}"),
            _ => return,
        };
        let _ = writeln!(self.out, "{} {}", started.elapsed().as_millis(), command);
        let _ = self.out.flush();
    }
}

/// A key's name as script steps spell it: SDL names, without spaces.
fn key_name(key: sdl2::keyboard::Keycode) -> String {
    match key.name().to_ascii_lowercase().as_str() {
        "left shift" => "lshift".to_string(),
        "right shift" => "rshift".to_string(),
        "left ctrl" => "lctrl".to_string(),
        "right ctrl" => "rctrl".to_string(),
        "left alt" => "lalt".to_string(),
        "right alt" => "ralt".to_string(),
        other => other.replace(' ', "_"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps_in_time_order_and_rejects_malformed_lines() {
        let steps = parse(
            "# comment\n100 tap 10 20 # tap\n50 key W down\n55 text Abc\n60 down 1 1 2\n70 mouse -5 3\n90 quit\n",
        )
        .unwrap();
        let ms = |v| Duration::from_millis(v);
        assert_eq!(
            steps,
            vec![
                (ms(50), Step::Key("w".into(), true)),
                (ms(55), Step::Text("Abc".into())),
                (ms(60), Step::Down(1, (1.0, 2.0))),
                (ms(70), Step::Mouse(-5, 3)),
                (ms(90), Step::Quit),
                (ms(100), Step::Down(TAP_FINGER, (10.0, 20.0))),
                (ms(180), Step::Up(TAP_FINGER, (10.0, 20.0))),
            ]
        );
        for bad in [
            "x tap 1 2",
            "1 tap 1",
            "1 down 255 1 2",
            "1 key w sideways",
            "1 dance",
            "1 capture now",
            "1 text",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn wait_holds_later_steps_and_keeps_their_gaps() {
        let steps = parse("0 wait menu\n0 key return down\n50 capture\n").unwrap();
        assert_eq!(steps[0], (Duration::ZERO, Step::Wait(Condition::Menu)));
        assert!(parse("0 wait forever\n").is_err());
        let mut script = InputScript {
            started: Instant::now() - Duration::from_millis(1000),
            steps: steps.into(),
        };
        assert!(script.due(|_| false).is_empty());
        assert_eq!(
            script.due(|c| c == Condition::Menu),
            vec![Step::Key("return".into(), true)]
        );
        // The capture is 50 ms after the wait was satisfied, not overdue.
        assert!(script.due(|_| true).is_empty());
    }
}
