/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Same-window movie presentation. SDL remains confined to the window module.

use super::Window;
use crate::gles::{gles11_raw as gl, present::present_frame};
use crate::matrix::Matrix;
use sdl2::audio::{AudioQueue, AudioSpecDesired};

pub(crate) struct MovieOutput {
    audio: AudioQueue<i16>,
}

impl MovieOutput {
    /// Called on the runtime's parent stack, just like window event handling.
    pub(crate) fn new(window: &mut Window, samples: &[i16], volume: f32) -> Result<Self, String> {
        assert!(window.on_main_stack);
        let audio = window._sdl_ctx.audio()?.open_queue(
            None,
            &AudioSpecDesired {
                freq: Some(48_000),
                channels: Some(2),
                samples: Some(1024),
            },
        )?;
        let scaled: Vec<i16> = samples
            .iter()
            .map(|&sample| (sample as f32 * volume.clamp(0.0, 1.0)) as i16)
            .collect();
        audio.queue_audio(&scaled)?;
        // Resume only after the first video frame has been presented.
        Ok(Self { audio })
    }

    pub(crate) fn resume(&self) {
        self.audio.resume();
    }
    pub(crate) fn pause(&self) {
        self.audio.pause();
    }
    pub(crate) fn audio_finished(&self) -> bool {
        self.audio.size() == 0
    }
}
impl Drop for MovieOutput {
    fn drop(&mut self) {
        self.audio.pause();
        self.audio.clear();
    }
}

impl Window {
    pub(crate) fn set_movie_active(&mut self, active: bool) {
        self.movie_active = active;
        self.movie_skip_requested = false;
        if !active {
            self.movie_output = None;
        }
    }

    pub(crate) fn movie_start_audio(&mut self, samples: &[i16], volume: f32) -> Result<(), String> {
        self.movie_output = Some(MovieOutput::new(self, samples, volume)?);
        Ok(())
    }
    pub(crate) fn movie_resume(&self) {
        if let Some(output) = &self.movie_output {
            output.resume();
        }
    }
    pub(crate) fn movie_pause(&self) {
        if let Some(output) = &self.movie_output {
            output.pause();
        }
    }
    pub(crate) fn movie_audio_finished(&self) -> bool {
        self.movie_output
            .as_ref()
            .is_some_and(MovieOutput::audio_finished)
    }

    pub(crate) fn take_movie_skip(&mut self) -> bool {
        std::mem::take(&mut self.movie_skip_requested)
    }

    /// Consume movie controls before the guest action/touch layers. Keep quit,
    /// focus and lifecycle events visible to the runtime.
    pub(crate) fn consume_movie_event(&mut self, event: &sdl2::event::Event) -> bool {
        if !self.movie_active {
            return false;
        }
        use sdl2::event::Event as E;
        use sdl2::keyboard::Keycode as K;
        match event {
            E::KeyDown {
                keycode: Some(K::Escape | K::Return | K::Space),
                repeat: false,
                ..
            }
            | E::MouseButtonDown {
                mouse_btn: sdl2::mouse::MouseButton::Left,
                ..
            }
            | E::FingerDown { .. } => {
                self.movie_skip_requested = true;
                true
            }
            E::KeyDown {
                keycode: Some(K::F11 | K::F12),
                ..
            } => false,
            E::KeyDown { .. }
            | E::KeyUp { .. }
            | E::MouseButtonDown { .. }
            | E::MouseButtonUp { .. }
            | E::MouseMotion { .. }
            | E::FingerUp { .. }
            | E::FingerMotion { .. } => true,
            _ => false,
        }
    }

    pub(crate) fn present_movie_frame(&mut self, pixels: &[u8], width: u32, height: u32) {
        assert!(self.on_main_stack);
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
        // Movies are already in display orientation; their landscape frame
        // must not receive the portrait guest framebuffer's rotation.
        let (screen_w, screen_h) = self.window.drawable_size();
        let scale = (screen_w as f32 / width as f32).min(screen_h as f32 / height as f32);
        let (w, h) = (
            (width as f32 * scale) as u32,
            (height as f32 * scale) as u32,
        );
        let viewport = ((screen_w - w) / 2, (screen_h - h) / 2, w, h);
        {
            let mut gles = self.make_internal_gl_ctx_current();
            unsafe {
                let mut texture = 0;
                gles.BindFramebufferOES(gl::FRAMEBUFFER_OES, 0);
                gles.GenTextures(1, &mut texture);
                gles.BindTexture(gl::TEXTURE_2D, texture);
                gles.TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA as _,
                    width as _,
                    height as _,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    pixels.as_ptr().cast(),
                );
                gles.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gles.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gles.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gles.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
                gles.MatrixMode(gl::PROJECTION);
                gles.LoadIdentity();
                gles.MatrixMode(gl::MODELVIEW);
                gles.LoadIdentity();
                gles.Disable(gl::BLEND);
                gles.Disable(gl::DEPTH_TEST);
                gles.Disable(gl::SCISSOR_TEST);
                gles.Color4f(1.0, 1.0, 1.0, 1.0);
                present_frame(
                    gles.as_mut(),
                    viewport,
                    Matrix::identity(),
                    None,
                    None,
                    None,
                );
                gles.DeleteTextures(1, &texture);
            }
        }
        self.window.gl_swap_window();
        crate::codwawzpc::metrics::presented();
    }
}
