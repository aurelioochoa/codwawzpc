#!/usr/bin/env python3
"""Check codwawzpc's translations against your copy of the game.

usage: review_translation.py [--game _main.dz] [--show] [german|italian|spanish ...]

Reads the UK English strings and their ids from the game's `_main.dz` (by
default the imported installation's), then checks each translation in
src/codwawzpc/localisation: every id in the game's order, the same printf
placeholders as English, and only characters the game's fonts can draw.
With --show it prints each English string above its translation, which is the
easiest way to review one. No game text is written anywhere.
"""
import argparse
import lzma
import os
import plistlib
import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANGUAGES = ('german', 'italian', 'spanish')
PLACEHOLDER = re.compile(r'%[-+ #0]*\d*(?:\.\d+)?[a-zA-Z%]')


def default_archive():
    base = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share') / 'codwawzpc'
    installation = plistlib.loads((base / 'current.plist').read_bytes())['Installation']
    return base / 'installations' / installation / 'Payload/kiwi.app/_main.dz'


def read_archive(path):
    """The files of a DTRZ archive whose chunks are LZMA, by path."""
    b = path.read_bytes()
    if b[:4] != b'DTRZ':
        raise ValueError(f'{path} is not a Marmalade archive')
    nfiles, ndirs = struct.unpack_from('<HH', b, 4)
    o, names = 9, []
    for _ in range(nfiles + ndirs - 1):
        end = b.index(b'\0', o)
        names.append(b[o:end].decode('latin-1'))
        o = end + 1
    files, dirs = names[:nfiles], [''] + names[nfiles:]
    mapping = []
    for _ in range(nfiles):
        directory, = struct.unpack_from('<H', b, o)
        o += 2
        chunks = []
        while (chunk := struct.unpack_from('<H', b, o)[0]) != 0xffff:
            chunks.append(chunk)
            o += 2
        o += 2
        mapping.append((directory, chunks))
    _, count = struct.unpack_from('<HH', b, o)
    table = [struct.unpack_from('<IIIHH', b, o + 4 + 16 * i) for i in range(count)]
    offsets = sorted(entry[0] for entry in table)

    def chunk(i):
        offset, _, size, flags, _ = table[i]
        if flags != 0x200:
            raise ValueError(f'chunk {i} is not LZMA')
        end = next((x for x in offsets if x > offset), len(b))
        return lzma.LZMADecompressor(lzma.FORMAT_ALONE).decompress(b[offset:end])[:size]

    return {f'{dirs[d]}/{name}'.lstrip('/').lower(): (lambda c=chunks: b''.join(map(chunk, c)))
            for name, (d, chunks) in zip(files, mapping)}


def game_strings(archive):
    """UK English strings and their ids, in the game's order."""
    files = read_archive(archive)
    hashes = files['localisation/localisehashes.dat']()
    n, = struct.unpack_from('<H', hashes, 4)
    ids = struct.unpack_from(f'<{n}I', hashes, 6)
    index = struct.unpack_from(f'<{n}H', hashes, 6 + 4 * n)
    by_index = dict(zip(index, ids))
    table = files['localisation/english_uk.dat']()
    count, = struct.unpack_from('<I', table, 12)
    offsets = struct.unpack_from(f'<{count}I', table, 16)
    base = table.index(b'\0', 16 + 4 * count) + 1
    strings = []
    for i, offset in enumerate(offsets):
        start = base + offset
        strings.append((by_index[i], table[start:table.index(b'\0', start)].decode()))
    return strings


def translation(name):
    entries = []
    path = ROOT / 'src/codwawzpc/localisation' / f'{name}.txt'
    for line in path.read_text(encoding='utf-8').splitlines():
        if line.startswith('#') or not line:
            continue
        ident, text = line.split('\t', 1)
        entries.append((int(ident, 16), re.sub(r'\\(.)', lambda m: {'n': '\n', '\\': '\\'}[m[1]], text)))
    return entries


def check(name, english, show):
    problems = []
    entries = translation(name)
    if len(entries) != len(english):
        problems.append(f'{len(entries)} strings instead of {len(english)}')
    for i, ((ident, en), (tid, text)) in enumerate(zip(english, entries)):
        where = f'{name} string {i} ({ident:08x})'
        if tid != ident:
            problems.append(f'{where}: id {tid:08x} is out of order')
        if PLACEHOLDER.findall(en) != PLACEHOLDER.findall(text):
            problems.append(f'{where}: placeholders {PLACEHOLDER.findall(text)} instead of {PLACEHOLDER.findall(en)}')
        missing = sorted({c for c in text if ord(c) > 0xff and c not in '•€'})
        if missing:
            problems.append(f'{where}: the game cannot draw {"".join(missing)!r}')
        if show:
            print(f'--- {i} {ident:08x}\n{en}\n>>>\n{text}')
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--game', type=Path, help='the game\'s _main.dz (default: the imported one)')
    parser.add_argument('--show', action='store_true', help='print English and translation side by side')
    parser.add_argument('languages', nargs='*', choices=LANGUAGES, default=LANGUAGES)
    args = parser.parse_args()
    try:
        english = game_strings(args.game or default_archive())
    except (OSError, KeyError, ValueError, lzma.LZMAError) as error:
        print(f'Cannot read the game\'s strings ({error!r}); import the game or pass --game',
              file=sys.stderr)
        return 2
    problems = [p for name in args.languages for p in check(name, english, args.show)]
    for problem in problems:
        print(problem, file=sys.stderr)
    print(f'{len(args.languages)} translations, {len(english)} strings each: '
          f'{"no problems" if not problems else f"{len(problems)} problems"}', file=sys.stderr)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
