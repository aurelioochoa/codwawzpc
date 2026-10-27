# SPDX-License-Identifier: MPL-2.0
"""Launcher-only app icon selection; the host settings schema stays separate."""
from pathlib import Path

from . import preferences
from .i18n import N_


ICONS = {'wunderwaffe': N_('Wunderwaffe'), 'undead': N_('Undead'),
         'field-radio': N_('Field Radio'), 'mystery-box': N_('Mystery Box')}
DEFAULT_ICON = 'wunderwaffe'
ASSETS = Path(__file__).resolve().parent / 'assets'
ICON_SIZES = (256, 128, 64, 48, 32, 16)


def load_icon(dirs):
    icon = preferences.load(dirs).get('Icon')
    return icon if isinstance(icon, str) and icon in ICONS else DEFAULT_ICON


def save_icon(dirs, icon):
    if not isinstance(icon, str) or icon not in ICONS:
        raise ValueError('Choose an available launcher icon')
    preferences.update(dirs, Icon=icon)
