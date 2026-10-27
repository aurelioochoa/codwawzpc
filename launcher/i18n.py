# SPDX-License-Identifier: MPL-2.0
"""Launcher interface language. English text is the key; catalogs translate it."""
import json
import os
from pathlib import Path

from . import preferences

# Each language by its own name, so it can be found whatever is shown.
LANGUAGES = {'en': 'English', 'fr': 'Français', 'de': 'Deutsch', 'it': 'Italiano', 'es': 'Español'}
SYSTEM = 'system'
LOCALE = Path(__file__).resolve().parent / 'assets' / 'locale'

_catalog = {}
current = 'en'


def system_language(environ=os.environ):
    """The supported language of the desktop session, following gettext's variable order."""
    locale = next((environ[name] for name in ('LC_ALL', 'LC_MESSAGES', 'LANG') if environ.get(name)), '')
    candidates = [locale]
    # LANGUAGE is a priority list, but gettext ignores it under the C locale.
    if locale.split('.')[0] not in ('', 'C', 'POSIX'):
        candidates = environ.get('LANGUAGE', '').split(':') + candidates
    for candidate in candidates:
        code = candidate[:2].lower()
        if code in LANGUAGES and (len(candidate) == 2 or not candidate[2:3].isalpha()):
            return code
    return 'en'


def load_choice(dirs):
    choice = preferences.load(dirs).get('Language')
    return choice if isinstance(choice, str) and (choice == SYSTEM or choice in LANGUAGES) else SYSTEM


def save_choice(dirs, choice):
    if not isinstance(choice, str) or (choice != SYSTEM and choice not in LANGUAGES):
        raise ValueError('Choose an available launcher language')
    preferences.update(dirs, Language=choice)


def activate(choice):
    """Switch the interface to choice (a language code or SYSTEM); returns the code in use."""
    global _catalog, current
    code = system_language() if choice == SYSTEM else choice
    if code not in LANGUAGES:
        code = 'en'
    catalog = {}
    if code != 'en':
        try:
            catalog = json.loads((LOCALE / f'{code}.json').read_text(encoding='utf-8'))
        except (OSError, ValueError):
            # A damaged catalog falls back to English rather than blocking startup.
            code = 'en'
    _catalog, current = catalog, code
    return code


def N_(text):
    """Mark English text for the catalogs where it is defined; tr() translates it when shown."""
    return text


def tr(text, **values):
    """Translate English text; {name} fields are filled after translation."""
    text = _catalog.get(text, text)
    return text.format(**values) if values else text
