#!/usr/bin/env python3
"""Exercise a portable folder in isolation. Optional CODWAWZPC_TEST_IPA imports a real IPA."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zlib


def main():
    if len(sys.argv) != 2:
        raise SystemExit('Usage: verify_portable_release.py <portable folder>')
    source = Path(sys.argv[1]).resolve(strict=True)
    if (source / 'userdata').exists():
        raise SystemExit('Release folder is not fresh: userdata already exists')
    with tempfile.TemporaryDirectory(prefix='codwawzpc-release-check-') as tmp:
        temp = Path(tmp)
        root = temp / 'first install'
        shutil.copytree(source, root)
        xdg = temp / 'xdg'
        xdg.mkdir()
        (xdg / 'sentinel').write_text('unchanged')
        env = dict(os.environ, HOME=str(temp), XDG_DATA_HOME=str(xdg),
                   XDG_CONFIG_HOME=str(xdg), XDG_STATE_HOME=str(xdg))
        def environment(folder):
            return dict(env, LD_LIBRARY_PATH=str(folder / 'runtime/lib'),
                        PYTHONHOME=str(folder / 'runtime'),
                        PYTHONPATH=str(folder / 'runtime/lib/python3.10/site-packages'),
                        TCL_LIBRARY=str(folder / 'runtime/share/tcltk/tcl8.6'),
                        TK_LIBRARY=str(folder / 'runtime/share/tcltk/tk8.6'),
                        FONTCONFIG_FILE=str(folder / 'runtime/etc/fonts/fonts.conf'),
                        FONTCONFIG_PATH=str(folder / 'runtime/etc/fonts'), PYTHONNOUSERSITE='1')
        def run(folder, executable, *args, ok=True):
            process = subprocess.run([str(folder / executable), *map(str, args)], cwd=folder,
                                     env=environment(folder), capture_output=True, text=True)
            assert (process.returncode == 0) == ok, (executable, args, process.stdout, process.stderr)
            return process.stdout + process.stderr
        assert 'No game has been imported' in run(root, 'codwawzpc', 'check', ok=False)
        assert 'Cannot open' in run(root, 'codwawzpc', 'import', temp / 'missing.ipa', ok=False)
        invalid = temp / 'invalid.ipa'
        invalid.write_bytes(b'not a zip archive')
        assert 'Invalid IPA' in run(root, 'codwawzpc', 'import', invalid, ok=False)
        assert 'No game has been imported' in run(root, 'codwawzpc', 'check', ok=False)
        run(root, 'runtime/bin/python3.10', '-c', 'import tkinter; from PIL import Image, ImageTk; assert Image.__version__.startswith("10.")')
        assert 'libx264' in run(root, 'runtime/bin/ffmpeg', '-hide_banner', '-encoders')
        payload = bytes([0, 8, 1, 0]) + b'portable fixture'
        save = temp / 'save.i3d'
        save.write_bytes(zlib.adler32(payload).to_bytes(4, 'little') + payload)
        run(root, 'codwawzpc', 'import-save', save)
        ipa = os.environ.get('CODWAWZPC_TEST_IPA')
        if ipa:
            assert 'Imported Call of Duty' in run(root, 'codwawzpc', 'import', Path(ipa).resolve())
            assert 'Installation verified' in run(root, 'codwawzpc', 'check')
        moved = temp / 'moved install'
        root.rename(moved)
        exported = temp / 'export.i3d'
        run(moved, 'codwawzpc', 'export-save', exported)
        assert exported.read_bytes() == save.read_bytes()
        if ipa:
            assert 'Installation verified' in run(moved, 'codwawzpc', 'check')
        assert list(xdg.iterdir()) == [xdg / 'sentinel']
        assert (xdg / 'sentinel').read_text() == 'unchanged'
    print('PORTABLE_RELEASE_VERIFY_PASS' + (' (IPA imported)' if ipa else ' (no IPA supplied)'))


if __name__ == '__main__':
    main()
