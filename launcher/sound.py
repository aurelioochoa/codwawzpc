# SPDX-License-Identifier: MPL-2.0
"""Original electrical and wind-up toy SFX; optional system audio."""
import array
import math
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
import wave


def synthesize(kind, volume=1):
    rate = 22050
    duration = {'discharge': .72, 'switch': .09, 'monkey': 1.2}[kind]
    rng = random.Random(935)
    samples = array.array('h')
    filtered = 0
    for index in range(round(rate * duration)):
        t = index / rate
        noise = rng.uniform(-1, 1)
        filtered = filtered * .7 + noise * .3
        if kind == 'discharge':
            envelope = min(1, t / .018) * (1 - t / duration) ** 1.8
            buzz = math.sin(math.tau * (105 * t + 90 * t * t))
            crackle = (noise - filtered) * (.25 + .75 * max(0, math.sin(t * 137)))
            value = envelope * (.24 * buzz + .65 * crackle)
        elif kind == 'monkey':
            # A short clockwork melody with three inharmonic brass clashes.
            # These are original synthesized sounds, not extracted game audio.
            beat = int(t / .24)
            note_time = t % .24
            note = (659.25, 783.99, 739.99, 587.33, 659.25)[min(beat, 4)]
            melody = math.exp(-note_time * 16) * (math.sin(math.tau * note * t)
                      + .28 * math.sin(math.tau * note * 2.01 * t))
            strike = t % .40
            brass = math.exp(-strike * 14) * sum(math.sin(math.tau * f * t)
                    for f in (1137, 1631, 2279, 3067)) / 4
            rattle = math.exp(-(t % .08) * 120) * noise
            value = min(1, t / .006) * min(1, (duration - t) / .1) * (
                .34 * melody + .48 * brass + .10 * rattle)
        else:
            value = math.exp(-t * 65) * (noise * .5 + math.sin(math.tau * 180 * t) * .3)
        samples.append(round(max(-1, min(1, value * volume * .35)) * 32767))
    if sys.byteorder != 'little':
        samples.byteswap()
    return samples.tobytes()


class SoundPlayer:
    def __init__(self, volume):
        self.player = next((path for name in ('paplay', 'pw-play', 'aplay', 'afplay')
                            if (path := shutil.which(name))), None)
        self.enabled = self.player is not None
        self.process = None
        self.temporary = tempfile.TemporaryDirectory(prefix='codwawzpc-sfx-')
        self.prepared = set()
        self.set_volume(volume)

    def set_volume(self, volume):
        if getattr(self, 'volume', None) == volume:
            return
        self.volume = volume
        self.prepared.clear()

    def prepare(self, kind):
        """Synthesize each unchanged, full-quality sound only when it is used."""
        if kind not in self.prepared:
            with wave.open(str(Path(self.temporary.name) / f'{kind}.wav'), 'wb') as audio:
                audio.setparams((1, 2, 22050, 0, 'NONE', 'not compressed'))
                audio.writeframes(synthesize(kind, self.volume))
            self.prepared.add(kind)

    def play(self, kind='discharge'):
        if not self.enabled or not self.volume or not self.player:
            return
        if self.process and self.process.poll() is None:
            return
        try:
            self.prepare(kind)
            self.process = subprocess.Popen([self.player, str(Path(self.temporary.name) / f'{kind}.wav')],
                                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            self.enabled = False

    def stop(self):
        if self.process:
            if self.process.poll() is None:
                self.process.terminate()
            try:
                self.process.wait(timeout=.3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None

    def close(self):
        self.stop()
        self.temporary.cleanup()
