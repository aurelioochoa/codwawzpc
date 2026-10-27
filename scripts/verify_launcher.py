#!/usr/bin/env python3
"""Native interaction checks using isolated XDG state and an explicit host fixture."""
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import time
import traceback
import tkinter as tk
from tkinter import ttk
from unittest.mock import patch
import zlib
from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from launcher.backend import DEFAULTS, Directories, load_settings
from launcher import controls as gameplay_controls
from launcher.ui import Launcher
from launcher.icons import ICONS, load_icon
from launcher import i18n


FIXTURE = '''#!/usr/bin/env python3
import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['XDG_DATA_HOME']) / 'codwawzpc'
root.mkdir(parents=True, exist_ok=True)
with (root / 'calls.jsonl').open('a') as stream:
    stream.write(json.dumps(sys.argv[1:]) + '\\n')
op = sys.argv[1]
if op == 'import':
    time.sleep(.2)
    if 'reject' in sys.argv[2]:
        print('Fixture rejected archive; previous installation preserved', flush=True)
        sys.exit(3)
    (root / 'current.plist').write_text('explicit synthetic fixture')
    print('Fixture import completed', flush=True)
elif op == 'check':
    time.sleep(.1)
    if not (root / 'current.plist').exists():
        print('Fixture missing installation', flush=True)
        sys.exit(2)
    print('Fixture verified', flush=True)
elif op == 'run':
    print('Fixture runtime started', flush=True)
    while not (root / 'finish-run').exists():
        time.sleep(.05)
'''


def pump(window, predicate=lambda: True, timeout=12):
    deadline = time.monotonic() + timeout
    while True:
        window.update()
        if predicate():
            return
        if time.monotonic() > deadline:
            raise AssertionError('Timed out waiting for native UI')
        time.sleep(.015)


def settle(view):
    pump(view.window, lambda: view.resize_job is None and hasattr(view, 'background'))


def grab_window(window, path):
    window.lift()
    window.update()
    # Read the actual mapped X window, including under rootless Xwayland.
    subprocess.run(['import', '-window', str(window.winfo_id()), str(path)],
                   check=True, timeout=20, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    with Image.open(path) as captured:
        assert captured.size == (window.winfo_width(), window.winfo_height()), 'Capture clipped by the desktop'


def capture(view, path):
    settle(view)
    grab_window(view.window, path)


def capture_manual(manual, path, geometry='1120x740'):
    dialog = manual.dialog
    dialog.overrideredirect(True)
    dialog.geometry(geometry + '+30+30')
    dialog.update()
    manual.page.redraw()
    dialog.update()
    assert (dialog.winfo_width(), dialog.winfo_height()) == tuple(map(int, geometry.split('x')))
    for button in manual.actions.winfo_children():
        assert button.winfo_ismapped()
        assert button.winfo_rooty() + button.winfo_height() <= dialog.winfo_rooty() + dialog.winfo_height()
        assert button.winfo_rootx() + button.winfo_width() <= dialog.winfo_rootx() + dialog.winfo_width()
    for button in manual.nav_buttons:
        assert button.winfo_ismapped()
        assert button.winfo_rooty() + button.winfo_height() <= manual.sidebar.winfo_rooty() + manual.sidebar.winfo_height(), button.cget('text')
    canvas = manual.page.canvas
    for item in canvas.find_all():
        if canvas.type(item) in ('text', 'window'):
            box = canvas.bbox(item)
            assert box[0] >= 0 and box[2] <= canvas.winfo_width(), (canvas.type(item), box, canvas.winfo_width())
    grab_window(dialog, path)


class ScriptedGamepads:
    """Stands in for evdev so checks never depend on attached hardware.
    tests/test_launcher.py covers decoding real event records."""
    pads = {'scripted': None}

    def __init__(self):
        self.actions = []

    def poll(self):
        actions, self.actions = self.actions, []
        return actions, 0.

    def close(self):
        pass


def press(window, *keys):
    for key in keys:
        window.focus_get().event_generate(f'<{key}>')
        window.update()
    return window.focus_get()


def pad(window, gamepads, *actions):
    gamepads.actions += actions
    pump(window, lambda: not gamepads.actions)
    window.update()
    return window.focus_get()


def last_focus(top):
    return top.nametowidget(top.tk.call('focus', '-lastfor', top._w))


def check_navigation(view, gamepads):
    """Keyboard and gamepad reach every launcher control, including page controls below the fold."""
    window, buttons = view.window, view.buttons
    assert window.title() == 'Call of Duty: World at War: Zombies Mobile — Launcher'
    buttons['play'].focus_force()
    window.update()
    assert press(window, 'Right', 'Right', 'Right') is buttons['achievements']
    assert press(window, 'Right') is buttons['achievements'], 'Left/Right stay on their row'
    assert press(window, 'Down').master is view.footer
    assert press(window, 'Left').master is view.footer
    assert press(window, 'Up') in buttons.values()
    buttons['settings'].focus_force()
    window.update()
    press(window, 'Return')
    pump(window, lambda: 'settings' in view.dialogs)
    dialog = view.dialogs['settings']
    assert dialog.title() == 'Settings & controls — Call of Duty: World at War: Zombies Mobile'
    manual = dialog.field_manual
    dialog.overrideredirect(True)
    dialog.geometry('900x620+30+30')
    dialog.update()
    manual.page.redraw()
    dialog.update()
    assert last_focus(dialog) is manual.nav_buttons[0]
    manual.nav_buttons[0].focus_force()
    window.update()
    assert press(window, 'Right').cget('text') == 'Start in fullscreen'
    assert press(window, 'Down').winfo_class() == 'TCombobox'
    press(window, 'Right')
    assert view.setting_vars['FrameRate'].get() == '45'
    press(window, 'Left')
    assert view.setting_vars['FrameRate'].get() == '60'
    assert press(window, 'Down') is view.field_of_view_scale
    press(window, 'Right', 'Right', 'Left')
    assert view.setting_vars['FieldOfView'].get() == '51'
    visited = set()
    for _ in range(8):
        focus = press(window, 'Down')
        if focus.winfo_toplevel() is dialog and manual.page.holder(focus):
            visited.add(manual.page.holder(focus))
        if focus.master is manual.actions:
            break
    assert focus.master is manual.actions, 'Down leaves the page for its actions'
    assert manual.page.canvas.yview()[1] == 1.0, 'The page scrolled to its end on the way'
    assert len(visited) >= 2
    assert manual.page.holder(press(window, 'Up'))
    manual.page.canvas.yview_moveto(0)
    window.update()
    order = view.navigator.tab_order(dialog)
    page_controls = [widget for widget in order if manual.page.holder(widget)]
    def shown(widget):
        return view.navigator.visible(manual.page, view.navigator.page_rect(manual.page, widget))
    hidden = next(i for i, widget in enumerate(page_controls) if not shown(widget))
    assert len(page_controls) == 6 and hidden > 0
    assert order.index(page_controls[0]) > order.index(manual.nav_buttons[-1])
    page_controls[hidden - 1].focus_force()
    window.update()
    assert press(window, 'Tab') is page_controls[hidden]
    assert shown(page_controls[hidden]), 'Tab scrolls an off-page control into view'
    assert press(window, 'Control-Next') is manual.nav_buttons[1] and manual.selected == 1
    assert pad(window, gamepads, 'next') is manual.nav_buttons[2] and manual.selected == 2
    assert pad(window, gamepads, 'next') is manual.nav_buttons[3] and manual.selected == 3
    assert pad(window, gamepads, 'right') is view.control_fields['Forward']
    assert pad(window, gamepads, 'down') is view.control_fields['Left']
    pad(window, gamepads, 'back')
    assert not dialog.winfo_exists() and last_focus(window) is buttons['settings']
    buttons['saves'].focus_force()
    window.update()
    pad(window, gamepads, 'accept')
    saves = view.dialogs['saves']
    pump(window, lambda: last_focus(saves) is view.save_transfer_buttons[0])
    pad(window, gamepads, 'back')
    assert not saves.winfo_exists() and last_focus(window) is buttons['saves']
    buttons['saves'].focus_force()
    window.update()
    with patch.object(view, 'start') as start:
        pad(window, gamepads, 'start')
        start.assert_called_once_with('run')

    # Tk builds message boxes itself, outside tkinter's widget tree; the pad drives them by path.
    def answer(*actions):
        def press_when_shown():
            shown = [w for w in window.tk.splitlist(window.tk.call('winfo', 'children', '.')) if 'messagebox' in w]
            if not shown or not int(window.tk.call('winfo', 'viewable', shown[0])):
                window.after(30, press_when_shown)
                return
            # Focus the initial Yes button after mapping. Desktop policies may
            # otherwise focus No or the toplevel, making this fixture ambiguous.
            window.tk.call('focus', '-force', shown[0] + '.yes')
            gamepads.actions += actions
        window.after(30, press_when_shown)
        return str(window.tk.call('tk_messageBox', '-type', 'yesno', '-message', 'Gamepad check', '-parent', '.'))
    assert answer('accept') == 'yes'
    assert answer('right', 'accept') == 'no'
    assert answer('back') == 'no'
    buttons['saves'].focus_force()
    window.update()


def footer_fits(view):
    for control in view.footer.winfo_children():
        assert control.winfo_ismapped(), control
        assert control.winfo_width() >= control.winfo_reqwidth(), (control, control.cget('text'))
        assert control.winfo_x() + control.winfo_width() <= view.footer.winfo_width(), (control, control.cget('text'))
    footer_buttons = sorted(view.footer.winfo_children(), key=lambda widget: widget.winfo_x())
    assert all(left.winfo_x() + left.winfo_width() < right.winfo_x()
               for left, right in zip(footer_buttons, footer_buttons[1:])), [(w.cget('text'), w.winfo_x(), w.winfo_width()) for w in footer_buttons]


def check_languages(view, dirs, output):
    """Every interface language relabels the main window live and fits the minimum footer."""
    window = view.window
    assert view.language_button.cget('text') == 'English'
    view.language_button.invoke()
    dialog = view.dialogs['languages']
    dialog.overrideredirect(True)
    dialog.geometry('660x440+30+30')
    assert view.language_choices[i18n.SYSTEM].cget('text') == 'System language (English)'
    expected = {'en': ('Verify files', 'PLAY'), 'fr': ('Vérifier', 'JOUER'),
                'de': ('Dateien prüfen', 'SPIELEN'), 'it': ('Verifica file', 'GIOCA'), 'es': ('Comprobar', 'JUGAR')}
    window.geometry('1000x640+0+0')
    for code in ('de', 'fr', 'it', 'es', 'en'):
        choice = view.language_choices[code]
        choice.focus_force()
        window.update()
        choice.event_generate('<Return>')
        window.update()
        assert i18n.load_choice(dirs) == code == view.interface_choice == i18n.current
        assert load_icon(dirs) == 'mystery-box', 'Saving the language keeps the icon'
        assert (view.verify_button.cget('text'), view.buttons['play'].label) == expected[code]
        assert view.language_button.cget('text') == i18n.LANGUAGES[code]
        assert window.title() == f'Call of Duty: World at War: Zombies Mobile — {i18n.tr("Launcher")}'
        assert dialog.title().startswith(i18n.tr('Launcher language'))
        assert view.status_text.get() == i18n.tr(view.session.status)
        view.schedule_render()
        settle(view)
        window.update()
        footer_fits(view)
        corner = view.language_button
        assert corner.winfo_ismapped() and corner.winfo_width() >= corner.winfo_reqwidth()
        assert 0 <= corner.winfo_x() and corner.winfo_x() + corner.winfo_width() <= view.canvas.winfo_width(), code
        for name, button in view.buttons.items():
            bx = button.bbox('label')
            assert bx[0] >= 0 and bx[1] >= 0 and bx[2] <= button.winfo_width() and bx[3] <= button.winfo_height(), (code, name, bx)
        capture(view, output / f'language-{code}-1000x640.png')
    grab_window(dialog, output / 'launcher-language.png')
    view.language_choices[i18n.SYSTEM].invoke()
    assert i18n.load_choice(dirs) == i18n.SYSTEM and i18n.current == 'en'
    assert view.language_message.get() == 'Language saved.'
    with patch('launcher.ui.i18n.save_choice', side_effect=OSError('Synthetic read-only config')):
        view.language_choices['fr'].invoke()
    assert view.language_choice.get() == i18n.SYSTEM and i18n.current == 'en'
    assert 'Could not save language' in view.language_message.get()
    dialog.destroy()
    window.geometry('1440x844+0+0')
    window.update()


def main():
    output = Path(os.environ.get('CODWAWZPC_LAUNCHER_VERIFY_OUTPUT', ROOT / 'docs/launcher'))
    output.mkdir(parents=True, exist_ok=True)
    errors, callback_errors = [], []
    measured = {'provenance': 'Actual Tk window captures; installed and running states use the explicit synthetic host fixture in scripts/verify_launcher.py. No game captures.', 'sizes': [], 'checks': []}
    with tempfile.TemporaryDirectory(prefix='codwawzpc-launcher-ui-') as temporary:
        base = Path(temporary)
        # The checks read English labels whatever the desktop's language is.
        env = {'XDG_DATA_HOME': str(base / 'data'), 'XDG_CONFIG_HOME': str(base / 'config'), 'XDG_STATE_HOME': str(base / 'state'),
               'LC_ALL': 'C.UTF-8', 'LANGUAGE': ''}
        with patch.dict(os.environ, env), \
             patch('launcher.ui.messagebox.showerror', side_effect=lambda *a, **k: errors.append(a)), \
             patch('launcher.ui.messagebox.askyesno', return_value=True):
            dirs = Directories.discover()
            fixture = base / 'fixture-host'
            fixture.write_text(FIXTURE)
            fixture.chmod(0o700)
            window = tk.Tk()
            # Keep the desktop's tiling policy from overriding test sizes.
            # Production windows retain their ordinary native decorations.
            window.overrideredirect(True)
            window.report_callback_exception = lambda *args: callback_errors.append(''.join(traceback.format_exception(*args)))
            gamepads = ScriptedGamepads()
            view = Launcher(window, ROOT / 'target/release/codwawzpc', dirs, gamepads)
            try:
                assert set(view.icon_photos) == {view.selected_icon}
                assert not view.icon_previews and not view.sound.prepared
                settle(view)
                background = view.background_photo
                with patch.object(view.art, 'render', wraps=view.art.render) as compose:
                    view.render()
                    compose.assert_not_called()
                assert view.background_photo is background
                measured['checks'].append('Startup loads only the selected app icon and no audio; unchanged viewport redraw preserves the scene and skips composition')
                view.icon_button.invoke()
                icon_dialog = view.dialogs['icons']
                view.icon_button.invoke()
                assert view.dialogs['icons'] is icon_dialog
                icon_dialog.overrideredirect(True)
                icon_dialog.geometry('800x370+30+30')
                for icon in ICONS:
                    choice = view.icon_choices[icon]
                    choice.focus_force()
                    window.update()
                    with patch.object(window, 'iconphoto', wraps=window.iconphoto) as applied:
                        choice.event_generate('<Return>')
                        window.update()
                        applied.assert_called_once_with(True, *view.icon_photos[icon])
                    assert view.selected_icon == load_icon(dirs) == view.icon_choice.get() == icon
                    assert choice.instate(['selected'])
                    assert window.tk.splitlist(view.icon_button.cget('image')) == (str(view.icon_thumbnails[icon]),)
                grab_window(icon_dialog, output / 'launcher-icons.png')
                icon_dialog.geometry('740x370+30+30')
                icon_dialog.update()
                for choice in view.icon_choices.values():
                    assert choice.winfo_width() >= choice.winfo_reqwidth()
                    assert choice.winfo_rootx() >= icon_dialog.winfo_rootx()
                    assert choice.winfo_rootx() + choice.winfo_width() <= icon_dialog.winfo_rootx() + icon_dialog.winfo_width()
                grab_window(icon_dialog, output / 'launcher-icons-minimum.png')
                with patch('launcher.ui.save_icon', side_effect=OSError('Synthetic read-only config')):
                    view.icon_choices['undead'].invoke()
                assert view.selected_icon == view.icon_choice.get() == load_icon(dirs) == 'mystery-box'
                assert 'Could not save icon' in view.icon_message.get()
                icon_dialog.destroy()
                view.icon_button.invoke()
                assert view.icon_choice.get() == 'mystery-box'
                view.dialogs['icons'].destroy()
                measured['checks'].append('All four launcher icons switch via keyboard Return, update native iconphoto and footer preview, persist separately, fit 800x370 and 740x370, and preserve selection after failed save')
                check_languages(view, dirs, output)
                measured['checks'].append('Launcher language switches live via keyboard Return in all five languages, persists beside the icon, relabels the footer, radio plates and title, fits the 1000x640 footer, and returns to the system language')
                assert not view.buttons['play'].enabled
                capture(view, output / 'import-required.png')
                view.buttons['play'].invoke()
                assert view.session.child is None
                view.verify_button.invoke()
                pump(window, lambda: view.session.child is None)
                assert errors and not view.session.verified, (errors, view.session.status, view.verify_button.state(), callback_errors)
                invalid = base / 'invalid game.ipa'
                invalid.write_text('invalid archive')
                with patch('launcher.ui.filedialog.askopenfilename') as chooser:
                    view.rail_import_button.invoke()
                    assert 'installation' in view.dialogs and not chooser.called
                capture_manual(view.installation_manual, output / 'installation-missing.png')
                assert view.install_verify_button.instate(['disabled'])
                with patch('launcher.ui.filedialog.askopenfilename', return_value=str(invalid)):
                    view.import_button.invoke()
                pump(window, lambda: view.session.child is None)
                assert len(errors) == 2 and not (dirs.data / 'current.plist').exists()
                assert 'Import failed' in view.installation_manual.message.get()
                capture_manual(view.installation_manual, output / 'installation-rejected.png')
                measured['checks'].append('Real host rejects missing installation and invalid IPA without enabling Play')
                view.session.binary = fixture
                archive = base / 'game ; literal $name.ipa'
                with patch('launcher.ui.filedialog.askopenfilename', return_value=str(archive)):
                    view.import_button.invoke()
                assert view.rail_import_button.instate(['disabled']) and not view.buttons['play'].enabled
                assert view.import_button.instate(['disabled']) and view.install_verify_button.instate(['disabled'])
                capture_manual(view.installation_manual, output / 'installation-importing.png')
                pump(window, lambda: view.session.child is None and view.session.verified)
                calls = [json.loads(line) for line in (dirs.data / 'calls.jsonl').read_text().splitlines()]
                assert ['import', str(archive)] in calls and calls[-1] == ['check']
                assert view.buttons['play'].enabled
                capture_manual(view.installation_manual, output / 'installation.png')
                capture_manual(view.installation_manual, output / 'installation-minimum.png', '900x620')
                view.installation_manual.nav_buttons[1].invoke()
                capture_manual(view.installation_manual, output / 'installation-storage.png')
                capture_manual(view.installation_manual, output / 'installation-storage-minimum.png', '900x620')
                view.dialogs['installation'].destroy()
                measured['checks'].append('Import uses literal argv, blocks overlapping operations and verifies before Play')
                for width, height in [(1440, 844), (1000, 640), (1100, 900), (1560, 740)]:
                    window.geometry(f'{width}x{height}+0+0')
                    window.update()
                    view.schedule_render()
                    settle(view)
                    assert (window.winfo_width(), window.winfo_height()) == (width, height)
                    # Lamp footing follows the actual background shelf crop,
                    # rather than the independent contained foreground origin.
                    scene_w, scene_h = view.canvas.winfo_width(), view.canvas.winfo_height()
                    cover = max(scene_w / 1672, scene_h / 941)
                    expected_foot = (scene_h - 941 * cover) / 2 + 482 * cover
                    _, lamp_y, _, lamp_h = view.box(view.art.lantern)
                    assert abs(lamp_y + lamp_h - expected_foot) <= 2
                    for control in view.footer.winfo_children():
                        assert control.winfo_ismapped()
                        assert control.winfo_width() >= control.winfo_reqwidth(), control
                        assert control.winfo_x() + control.winfo_width() <= view.footer.winfo_width(), control
                        assert abs(control.winfo_y() + control.winfo_height() / 2 -
                                   view.footer.winfo_height() / 2) <= 2, control
                    footer_buttons = sorted(view.footer.winfo_children(), key=lambda widget: widget.winfo_x())
                    assert all(left.winfo_x() + left.winfo_width() < right.winfo_x()
                               for left, right in zip(footer_buttons, footer_buttons[1:])), [(w.winfo_name(), w.winfo_x(), w.winfo_width()) for w in footer_buttons]
                    for name, button in view.buttons.items():
                        x, y, w, h = button.winfo_x(), button.winfo_y(), button.winfo_width(), button.winfo_height()
                        assert 0 <= x < x + w <= view.canvas.winfo_width(), name
                        assert 0 <= y < y + h <= view.canvas.winfo_height(), name
                        bx = button.bbox('label')
                        assert bx[0] >= 0 and bx[1] >= 0 and bx[2] <= w and bx[3] <= h, (name, bx, w, h)
                        if name != 'play':
                            assert h >= 80, (name, h)
                    assert not view.canvas.find_withtag('readiness')
                    measured['sizes'].append([window.winfo_width(), window.winfo_height()])
                    capture(view, output / f'overview-{width}x{height}.png')
                window.geometry('1440x844+0+0')
                window.update()
                view.schedule_render()
                settle(view)
                assert view.readiness() == 'ready'
                assert 'RailPlate' in ttk.Style(window).element_names()
                view.verify_button.focus_force()
                window.update()
                assert window.focus_get() == view.verify_button
                capture(view, output / 'footer-focus.png')
                check_navigation(view, gamepads)
                measured['checks'].append('Arrow keys and gamepad move between neighbouring controls; Left/Right stay on a row; manual pages scroll to controls below the fold; Tab reaches them in reading order; LB/RB switch sections; A activates, B closes and restores focus, Start plays; the pad also answers Tk message boxes')
                view.window.after_cancel(view.animation_job)
                view.animation_job = None
                effects = view.effects
                assert not any(name.startswith('zombie') for name in effects.items)
                assert not (ROOT / 'launcher/assets/zombie-walk-sheet.png').exists()
                effects.update(1)
                mist = view.canvas.coords(effects.items['mist0'])
                rain = view.canvas.coords(effects.rain[0][0])
                lantern = view.canvas.itemcget(effects.items['lantern-flame'], 'image')
                effects.update(6.12)
                assert view.canvas.coords(effects.items['mist0']) != mist
                assert view.canvas.coords(effects.rain[0][0]) != rain
                assert view.canvas.itemcget(effects.items['lantern-flame'], 'image') != lantern
                assert view.canvas.itemcget(effects.items['arcs'], 'state') == 'normal'
                assert view.canvas.itemcget(effects.items['muzzle-flash'], 'state') == 'normal'
                effects.update(6.06)
                assert view.canvas.itemcget(effects.items['arcs'], 'state') == 'hidden'
                assert view.canvas.itemcget(effects.items['muzzle-flash'], 'state') == 'hidden'
                effects.update(6.12)
                capture(view, output / 'electrical-discharge.png')
                effects.monkey_sequence.next_at = 2.4
                effects.update(2.5)
                assert 122.5 <= effects.monkey_sequence.next_at <= 182.5
                monkey_image = view.canvas.itemcget(effects.items['monkey'], 'image')
                monkey_position = view.canvas.coords(effects.items['monkey'])
                assert effects.monkey_beat
                effects.update(2.7)
                assert view.canvas.itemcget(effects.items['monkey'], 'image') != monkey_image
                assert view.canvas.coords(effects.items['monkey']) != monkey_position
                mx, my, mw, mh = view.art.monkey
                assert my + mh <= min(box[1] for box in view.art.controls.values())
                capture(view, output / 'monkey-windup.png')
                # Drive the actual animation/audio dispatcher at a monkey beat.
                def monkey_tick(focused=True):
                    view.was_monkey_beat = False
                    effects.monkey_sequence.next_at = 2.4
                    with patch('launcher.ui.time.monotonic', return_value=view.animation_start + 2.5), \
                         patch.object(view, 'display_focused', return_value=focused):
                        view.animate()
                    window.after_cancel(view.animation_job)
                    view.animation_job = None
                with patch.object(view.sound, 'play') as sound:
                    monkey_tick()
                    sound.assert_called_once_with('monkey')
                    sound.reset_mock()
                    monkey_tick(focused=False)
                    sound.assert_not_called()
                    with patch.object(view.session, 'child', object()):
                        monkey_tick()
                    sound.assert_not_called()
                    with patch.object(view, 'closing', True):
                        monkey_tick()
                    sound.assert_not_called()
                view.toggle_motion()
                assert not view.motion
                frozen = view.canvas.coords(effects.items['mist0'])
                monkey_image = view.canvas.itemcget(effects.items['monkey'], 'image')
                monkey_position = view.canvas.coords(effects.items['monkey'])
                effects.update(100, False)
                assert view.canvas.coords(effects.items['mist0']) == frozen
                assert view.canvas.itemcget(effects.items['arcs'], 'state') == 'hidden'
                assert view.canvas.itemcget(effects.items['muzzle-flash'], 'state') == 'hidden'
                assert not effects.monkey_beat
                assert view.canvas.itemcget(effects.items['monkey'], 'image') == monkey_image
                assert view.canvas.coords(effects.items['monkey']) == monkey_position
                with patch.object(view.sound, 'play') as sound:
                    monkey_tick()
                    sound.assert_not_called()
                capture(view, output / 'motion-off.png')
                view.toggle_sound()
                assert 'off' in view.sound_button.cget('text').lower()
                assert window.tk.splitlist(view.sound_button.cget('image')) == (str(view.rail_switch_photos[1]),)
                assert window.tk.splitlist(view.motion_button.cget('image')) == (str(view.rail_switch_photos[1]),)
                capture(view, output / 'footer-switches-off.png')
                view.toggle_sound()
                view.toggle_motion()
                enabled = view.sound.enabled
                view.toggle_sound()
                assert view.sound.enabled != enabled
                view.toggle_sound()
                view.animation_job = window.after(42, view.animate)
                measured['checks'].append('Mist, rain, lantern flicker and weapon discharge change over time; double strike has a dark gap; Motion off freezes effects; sound toggle responds; status text fits at every size')
                measured['checks'].append('Monkey rocks, bobs and glints during its wind-up beat; dispatches monkey SFX once; focus loss, gameplay, closing and reduced motion suppress it; Motion off freezes the toy')
                measured['checks'].append('Monkey wind-up chooses a new random 120–180 second interval after each beat; initial wait uses the same range')
                measured['checks'].append('Lantern footing follows the background shelf after cover cropping at all four sizes, with its contact shadow and light sprites attached')
                view.buttons['settings'].focus_force()
                window.update()
                view.buttons['settings'].event_generate('<Return>')
                pump(window, lambda: 'settings' in view.dialogs)
                # Conflicts must not write anything. Closing discards edits.
                view.control_vars['Reload'].set('Key:W')
                view.save_button.invoke()
                assert 'Conflict' in view.settings_message.get()
                assert gameplay_controls.load(dirs) == gameplay_controls.DEFAULTS
                view.control_vars['Reload'].set('Key:T,Pad:X')
                view.dialogs['settings'].destroy()
                view.show_settings()
                pump(window, lambda: view.dialogs['settings'].winfo_viewable())
                assert view.control_vars['Reload'].get() == gameplay_controls.DEFAULTS['Reload']
                view.control_vars['Reload'].set('Key:T,Pad:X')
                view.save_button.invoke()
                assert gameplay_controls.load(dirs)['Reload'] == 'Key:T,Pad:X'
                view.reset_settings()
                view.save_button.invoke()
                assert gameplay_controls.load(dirs) == gameplay_controls.DEFAULTS
                view.setting_vars['Fullscreen'].set(True)
                view.setting_vars['InvertY'].set(True)
                view.setting_vars['IntroLogos'].set(True)
                view.setting_vars['Sensitivity'].set('2.5')
                view.setting_vars['FrameRate'].set('45')
                view.field_of_view_scale.set(62.4)
                assert view.setting_vars['FieldOfView'].get() == '62'
                view.field_of_view_scale.focus_force()
                pump(window, lambda: view.dialogs['settings'].focus_get() == view.field_of_view_scale)
                view.field_of_view_scale.event_generate('<Right>')
                assert view.setting_vars['FieldOfView'].get() == '63'
                view.dialogs['settings'].focus_force()
                view.setting_vars['FieldOfView'].set('75')
                view.setting_vars['AutoAim'].set('Medium')
                view.setting_vars['Language'].set('Italiano')
                view.setting_vars['CoopServer'].set('[::1]:27960')
                view.setting_vars['PlayerName'].set('Joueur Émile')
                view.volume.set(35)
                view.save_button.invoke()
                expected = {**DEFAULTS, 'Fullscreen': True, 'InvertY': True, 'IntroLogos': True, 'Sensitivity': 2.5, 'FrameRate': 45, 'FieldOfView': 75, 'AutoAim': 'medium', 'Language': 'italian', 'Volume': .35,
                            'CoopServer': '[::1]:27960', 'PlayerName': 'Joueur Émile'}
                assert load_settings(dirs) == expected
                view.setting_vars['Sensitivity'].set('nan')
                view.save_button.invoke()
                assert load_settings(dirs) == expected and 'Sensitivity' in view.settings_message.get()
                capture_manual(view.settings_manual, output / 'settings-validation.png')
                view.setting_vars['Sensitivity'].set('2.5')
                view.save_button.invoke()
                dialog = view.dialogs['settings']
                dialog.update()
                assert view.save_button.winfo_height() >= 28
                capture_manual(view.settings_manual, output / 'settings.png')
                capture_manual(view.settings_manual, output / 'settings-minimum.png', '900x620')
                view.settings_manual.nav_buttons[1].invoke()
                capture_manual(view.settings_manual, output / 'settings-graphics.png')
                capture_manual(view.settings_manual, output / 'settings-graphics-minimum.png', '900x620')
                view.setting_vars['RenderScale'].set('2× (960×640)')
                view.setting_vars['TextureFilter'].set('16× anisotropic')
                view.setting_vars['Widescreen'].set('16:9')
                view.picture_scales['Gamma'].set(1.337)
                assert view.setting_vars['Gamma'].get() == '1.35'
                view.setting_vars['Sharpening'].set('0.40')
                view.save_button.invoke()
                expected.update(RenderScale=2, Widescreen='16:9', TextureFilter='16x', Gamma=1.35, Sharpening=.4)
                assert load_settings(dirs) == expected, load_settings(dirs)
                view.original_picture_button.invoke()
                assert [view.setting_vars[k].get() for k in ('Gamma', 'Contrast', 'Sharpening')] == ['1.00', '1.00', '0.00']
                view.save_button.invoke()
                expected.update(Gamma=1.0, Sharpening=0.0)
                assert load_settings(dirs) == expected
                measured['checks'].append('Graphics: render scale, texture filtering and picture adjustments persist; Original picture restores gamma, contrast and sharpening')
                view.settings_manual.nav_buttons[2].invoke()
                capture_manual(view.settings_manual, output / 'settings-mouse.png')
                capture_manual(view.settings_manual, output / 'settings-mouse-minimum.png', '900x620')
                sensitivity = next(widget for widget in view.settings_manual.page.canvas.winfo_children()
                                   if isinstance(widget, ttk.Spinbox))
                sensitivity.focus_force()
                dialog.update()
                assert dialog.focus_get() == sensitivity
                capture_manual(view.settings_manual, output / 'settings-keyboard-focus.png', '900x620')
                view.settings_manual.nav_buttons[3].invoke()
                capture_manual(view.settings_manual, output / 'settings-controls.png')
                capture_manual(view.settings_manual, output / 'settings-controls-minimum.png', '900x620')
                before = view.settings_manual.page.canvas.yview()
                view.settings_manual.page.canvas.event_generate('<Button-5>')
                dialog.update()
                assert view.settings_manual.page.canvas.yview() != before
                view.settings_manual.page.canvas.yview_moveto(1)
                dialog.update()
                grab_window(dialog, output / 'settings-controls-scrolled.png')
                view.settings_manual.nav_buttons[4].invoke()
                capture_manual(view.settings_manual, output / 'settings-achievement-captures.png')
                capture_manual(view.settings_manual, output / 'settings-achievement-captures-minimum.png', '900x620')
                controls = [widget for widget in view.navigator.tab_order(dialog)
                            if view.settings_manual.page.holder(widget)]
                controls[0].focus_force()
                dialog.update()
                for control in controls[1:]:
                    assert press(window, 'Tab') is control
                    assert view.navigator.visible(view.settings_manual.page,
                        view.navigator.page_rect(view.settings_manual.page, control))
                for key, value in [('AchievementScreenshotFormat', 'BMP (lossless)'),
                                   ('AchievementScreenshotResolution', 'Up to 720p'),
                                   ('AchievementClipFps', '30'),
                                   ('AchievementClipResolution', 'Up to 720p'),
                                   ('AchievementClipQuality', 'Ultra')]:
                    field = view.capture_fields[key]
                    view.navigator.focus(field)
                    dialog.update()
                    assert field.winfo_ismapped(), f'{key} is reachable by focus and scrolling'
                    assert field.get() in dialog.tk.splitlist(field.cget('values'))
                    view.setting_vars[key].set(value)
                view.save_button.invoke()
                expected.update(AchievementScreenshotFormat='bmp', AchievementScreenshotResolution='720p',
                                AchievementClipFps=30, AchievementClipResolution='720p', AchievementClipQuality='ultra')
                assert load_settings(dirs) == expected
                grab_window(dialog, output / 'settings-achievement-captures-scrolled.png')
                measured['checks'].append('Capture format, resolution, 30/60 FPS and H.264 quality controls persist and remain reachable at minimum window size')
                view.settings_manual.nav_buttons[5].invoke()
                capture_manual(view.settings_manual, output / 'settings-coop.png')
                capture_manual(view.settings_manual, output / 'settings-coop-minimum.png', '900x620')
                assert view.coop_server_entry.winfo_ismapped() and view.player_name_entry.winfo_ismapped()
                view.coop_server_entry.focus_force()
                dialog.update()
                assert dialog.focus_get() is view.coop_server_entry
                assert press(window, 'Tab') is view.player_name_entry
                for key, value in [('CoopServer', 'host'), ('PlayerName', '')]:
                    previous = view.setting_vars[key].get()
                    view.setting_vars[key].set(value)
                    view.save_button.invoke()
                    assert load_settings(dirs) == expected and key in view.settings_message.get()
                    view.setting_vars[key].set(previous)
                view.setting_vars['CoopServer'].set('')
                view.save_button.invoke()
                expected['CoopServer'] = ''
                assert load_settings(dirs) == expected
                measured['checks'].append('Co-op server and Unicode player name persist; malformed server and empty name preserve settings; empty server disables co-op')
                view.reset_settings()
                assert view.setting_vars['Sensitivity'].get() == '1.0' and view.volume.get() == 100
                assert view.field_of_view_scale.get() == 50
                assert view.setting_vars['RenderScale'].get() == '1× (original 480×320)'
                assert view.setting_vars['TextureFilter'].get() == 'Original'
                assert view.setting_vars['Widescreen'].get() == 'Off (original 3:2)'
                assert view.setting_vars['Gamma'].get() == '1.00'
                assert view.setting_vars['CoopServer'].get() == ''
                assert view.setting_vars['PlayerName'].get() == DEFAULTS['PlayerName']
                assert view.setting_vars['AchievementClipFps'].get() == '60'
                assert view.setting_vars['AchievementClipResolution'].get() == 'Up to 1080p'
                assert view.setting_vars['AchievementScreenshotFormat'].get() == 'PNG (lossless)'
                assert load_settings(dirs) == expected
                dialog.destroy()
                measured['checks'].append('Keyboard Return opens settings; validated preferences persist in host plist; invalid input preserves saved values')
                view.buttons['achievements'].invoke()
                assert len(view.achievement_rows) == 106
                window.update()
                assert not view.achievement_icon_photos, 'Locked overview does not decode all 106 icons'
                assert min(view.achievements_manual.art_source.size) > 1000, 'Trophy retains generated detail before display sizing'
                assert view.buttons['achievements'].icon_photo.width() > view.buttons['settings'].icon_photo.width()
                assert view.buttons['achievements'].icon_photo.height() > view.buttons['settings'].icon_photo.height()
                capture_manual(view.achievements_manual, output / 'achievements-empty.png')
                capture_manual(view.achievements_manual, output / 'achievements-empty-minimum.png', '900x620')
                view.achievements_manual.nav_buttons[1].invoke()
                capture_manual(view.achievements_manual, output / 'achievements-any-map.png')
                assert 0 < len(view.achievement_icon_photos) < 106, 'Map pages load only their own icons'
                measured['checks'].append('Full-resolution generated trophy appears in Achievements; challenge icons load on demand for the selected map')
                for nav, map_id in zip(view.achievements_manual.nav_buttons[2:],
                                       ('nacht', 'verruckt', 'shinonuma', 'derriese'), strict=True):
                    nav.invoke()
                    window.update()
                    expected_icons = {row['Id'] for row in view.achievement_rows if row['Map'] == map_id}
                    assert expected_icons <= set(view.achievement_icon_photos), f'Missing {map_id} trophy images'
                    capture_manual(view.achievements_manual, output / f'achievements-{map_id}.png')
                assert set(view.achievement_icon_photos) == {row['Id'] for row in view.achievement_rows}, 'Every trophy image decodes in the native gallery'
                measured['checks'].append('All map sections render their individual generated trophy images; the full catalog decodes successfully')
                view.clip_toggle.invoke()
                assert load_settings(dirs)['AchievementClips'] is True
                assert view.clip_toggle.cget('text') == 'Clips on'
                view.clip_toggle.invoke()
                assert load_settings(dirs)['AchievementClips'] is False
                view.screenshot_toggle.invoke()
                assert load_settings(dirs)['AchievementScreenshots'] is False
                view.screenshot_toggle.invoke()
                assert load_settings(dirs)['AchievementScreenshots'] is True
                report = {'Format': 1, 'Achievements': [
                    {**{key: row[key] for key in ('Id', 'Name', 'Description', 'Points', 'Map')},
                     'Unlocked': row['Id'] == 2,
                     **({'UnlockedAt': 1720000000} if row['Id'] == 2 else {})}
                    for row in view.achievement_rows]}
                (dirs.data / 'achievements.plist').write_bytes(plistlib.dumps(report))
                view.refresh_achievements()
                assert sum(row['Unlocked'] for row in view.achievement_rows) == 1
                view.achievements_manual.nav_buttons[0].invoke()
                capture_manual(view.achievements_manual, output / 'achievements-earned.png')
                media = dirs.data / 'achievement-media' / '002'
                media.mkdir(parents=True)
                Image.new('RGB', (320, 180), (60, 90, 70)).save(media / 'screenshot.bmp')
                (media / 'clip.mp4').write_bytes(b'explicit synthetic clip fixture')
                view.refresh_achievements()
                window.update()
                view.achievements_manual.nav_buttons[0].focus_force()
                window.update()
                with patch.object(view, 'open_media') as opened:
                    assert press(window, 'Right') is view.achievement_links[('screenshot', 2)]
                    press(window, 'Return')
                    assert press(window, 'Down') is view.achievement_links[('clip', 2)]
                    press(window, 'Return')
                    assert [call.args[0].name for call in opened.call_args_list] == ['screenshot.bmp', 'clip.mp4']
                Image.new('RGB', (640, 360), (80, 100, 60)).save(media / 'screenshot.png')
                view.refresh_achievements()
                window.update()
                with patch.object(view, 'open_media') as opened:
                    view.achievement_links[('screenshot', 2)].invoke()
                    assert opened.call_args.args[0].name == 'screenshot.png'
                # Switching back to BMP must show the new capture, not a stale PNG.
                os.utime(media / 'screenshot.bmp', ns=(time.time_ns() + 1_000_000, time.time_ns() + 1_000_000))
                view.refresh_achievements()
                window.update()
                with patch.object(view, 'open_media') as opened:
                    view.achievement_links[('screenshot', 2)].invoke()
                    assert opened.call_args.args[0].name == 'screenshot.bmp'
                view.refresh_achievements()
                window.update()
                assert window.focus_get() is view.achievement_links[('clip', 2)], 'Refresh keeps capture focus'
                view.dialogs['achievements'].destroy()
                measured['checks'].append('Unlock captures are focusable buttons opened with Return, and keep focus across Refresh')
                measured['checks'].append('Achievements show the full offline catalog, filter by map, and refresh unlocks from the game report')
                view.buttons['saves'].invoke()
                capture_manual(view.saves_manual, output / 'saves-empty.png')
                capture_manual(view.saves_manual, output / 'saves-empty-minimum.png', '900x620')
                assert not view.save_table.get_children() and view.saves_summary == 'No save files yet'
                view.dialogs['saves'].destroy()
                dirs.saves.mkdir(parents=True)
                (dirs.saves / 'fixture-save.dat').write_bytes(b'explicit synthetic save fixture')
                (dirs.saves / 'external-link').symlink_to(archive)
                view.buttons['saves'].invoke()
                window.update()
                assert len(view.save_table.get_children()) == 1
                values = view.save_table.item(view.save_table.get_children()[0], 'values')
                assert len(values) == 2 and values[0] and values[1] == '31 B'
                capture_manual(view.saves_manual, output / 'saves.png')
                capture_manual(view.saves_manual, output / 'saves-minimum.png', '900x620')
                with patch('launcher.ui.os.walk', side_effect=PermissionError('Synthetic access denied')):
                    view.refresh_saves()
                assert not view.save_table.get_children() and 'access denied' in view.saves_message.get()
                capture_manual(view.saves_manual, output / 'saves-error.png')
                view.refresh_saves()
                assert len(view.save_table.get_children()) == 1
                with patch('launcher.ui.subprocess.Popen') as opened:
                    view.open_folder(dirs.saves)
                    assert opened.call_args.args[0] == ['xdg-open', str(dirs.saves)]
                # Export and import through the real host: Adler-32, then version 1.8.0.
                view.session.binary = ROOT / 'target/release/codwawzpc'
                payload = bytes([0, 8, 1, 0]) + b'synthetic progress'
                save = base / 'synthetic save.i3d'
                save.write_bytes(zlib.adler32(payload).to_bytes(4, 'little') + payload)
                corrupt = base / 'corrupt.i3d'
                corrupt.write_bytes(save.read_bytes()[:-1] + b'?')
                exported = base / 'exported save.i3d'
                infos = []
                with patch('launcher.ui.messagebox.showinfo', side_effect=lambda *a, **k: infos.append(a)):
                    with patch('launcher.ui.filedialog.askopenfilename', return_value=str(corrupt)):
                        view.save_transfer_buttons[1].invoke()
                    pump(window, lambda: view.session.child is None)
                    assert 'checksum' in errors.pop()[1] and not (dirs.saves / 'savedata1.i3d').exists()
                    with patch('launcher.ui.filedialog.askopenfilename', return_value=str(save)):
                        view.save_transfer_buttons[1].invoke()
                    assert all(b.instate(['disabled']) for b in view.save_transfer_buttons)
                    pump(window, lambda: view.session.child is None)
                    assert (dirs.saves / 'savedata1.i3d').read_bytes() == save.read_bytes(), errors
                    assert 'savedata1.i3d' in [view.save_table.item(row, 'text') for row in view.save_table.get_children()]
                    with patch('launcher.ui.filedialog.asksaveasfilename', return_value=str(exported)):
                        view.save_transfer_buttons[0].invoke()
                    pump(window, lambda: view.session.child is None)
                    assert exported.read_bytes() == save.read_bytes(), errors
                    # Declining the reset, or cancelling at the backup question, keeps the save.
                    with patch('launcher.ui.messagebox.askyesno', return_value=False), \
                         patch('launcher.ui.messagebox.askyesnocancel') as asked:
                        view.save_transfer_buttons[2].invoke()
                    assert not asked.called
                    with patch('launcher.ui.messagebox.askyesnocancel', return_value=None):
                        view.save_transfer_buttons[2].invoke()
                    assert view.session.child is None and (dirs.saves / 'savedata1.i3d').exists()
                    with patch('launcher.ui.messagebox.askyesnocancel', return_value=True):
                        view.save_transfer_buttons[2].invoke()
                    pump(window, lambda: view.session.child is None)
                    assert not (dirs.saves / 'savedata1.i3d').exists(), errors
                    assert 'savedata1.i3d' not in [view.save_table.item(row, 'text') for row in view.save_table.get_children()]
                    backups = sorted((dirs.data / 'save-backups').iterdir())
                    assert (backups[-1] / 'savedata1.i3d').read_bytes() == save.read_bytes()
                    # Reset without a backup: no new backup folder.
                    with patch('launcher.ui.filedialog.askopenfilename', return_value=str(save)):
                        view.save_transfer_buttons[1].invoke()
                    pump(window, lambda: view.session.child is None)
                    backups = len(list((dirs.data / 'save-backups').iterdir()))
                    with patch('launcher.ui.messagebox.askyesnocancel', return_value=False):
                        view.save_transfer_buttons[2].invoke()
                    pump(window, lambda: view.session.child is None)
                    assert not (dirs.saves / 'savedata1.i3d').exists(), errors
                    assert len(list((dirs.data / 'save-backups').iterdir())) == backups
                    assert 'No backup was made' in infos[-1][1]
                assert [title for title, _ in infos] == ['Saved game imported', 'Saved game exported', 'Saved game reset',
                                                         'Saved game imported', 'Saved game reset'], infos
                assert view.session.verified and not any(b.instate(['disabled']) for b in view.save_transfer_buttons)
                view.session.binary = fixture
                measured['checks'].append('Saved games exports, imports and resets through the real host, rejects a corrupt save, asks whether to back up before reset and keeps readiness')
                view.dialogs['saves'].destroy()
                view.show_logs()
                window.update()
                assert view.dialogs['logs'].winfo_exists()
                view.dialogs['logs'].destroy()
                measured['checks'].append('Saved games lists actual sandbox files; folder action dispatches literal path; log viewer opens')
                with patch('launcher.ui.filedialog.askopenfilename', return_value=str(base / 'reject.ipa')):
                    view.rail_import_button.invoke()
                    view.import_button.invoke()
                pump(window, lambda: view.session.child is None and view.session.verified)
                assert len(errors) == 3 and view.buttons['play'].enabled
                assert 'Import failed' in view.installation_manual.message.get()
                view.dialogs['installation'].destroy()
                measured['checks'].append('Rejected re-import rechecks and retains prior installation readiness')
                play = view.buttons['play']
                play.event_generate('<ButtonPress-1>', x=10, y=10)
                play.event_generate('<ButtonRelease-1>', x=10, y=10)
                child = view.session.child
                assert view.session.operation == 'run' and not view.buttons['play'].enabled
                assert view.readiness() == 'busy'
                assert view.rail_import_button.instate(['disabled'])
                pump(window, lambda: bool(view.session.history))
                capture(view, output / 'running.png')
                (dirs.data / 'finish-run').touch()
                pump(window, lambda: view.session.child is None)
                assert child.process.returncode == 0 and view.buttons['play'].enabled
                (dirs.data / 'finish-run').unlink()
                view.buttons['play'].invoke()
                child = view.session.child
                view.close()
                pump(window, lambda: view.closed)
                assert child.process.poll() is not None
                measured['checks'].append('Game lifecycle disables actions, restores readiness on exit, and reaps child on close')
                assert not callback_errors, callback_errors
                measured['checks'].append('Illustrated manuals fit 1120x740 and 900x620; sections and scrolling work; reset remains unsaved; saves show timestamps, empty state, symlink exclusion and recoverable read errors; import guide precedes chooser and preserves failure guidance after recheck')
            finally:
                if view.session.child:
                    view.session.child.stop(force=True)
                    view.session.child.process.wait(timeout=5)
                if not view.closed:
                    view.destroy()
            second = tk.Tk()
            second.overrideredirect(True)
            second.report_callback_exception = lambda *args: callback_errors.append(str(args))
            closing = Launcher(second, fixture, dirs, ScriptedGamepads())
            try:
                assert closing.selected_icon == 'mystery-box'
                measured['checks'].append('A new launcher instance restores the saved Mystery Box icon')
                closing.verify()
                pump(second, lambda: closing.session.child is None and closing.session.verified)
                closing.start('import', str(base / 'finish-before-close.ipa'))
                imported = closing.session.child
                closing.close()
                assert closing.closing and not closing.closed
                pump(second, lambda: closing.closed)
                assert imported.process.returncode == 0
                measured['checks'].append('Closing during import waits for transactional completion without terminating the importer')
                assert not callback_errors, callback_errors
            finally:
                if closing.session.child:
                    closing.session.child.stop(force=True)
                    closing.session.child.process.wait(timeout=5)
                if not closing.closed:
                    closing.destroy()
    measured['passed'] = True
    (output / 'verification.json').write_text(json.dumps(measured, indent=2) + '\n')
    print('LAUNCHER_VERIFY_PASS')


if __name__ == '__main__':
    main()
