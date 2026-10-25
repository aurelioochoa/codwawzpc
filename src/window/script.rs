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
//! 7500 key w down           # desktop control key (SDL name, or lshift/rshift)
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
//! 9900 quit
//! ```
//!
//! Coordinates are logical points in the app's 480×320 landscape space. `#`
//! starts a comment.

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
    Quit,
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

    /// Steps whose time has come, in order.
    pub fn due(&mut self) -> Vec<Step> {
        let elapsed = self.started.elapsed();
        let mut due = Vec::new();
        while self.steps.front().is_some_and(|(at, _)| *at <= elapsed) {
            due.push(self.steps.pop_front().unwrap().1);
        }
        due
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
}
