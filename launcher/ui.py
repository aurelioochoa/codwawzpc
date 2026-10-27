# SPDX-License-Identifier: MPL-2.0
"""Native field-radio controls, dialogs and asynchronous host lifecycle."""
from pathlib import Path
import math
import os
import subprocess
import time
import tkinter as tk
from tkinter import filedialog, font as tkfont, messagebox, ttk

from PIL import Image, ImageEnhance, ImageTk

from .art import Artwork, ASSETS
from .achievements import MAPS, load_achievements
from .backend import AUTO_AIM, CAPTURE_CHOICES, DEFAULTS, LANGUAGES, Session, load_settings, save_settings, tail_log, validate_settings
from . import controls as control_settings
from .effects import SceneEffects, glow, status_lens
from .sound import SoundPlayer
from .lettering import painted_text
from .field_manual import FieldManual, FADED, INK, RED
from .icons import ASSETS as ICON_ASSETS, ICONS, ICON_SIZES, load_icon, save_icon
from .desktop import update_icon as update_desktop_icon
from .navigation import Navigator

PAPER = '#ead7b3'
MUTED = '#b5ad92'
BASE = '#191d16'
PANEL = '#25291f'
BRASS = '#ab8551'


def display_font(window):
    families = set(tkfont.families(window))
    return next((name for name in ('Nimbus Sans Narrow', 'DejaVu Sans', 'FreeSans', 'Arial')
                 if name in families), 'TkDefaultFont')


class RadioButton(tk.Canvas):
    def __init__(self, parent, text, command, icon=None, primary=False, sound=None, emphasis=False):
        super().__init__(parent, highlightthickness=0, bd=0, takefocus=True, cursor='hand2')
        self.label, self.command, self.icon, self.primary = text, command, icon, primary
        self.enabled = True
        self.hover = self.pressed = False
        self.photo = self.frame = self.skin = None
        self.scale = 1
        self.family = display_font(parent)
        self.sound = sound
        self.emphasis = emphasis
        self.bind('<Enter>', self.enter)
        self.bind('<Leave>', self.leave)
        self.bind('<ButtonPress-1>', self.press)
        self.bind('<ButtonRelease-1>', self.release)
        self.bind('<FocusIn>', lambda _: self.draw())
        self.bind('<FocusOut>', lambda _: self.draw())
        self.bind('<Return>', lambda _: self.invoke())
        self.bind('<space>', lambda _: self.invoke())

    def enter(self, _):
        self.hover = True
        self.draw()

    def leave(self, _):
        self.hover = self.pressed = False
        self.draw()

    def press(self, _):
        if self.enabled:
            self.focus_set()
            self.pressed = True
            self.draw()

    def release(self, event):
        pressed = self.pressed
        self.pressed = False
        self.draw()
        if pressed and 0 <= event.x < self.winfo_width() and 0 <= event.y < self.winfo_height():
            self.invoke()

    def invoke(self):
        if self.enabled:
            if self.sound:
                self.sound('switch')
            self.command()
        return 'break'

    def set_enabled(self, enabled):
        if enabled == self.enabled:
            return
        self.enabled = enabled
        self.configure(takefocus=enabled, cursor='hand2' if enabled else 'arrow')
        self.draw()

    def draw(self):
        if self.frame is None:
            return
        state = (self.frame, self.skin, self.scale, self.enabled, self.hover,
                 self.pressed, str(self.tk.call('focus')) == str(self))
        previous = getattr(self, '_drawn', None)
        if (previous and previous[0] is self.frame and previous[1] is self.skin
                and previous[2:] == state[2:]):
            return
        self._drawn = state
        image = self.frame.copy()
        factor = .52 if not self.enabled else (1.13 if self.hover else 1)
        skin = ImageEnhance.Brightness(self.skin).enhance(factor)
        offset = max(1, round(self.scale * 2)) if self.pressed else 0
        image.alpha_composite(skin, (0, offset))
        self.photo = ImageTk.PhotoImage(image)
        self.delete('all')
        self.create_image(0, 0, image=self.photo, anchor='nw')
        width, height = image.size
        size = max(12, round((112 if self.primary else 29) * self.scale))
        color = '#dfcba5' if self.enabled else '#9b8c74'
        if self.icon:
            icon = self.icon.copy()
            icon.thumbnail((round(width * (.36 if self.emphasis else .22)),
                            round(height * (.42 if self.emphasis else .28))), Image.Resampling.LANCZOS)
            self.icon_photo = ImageTk.PhotoImage(icon)
            self.create_image(width // 2, round(height * (.34 if self.emphasis else .26)) + offset,
                              image=self.icon_photo)
        lettering = painted_text(self.label, size, round(width * (.92 if self.emphasis else .80)),
                                  round(height * (.43 if self.icon else .62)), color)
        self.label_photo = ImageTk.PhotoImage(lettering)
        # Center the actual painted glyphs, not the font's line box, which
        # includes extra descender space below an uppercase word like PLAY.
        self.create_image(width / 2, height * ((.72 if self.emphasis else .68) if self.icon else .5) + offset,
                          image=self.label_photo, tags='label')
        if str(self.tk.call('focus')) == str(self) and self.enabled:
            self.create_rectangle(5, 5, width - 6, height - 6, outline=PAPER, dash=(3, 3), width=2)
        elif self.hover and self.enabled:
            self.create_rectangle(2, 2, width - 3, height - 3, outline=BRASS)


class Launcher:
    def __init__(self, window, binary, dirs, gamepads=None):
        self.window, self.dirs = window, dirs
        self.session = Session(binary, dirs)
        self.art = Artwork()
        self.closed = self.closing = False
        self.resize_job = None
        self.dialogs = {}
        self.selected_icon = load_icon(dirs)
        self.icon_photos = {}
        self.icon_previews = {}
        self.icon_thumbnails = {}
        self.apply_icon()
        self.settings_error = None
        try:
            self.settings = load_settings(dirs)
        except ValueError as error:
            self.settings = dict(DEFAULTS)
            self.settings_error = str(error)
        self.sound = SoundPlayer(self.settings['Volume'])
        self.motion = os.environ.get('CODWAWZPC_REDUCED_MOTION', '').lower() not in ('1', 'true', 'yes')
        self.animation_job = None
        self.effect_job = None
        self.rendered_size = None
        self.animation_start = time.monotonic()
        self.was_discharge = False
        self.was_monkey_beat = False
        self.family = display_font(window)
        window.title('codwawzpc — Call of Duty: Zombies')
        window.configure(bg=BASE)
        window.geometry(f'{min(1440, window.winfo_screenwidth() - 60)}x{min(844, window.winfo_screenheight() - 90)}')
        window.minsize(1000, 640)
        window.protocol('WM_DELETE_WINDOW', self.close)
        window.bind('<F11>', self.toggle_fullscreen)
        window.bind('<Escape>', self.escape)
        window.bind('<Control-l>', lambda _: self.show_logs())
        self.navigator = Navigator(window, gamepads)
        self.focused_once = False
        self.automatic_focus = None
        self.style()
        self.footer = tk.Canvas(window, height=72, bg='#161510', bd=0, highlightthickness=0)
        self.footer.pack(side='bottom', fill='x')
        switches = self.art.images['rail-switches']
        split = switches.width // 2
        self.rail_switch_photos = []
        for half in (switches.crop((0, 0, split, switches.height)),
                     switches.crop((split, 0, switches.width, switches.height))):
            bounds = half.getchannel('A').point(lambda a: 255 if a >= 8 else 0).getbbox()
            image = half.crop(bounds)
            image.thumbnail((42, 29), Image.Resampling.LANCZOS)
            self.rail_switch_photos.append(ImageTk.PhotoImage(image, master=window))
        self.rail_lamp_photos = {}
        self.rail_glow_frames = {}
        for state, color in (('ready', (78, 245, 113)), ('busy', (255, 183, 68)),
                             ('missing', (235, 99, 54))):
            lamp = status_lens(self.art.images['status-lamp'], color)
            lamp.thumbnail((26, 26), Image.Resampling.LANCZOS)
            self.rail_lamp_photos[state] = ImageTk.PhotoImage(lamp, master=window)
            self.rail_glow_frames[state] = [ImageTk.PhotoImage(glow((76, 62), color, .8 + i / 9), master=window)
                                            for i in range(12)]
        self.rail_glow = None
        self.verify_button = ttk.Button(self.footer, text='Verify files', style='Footer.TButton', command=self.verify)
        self.log_button = ttk.Button(self.footer, text='View logs', style='Footer.TButton', command=self.show_logs)
        self.rail_import_button = ttk.Button(self.footer, text='Import game', style='Footer.TButton',
                                             command=self.show_installation)
        self.status_text = tk.StringVar(value=self.session.status)
        self.motion_button = ttk.Button(self.footer, text='Motion on' if self.motion else 'Motion off',
                                        image=self.rail_switch_photos[0 if self.motion else 1],
                                        compound='right', style='Footer.TButton', command=self.toggle_motion)
        self.sound_button = ttk.Button(self.footer, text='Sound on' if self.sound.enabled else 'Sound unavailable',
                                       image=self.rail_switch_photos[0 if self.sound.enabled else 1],
                                       compound='right', style='Footer.TButton', command=self.toggle_sound)
        if not self.sound.player:
            self.sound_button.configure(state='disabled')
        self.icon_button = ttk.Button(self.footer, text='Launcher icon', compound='left',
                                      style='Footer.TButton',
                                      image=self.icon_thumbnails[self.selected_icon], command=self.show_icons)
        self.footer_state = 'missing'
        self.status_text.trace_add('write', lambda *_: self.render_footer())
        self.footer.bind('<Configure>', self.render_footer)
        self.canvas = tk.Canvas(window, highlightthickness=0, bd=0, bg=BASE)
        self.canvas.pack(fill='both', expand=True)
        self.effects = SceneEffects(self.canvas, self.art)
        self.buttons = {
            'play': RadioButton(self.canvas, 'PLAY', self.play, primary=True, sound=self.sound.play),
            'settings': RadioButton(self.canvas, 'Settings &\ncontrols', self.show_settings, self.art.images['icon-settings'], sound=self.sound.play),
            'saves': RadioButton(self.canvas, 'Saved\ngames', self.show_saves, self.art.images['icon-saves'], sound=self.sound.play),
            'achievements': RadioButton(self.canvas, 'Achievements', self.show_achievements,
                                        self.art.images['icon-achievements'], sound=self.sound.play, emphasis=True),
        }
        self.canvas.bind('<Configure>', self.schedule_render)
        self.navigator.manage(window, back=self.escape, start=self.buttons['play'].invoke,
                              default=self.default_focus, focus=False)
        self.refresh_state()
        if self.session.candidate:
            window.after(80, self.verify)
        self.poll_job = window.after(50, self.poll)
        self.animation_job = window.after(42, self.animate)

    def style(self):
        style = ttk.Style(self.window)
        style.theme_use('clam')
        style.configure('.', background=BASE, foreground=PAPER, font=('FreeSans', 12))
        style.configure('TFrame', background=BASE)
        style.configure('TLabel', background=BASE)
        style.configure('TButton', background=PANEL, foreground=PAPER, bordercolor=BRASS,
                        lightcolor=BRASS, darkcolor=BRASS, focuscolor=PAPER, padding=(12, 6))
        style.map('TButton', background=[('active', '#3d4230'), ('pressed', '#12170f')],
                  foreground=[('disabled', '#77796b')])
        # Generated blank cap, sliced by Tk while the live text and icon remain
        # native ttk content with normal keyboard focus and activation.
        cap = self.art.images['rail-button'].resize((88, 31), Image.Resampling.LANCZOS)
        self.rail_button_photos = [ImageTk.PhotoImage(image, master=self.window) for image in (
            cap, ImageEnhance.Brightness(cap).enhance(1.18),
            ImageEnhance.Brightness(cap).enhance(.73))]
        style.element_create('RailPlate', 'image', self.rail_button_photos[0],
                             ('pressed', self.rail_button_photos[2]),
                             ('active', self.rail_button_photos[1]),
                             border=(14, 7, 14, 7), sticky='nswe')
        style.layout('Footer.TButton', [('RailPlate', {'sticky': 'nswe', 'children': [
            ('Button.focus', {'sticky': 'nswe', 'children': [
                ('Button.padding', {'sticky': 'nswe', 'children': [('Button.label', {'sticky': 'nswe'})]})]})]})])
        style.configure('Footer.TButton', width=0, padding=(5, 2), font=(self.family, 10),
                        foreground='#e3d2ae', focuscolor='#e1b269')
        style.map('Footer.TButton', foreground=[('disabled', '#77796b'), ('active', '#fff0cf')])
        for name in ('TCheckbutton', 'TRadiobutton'):
            style.configure(name, background=BASE, foreground=PAPER)
            style.map(name, background=[('active', PANEL)], foreground=[('active', PAPER)])
        style.configure('TEntry', fieldbackground='#ece0c2', foreground='#191d16')
        style.configure('TSpinbox', fieldbackground='#ece0c2', foreground='#191d16')
        style.configure('TCombobox', fieldbackground='#ece0c2', foreground='#191d16')
        style.map('TCombobox', fieldbackground=[('readonly', '#ece0c2')], foreground=[('readonly', '#191d16')])
        style.configure('Treeview', background=PANEL, fieldbackground=PANEL, foreground=PAPER, rowheight=28)
        style.map('Treeview', background=[('selected', '#54583b')], foreground=[('selected', '#fff1cb')])
        style.configure('TLabelframe', background=BASE, bordercolor=BRASS)
        style.configure('TLabelframe.Label', background=BASE, foreground=PAPER)
        style.configure('TNotebook', background=BASE, bordercolor=BRASS)
        style.configure('TNotebook.Tab', background=PANEL, foreground=PAPER, padding=(12, 7))
        style.map('TNotebook.Tab', background=[('selected', '#454b33'), ('active', '#353c29')],
                  foreground=[('selected', '#fff0cc'), ('active', PAPER)])
        style.configure('Horizontal.TScale', background=BASE, troughcolor=PANEL,
                        bordercolor=BRASS, lightcolor=BRASS, darkcolor=BRASS)

    def schedule_render(self, _=None):
        if self.resize_job:
            self.window.after_cancel(self.resize_job)
        self.resize_job = self.window.after(90 if self.rendered_size else 0, self.render)

    def render_footer(self, _=None):
        width, height = self.footer.winfo_width(), self.footer.winfo_height()
        if width < 250 or height < 2:
            return
        if getattr(self, 'footer_size', None) != (width, height):
            self.footer_image = ImageTk.PhotoImage(self.art.rail(width, height), master=self.window)
            self.footer_size = (width, height)
        self.footer.delete('rail-art')
        self.footer.create_image(0, 0, image=self.footer_image, anchor='nw', tags='rail-art')
        center_y = height // 2
        def seat(widget, x):
            widget.place(x=x, y=center_y - widget.winfo_reqheight() // 2)
        left = max(35, round(height * .62))
        seat(self.verify_button, left)
        divider = left + self.verify_button.winfo_reqwidth() + 8
        self.footer.create_line(divider, 17, divider, height - 17,
                                fill='#9b7444', width=1, tags='rail-art')
        seat(self.log_button, divider + 10)
        import_x = divider + self.log_button.winfo_reqwidth() + 20
        seat(self.rail_import_button, import_x)
        left_end = import_x + self.rail_import_button.winfo_reqwidth()
        right = width - left
        for widget in (self.motion_button, self.sound_button, self.icon_button):
            right -= widget.winfo_reqwidth()
            seat(widget, right)
            right -= 10
        self.footer.create_line(right, 17, right, height - 17,
                                fill='#9b7444', width=1, tags='rail-art')
        status_x = left_end + 34
        status_width = max(25, right - status_x - 26)
        status = self.status_text.get()
        if status_width < 145:
            status = {'Ready to play': 'Ready', 'Installation missing': 'No game',
                      'Verifying installation…': 'Verifying…'}.get(status, status)
        self.rail_glow = self.footer.create_image(status_x, height // 2, tags='rail-art')
        self.pulse_rail_lamp()
        self.footer.create_image(status_x, height // 2,
                                 image=self.rail_lamp_photos[self.footer_state], tags='rail-art')
        self.footer.create_text(status_x + 20, height // 2,
                                text=status, anchor='w',
                                width=status_width,
                                font=(self.family, 13), fill='#ead9b6', tags='rail-art')
        self.footer.tag_lower('rail-art')

    def pulse_rail_lamp(self):
        if self.rail_glow is None:
            return
        level = 7
        if self.motion:
            t = time.monotonic() - self.animation_start
            level = round(3 + (math.sin(t * (4.5 if self.footer_state == 'busy' else 2)) + 1) / 2 * 8)
        self.footer.itemconfigure(self.rail_glow, image=self.rail_glow_frames[self.footer_state][level])

    def render(self):
        self.resize_job = None
        width, height = self.canvas.winfo_width(), self.canvas.winfo_height()
        if min(width, height) < 2:
            return
        if self.rendered_size == (width, height):
            return
        backdrop, foreground, self.scale, self.ox, self.oy = self.art.render(width, height)
        self.background = Image.alpha_composite(backdrop, foreground)
        self.background_photo = ImageTk.PhotoImage(backdrop)
        self.foreground_photo = ImageTk.PhotoImage(foreground)
        self.canvas.delete('all')
        self.canvas.create_image(0, 0, image=self.background_photo, anchor='nw')
        self.canvas.create_image(0, 0, image=self.foreground_photo, anchor='nw', tags='foreground')
        for name, control in self.buttons.items():
            x, y, w, h = self.box(self.art.controls[name])
            control.frame = self.background.crop((x, y, x + w, y + h))
            control.skin = self.art.plate('play-plate' if name == 'play' else 'utility-plate', w, h)
            control.scale = self.scale
            control.place(x=x, y=y, width=w, height=h)
            control.draw()
        self.effects.rebuild(self.scale, self.ox, self.oy)
        self.rendered_size = (width, height)
        if self.effect_job:
            self.window.after_cancel(self.effect_job)
        self.effect_job = self.window.after(100, self.warm_effects)
        self.draw_status()
        if not self.focused_once:
            self.focused_once = True
            if str(self.window.tk.call('focus', '-lastfor', self.window._w)) == str(self.window):
                self.automatic_focus = self.default_focus()
                self.automatic_focus.focus_set()

    def warm_effects(self):
        """Prepare one original animation frame per turn after the scene is visible."""
        self.effect_job = None
        if not self.closed and self.motion:
            if self.session.child:
                self.effect_job = self.window.after(100, self.warm_effects)
            elif self.effects.warm_next():
                self.effect_job = self.window.after(10, self.warm_effects)

    def default_focus(self):
        if self.buttons['play'].enabled:
            return self.buttons['play']
        if not self.session.verified and self.rail_import_button.instate(['!disabled']):
            return self.rail_import_button
        return self.buttons['settings']

    def readiness(self):
        if self.session.child:
            return 'busy'
        if self.session.verified and not self.settings_error and not self.closing:
            return 'ready'
        return 'missing'

    def display_focused(self):
        return bool(str(self.window.tk.call('focus', '-displayof', self.window._w)))

    def animate(self):
        if self.closed:
            return
        visible = self.window.state() != 'iconic' and self.window.winfo_ismapped()
        focused = self.display_focused()
        if visible:
            discharge = self.effects.update(time.monotonic() - self.animation_start, self.motion)
            self.pulse_rail_lamp()
            can_sound = not self.session.child and not self.closing and focused
            if can_sound:
                if self.effects.monkey_beat and not self.was_monkey_beat:
                    self.sound.play('monkey')
                elif discharge and not self.was_discharge:
                    self.sound.play()
            self.was_discharge = discharge
            self.was_monkey_beat = self.effects.monkey_beat
        if (not visible or not focused) and self.sound.process:
            self.sound.stop()
        self.animation_job = self.window.after(42 if visible and self.motion else 250, self.animate)

    def toggle_motion(self):
        self.motion = not self.motion
        self.motion_button.configure(text='Motion on' if self.motion else 'Motion off',
                                     image=self.rail_switch_photos[0 if self.motion else 1])
        self.render_footer()
        self.effects.update(0, self.motion)
        self.was_monkey_beat = False
        self.was_discharge = False
        if not self.motion:
            self.sound.stop()
        elif not self.effect_job:
            self.effect_job = self.window.after(100, self.warm_effects)

    def toggle_sound(self):
        self.sound.enabled = not self.sound.enabled
        if not self.sound.enabled:
            self.sound.stop()
        self.sound_button.configure(text='Sound on' if self.sound.enabled else 'Sound off',
                                    image=self.rail_switch_photos[0 if self.sound.enabled else 1])
        self.render_footer()

    def box(self, bounds):
        x, y, w, h = bounds
        return (round(self.ox + x * self.scale), round(self.oy + y * self.scale),
                max(1, round(w * self.scale)), max(1, round(h * self.scale)))

    def draw_status(self):
        if not hasattr(self, 'background'):
            return
        self.footer_state = self.readiness()
        self.render_footer()

    def refresh_state(self):
        busy = self.session.child is not None or self.closing
        self.buttons['play'].set_enabled(not busy and self.session.verified and not self.settings_error)
        for name in ('settings', 'saves', 'achievements'):
            self.buttons[name].set_enabled(not busy)
        self.verify_button.configure(state='disabled' if busy else 'normal')
        # Verification finishes after the first render; move untouched startup focus to PLAY.
        if self.automatic_focus and self.buttons['play'].enabled:
            if str(self.window.tk.call('focus', '-lastfor', self.window._w)) in (str(self.automatic_focus), '.'):
                self.buttons['play'].focus_set()
            self.automatic_focus = None
        self.rail_import_button.configure(state='disabled' if busy else 'normal')
        self.status_text.set('Finishing import before closing…' if self.closing and self.session.operation == 'import'
                             else self.session.status)
        installation = self.dialogs.get('installation')
        if installation and installation.winfo_exists() and hasattr(self, 'installation_manual'):
            self.import_button.configure(state='disabled' if busy else 'normal')
            self.install_verify_button.configure(state='disabled' if busy or not self.session.candidate else 'normal')
            self.installation_manual.message.set(self.installation_issue or
                ('Installation verified. Return to the launcher and choose Play.' if self.session.verified else
                 'Rejected imports preserve your existing installation and saves.'))
            self.installation_manual.page.schedule()
        settings = self.dialogs.get('settings')
        if settings and settings.winfo_exists() and hasattr(self, 'save_button'):
            self.save_button.configure(state='disabled' if busy else 'normal')
        saves = self.dialogs.get('saves')
        if saves and saves.winfo_exists() and hasattr(self, 'save_transfer_buttons'):
            for button in self.save_transfer_buttons:
                button.configure(state='disabled' if busy else 'normal')
        self.draw_status()

    def start(self, operation, *args):
        if operation == 'run':
            self.sound.stop()
        try:
            result = self.session.start(operation, *args)
        except OSError as error:
            self.session.status = 'Host could not start'
            messagebox.showerror('Cannot start host', f'{error}\n\nBuild it with make build.', parent=self.window)
            result = False
        self.refresh_state()
        return result

    def verify(self):
        return self.start('check')

    def play(self):
        return self.start('run')

    def choose_import(self):
        if self.session.child:
            return
        installation = self.dialogs.get('installation')
        parent = installation if installation and installation.winfo_exists() else self.window
        path = filedialog.askopenfilename(parent=parent, title='Import Call of Duty: Zombies 1.5.0',
                                          filetypes=[('iPhone archive', '*.ipa *.IPA'), ('All files', '*')])
        if path:
            self.last_import_path = path
            self.installation_issue = ''
            if installation and installation.winfo_exists():
                self.installation_file.set(path)
            self.start('import', path)

    def poll(self):
        if self.closed:
            return
        result = self.session.tick()
        if result:
            operation, code, output = result
            if operation in ('export-save', 'import-save'):
                saves = self.dialogs.get('saves')
                if saves and saves.winfo_exists():
                    self.refresh_saves()
            if operation == 'import':
                self.installation_issue = ('Import failed. Open View logs for details. Check that your IPA is unencrypted and version 1.5.0.' if code else '')
            self.refresh_state()
            if self.closing:
                self.destroy()
                return
            if code:
                messagebox.showerror(self.session.status, output[-6000:] or f'The host exited with code {code}.', parent=self.window)
                # Import is transactional. Recheck the prior installation after failure.
                if operation == 'import' and self.session.candidate:
                    self.verify()
            elif operation == 'import':
                self.verify()
            elif operation in ('export-save', 'import-save'):
                saves = self.dialogs.get('saves')
                messagebox.showinfo('Saved game exported' if operation == 'export-save' else 'Saved game imported',
                                    output.strip(), parent=saves if saves and saves.winfo_exists() else self.window)
        self.poll_job = self.window.after(50, self.poll)

    def toggle_fullscreen(self, _=None):
        self.window.attributes('-fullscreen', not self.window.attributes('-fullscreen'))
        return 'break'

    def escape(self, _=None):
        if self.window.attributes('-fullscreen'):
            self.window.attributes('-fullscreen', False)

    def dialog(self, name, title, geometry):
        existing = self.dialogs.get(name)
        if existing and existing.winfo_exists():
            existing.lift()
            existing.focus_set()
            return None
        dialog = tk.Toplevel(self.window, bg=BASE)
        dialog.iconphoto(False, *self.icon_photos[self.selected_icon])
        self.dialogs[name] = dialog
        dialog.title(title)
        dialog.geometry(geometry)
        dialog.transient(self.window)
        dialog.bind('<Escape>', lambda _: dialog.destroy())
        opener = self.window.focus_get()
        self.navigator.manage(dialog, back=dialog.destroy)

        def restore(event):
            # Return keyboard and gamepad focus to the control that opened the dialog.
            if event.widget is dialog and not self.closed and opener and opener.winfo_exists():
                opener.focus_set()
        dialog.bind('<Destroy>', restore, add='+')
        return dialog

    def apply_icon(self):
        # Keep PhotoImages alive and advertise several sizes to the window manager.
        self.prepare_icon(self.selected_icon)
        self.window.iconphoto(True, *self.icon_photos[self.selected_icon])
        for dialog in self.dialogs.values():
            if dialog.winfo_exists():
                dialog.iconphoto(False, *self.icon_photos[self.selected_icon])
        if hasattr(self, 'icon_button'):
            self.icon_button.configure(image=self.icon_thumbnails[self.selected_icon])

    def prepare_icon(self, icon, preview=False):
        if icon in self.icon_photos and (not preview or icon in self.icon_previews):
            return
        with Image.open(ICON_ASSETS / f'app-icon-{icon}.png') as source:
            image = source.convert('RGBA')
        if icon not in self.icon_photos:
            self.icon_photos[icon] = [ImageTk.PhotoImage(
                image.resize((size, size), Image.Resampling.LANCZOS), master=self.window)
                for size in ICON_SIZES]
            self.icon_thumbnails[icon] = self.icon_photos[icon][-1]
        if preview:
            self.icon_previews[icon] = ImageTk.PhotoImage(
                image.resize((112, 112), Image.Resampling.LANCZOS), master=self.window)

    def show_icons(self):
        dialog = self.dialog('icons', 'Launcher icon — codwawzpc', '800x370')
        if dialog is None:
            return
        dialog.minsize(740, 370)
        body = ttk.Frame(dialog, padding=24)
        body.pack(fill='both', expand=True)
        ttk.Label(body, text='Choose your launcher icon', font=(self.family, 20, 'bold')).pack(anchor='w')
        ttk.Label(body, text='Changes apply immediately and are remembered next time.').pack(anchor='w', pady=(5, 20))
        choices = ttk.Frame(body)
        choices.pack(fill='both', expand=True)
        self.icon_choice = tk.StringVar(master=dialog, value=self.selected_icon)
        self.icon_choices = {}
        for index, (icon, label) in enumerate(ICONS.items()):
            self.prepare_icon(icon, preview=True)
            choices.columnconfigure(index, weight=1, uniform='icons')
            choice = ttk.Radiobutton(choices, text=label, image=self.icon_previews[icon], compound='top',
                                     value=icon, variable=self.icon_choice, command=self.choose_icon,
                                     takefocus=True, padding=(8, 8))
            choice.grid(row=0, column=index, sticky='nsew', padx=5)
            self.icon_choices[icon] = choice
            choice.bind('<Return>', lambda _, button=choice: button.invoke())
        self.icon_message = tk.StringVar(master=dialog, value=f'{ICONS[self.selected_icon]} selected')
        ttk.Label(body, textvariable=self.icon_message, wraplength=680).pack(anchor='w', pady=(12, 0))
        ttk.Button(body, text='Done', command=dialog.destroy).pack(anchor='e', pady=(12, 0))
        self.icon_choices[self.selected_icon].focus_set()

    def choose_icon(self):
        icon = self.icon_choice.get()
        try:
            save_icon(self.dirs, icon)
        except (OSError, ValueError) as error:
            self.icon_choice.set(self.selected_icon)
            self.icon_message.set(f'Could not save icon: {error}')
            return
        self.selected_icon = icon
        self.apply_icon()
        self.icon_message.set(f'{ICONS[icon]} selected and saved')
        try:
            update_desktop_icon(self.dirs, icon)
        except OSError as error:
            self.icon_message.set(f'{ICONS[icon]} saved; app menu icon could not update: {error}')

    def show_settings(self):
        if self.session.child:
            return
        dialog = self.dialog('settings', 'Settings & controls — codwawzpc', '1120x740')
        if dialog is None:
            return
        manual = self.settings_manual = FieldManual(dialog, title='Settings &\ncontrols',
            subtitle='Prepare your equipment before heading into the swamp.', asset='field-radio',
            note='NEXT DEPLOYMENT\nSaved preferences take effect the next time you launch the game.')
        self.setting_vars = {key: (tk.BooleanVar(value=value) if type(value) is bool else tk.StringVar(value=str(value)))
                             for key, value in self.settings.items()}
        variables = self.setting_vars
        self.control_error = None
        try:
            control_values = control_settings.load(self.dirs)
        except ValueError as error:
            control_values = dict(control_settings.DEFAULTS)
            self.control_error = str(error)
        self.control_vars = {name: tk.StringVar(value=value) for name, value in control_values.items()}
        variables['AutoAim'].set(AUTO_AIM[self.settings['AutoAim']])
        variables['Language'].set(LANGUAGES[self.settings['Language']])
        for key, choices in CAPTURE_CHOICES.items():
            variables[key].set(choices[self.settings[key]])
        page = manual.page
        fields = {}
        self.capture_fields = {}
        for key, choices in CAPTURE_CHOICES.items():
            self.capture_fields[key] = fields[key] = ttk.Combobox(
                page.canvas, values=tuple(choices.values()), state='readonly',
                textvariable=variables[key], width=20)
        self.capture_fields['AchievementClipFps'] = fields['AchievementClipFps'] = ttk.Combobox(
            page.canvas, values=('60', '30'), state='readonly',
            textvariable=variables['AchievementClipFps'], width=20)
        for key, label in [('Fullscreen', 'Start in fullscreen'), ('TouchMode', 'Use touch controls'),
                           ('InvertY', 'Invert vertical look'),
                           ('AchievementScreenshots', 'Capture unlock screenshots'),
                           ('AchievementClips', 'Record unlock clips'),
                           ('IntroLogos', 'Show startup logos')]:
            fields[key] = ttk.Checkbutton(page.canvas, text=label, variable=variables[key], style='Manual.TCheckbutton')
        fields['Sensitivity'] = ttk.Spinbox(page.canvas, from_=.05, to=20, increment=.05,
                                           textvariable=variables['Sensitivity'], width=10)
        fields['FrameRate'] = ttk.Combobox(page.canvas, values=('60', '45'), state='readonly',
                                          textvariable=variables['FrameRate'], width=10)
        fields['AutoAim'] = ttk.Combobox(page.canvas, values=tuple(AUTO_AIM.values()), state='readonly',
                                        textvariable=variables['AutoAim'], width=10)
        fields['Language'] = ttk.Combobox(page.canvas, values=tuple(LANGUAGES.values()), state='readonly',
                                         textvariable=variables['Language'], width=14)
        self.coop_server_entry = fields['CoopServer'] = ttk.Entry(
            page.canvas, textvariable=variables['CoopServer'], width=28)
        self.player_name_entry = fields['PlayerName'] = ttk.Entry(
            page.canvas, textvariable=variables['PlayerName'], width=28)
        fields['FieldOfView'] = ttk.Frame(page.canvas, style='Manual.TFrame')
        field_of_view = variables['FieldOfView']
        # Dragging writes fractional values; snap to the whole numbers the host accepts.
        self.field_of_view_scale = ttk.Scale(fields['FieldOfView'], from_=40, to=100, variable=field_of_view,
                                             command=lambda value: field_of_view.set(str(round(float(value)))),
                                             style='Manual.Horizontal.TScale')
        self.field_of_view_scale.nav_step = 1
        self.field_of_view_scale.pack(side='left', fill='x', expand=True)
        fov_label = ttk.Label(fields['FieldOfView'], text=field_of_view.get(),
                              width=5, anchor='e', style='Manual.TLabel')
        fov_label.pack(side='right', padx=(8, 0))
        field_of_view.trace_add('write', lambda *_: fov_label.configure(text=field_of_view.get()))
        volume_frame = ttk.Frame(page.canvas, style='Manual.TFrame')
        self.volume = tk.DoubleVar(value=self.settings['Volume'] * 100)
        volume_scale = ttk.Scale(volume_frame, from_=0, to=100, variable=self.volume,
                                 style='Manual.Horizontal.TScale')
        volume_scale.nav_step = 5
        volume_scale.pack(side='left', fill='x', expand=True)
        self.volume_label = ttk.Label(volume_frame, text=f'{self.volume.get():.0f}%',
                                      width=5, anchor='e', style='Manual.TLabel')
        self.volume_label.pack(side='right', padx=(8, 0))
        self.volume.trace_add('write', lambda *_: self.volume_label.configure(text=f'{self.volume.get():.0f}%'))
        self.control_fields = control_fields = {name: ttk.Entry(page.canvas, textvariable=variable, width=46)
                          for name, variable in self.control_vars.items() if name != 'StickPreset'}
        stick_field = ttk.Combobox(page.canvas, values=('normal', 'swapped'), state='readonly',
                                   textvariable=self.control_vars['StickPreset'], width=12)

        def display():
            y = page.begin('Display & sound', 'Set the game window, frame pacing, field of view and master volume.')
            y = page.row(y, 'Game window', 'Windowed is the default. F11 also toggles fullscreen while playing.', fields['Fullscreen'])
            y = page.row(y, 'Frame-rate cap', '60 FPS is the default cap. Try 45 if your computer cannot hold 60 consistently.', fields['FrameRate'])
            y = page.row(y, 'Field of view', "50 is the game's own view. Higher values show more of the scene. Aiming keeps each weapon's zoom. Range: 40–100.", fields['FieldOfView'])
            y = page.row(y, 'Master volume', 'Controls all game audio. Set to 0% to mute, or 100% for full volume.', volume_frame)
            y = page.row(y, 'Language', "Game's choice is the default: the language picked on the game's own Language screen, English or French. Deutsch, Italiano and Español are this project's translations, which the game shipped without.", fields['Language'])
            y = page.row(y, 'Startup logos', 'Off skips the Activision, Treyarch and Ideaworks logos. On shows them for about 8 seconds. The intro movie plays separately.', fields['IntroLogos'])
            y = page.text('Launcher fullscreen is separate from the game window preference.', y, size=10, color=FADED)
            page.finish(y)

        def mouse():
            y = page.begin('Mouse & touch', 'Choose how you look around and interact with the original game.')
            y = page.row(y, 'Mouse sensitivity', '1.0 is the default. Lower values slow your aim; higher values turn faster. Range: 0.05–20.', fields['Sensitivity'], height=fields['Sensitivity'].winfo_reqheight())
            y = page.row(y, 'Vertical look', 'Enable to reverse up and down movement when looking with the mouse.', fields['InvertY'])
            y = page.row(y, 'Auto-aim', "Off is the default: your aim is never pulled toward zombies. Low, Medium and High are the game's own levels; even Low still locks on. Replaces the in-game option.", fields['AutoAim'])
            y = page.row(y, 'Original touch input', 'Off uses desktop controls. On keeps the original touch interface: the mouse acts as a finger.', fields['TouchMode'])
            y = page.text('Press Tab during desktop play to release or capture the mouse.', y, size=10, color=FADED)
            page.finish(y)

        def controls():
            y = page.begin('Field controls', 'Edit the inputs for each action. Separate multiple inputs with commas; leave blank to unbind.')
            y = page.text('Examples: Key:W, Key:Left Shift, Mouse:Left, Mouse:Wheel, Pad:A, Pad:RightTrigger. Conflicting inputs are reported when saving.', y, size=10, color=FADED)
            y += 12
            for name, field in control_fields.items():
                y = page.text(name, y, size=12, bold=True)
                y = page.embed(field, y + 4, height=field.winfo_reqheight(), x=30, width=page.width - 60) + 14
            y = page.row(y, 'Controller sticks', 'Normal: left moves and right looks. Swapped reverses these.', stick_field)
            y = page.text('Menus keep their fixed controls. Escape, Tab, F10 and F11 remain recovery shortcuts. Hold a grenade input to cook and release to throw.', y + 10, size=10, color=FADED)
            page.finish(y)

        def recording():
            y = page.begin('Achievement captures', 'Save actual gameplay when a new achievement unlocks.')
            y = page.row(y, 'Screenshots', 'Capture the presented game frame with its unlock banner. On by default.',
                         fields['AchievementScreenshots'])
            y = page.row(y, 'Screenshot format', 'PNG is lossless and easier to share. BMP also keeps every pixel, with larger files.',
                         fields['AchievementScreenshotFormat'])
            y = page.row(y, 'Screenshot resolution', 'Full game window preserves all rendered detail. Smaller limits reduce file size; images are never enlarged.',
                         fields['AchievementScreenshotResolution'])
            y = page.row(y, 'Video clips', 'Save an H.264 clip with game audio, including roughly 3 seconds before and 3 seconds after each unlock. Requires FFmpeg with libx264 and AAC.',
                         fields['AchievementClips'])
            y = page.row(y, 'Clip frame rate', '60 FPS is the default for smooth motion. Choose 30 to reduce recording load. Source motion follows the gameplay frame-rate cap.',
                         fields['AchievementClipFps'])
            y = page.row(y, 'Clip resolution', 'Up to 1080p is the default. Choose 720p or 540p to reduce recording load. Keeps the game aspect ratio and never enlarges the image.',
                         fields['AchievementClipResolution'])
            y = page.row(y, 'Clip quality', 'High is the default. Ultra preserves more detail with larger files; Standard uses less space.',
                         fields['AchievementClipQuality'])
            y = page.text('Captures are stored per achievement in your local data folder. Earlier unlocks cannot be reconstructed.',
                          y + 10, size=11, color=FADED)
            page.finish(y)

        def coop():
            y = page.begin('Co-op', 'Play through the game’s Co-Op → Local (Wi-Fi) menus with other codwawzpc players.')
            y = page.row(y, 'Co-op server', 'Enter the same relay address as your friends: HOST:PORT, for example relay.example.org:27960. Use [::1]:27960 for IPv6. Leave empty to turn co-op off.', fields['CoopServer'])
            y = page.row(y, 'Player name', 'Your OS user name is the default. The server makes duplicate names unique and limits names to 63 UTF-8 bytes.', fields['PlayerName'])
            y = page.text('Save, launch the game, then choose Co-Op → Local (Wi-Fi). If the relay cannot be reached, the game still starts with co-op unavailable.', y + 10, size=11, color=FADED)
            page.finish(y)

        self.settings_pages = {'display': display, 'mouse': mouse, 'controls': controls,
                               'recording': recording, 'coop': coop}
        manual.sections([('01   Display & sound', display), ('02   Mouse & touch', mouse),
                         ('03   Field controls', controls), ('04   Achievement captures', recording),
                         ('05   Co-op', coop)])
        self.settings_message = manual.message
        self.settings_message.set(self.control_error or self.settings_error or 'Changes apply on your next game launch. Save before returning to the launcher.')
        actions = manual.actions
        self.save_button = ttk.Button(actions, text='Save settings', style='Primary.TButton', command=self.commit_settings)
        self.save_button.pack(side='right')
        ttk.Button(actions, text='Close', command=dialog.destroy).pack(side='right', padx=8)
        ttk.Button(actions, text='Reset defaults', command=self.reset_settings).pack(side='left')

    def reset_settings(self):
        for name, value in DEFAULTS.items():
            self.setting_vars[name].set(value if type(value) is bool else str(value))
        self.setting_vars['AutoAim'].set(AUTO_AIM[DEFAULTS['AutoAim']])
        self.setting_vars['Language'].set(LANGUAGES[DEFAULTS['Language']])
        for key, choices in CAPTURE_CHOICES.items():
            self.setting_vars[key].set(choices[DEFAULTS[key]])
        for name, value in control_settings.DEFAULTS.items():
            self.control_vars[name].set(value)
        self.control_error = None
        self.volume.set(DEFAULTS['Volume'] * 100)
        self.settings_message.set('Defaults selected. Save settings to apply them.')
        self.settings_manual.message_label.configure(foreground='#c5b995')

    def commit_settings(self):
        if self.session.child:
            return
        try:
            if self.control_error:
                raise ValueError('Controls could not be read. Reset defaults before saving: ' + self.control_error)
            controls = control_settings.validate({name: var.get() for name, var in self.control_vars.items()})
            values = {key: variable.get() for key, variable in self.setting_vars.items()}
            values['Sensitivity'] = float(values['Sensitivity'])
            values['FrameRate'] = int(values['FrameRate'])
            values['FieldOfView'] = int(values['FieldOfView'])
            values['AutoAim'] = next(key for key, label in AUTO_AIM.items() if label == values['AutoAim'])
            values['Language'] = next(key for key, label in LANGUAGES.items() if label == values['Language'])
            values['AchievementClipFps'] = int(values['AchievementClipFps'])
            for name, choices in CAPTURE_CHOICES.items():
                values[name] = next(key for key, label in choices.items() if label == values[name])
            values['Volume'] = float(self.volume.get() / 100)
            values = validate_settings(values)
            control_settings.save(self.dirs, controls)
            save_settings(self.dirs, values)
        except (OSError, ValueError, tk.TclError) as error:
            self.settings_message.set(str(error))
            self.settings_manual.message_label.configure(foreground='#e6a58b')
            return
        self.settings, self.settings_error = values, None
        self.update_achievement_toggles()
        self.sound.set_volume(values['Volume'])
        self.settings_message.set('Settings saved. They apply the next time you play.')
        self.settings_manual.message_label.configure(foreground='#c5b995')
        self.refresh_state()

    def show_saves(self):
        dialog = self.dialog('saves', 'Saved games — codwawzpc', '1120x740')
        if dialog is None:
            return
        manual = self.saves_manual = FieldManual(dialog, title='Saved games',
            subtitle='Your progress, kept on this computer.', asset='field-archive',
            note='FIELD ARCHIVE\nYour progress and achievements live in one save file. Export keeps a copy; import restores one.')
        page = manual.page
        location = ttk.Entry(page.canvas)
        location.insert(0, str(self.dirs.saves))
        location.configure(state='readonly')
        table_frame = ttk.Frame(page.canvas, style='Manual.TFrame')
        self.save_table = ttk.Treeview(table_frame, columns=('modified', 'size'),
                                      show='tree headings', style='Manual.Treeview')
        self.save_table.heading('#0', text='File')
        self.save_table.heading('modified', text='Modified (local time)')
        self.save_table.heading('size', text='Size')
        self.save_table.column('#0', width=260, minwidth=180)
        self.save_table.column('modified', width=168, minwidth=168, stretch=False)
        self.save_table.column('size', width=95, minwidth=95, stretch=False, anchor='e')
        scrollbar = ttk.Scrollbar(table_frame, orient='vertical', command=self.save_table.yview)
        horizontal = ttk.Scrollbar(table_frame, orient='horizontal', command=self.save_table.xview)
        self.save_table.configure(yscrollcommand=scrollbar.set, xscrollcommand=horizontal.set)
        horizontal.pack(side='bottom', fill='x')
        scrollbar.pack(side='right', fill='y')
        self.save_table.pack(side='left', fill='both', expand=True)
        self.saves_message = manual.message
        self.saves_summary = 'Reading save files…'
        self.saves_error = None

        def render():
            y = page.begin('Local save archive', 'The game writes progress and preferences to its Documents folder.')
            y = page.text('SAVE LOCATION', y, size=10, mono=True, color=FADED)
            y = page.embed(location, y + 8, height=34)
            y = page.text(self.saves_summary, y + 23, size=16, bold=True)
            if self.saves_error:
                y = page.text('Could not read the save folder', y + 24, size=19, bold=True, color=RED)
                y = page.text('Check folder access, then choose Refresh. No files have been changed.', y + 12, size=11)
            elif not self.save_table.get_children():
                y = page.text('Your first deployment starts here.', y + 26, size=21, bold=True)
                y = page.text('Play the game and let it save your progress. Return here and choose Refresh to see the files it creates.', y + 12, size=12)
                y = page.text('Already have a save, from this launcher or the iPhone version 1.5.0? Choose Import save.', y + 18, size=11, color=FADED)
            else:
                table_height = min(240, max(128, (len(self.save_table.get_children()) + 1) * 32 + 26))
                y = page.embed(table_frame, y + 14, height=table_height)
            y = page.rule(y + 24) + 20
            y = page.text('Export and import', y, size=19, bold=True)
            y = page.text('Your rank, stats, unlocks and achievements are all in one file, savedata1.i3d or savedata2.i3d. The game alternates between the two and keeps only the newest.', y + 10, size=11)
            y = page.text('Export save copies it to a file you choose. Import save checks a file’s checksum, backs up your current save to the save-backups folder, then installs the file. Both need the game to be closed.', y + 10, size=11)
            y = page.text('Saves from the iPhone version 1.5.0 work too. Achievements in the launcher update the next time you play.', y + 10, size=11, color=FADED)
            page.finish(y)

        page.set_render(render)
        buttons = manual.actions
        self.save_transfer_buttons = [
            ttk.Button(buttons, text='Export save…', style='Primary.TButton', command=self.export_save),
            ttk.Button(buttons, text='Import save…', command=self.import_save)]
        self.save_transfer_buttons[0].pack(side='left')
        self.save_transfer_buttons[1].pack(side='left', padx=8)
        ttk.Button(buttons, text='Open save folder',
                   command=lambda: self.open_folder(self.dirs.saves)).pack(side='left')
        ttk.Button(buttons, text='Refresh', command=self.refresh_saves).pack(side='left', padx=8)
        ttk.Button(buttons, text='Close', command=dialog.destroy).pack(side='right')
        self.refresh_saves()
        self.refresh_state()

    def show_achievements(self):
        dialog = self.dialog('achievements', 'Achievements — codwawzpc', '1120x740')
        if dialog is None:
            return
        manual = self.achievements_manual = FieldManual(dialog, title='Achievements',
            subtitle='A record of the battles you survived.', asset='achievement-trophy',
            note='FIELD RECORD\nUnlocks come from the game in Regular mode. Progress is read from your local save report.')
        self.achievement_rows = []
        self.achievement_notice = ''
        self.achievement_filter = 'overview'
        self.achievement_icon_photos = {}
        self.achievement_thumb_photos = {}
        self.achievement_links = {}
        page = manual.page

        def render():
            rows = self.achievement_rows
            earned = sum(row['Points'] for row in rows if row['Unlocked'])
            unlocked = sum(row['Unlocked'] for row in rows)
            total = sum(row['Points'] for row in rows)
            section = self.achievement_filter
            title = 'Service record' if section == 'overview' else MAPS[section]
            y = page.begin(title, f'{unlocked} of {len(rows)} unlocked  /  {earned:,} of {total:,} points')
            y = page.text(self.achievement_notice, y, size=10, color=FADED)
            y = page.rule(y + 19) + 19
            if section == 'overview':
                visible = sorted((row for row in rows if row['Unlocked']),
                                 key=lambda row: row['UnlockedAt'] or 0, reverse=True)[:8]
                if visible:
                    y = page.text('Recently earned', y, size=19, bold=True)
                else:
                    y = page.text('Your record begins here.', y, size=20, bold=True)
                    y = page.text('Browse the maps at left to see every challenge. Play in Regular mode, then Refresh to see your progress.',
                                  y + 12, size=12)
            else:
                visible = [row for row in rows if row['Map'] == section]
                count = sum(row['Unlocked'] for row in visible)
                y = page.text(f'{count} of {len(visible)} earned', y, size=17, bold=True)
            from datetime import datetime
            used = set()
            for row in visible:
                top = y + 18
                if row['Id'] not in self.achievement_icon_photos:
                    with Image.open(ASSETS / 'achievement-icons' / f"{row['Id']:03d}.png") as source:
                        icon = source.convert('RGBA').resize((62, 62), Image.Resampling.LANCZOS)
                    self.achievement_icon_photos[row['Id']] = ImageTk.PhotoImage(icon, master=dialog)
                page.canvas.create_image(66, top + 33, image=self.achievement_icon_photos[row['Id']])
                text_width = max(120, page.width - 315)
                y = page.text(row['Name'], top, x=108, width=text_width, size=13, bold=True,
                              color=INK if row['Unlocked'] else FADED)
                y = page.text(row['Description'], y + 5, x=108, width=text_width, size=10, color=FADED)
                detail = f"{row['Points']:,} points"
                if row['UnlockedAt']:
                    try:
                        detail += f"  ·  First seen {datetime.fromtimestamp(row['UnlockedAt']):%Y-%m-%d}"
                    except (OverflowError, OSError, ValueError):
                        pass
                y = page.text(detail, y + 5, x=108, width=text_width,
                              size=10, color=RED if row['Unlocked'] else FADED)
                media = self.dirs.data / 'achievement-media' / f"{row['Id']:03d}"
                # Prefer the newest capture if the player changed file format;
                # keep existing BMP captures visible after upgrading.
                screenshots = [p for p in (media / 'screenshot.png', media / 'screenshot.bmp') if p.is_file()]
                screenshot = max(screenshots, key=lambda p: p.stat().st_mtime_ns) if screenshots else None
                clip = media / 'clip.mp4'
                media_x = page.width - 184
                if row['Unlocked'] and screenshot and screenshot.is_file():
                    try:
                        with Image.open(screenshot) as source:
                            thumb = source.convert('RGB')
                            thumb.thumbnail((152, 86), Image.Resampling.LANCZOS)
                        photo = ImageTk.PhotoImage(thumb, master=dialog)
                        self.achievement_thumb_photos[row['Id']] = photo
                        link = self.achievement_link(page, ('screenshot', row['Id']), screenshot,
                                                     image=photo, style='Media.TButton')
                        page.embed(link, top, height=90, x=media_x, width=158)
                        used.add(('screenshot', row['Id']))
                    except (OSError, ValueError):
                        screenshot = None
                if not row['Unlocked'] or not screenshot or not screenshot.is_file():
                    page.text('No gameplay capture yet', top + 15, x=media_x, width=154, size=9, color=FADED)
                end = top + 112
                if row['Unlocked'] and clip.is_file():
                    link = self.achievement_link(page, ('clip', row['Id']), clip,
                                                 text='▶ Play unlock clip', style='Link.TButton')
                    page.embed(link, top + 92, height=28, x=media_x, width=158)
                    used.add(('clip', row['Id']))
                    end = top + 120
                y = page.rule(max(y, end) + 13)
            for key in set(self.achievement_links) - used:
                self.achievement_links.pop(key).destroy()
            page.finish(y + 16)

        # Use the same map keys written by the game report.
        sections = [('Overview', 'overview')] + [(label, key) for key, label in MAPS.items()]
        manual.sections([(label, lambda key=key: self.select_achievements(key, render))
                         for label, key in sections])
        ttk.Button(manual.actions, text='Refresh', command=self.refresh_achievements).pack(side='left')
        self.screenshot_toggle = ttk.Button(manual.actions, command=lambda: self.toggle_achievement_capture('AchievementScreenshots'))
        self.screenshot_toggle.pack(side='left', padx=8)
        self.clip_toggle = ttk.Button(manual.actions, command=lambda: self.toggle_achievement_capture('AchievementClips'))
        self.clip_toggle.pack(side='left')
        ttk.Button(manual.actions, text='Close', command=dialog.destroy).pack(side='right')
        self.update_achievement_toggles()
        self.refresh_achievements()

    def achievement_link(self, page, key, path, **options):
        # Reuse buttons across re-renders so keyboard and gamepad focus survives Refresh.
        link = self.achievement_links.get(key)
        if link is None or not link.winfo_exists():
            link = self.achievement_links[key] = ttk.Button(page.canvas, cursor='hand2')
        link.configure(command=lambda: self.open_media(path), **options)
        return link

    def update_achievement_toggles(self):
        if hasattr(self, 'screenshot_toggle') and self.screenshot_toggle.winfo_exists():
            self.screenshot_toggle.configure(text='Screenshots on' if self.settings['AchievementScreenshots'] else 'Screenshots off')
            self.clip_toggle.configure(text='Clips on' if self.settings['AchievementClips'] else 'Clips off')

    def toggle_achievement_capture(self, key):
        values = {**self.settings, key: not self.settings[key]}
        try:
            save_settings(self.dirs, values)
        except (OSError, ValueError) as error:
            self.achievements_manual.message.set(f'Could not save capture setting: {error}')
            return
        self.settings = values
        if hasattr(self, 'setting_vars') and key in self.setting_vars:
            self.setting_vars[key].set(values[key])
        self.update_achievement_toggles()
        self.achievements_manual.message.set('Capture setting saved. It applies on your next game launch.')

    def select_achievements(self, section, render):
        self.achievement_filter = section
        self.achievements_manual.page.set_render(render)

    def refresh_achievements(self):
        self.achievement_rows, self.achievement_notice = load_achievements(self.dirs.data)
        self.achievement_thumb_photos.clear()
        self.achievements_manual.message.set(self.achievement_notice)
        self.achievements_manual.page.schedule()

    def open_media(self, path):
        try:
            subprocess.Popen(['xdg-open', str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            messagebox.showerror('Cannot open capture', str(error), parent=self.dialogs['achievements'])

    def export_save(self):
        if self.session.child:
            return
        from datetime import datetime
        path = filedialog.asksaveasfilename(parent=self.dialogs['saves'], title='Export saved game',
                                            initialfile=f'codwawzpc-save-{datetime.now():%Y-%m-%d}.i3d',
                                            defaultextension='.i3d',
                                            filetypes=[('COD Zombies save', '*.i3d'), ('All files', '*')])
        if path:
            self.start('export-save', path)

    def import_save(self):
        if self.session.child:
            return
        dialog = self.dialogs['saves']
        path = filedialog.askopenfilename(parent=dialog, title='Import saved game',
                                          filetypes=[('COD Zombies save', '*.i3d'), ('All files', '*')])
        if path and messagebox.askyesno('Replace your saved game?',
                f'Replace your current progress with {Path(path).name}?\n\nYour current save is backed up '
                f'to {self.dirs.data / "save-backups"} first.', parent=dialog):
            self.start('import-save', path)

    def refresh_saves(self):
        from datetime import datetime
        self.save_table.delete(*self.save_table.get_children())
        self.saves_error = None
        count, total, limited = 0, 0, False
        def read_error(error):
            raise error
        try:
            if self.dirs.saves.exists():
                # Never follow links out of the save folder; bound traversal and rows.
                for visited, (folder, directories, files) in enumerate(os.walk(self.dirs.saves, followlinks=False,
                                                                              onerror=read_error)):
                    if visited >= 1000:
                        limited = True
                        break
                    directories[:] = sorted(d for d in directories if not (Path(folder) / d).is_symlink())
                    for name in sorted(files):
                        path = Path(folder) / name
                        if not path.is_symlink() and path.is_file():
                            info = path.stat()
                            count += 1
                            total += info.st_size
                            self.save_table.insert('', 'end', text=str(path.relative_to(self.dirs.saves)),
                                values=(datetime.fromtimestamp(info.st_mtime).strftime('%Y-%m-%d %H:%M'), f'{info.st_size:,} B'))
                        if count >= 1000:
                            limited = True
                            break
                    if limited:
                        break
            noun = 'file' if count == 1 else 'files'
            self.saves_summary = f'{count:,} {noun}  /  {total:,} bytes' if count else 'No save files yet'
            self.saves_message.set('Showing the first 1,000 files or folders. Open the save folder to inspect the full archive.' if limited
                                   else 'Files in the game’s Documents folder. Refresh updates this view.')
        except OSError as error:
            self.save_table.delete(*self.save_table.get_children())
            self.saves_error = str(error)
            self.saves_summary = 'Save folder unavailable'
            self.saves_message.set(str(error))
        self.saves_manual.page.schedule()

    def show_installation(self):
        if self.session.child:
            return
        dialog = self.dialog('installation', 'Installation & import — codwawzpc', '1120x740')
        if dialog is None:
            return
        manual = self.installation_manual = FieldManual(dialog, title='Installation',
            subtitle='Import once. Deploy from this computer.', asset='field-supply',
            note='SUPPLY CHECK\nNo game is included. Use your own unencrypted iPhone archive, version 1.5.0.')
        page = manual.page
        self.installation_issue = getattr(self, 'installation_issue', '')
        self.installation_file = tk.StringVar(value=getattr(self, 'last_import_path', 'No archive selected in this session.'))
        archive = ttk.Entry(page.canvas, textvariable=self.installation_file, state='readonly')
        data_location = ttk.Entry(page.canvas)
        data_location.insert(0, str(self.dirs.data))
        data_location.configure(state='readonly')
        save_location = ttk.Entry(page.canvas)
        save_location.insert(0, str(self.dirs.saves))
        save_location.configure(state='readonly')
        self.installation_status = ttk.Label(page.canvas, textvariable=self.status_text,
                                             style='Manual.TLabel', font=('DejaVu Sans', 13, 'bold'))

        def guide():
            y = page.begin('Bring your game to the outpost', 'Three steps to prepare Call of Duty: Zombies for this computer.')
            for number, title, detail in [
                ('01', 'Prepare your archive', 'Use your own Call of Duty: Zombies iPhone IPA, version 1.5.0. Keep the .ipa intact. Encrypted App Store archives cannot be imported.'),
                ('02', 'Choose the .ipa file', 'Choose game IPA below. The host validates and copies the required files into local storage. Keep the launcher open until the import finishes.'),
                ('03', 'Wait for Ready to play', 'The launcher checks the installation after copying. When it is ready, return to the launcher and choose Play. The source archive is no longer needed to run the game.')]:
                page.text(number, y + 2, size=12, mono=True, color=RED, width=35)
                end = page.text(title, y, x=77, size=18, bold=True)
                y = page.text(detail, end + 8, x=77, size=11) + 23
            y = page.rule(y) + 20
            y = page.embed(self.installation_status, y, height=35)
            y = page.text('LAST SELECTED ARCHIVE', y + 12, size=9, mono=True, color=FADED)
            y = page.embed(archive, y + 8, height=33)
            if self.installation_issue:
                y = page.text(self.installation_issue, y + 18, size=11, color=RED)
            page.finish(y)

        def storage():
            y = page.begin('Storage & troubleshooting', 'Your imported game and progress live in separate local folders.')
            y = page.text('Installed game data', y, size=19, bold=True)
            y = page.embed(data_location, y + 12, height=34)
            y = page.text('Import copies the game here. Rejected imports preserve the previously installed game and its save files.', y + 12, size=11, color=FADED)
            y = page.text('Saved games', y + 26, size=19, bold=True)
            y = page.embed(save_location, y + 12, height=34)
            y = page.text('Progress stays in the game’s Documents folder. Use Saved games in the launcher to export or import your save.', y + 12, size=11, color=FADED)
            y = page.rule(y + 22) + 22
            y = page.text('If an import fails', y, size=19, bold=True)
            y = page.text('Check the archive’s version and encryption, then open View logs for the host’s exact reason. Choose another IPA to retry. Verify files checks an existing installation without importing again.', y + 12, size=11)
            y = page.text('On first launch, allow extra time for the game to prepare its cache.', y + 20, size=11, color=FADED)
            page.finish(y)

        manual.sections([('01   Import guide', guide), ('02   Storage & help', storage)])
        self.import_button = ttk.Button(manual.actions, text='Choose game IPA…', style='Primary.TButton', command=self.choose_import)
        self.import_button.pack(side='left')
        self.install_verify_button = ttk.Button(manual.actions, text='Verify files', command=self.verify)
        self.install_verify_button.pack(side='left', padx=8)
        ttk.Button(manual.actions, text='View logs', command=self.show_logs).pack(side='left')
        ttk.Button(manual.actions, text='Close', command=dialog.destroy).pack(side='right')
        self.refresh_state()

    def open_folder(self, path):
        try:
            path.mkdir(parents=True, exist_ok=True)
            subprocess.Popen(['xdg-open', str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            messagebox.showerror('Cannot open folder', str(error), parent=self.window)

    def show_logs(self):
        dialog = self.dialog('logs', 'Logs — codwawzpc', '900x570')
        if dialog is None:
            return
        body = ttk.Frame(dialog, padding=18)
        body.pack(fill='both', expand=True)
        ttk.Label(body, text='HOST LOG', font=('Times', 22, 'bold')).pack(anchor='w', pady=(0, 10))
        frame = ttk.Frame(body)
        frame.pack(fill='both', expand=True)
        text = tk.Text(frame, bg='#10140f', fg=PAPER, insertbackground=PAPER, wrap='word',
                       font=('Courier', 11), relief='flat', padx=12, pady=12, takefocus=True)
        scrollbar = ttk.Scrollbar(frame, command=text.yview)
        text.configure(yscrollcommand=scrollbar.set)
        text.pack(side='left', fill='both', expand=True)
        scrollbar.pack(side='right', fill='y')
        buttons = ttk.Frame(body)
        buttons.pack(fill='x', pady=(12, 0))

        def refresh():
            try:
                content = tail_log(self.dirs.logs / 'codwawzpc.log')
            except OSError as error:
                content = str(error)
            current = '\n\n'.join(self.session.recent)
            if self.session.child:
                current += '\n\nCurrent operation:\n' + ''.join(self.session.history)[-65536:]
            text.configure(state='normal')
            text.delete('1.0', 'end')
            text.insert('1.0', content + ('\n\nLatest launcher operation:\n' + current if current else ''))
            text.configure(state='disabled')
            text.see('end')
        ttk.Button(buttons, text='Refresh', command=refresh).pack(side='left')
        ttk.Button(buttons, text='Open log folder', command=lambda: self.open_folder(self.dirs.logs)).pack(side='left', padx=8)
        ttk.Button(buttons, text='Close', command=dialog.destroy).pack(side='right')
        dialog.bind('<Control-r>', lambda _: refresh())
        refresh()
        text.focus_set()

    def close(self):
        if self.closing:
            return
        if self.session.operation == 'run':
            if not messagebox.askyesno('Close launcher?', 'The game is running. Stop it and close the launcher?', parent=self.window):
                return
        self.closing = True
        if self.session.child:
            if self.session.operation not in ('import', 'import-save'):
                self.session.child.stop()
                self.window.after(2000, self.force_stop)
            self.refresh_state()
        else:
            self.destroy()

    def force_stop(self):
        if not self.closed and self.closing and self.session.child:
            self.session.child.stop(force=True)

    def destroy(self):
        self.closed = True
        if self.resize_job:
            self.window.after_cancel(self.resize_job)
        if self.poll_job:
            self.window.after_cancel(self.poll_job)
        if self.animation_job:
            self.window.after_cancel(self.animation_job)
        if self.effect_job:
            self.window.after_cancel(self.effect_job)
        self.navigator.close()
        self.sound.close()
        self.window.destroy()
