/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Menu backgrounds on a widescreen (see [super::graphics::Widescreen]).
//!
//! The game builds every GUI screen at startup, laying each element out
//! against its parent: full-screen frames take the wider screen, and some
//! elements anchor to its right edge, but bitmaps authored with a fixed
//! width of 480 points stay that wide. On the front end those are the
//! backdrop behind the menus and the red title and highlight bars, which then
//! end short of the right edge, as do the frames of the info panels at the
//! foot of some menus. Each such bitmap or frame, placed at the left of a
//! parent that spans the screen, is widened to the screen: its width and its
//! laid-out size. Parents are widened before their children, so a panel's
//! own bitmaps follow it. Those 480 points wide inside narrower frames are
//! content (pictures) and are left alone.
//!
//! Screens not yet shown have no laid-out size, so for a bitmap whether its
//! parent spans the screen is decided from the widths asked for: the whole
//! of the parent's own parent, or the screen's width in points, all the way
//! up to a screen. Backdrops and bars are therefore widened before their
//! screen is first shown. Frames are only widened once their screen has been
//! laid out: widening frames of screens not yet shown stopped the title
//! screen from taking input (the cause was not found). The tree is checked
//! on every frame, so a panel is widened by the frame after it is shown.
//!
//! `CIwGuiElement` layout, as 16-bit values whose upper halves are unused:
//! x at +0x14 with its unit at +0x18, y at +0x1c/+0x20, the laid-out
//! position (x, y) at +0x24, width at +0x28/+0x2c, height at +0x30/+0x34,
//! the laid-out size (width, height) at +0x38, and the parent at +0x3c. Unit
//! 1 is points; unit 8 is a fraction of the parent, 4096 being all of it.
//! Children are an array at +0x50 with its count at +0x54; a screen's parent
//! is null.

use super::native::{peek, poke};
use crate::Environment;
use std::collections::HashSet;

/// `CIwGuiManager *`; its root element is embedded at +0xb8.
const GUI_MANAGER: u32 = 0xa1a548;
const GUI_ROOT: u32 = 0xb8;
const BITMAP_VTABLE: u32 = 0x2a478c;
const FRAME_VTABLE: u32 = 0x2a3550;
/// The game's GUI classes' vtables lie in this range.
const GUI_VTABLES: std::ops::RangeInclusive<u32> = 0x2a2b44..=0x2b17bc;

const X: u32 = 0x14;
const X_UNIT: u32 = 0x18;
const WIDTH: u32 = 0x28;
const WIDTH_UNIT: u32 = 0x2c;
const SIZE: u32 = 0x38;
const PARENT: u32 = 0x3c;
const CHILDREN: u32 = 0x50;
const CHILD_COUNT: u32 = 0x54;
const UNIT_POINTS: u32 = 1;
const UNIT_FRACTION: u32 = 8;
const WHOLE: u32 = 0x1000;

/// The width the game's bitmaps were authored for.
const AUTHORED_WIDTH: u32 = 480;

/// Widen the menu backgrounds. Called once per presented frame.
pub(crate) fn frame(env: &mut Environment) {
    let width = match env.options.iphone_long_side {
        Some(width) if width > AUTHORED_WIDTH => width,
        _ => return,
    };
    let Some(root) = peek(env, GUI_MANAGER)
        .filter(|&m| m != 0)
        .map(|m| m + GUI_ROOT)
    else {
        return;
    };
    let mut widened = 0;
    let mut seen = HashSet::new();
    let mut pending = vec![(root, 0)];
    while let Some((element, depth)) = pending.pop() {
        if !seen.insert(element) {
            continue;
        }
        let widenable = matches!(peek(env, element), Some(BITMAP_VTABLE | FRAME_VTABLE));
        if widenable && widen(env, element, width) {
            widened += 1;
        }
        let (Some(children), Some(count)) = (
            peek(env, element + CHILDREN),
            peek(env, element + CHILD_COUNT),
        ) else {
            continue;
        };
        if depth >= 16 || children == 0 || count > 512 {
            continue;
        }
        for i in 0..count {
            let Some(child) = peek(env, children + i * 4).filter(|&c| c != 0) else {
                continue;
            };
            if peek(env, child).is_some_and(|vtable| GUI_VTABLES.contains(&vtable)) {
                pending.push((child, depth + 1));
            }
        }
    }
    if widened > 0 {
        log_dbg!("Widened {} menu elements to {} points", widened, width);
    }
}

/// Widen an element 480 points wide at the left of a parent as wide as the
/// screen. Returns whether it was changed.
fn widen(env: &mut Environment, element: u32, width: u32) -> bool {
    let low = |v: u32| v & 0xffff;
    let (Some(x), Some(x_unit), Some(w), Some(w_unit), Some(size), Some(parent)) = (
        peek(env, element + X),
        peek(env, element + X_UNIT),
        peek(env, element + WIDTH),
        peek(env, element + WIDTH_UNIT),
        peek(env, element + SIZE),
        peek(env, element + PARENT),
    ) else {
        return false;
    };
    if low(x) != 0 || x_unit != UNIT_POINTS || low(w) != AUTHORED_WIDTH || w_unit != UNIT_POINTS {
        return false;
    }
    let spans = if peek(env, element) == Some(FRAME_VTABLE) {
        peek(env, parent + SIZE).map(low) == Some(width)
    } else {
        spans_screen(env, parent, width)
    };
    if parent == 0 || !spans {
        return false;
    }
    poke(env, element + WIDTH, (w & !0xffff) | width);
    if low(size) == AUTHORED_WIDTH {
        poke(env, element + SIZE, (size & !0xffff) | width);
    }
    true
}

/// Whether a GUI element is as wide as the screen: laid out so, or asking
/// for the whole of a parent that is (or for the screen's width), up to a
/// screen.
fn spans_screen(env: &Environment, mut element: u32, width: u32) -> bool {
    let low = |v: u32| v & 0xffff;
    for _ in 0..16 {
        if element == 0 {
            return true;
        }
        if !peek(env, element).is_some_and(|vtable| GUI_VTABLES.contains(&vtable)) {
            return false;
        }
        if peek(env, element + SIZE).map(low) == Some(width) {
            return true;
        }
        let asks = (
            peek(env, element + WIDTH).map(low),
            peek(env, element + WIDTH_UNIT),
        );
        let whole =
            asks == (Some(WHOLE), Some(UNIT_FRACTION)) || asks == (Some(width), Some(UNIT_POINTS));
        if !whole {
            return false;
        }
        let Some(parent) = peek(env, element + PARENT) else {
            return false;
        };
        element = parent;
    }
    false
}
