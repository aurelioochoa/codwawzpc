# Shi No Numa native launcher

The launcher uses the selected Shi No Numa concept: a misty swamp hut, separate
Wunderwaffe and lantern, and a low field-radio console. Main menu labels are
rendered at runtime as faded, chipped paint over the blank metal plates. PLAY
is centered by its visible glyphs, avoiding uneven font-baseline spacing.
Mouse hover, press, disabled state and keyboard focus remain native controls.

The Wunderwaffe's coils pulse and snap through irregularly spaced, short branching double strikes
with original synthesized SFX. Mist and rain drift behind the foreground artwork. The lantern
sits on the wooden rail and flickers with warm spill light. Its base and
contact shadow follow the shelf's background crop as the window changes shape.
The status lamp pulses green when Play is available, amber while busy, and red when attention
is needed. A separate Monkey Bomb sits on the case beside the weapon: it rocks,
bobs and catches a brass glint during a short clockwork-and-cymbal sequence at
random intervals of 2–3 minutes, including the initial wait. Its SFX are
originally synthesized and follow the same volume,
focus and gameplay rules as the weapon. Motion off freezes the toy and stops
any current ambience. Larger metal controls use condensed sans-serif labels
and separate icon space.

The background uses one continuous cover image at every window size, with
softly blurred and shaded edges. Foreground artwork keeps its proportions;
there are no repeated inset background strips. The bottom maintenance rail
uses generated worn brass and gunmetal artwork, with generated button plates
and physical on/off switches. Its controls remain live and keyboard accessible;
the rail's readiness light and label respond to actual state. See the
[generated bottom-bar concept](../docs/launcher-concepts/09-monkey-maintenance-rail.png)
and [native verification instructions](../docs/launcher/README.md).

The radio panel opens **Achievements**, where the full 106-entry catalog is
available before the first launch. The field manual shows earned points,
recent unlocks and challenges by map. Refresh reads the local
`achievements.plist` report produced by the game host; achievements unlock
in Regular mode. **Import game** now sits on the bottom maintenance rail.
The Achievements manual displays a unique generated trophy: a helmeted undead
skull with blue Wunderwaffe coils, brass laurel and a swamp-worn ammunition-box
plinth. Its unmodified 1254×1254 transparent master and runtime copy are in
`launcher/artwork/sources/achievement-trophy.png` and
`launcher/assets/achievement-trophy.png`; both preserve every generated pixel.
The trophy and radio button icon have their generated masters in `launcher/artwork/sources/`.
Each catalog entry also has its own individually image-generated trophy in
`launcher/assets/achievement-icons/`. The designs draw on the original 2008
PS3/Xbox 360 World at War trophy icons, Zombies DLC artwork and original concept
art, rendered as detailed weathered collectibles like the other ports.
The [contact sheet](../docs/launcher/achievement-icons-contact.png) shows all 106.
Full-resolution transparent masters and a portable integrity specification
are included in `launcher/artwork/`; private prompts and downloaded official
references remain local. `build_achievement_icons.py` packages the
256px runtime copies without repainting the masters. See
[artwork notes](artwork/achievement-artwork.md) for the source references.
The Achievements button has a larger, centered icon and label.

The Achievements manual has **Screenshots** and **Clips** toggles; the same
preferences are on Settings → Achievement captures. Screenshots default on,
clips default off. Settings take effect on the next game launch. When a new
achievement unlocks during play, the host saves the real presented frame to
`$XDG_DATA_HOME/codwawzpc/achievement-media/<id>/screenshot.png` (or `userdata/data/achievement-media/<id>/screenshot.png` in a portable folder). PNG is lossless
and uses the full game window by default. Settings also offer lossless BMP and
1080p/720p screenshot limits. The gallery opens both PNG and existing BMP captures.
If clips are enabled and FFmpeg with `libx264` and the AAC encoder is installed,
it also saves an H.264 `clip.mp4` with stereo game audio there, covering roughly
three seconds before and three seconds after the unlock (segment boundaries can
add up to a second). Clips default to
**60 FPS, up to 1080p, High quality**. Settings offer 30/60 FPS, 540p/720p/1080p
limits, and Standard/High/Ultra compression quality. Dimensions retain the game
aspect ratio and never exceed the rendered window. Higher resolutions need a
larger game window; recording cannot add source detail or motion above the
gameplay frame-rate cap.

Encoding and screenshot compression run in the background. Video uses a bounded
three-frame queue and a compressed rolling disk buffer, avoiding a large raw-frame
history. A bounded audio history captures the game's own mix at the configured
game volume, without microphone or other applications' audio. If the encoder
cannot keep up, it repeats missed frames to preserve
playback timing and logs dropped inputs; lower clip resolution or frame rate if
this happens. Closing the game flushes any pending clip. The launcher displays available captures on each unlocked
entry and opens them on click. Earlier unlocks have no retroactive media;
the launcher marks those entries as awaiting a capture.

These preferences are persisted in `settings.plist`:

| Key | Values | Default |
| --- | --- | --- |
| `AchievementScreenshotFormat` | `png`, `bmp` | `png` |
| `AchievementScreenshotResolution` | `native`, `1080p`, `720p` | `native` |
| `AchievementClipResolution` | `1080p`, `720p`, `540p` | `1080p` |
| `AchievementClipFps` | integer `60`, `30` | `60` |
| `AchievementClipQuality` | `standard`, `high`, `ultra` | `high` |

**Sound on/off** and **Motion on/off** in the footer control the ambience.
Launcher SFX follow the saved master volume and stop during gameplay or when
the launcher loses focus. Audio uses an available `paplay`, `pw-play`, `aplay`
or `afplay`; the launcher also works without an audio player. Set
`CODWAWZPC_REDUCED_MOTION=1` to start with animation disabled. These two launcher
toggles apply for the current session.

**Launcher icon** in the footer opens four illustrated choices: **Wunderwaffe**,
**Undead**, **Field Radio**, and **Mystery Box**. The Mystery Box emblem includes
the Ray Gun, Wunderwaffe DG-2, M2 flamethrower and Monkey Bomb around an open box.
Select an icon with the mouse or Tab and Space / Enter. The window and its
utility dialogs update immediately; the footer previews the active icon.
Your choice persists in `$XDG_CONFIG_HOME/codwawzpc/launcher.plist` (normally
`~/.config/codwawzpc/launcher.plist`), or `userdata/config/launcher.plist` in a portable folder. Wunderwaffe is the default. A missing or
damaged icon preference falls back to that default. A save failure keeps the
previous choice and displays an error in the chooser. Native desktop shells
decide where window icons appear. If you have installed the applications-menu
entry below, selecting an icon also updates that entry.

The globe button in the scene's top-right corner sets the **launcher language**:
English, Français, Deutsch, Italiano or Español, each listed by its own name, or
**System language**, the default. System language follows `LC_ALL`,
`LC_MESSAGES` or `LANG` (and `LANGUAGE` unless the locale is C) and falls back to
English. The main window relabels as soon as you choose; other open windows
change when you reopen them. The choice is stored as `Language` in the same
`launcher.plist` as the icon, and a damaged entry falls back to System language.
This setting only changes the launcher. The game's own language is in
Settings & controls → Display & sound. Translations live in
`assets/locale/<code>.json`, keyed by the English text that `tr()` and `N_()` mark
in the Python sources. `tests/test_launcher.py` fails if a catalog misses a
marked string or changes its `{fields}`. Achievement names, map names and host
output stay as the game and host write them.

## Start

Requires the project's Rust build dependencies plus Python 3.10+, Tk and Pillow.
Install your distribution's Tk/Python bindings and Pillow, or install
`launcher/requirements.txt` in a virtual environment after installing Tk.
On Debian/Ubuntu the system packages are `python3-tk` and `python3-pil`;
on Arch they are `tk` and `python-pillow`.

```sh
make dev
# Same command:
make launcher
# Already built:
./target/release/codwawzpc launcher
```

With a virtual environment, use `make dev PYTHON=.venv/bin/python`, or set
`CODWAWZPC_PYTHON` to that interpreter when invoking the executable directly.
The ordinary CLI and `make run` remain available without Python.

For a source build, add **Call of Duty: Zombies** to your Linux applications menu:

```sh
make install-desktop
```

This installs `codwawzpc.desktop` under `$XDG_DATA_HOME/applications` (normally
`~/.local/share/applications`), with your selected launcher icon. It registers
the current checkout and Python interpreter, so keep the checkout in place or
rerun installation after moving it. No administrator privileges are needed.
The launcher uses a matching window class so GNOME associates its window with
the menu entry. Remove that desktop file to unregister the app.

Direct entry from another directory works too:

```sh
python3 /absolute/path/to/codwawzpc/launcher/main.py --binary /absolute/path/to/codwawzpc/target/release/codwawzpc
```

## Actions

- **Import game:** opens an illustrated installation guide with three steps,
  live status, selected archive, storage paths and troubleshooting. Choose your
  own unencrypted iPhone 1.5.0 IPA using **Choose game IPA**. Validation and
  copying belong to the Rust host. A rejected import preserves the prior
  installation; the launcher rechecks it before enabling Play again.
- **Play:** available only after a successful installation check. The host
  verifies again before running. The launcher stays responsive while the game
  runs and returns to readiness when it exits. Overlapping operations are blocked.
- **Settings & controls:** fullscreen, original touch input, mouse sensitivity,
  inverted mouse look, auto-aim (Off by default, or the game's Low, Medium or High), volume, the
  host's 60/45 frame-rate cap, the startup logos (skipped by default) and the
  language (the game's own choice by default, or English, Français, Deutsch,
  Italiano or Español; see [docs/localisation.md](../docs/localisation.md)). Save settings
  writes the same XDG `settings.plist` as the CLI, atomically. Edits apply on
  the next launch. Reset defaults selects defaults; Save settings applies them.
  Invalid settings are reported and Play remains disabled until repaired.
  Display & sound, Graphics, Mouse & touch and Field controls each have their
  own paper page. Graphics sets render scale, widescreen, texture filtering,
  gamma, contrast and sharpening; Original picture restores the last three. Inline explanations describe defaults, ranges and input behavior.
- **Saved games:** listing of the game's Documents directory, with
  filenames, local modification times, sizes, Refresh and Open save folder.
  Empty and inaccessible folders have explicit guidance. **Export save…**
  copies your saved game to a `.i3d` file you choose. **Import save…** asks
  for confirmation, then the host checks the file's checksum, copies the
  current save to `save-backups/` in the data directory and installs the
  file. **Reset progress…** asks for confirmation, then asks whether to back
  up the save and `achievements.plist` to `save-backups/` (Yes, No or
  Cancel) before deleting them so the next
  game starts from scratch. All three are host commands (`export-save`,
  `import-save`, `reset-save`), are blocked while another operation runs and
  report the host's result. See
  [the save format](../docs/saves.md).
- **Verify files / View logs:** repeat the host check, view its output and the
  tail of the runtime log, or open the log directory. Ctrl-L opens logs and
  Ctrl-R refreshes an open log viewer.

F11 toggles launcher fullscreen; Escape leaves it. The game fullscreen preference
is separate. The minimum launcher size is 1000×640. Scene artwork retains its
proportions; button plates use scalable edges to make room for larger labels.
The three illustrated utility windows open at 1120×740 and support 900×620.
Their paper pages scroll while navigation and footer actions stay visible.
Artwork shrinks at smaller sizes to keep every section accessible.

Closing during import waits for the transactional import to finish. Closing
while the game runs asks before stopping it. Read-only verification can stop
on close. Child output uses a bounded queue and never blocks the UI thread.

The host and launcher share the documented XDG directories for source builds. Portable builds instead keep their data, settings and logs in `userdata` beside the launcher. Saves are under
`$XDG_DATA_HOME/codwawzpc/touchHLE_sandbox/com.activision.callofduty/Documents`.
No game files are bundled or downloaded.

Gameplay bindings are edited in Settings & controls → Field controls. Enter input names such as `Key:W`, `Mouse:Left`, or `Pad:RightTrigger`, separated by commas, and choose normal or swapped sticks. Save reports conflicts, Reset defaults restores defaults, and Close cancels unsaved changes. Menu controls and recovery shortcuts stay fixed.

## Keyboard and controller

Every launcher screen works without a mouse. The launcher focuses PLAY on start,
or Import game until an installation is verified.

| Keyboard | Controller | Action |
| --- | --- | --- |
| Arrow keys | D-pad / left stick | Move to the nearest control in that direction |
| Enter / Space | A (Cross) | Press the focused control |
| Escape | B (Circle) | Close the window, returning focus to the control that opened it; on the main screen, leave fullscreen |
| Ctrl+Page Up / Page Down | LB / RB | Previous / next section of a field manual |
| Page Up / Page Down | Right stick | Scroll the field manual page |
| — | Start | Play (main screen) |
| Tab / Shift-Tab | — | Next / previous control, in reading order |

Left and Right adjust the focused slider, sensitivity spinner, frame-rate cap
or auto-aim choice; Up and Down move between rows. Enter or A also cycles a
choice. In tables, Up and Down move the selected row and continue past the
ends. Left and Right stay on their row, so they never jump diagonally.

Field manual pages scroll toward controls below the fold before focusing them,
so the text in between stays readable. Right from the section list enters the
page at its first visible control. When there is none, the page itself takes
focus with a red border; Up and Down then scroll it. Unlock screenshots
and clips in Achievements are buttons too. Hold a direction to repeat.

Controllers are read directly from Linux evdev (`/dev/input/event*`) with no
extra Python packages. Any device reporting gamepad buttons works, including
Xbox, PlayStation and most generic pads. Controllers connected while the
launcher is open are detected within two seconds. Desktop sessions normally
grant access through udev's `uaccess` rule. If a controller is ignored, check
that your user can read its event device. The launcher reacts to the controller
only while it has focus, so it ignores input meant for the running game.
Native file choosers and message boxes keep Tk's own keyboard handling. A
controller moves between their buttons and presses them; B cancels. In a file
chooser, Up from the name field reaches the file list, where the d-pad selects
and A opens a folder or chooses a file.

## Distribution and checks

Startup decodes independent PNGs in a bounded four-thread pool and loads only
the selected app icon. Animation frames retain the original rendering and
resolution; they are cached on first use and prepared one at a time after the
scene appears. Sound synthesis runs only when each sound is first used, and
reuses its unchanged PCM until volume changes. Repeated redraws at the same
viewport size keep the existing scene. Manuals reuse their paper textures and
decoded illustrations, and achievement pages load icons only for visible entries.
No existing image, asset dimensions, resampling quality or animation detail was
reduced. See [startup measurements](../docs/launcher/performance.md).

Measure fresh-process startup with disposable preferences and no game import:

```sh
python3 scripts/benchmark_launcher.py --runs 5
```

This requires a display, Tk and Pillow. It reports construction, first scene,
unchanged-viewport redraw time and a SHA-256 of the composed scene pixels.

Install this `launcher/` directory beside the executable, or under
`share/codwawzpc/launcher/`. Include every Python module in `launcher/` (including
`effects.py`, `sound.py`, `icons.py`, `i18n.py`, `preferences.py`, `desktop.py`, `field_manual.py`, `navigation.py` and `gamepad.py`), all PNGs under `assets/` and the translations in `assets/locale/`.
The four `app-icon-*.ico` files provide multiple sizes for desktop shortcuts.
The development checkout is a fallback resource location. The ordinary host
resources and their notices are still required as described in the main README.
Generated masters and export tooling are development files and need not ship.

```sh
python3 tests/test_launcher.py
make test-launcher
```

Native interaction checks require an X11 or Xwayland display and ImageMagick's
`import` command for direct window captures. They use disposable XDG directories
and a clearly identified host fixture for successful import/play states; real
host checks exercise missing data and invalid imports. They do not alter your
installed game or saves. [Captures and measured checks](../docs/launcher/README.md)
record the verification scope.

See [the artwork guide](../docs/launcher-assets/README.md) for generated assets,
generated masters and the portable integrity specification; private generation
prompts are not required.
