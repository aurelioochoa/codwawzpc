/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Bounded, cancellable local-file decoding. Workers never touch guest objects.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 320;
pub const FPS: u32 = 30;
pub const SAMPLE_RATE: i32 = 48_000;
pub const CHANNELS: u8 = 2;
const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_SECONDS: usize = 60;

pub enum Message {
    Audio(Vec<i16>),
    Frame(Vec<u8>),
    End,
    Error(String),
}

struct LocalInput(PathBuf);
impl LocalInput {
    fn new(mut input: impl Read) -> Result<Self, String> {
        let path =
            std::env::temp_dir().join(format!("codwawzpc-movie-{}.mp4", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        let local = Self(path);
        let copied = std::io::copy(&mut input.by_ref().take(MAX_INPUT + 1), &mut file)
            .map_err(|e| e.to_string())?;
        if copied > MAX_INPUT {
            return Err("movie exceeds the 64 MiB local-file limit".into());
        }
        file.flush().map_err(|e| e.to_string())?;
        Ok(local)
    }
}
impl Drop for LocalInput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn ffmpeg(path: &Path) -> Command {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-nostdin",
        "-v",
        "error",
        "-threads",
        "1",
        "-protocol_whitelist",
        "file,pipe",
        "-i",
    ])
    .arg(path)
    .args(["-t", "60", "-threads", "1"])
    .stdin(Stdio::null());
    cmd
}

/// The runtime's cancellation token also covers audio preroll, before any
/// frames or samples have reached the bounded delivery queue.
pub struct Decoder {
    receiver: Receiver<Message>,
    cancelled: Arc<AtomicBool>,
}
impl Decoder {
    pub fn try_recv(&self) -> Result<Message, TryRecvError> {
        self.receiver.try_recv()
    }
    #[cfg(test)]
    fn recv_timeout(&self, timeout: Duration) -> Result<Message, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn decode(
    input: impl Read,
    sender: &SyncSender<Message>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let local = LocalInput::new(input)?;
    if cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    // Decode audio first (at most 11 MiB). Keeping PCM available before the
    // presentation clock starts synchronizes audio and video preroll. stderr
    // is discarded rather than accumulating potentially unbounded diagnostics.
    let mut child = ffmpeg(&local.0)
        .args([
            "-map", "0:a:0", "-vn", "-ac", "2", "-ar", "48000", "-f", "s16le", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start FFmpeg: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let max_audio = MAX_SECONDS * SAMPLE_RATE as usize * CHANNELS as usize * 2;
    let (pcm_tx, pcm_rx) = mpsc::sync_channel(1);
    let reader = match std::thread::Builder::new()
        .name("codwawzpc-movie-pcm".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(max_audio as u64 + 1).read_to_end(&mut bytes);
            let _ = pcm_tx.send((result, bytes));
        }) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("could not start movie audio reader: {error}"));
        }
    };
    let bytes = loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Ok(());
        }
        match pcm_rx.recv_timeout(Duration::from_millis(10)) {
            Ok((result, bytes)) => {
                if result.is_err()
                    || bytes.is_empty()
                    || bytes.len() > max_audio
                    || bytes.len() % 4 != 0
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("FFmpeg audio decode failed or exceeded the PCM limit".into());
                }
                break bytes;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("movie audio reader failed".into());
            }
        }
    };
    let _ = reader.join();
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err("FFmpeg audio decode failed".into());
    }
    let samples = bytes
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .collect();
    drop(bytes);
    if sender.send(Message::Audio(samples)).is_err() {
        return Ok(());
    }

    let mut child = ffmpeg(&local.0)
        .args(["-map", "0:v:0", "-an", "-vf",
            "fps=30,scale=480:320:force_original_aspect_ratio=decrease,pad=480:320:(ow-iw)/2:(oh-ih)/2,vflip",
            "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn().map_err(|e| format!("could not start FFmpeg video: {e}"))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut frames = 0;
    let result: Result<(), String> = loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
        let mut frame = vec![0; WIDTH * HEIGHT * 4];
        let mut offset = 0;
        while offset < frame.len() {
            match stdout.read(&mut frame[offset..]) {
                Ok(0) => break,
                Ok(n) => offset += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("movie frame read failed: {e}"));
                }
            }
        }
        if offset == 0 {
            break Ok(());
        }
        if offset != frame.len() {
            break Err("truncated decoded movie frame".into());
        }
        frames += 1;
        if frames > MAX_SECONDS * FPS as usize {
            break Err("movie frame limit exceeded".into());
        }
        if sender.send(Message::Frame(frame)).is_err() {
            // Dropping the receiver on stop/skip unblocks a full channel and
            // terminates the decoder; no detached FFmpeg remains behind.
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    result?;
    if frames == 0 || !status.success() {
        return Err("FFmpeg video decode failed".into());
    }
    let _ = sender.send(Message::End);
    Ok(())
}

pub fn start(input: impl Read + Send + 'static) -> Decoder {
    // Three queued frames (~1.8 MiB), independent of input length.
    let (sender, receiver) = mpsc::sync_channel(3);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancelled.clone();
    let failure_sender = sender.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("codwawzpc-movie".into())
        .spawn(move || {
            if let Err(error) = decode(input, &sender, &worker_cancel) {
                let _ = sender.send(Message::Error(error));
            }
        })
    {
        let _ = failure_sender.send(Message::Error(format!(
            "could not start decoder worker: {error}"
        )));
    }
    Decoder {
        receiver,
        cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn movie_rejects_corrupt_input() {
        let receiver = start(std::io::Cursor::new(b"not an MPEG-4 file"));
        match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            Message::Error(error) => assert!(error.contains("FFmpeg")),
            _ => panic!("corrupt movie was accepted"),
        }
    }

    #[test]
    fn movie_temp_input_is_removed() {
        let local = LocalInput::new(std::io::Cursor::new(b"private test movie")).unwrap();
        let path = local.0.clone();
        assert_eq!(std::fs::read(&path).unwrap(), b"private test movie");
        drop(local);
        assert!(!path.exists());
    }

    #[test]
    #[ignore = "requires the user's locally supplied intro; no assets are distributed"]
    fn movie_supplied_intro() {
        let path = std::env::var_os("CODWAWZPC_TEST_MOVIE").expect("CODWAWZPC_TEST_MOVIE");
        let receiver = start(std::fs::File::open(path).unwrap());
        let mut frames = 0;
        let mut samples = 0;
        loop {
            match receiver.recv_timeout(Duration::from_secs(20)).unwrap() {
                Message::Audio(pcm) => samples = pcm.len(),
                Message::Frame(frame) => {
                    assert_eq!(frame.len(), WIDTH * HEIGHT * 4);
                    frames += 1;
                }
                Message::End => break,
                Message::Error(error) => panic!("{error}"),
            }
        }
        let duration = samples as f64 / (SAMPLE_RATE as f64 * CHANNELS as f64);
        assert!((16.0..18.0).contains(&duration));
        assert!((frames as f64 / FPS as f64 - duration).abs() < 0.2);
        assert!(samples > 0 && frames > 0);
        println!("movie decoded: {frames} frames, {samples} samples, {duration:.3}s audio");
    }

    fn synthetic_movie() -> Vec<u8> {
        let output = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=480x270:r=30:d=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=44100:duration=2",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-movflags",
                "frag_keyframe+empty_moov",
                "-f",
                "mp4",
                "pipe:1",
            ])
            .output()
            .expect("FFmpeg is required for movie tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    #[test]
    fn movie_decodes_video_and_audio_positive_control() {
        let receiver = start(std::io::Cursor::new(synthetic_movie()));
        let mut frames = 0;
        let mut audio = None;
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
                Message::Audio(pcm) => {
                    assert!(pcm.iter().any(|&s| s != 0));
                    audio = Some(pcm);
                }
                Message::Frame(frame) => {
                    assert_eq!(frame.len(), WIDTH * HEIGHT * 4);
                    // FFmpeg vflip: letterbox bottom row is black; center is red.
                    assert!(frame[(HEIGHT / 2 * WIDTH + WIDTH / 2) * 4] > 200);
                    assert!(frame[0] < 10);
                    frames += 1;
                }
                Message::End => break,
                Message::Error(error) => panic!("{error}"),
            }
        }
        assert!((60..=61).contains(&frames));
        let duration = audio.unwrap().len() as f64 / (SAMPLE_RATE as f64 * CHANNELS as f64);
        assert!((duration - 2.0).abs() < 0.1);
    }

    #[test]
    fn movie_cancels_during_preroll() {
        let input = synthetic_movie();
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancelled.clone();
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = decode(std::io::Cursor::new(input), &sender, &worker_cancel);
            done_tx.send(result).unwrap();
        });
        // Cancel before consuming audio: this must terminate even if the
        // worker is still preparing PCM, not merely blocked on video delivery.
        std::thread::sleep(Duration::from_millis(5));
        cancelled.store(true, Ordering::Release);
        drop(receiver);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
    }

    #[test]
    fn movie_cancelled_queue_terminates_worker() {
        let input = synthetic_movie();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = decode(
                std::io::Cursor::new(input),
                &sender,
                &AtomicBool::new(false),
            );
            done_tx.send(result).unwrap();
        });
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
            Message::Audio(_)
        ));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
            Message::Frame(_)
        ));
        // The second frame fills the bounded queue. Dropping the receiver
        // releases send and the worker must kill AND wait for its FFmpeg child.
        drop(receiver);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
    }
}
