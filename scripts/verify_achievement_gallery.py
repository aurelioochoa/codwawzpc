#!/usr/bin/env python3
"""Verify that the launcher ships one real asset slot per achievement."""
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
catalog = json.loads((ROOT / 'launcher/assets/achievements.json').read_text())
folder = ROOT / 'launcher/assets/achievement-icons'
manifest = json.loads((folder / 'manifest.json').read_text())['Icons']
spec = json.loads((ROOT / 'launcher/artwork/achievement-spec.json').read_text())
designs = {item['Id']: item for item in spec['Icons']}
ids = {item['Id'] for item in catalog}
assert len(ids) == len(catalog) == len(manifest)
assert {item['Id'] for item in manifest} == ids
assert len(designs) == len(spec['Icons']) == len(catalog)
assert set(designs) == ids
assert {p.name for p in folder.glob('*.png')} == {f'{id_:03d}.png' for id_ in ids}
hashes = set()
source_hashes = set()
for item in manifest:
    path = folder / item['File']
    assert path.name == f"{item['Id']:03d}.png"
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest == item['Sha256'] and digest not in hashes
    hashes.add(digest)
    assert item['Generator'] == 'built-in image_gen'
    source = ROOT / item['Source']
    assert source == ROOT / 'launcher/artwork/sources/achievement-icons' / path.name
    source_digest = hashlib.sha256(source.read_bytes()).hexdigest()
    assert source_digest == item['SourceSha256'] and source_digest not in source_hashes
    source_hashes.add(source_digest)
    design = designs[item['Id']]
    assert source_digest == design['SourceSha256']
    assert item['ConsoleReference'] == design['ConsoleReference']
    with Image.open(source) as image:
        assert image.mode == 'RGBA' and image.width * image.height >= 1_000_000
        assert list(image.size) == item['SourceSize']
        assert image.getchannel('A').getextrema() == (0, 255)
    with Image.open(path) as image:
        assert image.mode == 'RGBA' and image.size == (256, 256)
        assert image.getchannel('A').getextrema() == (0, 255)
with Image.open(ROOT / 'docs/launcher/achievement-icons-contact.png') as image:
    assert image.size == (1000, 1100)
print(f'ACHIEVEMENT_GALLERY_VERIFIED: {len(ids)} distinct generated masters, runtime icons and contact sheet.')
