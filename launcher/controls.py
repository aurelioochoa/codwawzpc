# SPDX-License-Identifier: MPL-2.0
"""Gameplay binding persistence and validation, shared with the Rust host."""
import os
import plistlib
import tempfile

DEFAULTS = {
    'Forward': 'Key:W,Key:Up', 'Left': 'Key:A,Key:Left',
    'Back': 'Key:S,Key:Down', 'Right': 'Key:D,Key:Right',
    'Sprint': 'Key:Left Shift,Key:Right Shift,Pad:LeftStick',
    'Reload': 'Key:R,Pad:X', 'Interact': 'Key:E,Key:F,Pad:A',
    'Grenade': 'Key:G,Pad:RightShoulder', 'Special': 'Key:4,Pad:LeftShoulder',
    'Melee': 'Key:V,Pad:B,Pad:RightStick', 'NextWeapon': 'Key:Q,Mouse:Wheel,Pad:Y',
    'Fire': 'Mouse:Left,Pad:RightTrigger', 'Aim': 'Mouse:Right,Pad:LeftTrigger',
    'StickPreset': 'normal',
}
KEYS = {'Up', 'Down', 'Left', 'Right', 'Left Shift', 'Right Shift', 'Left Ctrl',
        'Right Ctrl', 'Left Alt', 'Right Alt', 'Space', 'Backspace', 'Delete',
        'Home', 'End', 'PageUp', 'PageDown', 'Insert', '-', '=', '[', ']', ';',
        "'", '.', '/', '\\'}
PAD = {'A', 'B', 'X', 'Y', 'LeftShoulder', 'RightShoulder', 'LeftStick', 'RightStick',
       'DPadUp', 'DPadDown', 'DPadLeft', 'DPadRight', 'LeftTrigger', 'RightTrigger'}
MOUSE = {'Left', 'Middle', 'Right', 'X1', 'X2', 'Wheel'}


def validate(values):
    if type(values) is not dict or values.keys() != DEFAULTS.keys():
        raise ValueError('Controls must contain every action and StickPreset')
    if values['StickPreset'] not in ('normal', 'swapped'):
        raise ValueError('Stick preset must be normal or swapped')
    seen = {}
    result = {}
    for action in DEFAULTS:
        if action == 'StickPreset':
            result[action] = values[action]
            continue
        if type(values[action]) is not str:
            raise ValueError(f'{action} must be a list of inputs')
        inputs = [part.strip() for part in values[action].split(',') if part.strip()]
        for token in inputs:
            kind, separator, name = token.partition(':')
            valid = separator and ((kind == 'Key' and (name in KEYS or
                      len(name) == 1 and name in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789')) or
                    (kind == 'Pad' and name in PAD) or (kind == 'Mouse' and name in MOUSE))
            if not valid:
                raise ValueError(f'Invalid input {token} for {action}')
            if token in seen:
                raise ValueError(f'Conflict: {token} is assigned to {seen[token]} and {action}')
            seen[token] = action
        result[action] = ','.join(inputs)
    return result


def load(dirs):
    path = dirs.config / 'controls.plist'
    if not path.exists():
        return dict(DEFAULTS)
    try:
        return validate(plistlib.loads(path.read_bytes()))
    except (ValueError, OSError, plistlib.InvalidFileException) as error:
        raise ValueError(f'Cannot read {path}: {error}') from error


def save(dirs, values):
    values = validate(values)
    dirs.config.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.controls-', dir=dirs.config)
    try:
        with os.fdopen(fd, 'wb') as stream:
            plistlib.dump(values, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, dirs.config / 'controls.plist')
    finally:
        if os.path.exists(name):
            os.unlink(name)
