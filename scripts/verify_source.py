#!/usr/bin/env python3
"""Audit the Git index or published history for private and non-source files.

This checks actual Git blobs, including image metadata. It complements manual
provenance review; it cannot infer the copyright owner of arbitrary bytes.
Untracked/ignored local files are never publication candidates.
"""
import argparse
import hashlib
from pathlib import PurePosixPath
import re
import subprocess


SUBMODULES = {'vendor/SDL', 'vendor/dynarmic', 'vendor/openal-soft', 'vendor/stb'}
DYLIBS = {f'touchHLE_dylibs/{name}.dylib' for name in (
    'libgcc_s.1', 'libsqlite3', 'libstdc++.6.0.9', 'libxml2.2', 'libz.1.2.3')}
TEST_BINARIES = {f'tests/TestApp.app/{name}' for name in (
    'test_fscanf', 'test_fscanf_new', 'test_ungetc')}
UPSTREAM_PLIST = 'tests/TestApp.app/Info.plist'
LIBXML2_SHA256 = '40607187ac35748e7aa0e515acd44c1cffbed5be3c0e05c51060e8b1d4cc697c'
IMAGE_ROOTS = ('launcher/assets/', 'launcher/artwork/sources/',
               'docs/launcher-concepts/', 'docs/launcher-assets/')
README_MEDIA = {f'docs/media/{name}' for name in (
    'banner.png', 'launcher.png', 'settings.png', 'achievements.png',
    'capture-settings.png', 'launcher.gif', 'achievement-browser.gif',
    'gameplay.gif', 'nacht-der-untoten.png', 'verruckt.png',
    'shi-no-numa.png', 'der-riese.png')}
IMAGES = README_MEDIA | {'res/icon.png', 'res/icon_preview.png', 'res/icon_unofficial.png',
          'tests/TestApp.app/test_1x1_black_pixel.jpg',
          'docs/launcher/achievement-icons-contact.png'}
PATTERNS = {
    'personal home path': rb'(?:/(?:home|Users)/[A-Za-z0-9_.-]+|[A-Za-z]:[\\/]+Users[\\/]+[A-Za-z0-9_.-]+)',
    'private key': rb'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----',
    'GitHub token': rb'(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})',
    'AWS access key': rb'(?:AKIA|ASIA)[A-Z0-9]{16}',
    # 10.77/24 is this project's virtual relay subnet, not a developer LAN.
    'private LAN address': rb'(?<![\d.])(?:192\.168\.\d{1,3}\.\d{1,3}|10\.(?!77\.)\d{1,3}\.\d{1,3}\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})(?![\d.])',
}


def git(*args):
    return subprocess.check_output(['git', *args])


def path_errors(name, mode):
    p = PurePosixPath(name)
    parts = p.parts
    errors = []
    if p.is_absolute() or '..' in parts or '\\' in name:
        errors.append('unsafe path')
    if mode == '160000':
        return [] if name in SUBMODULES else ['unreviewed submodule']
    if mode not in ('100644', '100755'):
        errors.append('non-regular file')
    private_dirs = {'.aws', '.agents', '.codex', '.unlazy', '__pycache__',
                    '.pytest_cache', 'target', 'Payload', 'touchHLE_apps',
                    'touchHLE_sandbox', 'installations', 'achievement-media',
                    'screenshots', 'recordings', 'save-backups', 'game', 'games', 'saves'}
    if private_dirs.intersection(parts):
        errors.append('private/runtime directory')
    if (p.name in {'AGENTS.md', 'CLAUDE.md', 'PLAN.md', 'DUMP.txt', 'touchHLE_log.txt'}
            or p.name.startswith(('GATES', '.env', 'codex-session-'))
            and p.name != '.env.example'
            or 'prompt' in p.name.lower() or p.name.endswith('.generation.json')
            or name.startswith('launcher/artwork/references/')):
        errors.append('private agent/reference file')
    if p.suffix.lower() in {'.ipa', '.ipsw', '.obb', '.dz', '.i3d', '.dat',
                            '.mp4', '.mov', '.mkv', '.webm', '.webp', '.bmp', '.pyc',
                            '.zip', '.exe', '.so', '.a'}:
        errors.append('game/archive/capture/build file')
    if any(x.endswith('.app') for x in parts) and not name.startswith('tests/TestApp.app/'):
        errors.append('extracted application')
    if p.suffix.lower() in {'.png', '.jpg', '.jpeg', '.ico', '.gif'}:
        if name not in IMAGES and not name.startswith(IMAGE_ROOTS):
            errors.append('unreviewed image or screenshot')
    if p.suffix == '.dylib' and name not in DYLIBS:
        errors.append('unreviewed dynamic library')
    return errors


def content_errors(data):
    return [label for label, pattern in PATTERNS.items() if re.search(pattern, data)]


def self_test():
    controls = [
        ('personal home path', b'/' + b'home' + b'/private-user/example.ipa'),
        ('personal home path', b'C:' + bytes([92]) + b'Users' + bytes([92]) + b'private-user'),
        ('private key', b'-----BEGIN ' + b'PRIVATE KEY-----'),
        ('GitHub token', b'gh' + b'p_' + b'x' * 36),
        ('AWS access key', b'AK' + b'IA' + b'X' * 16),
        ('private LAN address', b'192.' + b'168.7.8'),
    ]
    for label, data in controls:
        assert label in content_errors(data), label
    for name in ('Payload/kiwi.app/kiwi', 'game.ipa', 'game.dz', 'gameplay.bmp',
                 'docs/launcher/settings.png', 'recordings/sample.mp4',
                 'launcher/artwork/references/icon.png',
                 'docs/media/private-capture.png', 'docs/media/private-capture.gif', '.env'):
        assert path_errors(name, '100644'), name
    assert not content_errors(b'/path/to/game.ipa https://example.org 192.0.2.1')
    assert not path_errors('src/codwawzpc/runtime.rs', '100644')
    assert not path_errors('launcher/assets/title.png', '100644')
    assert not path_errors('docs/media/gameplay.gif', '100644')
    print('SOURCE_AUDIT_CONTROLS_PASS')


def entries(ref=None):
    command = ('ls-tree', '-rz', ref) if ref else ('ls-files', '-sz')
    for entry in git(*command).split(b'\0'):
        if not entry:
            continue
        header, raw_name = entry.split(b'\t', 1)
        mode, middle, last = header.decode().split()
        yield mode, last if ref else middle, raw_name.decode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--history', action='store_true', help='check every commit reachable from HEAD')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    self_test()
    if args.self_test:
        return
    refs = git('rev-list', 'HEAD').decode().splitlines() if args.history else [None]
    seen = set()
    failures = []
    count = 0
    for ref in refs:
        for mode, oid, name in entries(ref):
            count += 1
            failures.extend(f'{name}: {reason}' for reason in path_errors(name, mode))
            if mode == '160000' or oid in seen:
                continue
            seen.add(oid)
            data = git('cat-file', 'blob', oid)
            scan = data
            if name == 'touchHLE_dylibs/libxml2.2.dylib' and hashlib.sha256(data).hexdigest() == LIBXML2_SHA256:
                # This unchanged open-source library embeds public GitHub Actions
                # CI paths. Exempt only that prefix at this exact reviewed digest.
                prefix = b'/' + b'home' + b'/runner/work/libxml2-dylib/'
                scan = data.replace(prefix, b'/upstream-ci/')
            failures.extend(f'{name}: {reason}' for reason in content_errors(scan))
            # Binary payloads must have a reviewed purpose and license/provenance.
            p = PurePosixPath(name)
            if b'\0' in data and not (
                name in DYLIBS | TEST_BINARIES | IMAGES or name == UPSTREAM_PLIST
                or name.startswith(IMAGE_ROOTS) and p.suffix in {'.png', '.ico', '.wav'}
                or name.startswith('touchHLE_fonts/') and p.suffix in {'.ttf', '.otf'}
                or name == 'tests/TestApp.app/LiberationMono-Regular.ttf'
                or name == 'src/codwawzpc/lzma_sample.lzma'
            ):
                failures.append(f'{name}: unreviewed binary payload')
        if ref:
            identity = git('show', '-s', '--format=%ae%n%ce', ref).decode().splitlines()
            if not all(email.endswith('@users.noreply.github.com') for email in identity):
                failures.append(f'{ref[:12]}: commit uses a personal email address')
    if not count:
        failures.append('empty publication tree')
    if failures:
        raise SystemExit('\n'.join(sorted(set(failures))))
    print(f'SOURCE_AUDIT_PASS: {len(seen)} unique blobs; {len(refs)} tree(s); private-path/secret and binary-policy checks passed.')


if __name__ == '__main__':
    main()
