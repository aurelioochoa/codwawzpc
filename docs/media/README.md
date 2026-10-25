# README media

These files were created specifically for the public README. Personal gameplay
captures, downloaded reference art and extracted game files remain excluded.

| Files | Origin |
| --- | --- |
| `banner.png` | Original project artwork generated with the built-in imagegen tool: a jade swamp outpost, warm lantern, electrical prop, cymbal monkey and the project title. |
| `launcher.png`, `launcher.gif` | Actual native Tk launcher window, with generated project artwork and the real mist, rain, lantern and electrical effects. Readiness is explicit demo state. |
| `settings.png`, `capture-settings.png` | Actual native settings pages using disposable defaults and a demo player name. |
| `achievements.png`, `achievement-browser.gif` | Actual native achievement browser using the bundled catalog and no personal progress. |
| `nacht-der-untoten.png`, `verruckt.png`, `shi-no-numa.png`, `der-riese.png` | Fresh frames rendered by the real Linux host in solo play. |
| `gameplay.gif` | Fresh rendered solo gameplay from Nacht der Untoten, encoded as a silent looping GIF. |

Gameplay was captured with a separate copy of a user-supplied installation and
disposable configuration, state and saves. The IPA, installed game files,
intermediate capture frames, raw recordings and logs are not published. No
personal save progress or player identity is shown. These rendered documentation
captures do not supply the game; every user must import their own IPA.

The GIFs preserve actual UI or gameplay frames. They are encoded for README
loading speed. Click the PNG links for full-resolution stills. The banner is
generated promotional artwork; the screenshots and GIFs show the application.

## Reproduce launcher captures

From the repository root, with a graphical display, Python/Pillow, ImageMagick
and FFmpeg available:

```sh
python3 scripts/capture_readme_media.py
python3 scripts/verify_readme_media.py
```

The capture script uses temporary XDG directories, synthetic installation
readiness and an empty achievement catalog. It never launches a game or reads
your installed game, saves or private captures. It advances native scene effects
at normal speed and navigates the real achievement pages. Replacing media also
requires reviewing the new files and updating their digests in `manifest.json`.

Gameplay reproduction requires your own game installation. The host's existing
`CODWAWZPC_INPUT_SCRIPT` and `CODWAWZPC_CAPTURE_DIR` tools can capture presented
frames; their format is documented in `src/window/script.rs`. Keep raw output
private and review only the final documentation files for publication.

`manifest.json` records the reviewed files, their dimensions, animation duration
and SHA-256 digests. The source-publication audit allows only these explicit
documentation filenames; arbitrary media and personal capture libraries remain
outside the publication boundary.
