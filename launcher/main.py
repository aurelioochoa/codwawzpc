#!/usr/bin/env python3
"""Native entry point; paths are independent of the working directory."""
import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
if __package__ in (None, ''):
    sys.path.insert(0, str(ROOT))


def main():
    parser = argparse.ArgumentParser(description='codwawzpc — Shi No Numa launcher')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/codwawzpc')
    args = parser.parse_args()
    try:
        import tkinter as tk
        from launcher.backend import Directories
        from launcher.ui import Launcher
    except ImportError as error:
        parser.exit(1, f'Launcher dependency unavailable: {error}. Install Python Tk and Pillow; see launcher/README.md.\n')
    try:
        window = tk.Tk(className='Codwawzpc')
        Launcher(window, args.binary.resolve(), Directories.discover())
        window.mainloop()
    except (OSError, ValueError, tk.TclError) as error:
        parser.exit(1, f'Cannot open launcher: {error}\n')


if __name__ == '__main__':
    main()
