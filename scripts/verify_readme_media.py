#!/usr/bin/env python3
"""Decode and verify the reviewed README media, links and privacy metadata."""
import hashlib
import json
from pathlib import Path
import re

from PIL import Image

from verify_source import README_MEDIA, content_errors, path_errors

ROOT = Path(__file__).resolve().parents[1]
MEDIA = ROOT / 'docs/media'


def main():
    manifest = json.loads((MEDIA / 'manifest.json').read_text())
    records = manifest['media']
    assert len(records) == len(README_MEDIA)
    assert {f"docs/media/{item['file']}" for item in records} == README_MEDIA
    assert {p.name for p in MEDIA.iterdir() if p.suffix in ('.png', '.gif')} == {
        item['file'] for item in records}
    total = 0
    gifs = 0
    for item in records:
        name = 'docs/media/' + item['file']
        assert not path_errors(name, '100644'), name
        path = ROOT / name
        payload = path.read_bytes()
        assert hashlib.sha256(payload).hexdigest() == item['sha256'], name
        assert not content_errors(payload), name
        total += len(payload)
        assert item['origin'] in ('generated-artwork', 'launcher-capture', 'gameplay-capture')
        with Image.open(path) as image:
            assert list(image.size) == item['size'], name
            if path.suffix == '.gif':
                assert image.width >= 720 and image.height >= 400, name
            else:
                assert image.width >= 900 and image.height >= 500, name
            frames = getattr(image, 'n_frames', 1)
            assert frames == item['frames'], name
            duration = 0
            distinct = set()
            for frame in range(frames):
                image.seek(frame)
                image.load()
                duration += image.info.get('duration', 0)
                distinct.add(hashlib.sha256(image.convert('RGB').tobytes()).digest())
                for value in (*image.info.values(), dict(image.getexif())):
                    metadata = value if isinstance(value, bytes) else str(value).encode()
                    assert not content_errors(metadata), f'{name}: private decoded metadata'
            if path.suffix == '.gif':
                gifs += 1
                assert frames >= 6 and len(distinct) >= 4, name
                assert image.info.get('loop') == 0 and 3000 <= duration <= 12000, name
                assert duration == item['duration_ms'], name
                assert len(payload) < 8_000_000, name
            else:
                assert image.format == 'PNG' and frames == 1, name
                assert len(payload) < 4_000_000, name
    assert gifs == 3 and total < 20_000_000
    readme = (ROOT / 'README.md').read_text()
    linked = set(re.findall(r'\]\((docs/media/[^)#]+)', readme))
    assert README_MEDIA <= linked
    for name in linked:
        assert (ROOT / name).is_file(), name
    print(f'README_MEDIA_VERIFIED: {len(records)} files; {gifs} looping animations; '
          f'{total:,} bytes; digests, every frame, metadata and README links passed.')


if __name__ == '__main__':
    main()
