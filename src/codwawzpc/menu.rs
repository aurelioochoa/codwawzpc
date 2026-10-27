/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Keyboard and controller navigation of the game's touch menus.
//!
//! The menus only respond to touches. Each of their buttons, sliders and
//! scrolling lists registers a touch region with the game's touch manager
//! (`CIwTouchManager`), which hit-tests touches against the regions that
//! are enabled. Every frame outside play, [frame] reads the enabled regions
//! and keeps one of them focused. Direction input moves the focus to the
//! nearest region that way; Activate taps the focused button. Left and
//! right drag a focused slider's thumb, and moving past the last visible
//! row of a list drags the list by one row.
//!
//! The focused item is shown the game's own way where it has one: menu
//! items (`CGuiTouchButtonMenuItem` and its relatives) show a blood smear
//! behind a highlighted label when their "selected" flag is set, which the
//! game itself uses for the selected row of a list. Focused items get the
//! flag. The lists of achievements and levels rewrite it every frame from
//! their own selected row, so there the focused row becomes the list's
//! selected row: in the achievements list by tapping it, which also shows
//! its details, and in other lists (levels, where a tap starts the level)
//! by setting the list's selection directly. Controls without that art (sliders, check
//! boxes, option toggles), and items whose flag the game keeps clearing, get
//! a blood-red outline drawn by the window. All of this only while the
//! keyboard or controller is in use.
//!
//! Addresses and offsets belong to the one executable that `import`
//! accepts (see [super::profile]); regions are recognised by vtable.

use super::native::{peek, poke};
use crate::codwawz_controls::{Coords, Dir, MenuCommand, Phase};
use crate::mem::Ptr;
use crate::Environment;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

mod addr {
    /// `CIwTouchManager *`; its regions are an array at +4 with the count
    /// at +8.
    pub const TOUCH_MANAGER: u32 = 0xa1a56c;
}

mod vtable {
    /// `CDoubleTouchComponent<CIwTouchButton>`: menu buttons and list rows.
    pub const BUTTON: u32 = 0x2af4c8;
    /// `CIwTouchSlider`.
    pub const SLIDER: u32 = 0x2a7af0;
    /// `CIwTouchTracker`: the drag area of a scrolling list.
    pub const TRACKER: u32 = 0x2a8bec;
}

/// `CGuiTouchButtonAchievementMenuItem`: a row of an achievement list.
const ACHIEVEMENT_ITEM: u32 = 0x2b4440;
/// GUI elements that draw a blood smear when highlighted (their update is
/// `CGuiTouchButtonMenuItem`'s, at 0x1cffbc, or calls it).
const BLOOD_CLASSES: [u32; 5] = [
    0x2b0d8c, // CGuiTouchButtonMenuItem
    0x2b4440, // CGuiTouchButtonAchievementMenuItem
    0x2a869c, // CGuiTouchToggle
    0x2ae53c, // CGuiTouchMultiToggle
    0x2a989c, // CGuiTouchEditBox
];

/// `CGuiFrameAchievementList`.
const ACHIEVEMENT_LIST: u32 = 0x2b1658;
/// In a `CGuiFrameAchievementList`: its selected row's GUI element.
const LIST_SELECTED: u32 = 0x130;

/// Offsets in a menu item GUI element.
mod item {
    /// Hash of the element's name (see [super::name_hash]).
    pub const NAME: u32 = 4;
    /// The parent GUI element.
    pub const PARENT: u32 = 0x3c;
    /// The item's touch region is embedded here.
    pub const REGION: u32 = 0xa0;
    /// The "highlight" child element (the blood smear), if any.
    pub const HIGHLIGHT: u32 = 0x190;
    /// Byte: selected, so highlighted. Lists rewrite it every frame to mark
    /// their selected row.
    pub const SELECTED: u32 = 0x1a4;
}

/// Offsets in a touch region (`CIwTouchButton` and subclasses).
mod region {
    /// `short x, y, w, h` in points of the landscape screen.
    pub const RECT: u32 = 0x40;
    /// Byte: the region takes touches (its screen is showing).
    pub const ENABLED: u32 = 0x55;
    /// Slider: `ushort value` (4096 = full) then `ushort thumb width`.
    pub const SLIDER_VALUE: u32 = 0x60;
}

/// The landscape screen.
fn screen() -> Rect {
    let (w, h) = crate::window::logical_size();
    Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    }
}
/// Upper limit on regions read, against a corrupt count.
const MAX_REGIONS: u32 = 1024;
/// Slider change per left or right press, as a fraction of its range.
const SLIDER_STEP: f32 = 0.1;
/// Commands kept while a gesture is under way.
const MAX_PENDING: usize = 4;
/// Frames without menu touches before regions are trusted again (a drag
/// relays out its list on the next frame).
const SETTLE_FRAMES: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn right(&self) -> f32 {
        self.x + self.w
    }
    fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn center(&self) -> Coords {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Rect {
            x,
            y,
            w: (self.right().min(other.right()) - x).max(0.0),
            h: (self.bottom().min(other.bottom()) - y).max(0.0),
        }
    }
    fn covers_screen(&self) -> bool {
        let screen = screen();
        self.x <= 0.0 && self.y <= 0.0 && self.right() >= screen.w && self.bottom() >= screen.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    /// How it shows focus, and whether its item is selected.
    Button {
        highlight: Highlight,
        selected: bool,
    },
    /// Value 0–1 and thumb width.
    Slider(f32, f32),
    Tracker,
}

/// How a focused button is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Highlight {
    /// No art of its own: the window outlines it.
    Outline,
    /// The blood smear, through its item's selected flag.
    Flag,
    /// A row of a list (`CGuiFrameAchievementList`, also used for the
    /// levels), which flags its own selected row every frame: select it in
    /// the list.
    Select,
    /// A row of the achievements list, where a touch selects the row and
    /// shows the achievement's details: tap it.
    Tap,
}

/// An enabled touch region as the guest has it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Raw {
    addr: u32,
    class: Class,
    rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Button {
        highlight: Highlight,
        selected: bool,
    },
    Slider {
        value: f32,
        thumb: f32,
    },
    /// A scrolling area without rows (scrolling text).
    ScrollArea,
}

/// A region the focus can be on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Region {
    addr: u32,
    kind: Kind,
    /// The part that can be seen and touched.
    rect: Rect,
    /// Height as registered (a list's row height).
    height: f32,
    /// The list (index into [Screen::lists]) this row belongs to.
    list: Option<usize>,
    /// Scrolled out of its list.
    hidden: bool,
}

/// The enabled regions of the current screen, in registration order (list
/// rows are registered top to bottom).
#[derive(Debug, Default)]
struct Screen {
    regions: Vec<Region>,
    lists: Vec<Rect>,
}

/// What a command does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    None,
    Focus(u32),
    /// Focus a row and drag its list to bring it into view.
    FocusAndScroll(u32, usize, Dir),
    /// Drag a list by a row past its last (or first) row, whose neighbour
    /// only gets a touch region once scrolled in, then step again.
    ScrollThenMove(usize, Dir),
    Scroll(Rect, Dir, f32),
    /// Drag a slider's thumb by a fraction of its range.
    Adjust(u32, f32),
    Tap(Coords),
}

impl Screen {
    fn new(raw: &[Raw]) -> Self {
        let lists: Vec<Rect> = raw
            .iter()
            .filter(|r| r.class == Class::Tracker)
            .map(|r| r.rect)
            .collect();
        let mut used = vec![false; lists.len()];
        let mut regions = Vec::new();
        for r in raw {
            let kind = match r.class {
                Class::Button {
                    highlight,
                    selected,
                } => Kind::Button {
                    highlight,
                    selected,
                },
                Class::Slider(value, thumb) => Kind::Slider { value, thumb },
                Class::Tracker => continue,
            };
            // A row spans its list, and rows scrolled out are parked just
            // outside it.
            let list = lists.iter().position(|l| {
                r.rect.x == l.x
                    && r.rect.w == l.w
                    && r.rect.h < l.h
                    && r.rect.bottom() > l.y - r.rect.h
                    && r.rect.y < l.bottom() + r.rect.h
            });
            let (rect, hidden) = match list {
                Some(i) => {
                    used[i] = true;
                    let (_, cy) = r.rect.center();
                    let hidden = cy <= lists[i].y || cy > lists[i].bottom();
                    (r.rect.intersect(&lists[i]), hidden)
                }
                None => {
                    let rect = r.rect.intersect(&screen());
                    (rect, rect.is_empty())
                }
            };
            regions.push(Region {
                addr: r.addr,
                kind,
                rect,
                height: r.rect.h,
                list,
                hidden,
            });
        }
        for (i, r) in raw.iter().filter(|r| r.class == Class::Tracker).enumerate() {
            if !used[i] {
                regions.push(Region {
                    addr: r.addr,
                    kind: Kind::ScrollArea,
                    rect: r.rect.intersect(&screen()),
                    height: r.rect.h,
                    list: None,
                    hidden: false,
                });
            }
        }
        // A screen-sized region (such as "touch to start") only matters when
        // there is nothing else.
        let has_others = regions.iter().any(|r| !r.rect.covers_screen() && !r.hidden);
        if has_others {
            regions.retain(|r| !r.rect.covers_screen());
        }
        Screen { regions, lists }
    }

    fn get(&self, addr: u32) -> Option<&Region> {
        self.regions.iter().find(|r| r.addr == addr)
    }

    fn focusable(&self) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(|r| !r.hidden)
    }

    /// Identifies the screen independently of list scrolling.
    fn signature(&self) -> Vec<u32> {
        self.regions
            .iter()
            .filter(|r| r.list.is_none())
            .map(|r| r.addr)
            .collect()
    }

    /// A list's selected row, else the first region in reading order.
    fn first(&self) -> Option<u32> {
        let selected = self
            .focusable()
            .find(|r| r.list.is_some() && matches!(r.kind, Kind::Button { selected: true, .. }));
        if let Some(r) = selected {
            return Some(r.addr);
        }
        self.focusable()
            .min_by(|a, b| {
                (a.rect.y, a.rect.x)
                    .partial_cmp(&(b.rect.y, b.rect.x))
                    .unwrap()
            })
            .map(|r| r.addr)
    }

    fn step(&self, focus: &Region, command: MenuCommand) -> Step {
        let dir = match command {
            MenuCommand::Activate => {
                return match focus.kind {
                    Kind::Button { .. } if !focus.hidden => Step::Tap(focus.rect.center()),
                    _ => Step::None,
                };
            }
            MenuCommand::Move(dir) => dir,
        };
        let vertical = matches!(dir, Dir::Up | Dir::Down);
        match focus.kind {
            Kind::Slider { .. } if !vertical => {
                let sign = if dir == Dir::Left { -1.0 } else { 1.0 };
                return Step::Adjust(focus.addr, sign * SLIDER_STEP);
            }
            Kind::ScrollArea if vertical => {
                return Step::Scroll(focus.rect, dir, 40.0);
            }
            _ => (),
        }
        if let (Some(list), true) = (focus.list, vertical) {
            let rows: Vec<&Region> = self
                .regions
                .iter()
                .filter(|r| r.list == Some(list))
                .collect();
            let at = rows.iter().position(|r| r.addr == focus.addr).unwrap();
            let next = if dir == Dir::Up {
                at.checked_sub(1)
            } else {
                Some(at + 1)
            };
            return match next.and_then(|i| rows.get(i)) {
                Some(next) if next.hidden => Step::FocusAndScroll(next.addr, list, dir),
                Some(next) => Step::Focus(next.addr),
                None => Step::ScrollThenMove(list, dir),
            };
        }
        self.nearest(focus, dir).map_or(Step::None, Step::Focus)
    }

    /// After [Step::ScrollThenMove]: the row that scrolled in next to `from`,
    /// or else (the list ended) the nearest region outside the list.
    fn step_after_scroll(&self, from: u32, dir: Dir) -> Option<u32> {
        let focus = self.get(from)?;
        let list = focus.list?;
        let rows: Vec<&Region> = self
            .regions
            .iter()
            .filter(|r| r.list == Some(list))
            .collect();
        let at = rows.iter().position(|r| r.addr == from)?;
        let next = if dir == Dir::Up {
            at.checked_sub(1)
        } else {
            Some(at + 1)
        };
        match next.and_then(|i| rows.get(i)) {
            Some(next) if !next.hidden => Some(next.addr),
            _ => self.nearest(focus, dir),
        }
    }

    /// The region closest to `focus` in a direction, preferring ones in line
    /// with it.
    fn nearest(&self, focus: &Region, dir: Dir) -> Option<u32> {
        let (fx, fy) = focus.rect.center();
        let gap = |a0: f32, a1: f32, b0: f32, b1: f32| (b0 - a1).max(a0 - b1).max(0.0);
        self.focusable()
            .filter(|r| r.addr != focus.addr && (focus.list.is_none() || r.list != focus.list))
            .filter_map(|r| {
                let (cx, cy) = r.rect.center();
                let (along, across) = match dir {
                    Dir::Up => (
                        fy - cy,
                        gap(focus.rect.x, focus.rect.right(), r.rect.x, r.rect.right()),
                    ),
                    Dir::Down => (
                        cy - fy,
                        gap(focus.rect.x, focus.rect.right(), r.rect.x, r.rect.right()),
                    ),
                    Dir::Left => (
                        fx - cx,
                        gap(focus.rect.y, focus.rect.bottom(), r.rect.y, r.rect.bottom()),
                    ),
                    Dir::Right => (
                        cx - fx,
                        gap(focus.rect.y, focus.rect.bottom(), r.rect.y, r.rect.bottom()),
                    ),
                };
                (along > 1.0).then_some((along + 2.0 * across, r.addr))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, addr)| addr)
    }
}

/// A touch gesture as (time after start, phase, point) steps.
type Gesture = Vec<(Duration, Phase, Coords)>;

fn drag(from: Coords, to: Coords, moves: u32, interval_ms: u64, hold_ms: u64) -> Gesture {
    let ms = Duration::from_millis;
    let mut steps = vec![(Duration::ZERO, Phase::Down, from)];
    for i in 1..=moves {
        let t = i as f32 / moves as f32;
        let point = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        steps.push((ms(interval_ms * u64::from(i)), Phase::Move, point));
    }
    steps.push((ms(interval_ms * u64::from(moves) + hold_ms), Phase::Up, to));
    steps
}

/// Drag a list's contents by `amount` points so the rows move against
/// `dir`, slowly enough that the list does not keep scrolling after.
fn scroll_gesture(list: Rect, dir: Dir, amount: f32) -> Gesture {
    let x = list.x + (list.w / 4.0).min(60.0);
    let (from, to) = if dir == Dir::Down {
        let y = list.y + list.h * 0.7;
        (y, y - amount)
    } else {
        let y = list.y + list.h * 0.3;
        (y, y + amount)
    };
    drag((x, from), (x, to), 6, 25, 150)
}

fn slider_gesture(rect: Rect, value: f32, thumb: f32, delta: f32) -> Gesture {
    let range = (rect.w - thumb).max(1.0);
    let low = rect.x + thumb / 2.0;
    let from = low + range * value.clamp(0.0, 1.0);
    let to = (from + range * delta).clamp(low, low + range);
    let y = rect.center().1;
    drag((from, y), (to, y), 2, 30, 30)
}

/// Navigation state kept between frames.
#[derive(Default)]
pub(crate) struct State {
    focus: Option<u32>,
    /// Focus last used on each screen, so Back returns to the button that
    /// opened a submenu.
    remembered: HashMap<Vec<u32>, u32>,
    pending: VecDeque<MenuCommand>,
    idle_frames: u32,
    /// A list was dragged past the focused row, in a direction; step on
    /// once it settles.
    after_scroll: Option<(u32, Dir)>,
    /// The achievement row last tapped to select it.
    tapped: Option<u32>,
    /// The selected flag set for the focus.
    flagged: Option<Flagged>,
    /// Regions whose selected flag the game keeps clearing.
    flag_lost: HashSet<u32>,
    /// The current screen's regions, and for how many frames they have
    /// stayed the same (for input scripts' `wait menu`).
    settled: (Vec<u32>, u32),
}

/// A menu item whose selected flag is set for the focus.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flagged {
    region: u32,
    element: u32,
    class: u32,
    /// The flag before it was set.
    was: u8,
}

impl State {
    /// Apply commands to a screen; returns gestures to perform.
    fn update(&mut self, screen: &Screen, commands: Vec<MenuCommand>, idle: bool) -> Vec<Gesture> {
        if screen.regions.is_empty() {
            // Loading or between screens: input now would act on whatever
            // shows up next.
            self.pending.clear();
            self.focus = None;
            return Vec::new();
        }
        self.pending.extend(commands);
        while self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
        let signature = screen.signature();
        if self.focus.and_then(|f| screen.get(f)).is_none() {
            self.focus = self
                .remembered
                .get(&signature)
                .copied()
                .filter(|&f| screen.get(f).is_some_and(|r| !r.hidden))
                .or_else(|| screen.first());
        }
        let mut gestures = Vec::new();
        if idle {
            if let Some((from, dir)) = self.after_scroll.take() {
                if let Some(next) = screen.step_after_scroll(from, dir) {
                    self.focus = Some(next);
                }
            }
            if let Some(point) = self.tap_to_select(screen) {
                gestures.push(vec![
                    (Duration::ZERO, Phase::Down, point),
                    (Duration::from_millis(60), Phase::Up, point),
                ]);
                return gestures;
            }
        }
        if idle {
            // One command at a time: each may change the screen.
            if let (Some(command), Some(focus)) = (
                self.pending.pop_front(),
                self.focus.and_then(|f| screen.get(f)),
            ) {
                match screen.step(focus, command) {
                    Step::None => (),
                    Step::Focus(addr) => {
                        self.focus = Some(addr);
                    }
                    Step::FocusAndScroll(addr, list, dir) => {
                        let row = screen.get(addr).unwrap().height;
                        self.focus = Some(addr);
                        gestures.push(scroll_gesture(screen.lists[list], dir, row));
                    }
                    Step::ScrollThenMove(list, dir) => {
                        let row = focus.height;
                        self.after_scroll = Some((focus.addr, dir));
                        gestures.push(scroll_gesture(screen.lists[list], dir, row));
                    }
                    Step::Scroll(rect, dir, amount) => {
                        gestures.push(scroll_gesture(rect, dir, amount))
                    }
                    Step::Adjust(addr, delta) => {
                        if let Some(Region {
                            kind: Kind::Slider { value, thumb },
                            rect,
                            ..
                        }) = screen.get(addr)
                        {
                            gestures.push(slider_gesture(*rect, *value, *thumb, delta));
                        }
                    }
                    Step::Tap(point) => gestures.push(vec![
                        (Duration::ZERO, Phase::Down, point),
                        (Duration::from_millis(60), Phase::Up, point),
                    ]),
                }
            }
        }
        if let Some(focus) = self.focus {
            self.remembered.insert(signature, focus);
        }
        gestures
    }

    /// The focused region's outline, if it can be seen and the game does
    /// not show it.
    fn outline(&self, screen: &Screen) -> Option<Rect> {
        let region = screen.get(self.focus?)?;
        let own = match region.kind {
            Kind::Button {
                highlight: Highlight::Flag,
                ..
            } => !self.flag_lost.contains(&region.addr),
            Kind::Button {
                highlight: Highlight::Select | Highlight::Tap,
                ..
            } => true,
            _ => false,
        };
        (!region.hidden && !region.rect.is_empty() && !own).then_some(region.rect)
    }

    /// Where to tap the focused achievement row to select it, once.
    fn tap_to_select(&mut self, screen: &Screen) -> Option<Coords> {
        let region = screen.get(self.focus?)?;
        let tap = matches!(
            region.kind,
            Kind::Button {
                highlight: Highlight::Tap,
                selected: false,
            }
        );
        if !tap || region.hidden || self.tapped == Some(region.addr) {
            return None;
        }
        self.tapped = Some(region.addr);
        Some(region.rect.center())
    }

    /// The focused list row to make its list's selected row.
    fn select(&self, screen: &Screen) -> Option<u32> {
        let region = screen.get(self.focus?)?;
        let select = matches!(
            region.kind,
            Kind::Button {
                highlight: Highlight::Select,
                selected: false,
            }
        );
        (select && !region.hidden).then_some(region.addr)
    }

    /// The focused menu item whose selected flag shows its blood smear.
    fn flag(&self, screen: &Screen) -> Option<u32> {
        let region = screen.get(self.focus?)?;
        let flag = matches!(
            region.kind,
            Kind::Button {
                highlight: Highlight::Flag,
                ..
            }
        );
        (flag && !region.hidden && !self.flag_lost.contains(&region.addr)).then_some(region.addr)
    }
}

/// Navigate the menus for one frame. Called when the game presents a frame
/// and is not playing a level.
pub(crate) fn frame(env: &mut Environment, state: &mut State) {
    let now = Instant::now();
    let raw = read_regions(env);
    let window = env.window.as_mut().unwrap();
    let commands = window.take_menu_commands(now);
    let screen = Screen::new(&raw);
    let regions: Vec<u32> = screen.regions.iter().map(|r| r.addr).collect();
    if !regions.is_empty() && regions == state.settled.0 {
        state.settled.1 = state.settled.1.saturating_add(1);
    } else {
        state.settled = (regions, 0);
    }
    window.set_menu_settled_frames(state.settled.1);
    if window.menu_touching() {
        state.idle_frames = 0;
    } else {
        state.idle_frames = state.idle_frames.saturating_add(1);
    }
    let gestures = state.update(&screen, commands, state.idle_frames >= SETTLE_FRAMES);
    if !gestures.is_empty() {
        state.idle_frames = 0;
    }
    for gesture in gestures {
        window.menu_gesture(&gesture, now);
    }
    let outline = state.outline(&screen);
    window.set_menu_focus(outline.map(|r| (r.x, r.y, r.w, r.h)));
    let shown = window.menu_focus_outline_wanted();
    sync_flag(env, state, state.flag(&screen).filter(|_| shown));
    if let Some(row) = state.select(&screen).filter(|_| shown) {
        select_in_list(env, row);
    }
}

/// Set the selected flag of the focused menu item, and clear it on the one
/// focused before. When the focus is hidden, the item gets back the flag
/// it had (a list's selected row stays selected).
fn sync_flag(env: &mut Environment, state: &mut State, region: Option<u32>) {
    if let Some(flagged) = state.flagged {
        if region == Some(flagged.region) {
            if peek_u8(env, flagged.element + item::SELECTED) != Some(0) {
                return;
            }
            // Something clears it every frame (a list showing its own
            // selection): outline this one instead.
            state.flag_lost.insert(flagged.region);
        } else if peek(env, flagged.element) == Some(flagged.class) {
            // Screens are built once, so the item normally still exists;
            // its class is checked all the same.
            let restore = if region.is_none() { flagged.was } else { 0 };
            poke_u8(env, flagged.element + item::SELECTED, restore);
        }
        state.flagged = None;
    }
    let Some(region) = region.filter(|r| !state.flag_lost.contains(r)) else {
        return;
    };
    let element = region - item::REGION;
    let (Some(class), Some(was)) = (peek(env, element), peek_u8(env, element + item::SELECTED))
    else {
        return;
    };
    poke_u8(env, element + item::SELECTED, 1);
    state.flagged = Some(Flagged {
        region,
        element,
        class,
        was,
    });
}

/// The hash IwGui finds elements by: djb2 of the lowercased name.
fn name_hash(name: &str) -> u32 {
    name.bytes().fold(5381u32, |hash, byte| {
        hash.wrapping_mul(33)
            .wrapping_add(u32::from(byte.to_ascii_lowercase()))
    })
}

/// Make a row its list's selected row, which the list then highlights.
fn select_in_list(env: &mut Environment, region: u32) {
    let element = region - item::REGION;
    let Some(list) = peek(env, element + item::PARENT) else {
        return;
    };
    if peek(env, list) == Some(ACHIEVEMENT_LIST) {
        poke(env, list + LIST_SELECTED, element);
    }
}

/// The enabled regions of the touch manager.
fn read_regions(env: &Environment) -> Vec<Raw> {
    let Some(manager) = peek(env, addr::TOUCH_MANAGER).filter(|&m| m != 0) else {
        return Vec::new();
    };
    let (Some(list), Some(count)) = (peek(env, manager + 4), peek(env, manager + 8)) else {
        return Vec::new();
    };
    let mut regions = Vec::new();
    for i in 0..count.min(MAX_REGIONS) {
        let Some(object) = peek(env, list + i * 4).filter(|&o| o != 0) else {
            break;
        };
        if peek_u8(env, object + region::ENABLED).unwrap_or(0) == 0 {
            continue;
        }
        let Some(rect) = read_rect(env, object + region::RECT) else {
            continue;
        };
        let class = match peek(env, object) {
            Some(vtable::BUTTON) => {
                let element = object - item::REGION;
                let class = peek(env, element).unwrap_or(0);
                let blood = BLOOD_CLASSES.contains(&class)
                    && peek(env, element + item::HIGHLIGHT).is_some_and(|h| h != 0);
                let highlight = match (blood, class) {
                    (false, _) => Highlight::Outline,
                    (true, ACHIEVEMENT_ITEM) => {
                        let list = peek(env, element + item::PARENT).unwrap_or(0);
                        if peek(env, list + item::NAME) == Some(name_hash("va_buttonContainer")) {
                            Highlight::Tap
                        } else {
                            Highlight::Select
                        }
                    }
                    (true, _) => Highlight::Flag,
                };
                let selected = blood && peek_u8(env, element + item::SELECTED) == Some(1);
                Class::Button {
                    highlight,
                    selected,
                }
            }
            Some(vtable::TRACKER) => Class::Tracker,
            Some(vtable::SLIDER) => {
                let Some(word) = peek(env, object + region::SLIDER_VALUE) else {
                    continue;
                };
                Class::Slider((word & 0xffff) as f32 / 4096.0, (word >> 16) as f32)
            }
            _ => continue,
        };
        if rect.is_empty() {
            continue;
        }
        regions.push(Raw {
            addr: object,
            class,
            rect,
        });
    }
    regions
}

fn read_rect(env: &Environment, at: u32) -> Option<Rect> {
    let bytes = env.mem.get_bytes_fallible(Ptr::from_bits(at), 8)?;
    let short = |i: usize| i16::from_le_bytes([bytes[i], bytes[i + 1]]) as f32;
    Some(Rect {
        x: short(0),
        y: short(2),
        w: short(4),
        h: short(6),
    })
}

fn poke_u8(env: &mut Environment, at: u32, value: u8) {
    if let Some(bytes) = env.mem.get_bytes_fallible_mut(Ptr::from_bits(at), 1) {
        bytes[0] = value;
    }
}

fn peek_u8(env: &Environment, at: u32) -> Option<u8> {
    env.mem
        .get_bytes_fallible(Ptr::from_bits(at), 1)
        .map(|bytes| bytes[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    fn button(addr: u32, x: f32, y: f32, w: f32, h: f32) -> Raw {
        Raw {
            addr,
            class: Class::Button {
                highlight: Highlight::Outline,
                selected: false,
            },
            rect: rect(x, y, w, h),
        }
    }

    /// The main menu: four full-width rows and Back.
    fn main_menu() -> Vec<Raw> {
        vec![
            button(1, 0.0, 272.0, 120.0, 48.0),
            button(2, 0.0, 60.0, 480.0, 40.0),
            button(3, 0.0, 100.0, 480.0, 40.0),
            button(4, 0.0, 140.0, 480.0, 40.0),
            button(5, 0.0, 180.0, 480.0, 40.0),
        ]
    }

    fn run(state: &mut State, raw: &[Raw], commands: &[MenuCommand]) -> Vec<Gesture> {
        let screen = Screen::new(raw);
        let mut gestures = Vec::new();
        state.update(&screen, Vec::new(), false);
        for &command in commands {
            gestures.extend(state.update(&screen, vec![command], true));
        }
        gestures
    }

    use MenuCommand::{Activate, Move};

    #[test]
    fn focus_starts_at_the_top_and_moves_spatially() {
        let mut s = State::default();
        run(&mut s, &main_menu(), &[]);
        assert_eq!(s.focus, Some(2));
        run(&mut s, &main_menu(), &[Move(Dir::Down), Move(Dir::Down)]);
        assert_eq!(s.focus, Some(4));
        run(&mut s, &main_menu(), &[Move(Dir::Down), Move(Dir::Down)]);
        assert_eq!(s.focus, Some(1), "Back is below the last row");
        run(&mut s, &main_menu(), &[Move(Dir::Down), Move(Dir::Left)]);
        assert_eq!(s.focus, Some(1), "nothing further");
        run(&mut s, &main_menu(), &[Move(Dir::Up)]);
        assert_eq!(s.focus, Some(5));
    }

    #[test]
    fn activate_taps_the_focused_button() {
        let mut s = State::default();
        let gestures = run(&mut s, &main_menu(), &[Move(Dir::Down), Activate]);
        assert_eq!(gestures.len(), 1);
        assert_eq!(
            gestures[0][0],
            (Duration::ZERO, Phase::Down, (240.0, 120.0))
        );
        assert_eq!(gestures[0][1].1, Phase::Up);
    }

    #[test]
    fn screen_sized_regions_only_count_alone() {
        let mut s = State::default();
        let title = [button(9, 0.0, 0.0, 480.0, 320.0)];
        let gestures = run(&mut s, &title, &[Activate]);
        assert_eq!(gestures[0][0].2, (240.0, 160.0));
        let mut menu = main_menu();
        menu.push(button(9, 0.0, 0.0, 480.0, 320.0));
        run(&mut s, &menu, &[]);
        assert_eq!(s.focus, Some(2));
    }

    #[test]
    fn focus_is_remembered_per_screen() {
        let mut s = State::default();
        run(&mut s, &main_menu(), &[Move(Dir::Down), Move(Dir::Down)]);
        let options = [
            button(20, 0.0, 60.0, 480.0, 40.0),
            button(21, 0.0, 272.0, 120.0, 48.0),
        ];
        run(&mut s, &options, &[]);
        assert_eq!(s.focus, Some(20));
        run(&mut s, &main_menu(), &[]);
        assert_eq!(s.focus, Some(4));
    }

    #[test]
    fn commands_wait_for_idle_and_empty_screens_drop_them() {
        let mut s = State::default();
        let screen = Screen::new(&main_menu());
        assert!(s
            .update(&screen, vec![Move(Dir::Down), Activate], false)
            .is_empty());
        assert_eq!(s.focus, Some(2));
        assert!(s.update(&screen, Vec::new(), true).is_empty());
        assert_eq!(s.focus, Some(3));
        assert_eq!(s.update(&screen, Vec::new(), true).len(), 1);
        s.update(&screen, vec![Activate], false);
        assert!(s.update(&Screen::new(&[]), Vec::new(), true).is_empty());
        assert!(s.pending.is_empty());
    }

    /// The Settings screen: two sliders, a button extending off screen, a
    /// checkbox and Back.
    fn settings() -> Vec<Raw> {
        vec![
            button(1, 0.0, 272.0, 120.0, 48.0),
            Raw {
                addr: 2,
                class: Class::Slider(0.5, 40.0),
                rect: rect(250.0, 60.0, 200.0, 40.0),
            },
            Raw {
                addr: 3,
                class: Class::Slider(0.0, 40.0),
                rect: rect(250.0, 100.0, 200.0, 40.0),
            },
            button(4, 220.0, 140.0, 480.0, 40.0),
            button(5, 245.0, 172.0, 40.0, 40.0),
        ]
    }

    #[test]
    fn sliders_drag_their_thumb_on_left_and_right() {
        let mut s = State::default();
        let gestures = run(&mut s, &settings(), &[Move(Dir::Right)]);
        assert_eq!(s.focus, Some(2));
        let g = &gestures[0];
        assert_eq!(g.first().unwrap().1, Phase::Down);
        assert_eq!(g.first().unwrap().2, (350.0, 80.0));
        assert_eq!(g.last().unwrap().1, Phase::Up);
        assert_eq!(g.last().unwrap().2, (366.0, 80.0));
        // At the minimum, left stays put.
        let gestures = run(&mut s, &settings(), &[Move(Dir::Down), Move(Dir::Left)]);
        assert_eq!(s.focus, Some(3));
        assert_eq!(gestures[0].last().unwrap().2, (270.0, 120.0));
    }

    #[test]
    fn off_screen_parts_are_clipped() {
        let mut s = State::default();
        let gestures = run(
            &mut s,
            &settings(),
            &[Move(Dir::Down), Move(Dir::Down), Activate],
        );
        assert_eq!(s.focus, Some(4));
        assert_eq!(gestures[0][0].2, (350.0, 160.0));
    }

    /// The achievements list scrolled by two rows: rows 10 and 11 are
    /// parked above the list, 15 is half out at the bottom.
    fn scrolled_list() -> Vec<Raw> {
        let mut raw = vec![
            button(1, 0.0, 272.0, 120.0, 48.0),
            Raw {
                addr: 2,
                class: Class::Tracker,
                rect: rect(0.0, 68.0, 480.0, 192.0),
            },
        ];
        for (addr, y) in [
            (10, 48.0),
            (11, 48.0),
            (12, 68.0),
            (13, 108.0),
            (14, 148.0),
            (15, 188.0),
            (16, 228.0),
        ] {
            raw.push(button(addr, 0.0, y, 480.0, 40.0));
        }
        raw.push(button(17, 0.0, 268.0, 480.0, 40.0));
        raw
    }

    #[test]
    fn lists_scroll_to_rows_out_of_view() {
        let mut s = State::default();
        run(&mut s, &scrolled_list(), &[]);
        assert_eq!(s.focus, Some(12), "first visible row");
        // Up from the top visible row: the previous row, by list order.
        let gestures = run(&mut s, &scrolled_list(), &[Move(Dir::Up)]);
        assert_eq!(s.focus, Some(11));
        let g = &gestures[0];
        assert!(
            g.last().unwrap().2 .1 > g.first().unwrap().2 .1,
            "drags down"
        );
        assert!((g.last().unwrap().2 .1 - g.first().unwrap().2 .1 - 40.0).abs() < 0.01);

        let mut s = State::default();
        let gestures = run(
            &mut s,
            &scrolled_list(),
            &[
                Move(Dir::Down),
                Move(Dir::Down),
                Move(Dir::Down),
                Move(Dir::Down),
            ],
        );
        assert_eq!(s.focus, Some(16), "row 16's center is inside the list");
        assert!(gestures.is_empty());
        let gestures = run(&mut s, &scrolled_list(), &[Move(Dir::Down)]);
        assert_eq!(s.focus, Some(17));
        assert!(
            gestures[0].last().unwrap().2 .1 < gestures[0][0].2 .1,
            "drags up"
        );
    }

    #[test]
    fn leaving_a_list_goes_to_the_nearest_button() {
        let mut raw = scrolled_list();
        raw.retain(|r| r.addr != 17);
        let mut s = State::default();
        let gestures = run(&mut s, &raw, &[Move(Dir::Down); 5]);
        // Past the last row the list is dragged in case more rows follow;
        // none did, so the focus leaves the list.
        assert_eq!(gestures.len(), 1);
        assert_eq!(s.focus, Some(16));
        run(&mut s, &raw, &[]);
        let screen = Screen::new(&raw);
        s.update(&screen, Vec::new(), true);
        assert_eq!(s.focus, Some(1));
        run(&mut s, &raw, &[Move(Dir::Up)]);
        assert!(matches!(s.focus, Some(15 | 16)));
    }

    #[test]
    fn hidden_rows_are_not_outlined_or_tapped() {
        let mut s = State::default();
        let screen = Screen::new(&scrolled_list());
        s.update(&screen, Vec::new(), false);
        s.focus = Some(10);
        assert_eq!(s.outline(&screen), None);
        assert_eq!(screen.step(screen.get(10).unwrap(), Activate), Step::None);
        s.focus = Some(13);
        assert_eq!(s.outline(&screen), Some(rect(0.0, 108.0, 480.0, 40.0)));
    }

    #[test]
    fn scroll_areas_without_rows_scroll() {
        let raw = [
            button(1, 0.0, 272.0, 120.0, 48.0),
            Raw {
                addr: 2,
                class: Class::Tracker,
                rect: rect(0.0, 60.0, 480.0, 200.0),
            },
        ];
        let mut s = State::default();
        let gestures = run(&mut s, &raw, &[Move(Dir::Down)]);
        assert_eq!(s.focus, Some(2));
        assert_eq!(gestures.len(), 1);
    }

    fn item(addr: u32, y: f32, highlight: Highlight, selected: bool) -> Raw {
        Raw {
            addr,
            class: Class::Button {
                highlight,
                selected,
            },
            rect: rect(0.0, y, 480.0, 40.0),
        }
    }

    #[test]
    fn menu_items_show_blood_instead_of_an_outline() {
        let raw = [
            button(1, 0.0, 272.0, 120.0, 48.0),
            item(2, 60.0, Highlight::Flag, false),
            item(3, 100.0, Highlight::Flag, false),
        ];
        let screen = Screen::new(&raw);
        let mut s = State::default();
        run(&mut s, &raw, &[]);
        assert_eq!(s.outline(&screen), None);
        assert_eq!(s.flag(&screen), Some(2));
        run(&mut s, &raw, &[Move(Dir::Down), Move(Dir::Down)]);
        assert_eq!(s.focus, Some(1));
        assert_eq!(s.flag(&screen), None);
        assert_eq!(s.outline(&screen), Some(rect(0.0, 272.0, 120.0, 48.0)));
    }

    /// A list whose rows show the blood smear one way, the second
    /// selected.
    fn blood_list(highlight: Highlight) -> Vec<Raw> {
        vec![
            button(1, 0.0, 272.0, 120.0, 48.0),
            Raw {
                addr: 2,
                class: Class::Tracker,
                rect: rect(0.0, 60.0, 480.0, 200.0),
            },
            item(10, 60.0, highlight, false),
            item(11, 100.0, highlight, true),
            item(12, 140.0, highlight, false),
        ]
    }

    #[test]
    fn list_focus_starts_on_the_selected_row_and_selects_rows() {
        let raw = blood_list(Highlight::Select);
        let screen = Screen::new(&raw);
        let mut s = State::default();
        run(&mut s, &raw, &[]);
        assert_eq!(s.focus, Some(11));
        assert_eq!(s.flag(&screen), None, "lists keep their own flag");
        assert_eq!(s.outline(&screen), None);
        assert_eq!(s.select(&screen), None, "already selected");
        // Moving selects the new row in the list, without touching it.
        assert!(s.update(&screen, vec![Move(Dir::Down)], true).is_empty());
        assert_eq!(s.focus, Some(12));
        assert!(s.update(&screen, Vec::new(), true).is_empty());
        assert_eq!(s.select(&screen), Some(12));
    }

    #[test]
    fn rows_scrolled_in_past_the_end_get_the_focus() {
        // Five rows, none parked below: the sixth only exists once scrolled.
        let mut raw = scrolled_list();
        raw.retain(|r| r.addr != 17);
        let mut s = State::default();
        let screen = Screen::new(&raw);
        s.update(&screen, Vec::new(), false);
        s.focus = Some(16);
        let gestures = s.update(&screen, vec![Move(Dir::Down)], true);
        assert_eq!(gestures.len(), 1);
        assert!(
            gestures[0].last().unwrap().2 .1 < gestures[0][0].2 .1,
            "drags up"
        );
        // The list moved up a row and row 17 appeared.
        let mut moved: Vec<Raw> = raw
            .iter()
            .map(|r| match r.addr {
                10..=16 => {
                    let mut r = *r;
                    r.rect.y = (r.rect.y - 40.0).max(48.0);
                    r
                }
                _ => *r,
            })
            .collect();
        moved.push(button(17, 0.0, 228.0, 480.0, 40.0));
        let screen = Screen::new(&moved);
        s.update(&screen, Vec::new(), true);
        assert_eq!(s.focus, Some(17));
    }

    #[test]
    fn other_lists_get_the_flag_and_are_never_tapped() {
        // The level list: a tap on a level starts it.
        let raw = blood_list(Highlight::Flag);
        let screen = Screen::new(&raw);
        let mut s = State::default();
        run(&mut s, &raw, &[]);
        assert_eq!(s.focus, Some(11));
        assert_eq!(s.flag(&screen), Some(11));
        assert!(s.update(&screen, vec![Move(Dir::Down)], true).is_empty());
        assert!(s.update(&screen, Vec::new(), true).is_empty());
        assert_eq!(s.flag(&screen), Some(12));
        assert_eq!(s.outline(&screen), None);
        // If the list keeps clearing the flag, the row is outlined.
        s.flag_lost.insert(12);
        assert_eq!(s.flag(&screen), None);
        assert_eq!(s.outline(&screen), Some(rect(0.0, 140.0, 480.0, 40.0)));
    }

    #[test]
    fn name_hash_matches_the_game() {
        // Computed with the game's own hash (0x96df8, seed 5381).
        assert_eq!(name_hash("va_buttonContainer"), 0x39c6ef7a);
    }

    #[test]
    fn achievement_rows_are_tapped_once_when_focused() {
        let raw = blood_list(Highlight::Tap);
        let screen = Screen::new(&raw);
        let mut s = State::default();
        run(&mut s, &raw, &[]);
        assert_eq!(s.focus, Some(11));
        assert!(s.update(&screen, vec![Move(Dir::Down)], true).is_empty());
        let tap = s.update(&screen, Vec::new(), true);
        assert_eq!(tap[0][0], (Duration::ZERO, Phase::Down, (240.0, 160.0)));
        assert!(s.update(&screen, Vec::new(), true).is_empty(), "only once");
        assert_eq!(s.select(&screen), None);
        assert_eq!(s.outline(&screen), None);
    }
}
