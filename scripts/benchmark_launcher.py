#!/usr/bin/env python3
"""Measure native startup in fresh processes using disposable launcher state."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def sample():
    sys.path.insert(0, str(ROOT))
    import tkinter as tk
    from launcher.backend import Directories
    from launcher.ui import Launcher

    with tempfile.TemporaryDirectory(prefix='codwawzpc-benchmark-') as temporary:
        for key in ('XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'XDG_STATE_HOME'):
            os.environ[key] = temporary
        started = time.perf_counter()
        window = tk.Tk()
        window.overrideredirect(True)
        view = Launcher(window, Path(temporary) / 'missing-host', Directories.discover())
        window.geometry('1440x844+0+0')
        constructed = time.perf_counter()
        try:
            while not hasattr(view, 'background'):
                window.update()
                time.sleep(.001)
            painted = time.perf_counter()
            first_hash = hashlib.sha256(view.background.tobytes()).hexdigest()
            repeated = time.perf_counter()
            for _ in range(3):
                view.render()
            repeated_ms = (time.perf_counter() - repeated) * 1000 / 3
            assert hashlib.sha256(view.background.tobytes()).hexdigest() == first_hash
            return {'construct_ms': (constructed - started) * 1000,
                    'first_scene_ms': (painted - started) * 1000,
                    'repeat_render_ms': repeated_ms,
                    'scene_size': view.background.size, 'scene_sha256': first_hash}
        finally:
            view.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(sample()))
        return
    if args.runs < 1:
        parser.error('--runs must be positive')
    samples = [json.loads(subprocess.check_output(
        [sys.executable, str(Path(__file__).resolve()), '--worker'], text=True))
        for _ in range(args.runs)]
    assert len({s['scene_sha256'] for s in samples}) == 1
    print(json.dumps({'runs': samples, 'median_ms': {
        key: round(statistics.median(s[key] for s in samples), 2)
        for key in ('construct_ms', 'first_scene_ms', 'repeat_render_ms')}}, indent=2))


if __name__ == '__main__':
    main()
