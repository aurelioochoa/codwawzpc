# Native launcher verification

The launcher uses generated project artwork with native Tk controls. Gameplay
screenshots, videos, local storage screenshots and private verification records
are excluded from the public repository. The generated trophy
[contact sheet](achievement-icons-contact.png) is included.

## Reproduce checks locally

```sh
python3 -m unittest discover -s tests -p test_launcher.py
make build
python3 scripts/verify_cli.py
python3 scripts/verify_launcher.py
python3 scripts/verify_launcher_assets.py
python3 scripts/verify_achievement_gallery.py
```

Native UI checks require a graphical display and ImageMagick's `import` command.
They use temporary XDG directories, an explicit synthetic host fixture and a
synthetic save. The real executable also exercises missing-installation and
invalid-archive behavior. No game files are needed for these checks.

The UI script creates local captures of window sizes, installation, settings,
controller navigation, saves, achievements, capture preferences and icon choices.
Those captures and JSON reports are ignored. They verify UI wiring, rendering
and process lifecycle; they do not demonstrate a real game import or gameplay.

Backend tests cover preference validation, atomic writes, process output and
termination, controller input, icon selection, audio synthesis and reduced motion.
Artwork checks verify the source and export digests, dimensions, alpha and complete
106-icon coverage without private prompts or official reference downloads.

The screenshot and clip features remain available to every user. Captures are
written to that user's local XDG application storage, not to this source tree.
See [launcher behavior and capture settings](../../launcher/README.md).
