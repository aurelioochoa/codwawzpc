/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Counts relayed game traffic, so its rate and latency can be measured from
//! the `touchHLE::codwawzpc::coop::relay` debug log (see `docs/coop-join.md`).
use codwawzpc_coop_proto::Frame;
use std::time::{Duration, Instant};

/// How often [Meter::record] reports.
pub const WINDOW: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Out,
    In,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Datagram,
    Stream,
}

/// Frames and bytes per direction and kind over one [WINDOW].
pub struct Meter {
    start: Instant,
    counts: [[(u64, u64); 2]; 2],
}
impl Meter {
    pub fn new(now: Instant) -> Self {
        Self {
            start: now,
            counts: [[(0, 0); 2]; 2],
        }
    }
    /// Counts one frame. Once a window has passed, returns its summary and
    /// starts the next window.
    pub fn record(&mut self, now: Instant, dir: Dir, kind: Kind, len: usize) -> Option<String> {
        let count = &mut self.counts[kind as usize][dir as usize];
        count.0 += 1;
        count.1 += len as u64;
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed < WINDOW {
            return None;
        }
        let seconds = elapsed.as_secs_f64();
        let mut parts = Vec::new();
        for (kind, name) in [(Kind::Datagram, "datagram"), (Kind::Stream, "stream")] {
            for (dir, way) in [(Dir::Out, "out"), (Dir::In, "in")] {
                let (frames, bytes) = self.counts[kind as usize][dir as usize];
                parts.push(format!(
                    "{way} {name} {frames} frames {bytes} B ({:.1}/s)",
                    frames as f64 / seconds
                ));
            }
        }
        *self = Self::new(now);
        Some(format!("traffic over {seconds:.1} s: {}", parts.join(", ")))
    }
}

/// The log line for one relayed game frame: a stream's frames end with its
/// local id (each end numbers a stream differently), datagrams have none.
pub fn frame_line(dir: Dir, stream: Option<u32>, payload: &[u8], micros: u128) -> String {
    let kind = if stream.is_some() {
        Kind::Stream
    } else {
        Kind::Datagram
    };
    let mut line = format!(
        "{dir:?} {kind:?} {} B fp {:08x} at {micros} us",
        payload.len(),
        fingerprint(payload)
    );
    if let Some(id) = stream {
        line += &format!(" on stream {id}");
    }
    line
}

/// The log line for a sent or received control frame, or `None` for game
/// data and keep-alives, which [frame_line] and the [Meter] cover.
pub fn control_line(dir: Dir, frame: &Frame, micros: u128) -> Option<String> {
    match frame {
        Frame::Ping | Frame::Pong | Frame::Datagram { .. } | Frame::StreamData { .. } => None,
        _ => Some(format!("{dir:?} {frame:?} at {micros} us")),
    }
}

/// FNV-1a (32-bit) of a payload, to match a frame in the sender's and the
/// receiver's logs.
pub fn fingerprint(payload: &[u8]) -> u32 {
    payload.iter().fold(0x811c9dc5, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codwawzpc_coop_proto::VAddr;

    #[test]
    fn fingerprint_is_fnv1a() {
        assert_eq!(fingerprint(b""), 0x811c9dc5);
        assert_eq!(fingerprint(b"a"), 0xe40c292c);
        assert_eq!(fingerprint(b"foobar"), 0xbf9cf968);
    }

    #[test]
    fn meter_reports_each_window_then_resets() {
        let t0 = Instant::now();
        let mut meter = Meter::new(t0);
        assert_eq!(meter.record(t0, Dir::Out, Kind::Datagram, 100), None);
        assert_eq!(meter.record(t0, Dir::Out, Kind::Datagram, 20), None);
        assert_eq!(meter.record(t0, Dir::In, Kind::Stream, 5), None);
        let report = meter.record(t0 + WINDOW, Dir::In, Kind::Datagram, 7);
        assert_eq!(
            report.as_deref(),
            Some(
                "traffic over 5.0 s: out datagram 2 frames 120 B (0.4/s), \
                 in datagram 1 frames 7 B (0.2/s), out stream 0 frames 0 B (0.0/s), \
                 in stream 1 frames 5 B (0.2/s)"
            )
        );
        let later = t0 + WINDOW + Duration::from_secs(1);
        assert_eq!(meter.record(later, Dir::Out, Kind::Stream, 1), None);
    }

    #[test]
    fn stream_frame_line_ends_with_the_local_stream_id() {
        assert_eq!(
            frame_line(Dir::Out, Some(7), b"foobar", 42),
            "Out Stream 6 B fp bf9cf968 at 42 us on stream 7"
        );
    }

    #[test]
    fn datagram_frame_line_has_no_stream_id() {
        assert_eq!(
            frame_line(Dir::In, None, b"a", 5),
            "In Datagram 1 B fp e40c292c at 5 us"
        );
    }

    #[test]
    fn control_line_logs_stream_control_in_both_directions_with_a_time() {
        assert_eq!(
            control_line(Dir::Out, &Frame::StreamClose { stream: 3 }, 9).as_deref(),
            Some("Out StreamClose { stream: 3 } at 9 us")
        );
        assert_eq!(
            control_line(Dir::In, &Frame::StreamAccept { stream: 4 }, 10).as_deref(),
            Some("In StreamAccept { stream: 4 } at 10 us")
        );
    }

    #[test]
    fn control_line_skips_game_data_and_keep_alives() {
        let addr = VAddr {
            ip: [10, 77, 0, 2],
            port: 1,
        };
        for frame in [
            Frame::Ping,
            Frame::Pong,
            Frame::StreamData {
                stream: 1,
                payload: vec![0],
            },
            Frame::Datagram {
                src: addr,
                dst: addr,
                payload: vec![0],
            },
        ] {
            assert_eq!(control_line(Dir::Out, &frame, 1), None);
        }
    }
}
