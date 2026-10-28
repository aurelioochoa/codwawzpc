/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Presented-frame screenshots and background achievement recording.
//!
//! F10 saves one frame. `CODWAWZPC_CAPTURE_DIR` selects the output directory
//! (default: the log directory), and `CODWAWZPC_CAPTURE_INTERVAL_MS`
//! additionally saves a frame periodically, which makes unattended runs
//! reviewable.

mod recorder;

use crate::codwawzpc::capture_settings::{resolution_limit, CaptureSettings};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_READ_FRAMEBUFFER_BINDING: u32 = 0x8CAA;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_VERSION: u32 = 0x1F02;
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
const GL_STREAM_READ: u32 = 0x88E1;
const GL_READ_ONLY: u32 = 0x88B8;

pub(super) struct FrameCapture {
    directory: Option<PathBuf>,
    interval: Option<Duration>,
    last_periodic: Option<Instant>,
    requested: bool,
    count: u32,
    achievement_dir: Option<PathBuf>,
    achievement_screenshots: bool,
    achievement_clips: bool,
    achievement_ids: Vec<u32>,
    achievement_requested_at: Option<Instant>,
    settings: CaptureSettings,
    next_clip_frame: Option<Instant>,
    recorder: Option<recorder::Recorder>,
    clip_ids: Vec<u32>,
    readback: Readback,
    screenshot_jobs: Vec<std::thread::JoinHandle<()>>,
}

impl FrameCapture {
    pub fn from_env() -> Self {
        let directory = std::env::var_os("CODWAWZPC_CAPTURE_DIR")
            .map(PathBuf::from)
            .or_else(crate::paths::capture_directory);
        let interval = std::env::var("CODWAWZPC_CAPTURE_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis);
        let config = crate::codwawzpc::runtime::CONFIG.get();
        Self {
            directory,
            interval,
            last_periodic: None,
            requested: false,
            count: 0,
            achievement_dir: config.map(|c| c.data_directory.join("achievement-media")),
            achievement_screenshots: config.is_some_and(|c| c.settings.achievement_screenshots),
            achievement_clips: config.is_some_and(|c| c.settings.achievement_clips),
            achievement_ids: Vec::new(),
            achievement_requested_at: None,
            settings: config
                .map(|c| c.settings.achievement_capture.clone())
                .unwrap_or_default(),
            next_clip_frame: None,
            recorder: None,
            clip_ids: Vec::new(),
            readback: Readback::default(),
            screenshot_jobs: Vec::new(),
        }
    }

    pub fn request(&mut self) {
        self.requested = true;
    }

    pub fn request_achievement(&mut self, id: u32) {
        if !self.achievement_screenshots && !self.achievement_clips {
            return;
        }
        if self.achievement_screenshots && !self.achievement_ids.contains(&id) {
            self.achievement_ids.push(id);
            self.achievement_requested_at
                .get_or_insert_with(Instant::now);
        }
        if self.achievement_clips && !self.clip_ids.contains(&id) {
            self.clip_ids.push(id);
        }
    }

    /// Hand a frame (rows bottom-up, as OpenGL reads them) to the encoder.
    fn send_clip_frame(&mut self, frame: recorder::Frame) {
        if let Some(recorder) = &mut self.recorder {
            if !recorder.frame(frame) {
                self.achievement_clips = false;
            }
        }
    }

    fn due(&mut self) -> bool {
        let periodic = self.interval.is_some_and(|interval| {
            self.last_periodic
                .is_none_or(|last| last.elapsed() >= interval)
        });
        if periodic {
            self.last_periodic = Some(Instant::now());
        }
        std::mem::take(&mut self.requested) || periodic
    }

    /// Call with the window's GL context current, just before swapping.
    pub fn capture_if_due(
        &mut self,
        size: (u32, u32),
        get_proc_address: &dyn Fn(&str) -> *const c_void,
    ) {
        let ordinary = self.due();
        let screenshot_due = !self.achievement_ids.is_empty()
            && self
                .achievement_requested_at
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(250));
        let now = Instant::now();
        let next_clip_frame = self
            .achievement_clips
            .then(|| clip_deadline(self.next_clip_frame, now, self.settings.clip_fps))
            .flatten();
        let clip_due = next_clip_frame.is_some();
        if !ordinary && !screenshot_due && !clip_due {
            return;
        }
        let (width, height) = size;
        if width == 0 || height == 0 {
            return;
        }
        if clip_due {
            self.next_clip_frame = next_clip_frame;
            if self.recorder.is_none() {
                if let Some(root) = self.achievement_dir.clone() {
                    self.recorder = Some(recorder::Recorder::start(root, self.settings.clone()));
                }
            }
            if let Some(recorder) = &mut self.recorder {
                for id in self.clip_ids.drain(..) {
                    recorder.unlock(id);
                }
            }
        }
        let Some(gl) = Gl::load(get_proc_address) else {
            log!("Frame capture unavailable: missing GL entry points");
            return;
        };
        // A clip frame alone is read back asynchronously: waiting for the
        // GPU and copying the frame on every presented frame made gameplay
        // stutter while clips were recorded.
        if clip_due && !ordinary && !screenshot_due {
            if let Some(completed) = self.readback.read(&gl, size, now) {
                if let Some(frame) = completed {
                    self.send_clip_frame(frame);
                }
                return;
            }
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        unsafe {
            gl.read_window(size, pixels.as_mut_ptr().cast());
        }
        if clip_due {
            let frame = recorder::Frame {
                pixels: Arc::new(if ordinary || screenshot_due {
                    pixels.clone()
                } else {
                    std::mem::take(&mut pixels)
                }),
                size,
                at: now,
            };
            self.send_clip_frame(frame);
        }
        if !ordinary && !screenshot_due {
            return;
        }
        // OpenGL rows start at the bottom; images are stored top-down.
        let row = width as usize * 4;
        let mut flipped = vec![0u8; pixels.len()];
        for y in 0..height as usize {
            let source = (height as usize - 1 - y) * row;
            flipped[y * row..][..row].copy_from_slice(&pixels[source..][..row]);
        }
        // PNG compression and video encoding must never run on the game thread.
        self.screenshot_jobs.retain(|job| !job.is_finished());
        if screenshot_due {
            let ids = std::mem::take(&mut self.achievement_ids);
            if let Some(root) = self.achievement_dir.clone() {
                let pixels = flipped.clone();
                let settings = self.settings.clone();
                self.screenshot_jobs.push(std::thread::spawn(move || {
                    let (mut pixels, (width, height)) = resize_rgba(
                        &pixels,
                        width,
                        height,
                        resolution_limit(&settings.screenshot_resolution),
                    );
                    for id in ids {
                        let path = root
                            .join(format!("{id:03}"))
                            .join(format!("screenshot.{}", settings.screenshot_format));
                        let result = if settings.screenshot_format == "bmp" {
                            save_bmp(&path, &mut pixels, width, height)
                        } else {
                            save_png(&path, &pixels, width, height)
                        };
                        match result {
                            Ok(()) => {
                                log!("Saved achievement screenshot {}", path.display());
                            }
                            Err(e) => {
                                log!("Cannot save achievement screenshot {}: {e}", path.display());
                            }
                        }
                    }
                }));
            }
            self.achievement_requested_at = None;
        }
        if !ordinary {
            return;
        }
        let Some(directory) = self.directory.clone() else {
            log!("Frame capture requested, but no capture directory is configured");
            return;
        };
        self.count += 1;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let path = directory.join(format!("frame-{millis}-{:04}.bmp", self.count));
        let result = save_bmp(&path, &mut flipped, width, height);
        match result {
            Ok(()) => {
                log!("Saved frame capture {}", path.display());
            }
            Err(error) => {
                log!("Cannot save frame capture {}: {error}", path.display());
            }
        }
    }
}

type GetIntegerv = unsafe extern "C" fn(u32, *mut i32);
type GetString = unsafe extern "C" fn(u32) -> *const u8;
type BindFramebuffer = unsafe extern "C" fn(u32, u32);
type PixelStorei = unsafe extern "C" fn(u32, i32);
type ReadPixels = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
type GenBuffers = unsafe extern "C" fn(i32, *mut u32);
type BindBuffer = unsafe extern "C" fn(u32, u32);
type BufferData = unsafe extern "C" fn(u32, isize, *const c_void, u32);
type MapBuffer = unsafe extern "C" fn(u32, u32) -> *mut c_void;
type UnmapBuffer = unsafe extern "C" fn(u32) -> u8;

/// The GL entry points capture uses, in whatever context is current.
struct Gl {
    get: GetIntegerv,
    bind_framebuffer: BindFramebuffer,
    store: PixelStorei,
    read: ReadPixels,
    /// Pixel buffer objects, for asynchronous reads (not in OpenGL ES 1).
    buffers: Option<BufferFns>,
}

#[derive(Clone, Copy)]
struct BufferFns {
    get_string: GetString,
    gen: GenBuffers,
    bind: BindBuffer,
    data: BufferData,
    map: MapBuffer,
    unmap: UnmapBuffer,
}

impl Gl {
    fn load(get_proc_address: &dyn Fn(&str) -> *const c_void) -> Option<Self> {
        let get = |name| Some(get_proc_address(name)).filter(|p| !p.is_null());
        unsafe {
            Some(Self {
                get: std::mem::transmute::<*const c_void, GetIntegerv>(get("glGetIntegerv")?),
                bind_framebuffer: std::mem::transmute::<*const c_void, BindFramebuffer>(get(
                    "glBindFramebuffer",
                )?),
                store: std::mem::transmute::<*const c_void, PixelStorei>(get("glPixelStorei")?),
                read: std::mem::transmute::<*const c_void, ReadPixels>(get("glReadPixels")?),
                buffers: (|| {
                    Some(BufferFns {
                        get_string: std::mem::transmute::<*const c_void, GetString>(get(
                            "glGetString",
                        )?),
                        gen: std::mem::transmute::<*const c_void, GenBuffers>(get("glGenBuffers")?),
                        bind: std::mem::transmute::<*const c_void, BindBuffer>(get(
                            "glBindBuffer",
                        )?),
                        data: std::mem::transmute::<*const c_void, BufferData>(get(
                            "glBufferData",
                        )?),
                        map: std::mem::transmute::<*const c_void, MapBuffer>(get("glMapBuffer")?),
                        unmap: std::mem::transmute::<*const c_void, UnmapBuffer>(get(
                            "glUnmapBuffer",
                        )?),
                    })
                })(),
            })
        }
    }

    /// Read the window's framebuffer (RGBA, rows bottom-up) to `pixels`: a
    /// pointer, or an offset into the bound pixel pack buffer.
    unsafe fn read_window(&self, (width, height): (u32, u32), pixels: *mut c_void) {
        let mut previous_read = 0;
        let mut previous_alignment = 4;
        (self.get)(GL_READ_FRAMEBUFFER_BINDING, &mut previous_read);
        (self.get)(GL_PACK_ALIGNMENT, &mut previous_alignment);
        (self.bind_framebuffer)(GL_READ_FRAMEBUFFER, 0);
        (self.store)(GL_PACK_ALIGNMENT, 1);
        (self.read)(
            0,
            0,
            width as i32,
            height as i32,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels,
        );
        (self.store)(GL_PACK_ALIGNMENT, previous_alignment);
        (self.bind_framebuffer)(GL_READ_FRAMEBUFFER, previous_read as u32);
    }
}

/// Reads in flight per context. A frame is copied out two frames after it
/// was read, by when the GPU has long finished it, so neither the GPU nor
/// the game waits.
const READS_IN_FLIGHT: usize = 3;
/// Frame buffers kept for reuse: the encoder's queue, the frame it repeats
/// and the one being filled. Fresh buffers cost a page fault per 4 KiB.
const POOLED_FRAMES: usize = 6;

/// Asynchronous window reads through pixel buffer objects. GL objects belong
/// to the context that made them, and frames are presented from the game's
/// context or from the compositor's, so each context has its own buffers.
#[derive(Default)]
struct Readback {
    rings: Vec<ReadRing>,
    /// The context read from last.
    context: usize,
    pool: Vec<Arc<Vec<u8>>>,
}

struct ReadRing {
    context: usize,
    /// None when the context cannot read into buffers.
    buffers: Option<[u32; READS_IN_FLIGHT]>,
    size: (u32, u32),
    /// Buffer indices with the time of their frame, oldest first.
    pending: std::collections::VecDeque<(usize, Instant)>,
    next: usize,
}

impl Readback {
    /// Start reading the current frame. Returns None if this context cannot
    /// read asynchronously, else the oldest finished frame, if any.
    fn read(&mut self, gl: &Gl, size: (u32, u32), at: Instant) -> Option<Option<recorder::Frame>> {
        let BufferFns {
            get_string,
            gen,
            bind,
            data,
            map,
            unmap,
        } = gl.buffers?;
        let context = unsafe { sdl2::sys::SDL_GL_GetCurrentContext() } as usize;
        if context == 0 {
            return None;
        }
        let switched = std::mem::replace(&mut self.context, context) != context;
        let index = match self.rings.iter().position(|r| r.context == context) {
            Some(index) => index,
            None => {
                let version = unsafe { get_string(GL_VERSION) };
                let desktop = !version.is_null()
                    && !unsafe { std::ffi::CStr::from_ptr(version.cast()) }
                        .to_bytes()
                        .starts_with(b"OpenGL ES");
                let buffers = desktop.then(|| {
                    let mut buffers = [0; READS_IN_FLIGHT];
                    unsafe { gen(READS_IN_FLIGHT as i32, buffers.as_mut_ptr()) };
                    buffers
                });
                self.rings.push(ReadRing {
                    context,
                    buffers,
                    size: (0, 0),
                    pending: Default::default(),
                    next: 0,
                });
                self.rings.len() - 1
            }
        };
        let ring = &mut self.rings[index];
        let buffers = ring.buffers?;
        let length = size.0 as usize * size.1 as usize * 4;
        let mut previous_buffer = 0;
        unsafe { (gl.get)(GL_PIXEL_PACK_BUFFER_BINDING, &mut previous_buffer) };
        // Reads left behind in another context's turn are stale.
        if switched || ring.size != size {
            ring.pending.clear();
        }
        if ring.size != size {
            for &buffer in &buffers {
                unsafe {
                    bind(GL_PIXEL_PACK_BUFFER, buffer);
                    data(
                        GL_PIXEL_PACK_BUFFER,
                        length as isize,
                        std::ptr::null(),
                        GL_STREAM_READ,
                    );
                }
            }
            ring.size = size;
        }
        let mut completed = None;
        if ring.pending.len() == READS_IN_FLIGHT - 1 {
            let (oldest, at) = ring.pending.pop_front().unwrap();
            unsafe {
                bind(GL_PIXEL_PACK_BUFFER, buffers[oldest]);
                let mapped = map(GL_PIXEL_PACK_BUFFER, GL_READ_ONLY);
                if !mapped.is_null() {
                    let source = std::slice::from_raw_parts(mapped.cast::<u8>(), length);
                    completed = Some(recorder::Frame {
                        pixels: pooled(&mut self.pool, source),
                        size,
                        at,
                    });
                }
                unmap(GL_PIXEL_PACK_BUFFER);
            }
        }
        let ring = &mut self.rings[index];
        unsafe {
            bind(GL_PIXEL_PACK_BUFFER, buffers[ring.next]);
            gl.read_window(size, std::ptr::null_mut());
            bind(GL_PIXEL_PACK_BUFFER, previous_buffer as u32);
        }
        ring.pending.push_back((ring.next, at));
        ring.next = (ring.next + 1) % READS_IN_FLIGHT;
        Some(completed)
    }
}

/// A copy of `source` in a buffer the encoder has finished with, if any.
fn pooled(pool: &mut Vec<Arc<Vec<u8>>>, source: &[u8]) -> Arc<Vec<u8>> {
    pool.retain(|pixels| pixels.len() == source.len());
    let index = if let Some(index) = pool.iter().position(|p| Arc::strong_count(p) == 1) {
        Arc::get_mut(&mut pool[index])
            .unwrap()
            .copy_from_slice(source);
        index
    } else if pool.len() < POOLED_FRAMES {
        pool.push(Arc::new(source.to_vec()));
        pool.len() - 1
    } else {
        return Arc::new(source.to_vec());
    };
    pool[index].clone()
}

fn clip_deadline(previous: Option<Instant>, now: Instant, fps: u32) -> Option<Instant> {
    let interval = Duration::from_nanos(1_000_000_000 / fps as u64);
    if previous.is_some_and(|next| now + interval / 2 < next) {
        return None;
    }
    // A fixed deadline and half-frame tolerance absorb presentation jitter.
    // Using `now + interval` would frequently turn a 60 Hz source into 30 FPS.
    let mut next = previous.unwrap_or(now) + interval;
    if next <= now {
        next +=
            interval.mul_f64(((now - next).as_secs_f64() / interval.as_secs_f64()).floor() + 1.0);
    }
    Some(next)
}

/// Bilinear downsampling avoids the jagged edges of the old nearest-neighbor clips.
fn resize_rgba(
    source: &[u8],
    width: u32,
    height: u32,
    limit: Option<(u32, u32)>,
) -> (Vec<u8>, (u32, u32)) {
    let Some((max_width, max_height)) = limit else {
        return (source.to_vec(), (width, height));
    };
    let scale = (width as f64 / max_width as f64)
        .max(height as f64 / max_height as f64)
        .max(1.0);
    let tw = ((width as f64 / scale).round() as u32).max(1);
    let th = ((height as f64 / scale).round() as u32).max(1);
    if (tw, th) == (width, height) {
        return (source.to_vec(), (width, height));
    }
    let mut result = vec![0; tw as usize * th as usize * 4];
    for y in 0..th {
        let sy = ((y as f64 + 0.5) * height as f64 / th as f64 - 0.5).max(0.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(height - 1);
        let fy = sy - y0 as f64;
        for x in 0..tw {
            let sx = ((x as f64 + 0.5) * width as f64 / tw as f64 - 0.5).max(0.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(width - 1);
            let fx = sx - x0 as f64;
            for c in 0..4 {
                let at = |x, y| source[((y * width + x) * 4 + c) as usize] as f64;
                let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                result[((y * tw + x) * 4 + c) as usize] =
                    (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    (result, (tw, th))
}

fn save_bmp(
    path: &std::path::Path,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("Capture path has no parent")?)
        .map_err(|e| e.to_string())?;
    let surface = sdl2::surface::Surface::from_data(
        pixels,
        width,
        height,
        width * 4,
        sdl2::pixels::PixelFormatEnum::ABGR8888,
    )
    .map_err(|e| e.to_string())?;
    surface.save_bmp(path).map_err(|e| e.to_string())
}

fn save_png(path: &std::path::Path, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    use std::ffi::c_int;
    unsafe extern "C" fn write(context: *mut c_void, data: *mut c_void, length: c_int) {
        let output = unsafe { &mut *context.cast::<Vec<u8>>() };
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length as usize) };
        output.extend_from_slice(bytes);
    }
    // The window framebuffer's alpha is not meaningful. Store opaque RGB so
    // screenshots don't disappear in image viewers when GL returns alpha 0.
    let rgb: Vec<u8> = pixels
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    let mut encoded = Vec::<u8>::new();
    let success = unsafe {
        touchHLE_stb_image_wrapper::stbi_write_png_to_func(
            write,
            (&mut encoded as *mut Vec<u8>).cast(),
            width as i32,
            height as i32,
            3,
            rgb.as_ptr().cast(),
            width as i32 * 3,
        )
    };
    if success == 0 {
        return Err("PNG encoding failed".into());
    }
    std::fs::create_dir_all(path.parent().ok_or("Capture path has no parent")?)
        .map_err(|e| e.to_string())?;
    let temporary = path.with_extension("png.tmp");
    std::fs::write(&temporary, encoded)
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| e.to_string())
}

impl Drop for FrameCapture {
    fn drop(&mut self) {
        // Dropping the recorder closes its queue, flushes its encoder and exports
        // a final partial clip if the game exits during an unlock.
        drop(self.recorder.take());
        for job in self.screenshot_jobs.drain(..) {
            let _ = job.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_jitter_does_not_halve_60fps_sampling() {
        let start = Instant::now();
        let mut deadline = None;
        let mut sampled = 0;
        for index in 0..600 {
            let jitter = if index % 2 == 0 { 0.002 } else { -0.002 };
            let at = start + Duration::from_secs_f64(index as f64 / 60.0 + 0.002 + jitter);
            if let Some(next) = clip_deadline(deadline, at, 60) {
                deadline = Some(next);
                sampled += 1;
            }
            assert!(clip_deadline(deadline, at + Duration::from_millis(2), 60).is_none());
        }
        assert_eq!(sampled, 600);
    }

    #[test]
    fn unlock_captures_the_presented_frame_with_correct_orientation() {
        unsafe extern "C" fn get(_name: u32, value: *mut i32) {
            unsafe {
                *value = 4;
            }
        }
        unsafe extern "C" fn bind(_target: u32, _buffer: u32) {}
        unsafe extern "C" fn store(_name: u32, _value: i32) {}
        unsafe extern "C" fn read(
            _x: i32,
            _y: i32,
            width: i32,
            height: i32,
            _format: u32,
            _kind: u32,
            pixels: *mut c_void,
        ) {
            let pixels = unsafe {
                std::slice::from_raw_parts_mut(pixels.cast::<u8>(), (width * height * 4) as usize)
            };
            for (row, pixels) in pixels.chunks_exact_mut(width as usize * 4).enumerate() {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel.copy_from_slice(&[22, row as u8, 66, 0]);
                }
            }
        }
        let root = std::env::temp_dir().join(format!("capture-frame-{}", uuid::Uuid::new_v4()));
        let mut capture = FrameCapture::from_env();
        capture.directory = None;
        capture.interval = None;
        capture.achievement_dir = Some(root.clone());
        capture.achievement_screenshots = true;
        capture.achievement_clips = false;
        capture.request_achievement(10);
        capture.achievement_requested_at = Some(Instant::now() - Duration::from_millis(300));
        capture.capture_if_due((8, 4), &|name| match name {
            "glGetIntegerv" => get as *const c_void,
            "glBindFramebuffer" => bind as *const c_void,
            "glPixelStorei" => store as *const c_void,
            "glReadPixels" => read as *const c_void,
            _ => std::ptr::null(),
        });
        drop(capture);
        let data = std::fs::read(root.join("010/screenshot.png")).unwrap();
        let image = crate::image::Image::from_bytes(&data).unwrap();
        assert_eq!(&image.pixels()[..4], &[22, 3, 66, 255]);
        assert_eq!(&image.pixels()[8 * 3 * 4..][..4], &[22, 0, 66, 255]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn screenshots_are_lossless_opaque_and_resizing_preserves_aspect() {
        let root = std::env::temp_dir().join(format!("capture-png-{}", uuid::Uuid::new_v4()));
        let pixels = [12, 34, 56, 0].repeat(8 * 4);
        let (smaller, size) = resize_rgba(&pixels, 8, 4, Some((4, 4)));
        assert_eq!(size, (4, 2));
        assert_eq!(smaller, [12, 34, 56, 0].repeat(8));
        assert_eq!(resize_rgba(&pixels, 8, 4, Some((1920, 1080))).1, (8, 4));
        let file = root.join("screenshot.png");
        save_png(&file, &pixels, 8, 4).unwrap();
        let data = std::fs::read(&file).unwrap();
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
        // Decode independently through the existing image reader, not the writer.
        let decoded = crate::image::Image::from_bytes(&data).unwrap();
        assert_eq!(decoded.dimensions(), (8, 4));
        assert_eq!(decoded.pixels(), [12, 34, 56, 255].repeat(32));
        let mut bmp = pixels.clone();
        save_bmp(&root.join("screenshot.bmp"), &mut bmp, 8, 4).unwrap();
        assert!(std::fs::read(root.join("screenshot.bmp"))
            .unwrap()
            .starts_with(b"BM"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn downsampling_blends_edges_instead_of_aliasing() {
        let checker = [0, 0, 0, 255, 255, 255, 255, 255].repeat(2);
        let (pixels, size) = resize_rgba(&checker, 2, 2, Some((1, 1)));
        assert_eq!(size, (1, 1));
        assert_eq!(pixels, [128, 128, 128, 255]);
    }
}
