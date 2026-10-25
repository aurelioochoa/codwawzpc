# SPDX-License-Identifier: MPL-2.0
"""Keyboard and gamepad navigation shared by the launcher and its dialogs.

Arrow keys and the d-pad move focus to the nearest control in that direction.
Inside a field manual, controls scrolled out of view are still reachable: the
paper page scrolls toward them first, so text between controls stays readable.
"""
import tkinter as tk

from .gamepad import Gamepads

TAG = 'LauncherNav'
KEYS = {'Up': 'up', 'Down': 'down', 'Left': 'left', 'Right': 'right'}
FOCUSABLE = {'TButton', 'Button', 'TCheckbutton', 'TRadiobutton', 'Checkbutton', 'Radiobutton',
             'TCombobox', 'TSpinbox', 'TEntry', 'TScale', 'Treeview', 'Text'}
PRESSABLE = {'TButton', 'Button', 'TCheckbutton', 'TRadiobutton', 'Checkbutton', 'Radiobutton'}


class Scope:
    def __init__(self, back, start=None, default=None):
        self.back, self.start, self.default = back, start, default


def descendants(widget):
    for child in widget.winfo_children():
        if isinstance(child, (tk.Toplevel, tk.Tk)):
            continue
        yield child
        yield from descendants(child)


def contains(parent, widget):
    path = str(parent)
    return str(widget) == path or str(widget).startswith(path + '.')


def score(origin, target, direction, strict=True):
    """Distance to a candidate, or None when it is not in that direction."""
    ax0, ay0, ax1, ay1 = origin
    bx0, by0, bx1, by1 = target
    acx, acy, bcx, bcy = (ax0 + ax1) / 2, (ay0 + ay1) / 2, (bx0 + bx1) / 2, (by0 + by1) / 2
    if direction in ('up', 'down'):
        sign = 1 if direction == 'down' else -1
        along, gap = (bcy - acy) * sign, (by0 - ay1 if sign > 0 else ay0 - by1)
        across, offset = max(0, bx0 - ax1, ax0 - bx1), abs(bcx - acx)
    else:
        sign = 1 if direction == 'right' else -1
        along, gap = (bcx - acx) * sign, (bx0 - ax1 if sign > 0 else ax0 - bx1)
        across, offset = max(0, by0 - ay1, ay0 - by1), abs(bcy - acy)
    if along <= 1 or (strict and across > 0 and direction in ('left', 'right')):
        return None
    # Controls sharing the row or column win over diagonal ones, as on a console menu.
    return across > 0, max(0, gap) + across * 4 + offset * .1


def best(origin, candidates, direction, strict=True):
    ranked = [(score(origin, rect, direction, strict), index) for index, (_, rect) in enumerate(candidates)]
    ranked = [entry for entry in ranked if entry[0] is not None]
    return candidates[min(ranked)[1]][0] if ranked else None


class Navigator:
    def __init__(self, window, gamepads=None):
        self.window = window
        self.scopes = {}
        self.gamepads = Gamepads() if gamepads is None else gamepads
        self.scroll_carry = 0.
        self.job = None
        for key, direction in KEYS.items():
            window.bind_class(TAG, f'<{key}>', lambda event, d=direction: self.key(event, d))
        for key in ('<Return>', '<KP_Enter>'):
            window.bind_class(TAG, key, lambda event: 'break' if self.activate(event.widget) else None)
        window.bind_class(TAG, '<Prior>', lambda event: self.page_key(event, -1))
        window.bind_class(TAG, '<Next>', lambda event: self.page_key(event, 1))
        window.bind_class(TAG, '<Control-Prior>', lambda event: self.section_key(event, -1))
        window.bind_class(TAG, '<Control-Next>', lambda event: self.section_key(event, 1))
        window.bind_class(TAG, '<Tab>', lambda event: self.tab(event, 1))
        window.bind_class(TAG, '<<PrevWindow>>', lambda event: self.tab(event, -1))
        window.bind_all('<FocusIn>', self.adopt, add='+')
        self.job = window.after(250, self.poll)

    # Scopes are the launcher's own windows; message boxes and file choosers keep Tk's defaults.
    def manage(self, top, back, start=None, default=None, focus=True):
        self.scopes[str(top)] = Scope(back, start, default)
        for widget in (top, *descendants(top)):
            self.tag(widget)
        top.bind('<Destroy>', lambda event: self.scopes.pop(str(top), None) if event.widget is top else None, add='+')
        if focus:
            # Once the window is shown, give it a starting control unless one was chosen.
            def mapped(event):
                if event.widget is top:
                    top.after_idle(lambda: top.winfo_exists() and self.unfocused(top) and self.default(top))
            top.bind('<Map>', mapped, add='+')

    def unfocused(self, top):
        return str(self.window.tk.call('focus', '-lastfor', top._w)) == str(top)

    def scope(self, widget):
        return self.scopes.get(str(widget.winfo_toplevel())) if widget else None

    def tag(self, widget):
        tags = widget.bindtags()
        if TAG not in tags:
            widget.bindtags((TAG,) + tags)

    def adopt(self, event):
        widget = event.widget
        if isinstance(widget, tk.Misc) and widget.winfo_exists() and self.scope(widget):
            self.tag(widget)

    @staticmethod
    def manual(widget):
        return getattr(widget.winfo_toplevel(), 'field_manual', None)

    @staticmethod
    def section_button(manual):
        return manual.nav_buttons[manual.selected] if manual.nav_buttons else None

    # Geometry
    def rect(self, widget):
        x, y = widget.winfo_rootx(), widget.winfo_rooty()
        return x, y, x + widget.winfo_width(), y + widget.winfo_height()

    def viewport(self, page):
        return self.rect(page.canvas)

    def page_rect(self, page, widget):
        holder = page.holder(widget)
        x0, y0, x1, y1 = page.canvas.bbox(page.embedded[holder])
        dx = page.canvas.winfo_rootx() - page.canvas.canvasx(0)
        dy = page.canvas.winfo_rooty() - page.canvas.canvasy(0)
        x0, y0, x1, y1 = x0 + dx, y0 + dy, x1 + dx, y1 + dy
        if widget is holder:
            return x0, y0, x1, y1
        dx = dy = 0
        current = widget
        while current is not holder:
            dx, dy = dx + current.winfo_x(), dy + current.winfo_y()
            current = current.master
        width = widget.winfo_width() if widget.winfo_width() > 1 else widget.winfo_reqwidth()
        height = widget.winfo_height() if widget.winfo_height() > 1 else widget.winfo_reqheight()
        return x0 + dx, y0 + dy, x0 + dx + width, y0 + dy + height

    def visible(self, page, rect):
        _, top, _, bottom = self.viewport(page)
        # A control taller than half the page counts once it fills half the page.
        shown = min(rect[3], bottom) - max(rect[1], top)
        return shown >= min(rect[3] - rect[1], (bottom - top) / 2) - 1

    # Candidates
    def focus_ok(self, widget):
        try:
            return bool(int(self.window.tk.call('::tk::FocusOK', widget._w)))
        except (tk.TclError, ValueError):
            return False

    def page_focusable(self, widget):
        if widget.winfo_class() not in FOCUSABLE and not callable(getattr(widget, 'invoke', None)):
            return False
        try:
            if str(widget.cget('takefocus')) in ('0', 'False', 'false'):
                return False
        except tk.TclError:
            pass
        if hasattr(widget, 'instate'):
            return widget.instate(['!disabled'])
        try:
            return str(widget.cget('state')) != 'disabled'
        except tk.TclError:
            return True

    def page_candidates(self, page, only_visible=False):
        result = []
        for holder in list(page.embedded):
            if not holder.winfo_exists():
                continue
            for widget in (holder, *descendants(holder)):
                if self.page_focusable(widget):
                    rect = self.page_rect(page, widget)
                    if not only_visible or self.visible(page, rect):
                        result.append((widget, rect))
        return result

    def outside_candidates(self, top, page=None):
        result = []
        for widget in descendants(top):
            if page and contains(page.canvas, widget):
                continue
            if self.focus_ok(widget):
                result.append((widget, self.rect(widget)))
        return result

    # Focus changes
    def focus(self, widget):
        widget.focus_set()
        manual = self.manual(widget)
        if manual and manual.page.holder(widget):
            manual.page.reveal(widget)

    def default(self, top):
        scope = self.scopes.get(str(top))
        manual = getattr(top, 'field_manual', None)
        target = None
        if scope and scope.default:
            target = scope.default()
        elif manual:
            # Children may not be mapped yet, so read the actions directly.
            actions = [w for w in manual.actions.winfo_children() if self.page_focusable(w)]
            target = self.section_button(manual) or next(iter(actions), None)
        if target is None:
            candidates = self.outside_candidates(top)
            target = candidates[0][0] if candidates else None
        if target:
            self.focus(target)
        return True

    def move(self, widget, direction):
        top = widget.winfo_toplevel()
        if widget is top:
            return self.default(top)
        manual = self.manual(widget)
        if manual:
            return self.manual_move(manual, widget, direction)
        target = best(self.rect(widget), [c for c in self.outside_candidates(top) if c[0] is not widget], direction)
        if target:
            self.focus(target)
        return True

    def manual_move(self, manual, widget, direction):
        page = manual.page
        top = widget.winfo_toplevel()
        reading = widget is page.canvas
        if reading or page.holder(widget):
            return self.page_move(manual, widget, direction)
        origin = self.rect(widget)
        candidates = [c for c in self.outside_candidates(top, page) if c[0] is not widget]
        # The page joins as one region; entering it picks a visible control or the page itself.
        candidates.append((page.canvas, self.viewport(page)))
        target = best(origin, candidates, direction)
        if target is page.canvas:
            shown = self.page_candidates(page, only_visible=True)
            if widget in manual.nav_buttons and shown:
                # From the section list, start reading at the top of the page.
                target = min(shown, key=lambda c: (round(c[1][1]), c[1][0]))[0]
            else:
                target = best(origin, shown, direction, strict=False) or page.canvas
        elif target in manual.nav_buttons and widget not in manual.nav_buttons:
            target = self.section_button(manual)
        if target:
            self.focus(target)
        return True

    def page_move(self, manual, widget, direction):
        page = manual.page
        reading = widget is page.canvas
        x0, top, x1, bottom = self.viewport(page)
        if reading:
            origin = (x0, top - 2, x1, top - 1) if direction == 'down' else (x0, bottom + 1, x1, bottom + 2)
        else:
            origin = self.page_rect(page, widget)
        if direction == 'left' and (reading or origin[0] - x0 < 60) and manual.nav_buttons:
            self.focus(self.section_button(manual))
            return True
        others = [c for c in self.page_candidates(page) if c[0] is not widget]
        if direction in ('left', 'right'):
            target = best(origin, [c for c in others if self.visible(page, c[1])], direction)
            if target:
                self.focus(target)
            elif direction == 'left' and manual.nav_buttons:
                self.focus(self.section_button(manual))
            return True
        target = best(origin, others, direction)
        rect = dict(others).get(target)
        if target and self.visible(page, rect):
            self.focus(target)
            return True
        step = 1 if direction == 'down' else -1
        if self.can_scroll(page, step):
            self.scroll_page(page, step * max(1, round(page.canvas.winfo_height() * .4 / 28)))
            return True
        if target:
            self.focus(target)
            return True
        origin = self.viewport(page) if reading else origin
        outside = self.outside_candidates(widget.winfo_toplevel(), page)
        outside = [c for c in outside if c[0] not in manual.nav_buttons]
        target = best(origin, outside, direction)
        if target:
            self.focus(target)
        return True

    @staticmethod
    def can_scroll(page, step):
        first, last = page.canvas.yview()
        return first > 0 if step < 0 else last < 1

    def scroll_page(self, page, units):
        page.canvas.yview_scroll(units, 'units')
        focus = self.window.focus_get()
        # Never leave focus on a control that has scrolled out of sight.
        if focus is not None and page.holder(focus):
            page.canvas.update_idletasks()
            if not self.visible(page, self.page_rect(page, focus)):
                page.canvas.focus_set()

    # Per-control adjustments
    def adjust(self, widget, direction):
        cls = widget.winfo_class()
        horizontal = direction in ('left', 'right')
        step = 1 if direction in ('right', 'down') else -1
        if horizontal and cls == 'TScale':
            low, high = float(widget.cget('from')), float(widget.cget('to'))
            amount = getattr(widget, 'nav_step', (high - low) / 20)
            widget.set(min(high, max(low, float(widget.get()) + step * amount)))
            return True
        if horizontal and cls == 'TSpinbox':
            widget.event_generate('<<Increment>>' if step > 0 else '<<Decrement>>')
            return True
        if horizontal and cls == 'TCombobox':
            return self.cycle(widget, step, wrap=False)
        if not horizontal and cls == 'Treeview':
            return self.tree_step(widget, step)
        if not horizontal and cls == 'Text':
            first, last = widget.yview()
            if (step < 0 and first <= 0) or (step > 0 and last >= 1):
                return False
            widget.yview_scroll(step * 3, 'units')
            return True
        if horizontal and cls == 'TEntry' and not widget.instate(['readonly']):
            return None
        return False

    @staticmethod
    def cycle(widget, step, wrap):
        values = widget.cget('values')
        if not values:
            return True
        values = list(widget.tk.splitlist(values))
        index = widget.current()
        index = (index + step) % len(values) if wrap else min(len(values) - 1, max(0, index + step))
        if index != widget.current():
            widget.current(index)
            widget.event_generate('<<ComboboxSelected>>')
        return True

    @staticmethod
    def tree_step(tree, step):
        items = tree.get_children()
        if not items:
            return False
        current = tree.focus()
        if current in items:
            index = items.index(current) + step
            if not 0 <= index < len(items):
                return False
        else:
            index = 0 if step > 0 else len(items) - 1
        tree.focus(items[index])
        tree.selection_set(items[index])
        tree.see(items[index])
        return True

    def direction(self, widget, direction):
        """Apply a direction to the focused widget; True when it was handled."""
        if widget is None or not self.scope(widget):
            return False
        adjusted = self.adjust(widget, direction)
        if adjusted is None:
            return False
        return adjusted or self.move(widget, direction)

    def activate(self, widget):
        if widget is None:
            return False
        cls = widget.winfo_class()
        if cls == 'TCombobox':
            return self.cycle(widget, 1, wrap=True)
        if cls in PRESSABLE or (cls == 'Canvas' and callable(getattr(widget, 'invoke', None))):
            widget.invoke()
            return True
        if widget is widget.winfo_toplevel() and self.scope(widget):
            return self.default(widget)
        return False

    # Keyboard
    def key(self, event, direction):
        return 'break' if self.direction(event.widget, direction) else None

    def page_key(self, event, step):
        manual = self.manual(event.widget)
        if not manual or event.widget.winfo_class() == 'Treeview':
            return None
        if self.can_scroll(manual.page, step):
            self.scroll_page(manual.page, step * max(1, round(manual.page.canvas.winfo_height() * .85 / 28)))
        return 'break'

    def section_key(self, event, step):
        return 'break' if self.section(event.widget, step) else None

    def section(self, widget, step):
        manual = self.manual(widget) if widget else None
        if not manual or len(manual.nav_buttons) < 2:
            return False
        index = (manual.selected + step) % len(manual.nav_buttons)
        manual.nav_buttons[index].invoke()
        manual.nav_buttons[index].focus_set()
        return True

    def tab_order(self, top):
        manual = getattr(top, 'field_manual', None)
        page = manual.page
        outside = [w for w, _ in self.outside_candidates(top, page)]
        footer = [w for w in outside if contains(manual.footer, w)]
        footer.sort(key=lambda w: (w.winfo_rooty(), w.winfo_rootx()))
        inside = sorted(self.page_candidates(page), key=lambda c: (round(c[1][1]), c[1][0]))
        return [w for w in outside if w not in footer] + [w for w, _ in inside] + footer

    def tab(self, event, step):
        widget = event.widget
        top = widget.winfo_toplevel()
        if getattr(top, 'field_manual', None) is None:
            return None
        order = self.tab_order(top)
        if not order:
            return 'break'
        if widget in order:
            target = order[(order.index(widget) + step) % len(order)]
        else:
            target = order[0 if step > 0 else -1]
        self.focus(target)
        return 'break'

    # Gamepad
    def poll(self):
        self.job = None
        try:
            actions, scroll = self.gamepads.poll()
            if str(self.window.tk.call('focus')):
                for action in actions:
                    self.pad(action)
                if scroll:
                    self.pad_scroll(scroll)
            else:
                self.scroll_carry = 0.
        finally:
            # A failing action must not stop the controller for the rest of the session.
            self.job = self.window.after(16 if self.gamepads.pads else 1000, self.poll)

    def focused(self):
        """The focused widget, or None for windows Tk builds itself (message boxes, file choosers)."""
        path = str(self.window.tk.call('focus'))
        try:
            return self.window.nametowidget(path) if path else None
        except KeyError:
            return None

    def pad(self, action):
        widget = self.focused()
        scope = self.scope(widget)
        if scope is None:
            path = str(self.window.tk.call('focus'))
            if path:
                self.native_pad(path, action)
            return
        if action in KEYS.values():
            self.direction(widget, action)
        elif action == 'accept':
            if not self.activate(widget):
                widget.event_generate('<Return>')
        elif action == 'back':
            scope.back()
        elif action in ('previous', 'next'):
            self.section(widget, -1 if action == 'previous' else 1)
        elif action == 'start' and scope.start:
            scope.start()

    def native_pad(self, path, action):
        """Drive Tk's own dialogs by window path: buttons move spatially, other controls get keys."""
        call = self.window.tk.call
        cls = str(call('winfo', 'class', path))
        pressable = cls in PRESSABLE
        if action in KEYS.values():
            # Entries have no use for Up and Down, so those leave, e.g. for a file chooser's list.
            if not pressable and not (cls in ('Entry', 'TEntry') and action in ('up', 'down')):
                call('event', 'generate', path, f'<{next(k for k, d in KEYS.items() if d == action)}>')
                return

            def rect(widget):
                x, y = int(call('winfo', 'rootx', widget)), int(call('winfo', 'rooty', widget))
                return x, y, x + int(call('winfo', 'width', widget)), y + int(call('winfo', 'height', widget))

            def walk(parent):
                for child in self.window.tk.splitlist(call('winfo', 'children', parent)):
                    if str(call('winfo', 'toplevel', child)) == top:
                        yield str(child)
                        yield from walk(child)
            top = str(call('winfo', 'toplevel', path))
            focusable = [w for w in walk(top) if w != path and str(call('::tk::FocusOK', w)) == '1']
            # Composite widgets such as the file list accept focus on an outer frame; use the inner one.
            focusable = [w for w in focusable if not any(other.startswith(w + '.') for other in focusable)]
            candidates = [(w, rect(w)) for w in focusable]
            target = best(rect(path), candidates, action)
            if target:
                call('focus', target)
        elif action == 'accept':
            call(path, 'invoke') if pressable else call('event', 'generate', path, '<Return>')
        elif action == 'back':
            call('event', 'generate', path, '<Escape>')

    def pad_scroll(self, value):
        widget = self.focused()
        manual = self.manual(widget) if widget else None
        self.scroll_carry += value * .5
        units = int(self.scroll_carry)
        if not units:
            return
        self.scroll_carry -= units
        if manual:
            self.scroll_page(manual.page, units)
        elif widget is not None and widget.winfo_class() == 'Text':
            widget.yview_scroll(units, 'units')

    def close(self):
        if self.job:
            self.window.after_cancel(self.job)
            self.job = None
        self.gamepads.close()
