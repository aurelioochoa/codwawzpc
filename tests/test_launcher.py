#!/usr/bin/env python3
"""Backend contracts; all writes and children use disposable directories."""
import math
import os
from pathlib import Path
import plistlib
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import wave
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from launcher.backend import (CAPTURE_CHOICES, Child, DEFAULTS, Directories, Session, load_settings,
                              save_settings, tail_log, validate_settings)
from launcher.sound import SoundPlayer, synthesize
from launcher.effects import CachedFrames, MonkeySequence, strike_phase
from launcher.icons import DEFAULT_ICON, ICONS, load_icon, save_icon
from launcher.achievements import load_achievements
from launcher import gamepad
from launcher.gamepad import Gamepads, has_bit
from launcher.navigation import best, score


class BackendTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='codwawzpc-launcher-unit-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.dirs = Directories(self.base / 'data', self.base / 'config', self.base / 'logs')

    def wait_child(self, child):
        output = ''
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            text, code = child.poll()
            output += text
            if code is not None:
                return output, code
            time.sleep(.01)
        child.stop(force=True)
        self.fail('Child did not finish')

    def test_xdg_matches_host(self):
        with patch.dict(os.environ, {'HOME': str(self.base), 'XDG_DATA_HOME': 'relative',
                                     'XDG_CONFIG_HOME': str(self.base / 'settings'), 'XDG_STATE_HOME': ''}):
            dirs = Directories.discover()
            self.assertEqual(dirs.data, self.base / '.local/share/codwawzpc')
            self.assertEqual(dirs.config, self.base / 'settings/codwawzpc')
            self.assertEqual(dirs.logs, self.base / '.local/state/codwawzpc/logs')
            self.assertTrue(str(dirs.saves).endswith('touchHLE_sandbox/com.activision.callofduty/Documents'))

    def test_portable_paths_follow_folder_and_ignore_xdg(self):
        import launcher.backend as backend
        first = self.base / 'first'
        (first / 'launcher').mkdir(parents=True)
        (first / '.codwawzpc-portable').touch()
        with patch.object(backend, '__file__', str(first / 'launcher/backend.py')):
            with patch.dict(os.environ, {'HOME': '', 'XDG_DATA_HOME': str(self.base / 'xdg')}):
                dirs = Directories.discover()
                self.assertEqual(dirs.data, first / 'userdata/data')
                save_settings(dirs, {**DEFAULTS, 'Fullscreen': True})
        moved = self.base / 'moved'
        first.rename(moved)
        with patch.object(backend, '__file__', str(moved / 'launcher/backend.py')):
            dirs = Directories.discover()
            self.assertEqual(dirs.logs, moved / 'userdata/logs')
            self.assertTrue(load_settings(dirs)['Fullscreen'])
        self.assertFalse((self.base / 'xdg').exists())

    def test_portable_desktop_entry_uses_launcher_and_follows_relocation(self):
        from launcher import desktop
        root = self.base / 'portable one'
        root.mkdir()
        (root / '.codwawzpc-portable').touch()
        host = root / 'codwawzpc'
        host.write_text('#!/bin/sh\n')
        host.chmod(0o755)
        (root / 'Launch').touch()
        with patch.object(desktop, 'ROOT', root), patch.dict(os.environ, {
                'HOME': str(self.base), 'XDG_DATA_HOME': str(self.base / 'desktop-data')}), \
                patch.object(desktop.shutil, 'which', return_value=None):
            path = desktop.install(self.dirs, host)
            self.assertEqual(path.name, 'codwawzpc-portable.desktop')
            self.assertIn(str(root / 'Launch'), path.read_text())
        moved = self.base / 'portable two'
        root.rename(moved)
        with patch.object(desktop, 'ROOT', moved), patch.dict(os.environ, {
                'HOME': str(self.base), 'XDG_DATA_HOME': str(self.base / 'desktop-data')}), \
                patch.object(desktop.shutil, 'which', return_value=None):
            desktop.install(self.dirs, moved / 'codwawzpc')
            self.assertIn(str(moved / 'Launch'), path.read_text())

    def test_controls_validation_conflicts_and_persistence(self):
        from launcher import controls
        values = dict(controls.DEFAULTS)
        values['Reload'] = 'Key:T,Pad:RightTrigger'
        values['Fire'] = 'Mouse:Left,Pad:X'
        values['StickPreset'] = 'swapped'
        controls.save(self.dirs, values)
        self.assertEqual(controls.load(self.dirs), values)
        before = (self.dirs.config / 'controls.plist').read_bytes()
        values['Reload'] = 'Key:W'
        with self.assertRaisesRegex(ValueError, 'Conflict'):
            controls.save(self.dirs, values)
        self.assertEqual((self.dirs.config / 'controls.plist').read_bytes(), before)
        values['Reload'] = 'Key:Escape'
        with self.assertRaisesRegex(ValueError, 'Invalid input'):
            controls.validate(values)

    def test_missing_defaults_and_roundtrip(self):
        self.assertEqual(load_settings(self.dirs), DEFAULTS)
        values = {**DEFAULTS, 'Sensitivity': 2.75, 'Volume': .35, 'Fullscreen': True,
                  'InvertY': True, 'FrameRate': 45, 'FieldOfView': 90, 'AutoAim': 'medium',
                  'IntroLogos': True, 'Language': 'german'}
        save_settings(self.dirs, values)
        self.assertEqual(load_settings(self.dirs), values)
        self.assertEqual(plistlib.loads((self.dirs.config / 'settings.plist').read_bytes()), values)
        self.assertEqual(list(self.dirs.config.iterdir()), [self.dirs.config / 'settings.plist'])

    def test_sparse_host_settings(self):
        self.dirs.config.mkdir()
        (self.dirs.config / 'settings.plist').write_bytes(plistlib.dumps({'Fullscreen': True}))
        self.assertEqual(load_settings(self.dirs), {**DEFAULTS, 'Fullscreen': True})

    def test_capture_settings_roundtrip_and_invalid_values_preserve_previous(self):
        self.assertEqual(DEFAULTS['AchievementClipFps'], 60)
        self.assertEqual(DEFAULTS['AchievementClipResolution'], '1080p')
        self.assertEqual(DEFAULTS['AchievementScreenshotResolution'], 'native')
        for key, choices in CAPTURE_CHOICES.items():
            for choice in choices:
                values = {**DEFAULTS, key: choice, 'AchievementClipFps': 30}
                save_settings(self.dirs, values)
                self.assertEqual(load_settings(self.dirs), values)
        before = (self.dirs.config / 'settings.plist').read_bytes()
        invalid = [(key, value) for key in CAPTURE_CHOICES for value in ('unknown', '', 1, True)]
        invalid += [('AchievementClipFps', value) for value in (8, 24, 59, 120, 60.0, True, '60')]
        for key, value in invalid:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                save_settings(self.dirs, {**DEFAULTS, key: value})
            self.assertEqual((self.dirs.config / 'settings.plist').read_bytes(), before)

    def test_coop_settings_roundtrip_and_disabled_default(self):
        self.assertEqual(DEFAULTS['CoopServer'], '')
        self.assertTrue(DEFAULTS['PlayerName'].strip())
        for server in ('', 'localhost:27960', 'relay.example.org.:443', '127.0.0.1:1',
                       '[::1]:27960', '[2001:db8::1]:65535'):
            with self.subTest(server=server):
                values = {**DEFAULTS, 'CoopServer': server, 'PlayerName': 'Joueur 日本語'}
                save_settings(self.dirs, values)
                self.assertEqual(load_settings(self.dirs), values)

    def test_coop_settings_reject_invalid_values_without_overwriting(self):
        save_settings(self.dirs, DEFAULTS)
        before = (self.dirs.config / 'settings.plist').read_bytes()
        invalid_servers = ('host', ':27960', 'host:', 'host:0', 'host:65536', 'host:-1',
                           'host:+1', 'host:１２', 'host:port', 'host:1:2', '::1:27960',
                           '[::1]', '[::1]:0', '[invalid]:27960', '[::1]extra:80',
                           ' host:27960', 'host:27960 ', 'bad host:27960',
                           'https://host:27960', 'host/path:80', 'host@relay:80',
                           'host\\relay:80', 'host\n:80', '.:80', '-host:80', 'host-:80',
                           'relay..example:80', 'a' * 64 + ':80', '999.999.999.999:80',
                           '[fe80::1%eth0]:80', 27960, None)
        for values in ([{'CoopServer': server} for server in invalid_servers] +
                       [{'PlayerName': name} for name in ('', '  ', '\n', 4, None)]):
            with self.subTest(values=values), self.assertRaises(ValueError):
                save_settings(self.dirs, {**DEFAULTS, **values})
            self.assertEqual((self.dirs.config / 'settings.plist').read_bytes(), before)

    def test_invalid_values_match_rust_schema(self):
        for values in ([], {'Other': True}, {'Volume': math.nan}, {'Volume': math.inf},
                       {'Volume': 1}, {'Volume': 1.1}, {'Sensitivity': .049},
                       {'Sensitivity': 20.01}, {'Sensitivity': -math.inf},
                       {'FrameRate': 30}, {'FrameRate': 60.0}, {'FrameRate': True},
                       {'FieldOfView': 39}, {'FieldOfView': 101}, {'FieldOfView': 75.0},
                       {'FieldOfView': True}, {'AutoAim': 'on'}, {'AutoAim': 'Off'}, {'AutoAim': 'Medium'},
                       {'AutoAim': False},
                       {'Fullscreen': 1}, {'TouchMode': 'yes'}, {'InvertY': 0},
                       {'AchievementScreenshots': 1}, {'AchievementClips': 'yes'},
                       {'IntroLogos': 'on'}, {'Language': 'German'}, {'Language': 'deutsch'},
                       {'Language': ''}, {'Language': 4}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                validate_settings(values)

    def test_corrupt_settings_reported(self):
        self.dirs.config.mkdir()
        (self.dirs.config / 'settings.plist').write_text('bad settings')
        with self.assertRaisesRegex(ValueError, 'Cannot read'):
            load_settings(self.dirs)

    def test_failed_atomic_save_preserves_previous(self):
        save_settings(self.dirs, DEFAULTS)
        before = (self.dirs.config / 'settings.plist').read_bytes()
        with patch('launcher.backend.os.replace', side_effect=OSError('write failed')):
            with self.assertRaises(OSError):
                save_settings(self.dirs, {**DEFAULTS, 'Fullscreen': True})
        self.assertEqual((self.dirs.config / 'settings.plist').read_bytes(), before)
        self.assertEqual(len(list(self.dirs.config.iterdir())), 1)

    def test_log_tail_is_bounded(self):
        self.assertEqual(tail_log(self.base / 'missing'), 'No runtime log yet.')
        log = self.base / 'log'
        log.write_bytes(b'x' * 100000 + b'final')
        self.assertEqual(tail_log(log, 100), 'x' * 95 + 'final')

    def test_child_no_shell_and_failure_output(self):
        marker = self.base / 'unsafe'
        argument = f'; touch {marker} $(echo unsafe)'
        child = Child([sys.executable, '-c', 'import sys; print(sys.argv[1]); sys.exit(7)', argument])
        output, code = self.wait_child(child)
        self.assertEqual(code, 7)
        self.assertIn(argument, output)
        self.assertFalse(marker.exists())

    def test_noisy_output_does_not_block(self):
        child = Child([sys.executable, '-c', 'import sys; sys.stdout.write("x"*3000000); print("END")'])
        child.reader.join(5)
        self.assertFalse(child.reader.is_alive())
        self.assertLessEqual(child.output.qsize(), 128)
        output, code = self.wait_child(child)
        self.assertEqual(code, 0)
        self.assertTrue(output.endswith('END\n'))

    def test_stop_reaps_process_group(self):
        child = Child([sys.executable, '-c', 'import time; time.sleep(30)'])
        child.stop()
        _, code = self.wait_child(child)
        self.assertNotEqual(code, 0)

    def test_session_blocks_unverified_and_overlapping_runs(self):
        session = Session(sys.executable, self.dirs)
        self.assertFalse(session.start('run'))
        with patch('launcher.backend.Child') as factory:
            self.assertTrue(session.start('check'))
            self.assertFalse(session.start('import', 'game.ipa'))
            factory.return_value.poll.return_value = ('Installation verified', 0)
            self.assertEqual(session.tick(), ('check', 0, 'Installation verified'))
            self.assertTrue(session.verified)
            session.start('run')
            factory.return_value.poll.return_value = ('run failed', 2)
            self.assertEqual(session.tick(), ('run', 2, 'run failed'))
            self.assertTrue(session.verified)

    def test_save_transfers_use_host_and_keep_verification(self):
        session = Session(sys.executable, self.dirs)
        session.verified = True
        with patch('launcher.backend.Child') as factory:
            self.assertTrue(session.start('export-save', '/tmp/out.i3d'))
            factory.assert_called_with([Path(sys.executable), 'export-save', '/tmp/out.i3d'])
            self.assertEqual(session.status, 'Exporting saved game…')
            self.assertFalse(session.start('run'))
            factory.return_value.poll.return_value = ('Exported saved game', 0)
            self.assertEqual(session.tick(), ('export-save', 0, 'Exported saved game'))
            self.assertTrue(session.verified)
            self.assertEqual(session.status, 'Ready to play')
            self.assertTrue(session.start('import-save', 'save.i3d'))
            factory.return_value.poll.return_value = ('Not a valid COD Zombies save', 1)
            self.assertEqual(session.tick(), ('import-save', 1, 'Not a valid COD Zombies save'))
            self.assertTrue(session.verified)
            self.assertEqual(session.status, 'Save import failed')

    def test_spawn_failure_does_not_leave_busy_state(self):
        session = Session(self.base / 'missing', self.dirs)
        with self.assertRaises(OSError):
            session.start('check')
        self.assertIsNone(session.child)
        self.assertIsNone(session.operation)


class LauncherIconTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='codwawzpc-icons-unit-')
        self.addCleanup(temporary.cleanup)
        base = Path(temporary.name)
        self.dirs = Directories(base / 'data', base / 'config', base / 'logs')

    def test_all_icons_roundtrip_without_changing_host_settings(self):
        self.assertEqual(load_icon(self.dirs), DEFAULT_ICON)
        save_settings(self.dirs, {**DEFAULTS, 'Volume': .35})
        host = (self.dirs.config / 'settings.plist').read_bytes()
        for icon in ICONS:
            save_icon(self.dirs, icon)
            self.assertEqual(load_icon(self.dirs), icon)
            self.assertEqual((self.dirs.config / 'settings.plist').read_bytes(), host)
        self.assertEqual({p.name for p in self.dirs.config.iterdir()}, {'launcher.plist', 'settings.plist'})

    def test_corrupt_and_unknown_preferences_fall_back(self):
        self.dirs.config.mkdir()
        path = self.dirs.config / 'launcher.plist'
        invalid = [b'broken', b'<?xml version="1.0"?><plist><dict>']
        invalid += [plistlib.dumps(value) for value in ({'Icon': '../bad'}, {'Icon': []},
                                                      {'Icon': 1}, [], {}, 'undead')]
        for value in invalid:
            with self.subTest(value=value):
                path.write_bytes(value)
                self.assertEqual(load_icon(self.dirs), DEFAULT_ICON)

    def test_invalid_selection_never_creates_config(self):
        for icon in ('../bad', 'unknown', None, [], 1):
            with self.subTest(icon=icon), self.assertRaises(ValueError):
                save_icon(self.dirs, icon)
        self.assertFalse(self.dirs.config.exists())

    def test_failed_save_preserves_previous_selection_and_cleans_temp(self):
        save_icon(self.dirs, 'undead')
        before = (self.dirs.config / 'launcher.plist').read_bytes()
        with patch('launcher.icons.os.replace', side_effect=OSError('read-only config')):
            with self.assertRaises(OSError):
                save_icon(self.dirs, 'field-radio')
        self.assertEqual((self.dirs.config / 'launcher.plist').read_bytes(), before)
        self.assertEqual(list(self.dirs.config.iterdir()), [self.dirs.config / 'launcher.plist'])


class AchievementReportTests(unittest.TestCase):
    def test_catalog_and_unlocks(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            rows, message = load_achievements(base)
            self.assertEqual((len(rows), sum(row['Points'] for row in rows)), (106, 105050))
            self.assertFalse(any(row['Unlocked'] for row in rows))
            self.assertIn('Regular mode', message)
            entries = [{**{key: row[key] for key in ('Id', 'Name', 'Description', 'Points', 'Map')},
                        'Unlocked': row['Id'] == 2,
                        **({'UnlockedAt': 1720000000} if row['Id'] == 2 else {})}
                       for row in rows]
            (base / 'achievements.plist').write_bytes(plistlib.dumps(
                {'Format': 1, 'Achievements': entries}))
            loaded, _ = load_achievements(base)
            self.assertEqual([row['Id'] for row in loaded if row['Unlocked']], [2])
            self.assertEqual(loaded[1]['UnlockedAt'], 1720000000)
            (base / 'achievements.plist').write_bytes(b'broken plist')
            fallback, issue = load_achievements(base)
            self.assertFalse(any(row['Unlocked'] for row in fallback))
            self.assertIn('Could not read', issue)


class MonkeyTimingTests(unittest.TestCase):
    def test_random_intervals_are_quiet_between_beats_and_skip_catchup(self):
        with patch('launcher.effects.random.uniform', side_effect=[120, 180, 150]) as interval:
            sequence = MonkeySequence()
            self.assertFalse(sequence.active(119.9, True))
            self.assertFalse(sequence.active(120, False))
            self.assertEqual(sequence.next_at, 120)
            self.assertTrue(sequence.active(120, True))
            self.assertTrue(sequence.active(120.5, True))
            self.assertEqual(sequence.next_at, 300)
            self.assertFalse(sequence.active(121.3, True))
            self.assertFalse(sequence.active(299.9, True))
            # A long inactive period starts one new beat, with no catch-up burst.
            self.assertTrue(sequence.active(400, True))
            self.assertEqual(sequence.next_at, 550)
            self.assertTrue(sequence.active(400.5, True))
            self.assertFalse(sequence.active(401.3, True))
            self.assertEqual(interval.call_args_list, [unittest.mock.call(120, 180)] * 3)


class WeaponTimingTests(unittest.TestCase):
    def test_strikes_are_brief_and_unevenly_spaced(self):
        self.assertIsNone(strike_phase(5.89))
        self.assertIsNotNone(strike_phase(5.95))
        self.assertIsNone(strike_phase(6.06))
        self.assertIsNotNone(strike_phase(6.12))
        self.assertIsNone(strike_phase(6.23))
        starts = [5.9, 14.1, 20.9, 30.4, 40.7]
        self.assertEqual(len({round(b - a, 1) for a, b in zip(starts, starts[1:])}), 4)
        for start in starts:
            self.assertIsNotNone(strike_phase(start + .02))


class SoundTests(unittest.TestCase):
    def test_pcm_is_audible_bounded_and_respects_mute(self):
        import array
        for kind in ('switch', 'discharge', 'monkey'):
            samples = array.array('h', synthesize(kind, .7))
            self.assertGreater(max(samples), 1000)
            self.assertLess(max(abs(value) for value in samples), 32767)
            self.assertEqual(set(synthesize(kind, 0)), {0})

    def test_playback_is_nonblocking_and_close_reaps_audio(self):
        with patch('launcher.sound.shutil.which', return_value='/usr/bin/paplay'), \
             patch('launcher.sound.subprocess.Popen') as spawn:
            sound = SoundPlayer(.4)
            directory = Path(sound.temporary.name)
            try:
                self.assertEqual(list(directory.iterdir()), [])
                sound.prepare('discharge')
                with wave.open(str(directory / 'discharge.wav')) as audio:
                    self.assertEqual((audio.getnchannels(), audio.getsampwidth(), audio.getframerate()), (1, 2, 22050))
                    self.assertGreater(audio.getnframes(), 15000)
                    self.assertEqual(audio.readframes(audio.getnframes()), synthesize('discharge', .4))
                sound.prepare('monkey')
                with wave.open(str(directory / 'monkey.wav')) as audio:
                    self.assertEqual(audio.getnframes(), 26460)
                sound.enabled = False
                sound.play()
                spawn.assert_not_called()
                sound.enabled = True
                sound.play()
                self.assertEqual(spawn.call_args.args[0], ['/usr/bin/paplay', str(directory / 'discharge.wav')])
                spawn.return_value.poll.return_value = None
                sound.play()
                self.assertEqual(spawn.call_count, 1)
            finally:
                sound.close()
            spawn.return_value.terminate.assert_called_once()
            spawn.return_value.wait.assert_called_once()
            self.assertFalse(directory.exists())

    def test_audio_is_lazy_reused_and_invalidated_when_volume_changes(self):
        with patch('launcher.sound.shutil.which', return_value='/usr/bin/paplay'), \
             patch('launcher.sound.synthesize', wraps=synthesize) as generated, \
             patch('launcher.sound.subprocess.Popen') as spawn:
            sound = SoundPlayer(.4)
            try:
                generated.assert_not_called()
                sound.enabled = False
                sound.play('monkey')
                generated.assert_not_called()
                sound.enabled = True
                spawn.return_value.poll.return_value = 0
                sound.play('switch')
                sound.play('switch')
                generated.assert_called_once_with('switch', .4)
                sound.set_volume(.4)
                sound.play('switch')
                self.assertEqual(generated.call_count, 1)
                sound.set_volume(.2)
                generated.assert_called_once_with('switch', .4)
                sound.play('switch')
                self.assertEqual(generated.call_count, 2)
                with wave.open(str(Path(sound.temporary.name) / 'switch.wav')) as audio:
                    self.assertEqual(audio.readframes(audio.getnframes()), synthesize('switch', .2))
                sound.set_volume(0)
                sound.play('monkey')
                self.assertEqual(generated.call_count, 2)
            finally:
                sound.close()

    def test_missing_player_is_optional(self):
        with patch('launcher.sound.shutil.which', return_value=None):
            sound = SoundPlayer(.4)
            try:
                self.assertFalse(sound.enabled)
                self.assertEqual(list(Path(sound.temporary.name).iterdir()), [])
                sound.play()
                self.assertIsNone(sound.process)
            finally:
                sound.close()


class FrameCacheTests(unittest.TestCase):
    def test_full_frames_are_created_only_on_demand_and_reused(self):
        from launcher.effects import discharge_frame
        with patch('launcher.effects.discharge_frame', wraps=discharge_frame) as generated:
            frames = CachedFrames(16, lambda index: generated((360, 120), index, .5))
            generated.assert_not_called()
            for index in (0, 8, 15):
                expected = discharge_frame((360, 120), index, .5)
                self.assertEqual(frames[index].tobytes(), expected.tobytes())
                self.assertEqual(frames[index].size, expected.size)
                self.assertIs(frames[index], frames[index])
            self.assertEqual(generated.call_count, 3)
            for index in (-1, 16):
                with self.assertRaises(IndexError):
                    frames[index]


class GamepadTests(unittest.TestCase):
    """Decode evdev through a FIFO standing in for /dev/input/eventN."""
    WORD = struct.calcsize('l') * 8

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='codwawzpc-gamepad-unit-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        (self.base / 'dev').mkdir()

    def capabilities(self, *codes):
        words = [0] * 12
        for code in codes:
            words[code // self.WORD] |= 1 << (code % self.WORD)
        return ' '.join(f'{word:x}' for word in reversed(words)).lstrip('0 ') or '0'

    def device(self, name, *codes):
        folder = self.base / 'sys' / name / 'device/capabilities'
        folder.mkdir(parents=True)
        (folder / 'key').write_text(self.capabilities(*codes) + '\n')
        os.mkfifo(self.base / 'dev' / name)

    def connect(self):
        pads = Gamepads(self.base / 'dev', self.base / 'sys')
        self.addCleanup(pads.close)
        self.assertEqual(pads.poll(now=0), ([], 0.))
        writer = os.open(self.base / 'dev/event0', os.O_WRONLY | os.O_NONBLOCK)
        self.addCleanup(os.close, writer)
        pad = pads.pads[str(self.base / 'dev/event0')]
        # FIFOs have no EVIOCGABS; give the axes a real controller's ranges.
        pad.ranges = {gamepad.ABS_X: (-32768, 32767), gamepad.ABS_Y: (-32768, 32767),
                      gamepad.ABS_RY: (-32768, 32767), gamepad.ABS_HAT0X: (-1, 1), gamepad.ABS_HAT0Y: (-1, 1)}

        def send(*events):
            os.write(writer, b''.join(gamepad.EVENT.pack(0, 0, kind, code, value) for kind, code, value in events))
        return pads, send

    def test_capability_words(self):
        self.assertTrue(has_bit(self.capabilities(gamepad.BTN_SOUTH, gamepad.BTN_START), gamepad.BTN_SOUTH))
        self.assertFalse(has_bit(self.capabilities(30, 31, 272), gamepad.BTN_SOUTH))
        self.assertFalse(has_bit('0', gamepad.BTN_SOUTH))

    def test_only_gamepads_open(self):
        self.device('event0', gamepad.BTN_SOUTH, gamepad.BTN_EAST)
        self.device('event1', 30, 31, 272)
        pads = Gamepads(self.base / 'dev', self.base / 'sys')
        self.addCleanup(pads.close)
        pads.poll(now=0)
        self.assertEqual(list(pads.pads), [str(self.base / 'dev/event0')])
        (self.base / 'dev/event0').unlink()
        pads.poll(now=gamepad.RESCAN)
        self.assertEqual(pads.pads, {})

    def test_buttons_taps_repeat_and_scroll(self):
        self.device('event0', gamepad.BTN_SOUTH)
        pads, send = self.connect()
        key, axis = gamepad.EV_KEY, gamepad.EV_ABS
        send((key, gamepad.BTN_SOUTH, 1), (key, gamepad.BTN_SOUTH, 0), (key, gamepad.BTN_EAST, 1),
             (key, gamepad.BTN_TL, 1), (key, gamepad.BTN_TR, 1), (key, gamepad.BTN_START, 2))
        self.assertEqual(pads.poll(now=1), (['accept', 'back', 'previous', 'next'], 0.))
        # A d-pad tap released before the next poll still moves once.
        send((axis, gamepad.ABS_HAT0X, -1), (axis, gamepad.ABS_HAT0X, 0))
        self.assertEqual(pads.poll(now=2)[0], ['left'])
        send((key, 0x221, 1))
        self.assertEqual(pads.poll(now=3)[0], ['down'])
        self.assertEqual(pads.poll(now=3 + gamepad.REPEAT_DELAY / 2)[0], [])
        self.assertEqual(pads.poll(now=3 + gamepad.REPEAT_DELAY)[0], ['down'])
        self.assertEqual(pads.poll(now=3 + gamepad.REPEAT_DELAY + gamepad.REPEAT_RATE)[0], ['down'])
        send((key, 0x221, 0))
        self.assertEqual(pads.poll(now=5)[0], [])
        # The stick needs a firm push, then holds until it nearly recentres.
        send((axis, gamepad.ABS_X, 15000))
        self.assertEqual(pads.poll(now=6)[0], [])
        send((axis, gamepad.ABS_X, 25000))
        self.assertEqual(pads.poll(now=7)[0], ['right'])
        send((axis, gamepad.ABS_X, 15000))
        self.assertEqual(pads.poll(now=7.1)[0], [])
        self.assertEqual(pads.held, 'right')
        send((axis, gamepad.ABS_X, 0), (axis, gamepad.ABS_RY, 32767))
        actions, scroll = pads.poll(now=8)
        self.assertEqual((actions, round(scroll, 2)), ([], 1.0))
        send((axis, gamepad.ABS_RY, 3000))
        self.assertEqual(pads.poll(now=9), ([], 0.))


class NavigationScoreTests(unittest.TestCase):
    def test_rows_and_columns_win(self):
        origin = (100, 100, 200, 140)
        same_row = (600, 100, 700, 140)
        diagonal = (220, 160, 320, 200)
        self.assertIsNone(score(origin, diagonal, 'right'))
        self.assertEqual(best(origin, [('row', same_row), ('diagonal', diagonal)], 'right'), 'row')
        self.assertIsNotNone(score(origin, diagonal, 'right', strict=False))
        below = (110, 400, 190, 440)
        near_diagonal = (300, 150, 400, 190)
        self.assertEqual(best(origin, [('column', below), ('diagonal', near_diagonal)], 'down'), 'column')
        self.assertEqual(best(origin, [('diagonal', near_diagonal)], 'down'), 'diagonal')
        self.assertIsNone(best(origin, [('column', below)], 'up'))


if __name__ == '__main__':
    result = unittest.main(exit=False)
    if not result.result.wasSuccessful():
        raise SystemExit(1)
    print('LAUNCHER_BACKEND_PASS')
