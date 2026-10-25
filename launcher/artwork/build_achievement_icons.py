#!/usr/bin/env python3
"""Package individually image-generated trophy masters for the launcher.

Never generates or paints artwork. Every full-resolution transparent master
comes from a separate built-in image_gen request. Only runtime copies and
the review contact sheet are resized here.
"""
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps

ROOT = Path(__file__).resolve().parents[2]
CATALOG = ROOT / 'launcher/assets/achievements.json'
SPEC = ROOT / 'launcher/artwork/achievement-spec.json'
SOURCES = ROOT / 'launcher/artwork/sources/achievement-icons'
OUTPUT = ROOT / 'launcher/assets/achievement-icons'
SIZE = 256


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build():
    catalog = json.loads(CATALOG.read_text(encoding='utf-8'))
    spec = json.loads(SPEC.read_text(encoding='utf-8'))
    designs = {row['Id']: row for row in spec['Icons']}
    assert len(designs) == len(spec['Icons']) == len(catalog)
    assert set(designs) == {row['Id'] for row in catalog}, 'Artwork/catalog mismatch'
    # Validate every master before touching a working runtime asset.
    for row in catalog:
        path = SOURCES / f"{row['Id']:03d}.png"
        assert digest(path) == designs[row['Id']]['SourceSha256'], 'Master changed; review and update the artwork specification'
        with Image.open(path) as source:
            assert source.mode == 'RGBA', f'{path}: expected generated alpha'
            assert source.width * source.height >= 1_000_000, f'{path}: expected full-resolution master'
            assert source.getchannel('A').getextrema() == (0, 255), f'{path}: missing transparency'
    OUTPUT.mkdir(parents=True, exist_ok=True)
    records = []
    sheet = Image.new('RGB', (1000, 1100), '#151913')
    draw = ImageDraw.Draw(sheet)
    face = ImageFont.load_default()
    for index, row in enumerate(catalog):
        path = SOURCES / f"{row['Id']:03d}.png"
        with Image.open(path) as source:
            source_size = list(source.size)
            icon = ImageOps.contain(source, (SIZE, SIZE), Image.Resampling.LANCZOS)
            runtime = Image.new('RGBA', (SIZE, SIZE))
            runtime.paste(icon, ((SIZE - icon.width) // 2, (SIZE - icon.height) // 2))
        target = OUTPUT / path.name
        runtime.save(target)
        thumb = runtime.resize((82, 82), Image.Resampling.LANCZOS)
        x, y = (index % 10) * 100, (index // 10) * 100
        sheet.paste(thumb, (x + 9, y + 1), thumb)
        draw.text((x + 50, y + 89), f"{row['Id']:03d}", fill='#eee0bd', font=face, anchor='mm')
        design = designs[row['Id']]
        records.append({
            'Id': row['Id'], 'Name': row['Name'], 'Map': row['Map'],
            'File': target.name, 'Sha256': digest(target),
            'Source': str(path.relative_to(ROOT)), 'SourceSha256': digest(path),
            'SourceSize': source_size,
            'Generator': 'built-in image_gen',
            'ConsoleReference': design['ConsoleReference'],
        })
    (OUTPUT / 'manifest.json').write_text(json.dumps({
        'Source': 'achievements.json', 'Generator': 'built-in image_gen',
        'Spec': str(SPEC.relative_to(ROOT)), 'Icons': records,
    }, indent=2) + '\n', encoding='utf-8')
    sheet.save(ROOT / 'docs/launcher/achievement-icons-contact.png')
    print(f'Packaged {len(catalog)} individually generated achievement trophies.')


if __name__ == '__main__':
    build()
