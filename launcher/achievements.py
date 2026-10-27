# SPDX-License-Identifier: MPL-2.0
"""Read the game's achievement report, with a bundled first-launch catalog."""
import json
import plistlib
from pathlib import Path

from .i18n import N_, tr

CATALOG = Path(__file__).resolve().parent / 'assets' / 'achievements.json'
MAPS = {'any': N_('Any map'), 'nacht': 'Nacht der Untoten',
        'verruckt': 'Verrückt', 'shinonuma': 'Shi No Numa',
        'derriese': 'Der Riese'}


def load_achievements(data_dir):
    """Return (rows, message); preserve the catalog if the report is absent or bad."""
    catalog = json.loads(CATALOG.read_text(encoding='utf-8'))
    rows = [{**entry, 'Unlocked': False, 'UnlockedAt': None} for entry in catalog]
    path = Path(data_dir) / 'achievements.plist'
    if not path.exists():
        return rows, tr('Play in Regular mode to record your first achievements.')
    try:
        if path.stat().st_size > 2_000_000:
            raise ValueError('report is too large')
        report = plistlib.loads(path.read_bytes())
        if not isinstance(report, dict) or report.get('Format') != 1:
            raise ValueError('unsupported report format')
        entries = report.get('Achievements')
        if not isinstance(entries, list) or len(entries) != len(rows):
            raise ValueError('incomplete report')
        by_id = {entry['Id']: entry for entry in entries if isinstance(entry, dict) and
                 type(entry.get('Id')) is int}
        if len(by_id) != len(rows):
            raise ValueError('duplicate or missing achievements')
        for row in rows:
            entry = by_id[row['Id']]
            if type(entry.get('Unlocked')) is not bool:
                raise ValueError('invalid unlock state')
            row['Unlocked'] = entry['Unlocked']
            stamp = entry.get('UnlockedAt')
            row['UnlockedAt'] = stamp if row['Unlocked'] and type(stamp) is int and stamp > 0 else None
        return rows, tr('Progress refreshes after you play or import a saved game.')
    except (OSError, ValueError, TypeError, KeyError, plistlib.InvalidFileException) as error:
        return [{**entry, 'Unlocked': False, 'UnlockedAt': None} for entry in catalog], \
            tr('Could not read achievement progress: {error}. The catalog is still available.', error=error)
