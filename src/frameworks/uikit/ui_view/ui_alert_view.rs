/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIAlertView`.
//!
//! The alert is shown in its own full-screen window above the app's windows,
//! so it is modal like on the device. Apps may add their own subviews (e.g. a
//! `UITextField` for text entry, as Marmalade's `s3eOSReadString` does); the
//! buttons are laid out below them. Pressing Return in a text field inside an
//! alert clicks its default button, since there's no on-screen keyboard.

use crate::frameworks::core_graphics::cg_affine_transform::{
    CGAffineTransform, CGAffineTransformIdentity,
};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{ns_string, NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr, SEL,
};
use crate::window::DeviceOrientation;
use crate::Environment;

const WIDTH: CGFloat = 284.0;
const MARGIN: CGFloat = 12.0;
const TITLE_HEIGHT: CGFloat = 24.0;
const MESSAGE_HEIGHT: CGFloat = 21.0;
const BUTTON_HEIGHT: CGFloat = 40.0;

struct UIAlertViewHostObject {
    superclass: super::UIViewHostObject,
    /// Weak reference.
    delegate: id,
    /// `UILabel*`
    title_label: id,
    /// `UILabel*`
    message_label: id,
    /// `UILabel*`s, one per button, in index order.
    buttons: Vec<id>,
    cancel_button_index: NSInteger,
    /// The full-screen window the alert is shown in, while visible.
    window: id,
}
impl_HostObject_with_superclass!(UIAlertViewHostObject);
impl Default for UIAlertViewHostObject {
    fn default() -> Self {
        UIAlertViewHostObject {
            superclass: Default::default(),
            delegate: nil,
            title_label: nil,
            message_label: nil,
            buttons: Vec::new(),
            cancel_button_index: -1,
            window: nil,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIAlertView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIAlertViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];

    let background: id = msg_class![env; UIColor colorWithRed:0.05f32 green:0.1f32 blue:0.25f32 alpha:0.92f32];
    () = msg![env; this setBackgroundColor:background];

    let title_label = new_label(env, 18.0, true);
    let message_label = new_label(env, 16.0, false);
    () = msg![env; this addSubview:title_label];
    () = msg![env; this addSubview:message_label];

    let host_object = env.objc.borrow_mut::<UIAlertViewHostObject>(this);
    host_object.title_label = title_label;
    host_object.message_label = message_label;

    this
}

- (id)initWithTitle:(id)title // NSString*
            message:(id)message // NSString*
           delegate:(id)delegate
  cancelButtonTitle:(id)cancel_button_title // NSString*
  otherButtonTitles:(id)first_other_title, // NSString*
                    ...args {
    let this: id = msg![env; this init];
    () = msg![env; this setTitle:title];
    () = msg![env; this setMessage:message];
    () = msg![env; this setDelegate:delegate];
    if cancel_button_title != nil {
        let index: NSInteger = msg![env; this addButtonWithTitle:cancel_button_title];
        env.objc.borrow_mut::<UIAlertViewHostObject>(this).cancel_button_index = index;
    }
    if first_other_title != nil {
        let _: NSInteger = msg![env; this addButtonWithTitle:first_other_title];
        let mut varargs = args.start();
        loop {
            let title: id = varargs.next(env);
            if title == nil {
                break;
            }
            let _: NSInteger = msg![env; this addButtonWithTitle:title];
        }
    }
    this
}

- (())dealloc {
    let UIAlertViewHostObject {
        title_label,
        message_label,
        buttons,
        window,
        ..
    } = std::mem::take(env.objc.borrow_mut(this));
    release(env, title_label);
    release(env, message_label);
    for button in buttons {
        release(env, button);
    }
    release(env, window);
    msg_super![env; this dealloc]
}

// weak/non-retaining
- (())setDelegate:(id)delegate { // UIAlertViewDelegate
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).delegate = delegate;
}
- (id)delegate {
    env.objc.borrow::<UIAlertViewHostObject>(this).delegate
}

- (id)title {
    let label = env.objc.borrow::<UIAlertViewHostObject>(this).title_label;
    msg![env; label text]
}
- (())setTitle:(id)title { // NSString*
    let label = env.objc.borrow::<UIAlertViewHostObject>(this).title_label;
    () = msg![env; label setText:title];
}

- (id)message {
    let label = env.objc.borrow::<UIAlertViewHostObject>(this).message_label;
    msg![env; label text]
}
- (())setMessage:(id)message { // NSString*
    let label = env.objc.borrow::<UIAlertViewHostObject>(this).message_label;
    () = msg![env; label setText:message];
}

- (NSInteger)addButtonWithTitle:(id)title { // NSString*
    let button = new_label(env, 18.0, true);
    () = msg![env; button setText:title];
    let background: id = msg_class![env; UIColor colorWithWhite:1.0f32 alpha:0.2f32];
    () = msg![env; button setBackgroundColor:background];
    () = msg![env; this addSubview:button];
    let buttons = &mut env.objc.borrow_mut::<UIAlertViewHostObject>(this).buttons;
    buttons.push(button);
    (buttons.len() - 1) as NSInteger
}

- (NSInteger)numberOfButtons {
    env.objc.borrow::<UIAlertViewHostObject>(this).buttons.len() as NSInteger
}

- (id)buttonTitleAtIndex:(NSInteger)index {
    let buttons = &env.objc.borrow::<UIAlertViewHostObject>(this).buttons;
    let Some(&button) = usize::try_from(index).ok().and_then(|i| buttons.get(i)) else {
        return nil;
    };
    msg![env; button text]
}

- (NSInteger)cancelButtonIndex {
    env.objc.borrow::<UIAlertViewHostObject>(this).cancel_button_index
}
- (())setCancelButtonIndex:(NSInteger)index {
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).cancel_button_index = index;
}

- (bool)isVisible {
    env.objc.borrow::<UIAlertViewHostObject>(this).window != nil
}

- (())show {
    if env.objc.borrow::<UIAlertViewHostObject>(this).window != nil {
        return;
    }
    log_dbg!("[(UIAlertView*){:?} show]", this);

    let screen: id = msg_class![env; UIScreen mainScreen];
    let screen_bounds: CGRect = msg![env; screen bounds];
    let window: id = msg_class![env; UIWindow alloc];
    let window: id = msg![env; window initWithFrame:screen_bounds];
    let dim: id = msg_class![env; UIColor colorWithWhite:0.0f32 alpha:0.4f32];
    () = msg![env; window setBackgroundColor:dim];
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).window = window;

    layout(env, this);
    // touchHLE only runs a layout pass at launch, so do one for the views
    // entering the screen (e.g. so a text field sizes its text label).
    let subviews: id = msg![env; this subviews];
    let count: NSUInteger = msg![env; subviews count];
    for i in 0..count {
        let view: id = msg![env; subviews objectAtIndex:i];
        () = msg![env; view layoutSubviews];
    }
    () = msg![env; window addSubview:this];
    () = msg![env; window setHidden:false];

    let sel: SEL = env.objc.register_host_selector("didPresentAlertView:".to_string(), &mut env.mem);
    let delegate = env.objc.borrow::<UIAlertViewHostObject>(this).delegate;
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate didPresentAlertView:this];
    }
}

- (())dismissWithClickedButtonIndex:(NSInteger)index
                           animated:(bool)_animated {
    let window = env.objc.borrow::<UIAlertViewHostObject>(this).window;
    if window == nil {
        return;
    }
    log_dbg!("[(UIAlertView*){:?} dismissWithClickedButtonIndex:{}]", this, index);

    // Removing the alert from its window may drop the last reference.
    retain(env, this);
    let delegate = env.objc.borrow::<UIAlertViewHostObject>(this).delegate;
    let sel: SEL = env.objc.register_host_selector("alertView:willDismissWithButtonIndex:".to_string(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate alertView:this willDismissWithButtonIndex:index];
    }

    let _: bool = msg![env; this endEditing:true];
    () = msg![env; this removeFromSuperview];
    () = msg![env; window setHidden:true];
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).window = nil;
    release(env, window);

    let sel: SEL = env.objc.register_host_selector("alertView:didDismissWithButtonIndex:".to_string(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate alertView:this didDismissWithButtonIndex:index];
    }
    release(env, this);
}

// UIResponder implementation: clicking a button.
- (())touchesBegan:(id)_touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
}
- (())touchesMoved:(id)_touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
}
- (())touchesEnded:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let buttons = env.objc.borrow::<UIAlertViewHostObject>(this).buttons.clone();
    for (index, button) in buttons.into_iter().enumerate() {
        let frame: CGRect = msg![env; button frame];
        if point.x >= frame.origin.x
            && point.x < frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y < frame.origin.y + frame.size.height
        {
            click(env, this, index as NSInteger);
            return;
        }
    }
}

@end

};

fn new_label(env: &mut Environment, size: CGFloat, bold: bool) -> id {
    let label: id = msg_class![env; UILabel new];
    let font: id = if bold {
        msg_class![env; UIFont boldSystemFontOfSize:size]
    } else {
        msg_class![env; UIFont systemFontOfSize:size]
    };
    () = msg![env; label setFont:font];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; label setTextColor:white];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; label setBackgroundColor:clear];
    () = msg![env; label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; label setUserInteractionEnabled:false];
    label
}

fn has_visible_text(env: &mut Environment, label: id) -> bool {
    let text: id = msg![env; label text];
    text != nil && !ns_string::to_rust_string(env, text).trim().is_empty()
}

/// Size the alert around its title, message, the app's own subviews and the
/// buttons, and center it upright on the screen.
fn layout(env: &mut Environment, this: id) {
    let UIAlertViewHostObject {
        title_label,
        message_label,
        ref buttons,
        window,
        ..
    } = *env.objc.borrow(this);
    let buttons = buttons.clone();

    let inner_width = WIDTH - 2.0 * MARGIN;
    let mut bottom = MARGIN;
    for (label, height) in [(title_label, TITLE_HEIGHT), (message_label, MESSAGE_HEIGHT)] {
        if has_visible_text(env, label) {
            let frame = CGRect {
                origin: CGPoint {
                    x: MARGIN,
                    y: bottom,
                },
                size: CGSize {
                    width: inner_width,
                    height,
                },
            };
            () = msg![env; label setFrame:frame];
            () = msg![env; label setHidden:false];
            bottom += height + 4.0;
        } else {
            () = msg![env; label setHidden:true];
        }
    }

    // Subviews added by the app keep their frames.
    let subviews: id = msg![env; this subviews];
    let count: NSUInteger = msg![env; subviews count];
    for i in 0..count {
        let view: id = msg![env; subviews objectAtIndex:i];
        if view == title_label || view == message_label || buttons.contains(&view) {
            continue;
        }
        let frame: CGRect = msg![env; view frame];
        bottom = bottom.max(frame.origin.y + frame.size.height);
    }
    bottom += MARGIN;

    // Two buttons share a row, otherwise they are stacked.
    let side_by_side = buttons.len() == 2;
    let button_width = if side_by_side {
        (inner_width - MARGIN) / 2.0
    } else {
        inner_width
    };
    for (i, &button) in buttons.iter().enumerate() {
        let (x, y) = if side_by_side {
            (MARGIN + i as CGFloat * (button_width + MARGIN), bottom)
        } else {
            (MARGIN, bottom + i as CGFloat * (BUTTON_HEIGHT + 8.0))
        };
        let frame = CGRect {
            origin: CGPoint { x, y },
            size: CGSize {
                width: button_width,
                height: BUTTON_HEIGHT,
            },
        };
        () = msg![env; button setFrame:frame];
    }
    let rows = match buttons.len() {
        0 => 0,
        _ if side_by_side => 1,
        n => n,
    };
    if rows > 0 {
        bottom += rows as CGFloat * (BUTTON_HEIGHT + 8.0) - 8.0 + MARGIN;
    }

    let window_bounds: CGRect = msg![env; window bounds];
    let bounds = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: WIDTH,
            height: bottom,
        },
    };
    let center = CGPoint {
        x: window_bounds.size.width / 2.0,
        y: window_bounds.size.height / 2.0,
    };
    // Keep the alert upright when the interface is rotated. (Checked against
    // the composited output in landscape right.)
    let transform = match env.window().current_rotation() {
        DeviceOrientation::Portrait => CGAffineTransformIdentity,
        DeviceOrientation::PortraitUpsideDown => {
            CGAffineTransform::make_rotation(-std::f32::consts::PI)
        }
        DeviceOrientation::LandscapeLeft => {
            CGAffineTransform::make_rotation(std::f32::consts::FRAC_PI_2)
        }
        DeviceOrientation::LandscapeRight => {
            CGAffineTransform::make_rotation(-std::f32::consts::FRAC_PI_2)
        }
    };
    () = msg![env; this setTransform:transform];
    () = msg![env; this setBounds:bounds];
    () = msg![env; this setCenter:center];
}

/// Click a button: tell the delegate, then dismiss.
fn click(env: &mut Environment, this: id, index: NSInteger) {
    log_dbg!("UIAlertView {:?}: clicked button {}", this, index);
    retain(env, this);
    let delegate = env.objc.borrow::<UIAlertViewHostObject>(this).delegate;
    let sel: SEL = env
        .objc
        .register_host_selector("alertView:clickedButtonAtIndex:".to_string(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate alertView:this clickedButtonAtIndex:index];
    }
    () = msg![env; this dismissWithClickedButtonIndex:index animated:true];
    release(env, this);
}

/// Return was pressed in `text_field`. If it is inside a visible alert, click
/// the alert's default button (the first one that isn't the cancel button).
/// Returns true if an alert handled it.
pub fn text_field_returned(env: &mut Environment, text_field: id) -> bool {
    let alert_class = env.objc.get_known_class("UIAlertView", &mut env.mem);
    let mut view: id = msg![env; text_field superview];
    while view != nil {
        let class: crate::objc::Class = msg![env; view class];
        if env.objc.class_is_subclass_of(class, alert_class) {
            break;
        }
        view = msg![env; view superview];
    }
    if view == nil || env.objc.borrow::<UIAlertViewHostObject>(view).window == nil {
        return false;
    }
    let host_object = env.objc.borrow::<UIAlertViewHostObject>(view);
    let count = host_object.buttons.len() as NSInteger;
    let cancel = host_object.cancel_button_index;
    let index = (0..count).find(|&i| i != cancel).unwrap_or(cancel);
    click(env, view, index);
    true
}
