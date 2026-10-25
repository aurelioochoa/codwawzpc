/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Development aid: a sampling profiler that needs no `perf` permissions.
//!
//! With `CODWAWZPC_PROFILE=<file>`, a `SIGPROF` timer samples the
//! instruction pointer of whichever thread is using CPU, about once per
//! millisecond of CPU time. At exit the samples are written to `<file>` as
//! `<ms since start> <address> <main thread: 1|0>` lines, followed by
//! `/proc/self/maps`. Dynarmic names its translated blocks by guest PC in
//! `$PERF_BUILDID_DIR/perf-<pid>.map`, so guest code can be attributed too.

#[cfg(target_os = "linux")]
mod imp {
    use std::io::Write;
    use std::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
    use std::sync::OnceLock;

    const CAPACITY: usize = 1 << 21;
    static PATH: OnceLock<String> = OnceLock::new();
    static STARTED_NS: AtomicU64 = AtomicU64::new(0);
    static MAIN_TID: AtomicI32 = AtomicI32::new(0);
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    static SAMPLES: OnceLock<Box<[AtomicU64]>> = OnceLock::new();

    fn now_ns() -> u64 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
    }

    extern "C" fn on_sample(_: libc::c_int, _: *mut libc::siginfo_t, context: *mut libc::c_void) {
        let Some(samples) = SAMPLES.get() else {
            return;
        };
        let index = COUNT.fetch_add(1, Ordering::Relaxed);
        if index * 2 + 1 >= samples.len() {
            return;
        }
        let context = unsafe { &*(context as *const libc::ucontext_t) };
        let rip = context.uc_mcontext.gregs[libc::REG_RIP as usize] as u64;
        let ms = (now_ns() - STARTED_NS.load(Ordering::Relaxed)) / 1_000_000;
        let main = unsafe { libc::gettid() } == MAIN_TID.load(Ordering::Relaxed);
        samples[index * 2].store(rip, Ordering::Relaxed);
        samples[index * 2 + 1].store(ms << 1 | u64::from(main), Ordering::Relaxed);
    }

    extern "C" fn dump() {
        let (Some(path), Some(samples)) = (PATH.get(), SAMPLES.get()) else {
            return;
        };
        let stop = libc::itimerval {
            it_interval: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            it_value: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
        };
        unsafe { libc::setitimer(libc::ITIMER_PROF, &stop, std::ptr::null_mut()) };
        let count = COUNT.load(Ordering::Relaxed).min(samples.len() / 2);
        let result = (|| -> std::io::Result<()> {
            let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
            for i in 0..count {
                let rip = samples[i * 2].load(Ordering::Relaxed);
                let tagged = samples[i * 2 + 1].load(Ordering::Relaxed);
                writeln!(out, "{} {:x} {}", tagged >> 1, rip, tagged & 1)?;
            }
            writeln!(out, "# maps")?;
            out.write_all(&std::fs::read("/proc/self/maps")?)?;
            out.flush()
        })();
        match result {
            Ok(()) => eprintln!("Profile: {count} samples written to {path}"),
            Err(e) => eprintln!("Profile {path} failed: {e}"),
        }
    }

    pub fn start() {
        let Ok(path) = std::env::var("CODWAWZPC_PROFILE") else {
            return;
        };
        if PATH.set(path).is_err() {
            return;
        }
        SAMPLES.get_or_init(|| (0..CAPACITY * 2).map(|_| AtomicU64::new(0)).collect());
        STARTED_NS.store(now_ns(), Ordering::Relaxed);
        MAIN_TID.store(unsafe { libc::gettid() }, Ordering::Relaxed);
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_sample as extern "C" fn(_, _, _) as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(libc::SIGPROF, &action, std::ptr::null_mut());
            libc::atexit(dump);
            let period = libc::timeval {
                tv_sec: 0,
                tv_usec: 1000,
            };
            let timer = libc::itimerval {
                it_interval: period,
                it_value: period,
            };
            libc::setitimer(libc::ITIMER_PROF, &timer, std::ptr::null_mut());
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use imp::start;

#[cfg(not(target_os = "linux"))]
pub(crate) fn start() {}
