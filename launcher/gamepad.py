# SPDX-License-Identifier: MPL-2.0
"""Linux gamepads read directly from evdev; no extra Python packages are needed.

Desktop sessions grant the signed-in user access to game controllers through
udev's uaccess rule. Devices that cannot be opened are skipped quietly.
"""
import fcntl
import os
from pathlib import Path
import struct
import time

# struct input_event: struct timeval, __u16 type, __u16 code, __s32 value.
EVENT = struct.Struct('llHHi')
EV_KEY, EV_ABS = 0x01, 0x03
BTN_SOUTH, BTN_EAST, BTN_TL, BTN_TR, BTN_START = 0x130, 0x131, 0x136, 0x137, 0x13b
BUTTONS = {BTN_SOUTH: 'accept', BTN_EAST: 'back', BTN_TL: 'previous', BTN_TR: 'next', BTN_START: 'start'}
DPAD = {0x220: 'up', 0x221: 'down', 0x222: 'left', 0x223: 'right'}
ABS_X, ABS_Y, ABS_RY, ABS_HAT0X, ABS_HAT0Y = 0x00, 0x01, 0x04, 0x10, 0x11
STICK_ON, STICK_OFF, SCROLL_DEAD = .55, .35, .22
REPEAT_DELAY, REPEAT_RATE, RESCAN = .38, .11, 2.0


def eviocgabs(axis):
    # _IOR('E', 0x40 + axis, struct input_absinfo), six __s32 fields.
    return 0x80184540 + axis


def has_bit(words, code):
    bits = struct.calcsize('l') * 8
    words = words.split()
    index = code // bits
    return index < len(words) and bool(int(words[-1 - index], 16) >> (code % bits) & 1)


class Pad:
    def __init__(self, path):
        self.path = path
        self.fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
        self.ranges, self.axes, self.dpad = {}, {}, set()
        # D-pad presses seen since the last poll, so a tap shorter than one poll still counts.
        self.taps = []
        for axis in (ABS_X, ABS_Y, ABS_RY, ABS_HAT0X, ABS_HAT0Y):
            try:
                value, low, high = struct.unpack('6i', fcntl.ioctl(self.fd, eviocgabs(axis), bytes(24)))[:3]
            except OSError:
                continue
            if high > low:
                self.ranges[axis] = (low, high)
                self.axes[axis] = self.normal(axis, value)

    def normal(self, axis, value):
        low, high = self.ranges[axis]
        if axis in (ABS_HAT0X, ABS_HAT0Y):
            return float((value > 0) - (value < 0))
        return max(-1., min(1., (value - (low + high) / 2) / ((high - low) / 2)))

    def read(self):
        """Return pressed button actions; axis and d-pad state is kept on the pad."""
        pressed = []
        while True:
            try:
                data = os.read(self.fd, EVENT.size * 64)
            except BlockingIOError:
                return pressed
            if not data:
                return pressed
            for offset in range(0, len(data) - EVENT.size + 1, EVENT.size):
                _, _, kind, code, value = EVENT.unpack_from(data, offset)
                if kind == EV_KEY and code in BUTTONS and value == 1:
                    pressed.append(BUTTONS[code])
                elif kind == EV_KEY and code in DPAD:
                    (self.dpad.add if value else self.dpad.discard)(DPAD[code])
                    if value == 1:
                        self.taps.append(DPAD[code])
                elif kind == EV_ABS and code in self.ranges:
                    previous, self.axes[code] = self.axes.get(code, 0), self.normal(code, value)
                    if code in (ABS_HAT0X, ABS_HAT0Y) and self.axes[code] and self.axes[code] != previous:
                        names = ('left', 'right') if code == ABS_HAT0X else ('up', 'down')
                        self.taps.append(names[self.axes[code] > 0])

    def direction(self, held):
        """The pad's current direction: d-pad first, then the left stick with hysteresis."""
        x, y = self.axes.get(ABS_HAT0X, 0), self.axes.get(ABS_HAT0Y, 0)
        if self.dpad:
            x = ('right' in self.dpad) - ('left' in self.dpad)
            y = ('down' in self.dpad) - ('up' in self.dpad)
        if x or y:
            return ('right' if x > 0 else 'left') if abs(x) >= abs(y) else ('down' if y > 0 else 'up')
        x, y = self.axes.get(ABS_X, 0), self.axes.get(ABS_Y, 0)
        threshold = STICK_OFF if held else STICK_ON
        if max(abs(x), abs(y)) < threshold:
            return None
        return ('right' if x > 0 else 'left') if abs(x) >= abs(y) else ('down' if y > 0 else 'up')

    def scroll(self):
        value = self.axes.get(ABS_RY, 0)
        return 0. if abs(value) < SCROLL_DEAD else (value - SCROLL_DEAD * (1 if value > 0 else -1)) / (1 - SCROLL_DEAD)

    def close(self):
        try:
            os.close(self.fd)
        except OSError:
            pass


class Gamepads:
    """Polls every connected gamepad and turns its state into launcher actions."""
    def __init__(self, devices=Path('/dev/input'), sysfs=Path('/sys/class/input')):
        self.devices, self.sysfs = Path(devices), Path(sysfs)
        self.pads = {}
        self.rejected = set()
        self.scanned = -RESCAN
        self.held = None
        self.repeat_at = 0.

    def is_gamepad(self, name):
        try:
            return has_bit((self.sysfs / name / 'device/capabilities/key').read_text(), BTN_SOUTH)
        except OSError:
            return False

    def scan(self):
        try:
            names = sorted(name for name in os.listdir(self.devices) if name.startswith('event'))
        except OSError:
            names = []
        present = {str(self.devices / name) for name in names}
        self.rejected &= present
        for path in list(self.pads):
            if path not in present:
                self.pads.pop(path).close()
        for name in names:
            path = str(self.devices / name)
            if path in self.pads or path in self.rejected:
                continue
            if not self.is_gamepad(name):
                self.rejected.add(path)
                continue
            try:
                self.pads[path] = Pad(path)
            except OSError:
                self.rejected.add(path)

    def poll(self, now=None):
        """Return (actions, scroll): discrete actions and the right stick's vertical deflection."""
        now = time.monotonic() if now is None else now
        if now - self.scanned >= RESCAN:
            self.scanned = now
            self.scan()
        actions, taps, direction, scroll = [], [], None, 0.
        for path, pad in list(self.pads.items()):
            try:
                actions += pad.read()
                taps += pad.taps
                pad.taps = []
            except OSError:
                # Unplugged or access revoked; the next scan reopens it if it returns.
                self.pads.pop(path).close()
                continue
            direction = direction or pad.direction(self.held)
            scroll = scroll or pad.scroll()
        if direction != self.held:
            self.held = direction
            if direction:
                if direction in taps:
                    taps.remove(direction)
                taps.append(direction)
                self.repeat_at = now + REPEAT_DELAY
        elif direction and now >= self.repeat_at:
            taps.append(direction)
            self.repeat_at = now + REPEAT_RATE
        return taps + actions, scroll

    def close(self):
        for pad in self.pads.values():
            pad.close()
        self.pads.clear()
