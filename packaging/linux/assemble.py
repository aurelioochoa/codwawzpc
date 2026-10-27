#!/usr/bin/env python3
"""Assemble an allowlisted portable tree inside the Ubuntu build container."""
import glob
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

src, target, out = map(Path, sys.argv[1:])
if out.exists():
    raise SystemExit(f'Refusing to overwrite {out}')
out.mkdir(parents=True)
runtime = out / 'runtime'
for name in ('bin', 'lib', 'share'):
    (runtime / name).mkdir(parents=True)
shutil.copy2(target / 'release/codwawzpc', out / 'codwawzpc')
(out / '.codwawzpc-portable').touch()
for name in ('LICENSE', 'NOTICE.md', 'README.md', 'OPTIONS_HELP.txt'):
    shutil.copy2(src / name, out / name)
launcher = out / 'launcher'
launcher.mkdir()
for path in (src / 'launcher').glob('*.py'):
    shutil.copy2(path, launcher / path.name)
shutil.copytree(src / 'launcher/assets', launcher / 'assets', ignore=shutil.ignore_patterns('__pycache__', '*.pyc'))
resources = out / 'resources'
resources.mkdir()
for name in ('touchHLE_dylibs', 'touchHLE_fonts'):
    shutil.copytree(src / name, resources / name)
shutil.copy2(src / 'touchHLE_default_options.txt', resources)
shutil.copytree('/usr/lib/python3.10', runtime / 'lib/python3.10',
                ignore=shutil.ignore_patterns('__pycache__', '*.pyc', 'test', 'tests', 'ensurepip'))
for name in ('PIL', 'pillow.libs', 'pillow-10.4.0.dist-info'):
    shutil.copytree(Path('/opt/pillow') / name, runtime / 'lib/python3.10/site-packages' / name,
                    ignore=shutil.ignore_patterns('__pycache__', '*.pyc', 'tests'))
shutil.copytree('/usr/share/tcltk', runtime / 'share/tcltk')
shutil.copytree('/usr/share/fonts/truetype/dejavu', runtime / 'share/fonts/dejavu')
(runtime / 'etc/fonts').mkdir(parents=True)
shutil.copy2(src / 'packaging/linux/fonts.conf', runtime / 'etc/fonts/fonts.conf')
for name in ('python3.10', 'ffmpeg', 'ffprobe', 'paplay'):
    shutil.copy2('/usr/bin/' + name, runtime / 'bin' / name)

# Copy the ELF dependencies of the host, bundled tools and Python extension
# modules, as well as libraries SDL loads dynamically. Leave glibc and graphics
# drivers to the supported host so the driver's ABI stays compatible.
exclude = re.compile(r'^(ld-linux|libc\.so|libm\.so|libdl\.so|libpthread\.so|librt\.so|libresolv\.so|libstdc\+\+\.so|libgcc_s\.so|libGL|libEGL|libOpenGL|libdrm|libgbm)')
files = [out / 'codwawzpc', *list((runtime / 'bin').iterdir()),
         *list((runtime / 'lib/python3.10').rglob('*.so'))]
for pattern in ('libasound.so.*', 'libpulse.so.*', 'libudev.so.*', 'libwayland-client.so.*',
                'libX11.so.*', 'libXcursor.so.*', 'libXi.so.*', 'libXrandr.so.*', 'libXinerama.so.*'):
    files.extend(Path(p) for p in glob.glob('/usr/lib/x86_64-linux-gnu/' + pattern))
seen = set()
while files:
    path = files.pop()
    try:
        output = subprocess.run(['ldd', str(path)], capture_output=True, text=True).stdout
    except OSError:
        continue
    for line in output.splitlines():
        if 'not found' in line:
            raise SystemExit(f'Missing dependency for {path}: {line}')
        match = re.search(r'^\s*(\S+)\s+=>\s+(/\S+)', line)
        if not match:
            continue
        name, source = match.groups()
        # The pinned Pillow wheel already carries its private libraries in
        # pillow.libs; copying those would break their $ORIGIN dependencies.
        if 'pillow.libs' in Path(source).resolve().parts:
            continue
        if name in seen or exclude.match(name):
            continue
        seen.add(name)
        destination = runtime / 'lib' / name
        shutil.copy2(source, destination, follow_symlinks=True)
        files.append(destination)
notices = out / 'licenses' / 'ubuntu'
notices.mkdir(parents=True)
subprocess.run([str(out / 'codwawzpc'), '--copyright'], check=True,
               stdout=(out / 'licenses/host-dependencies.txt').open('w'))
for path in Path('/usr/share/doc').glob('*/copyright'):
    shutil.copy2(path, notices / (path.parent.name + '.txt'))
subprocess.run(['dpkg-query', '-W', '-f=${Package} ${Version}\n'], check=True,
               stdout=(out / 'licenses/packages.txt').open('w'))
shutil.copy2(src / 'packaging/linux/Launch', out / 'Launch')
shutil.copy2(src / 'packaging/linux/PORTABLE.md', out / 'PORTABLE.md')
print(f'Assembled {out} with {len(seen)} shared libraries')
