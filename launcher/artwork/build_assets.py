#!/usr/bin/env python3
"""Export generated PNGs and compose review images; never retouch source pixels."""
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / 'launcher/artwork'
ASSETS = ROOT / 'launcher/assets'
DOCS = ROOT / 'docs/launcher-assets'
BOUNDS = {
    'maintenance-rail': (2400, 320),
    'rail-switches': (1200, 650),
    'rail-button': (900, 260),
    'monkey-bomb': (520, 600),
    'wunderwaffe': (1600, 700), 'lantern': (500, 900),
    'radio-housing': (2400, 1000), 'play-plate': (800, 300),
    'utility-plate': (500, 300), 'title': (1100, 650),
    'status-lamp': (200, 200), 'icon-settings': (160, 160),
    'icon-saves': (160, 160), 'icon-import': (160, 160),
    'icon-achievements': (160, 160),
    'field-radio': (640, 640), 'field-archive': (640, 640),
    'field-achievements': (640, 640),
    'field-supply': (640, 640),
    'app-icon-wunderwaffe': (240, 240), 'app-icon-undead': (240, 240),
    'app-icon-field-radio': (240, 240),
    'app-icon-mystery-box': (240, 240),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inspect(path):
    with Image.open(path) as im:
        im.load()
        info = {'size': list(im.size), 'mode': im.mode,
                'bytes': path.stat().st_size, 'sha256': digest(path)}
        if 'A' in im.getbands():
            alpha = im.getchannel('A')
            hist = alpha.histogram()
            info['alpha'] = {'range': list(alpha.getextrema()),
                             'transparent': hist[0], 'opaque': hist[255],
                             'intermediate': sum(hist[1:255]),
                             'bbox': list(alpha.getbbox()) if alpha.getbbox() else None}
            info['alpha']['visible_bbox'] = list(visible_bbox(im))
        return info


def visible_bbox(im):
    """Ignore near-invisible generated fringe only when measuring layout bounds."""
    return im.getchannel('A').point(lambda value: 255 if value >= 8 else 0).getbbox()


def fitted(name, box):
    with Image.open(ASSETS / f'{name}.png') as source:
        im = source.convert('RGBA')
        if name != 'swamp-background':
            bounds = visible_bbox(im)
            # Keep a little original halo around the measured content.
            pad = max(2, round(max(im.size) * .005))
            bounds = (max(0, bounds[0] - pad), max(0, bounds[1] - pad),
                      min(im.width, bounds[2] + pad), min(im.height, bounds[3] + pad))
            im = im.crop(bounds)
        im.thumbnail((round(box[2]), round(box[3])), Image.Resampling.LANCZOS)
        return im


def place(canvas, name, box, anchor='center'):
    im = fitted(name, box)
    x, y, w, h = box
    left = round(x if anchor == 'left' else x + (w - im.width) / 2)
    top = round(y + (h - im.height) / 2)
    canvas.alpha_composite(im, (left, top))
    return left, top, im.width, im.height


def font(size, bold=False, condensed=False):
    family = 'NimbusSansNarrow' if condensed else 'NimbusRoman'
    weight = 'Bold' if bold else 'Regular'
    candidates = [Path(base) / f'{family}-{weight}.otf' for base in
                  ('/usr/share/fonts/gsfonts', '/usr/share/fonts/opentype/urw-base35')]
    fallback = 'DejaVuSansCondensed' if condensed else 'DejaVuSerif'
    suffix = '-Bold' if bold else ''
    candidates += [Path(base) / f'{fallback}{suffix}.ttf' for base in
                   ('/usr/share/fonts/TTF', '/usr/share/fonts/truetype/dejavu')]
    for candidate in candidates:
        if candidate.is_file():
            return ImageFont.truetype(str(candidate), size)
    raise RuntimeError('Preview requires URW Base35 or DejaVu fonts.')


def label(canvas, text, box, size, bold=False, condensed=False):
    draw = ImageDraw.Draw(canvas)
    face = font(size, bold, condensed)
    lines = text.split('\n')
    while max(draw.textlength(line, font=face) for line in lines) > box[2] and size > 10:
        size -= 1
        face = font(size, bold, condensed)
    bounds = draw.multiline_textbbox((0, 0), text, font=face, align='center', spacing=3)
    x = box[0] + (box[2] - (bounds[2] - bounds[0])) / 2 - bounds[0]
    y = box[1] + (box[3] - (bounds[3] - bounds[1])) / 2 - bounds[1]
    draw.multiline_text((round(x), round(y)),
                        text, font=face, fill='#ead7b3',
                        align='center', spacing=3, stroke_width=0)


def preview():
    with Image.open(ASSETS / 'swamp-background.png') as background:
        canvas = background.convert('RGBA').resize((1672, 941), Image.Resampling.LANCZOS)
    place(canvas, 'title', (55, 55, 690, 350))
    place(canvas, 'lantern', (1430, 123, 165, 330))
    rx, ry, rw, rh = place(canvas, 'radio-housing', (12, 590, 1648, 345))
    place(canvas, 'wunderwaffe', (0, 365, 1160, 315), 'left')
    # All labels are composed separately from blank artwork, as runtime text will be.
    py = ry + rh * .29
    ph = rh * .54
    primary = (rx + rw * .12, py, rw * .275, ph)
    place(canvas, 'play-plate', primary)
    label(canvas, 'PLAY', primary, 103, bold=True, condensed=True)
    buttons = [('icon-settings', 'Settings &\ncontrols'),
               ('icon-saves', 'Saved\ngames'), ('icon-achievements', 'Achievements')]
    for index, (icon, text) in enumerate(buttons):
        box = (rx + rw * (.41 + index * .139), py + ph * .08, rw * .127, ph * .84)
        bx, by, bw, bh = place(canvas, 'utility-plate', box)
        place(canvas, icon, (bx + bw * .11, by + bh * .26, bw * .22, bh * .43))
        label(canvas, text, (bx + bw * .36, by + bh * .22, bw * .55, bh * .56), 23, bold=True)
    status = (rx + rw * .825, py + ph * .02, rw * .07, ph * .96)
    place(canvas, 'status-lamp', (status[0] + status[2] * .30, status[1], status[2] * .40, status[3] * .48))
    label(canvas, 'Ready to\nplay', (status[0], status[1] + status[3] * .50, status[2], status[3] * .40), 22, bold=True)
    canvas.convert('RGB').save(DOCS / 'composition-preview.png')


def contact_sheet(jobs):
    cw, ch, cols = 420, 315, 3
    sheet = Image.new('RGB', (cw * cols, ((len(jobs) + cols - 1) // cols) * ch), '#171c1b')
    for index, job in enumerate(jobs):
        x, y = (index % cols) * cw, (index // cols) * ch
        tile = Image.new('RGBA', (cw, ch - 42), '#d6d3c5')
        ImageDraw.Draw(tile).rectangle((cw // 2, 0, cw, ch), fill='#202c29')
        place(tile, job['id'], (15, 15, cw - 30, ch - 72))
        sheet.paste(tile.convert('RGB'), (x, y))
        ImageDraw.Draw(sheet).text((x + 14, y + ch - 30), job['id'], font=font(18), fill='#ead7b3')
    sheet.save(DOCS / 'asset-contact-sheet.png')


def main():
    prompts = json.loads((ART / 'asset-spec.json').read_text())
    ASSETS.mkdir(parents=True, exist_ok=True)
    DOCS.mkdir(parents=True, exist_ok=True)
    records = []
    for job in prompts['assets']:
        name = job['id']
        source = ART / 'sources' / f'{name}.png'
        output = ASSETS / f'{name}.png'
        with Image.open(source) as original:
            original.load()
            if job['transparent']:
                if ('A' not in original.getbands()
                        or original.getchannel('A').getextrema()[0] != 0
                        or original.getchannel('A').getextrema()[1] < 240):
                    raise ValueError(f'{name}: generated master must have genuine transparent alpha')
            if job.get('preserve_original'):
                output.write_bytes(source.read_bytes())
            elif job['transparent']:
                crop = (visible_bbox(original) if name in ('maintenance-rail', 'rail-switches', 'rail-button')
                        else original.getchannel('A').getbbox())
                im = original.convert('RGBA').crop(crop)
                im.thumbnail(BOUNDS[name], Image.Resampling.LANCZOS)
                margin = max(4, round(max(im.size) * .02))
                exported = Image.new('RGBA', (im.width + margin * 2, im.height + margin * 2))
                exported.alpha_composite(im, (margin, margin))
                if name.startswith('app-icon-'):
                    exported = Image.new('RGBA', (256, 256))
                    exported.alpha_composite(im, ((256 - im.width) // 2, (256 - im.height) // 2))
                    exported.save(output.with_suffix('.ico'), sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
                exported.save(output)
            else:
                output.write_bytes(source.read_bytes())
        records.append({'id': name, 'transparent': job['transparent'],
                        'source': source.relative_to(ROOT).as_posix(),
                        'source_info': inspect(source),
                        'output': output.relative_to(ROOT).as_posix(),
                        'output_info': inspect(output)})
    manifest = {'schema': 1, 'generator': 'built-in image_gen',
                'reference': prompts['reference'], 'spec': 'launcher/artwork/asset-spec.json',
                'local_prompts': 'launcher/artwork/prompts.json',
                'processing': 'Preserved masters; crop transparent margins and proportionally resize exports only.',
                'assets': records}
    (ASSETS / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    preview()
    contact_sheet(prompts['assets'])
    print(f'Exported {len(records)} individual launcher assets and two review images.')


if __name__ == '__main__':
    main()
