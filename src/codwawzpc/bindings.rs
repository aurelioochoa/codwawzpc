/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Validated gameplay bindings. Menu and recovery input stays fixed.
#[cfg(test)]
use plist::Dictionary;
use plist::Value;
use std::collections::HashSet;
use std::path::Path;

pub const DEFAULTS: [(&str, &str); 13] = [
    ("Forward", "Key:W,Key:Up"),
    ("Left", "Key:A,Key:Left"),
    ("Back", "Key:S,Key:Down"),
    ("Right", "Key:D,Key:Right"),
    ("Sprint", "Key:Left Shift,Key:Right Shift,Pad:LeftStick"),
    ("Reload", "Key:R,Pad:X"),
    ("Interact", "Key:E,Key:F,Pad:A"),
    ("Grenade", "Key:G,Pad:RightShoulder"),
    ("Special", "Key:4,Pad:LeftShoulder"),
    ("Melee", "Key:V,Pad:B,Pad:RightStick"),
    ("NextWeapon", "Key:Q,Mouse:Wheel,Pad:Y"),
    ("Fire", "Mouse:Left,Pad:RightTrigger"),
    ("Aim", "Mouse:Right,Pad:LeftTrigger"),
];
const KEYS: &[&str] = &[
    "Up",
    "Down",
    "Left",
    "Right",
    "Left Shift",
    "Right Shift",
    "Left Ctrl",
    "Right Ctrl",
    "Left Alt",
    "Right Alt",
    "Space",
    "Backspace",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Insert",
    "-",
    "=",
    "[",
    "]",
    ";",
    "'",
    ".",
    "/",
    "\\",
];
const PAD: &[&str] = &[
    "A",
    "B",
    "X",
    "Y",
    "LeftShoulder",
    "RightShoulder",
    "LeftStick",
    "RightStick",
    "DPadUp",
    "DPadDown",
    "DPadLeft",
    "DPadRight",
    "LeftTrigger",
    "RightTrigger",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    pub entries: Vec<(String, Vec<String>)>,
    pub swapped_sticks: bool,
}
impl Default for Bindings {
    fn default() -> Self {
        Self {
            entries: DEFAULTS
                .iter()
                .map(|(a, b)| (a.to_string(), b.split(',').map(str::to_string).collect()))
                .collect(),
            swapped_sticks: false,
        }
    }
}
impl Bindings {
    pub fn action(&self, token: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(_, inputs)| inputs.iter().any(|s| s == token))
            .map(|(action, _)| action.as_str())
    }
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let dict = value
            .as_dictionary()
            .ok_or("Controls must be a dictionary")?;
        if dict.len() != DEFAULTS.len() + 1 {
            return Err("Controls must contain every action and StickPreset".into());
        }
        let swapped_sticks = match dict.get("StickPreset").and_then(Value::as_string) {
            Some("normal") => false,
            Some("swapped") => true,
            _ => return Err("StickPreset must be normal or swapped".into()),
        };
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for (name, _) in DEFAULTS {
            let text = dict
                .get(name)
                .and_then(Value::as_string)
                .ok_or_else(|| format!("Missing control {name}"))?;
            let mut inputs = Vec::new();
            for token in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let valid = if let Some(key) = token.strip_prefix("Key:") {
                    (key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase())
                        || (key.len() == 1 && key.as_bytes()[0].is_ascii_digit())
                        || KEYS.contains(&key)
                } else if let Some(pad) = token.strip_prefix("Pad:") {
                    PAD.contains(&pad)
                } else if let Some(mouse) = token.strip_prefix("Mouse:") {
                    ["Left", "Middle", "Right", "X1", "X2", "Wheel"].contains(&mouse)
                } else {
                    false
                };
                if !valid {
                    return Err(format!("Invalid input {token} for {name}"));
                }
                if !seen.insert(token.to_owned()) {
                    return Err(format!("Conflict: {token} is assigned more than once"));
                }
                inputs.push(token.to_owned());
            }
            entries.push((name.to_owned(), inputs));
        }
        Ok(Self {
            entries,
            swapped_sticks,
        })
    }
    pub fn load(config: &Path) -> Result<Self, String> {
        let path = config.join("controls.plist");
        if !path.exists() {
            return Ok(Self::default());
        }
        let value =
            Value::from_file(&path).map_err(|e| format!("Invalid {}: {e}", path.display()))?;
        Self::from_value(&value).map_err(|e| format!("Invalid {}: {e}", path.display()))
    }
    #[cfg(test)]
    fn value(&self) -> Value {
        let mut d = Dictionary::new();
        for (a, inputs) in &self.entries {
            d.insert(a.clone(), Value::String(inputs.join(",")));
        }
        d.insert(
            "StickPreset".into(),
            Value::String(
                if self.swapped_sticks {
                    "swapped"
                } else {
                    "normal"
                }
                .into(),
            ),
        );
        Value::Dictionary(d)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdl_key_names_match_configured_defaults() {
        use sdl2::keyboard::Keycode;
        assert_eq!(Keycode::W.name(), "W");
        assert_eq!(Keycode::LShift.name(), "Left Shift");
        assert_eq!(Keycode::Num4.name(), "4");
        assert_eq!(Keycode::Up.name(), "Up");
    }
    #[test]
    fn defaults_round_trip_and_conflicts_rejected() {
        let defaults = Bindings::default();
        assert_eq!(Bindings::from_value(&defaults.value()).unwrap(), defaults);
        let mut v = defaults.value();
        v.as_dictionary_mut()
            .unwrap()
            .insert("Reload".into(), Value::String("Key:W".into()));
        assert!(Bindings::from_value(&v).unwrap_err().contains("Conflict"));
    }
}
