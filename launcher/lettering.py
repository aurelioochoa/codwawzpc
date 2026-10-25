# SPDX-License-Identifier: MPL-2.0
"""Optically centered, chipped paint lettering for the main radio controls."""
from functools import lru_cache
import math
import random

from PIL import Image, ImageChops, ImageColor, ImageDraw, ImageFont


@lru_cache(maxsize=32)
def paint_font(size):
    # Pillow searches the platform's font directories by filename. The final
    # fallback is bundled with Pillow, so missing desktop fonts never block UI.
    for name in ('NimbusSansNarrow-Bold.otf', 'DejaVuSansCondensed-Bold.ttf',
                 'FreeSansBold.otf', 'arialbd.ttf', 'Arial Bold.ttf'):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    try:
        return ImageFont.load_default(size=size)
    except TypeError:  # Pillow 10.0 has only a fixed-size default font.
        return ImageFont.load_default()


@lru_cache(maxsize=64)
def painted_text(text, size, max_width, max_height, color):
    """Crop to visible ink before centering; font baselines add uneven space."""
    antialias = 2
    font = paint_font(size * antialias)
    spacing = max(2, round(size * .08 * antialias))
    measure = ImageDraw.Draw(Image.new('L', (1, 1)))
    bounds = measure.multiline_textbbox((0, 0), text, font=font, spacing=spacing, align='center')
    bounds = (math.floor(bounds[0]), math.floor(bounds[1]), math.ceil(bounds[2]), math.ceil(bounds[3]))
    mask = Image.new('L', (max(1, bounds[2] - bounds[0]), max(1, bounds[3] - bounds[1])))
    ImageDraw.Draw(mask).multiline_text((-bounds[0], -bounds[1]), text, font=font,
                                       fill=255, spacing=spacing, align='center')
    ratio = min(1 / antialias, max_width / mask.width, max_height / mask.height)
    mask = mask.resize((max(1, round(mask.width * ratio)), max(1, round(mask.height * ratio))), Image.Resampling.LANCZOS)
    mask = mask.crop(mask.getbbox())
    width, height = mask.size
    # The same seeded wear survives hover, focus and press. Chips reveal the
    # actual plate beneath the letters instead of drawing dark dots on top.
    rng = random.Random(text)
    wear = Image.new('L', mask.size)
    draw = ImageDraw.Draw(wear)
    unit = max(.5, height / (70 if '\n' not in text else 48))
    for _ in range(max(8, round(width * height * .009))):
        x, y = rng.randrange(width), rng.randrange(height)
        radius = rng.uniform(.25, .85) * unit
        draw.ellipse((x, y, x + radius * 2, y + radius), fill=rng.randrange(100, 255))
    for _ in range(max(3, width // 27)):
        x, y = rng.randrange(width), rng.randrange(height)
        draw.line((x, y, x + rng.uniform(3, 12) * unit, y - rng.uniform(0, 2) * unit),
                  fill=rng.randrange(130, 230), width=max(1, round(unit * .65)))
    mask = ImageChops.subtract(mask, wear)
    ink = Image.new('RGBA', mask.size)
    pixels = ink.load()
    red, green, blue = ImageColor.getrgb(color)
    for y in range(height):
        for x in range(width):
            # Uneven old pigment, softly darkened along the lower edge.
            shade = .90 + .10 * (1 - y / max(1, height - 1)) + rng.uniform(-.045, .025)
            pixels[x, y] = (round(red * shade), round(green * shade), round(blue * shade), 255)
    ink.putalpha(mask)
    return ink
