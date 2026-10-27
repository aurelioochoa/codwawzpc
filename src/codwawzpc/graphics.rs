/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Optional rendering improvements over the game's own settings.
//!
//! Texture filtering: the game uploads a full PVRTC or RGBA mip chain for
//! its world textures and samples them with `GL_LINEAR_MIPMAP_NEAREST`, so
//! each surface switches abruptly from one mip level to the next, and
//! surfaces at a glancing angle (floors, long walls) blur early. Interface
//! textures have no mip levels and use `GL_LINEAR`. [TextureFilter] changes
//! only the textures the game itself gives a mipmapped minification filter:
//! trilinear filtering blends neighbouring levels, and anisotropic filtering
//! keeps detail at glancing angles. Interface textures are left alone.
//!
//! Picture adjustments: [Picture] applies gamma, contrast and a restrained
//! sharpening to each presented frame, HUD included, as it is drawn to the
//! window. At the original values nothing is applied and the frame is drawn
//! exactly as before.
//!
//! Widescreen: the game takes its screen size from the iPhone's (via the
//! Marmalade loader's `UIScreen` bounds and renderbuffer size) and lays out
//! for it, so [Widescreen] reports a wider iPhone screen. Levels then show
//! more of the scene at the sides, and the HUD anchors to the new edges.
//! The game's field of view spans the mean of the screen's width and height
//! (see [super::fov]), so it is widened to keep the original vertical view.
//! Menus were authored for 480 points; their full-width backgrounds, bars
//! and panels are widened to the screen (see [super::widescreen]).

use crate::gles::gles11_raw as gles11;
use crate::gles::GLES;

/// Minification filtering of the game's mipmapped textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFilter {
    /// The game's own filters.
    Original,
    /// Blend between mip levels.
    Trilinear,
    /// Trilinear with this many anisotropic samples.
    Anisotropic(u8),
}

impl TextureFilter {
    pub const NAMES: [(&'static str, TextureFilter); 6] = [
        ("original", TextureFilter::Original),
        ("trilinear", TextureFilter::Trilinear),
        ("2x", TextureFilter::Anisotropic(2)),
        ("4x", TextureFilter::Anisotropic(4)),
        ("8x", TextureFilter::Anisotropic(8)),
        ("16x", TextureFilter::Anisotropic(16)),
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

    /// Set a guest texture parameter, replacing a mipmapped minification
    /// filter as configured. The texture is the one bound to `target`.
    ///
    /// # Safety
    /// The context must be current.
    pub unsafe fn tex_parameter(
        self,
        gles: &mut dyn GLES,
        target: gles11::types::GLenum,
        pname: gles11::types::GLenum,
        param: gles11::types::GLint,
    ) -> bool {
        if self == TextureFilter::Original
            || pname != gles11::TEXTURE_MIN_FILTER
            || !matches!(
                param as gles11::types::GLenum,
                gles11::NEAREST_MIPMAP_NEAREST
                    | gles11::LINEAR_MIPMAP_NEAREST
                    | gles11::NEAREST_MIPMAP_LINEAR
                    | gles11::LINEAR_MIPMAP_LINEAR
            )
        {
            return false;
        }
        gles.TexParameteri(target, pname, gles11::LINEAR_MIPMAP_LINEAR as _);
        let samples = match self {
            TextureFilter::Anisotropic(samples) => f32::from(samples),
            _ => 1.0,
        };
        // Drivers clamp to their own maximum.
        gles.TexParameterf(target, TEXTURE_MAX_ANISOTROPY_EXT, samples);
        true
    }
}

/// The shape of the screen the game is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widescreen {
    /// The original 3:2 (480×320).
    Off,
    /// 16:10 (512×320).
    W16x10,
    /// 16:9 (568×320, as on later iPhones).
    W16x9,
}

impl Widescreen {
    pub const NAMES: [(&'static str, Widescreen); 3] = [
        ("off", Widescreen::Off),
        ("16:10", Widescreen::W16x10),
        ("16:9", Widescreen::W16x9),
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

    /// The landscape screen's width in points; its height stays 320.
    pub fn screen_width(self) -> u32 {
        match self {
            Widescreen::Off => 480,
            Widescreen::W16x10 => 512,
            Widescreen::W16x9 => 568,
        }
    }
}

/// Adjustments to each presented frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    /// Above 1 brightens dark tones, below 1 darkens them.
    pub gamma: f32,
    /// Above 1 widens the range between dark and light tones.
    pub contrast: f32,
    /// Edge sharpening, from 0 (none) to 1.
    pub sharpening: f32,
}

impl Picture {
    pub const ORIGINAL: Picture = Picture {
        gamma: 1.0,
        contrast: 1.0,
        sharpening: 0.0,
    };
    pub const GAMMA_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;
    pub const CONTRAST_RANGE: std::ops::RangeInclusive<f32> = 0.5..=1.5;
    pub const SHARPENING_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

    pub fn is_original(&self) -> bool {
        *self == Self::ORIGINAL
    }

    /// The adjustments for a frame where `texel` is one displayed pixel in
    /// texture coordinates.
    pub fn adjust(&self, texel: (f32, f32)) -> crate::gles::present::PictureAdjust {
        crate::gles::present::PictureAdjust {
            gamma: self.gamma,
            contrast: self.contrast,
            sharpening: self.sharpening,
            texel,
        }
    }
}

impl Default for Picture {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// From `GL_EXT_texture_filter_anisotropic`.
const TEXTURE_MAX_ANISOTROPY_EXT: gles11::types::GLenum = 0x84FE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (name, filter) in TextureFilter::NAMES {
            assert_eq!(TextureFilter::from_name(name), Some(filter));
            assert_eq!(filter.name(), name);
        }
        assert_eq!(TextureFilter::from_name("32x"), None);
        assert_eq!(TextureFilter::from_name("Trilinear"), None);
        for (name, widescreen) in Widescreen::NAMES {
            assert_eq!(Widescreen::from_name(name), Some(widescreen));
            assert_eq!(widescreen.name(), name);
        }
        assert_eq!(Widescreen::from_name("21:9"), None);
    }
}
