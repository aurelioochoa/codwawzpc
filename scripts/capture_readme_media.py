#!/usr/bin/env python3
"""Capture the real launcher with disposable demo state; never opens a game.

Requires a display, Pillow, ImageMagick's import command and FFmpeg. The
installation-ready flag is explicitly synthetic. No personal saves, logs,
installation paths or gameplay captures are read.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import tkinter as tk
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from launcher.backend import DEFAULTS, Directories, save_settings
from launcher.ui import Launcher


class NoGamepads:
    pads = {}

    def poll(self):
        return [], 0.

    def close(self):
        pass


def settle(window, seconds=.15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        window.update()
        time.sleep(.01)


def capture(window, target):
    window.lift()
    window.update()
    subprocess.run(['import', '-window', str(window.winfo_id()), str(target)],
                   check=True, timeout=20, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def encode_gif(pattern, target, fps, width):
    subprocess.run([
        'ffmpeg', '-y', '-v', 'error', '-framerate', str(fps), '-i', str(pattern),
        '-filter_complex',
        f'[0:v]scale={width}:-2:flags=lanczos,split[a][b];'
        '[a]palettegen=max_colors=128:stats_mode=diff[p];'
        '[b][p]paletteuse=dither=bayer:bayer_scale=3',
        '-map_metadata', '-1', '-loop', '0', str(target),
    ], check=True, timeout=120)


def prepare_manual(manual):
    manual.dialog.overrideredirect(True)
    manual.dialog.geometry('1120x740+0+0')
    settle(manual.dialog)
    manual.page.redraw()
    settle(manual.dialog)


def main():
    output = ROOT / 'docs/media'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='codwawzpc-readme-') as temporary:
        work = Path(temporary)
        env = {'XDG_DATA_HOME': str(work / 'data'),
               'XDG_CONFIG_HOME': str(work / 'config'),
               'XDG_STATE_HOME': str(work / 'state')}
        with patch.dict(os.environ, env):
            dirs = Directories.discover()
            save_settings(dirs, {**DEFAULTS, 'PlayerName': 'Demo Player'})
            window = tk.Tk()
            window.overrideredirect(True)
            view = Launcher(window, work / 'nonexistent-demo-host', dirs, NoGamepads())
            try:
                window.geometry('1280x750+0+0')
                settle(window, 1.5)
                window.after_cancel(view.animation_job)
                view.animation_job = None
                view.sound.enabled = False
                view.session.verified = True
                view.session.status = 'Ready to play'
                view.refresh_state()
                settle(window)
                view.effects.update(6.12)
                capture(window, output / 'launcher.png')
                # Native effects advance at normal speed. No fabricated motion
                # or game footage is composited into this real window capture.
                for frame in range(48):
                    view.effects.update(5.5 + frame / 12)
                    capture(window, work / f'launcher-{frame:03d}.png')
                encode_gif(work / 'launcher-%03d.png', output / 'launcher.gif', 12, 960)

                view.show_settings()
                manual = view.settings_manual
                prepare_manual(manual)
                capture(manual.dialog, output / 'settings.png')
                manual.nav_buttons[3].invoke()
                settle(manual.dialog)
                capture(manual.dialog, output / 'capture-settings.png')
                manual.dialog.destroy()
                settle(window)

                view.show_achievements()
                manual = view.achievements_manual
                prepare_manual(manual)
                for frame, button_index in enumerate((1, 2, 3, 4, 5, 1)):
                    manual.nav_buttons[button_index].invoke()
                    settle(manual.dialog, .25)
                    capture(manual.dialog, work / f'browser-{frame:03d}.png')
                    if frame == 3:
                        capture(manual.dialog, output / 'achievements.png')
                encode_gif(work / 'browser-%03d.png', output / 'achievement-browser.gif', .75, 1008)
                assert view.session.child is None and not dirs.saves.exists()
            finally:
                view.destroy()
    print('README_LAUNCHER_MEDIA_CAPTURED: four real UI screenshots and two GIFs; synthetic installation state; no game or personal data.')


if __name__ == '__main__':
    main()
