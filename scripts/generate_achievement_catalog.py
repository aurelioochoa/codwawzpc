#!/usr/bin/env python3
"""Export the Rust achievement definitions for the launcher first-launch view."""
import ast
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/codwawzpc/achievements.rs').read_text()
source = source.split('pub const CATALOG: &[Achievement] = &[', 1)[1].split('\n];', 1)[0]
pattern = r'a\(\s*(\d+)\s*,\s*(\d+)\s*,\s*Map::(\w+)\s*,\s*("(?:\\.|[^"\\])*")\s*,\s*("(?:\\.|[^"\\])*")\s*,?\s*\)'
maps = {'Any': 'any', 'Nacht': 'nacht', 'Verruckt': 'verruckt',
        'ShiNoNuma': 'shinonuma', 'DerRiese': 'derriese'}
catalog = [{'Id': int(id_), 'Points': int(points), 'Map': maps[map_],
            'Name': ast.literal_eval(name), 'Description': ast.literal_eval(description)}
           for id_, points, map_, name, description in re.findall(pattern, source, re.S)]
if len(catalog) != 106 or sum(item['Points'] for item in catalog) != 105050:
    raise SystemExit('Achievement catalog changed; inspect the Rust definitions and exporter.')
(ROOT / 'launcher/assets/achievements.json').write_text(
    json.dumps(catalog, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(f'Exported {len(catalog)} achievements.')
