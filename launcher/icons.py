# SPDX-License-Identifier: MPL-2.0
"""Launcher-only app icon selection; the host settings schema stays separate."""
import os
from pathlib import Path
import plistlib
import tempfile
from xml.parsers.expat import ExpatError


ICONS = {'wunderwaffe': 'Wunderwaffe', 'undead': 'Undead',
         'field-radio': 'Field Radio', 'mystery-box': 'Mystery Box'}
DEFAULT_ICON = 'wunderwaffe'
ASSETS = Path(__file__).resolve().parent / 'assets'
ICON_SIZES = (256, 128, 64, 48, 32, 16)


def load_icon(dirs):
    """A missing or damaged cosmetic preference must not prevent startup."""
    try:
        values = plistlib.loads((dirs.config / 'launcher.plist').read_bytes())
        icon = values.get('Icon') if isinstance(values, dict) else None
        return icon if isinstance(icon, str) and icon in ICONS else DEFAULT_ICON
    except (OSError, ValueError, TypeError, OverflowError, ExpatError):
        return DEFAULT_ICON


def save_icon(dirs, icon):
    if not isinstance(icon, str) or icon not in ICONS:
        raise ValueError('Choose an available launcher icon')
    dirs.config.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.launcher-icon-', dir=dirs.config)
    try:
        with os.fdopen(fd, 'wb') as stream:
            plistlib.dump({'Icon': icon}, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, dirs.config / 'launcher.plist')
    finally:
        if os.path.exists(name):
            os.unlink(name)
