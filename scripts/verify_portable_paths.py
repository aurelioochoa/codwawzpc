#!/usr/bin/env python3
"""Exercise portable host paths with a disposable installation and save."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / 'target/debug/codwawzpc'


def main():
    with tempfile.TemporaryDirectory(prefix='codwawzpc-portable-') as temp:
        base = Path(temp)
        install = base / 'original installation'
        install.mkdir()
        shutil.copy2(BINARY, install / 'codwawzpc')
        (install / '.codwawzpc-portable').touch()
        xdg = base / 'existing-xdg'
        xdg.mkdir()
        sentinel = xdg / 'sentinel'
        sentinel.write_text('unchanged')
        env = dict(os.environ, HOME='', XDG_DATA_HOME=str(xdg),
                   XDG_CONFIG_HOME=str(xdg), XDG_STATE_HOME=str(xdg))
        payload = bytes([0, 8, 1, 0]) + b'progress'
        save = base / 'save.i3d'
        save.write_bytes(zlib.adler32(payload).to_bytes(4, 'little') + payload)

        def run(root, *args):
            return subprocess.run([root / 'codwawzpc', *map(str, args)], cwd=base,
                                  env=env, capture_output=True, text=True, check=True)

        run(install, 'import-save', save)
        moved = base / 'moved installation'
        install.rename(moved)
        exported = base / 'export.i3d'
        run(moved, 'export-save', exported)
        assert exported.read_bytes() == save.read_bytes()
        assert list(xdg.iterdir()) == [sentinel] and sentinel.read_text() == 'unchanged'
    print('PORTABLE_PATHS_VERIFIED')


if __name__ == '__main__':
    main()
