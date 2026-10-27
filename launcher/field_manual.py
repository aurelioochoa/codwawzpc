# SPDX-License-Identifier: MPL-2.0
"""Illustrated field manuals with live native controls and scrollable pages."""
from functools import lru_cache
import tkinter as tk
from tkinter import ttk

from PIL import Image, ImageTk

from .art import ASSETS, load_artwork
from .i18n import tr

INK = '#29291e'
FADED = '#625b45'
PAPER = '#e9d9b5'
OLIVE = '#191d16'
CREAM = '#ead7b3'
RED = '#873b2c'


@lru_cache(maxsize=6)
def manual_image(name):
    """Share immutable decoded illustrations between reopened manuals."""
    if name == 'achievement-trophy':
        return load_artwork(name)
    with Image.open(ASSETS / f'{name}.png') as source:
        image = source.convert('RGB' if name == 'field-paper' else 'RGBA')
    return image if name == 'field-paper' else image.crop(image.getbbox())


def manual_styles(window):
    style = ttk.Style(window)
    style.configure('Manual.TCheckbutton', background=PAPER, foreground=INK,
                    font=('DejaVu Sans', 11), padding=7)
    style.map('Manual.TCheckbutton', background=[('active', '#dac9a4')],
              foreground=[('disabled', FADED), ('active', INK)])
    style.configure('Manual.TFrame', background=PAPER)
    style.configure('Manual.TLabel', background=PAPER, foreground=INK,
                    font=('DejaVu Sans', 10))
    style.configure('Manual.Horizontal.TScale', background=PAPER, troughcolor='#b6a37c')
    style.configure('Manual.Treeview', background='#ecdfc3', fieldbackground='#ecdfc3',
                    foreground=INK, rowheight=32, font=('DejaVu Sans', 10))
    style.configure('Manual.Treeview.Heading', background='#c9b991', foreground=INK,
                    font=('DejaVu Sans', 10, 'bold'), padding=7)
    style.map('Manual.Treeview', background=[('selected', '#50563b')],
              foreground=[('selected', '#fff1d1')])
    style.configure('Primary.TButton', background='#74372b', foreground=CREAM,
                    padding=(18, 9), font=('DejaVu Sans', 11, 'bold'))
    style.map('Primary.TButton', background=[('active', '#924a37'), ('pressed', '#58291f')])
    style.configure('Section.TButton', anchor='w', padding=(14, 10),
                    font=('DejaVu Sans', 10), background='#25291f')
    style.map('Section.TButton', background=[('selected', '#454a31'), ('active', '#353b29')])
    # Keyboard and gamepad focus must stay visible on the paper page.
    style.map('Manual.Horizontal.TScale', troughcolor=[('focus', '#c98f6f')], bordercolor=[('focus', RED)])
    style.map('Manual.Treeview', bordercolor=[('focus', RED)], lightcolor=[('focus', RED)],
              darkcolor=[('focus', RED)])
    for name, foreground in (('Media.TButton', INK), ('Link.TButton', RED)):
        style.configure(name, background=PAPER, foreground=foreground, bordercolor=PAPER,
                        lightcolor=PAPER, darkcolor=PAPER, focuscolor=RED,
                        padding=(2, 2), font=('DejaVu Sans', 10, 'bold'))
        style.configure(name, anchor='center' if name == 'Media.TButton' else 'w')
        style.map(name, background=[('active', '#dac9a4'), ('pressed', '#cdb98f')],
                  foreground=[('active', foreground)])


class ManualPane:
    """A flowing paper canvas; embedded fields keep their native focus behavior."""
    def __init__(self, parent):
        self.frame = ttk.Frame(parent)
        # Not a Tab stop; arrow keys and the gamepad focus it to read pages without controls.
        self.canvas = tk.Canvas(self.frame, bg=PAPER, bd=0, highlightthickness=2,
                                highlightcolor=OLIVE, highlightbackground=OLIVE,
                                yscrollincrement=28, takefocus=False)
        self.bar = ttk.Scrollbar(self.frame, command=self.canvas.yview)
        self.bar.pack(side='right', fill='y')
        self.canvas.pack(side='left', fill='both', expand=True)
        self.canvas.configure(yscrollcommand=self.bar.set)
        self.paper = manual_image('field-paper')
        self.photo = None
        self.paper_size = None
        self.render = None
        self.job = None
        self.width = 600
        self.extent = 0
        self.embedded = {}
        self.canvas.bind('<Configure>', self.schedule)
        self.canvas.bind('<Destroy>', self.cleanup)
        # Tk also rings the canvas while an embedded control has focus; ring only the page itself.
        for event in ('<FocusIn>', '<FocusOut>'):
            self.canvas.bind(event, lambda _: self.canvas.configure(
                highlightcolor=RED if str(self.canvas.tk.call('focus')) == str(self.canvas) else OLIVE), add='+')
        # Scope the wheel binding to this dialog; trees and entry fields retain theirs.
        self.canvas.winfo_toplevel().bind('<MouseWheel>', self.wheel, add='+')
        self.canvas.winfo_toplevel().bind('<Button-4>', self.wheel, add='+')
        self.canvas.winfo_toplevel().bind('<Button-5>', self.wheel, add='+')

    def cleanup(self, event):
        if event.widget is self.canvas and self.job:
            self.canvas.after_cancel(self.job)
            self.job = None

    def wheel(self, event):
        over_full_tree = isinstance(event.widget, ttk.Treeview) and event.widget.yview() == (0.0, 1.0)
        if event.widget is self.canvas or over_full_tree:
            delta = -1 if getattr(event, 'num', 0) == 4 else 1
            if getattr(event, 'delta', 0):
                delta = -1 if event.delta > 0 else 1
            self.canvas.yview_scroll(delta * 3, 'units')
            return 'break'

    def schedule(self, _=None):
        if self.job:
            self.canvas.after_cancel(self.job)
        self.job = self.canvas.after_idle(self.redraw)

    def redraw(self):
        self.job = None
        if self.render and self.canvas.winfo_width() > 10:
            self.render()

    def begin(self, title, subtitle):
        self.canvas.delete('all')
        self.embedded = {}
        self.width = self.canvas.winfo_width()
        self.paper_item = self.canvas.create_image(0, 0, image=self.photo, anchor='nw')
        y = self.text(title, 29, size=25, bold=True)
        y = self.text(subtitle, y + 9, color=FADED, size=11)
        return self.rule(y + 22) + 24

    def text(self, text, y, *, x=30, width=None, size=12, color=INK, bold=False, mono=False):
        item = self.canvas.create_text(x, y, text=text, anchor='nw', fill=color,
            width=width if width is not None else max(1, self.width - x - 30),
            font=('DejaVu Sans Mono' if mono else ('DejaVu Serif' if size >= 19 else 'DejaVu Sans'),
                  size, 'bold' if bold else 'normal'))
        return self.canvas.bbox(item)[3]

    def rule(self, y):
        self.canvas.create_line(30, y, self.width - 30, y, fill='#ac9870')
        return y

    def embed(self, widget, y, height=38, *, x=30, width=None):
        self.embedded[widget] = self.canvas.create_window(x, y, window=widget, anchor='nw',
                                  width=width or max(1, self.width - x - 30), height=height)
        if not getattr(widget, '_manual_focus', False):
            widget.bind('<FocusIn>', lambda _: self.reveal(widget), add='+')
            widget._manual_focus = True
        return y + height

    def holder(self, widget):
        """The embedded page control that contains widget, if any."""
        while widget is not None and widget is not self.canvas:
            if widget in self.embedded:
                return widget
            widget = widget.master
        return None

    def reveal(self, widget):
        holder = self.holder(widget)
        if holder is None:
            return
        self.canvas.update_idletasks()
        # Item coordinates stay valid while the control is scrolled out and unmapped.
        _, y, _, end = self.canvas.bbox(self.embedded[holder])
        height = end - y
        top = self.canvas.canvasy(0)
        bottom = top + self.canvas.winfo_height()
        if y < top or end > bottom:
            target = max(0, y - 24) if y < top or height >= self.canvas.winfo_height() else end - self.canvas.winfo_height() + 24
            self.canvas.yview_moveto(target / max(1, self.extent))

    def row(self, y, title, detail, widget, height=38):
        control_width = min(230, max(180, self.width * .35))
        text_width = max(120, self.width - 95 - control_width)
        end = self.text(title, y, size=12, bold=True, width=text_width)
        end = self.text(detail, end + 6, size=10, color=FADED, width=text_width)
        self.embed(widget, y + 2, height, x=self.width - control_width - 30, width=control_width)
        return self.rule(max(end, y + height) + 18) + 20

    def finish(self, y):
        self.extent = max(y + 30, self.canvas.winfo_height())
        width, height = max(1, self.width), max(1, int(self.extent))
        if self.paper_size != (width, height):
            tile = self.paper.resize((width, max(1, round(self.paper.height * width / self.paper.width))),
                                     Image.Resampling.BILINEAR)
            flipped = tile.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
            texture = Image.new('RGB', (width, height))
            for index, top in enumerate(range(0, height, tile.height)):
                texture.paste(flipped if index % 2 else tile, (0, top))
            self.photo = ImageTk.PhotoImage(texture)
            self.paper_size = (width, height)
        self.canvas.itemconfigure(self.paper_item, image=self.photo)
        self.canvas.configure(scrollregion=(0, 0, self.width, self.extent))

    def set_render(self, render):
        self.render = render
        self.canvas.yview_moveto(0)
        self.schedule()


class FieldManual:
    def __init__(self, dialog, *, title, subtitle, asset, note):
        manual_styles(dialog)
        self.dialog = dialog
        dialog.field_manual = self
        self.selected = 0
        dialog.geometry('1120x740')
        dialog.minsize(900, 620)
        header = ttk.Frame(dialog, padding=(20, 12))
        header.pack(fill='x')
        ttk.Label(header, text=tr('SHI NO NUMA   /   FIELD MANUAL'), foreground='#b5ad92',
                  font=('DejaVu Sans Mono', 10)).pack(side='left')
        ttk.Button(header, text=tr('Back to launcher'), command=dialog.destroy).pack(side='right')
        tk.Frame(dialog, bg='#ab8551', height=1).pack(fill='x')
        self.footer = ttk.Frame(dialog, padding=(20, 12))
        self.footer.pack(side='bottom', fill='x')
        self.message = tk.StringVar()
        self.message_label = ttk.Label(self.footer, textvariable=self.message, foreground='#c5b995',
                                      font=('DejaVu Sans', 10), wraplength=1040)
        self.message_label.pack(fill='x', pady=(0, 10))
        self.actions = ttk.Frame(self.footer)
        self.actions.pack(fill='x')
        body = ttk.Frame(dialog, padding=(18, 16))
        body.pack(fill='both', expand=True)
        self.sidebar = ttk.Frame(body, width=250, padding=(0, 0, 20, 0))
        self.sidebar.pack(side='left', fill='y')
        self.sidebar.pack_propagate(False)
        self.art = tk.Canvas(self.sidebar, bg=OLIVE, highlightthickness=0, height=208)
        self.art.pack(fill='x')
        self.art_source = manual_image(asset)
        self.art_size = None
        self.side_title = ttk.Label(self.sidebar, text=title, font=('DejaVu Serif', 21, 'bold'), wraplength=230)
        self.side_title.pack(anchor='w', pady=(16, 8))
        self.side_subtitle = ttk.Label(self.sidebar, text=subtitle, font=('DejaVu Sans', 10), foreground='#c5b995',
                                       wraplength=226, justify='left')
        self.side_subtitle.pack(anchor='w')
        self.navigation = ttk.Frame(self.sidebar)
        self.navigation.pack(fill='x', pady=(22, 0))
        self.note = ttk.Label(self.sidebar, text=note, wraplength=226, justify='left',
                              font=('DejaVu Sans', 10), foreground='#b5ad92')
        self.note.pack(side='bottom', anchor='w', pady=(12, 0))
        self.page = ManualPane(body)
        self.page.frame.pack(side='left', fill='both', expand=True)
        self.nav_buttons = []
        dialog.bind('<Configure>', self.wrap_message, add='+')
        self.sidebar.bind('<Configure>', self.fit_sidebar)
        self.art.bind('<Configure>', self.paint_art)

    def fit_sidebar(self, _=None):
        fixed = self.side_title.winfo_reqheight() + 24 + self.side_subtitle.winfo_reqheight()
        fixed += self.navigation.winfo_reqheight() + 22
        available = self.sidebar.winfo_height() - fixed
        note_height = self.note.winfo_reqheight() + 12
        if available - note_height >= 80:
            if not self.note.winfo_manager():
                self.note.pack(side='bottom', anchor='w', pady=(12, 0))
            available -= note_height
        else:
            self.note.pack_forget()
        self.art.configure(height=max(1, min(208, available)))

    def paint_art(self, _=None):
        width, height = self.art.winfo_width(), self.art.winfo_height()
        if self.art_size == (width, height):
            return
        self.art_size = (width, height)
        source = self.art_source.copy()
        source.thumbnail((max(1, width - 4), max(1, height - 8)), Image.Resampling.LANCZOS)
        self.photo = ImageTk.PhotoImage(source)
        self.art.delete('all')
        self.art.create_image(width // 2, height // 2, image=self.photo)

    def wrap_message(self, event):
        if event.widget is self.dialog:
            self.message_label.configure(wraplength=max(200, event.width - 40))

    def sections(self, sections):
        for label, render in sections:
            index = len(self.nav_buttons)
            button = ttk.Button(self.navigation, text=label, style='Section.TButton',
                                command=lambda i=index, r=render: self.select(i, r))
            button.pack(fill='x', pady=3)
            self.nav_buttons.append(button)
        self.select(0, sections[0][1])
        self.nav_buttons[0].focus_set()
        self.dialog.after_idle(self.fit_sidebar)

    def select(self, index, render):
        self.selected = index
        for position, button in enumerate(self.nav_buttons):
            button.state(['selected'] if position == index else ['!selected'])
        self.page.set_render(render)
