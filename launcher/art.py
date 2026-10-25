# SPDX-License-Identifier: MPL-2.0
"""Compose the environment and measure control plates without stretching art."""
from functools import lru_cache
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageOps

ASSETS = Path(__file__).resolve().parent / 'assets'
SIZE = (1672, 941)


def load_artwork(name):
    """Decode independent PNGs concurrently, retaining the original RGBA pixels."""
    with Image.open(ASSETS / f'{name}.png') as source:
        im = source.convert('RGBA')
    if name != 'swamp-background':
        box = im.getchannel('A').point(lambda x: 255 if x >= 8 else 0).getbbox()
        if box is None:
            raise ValueError(f'Artwork {name} is empty')
        pad = max(2, round(max(im.size) * .005))
        im = im.crop((max(0, box[0] - pad), max(0, box[1] - pad),
                      min(im.width, box[2] + pad), min(im.height, box[3] + pad)))
    return im


class Artwork:
    def __init__(self):
        names = ('swamp-background', 'title', 'lantern', 'wunderwaffe',
                 'radio-housing', 'play-plate', 'utility-plate', 'status-lamp',
                 'icon-settings', 'icon-saves', 'icon-achievements', 'monkey-bomb',
                 'maintenance-rail', 'rail-switches', 'rail-button')
        # Pillow releases the GIL while decoding. Keep Tk objects on the UI
        # thread, and bound the pool so startup cannot flood slower machines.
        with ThreadPoolExecutor(max_workers=4, thread_name_prefix='launcher-art') as pool:
            self.images = dict(zip(names, pool.map(load_artwork, names)))
        self.foreground = Image.new('RGBA', SIZE)
        self.put(self.foreground, 'title', (55, 55, 690, 350))
        # Its footing follows the shelf in the cover-scaled background, while
        # its size follows the foreground. Compute its final position on resize.
        self.lantern_home = self.bounds('lantern', (1430, 138, 165, 330))
        self.lantern = self.lantern_home
        self.put(self.foreground, 'radio-housing', (12, 590, 1648, 345))
        self.weapon = self.put(self.foreground, 'wunderwaffe', (0, 365, 1160, 315), left=True)
        # Separate animated toy, seated on the case between muzzle and lantern.
        self.monkey = self.bounds('monkey-bomb', (1200, 420, 235, 238))
        mx, my, mw, mh = self.monkey
        shadow = Image.new('RGBA', SIZE)
        ImageDraw.Draw(shadow).ellipse((mx + mw * .08, my + mh - 12,
                                        mx + mw * .94, my + mh + 4), fill=(8, 5, 2, 180))
        self.foreground.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(5)))
        self.controls = {'play': (222, 706, 420, 153)}
        for index, name in enumerate(('settings', 'saves', 'achievements')):
            self.controls[name] = (665 + index * 244, 714, 226, 138)

    @lru_cache(maxsize=32)
    def plate(self, name, width, height):
        """Nine-slice blank metal plates, keeping their rivets proportional."""
        source = self.images[name]
        edge = round(min(source.size) * .25)
        dest_edge = max(1, min(round(edge * width / source.width), height // 3, width // 3))
        result = Image.new('RGBA', (width, height))
        sx, sy = (0, edge, source.width - edge, source.width), (0, edge, source.height - edge, source.height)
        dx, dy = (0, dest_edge, width - dest_edge, width), (0, dest_edge, height - dest_edge, height)
        for col in range(3):
            for row in range(3):
                tile = source.crop((sx[col], sy[row], sx[col + 1], sy[row + 1]))
                tile = tile.resize((dx[col + 1] - dx[col], dy[row + 1] - dy[row]), Image.Resampling.LANCZOS)
                result.alpha_composite(tile, (dx[col], dy[row]))
        return result

    def bounds(self, name, box):
        x, y, w, h = box
        im = self.images[name]
        scale = min(w / im.width, h / im.height)
        width, height = round(im.width * scale), round(im.height * scale)
        return round(x + (w - width) / 2), round(y + (h - height) / 2), width, height

    @lru_cache(maxsize=128)
    def resized(self, name, width, height):
        return self.images[name].resize((max(1, width), max(1, height)), Image.Resampling.LANCZOS)

    @lru_cache(maxsize=24)
    def rail(self, width, height):
        """Stretch only the quiet middle, preserving generated endcaps."""
        source = self.images['maintenance-rail']
        edge = min(round(source.height * .62), source.width // 5)
        dest = max(12, round(edge * height / source.height))
        result = Image.new('RGBA', (width, height), '#161510')
        for start, stop, target, span in (
            (0, edge, 0, dest),
            (edge, source.width - edge, dest, width - 2 * dest),
            (source.width - edge, source.width, width - dest, dest),
        ):
            if span > 0:
                result.alpha_composite(source.crop((start, 0, stop, source.height)).resize(
                    (span, height), Image.Resampling.LANCZOS), (target, 0))
        return result

    def put(self, canvas, name, box, left=False):
        x, y, w, h = self.bounds(name, box)
        if left:
            x = round(box[0])
        canvas.alpha_composite(self.resized(name, w, h), (x, y))
        return x, y, w, h

    def render(self, width, height):
        scale = min(width / SIZE[0], height / SIZE[1])
        sw, sh = round(SIZE[0] * scale), round(SIZE[1] * scale)
        ox, oy = (width - sw) // 2, (height - sh) // 2
        canvas = ImageOps.fit(self.images['swamp-background'], (width, height), method=Image.Resampling.LANCZOS)
        # One cover transform for every pixel: no second inset copy, even at
        # ultrawide or tall sizes. Feather blur and shade at the outer edges.
        edge = max(8, round(min(width, height) * .055))
        center = Image.new('L', canvas.size)
        ImageDraw.Draw(center).rectangle((edge, edge, width - edge, height - edge), fill=255)
        center = center.filter(ImageFilter.GaussianBlur(edge))
        canvas = Image.composite(canvas, canvas.filter(ImageFilter.GaussianBlur(max(2, edge / 5))), center)
        shade = center.point(lambda value: round((255 - value) * .30))
        overlay = Image.new('RGBA', canvas.size, (5, 9, 7, 0))
        overlay.putalpha(shade)
        canvas = Image.alpha_composite(canvas, overlay)
        foreground = Image.new('RGBA', canvas.size)
        foreground.alpha_composite(self.foreground.resize((sw, sh), Image.Resampling.LANCZOS), (ox, oy))
        cover = max(width / SIZE[0], height / SIZE[1])
        # Shelf top is y=475 in the source. Sink the foot seven source pixels
        # into that surface so the visible alpha edge makes firm contact.
        shelf_y = (height - SIZE[1] * cover) / 2 + 482 * cover
        lx, _, lw, lh = self.lantern_home
        lamp_w, lamp_h = max(1, round(lw * scale)), max(1, round(lh * scale))
        lamp_x, lamp_y = round(ox + lx * scale), round(shelf_y) - lamp_h
        shadow = Image.new('RGBA', canvas.size)
        ImageDraw.Draw(shadow).ellipse((lamp_x + lamp_w * .08, shelf_y - 5 * scale,
                                        lamp_x + lamp_w * .96, shelf_y + 3 * scale), fill=(12, 8, 3, 185))
        foreground.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(max(1, 3 * scale))))
        foreground.alpha_composite(self.resized('lantern', lamp_w, lamp_h), (lamp_x, lamp_y))
        self.lantern = ((lamp_x - ox) / scale, (lamp_y - oy) / scale,
                        lamp_w / scale, lamp_h / scale)
        return canvas, foreground, scale, ox, oy
