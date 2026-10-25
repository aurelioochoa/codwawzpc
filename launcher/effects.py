# SPDX-License-Identifier: MPL-2.0
"""Small cached light sprites and time-based canvas animation for the hut."""
import math
import random

from PIL import Image, ImageDraw, ImageFilter, ImageTk


def glow(size, color, strength=1):
    """Soft elliptical light with no opaque edge."""
    width, height = size
    image = Image.new('RGBA', size)
    draw = ImageDraw.Draw(image)
    for step in range(64, 0, -1):
        radius = step / 64
        alpha = min(255, round(100 * strength * (1 - radius) ** 1.7))
        draw.ellipse((width * (1 - radius) / 2, height * (1 - radius) / 2,
                      width * (1 + radius) / 2, height * (1 + radius) / 2), fill=(*color, alpha))
    return image.filter(ImageFilter.GaussianBlur(max(1, width / 60)))


def status_lens(source, color):
    """Tint the glass only; preserve the brass bezel and its highlights."""
    image = source.copy()
    pixels = image.load()
    cx, cy = image.width * .5, image.height * .51
    for y in range(image.height):
        for x in range(image.width):
            distance = math.hypot((x - cx) / image.width, (y - cy) / image.height)
            if distance < .31:
                r, g, b, a = pixels[x, y]
                light = max(r, g, b) / 255
                white = max(0, (min(r, g, b) / 255 - .35) / .65)
                mix = min(1, (.31 - distance) / .045)
                tinted = [round((channel * light) * (1 - white) + 255 * white) for channel in color]
                pixels[x, y] = tuple(round(old * (1 - mix) + new * mix)
                                      for old, new in zip((r, g, b), tinted)) + (a,)
    return image


def discharge_frame(size, frame, scale):
    """Layer blurred corona, jagged channels and hairline forks."""
    rng = random.Random(935 + frame)
    w, h = size
    core = Image.new('RGBA', size)
    draw = ImageDraw.Draw(core)
    routes = [((.421, .205), (.488, .193)), ((.49, .191), (.557, .241)),
              ((.805, .486), (.967, .505))]
    for route, (start, end) in enumerate(routes):
        points = []
        count = 9 if route < 2 else 18
        offsets = [0]
        for _ in range(1, count - 1):
            offsets.append(offsets[-1] * .55 + rng.uniform(-1, 1) * (.014 if route < 2 else .027))
        offsets.append(0)
        for i in range(count):
            t = i / (count - 1)
            x = (start[0] + (end[0] - start[0]) * t) * w
            y = (start[1] + (end[1] - start[1]) * t + offsets[i]) * h
            points.append((x, y))
        draw.line(points, fill=(108, 202, 255, 210), width=max(1, round(3 * scale)))
        draw.line(points, fill=(238, 251, 255, 255), width=max(1, round(1.2 * scale)))
        for branch in range(2 if route == 2 else 1):
            origin = points[rng.randrange(2, count - 2)]
            tip = (origin[0] + rng.uniform(-.028, .035) * w,
                   origin[1] + rng.choice((-1, 1)) * rng.uniform(.025, .075) * h)
            middle = ((origin[0] + tip[0]) / 2 + rng.uniform(-4, 4) * scale,
                      (origin[1] + tip[1]) / 2)
            draw.line((origin, middle, tip), fill=(147, 222, 255, 160), width=max(1, round(scale)))
    halo = core.filter(ImageFilter.GaussianBlur(max(2, round(8 * scale))))
    halo.putalpha(halo.getchannel('A').point(lambda a: round(a * .62)))
    return Image.alpha_composite(halo, core)


def strike_phase(elapsed):
    """Unevenly spaced capacitor fires, each with a short secondary arc."""
    cycle = elapsed % 34.8
    for start in (5.9, 14.1, 20.9, 30.4):
        phase = cycle - start
        if 0 <= phase < .14 or .19 <= phase < .32:
            return phase
    return None


class MonkeySequence:
    """Occasional wind-up beats, with a fresh quiet interval after each one."""
    def __init__(self):
        self.next_at = random.uniform(120, 180)
        self.started_at = -math.inf

    def active(self, elapsed, motion):
        if not motion:
            return False
        if elapsed >= self.next_at:
            self.started_at = elapsed
            self.next_at = elapsed + random.uniform(120, 180)
        return 0 <= elapsed - self.started_at < 1.2


class CachedFrames:
    """Materialize full-quality animation frames on first use, once per size."""
    def __init__(self, count, make):
        self.count, self.make = count, make
        self.frames = {}

    def __getitem__(self, index):
        if not 0 <= index < self.count:
            raise IndexError(index)
        if index not in self.frames:
            self.frames[index] = self.make(index)
        return self.frames[index]


class SceneEffects:
    def __init__(self, canvas, art):
        self.canvas, self.art = canvas, art
        self.frames = {}
        self.items = {}
        self.rain = []
        self.scale = None
        self.monkey_beat = False
        self.monkey_sequence = MonkeySequence()

    def point(self, x, y):
        return self.ox + x * self.scale, self.oy + y * self.scale

    def warm_next(self):
        for frames in self.frames.values():
            for index in range(frames.count):
                if index not in frames.frames:
                    frames[index]
                    return True
        return False

    def sprite(self, name, center, size, color, levels=12):
        size = tuple(max(2, round(value * self.scale)) for value in size)
        if name not in self.frames:
            self.frames[name] = CachedFrames(levels, lambda i: ImageTk.PhotoImage(
                glow(size, color, .35 + i / levels)))
        self.items[name] = self.canvas.create_image(*self.point(*center), tags='effects')

    def rebuild(self, scale, ox, oy):
        self.canvas.delete('effects')
        if scale != self.scale:
            self.frames.clear()
        self.items.clear()
        self.rain.clear()
        self.scale, self.ox, self.oy = scale, ox, oy
        # Atmosphere is below the foreground artwork: mist never washes out
        # the title, weapon, lantern or the radio's controls.
        for index in range(3):
            self.sprite(f'mist{index}', (930 + index * 160, 390 + index * 65),
                        (660, 105), (151, 180, 160), levels=1)
            self.canvas.itemconfigure(self.items[f'mist{index}'], image=self.frames[f'mist{index}'][0])
            self.canvas.tag_lower(self.items[f'mist{index}'], 'foreground')
        rng = random.Random(115)
        for _ in range(26):
            x, y, speed = rng.uniform(770, 1420), rng.uniform(215, 635), rng.uniform(80, 140)
            item = self.canvas.create_line(0, 0, 0, 0, fill='#a3b7aa', stipple='gray25', tags='effects')
            self.canvas.tag_lower(item, 'foreground')
            self.rain.append((item, x, y, speed))
        lx, ly, lw, lh = self.art.lantern
        self.sprite('lantern-halo', (lx + lw * .51, ly + lh * .59), (290, 330), (255, 146, 43))
        self.sprite('lantern-flame', (lx + lw * .51, ly + lh * .64), (66, 112), (255, 200, 88))
        self.sprite('lantern-spill', (lx + lw * .52, ly + lh + 4), (260, 30), (255, 157, 49))
        wx, wy, ww, wh = self.art.weapon
        for index, (u, v) in enumerate(((.428, .194), (.489, .194), (.551, .235), (.838, .49))):
            self.sprite(f'coil{index}', (wx + ww * u, wy + wh * v),
                        (75, 100) if index < 3 else (155, 60), (63, 178, 255))
        # Discharges and full monkey swings may be minutes away. Build their
        # original frames on first use, rather than blocking the first scene.
        arc_size = (max(2, round(ww * self.scale)), max(2, round(wh * self.scale)))
        if 'arcs' not in self.frames:
            self.frames['arcs'] = CachedFrames(16, lambda frame: ImageTk.PhotoImage(
                discharge_frame(arc_size, frame, scale)))
        self.items['arcs'] = self.canvas.create_image(*self.point(wx, wy), anchor='nw', tags='effects', state='hidden')
        self.sprite('muzzle-flash', (wx + ww * .946, wy + wh * .506),
                    (115, 83), (150, 223, 255))
        mx, my, mw, mh = self.art.monkey
        # Cache padded rotation frames once per resize, never resample per tick.
        toy = self.art.resized('monkey-bomb', max(1, round(mw * scale)), max(1, round(mh * scale)))
        pad = max(3, round(18 * scale))
        padded = Image.new('RGBA', (toy.width + pad * 2, toy.height + pad * 2))
        padded.alpha_composite(toy, (pad, pad))
        if 'monkey' not in self.frames:
            self.frames['monkey'] = CachedFrames(17, lambda frame: ImageTk.PhotoImage(
                padded.rotate((frame - 8) / 2, Image.Resampling.BICUBIC)))
        self.items['monkey'] = self.canvas.create_image(*self.point(mx + mw / 2, my + mh / 2),
                                                        image=self.frames['monkey'][8], tags='effects')
        self.sprite('cymbal-glint', (mx + mw * .63, my + mh * .60), (60, 45), (255, 206, 116))
        self.update(0, False)

    def update(self, elapsed, motion=True):
        if not self.items:
            return False
        t = elapsed if motion else 0
        flicker = math.sin(t * 7.3) * .13 + math.sin(t * 13.7) * .08 + math.sin(t * 2.1) * .2
        level = max(0, min(11, round(6 + flicker * 10)))
        for name in ('lantern-halo', 'lantern-flame', 'lantern-spill'):
            self.canvas.itemconfigure(self.items[name], image=self.frames[name][level])
        for index in range(3):
            self.canvas.coords(self.items[f'mist{index}'], *self.point(
                930 + index * 160 + math.sin(t * .12 + index * 2) * 95,
                390 + index * 65 + math.sin(t * .19 + index) * 12))
        for item, x, y, speed in self.rain:
            top = 215 + (y - 215 + t * speed) % 420
            self.canvas.coords(item, *self.point(x, top), *self.point(x - 3, top + 13))
        phase = strike_phase(t) if motion else None
        discharge = phase is not None
        for index in range(4):
            level = 11 if discharge else round(4 + 3 * math.sin(t * 2.8 + index * .7))
            self.canvas.itemconfigure(self.items[f'coil{index}'], image=self.frames[f'coil{index}'][level])
        self.canvas.itemconfigure(self.items['arcs'], state='normal' if discharge else 'hidden')
        self.canvas.itemconfigure(self.items['muzzle-flash'], state='normal' if discharge else 'hidden')
        if discharge:
            self.canvas.itemconfigure(self.items['arcs'], image=self.frames['arcs'][int(t * 38) % 16])
            self.canvas.itemconfigure(self.items['muzzle-flash'],
                                      image=self.frames['muzzle-flash'][11 if phase < .14 else 8])
        self.monkey_beat = self.monkey_sequence.active(elapsed, motion)
        monkey_t = elapsed - self.monkey_sequence.started_at if self.monkey_beat else 0
        rocking = math.sin(monkey_t * 15) * 4 if self.monkey_beat else math.sin(t * 1.8) * .8
        mx, my, mw, mh = self.art.monkey
        bob = abs(math.sin(monkey_t * 15)) * 3 if self.monkey_beat else 0
        self.canvas.coords(self.items['monkey'], *self.point(mx + mw / 2, my + mh / 2 - bob))
        self.canvas.itemconfigure(self.items['monkey'], image=self.frames['monkey'][round(rocking * 2) + 8])
        self.canvas.coords(self.items['cymbal-glint'], *self.point(mx + mw * .63, my + mh * .60 - bob))
        self.canvas.itemconfigure(self.items['cymbal-glint'], state='normal' if self.monkey_beat else 'hidden')
        if self.monkey_beat:
            self.canvas.itemconfigure(self.items['cymbal-glint'],
                                      image=self.frames['cymbal-glint'][round(abs(math.sin(monkey_t * 15)) * 11)])
        return discharge
