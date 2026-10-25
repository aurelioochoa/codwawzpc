/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `MPMoviePlayerController` etc.

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::foundation::{ns_string, ns_url, NSInteger, NSTimeInterval};
use crate::frameworks::uikit::ui_device::UIDeviceOrientation;
use crate::objc::{
    id, msg, msg_class, nil, objc_classes, release, retain, todo_objc_setter, ClassExports,
    HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::VecDeque;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

mod decoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishReason {
    Ended = 0,
    Failed = 1,
    UserExited = 2,
}

/// Clock stops while paused, so resuming never drops the paused interval's frames.
struct PlaybackClock {
    started: Instant,
    paused_at: Option<Instant>,
    paused: Duration,
}
impl PlaybackClock {
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            paused_at: None,
            paused: Duration::ZERO,
        }
    }
    fn elapsed(&self, now: Instant) -> Duration {
        self.paused_at
            .unwrap_or(now)
            .saturating_duration_since(self.started)
            .saturating_sub(self.paused)
    }
    fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
    }
    fn resume(&mut self, now: Instant) {
        if let Some(since) = self.paused_at.take() {
            self.paused += now.saturating_duration_since(since);
        }
    }
}

struct Playback {
    receiver: decoder::Decoder,
    audio: Option<Vec<i16>>,
    clock: Option<PlaybackClock>,
    pending_frame: Option<Vec<u8>>,
    frame_count: u32,
    decoder_ended: bool,
    paused: bool,
}
impl Playback {
    fn new(receiver: decoder::Decoder) -> Self {
        Self {
            receiver,
            audio: None,
            clock: None,
            pending_frame: None,
            frame_count: 0,
            decoder_ended: false,
            paused: false,
        }
    }
}

struct Notification {
    name: &'static str,
    object: id,
    time: Instant,
    reason: Option<FinishReason>,
}

#[derive(Default)]
pub struct State {
    active_player: Option<id>,
    /// Various apps (e.g. Crash Bandicoot Nitro Kart 3D and Spore Origins)
    /// create or start a player and await some kind of notification, but can't
    /// handle it if that notification happens immediately. This queue lets us
    /// delay such notifications until the app next returns to the run loop,
    /// which seems to be late enough.
    pending_notifications: VecDeque<Notification>,
    playback: Option<Playback>,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.framework_state.media_player.movie_player
    }
}

type MPMovieScalingMode = NSInteger;
type MPMovieControlStyle = NSInteger;

type MPMoviePlaybackState = NSInteger;
const MPMoviePlaybackStateStopped: MPMoviePlaybackState = 0;

// Values might not be correct, but as these are linked symbol constants, it
// shouldn't matter.
pub const MPMoviePlayerPlaybackDidFinishNotification: &str =
    "MPMoviePlayerPlaybackDidFinishNotification";
/// Apparently an undocumented, private API. Spore Origins uses it.
pub const MPMoviePlayerContentPreloadDidFinishNotification: &str =
    "MPMoviePlayerContentPreloadDidFinishNotification";
pub const MPMoviePlayerScalingModeDidChangeNotification: &str =
    "MPMoviePlayerScalingModeDidChangeNotification";
// TODO: More notifications?
const MPMoviePlayerPlaybackDidFinishReasonUserInfoKey: &str =
    "MPMoviePlayerPlaybackDidFinishReasonUserInfoKey";

/// `NSNotificationName` values and other constants.
pub const CONSTANTS: ConstantExports = &[
    (
        "_MPMoviePlayerPlaybackDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishNotification),
    ),
    (
        "_MPMoviePlayerContentPreloadDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerContentPreloadDidFinishNotification),
    ),
    (
        "_MPMoviePlayerScalingModeDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerScalingModeDidChangeNotification),
    ),
    (
        "_MPMoviePlayerPlaybackDidFinishReasonUserInfoKey",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishReasonUserInfoKey),
    ),
];

struct MPMoviePlayerControllerHostObject {
    // NSURL *
    content_url: id,
}
impl HostObject for MPMoviePlayerControllerHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation MPMoviePlayerController: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(MPMoviePlayerControllerHostObject {
        content_url: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithContentURL:(id)url { // NSURL*
    let is_file: bool = url != nil && msg![env; url isFileURL];
    let path = if is_file { Some(ns_url::to_rust_path(env, url)) } else { None };
    log!(
        "[(MPMoviePlayerController*){:?} initWithContentURL:{:?} ({:?})]",
        this,
        url,
        path,
    );

    retain(env, url);
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).content_url = url;

    // Act as if loading immediately completed (Spore Origins waits for this).
    queue_notification(env, MPMoviePlayerContentPreloadDidFinishNotification, this, Instant::now(), None);

    this
}

- (())dealloc {
    let url = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).content_url;
    release(env, url);

    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)contentURL {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).content_url
}

- (id)backgroundColor {
    msg_class![env; UIColor blackColor] // TODO
}
- (())setBackgroundColor:(id)color { // UIColor*
    todo_objc_setter!(this, color);
}

- (())setScalingMode:(MPMovieScalingMode)mode {
    todo_objc_setter!(this, mode);
}
- (())setUseApplicationAudioSession:(bool)use_session {
    todo_objc_setter!(this, use_session);
}
- (())setControlStyle:(MPMovieControlStyle)style {
    todo_objc_setter!(this, style);
}
- (())setFullscreen:(bool)fullsreen {
    todo_objc_setter!(this, fullsreen);
}
- (())setInitialPlaybackTime:(NSTimeInterval)initial_time {
    todo_objc_setter!(this, initial_time);
}

- (id)view {
    nil // TODO
}

- (MPMoviePlaybackState)playbackState {
    if crate::codwawzpc::runtime::CONFIG.get().is_some() && State::get(env).active_player == Some(this) {
        if State::get(env).playback.as_ref().is_some_and(|p| p.paused) { 2 } else { 1 }
    } else { MPMoviePlaybackStateStopped }
}

// Apparently an undocumented, private API, but Spore Origins uses it.
- (())setMovieControlMode:(NSInteger)_mode {
    // The supplied game uses this private selector. Its controls are handled
    // by the runtime window (touch/click/Escape to skip).
}

// Another undocumented one! But some apps may still use it :/
// https://stackoverflow.com/a/1390079/2241008
- (())setOrientation:(UIDeviceOrientation)_orientation animated:(bool)_animated {

}

// MPMediaPlayback implementation
- (())play {
    if crate::codwawzpc::runtime::CONFIG.get().is_some() {
        play_local(env, this);
        return;
    }
    log!("TODO: [(MPMoviePlayerController*){:?} play]", this);
    if let Some(old) = env.framework_state.media_player.movie_player.active_player {
        let _: () = msg![env; old stop];
    }
    assert!(env.framework_state.media_player.movie_player.active_player.is_none());
    // Movie player is retained by the runtime until it is stopped
    retain(env, this);
    env.framework_state.media_player.movie_player.active_player = Some(this);

    // Act as if playback immediately completed after 1 second
    // (various apps wait for this, such as BIA and Hero of Sparta).
    if !State::get(env).pending_notifications.iter().any(|n|
        n.name == MPMoviePlayerPlaybackDidFinishNotification && n.object == this) {
        queue_notification(env, MPMoviePlayerPlaybackDidFinishNotification, this,
            Instant::now() + Duration::from_secs(1), None);
    }
}

- (())pause {
    if crate::codwawzpc::runtime::CONFIG.get().is_none() {
        log!("TODO: [(MPMoviePlayerController*){:?} pause]", this);
        return;
    }
    if State::get(env).active_player == Some(this) {
        if let Some(playback) = State::get(env).playback.as_mut() {
            playback.paused = true;
            if let Some(clock) = &mut playback.clock { clock.pause(Instant::now()); }
        }
        env.on_parent_stack_in_coroutine(|window, _| window.movie_pause());
    }
}

- (())stop {
    if crate::codwawzpc::runtime::CONFIG.get().is_some() {
        if State::get(env).active_player == Some(this) { finish_local(env, FinishReason::UserExited); }
        return;
    }
    log!("TODO: [(MPMoviePlayerController*){:?} stop]", this);
    if env.framework_state.media_player.movie_player.active_player.is_some() {
        // Some applications (like NOVA2) may send 2 `stop` messages for each
        // 1 `play` message for the player. In that case, we want to release
        // the active player only once.
        assert!(this == env.framework_state.media_player.movie_player.active_player.take().unwrap());
        release(env, this);
    }
}

@end

@implementation MPMoviePlayerViewController: UIViewController

- (id)initWithContentURL:(id)url {
    log!(
        "TODO: [(MPMoviePlayerViewController*){:?} initWithContentURL:{:?} ({:?})] -> nil",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );
    release(env, this);
    nil // TODO
}

@end

};

/// For use by `NSRunLoop` via [super::handle_players]: check movie players'
/// status, send notifications if necessary.
fn queue_notification(
    env: &mut Environment,
    name: &'static str,
    object: id,
    time: Instant,
    reason: Option<FinishReason>,
) {
    // A queued notification outlives the guest's reference (some apps release
    // their controller immediately). Pair this with release after dispatch.
    retain(env, object);
    State::get(env)
        .pending_notifications
        .push_back(Notification {
            name,
            object,
            time,
            reason,
        });
}

fn play_local(env: &mut Environment, player: id) {
    if State::get(env).active_player == Some(player) {
        if let Some(playback) = State::get(env).playback.as_mut() {
            playback.paused = false;
            if let Some(clock) = &mut playback.clock {
                clock.resume(Instant::now());
            }
        }
        env.on_parent_stack_in_coroutine(|window, _| window.movie_resume());
        return;
    }
    if State::get(env).active_player.is_some() {
        finish_local(env, FinishReason::UserExited);
    }
    retain(env, player);
    State::get(env).active_player = Some(player);
    if env.window.is_none() {
        log!("Movie playback requires a runtime window");
        finish_local(env, FinishReason::Failed);
        return;
    }
    let url = env
        .objc
        .borrow::<MPMoviePlayerControllerHostObject>(player)
        .content_url;
    let is_file: bool = url != nil && msg![env; url isFileURL];
    let input = if is_file {
        let path = ns_url::to_rust_path(env, url);
        env.fs.open(path).ok()
    } else {
        None
    };
    let Some(input) = input else {
        log!("Local movie URL is missing or unreadable");
        finish_local(env, FinishReason::Failed);
        return;
    };
    // GuestFile may contain Rc-backed IPA data and cannot cross threads. Copy
    // only this bounded, read-only movie into a Send buffer before decoding.
    use std::io::Read;
    let mut bytes = Vec::new();
    if input
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > 64 * 1024 * 1024
    {
        finish_local(env, FinishReason::Failed);
        return;
    }
    State::get(env).playback = Some(Playback::new(decoder::start(std::io::Cursor::new(bytes))));
    env.on_parent_stack_in_coroutine(|window, _| {
        window.clear_desktop_input();
        window.set_movie_active(true);
    });
}

fn finish_local(env: &mut Environment, reason: FinishReason) {
    let Some(player) = State::get(env).active_player.take() else {
        return;
    };
    // All SDL resources are released on the parent stack. Dropping the receiver
    // cancels the bounded decoder worker, which owns and reaps its FFmpeg child.
    drop(State::get(env).playback.take());
    if env.window.is_some() {
        env.on_parent_stack_in_coroutine(|window, _| window.set_movie_active(false));
    }
    log!("Movie playback finished: {:?}", reason);
    queue_notification(
        env,
        MPMoviePlayerPlaybackDidFinishNotification,
        player,
        Instant::now(),
        Some(reason),
    );
    release(env, player);
}

fn update_local(env: &mut Environment) {
    let Some(mut playback) = State::get(env).playback.take() else {
        return;
    };
    let (result, playback) = env.on_parent_stack_in_coroutine(move |window, options| {
        let result = (|| {
            window.poll_for_events(options);
            if window.take_movie_skip() {
                return Some(FinishReason::UserExited);
            }
            if playback.paused {
                return None;
            }
            // Drain only frames already due. At most three frames are queued by
            // the worker, with one additional pending frame here.
            loop {
                if let Some(frame) = playback.pending_frame.take() {
                    let now = Instant::now();
                    let elapsed = playback
                        .clock
                        .as_ref()
                        .map_or(Duration::ZERO, |c| c.elapsed(now));
                    if elapsed.as_secs_f64()
                        < f64::from(playback.frame_count) / f64::from(decoder::FPS)
                    {
                        playback.pending_frame = Some(frame);
                        return None;
                    }
                    if playback.clock.is_none() {
                        let Some(audio) = playback.audio.take() else {
                            return Some(FinishReason::Failed);
                        };
                        if let Err(error) =
                            window.movie_start_audio(&audio, crate::codwawzpc::runtime::volume())
                        {
                            log!("Movie audio output failed: {error}");
                            return Some(FinishReason::Failed);
                        }
                    }
                    window.present_movie_frame(
                        &frame,
                        decoder::WIDTH as u32,
                        decoder::HEIGHT as u32,
                    );
                    if playback.clock.is_none() {
                        playback.clock = Some(PlaybackClock::new(Instant::now()));
                        window.movie_resume();
                    }
                    playback.frame_count += 1;
                }
                if playback.decoder_ended {
                    let elapsed = playback
                        .clock
                        .as_ref()
                        .map_or(Duration::ZERO, |c| c.elapsed(Instant::now()));
                    if elapsed.as_secs_f64()
                        >= f64::from(playback.frame_count) / f64::from(decoder::FPS)
                        && window.movie_audio_finished()
                    {
                        return Some(FinishReason::Ended);
                    }
                    return None;
                }
                match playback.receiver.try_recv() {
                    Ok(decoder::Message::Audio(samples)) => playback.audio = Some(samples),
                    Ok(decoder::Message::Frame(frame)) => playback.pending_frame = Some(frame),
                    Ok(decoder::Message::End) => playback.decoder_ended = true,
                    Ok(decoder::Message::Error(error)) => {
                        log!("Movie decoder failed: {error}");
                        return Some(FinishReason::Failed);
                    }
                    Err(TryRecvError::Empty) => return None,
                    Err(TryRecvError::Disconnected) => return Some(FinishReason::Failed),
                }
            }
        })();
        (result, playback)
    });
    State::get(env).playback = Some(playback);
    if let Some(reason) = result {
        finish_local(env, reason);
    }
}

pub(super) fn handle_players(env: &mut Environment) {
    if crate::codwawzpc::runtime::CONFIG.get().is_some() {
        update_local(env);
    }
    let now = Instant::now();
    let mut notifications = Vec::new();
    let pending = &mut State::get(env).pending_notifications;
    let mut i = 0;
    while i < pending.len() {
        if now >= pending[i].time {
            notifications.push(pending.remove(i).unwrap());
        } else {
            i += 1;
        }
    }
    for Notification {
        name: name_str,
        object,
        reason,
        ..
    } in notifications
    {
        let name = ns_string::get_static_str(env, name_str);
        let center: id = msg_class![env; NSNotificationCenter defaultCenter];
        let user_info = if let Some(reason) = reason {
            let key =
                ns_string::get_static_str(env, MPMoviePlayerPlaybackDidFinishReasonUserInfoKey);
            let value: id = msg_class![env; NSNumber numberWithInt:(reason as i32)];
            msg_class![env; NSDictionary dictionaryWithObject:value forKey:key]
        } else {
            nil
        };
        let _: () = msg![env; center postNotificationName:name object:object userInfo:user_info];
        release(env, object);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_pause_resume_clock_excludes_paused_time() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.pause(start + Duration::from_secs(2));
        clock.pause(start + Duration::from_secs(3));
        assert_eq!(
            clock.elapsed(start + Duration::from_secs(8)),
            Duration::from_secs(2)
        );
        clock.resume(start + Duration::from_secs(8));
        clock.resume(start + Duration::from_secs(9));
        assert_eq!(
            clock.elapsed(start + Duration::from_secs(10)),
            Duration::from_secs(4)
        );
    }

    #[test]
    fn movie_finish_reasons_match_ios_constants() {
        assert_eq!(FinishReason::Ended as i32, 0);
        assert_eq!(FinishReason::Failed as i32, 1);
        assert_eq!(FinishReason::UserExited as i32, 2);
    }
}
