# SPDX-License-Identifier: MPL-2.0
"""Register this checkout in the current user's Linux applications menu."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from .backend import Directories
from .icons import ASSETS, ICONS, load_icon

ROOT = Path(__file__).resolve().parents[1]
MARKER = 'X-Codwawzpc-Managed=true'


def desktop_path(dirs):
    return dirs.data.parent / 'applications' / 'codwawzpc.desktop'


def desktop_value(value):
    return str(value).replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


def exec_argument(value):
    # Exec uses desktop-entry quoting, not a shell. Escape its reserved bytes,
    # then the string-value layer; percent signs must escape field expansion.
    value = str(value).replace('%', '%%')
    for char in ('\\', '"', '`', '$'):
        value = value.replace(char, '\\' + char)
    return desktop_value('"' + value + '"')


def write_entry(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.codwawzpc-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def update_icon(dirs, icon):
    """Touch only an entry installed by us; GNOME watches this directory."""
    if icon not in ICONS:
        raise ValueError('Choose an available launcher icon')
    path = desktop_path(dirs)
    if not path.exists():
        return
    lines = path.read_text().splitlines()
    if MARKER not in lines:
        return
    icon_line = 'Icon=' + desktop_value(ASSETS / f'app-icon-{icon}.png')
    write_entry(path, '\n'.join(icon_line if line.startswith('Icon=') else line for line in lines) + '\n')


def install(dirs, binary):
    binary = Path(binary).resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f'Host executable is not runnable: {binary}')
    icon = load_icon(dirs)
    command = ' '.join(exec_argument(arg) for arg in
                       (sys.executable, ROOT / 'launcher/main.py', '--binary', binary))
    entry = '\n'.join([
        '[Desktop Entry]', 'Version=1.0', 'Type=Application',
        'Name=Call of Duty: Zombies', 'GenericName=Zombies game launcher',
        'Comment=Launch the World at War Zombies iPhone PC port',
        'Exec=' + command, 'Icon=' + desktop_value(ASSETS / f'app-icon-{icon}.png'),
        'Terminal=false', 'Categories=Game;ActionGame;',
        'Keywords=codwawzpc;Call of Duty;World at War;Zombies;',
        'StartupWMClass=Codwawzpc', 'StartupNotify=false', MARKER, '',
    ])
    path = desktop_path(dirs)
    write_entry(path, entry)
    updater = shutil.which('update-desktop-database')
    if updater:
        subprocess.run([updater, str(path.parent)], check=True)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/codwawzpc')
    args = parser.parse_args()
    try:
        print(f'Installed: {install(Directories.discover(), args.binary)}')
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Cannot install desktop entry: {error}\n')


if __name__ == '__main__':
    main()
