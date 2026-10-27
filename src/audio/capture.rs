/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Play and retain the same OpenAL mix when achievement recording is enabled.
//! Each device keeps a short, bounded PCM history; the encoder reads it by
//! presentation time, so slow video encoding cannot delay audible playback.
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use touchHLE_openal_soft_wrapper::{self as al, alc_types::*};

pub(crate) const RATE: u32 = 48_000;
pub(crate) const CHANNELS: usize = 2;
const HISTORY_FRAMES: usize = RATE as usize * 12;

struct Block {
    at: Instant,
    samples: Vec<i16>,
}

#[derive(Default)]
struct History {
    blocks: VecDeque<Block>,
    frames: usize,
}

impl History {
    fn push(&mut self, at: Instant, samples: &[i16]) {
        // Callback buffers are small, but bound individual blocks as well.
        let samples = &samples[..samples.len().min(HISTORY_FRAMES * CHANNELS)];
        self.frames += samples.len() / CHANNELS;
        self.blocks.push_back(Block {
            at,
            samples: samples.to_vec(),
        });
        while self.frames > HISTORY_FRAMES {
            self.frames -= self.blocks.pop_front().unwrap().samples.len() / CHANNELS;
        }
    }

    fn mix(&self, at: Instant, output: &mut [i32]) {
        for block in &self.blocks {
            let offset = if block.at >= at {
                (block.at - at).as_secs_f64() * RATE as f64
            } else {
                -(at - block.at).as_secs_f64() * RATE as f64
            }
            .round() as i64;
            let source = (-offset).max(0) as usize * CHANNELS;
            let target = offset.max(0) as usize * CHANNELS;
            if source >= block.samples.len() || target >= output.len() {
                continue;
            }
            for (out, sample) in output[target..].iter_mut().zip(&block.samples[source..]) {
                *out += i32::from(*sample);
            }
        }
    }
}

type SharedHistory = Arc<Mutex<History>>;
static SOURCES: OnceLock<Mutex<Vec<SharedHistory>>> = OnceLock::new();

fn prune(sources: &mut Vec<SharedHistory>) {
    let now = Instant::now();
    sources.retain(|source| {
        Arc::strong_count(source) > 1
            || source.lock().unwrap().blocks.back().is_some_and(|block| {
                now.saturating_duration_since(block.at) < Duration::from_secs(12)
            })
    });
}

fn register(history: &SharedHistory) {
    let mut sources = SOURCES.get_or_init(Default::default).lock().unwrap();
    prune(&mut sources);
    sources.push(history.clone());
}

/// Missing audio (including a suspended device) is silence. Multiple game
/// devices are mixed together; microphone and other applications never enter.
pub(crate) fn samples(at: Instant, frames: usize) -> Vec<i16> {
    let mut mix = vec![0i32; frames * CHANNELS];
    if let Some(sources) = SOURCES.get() {
        let mut sources = sources.lock().unwrap();
        // Retain recently closed devices until the bounded video handoff has
        // drained, so closing audio just before quitting cannot erase its tail.
        prune(&mut sources);
        for source in sources.iter() {
            source.lock().unwrap().mix(at, &mut mix);
        }
    }
    mix.into_iter()
        .map(|sample| sample.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
        .collect()
}

struct Render {
    // The SDL device is closed before the OpenAL device, so the callback's
    // handle remains valid throughout its lifetime, including shutdown.
    device: usize,
    history: SharedHistory,
    next: Option<Instant>,
}

impl AudioCallback for Render {
    type Channel = i16;

    fn callback(&mut self, output: &mut [i16]) {
        let now = Instant::now();
        // SDL requests the next block for playback now. Keep a sample clock
        // across ordinary scheduling jitter, but rebase after suspension.
        let at = self
            .next
            .filter(|next| now.max(*next) - now.min(*next) < Duration::from_millis(100))
            .unwrap_or(now);
        let frames = output.len() / CHANNELS;
        unsafe {
            al::alcRenderSamplesSOFT(
                self.device as *mut ALCdevice,
                output.as_mut_ptr().cast(),
                frames as ALCsizei,
            );
        }
        self.next = Some(at + Duration::from_secs_f64(frames as f64 / RATE as f64));
        // Never wait on the recorder from an audio callback. A contended block
        // is represented by silence in the recording; playback still proceeds.
        if let Ok(mut history) = self.history.try_lock() {
            history.push(at, output);
        }
    }
}

pub(super) struct Playback {
    device: *mut ALCdevice,
    output: Option<AudioDevice<Render>>,
}

impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Playback")
            .field("device", &self.device)
            .finish()
    }
}

impl Playback {
    pub(super) fn open() -> Result<Self, String> {
        let device = unsafe { al::alcLoopbackOpenDeviceSOFT(std::ptr::null()) };
        if device.is_null() {
            return Err("Could not open the OpenAL recording mixer".into());
        }
        let mut playback = Self {
            device,
            output: None,
        };
        let history = Arc::new(Mutex::new(History::default()));
        let output = sdl2::init()?.audio()?.open_playback(
            None,
            &AudioSpecDesired {
                freq: Some(RATE as i32),
                channels: Some(CHANNELS as u8),
                samples: Some(512),
            },
            |_| Render {
                device: device as usize,
                history: history.clone(),
                next: None,
            },
        )?;
        register(&history);
        playback.output = Some(output);
        Ok(playback)
    }

    pub(super) fn device(&self) -> *mut ALCdevice {
        self.device
    }

    pub(super) fn resume(&self) {
        self.output.as_ref().unwrap().resume();
    }

    pub(super) unsafe fn context(&self, attributes: *const ALCint) -> *mut ALCcontext {
        // Preserve source-count and other guest attributes, replacing only the
        // format requirements of the loopback device.
        let mut attrs = Vec::new();
        if !attributes.is_null() {
            let mut p = attributes;
            while *p != 0 {
                if ![0x1007, 0x1990, 0x1991].contains(&*p) {
                    attrs.extend_from_slice(&[*p, *p.add(1)]);
                }
                p = p.add(2);
            }
        }
        attrs.extend_from_slice(&[
            0x1007,
            RATE as ALCint, // ALC_FREQUENCY
            0x1990,
            0x1501, // ALC_FORMAT_CHANNELS_SOFT, ALC_STEREO_SOFT
            0x1991,
            0x1402, // ALC_FORMAT_TYPE_SOFT, ALC_SHORT_SOFT
            0,
        ]);
        al::alcCreateContext(self.device, attrs.as_ptr())
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        // SDL_CloseAudioDevice waits for an in-flight callback before returning.
        drop(self.output.take());
        unsafe { al::alcCloseDevice(self.device) };
    }
}

#[cfg(test)]
pub(crate) struct TestAudio(SharedHistory);

#[cfg(test)]
impl TestAudio {
    pub(crate) fn new() -> Self {
        let history = Arc::new(Mutex::new(History::default()));
        register(&history);
        Self(history)
    }

    pub(crate) fn push(&self, at: Instant, samples: &[i16]) {
        self.0.lock().unwrap().push(at, samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_history_aligns_trims_and_bounds_samples() {
        let at = Instant::now();
        let mut history = History::default();
        history.push(at, &[100, -100, 200, -200, 300, -300]);
        let mut output = [0; 8];
        history.mix(at - Duration::from_secs_f64(1.0 / RATE as f64), &mut output);
        assert_eq!(output, [0, 0, 100, -100, 200, -200, 300, -300]);
        output.fill(0);
        history.mix(at + Duration::from_secs_f64(1.0 / RATE as f64), &mut output);
        assert_eq!(output, [200, -200, 300, -300, 0, 0, 0, 0]);
        for second in 1..30 {
            history.push(
                at + Duration::from_secs(second),
                &vec![7; RATE as usize * CHANNELS],
            );
        }
        assert_eq!(history.frames, HISTORY_FRAMES);
        assert_eq!(history.blocks.len(), 12);
        output.fill(0);
        history.mix(at, &mut output);
        assert_eq!(output, [0; 8], "Evicted history must be silence");
    }

    #[test]
    fn loopback_mix_reaches_playback_and_recording() {
        // Exercise the exact playback callback with a real OpenAL mixer,
        // without requiring an audio device on a headless test runner.
        let device = unsafe { al::alcLoopbackOpenDeviceSOFT(std::ptr::null()) };
        assert!(!device.is_null());
        let playback = Playback {
            device,
            output: None,
        };
        let context = unsafe { playback.context(std::ptr::null()) };
        assert!(!context.is_null());
        let mut buffer = 0;
        let mut source = 0;
        unsafe {
            assert_ne!(al::alcMakeContextCurrent(context), 0);
            al::alGenBuffers(1, &mut buffer);
            al::alGenSources(1, &mut source);
            let tone: Vec<i16> = (0..RATE)
                .flat_map(|i| {
                    let sample = ((i as f64 * 440.0 * std::f64::consts::TAU / RATE as f64).sin()
                        * 12000.0) as i16;
                    [sample, -sample]
                })
                .collect();
            al::alBufferData(
                buffer,
                0x1103,
                tone.as_ptr().cast(),
                (tone.len() * 2) as i32,
                RATE as i32,
            );
            al::alSourcei(source, 0x1009, buffer as i32);
            al::alSourcePlay(source);
            assert_eq!(al::alGetError(), 0);
        }
        let history = Arc::new(Mutex::new(History::default()));
        let mut callback = Render {
            device: device as usize,
            history: history.clone(),
            next: None,
        };
        let mut output = vec![0; 4096 * CHANNELS];
        callback.callback(&mut output);
        let history = history.lock().unwrap();
        assert_eq!(history.frames, 4096, "Playback callback must render audio");
        let mut mixed = vec![0; output.len()];
        history.mix(history.blocks[0].at, &mut mixed);
        assert_eq!(
            mixed,
            output.iter().copied().map(i32::from).collect::<Vec<_>>(),
            "The recording must receive exactly what the playback buffer receives"
        );
        assert!(
            mixed.iter().any(|s| s.abs() > 500),
            "The retained game mix must be audible"
        );
        drop(history);
        unsafe {
            al::alDeleteSources(1, &source);
            al::alDeleteBuffers(1, &buffer);
            al::alcMakeContextCurrent(std::ptr::null_mut());
            al::alcDestroyContext(context);
        }
    }
}
