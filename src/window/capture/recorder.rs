/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! A bounded handoff to a continuous H.264 encoder. The rolling buffer lives
//! on disk as compressed one-second segments, never as seconds of raw frames.
use super::CaptureSettings;
use crate::audio::capture::{self as audio, CHANNELS, RATE};
use crate::codwawzpc::capture_settings::resolution_limit;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Instant;

/// A presented frame, RGBA with rows bottom-up as OpenGL reads them.
pub(super) struct Frame {
    pub pixels: Arc<Vec<u8>>,
    pub size: (u32, u32),
    pub at: Instant,
}

pub(super) struct Recorder {
    frames: Option<mpsc::SyncSender<Frame>>,
    unlocks: mpsc::Sender<u32>,
    worker: Option<JoinHandle<()>>,
    dropped: usize,
}

impl Recorder {
    pub fn start(root: PathBuf, settings: CaptureSettings) -> Self {
        // At most three raw frames can wait for the encoder. If encoding falls
        // behind, gameplay keeps running; timestamp-based repetition below
        // preserves playback speed instead of compressing time.
        let (tx, rx) = mpsc::sync_channel::<Frame>(3);
        let (unlock_tx, unlock_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut session: Option<Session> = None;
            while let Ok(frame) = rx.recv() {
                if session.as_ref().is_some_and(|s| s.size != frame.size) {
                    drop(session.take());
                }
                if session.is_none() {
                    session = match Session::new(&root, settings.clone(), frame.size) {
                        Ok(session) => Some(session),
                        Err(e) => {
                            log!("Achievement clips unavailable (FFmpeg with libx264 is required): {e}");
                            break;
                        }
                    };
                }
                let session = session.as_mut().unwrap();
                for id in unlock_rx.try_iter() {
                    session.unlock(id);
                }
                if let Err(e) = session.frame(frame) {
                    log!("Achievement recorder stopped: {e}");
                    break;
                }
            }
            // Requests can arrive immediately before shutdown, after the last frame.
            if let Some(session) = session.as_mut() {
                for id in unlock_rx.try_iter() {
                    session.unlock(id);
                }
            }
        });
        Self {
            frames: Some(tx),
            unlocks: unlock_tx,
            worker: Some(worker),
            dropped: 0,
        }
    }

    /// False means the recorder has failed. A full queue never blocks gameplay.
    pub fn frame(&mut self, frame: Frame) -> bool {
        match self.frames.as_ref().unwrap().try_send(frame) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                self.dropped += 1;
                true
            }
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn unlock(&self, id: u32) {
        let _ = self.unlocks.send(id);
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        drop(self.frames.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if self.dropped > 0 {
            log!(
                "Achievement recorder dropped {} input frames: try 720p or 30 fps if clips stutter",
                self.dropped
            );
        }
    }
}

struct Pending {
    ids: Vec<u32>,
    start: u64,
    end: u64,
}

struct Session {
    root: PathBuf,
    directory: PathBuf,
    settings: CaptureSettings,
    size: (u32, u32),
    child: Child,
    input: Option<ChildStdin>,
    epoch: Option<Instant>,
    written: u64,
    previous: Option<Arc<Vec<u8>>>,
    pending: Vec<Pending>,
    exports: Vec<JoinHandle<()>>,
    cleaned: u64,
    audio_written: u64,
    audio_file: Option<fs::File>,
}

impl Session {
    fn new(root: &Path, settings: CaptureSettings, size: (u32, u32)) -> io::Result<Self> {
        let directory = root.join(format!(".recording-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory)?;
        let result = Self::spawn(root, &directory, settings, size);
        if result.is_err() {
            let _ = fs::remove_dir_all(directory);
        }
        result
    }

    fn spawn(
        root: &Path,
        directory: &Path,
        settings: CaptureSettings,
        size: (u32, u32),
    ) -> io::Result<Self> {
        let (width, height) = size;
        let (mw, mh) = resolution_limit(&settings.clip_resolution).unwrap_or((1920, 1080));
        // Frames arrive as OpenGL reads them, rows bottom-up.
        let scale = format!(
            "vflip,scale=w='min({mw},iw)':h='min({mh},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2:flags=lanczos,format=yuv420p"
        );
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{width}x{height}"),
                "-framerate",
                &settings.clip_fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-vf",
                &scale,
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-threads",
                "2",
                "-crf",
                settings.crf(),
                "-pix_fmt",
                "yuv420p",
                "-g",
                &settings.clip_fps.to_string(),
                "-keyint_min",
                &settings.clip_fps.to_string(),
                "-sc_threshold",
                "0",
                "-bf",
                "0",
                "-f",
                "segment",
                "-segment_time",
                "1",
                "-reset_timestamps",
                "1",
                "-segment_format",
                "mpegts",
            ])
            .arg(directory.join("%08d.ts"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(fs::File::create(directory.join("encoder.log"))?)
            .spawn()?;
        let input = child.stdin.take();
        Ok(Self {
            root: root.to_path_buf(),
            directory: directory.to_path_buf(),
            settings,
            size,
            child,
            input,
            epoch: None,
            written: 0,
            previous: None,
            pending: Vec::new(),
            exports: Vec::new(),
            cleaned: 0,
            audio_written: 0,
            audio_file: None,
        })
    }

    fn unlock(&mut self, id: u32) {
        if self.pending.iter().any(|p| p.ids.contains(&id)) {
            return;
        }
        let segment = self.written / self.settings.clip_fps as u64;
        // Unlocks in the same second can share their capture. A later unlock
        // gets its own post-roll instead of being cut off by an earlier one.
        if let Some(pending) = self.pending.iter_mut().find(|p| p.end == segment + 4) {
            pending.ids.push(id);
        } else {
            self.pending.push(Pending {
                ids: vec![id],
                start: segment.saturating_sub(3),
                end: segment + 4,
            });
        }
    }

    fn frame(&mut self, frame: Frame) -> io::Result<()> {
        let fps = self.settings.clip_fps as u64;
        let epoch = *self.epoch.get_or_insert(frame.at);
        let target =
            (frame.at.saturating_duration_since(epoch).as_secs_f64() * fps as f64).round() as u64;
        // Long gaps (minimization/suspend) are omitted, rather than writing
        // minutes of frozen frames and blocking the encoder's bounded queue.
        let target = if target > self.written + fps * 2 {
            self.write_audio(self.written * RATE as u64 / fps)?;
            self.epoch = Some(
                frame.at - std::time::Duration::from_secs_f64(self.written as f64 / fps as f64),
            );
            self.written
        } else {
            target
        };
        if target < self.written {
            return Ok(());
        }
        // Read only audio whose presentation time has passed. The final video
        // frame's audio is flushed on the next input or at shutdown.
        self.write_audio(target * RATE as u64 / fps)?;
        let input = self.input.as_mut().unwrap();
        if let Some(previous) = &self.previous {
            while self.written < target {
                input.write_all(previous)?;
                self.written += 1;
            }
        }
        input.write_all(&frame.pixels)?;
        self.written += 1;
        self.previous = Some(frame.pixels);
        self.exports.retain(|job| !job.is_finished());
        // A pipe write only means FFmpeg accepted bytes. The next segment's
        // existence, rather than our input count, proves the previous is closed.
        let complete = fs::read_dir(&self.directory)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()?.to_str()? == "ts").then_some(())?;
                path.file_stem()?.to_str()?.parse::<u64>().ok()
            })
            .max()
            .unwrap_or(0);
        // The next keyframe closes the previous segment. Only export closed
        // segments; the final one is flushed explicitly on shutdown.
        let (ready, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| complete >= p.end);
        self.pending = pending;
        for pending in ready {
            self.export(pending, complete);
        }
        let keep = self
            .pending
            .iter()
            .map(|p| p.start)
            .min()
            .unwrap_or(complete.saturating_sub(4));
        while self.cleaned < keep {
            let _ = fs::remove_file(self.segment(self.cleaned));
            let _ = fs::remove_file(self.directory.join(format!("{:08}.pcm", self.cleaned)));
            self.cleaned += 1;
        }
        Ok(())
    }

    fn segment(&self, index: u64) -> PathBuf {
        self.directory.join(format!("{index:08}.ts"))
    }

    fn write_audio(&mut self, end: u64) -> io::Result<()> {
        let Some(epoch) = self.epoch else {
            return Ok(());
        };
        while self.audio_written < end {
            let index = self.audio_written / RATE as u64;
            let offset = self.audio_written % RATE as u64;
            if self.audio_file.is_none() {
                self.audio_file = Some(fs::File::create(
                    self.directory.join(format!("{index:08}.pcm")),
                )?);
            }
            let frames = (end - self.audio_written).min(RATE as u64 - offset);
            let at =
                epoch + std::time::Duration::from_secs_f64(self.audio_written as f64 / RATE as f64);
            let samples = audio::samples(at, frames as usize);
            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            self.audio_file.as_mut().unwrap().write_all(&bytes)?;
            self.audio_written += frames;
            if self.audio_written.is_multiple_of(RATE as u64) {
                drop(self.audio_file.take());
            }
        }
        Ok(())
    }

    fn export(&mut self, pending: Pending, end: u64) {
        let staging = self
            .directory
            .join(format!("export-{}", uuid::Uuid::new_v4()));
        let result = (|| -> io::Result<()> {
            fs::create_dir(&staging)?;
            let mut list = String::new();
            let mut soundtrack = fs::File::create(staging.join("audio.pcm"))?;
            for index in pending.start..pending.end.min(end) {
                let name = format!("{index:08}.ts");
                let source = self.segment(index);
                if source.metadata().is_ok_and(|m| m.len() > 0) {
                    // Keep an independent link while the rolling buffer advances.
                    fs::hard_link(&source, staging.join(&name))
                        .or_else(|_| fs::copy(&source, staging.join(&name)).map(|_| ()))?;
                    list.push_str(&format!("file '{name}'\n"));
                    let mut pcm = fs::File::open(self.directory.join(format!("{index:08}.pcm")))?;
                    io::copy(&mut pcm, &mut soundtrack)?;
                }
            }
            if list.is_empty() {
                return Err(io::Error::other("No encoded frames available for unlock"));
            }
            fs::write(staging.join("input.txt"), list)
        })();
        if let Err(e) = result {
            log!("Cannot prepare achievement clip: {e}");
            let _ = fs::remove_dir_all(&staging);
            return;
        }
        let root = self.root.clone();
        self.exports.push(std::thread::spawn(move || {
            let output = staging.join("clip.mp4");
            let result = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                ])
                .arg(staging.join("input.txt"))
                .args([
                    "-f",
                    "s16le",
                    "-ar",
                    &RATE.to_string(),
                    "-ac",
                    &CHANNELS.to_string(),
                    "-i",
                ])
                .arg(staging.join("audio.pcm"))
                .args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                ])
                .arg(&output)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output();
            match result {
                Ok(result) if result.status.success() => {
                    for id in pending.ids {
                        let directory = root.join(format!("{id:03}"));
                        let temporary = directory.join("clip.tmp.mp4");
                        let result = fs::create_dir_all(&directory)
                            .and_then(|()| fs::copy(&output, &temporary))
                            .and_then(|_| fs::rename(&temporary, directory.join("clip.mp4")));
                        match result {
                            Ok(()) => {
                                log!(
                                    "Saved achievement clip {}",
                                    directory.join("clip.mp4").display()
                                );
                            }
                            Err(e) => {
                                log!("Cannot save achievement clip: {e}");
                            }
                        }
                    }
                }
                Ok(result) => {
                    log!(
                        "Cannot export achievement clip: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
                Err(e) => {
                    log!("Cannot export achievement clip: {e}");
                }
            }
            let _ = fs::remove_dir_all(staging);
        }));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) =
            self.write_audio(self.written * RATE as u64 / self.settings.clip_fps as u64)
        {
            log!("Cannot flush achievement audio: {error}");
        }
        drop(self.audio_file.take());
        drop(self.input.take());
        if self.child.wait().is_ok_and(|status| status.success()) {
            for pending in std::mem::take(&mut self.pending) {
                let end = self.written.div_ceil(self.settings.clip_fps as u64);
                self.export(pending, end);
            }
        } else if let Ok(error) = fs::read_to_string(self.directory.join("encoder.log")) {
            log!(
                "Achievement encoder failed: {}",
                error.chars().take(4096).collect::<String>()
            );
        }
        for job in self.exports.drain(..) {
            let _ = job.join();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn soundtrack(path: &Path) -> Vec<i16> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name,sample_rate,channels",
                "-of",
                "default=noprint_wrappers=1",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let info = String::from_utf8(output.stdout).unwrap();
        for expected in ["codec_name=aac", "sample_rate=48000", "channels=2"] {
            assert!(info.contains(expected), "Missing {expected}: {info}");
        }
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-f",
                "s16le",
                "-acodec",
                "pcm_s16le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        decoded
            .stdout
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]))
            .collect()
    }

    fn assert_tones(path: &Path, offset: usize, seconds: usize) {
        let samples = soundtrack(path);
        let frames = samples.len() / CHANNELS;
        assert!(
            (seconds * RATE as usize..seconds * RATE as usize + 2048).contains(&frames),
            "Audio duration must match video (within AAC padding): {frames} frames"
        );
        for second in 0..seconds {
            let from = (second * RATE as usize + RATE as usize / 4) * CHANNELS;
            let to = from + RATE as usize / 2 * CHANNELS;
            for (channel, base) in [220, 660].into_iter().enumerate() {
                let channel_samples: Vec<_> = samples[from..to]
                    .chunks_exact(CHANNELS)
                    .map(|s| s[channel])
                    .collect();
                let crossings = channel_samples
                    .windows(2)
                    .filter(|s| s[0] <= 0 && s[1] > 0)
                    .count();
                let hz = crossings * 2;
                let expected = base + (second + offset) * 20;
                assert!(hz.abs_diff(expected) <= 4,
                    "Audio must retain channel and timeline: second={second}, channel={channel}, {hz} != {expected}");
                assert!(
                    channel_samples.iter().any(|s| s.unsigned_abs() > 1000),
                    "Audio must be audible"
                );
            }
        }
    }

    fn probe(path: &Path) -> String {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_entries",
                "stream=codec_name,width,height,r_frame_rate,nb_read_frames:format=duration",
                "-of",
                "default=noprint_wrappers=1",
            ])
            .arg(path)
            .output()
            .expect("FFprobe is required for recorder integration tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn real_60fps_clip_has_smooth_timing_and_a_bounded_disk_ring() {
        let root = std::env::temp_dir().join(format!("clip-60-{}", uuid::Uuid::new_v4()));
        let mut session = Session::new(&root, CaptureSettings::default(), (64, 48))
            .expect("FFmpeg with libx264 is required for recorder integration tests");
        let start = Instant::now() + Duration::from_secs(3600);
        let audio = audio::TestAudio::new();
        for index in 0..660 {
            let samples: Vec<i16> = (index * 800..(index + 1) * 800)
                .flat_map(|sample| {
                    let second = sample / RATE;
                    [220, 660].map(|base| {
                        ((sample as f64 * (base + second * 20) as f64 * std::f64::consts::TAU
                            / RATE as f64)
                            .sin()
                            * 10000.0) as i16
                    })
                })
                .collect();
            audio.push(
                start + Duration::from_secs_f64(index as f64 / 60.0),
                &samples,
            );
            // A moving bar gives every input frame a distinct spatial position.
            let mut pixels = vec![30, 60, 90, 255].repeat(64 * 48);
            for y in 0..48 {
                let x = index as usize % 64;
                pixels[(y * 64 + x) * 4..][..4].copy_from_slice(&[240, 220, 40, 255]);
            }
            session
                .frame(Frame {
                    pixels: Arc::new(pixels),
                    size: (64, 48),
                    at: start + Duration::from_secs_f64(index as f64 / 60.0),
                })
                .unwrap();
            if index == 180 {
                session.unlock(10);
                session.unlock(11);
            }
            if index == 360 {
                session.unlock(12);
            }
        }
        assert!(
            session.pending.is_empty(),
            "The clip must export while recording continues"
        );
        let segments = fs::read_dir(&session.directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|s| s == "ts"))
            .count();
        assert!(segments <= 7, "Rolling segments must be pruned: {segments}");
        let pcm_segments = fs::read_dir(&session.directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|s| s == "pcm"))
            .count();
        assert!(
            pcm_segments <= 8,
            "Rolling PCM must be pruned: {pcm_segments}"
        );
        drop(session);
        let clip = root.join("010/clip.mp4");
        let info = probe(&clip);
        for expected in [
            "codec_name=h264",
            "width=64",
            "height=48",
            "r_frame_rate=60/1",
            "nb_read_frames=420",
            "duration=7.000000",
        ] {
            assert!(info.contains(expected), "Missing {expected}: {info}");
        }
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&clip)
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 420 * 64 * 48 * 3);
        for (index, frame) in decoded.stdout.chunks_exact(64 * 48 * 3).enumerate() {
            let x = frame[..64 * 3]
                .chunks_exact(3)
                .enumerate()
                .max_by_key(|(_, rgb)| rgb[0])
                .unwrap()
                .0;
            assert_eq!(
                x,
                index % 64,
                "Every 60 FPS sample must retain its distinct moving-bar position"
            );
        }
        assert_eq!(
            fs::read(&clip).unwrap(),
            fs::read(root.join("011/clip.mp4")).unwrap()
        );
        let later = root.join("012/clip.mp4");
        let later_info = probe(&later);
        assert!(
            later_info.contains("nb_read_frames=420"),
            "The later unlock needs its own full post-roll: {later_info}"
        );
        assert_ne!(fs::read(clip).unwrap(), fs::read(later).unwrap());
        assert_tones(&root.join("010/clip.mp4"), 0, 7);
        assert_tones(&root.join("012/clip.mp4"), 3, 7);
        assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timestamps_repeat_missing_frames_and_exit_flushes_partial_30fps_clip() {
        let root = std::env::temp_dir().join(format!("clip-30-{}", uuid::Uuid::new_v4()));
        let options = CaptureSettings {
            clip_fps: 30,
            clip_resolution: "720p".into(),
            clip_quality: "ultra".into(),
            ..Default::default()
        };
        let mut session = Session::new(&root, options, (1920, 1080)).unwrap();
        let at = Instant::now() + Duration::from_secs(7200);
        let audio = audio::TestAudio::new();
        audio.push(at, &vec![5000; RATE as usize * CHANNELS]);
        audio.push(
            at + Duration::from_secs(60),
            &vec![-5000; RATE as usize * CHANNELS],
        );
        let pixels = Arc::new(vec![22, 44, 66, 255].repeat(1920 * 1080));
        session
            .frame(Frame {
                pixels: pixels.clone(),
                size: (1920, 1080),
                at,
            })
            .unwrap();
        session.unlock(7);
        // Skipping input frames must preserve time, rather than play too fast.
        session
            .frame(Frame {
                pixels: pixels.clone(),
                size: (1920, 1080),
                at: at + Duration::from_millis(100),
            })
            .unwrap();
        assert_eq!(session.written, 4);
        session
            .frame(Frame {
                pixels,
                size: (1920, 1080),
                at: at + Duration::from_secs(60),
            })
            .unwrap();
        assert_eq!(
            session.written, 5,
            "Suspending must not fill the disk with frozen frames"
        );
        drop(session);
        let info = probe(&root.join("007/clip.mp4"));
        for expected in [
            "codec_name=h264",
            "width=1280",
            "height=720",
            "r_frame_rate=30/1",
            "nb_read_frames=5",
        ] {
            assert!(info.contains(expected), "Missing {expected}: {info}");
        }
        let samples = soundtrack(&root.join("007/clip.mp4"));
        assert!(
            (8000..10048).contains(&(samples.len() / CHANNELS)),
            "Suspended audio must be omitted"
        );
        assert!(
            samples[2000..4000].iter().any(|s| *s > 1000),
            "The pre-suspend audio must survive"
        );
        assert!(
            samples[14000..15000].iter().any(|s| *s < -1000),
            "The final frame must use resumed audio"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_full_handoff_drops_input_without_blocking_gameplay() {
        let (tx, rx) = mpsc::sync_channel(3);
        let (unlocks, _unlock_rx) = mpsc::channel();
        let mut recorder = Recorder {
            frames: Some(tx),
            unlocks,
            worker: None,
            dropped: 0,
        };
        let at = Instant::now();
        for _ in 0..100 {
            assert!(recorder.frame(Frame {
                pixels: Arc::new(vec![0; 4]),
                size: (1, 1),
                at
            }));
        }
        assert_eq!(recorder.dropped, 97);
        assert_eq!(rx.try_iter().count(), 3);
        drop(rx);
        assert!(!recorder.frame(Frame {
            pixels: Arc::new(vec![0; 4]),
            size: (1, 1),
            at
        }));
    }
}
