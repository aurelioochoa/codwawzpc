/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Preferences shared by the launcher plist and the gameplay recorder.
use plist::{Dictionary, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSettings {
    pub screenshot_format: String,
    pub screenshot_resolution: String,
    pub clip_resolution: String,
    pub clip_fps: u32,
    pub clip_quality: String,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            screenshot_format: "png".into(),
            screenshot_resolution: "native".into(),
            clip_resolution: "1080p".into(),
            clip_fps: 60,
            clip_quality: "high".into(),
        }
    }
}

impl CaptureSettings {
    /// Returns None for keys owned by other host settings.
    pub(super) fn read(&mut self, key: &str, value: &Value) -> Option<Result<(), String>> {
        let (target, choices): (&mut String, &[&str]) = match key {
            "AchievementScreenshotFormat" => (&mut self.screenshot_format, &["png", "bmp"]),
            "AchievementScreenshotResolution" => (
                &mut self.screenshot_resolution,
                &["native", "1080p", "720p"],
            ),
            "AchievementClipResolution" => (&mut self.clip_resolution, &["1080p", "720p", "540p"]),
            "AchievementClipQuality" => (&mut self.clip_quality, &["standard", "high", "ultra"]),
            "AchievementClipFps" => {
                return Some(match value.as_unsigned_integer() {
                    Some(fps @ (30 | 60)) => {
                        self.clip_fps = fps as u32;
                        Ok(())
                    }
                    _ => Err(format!("{key} must be 30 or 60")),
                });
            }
            _ => return None,
        };
        Some(
            match value.as_string().filter(|name| choices.contains(name)) {
                Some(name) => {
                    *target = name.into();
                    Ok(())
                }
                None => Err(format!("{key} must be one of {}", choices.join(", "))),
            },
        )
    }

    pub(super) fn write(&self, dictionary: &mut Dictionary) {
        for (key, value) in [
            ("AchievementScreenshotFormat", &self.screenshot_format),
            (
                "AchievementScreenshotResolution",
                &self.screenshot_resolution,
            ),
            ("AchievementClipResolution", &self.clip_resolution),
            ("AchievementClipQuality", &self.clip_quality),
        ] {
            dictionary.insert(key.into(), Value::String(value.clone()));
        }
        dictionary.insert(
            "AchievementClipFps".into(),
            Value::Integer(self.clip_fps.into()),
        );
    }

    pub(crate) fn crf(&self) -> &'static str {
        match self.clip_quality.as_str() {
            "ultra" => "14",
            "standard" => "23",
            _ => "18",
        }
    }
}

/// Resolution is a bounding box; neither dimension is ever enlarged.
pub(crate) fn resolution_limit(name: &str) -> Option<(u32, u32)> {
    match name {
        "1080p" => Some((1920, 1080)),
        "720p" => Some((1280, 720)),
        "540p" => Some((960, 540)),
        _ => None,
    }
}
