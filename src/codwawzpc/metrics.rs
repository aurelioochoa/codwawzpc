/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Development measurements: load milestones and, optionally, actual host
//! presentation intervals.
use std::io::{BufWriter, Write};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Start the clock that [milestone] measures from.
pub(crate) fn start() {
    LazyLock::force(&STARTED);
}

/// Log (with `CODWAWZPC_TRACE=touchHLE::codwawzpc::metrics`) when the game
/// starts reading another archive. It reopens each one several times in a
/// row; only the first of a run is logged.
pub(crate) fn archive_opened(path: &str) {
    static LAST: Mutex<String> = Mutex::new(String::new());
    let mut last = LAST.lock().unwrap();
    if *last != path {
        path.clone_into(&mut last);
        let name = path.rsplit('/').next().unwrap_or(path);
        log_dbg!(
            "Milestone: open {} at {} ms",
            name,
            STARTED.elapsed().as_millis()
        );
    }
}

/// Log a point in the game's startup or loading, in ms since [start].
pub(crate) fn milestone(what: std::fmt::Arguments) {
    log!(
        "Milestone: {} at {} ms",
        what,
        STARTED.elapsed().as_millis()
    );
}

struct Trace {
    writer: BufWriter<std::fs::File>,
    started: Instant,
    previous: Instant,
    frames: u64,
}

pub(crate) fn presented() {
    static TRACE: LazyLock<Option<Mutex<Trace>>> = LazyLock::new(|| {
        let path = std::env::var_os("CODWAWZPC_FRAME_TRACE")?;
        let result = (|| -> std::io::Result<Trace> {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let mut writer = BufWriter::new(file);
            writeln!(writer, "elapsed_ms,frame_ms")?;
            let now = Instant::now();
            Ok(Trace {
                writer,
                started: now,
                previous: now,
                frames: 0,
            })
        })();
        match result {
            Ok(trace) => Some(Mutex::new(trace)),
            Err(e) => {
                eprintln!("Cannot open frame trace: {e}");
                None
            }
        }
    });
    let Some(trace) = TRACE.as_ref() else {
        return;
    };
    let Ok(mut trace) = trace.lock() else {
        return;
    };
    let now = Instant::now();
    let elapsed = now.duration_since(trace.started).as_secs_f64() * 1000.0;
    let interval = now.duration_since(trace.previous).as_secs_f64() * 1000.0;
    trace.previous = now;
    trace.frames += 1;
    let _ = writeln!(trace.writer, "{elapsed:.6},{interval:.6}");
    if trace.frames.is_multiple_of(60) {
        let _ = trace.writer.flush();
    }
}
