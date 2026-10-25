# SPDX-License-Identifier: MPL-2.0
"""Shared host preferences and bounded asynchronous CLI operations."""
from collections import deque
from dataclasses import dataclass
import math
import ipaddress
import getpass
import os
from pathlib import Path
import plistlib
import queue
import signal
import subprocess
import tempfile
import threading


@dataclass(frozen=True)
class Directories:
    data: Path
    config: Path
    logs: Path

    @classmethod
    def discover(cls):
        home = Path(os.environ.get('HOME', ''))
        if not home.is_absolute():
            raise ValueError('HOME must be set to an absolute directory')

        def base(key, fallback):
            value = Path(os.environ.get(key, ''))
            return (value if value.is_absolute() else home / fallback) / 'codwawzpc'

        return cls(base('XDG_DATA_HOME', '.local/share'),
                   base('XDG_CONFIG_HOME', '.config'),
                   base('XDG_STATE_HOME', '.local/state') / 'logs')

    @property
    def saves(self):
        return self.data / 'touchHLE_sandbox/com.activision.callofduty/Documents'


def default_player_name():
    try:
        import pwd
        name = pwd.getpwuid(os.geteuid()).pw_name
        if name.strip():
            return name
    except (ImportError, KeyError, OSError):
        pass
    try:
        name = getpass.getuser()
        if name.strip():
            return name
    except (KeyError, OSError):
        pass
    return 'Player'


DEFAULTS = {'Fullscreen': False, 'TouchMode': False, 'Sensitivity': 1.0,
            'InvertY': False, 'Volume': 1.0, 'FrameRate': 60, 'FieldOfView': 50,
            'AutoAim': 'off', 'AchievementScreenshots': True, 'AchievementClips': False,
            'AchievementScreenshotFormat': 'png', 'AchievementScreenshotResolution': 'native',
            'AchievementClipResolution': '1080p', 'AchievementClipFps': 60, 'AchievementClipQuality': 'high',
            'IntroLogos': False, 'Language': 'game',
            'CoopServer': '', 'PlayerName': default_player_name()}

AUTO_AIM = {'off': 'Off', 'low': 'Low', 'medium': 'Medium', 'high': 'High'}
# Each language by its own name, so it can be found whatever is shown.
LANGUAGES = {'game': "Game's choice", 'english': 'English', 'french': 'Français',
             'german': 'Deutsch', 'italian': 'Italiano', 'spanish': 'Español'}

CAPTURE_CHOICES = {
    'AchievementScreenshotFormat': {'png': 'PNG (lossless)', 'bmp': 'BMP (lossless)'},
    'AchievementScreenshotResolution': {'native': 'Full game window', '1080p': 'Up to 1080p', '720p': 'Up to 720p'},
    'AchievementClipResolution': {'1080p': 'Up to 1080p', '720p': 'Up to 720p', '540p': 'Up to 540p'},
    'AchievementClipQuality': {'standard': 'Standard', 'high': 'High', 'ultra': 'Ultra'},
}


def validate_coop_server(server):
    message = 'CoopServer must be HOST:PORT (port 1–65535), [IPv6]:PORT, or empty to disable co-op'
    if type(server) is not str:
        raise ValueError(message)
    if not server:
        return
    if any(c.isspace() or ord(c) < 32 or ord(c) == 127 for c in server):
        raise ValueError(message)
    if server.startswith('['):
        host, separator, port = server[1:].partition(']:')
        try:
            if not separator or '%' in host:
                raise ValueError(message)
            ipaddress.IPv6Address(host)
        except ValueError as error:
            raise ValueError(message) from error
    else:
        host, separator, port = server.rpartition(':')
        dns = host[:-1] if host.endswith('.') else host
        labels = dns.split('.')
        if (not separator or not dns or len(dns) > 253 or any(
                not label or len(label) > 63 or not label[0].isascii() or not label[0].isalnum() or
                not label[-1].isascii() or not label[-1].isalnum() or
                any(not (c.isascii() and (c.isalnum() or c == '-')) for c in label)
                for label in labels)):
            raise ValueError(message)
        if '.' in dns and all(c in '0123456789.' for c in dns):
            try:
                ipaddress.IPv4Address(dns)
            except ValueError as error:
                raise ValueError(message) from error
    if not port or not port.isascii() or not port.isdecimal() or not 1 <= int(port) <= 65535:
        raise ValueError(message)


def validate_settings(values):
    if not isinstance(values, dict) or values.keys() - DEFAULTS.keys():
        raise ValueError('Unknown host settings; expected a settings dictionary')
    result = {**DEFAULTS, **values}
    for name in ('Fullscreen', 'TouchMode', 'InvertY', 'AchievementScreenshots', 'AchievementClips',
                 'IntroLogos'):
        if type(result[name]) is not bool:
            raise ValueError(f'{name} must be a boolean')
    for name, low, high in [('Sensitivity', .05, 20), ('Volume', 0, 1)]:
        value = result[name]
        if type(value) is not float or not math.isfinite(value) or not low <= value <= high:
            raise ValueError(f'{name} must be a real number from {low} to {high}')
    if type(result['FrameRate']) is not int or result['FrameRate'] not in (45, 60):
        raise ValueError('FrameRate must be 45 or 60')
    if type(result['FieldOfView']) is not int or not 40 <= result['FieldOfView'] <= 100:
        raise ValueError('FieldOfView must be a whole number from 40 to 100')
    if type(result['AutoAim']) is not str or result['AutoAim'] not in AUTO_AIM:
        raise ValueError('AutoAim must be off, low, medium or high')
    if type(result['Language']) is not str or result['Language'] not in LANGUAGES:
        raise ValueError('Language must be game, english, french, german, italian or spanish')
    for key, choices in CAPTURE_CHOICES.items():
        if type(result[key]) is not str or result[key] not in choices:
            raise ValueError(f'{key} must be one of {", ".join(choices)}')
    if type(result['AchievementClipFps']) is not int or result['AchievementClipFps'] not in (30, 60):
        raise ValueError('AchievementClipFps must be 30 or 60')
    validate_coop_server(result['CoopServer'])
    if type(result['PlayerName']) is not str or not result['PlayerName'].strip():
        raise ValueError('PlayerName must not be empty')
    return result


def load_settings(dirs):
    path = dirs.config / 'settings.plist'
    if not path.exists():
        return dict(DEFAULTS)
    try:
        return validate_settings(plistlib.loads(path.read_bytes()))
    except Exception as error:
        raise ValueError(f'Cannot read {path}: {error}') from error


def save_settings(dirs, values):
    values = validate_settings(values)
    dirs.config.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.launcher-', dir=dirs.config)
    try:
        with os.fdopen(fd, 'wb') as stream:
            plistlib.dump(values, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, dirs.config / 'settings.plist')
    finally:
        if os.path.exists(name):
            os.unlink(name)


def tail_log(path, limit=65536):
    try:
        with path.open('rb') as stream:
            stream.seek(0, os.SEEK_END)
            stream.seek(max(0, stream.tell() - limit))
            return stream.read(limit).decode('utf-8', errors='replace')
    except FileNotFoundError:
        return 'No runtime log yet.'


class Child:
    """Read on a worker thread. A noisy or partial line never blocks Tk."""
    def __init__(self, command):
        self.process = subprocess.Popen([str(x) for x in command],
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                        start_new_session=True, bufsize=0)
        self.output = queue.Queue(maxsize=128)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        import codecs
        decoder = codecs.getincrementaldecoder('utf-8')(errors='replace')
        try:
            while chunk := self.process.stdout.read(4096):
                self._put(decoder.decode(chunk))
            self._put(decoder.decode(b'', final=True))
        finally:
            self.process.stdout.close()

    def _put(self, chunk):
        if not chunk:
            return
        try:
            self.output.put_nowait(chunk)
        except queue.Full:
            try:
                self.output.get_nowait()
            except queue.Empty:
                pass
            self.output.put_nowait(chunk)

    def poll(self):
        chunks = []
        while True:
            try:
                chunks.append(self.output.get_nowait())
            except queue.Empty:
                break
        code = self.process.poll()
        return ''.join(chunks), code if code is not None and not self.reader.is_alive() and self.output.empty() else None

    def stop(self, force=False):
        try:
            os.killpg(self.process.pid, signal.SIGKILL if force else signal.SIGTERM)
        except ProcessLookupError:
            pass


# Host commands the launcher runs, with their busy and failure statuses.
OPERATIONS = {'check': 'Checking installed files…', 'import': 'Importing game…', 'run': 'Game running',
              'export-save': 'Exporting saved game…', 'import-save': 'Importing saved game…'}
FAILURES = {'check': 'Installation needs attention', 'import': 'Import failed',
            'run': 'Game exited with an error', 'export-save': 'Save export failed',
            'import-save': 'Save import failed'}


class Session:
    def __init__(self, binary, dirs):
        self.binary, self.dirs = Path(binary), dirs
        self.child = None
        self.operation = None
        self.verified = False
        self.status = 'Import your game to begin'
        self.history = deque(maxlen=128)
        self.recent = deque(maxlen=8)
        self.last_output = ''

    @property
    def candidate(self):
        return (self.dirs.data / 'current.plist').is_file()

    def start(self, operation, *arguments):
        if self.child is not None:
            return False
        if operation not in OPERATIONS:
            raise ValueError(f'Unsupported operation: {operation}')
        if operation == 'run' and not self.verified:
            return False
        child = Child([self.binary, operation, *arguments])
        self.child, self.operation = child, operation
        self.history.clear()
        self.last_output = ''
        self.status = OPERATIONS[operation]
        return True

    def tick(self):
        if not self.child:
            return None
        chunk, code = self.child.poll()
        if chunk:
            self.history.append(chunk[-8192:])
        if code is None:
            return None
        operation = self.operation
        self.last_output = ''.join(self.history)[-65536:]
        self.recent.append(f'{operation} (exit {code})\n{self.last_output[-8192:]}')
        self.child, self.operation = None, None
        if operation in ('check', 'import'):
            self.verified = code == 0
        if code:
            self.status = FAILURES[operation]
        else:
            self.status = 'Ready to play' if self.verified else 'Import your game to begin'
        return operation, code, self.last_output
