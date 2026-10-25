#!/usr/bin/env python3
"""Check the saved art contract, genuine alpha, and integrity of every asset."""
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = {'swamp-background', 'wunderwaffe', 'lantern', 'radio-housing',
            'play-plate', 'utility-plate', 'title', 'status-lamp', 'achievement-trophy',
            'icon-settings', 'icon-saves', 'icon-import', 'icon-achievements', 'field-radio',
            'field-archive', 'field-achievements', 'field-supply', 'field-paper',
            'app-icon-wunderwaffe', 'app-icon-undead', 'app-icon-field-radio',
            'app-icon-mystery-box', 'monkey-bomb', 'maintenance-rail', 'rail-switches', 'rail-button'}


def check_image(path, info, transparent, exported):
    assert path.is_file(), path
    assert hashlib.sha256(path.read_bytes()).hexdigest() == info['sha256'], path
    with Image.open(path) as im:
        im.load()
        assert list(im.size) == info['size'], path
        assert im.mode == info['mode'], path
        assert min(im.size) >= 32, path
        if transparent:
            assert im.mode == 'RGBA', path
            alpha = im.getchannel('A')
            low, high = alpha.getextrema()
            assert low == 0 and high >= 240, path
            hist = alpha.histogram()
            assert hist[0] > im.width * im.height * .01, path
            assert sum(hist[240:]) > im.width * im.height * .01, path
            box = alpha.getbbox()
            assert box, path
            if exported:
                assert box[0] > 0 and box[1] > 0 and box[2] < im.width and box[3] < im.height, path
            assert hist[0] == info['alpha']['transparent'], path
            assert hist[255] == info['alpha']['opaque'], path
        else:
            assert 'A' not in im.getbands() or im.getchannel('A').getextrema() == (255, 255), path


def main():
    manifest = json.loads((ROOT / 'launcher/assets/manifest.json').read_text())
    prompts = json.loads((ROOT / manifest['spec']).read_text())
    assets = manifest['assets']
    assert {x['id'] for x in assets} == REQUIRED
    assert len(assets) == len(REQUIRED)
    assert {x['id'] for x in prompts['assets']} == REQUIRED
    for asset in assets:
        prompt = next(x for x in prompts['assets'] if x['id'] == asset['id'])
        assert prompt['transparent'] == asset['transparent']
        for kind in ('source', 'output'):
            path = ROOT / asset[kind]
            assert path.resolve().is_relative_to(ROOT)
            check_image(path, asset[kind + '_info'], asset['transparent'],
                        kind == 'output' and not prompt.get('preserve_original'))
        if prompt.get('preserve_original'):
            assert asset['source_info']['sha256'] == asset['output_info']['sha256']
        if asset['id'].startswith('app-icon-'):
            assert asset['output_info']['size'] == [256, 256]
            with Image.open((ROOT / asset['output']).with_suffix('.ico')) as icon:
                assert icon.ico.sizes() == {(s, s) for s in (16, 24, 32, 48, 64, 128, 256)}
                for size in icon.ico.sizes():
                    frame = icon.ico.getimage(size)
                    frame.load()
                    assert frame.size == size and frame.getchannel('A').getextrema() == (0, 255)
    rows = (len(REQUIRED) + 2) // 3
    for name, size in [('composition-preview.png', (1672, 941)), ('asset-contact-sheet.png', (1260, 315 * rows))]:
        with Image.open(ROOT / 'docs/launcher-assets' / name) as im:
            im.load()
            assert im.size == size
    print(f'LAUNCHER_ASSETS_VERIFIED: {len(assets)} masters, {len(assets)} exports, 2 previews; hashes, bounds and alpha valid.')


if __name__ == '__main__':
    main()
