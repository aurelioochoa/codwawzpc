# SPDX-License-Identifier: MPL-2.0
"""Launcher-only preferences in launcher.plist; the host settings schema stays separate."""
import os
import plistlib
import tempfile
from xml.parsers.expat import ExpatError


def load(dirs):
    """A missing or damaged cosmetic preference must not prevent startup."""
    try:
        values = plistlib.loads((dirs.config / 'launcher.plist').read_bytes())
        return values if isinstance(values, dict) else {}
    except (OSError, ValueError, TypeError, OverflowError, ExpatError):
        return {}


def update(dirs, **values):
    """Atomically replace launcher.plist, keeping the other preferences it holds."""
    merged = {**load(dirs), **values}
    dirs.config.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.launcher-prefs-', dir=dirs.config)
    try:
        with os.fdopen(fd, 'wb') as stream:
            plistlib.dump(merged, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, dirs.config / 'launcher.plist')
    finally:
        if os.path.exists(name):
            os.unlink(name)
