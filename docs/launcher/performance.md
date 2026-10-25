# Launcher startup measurements

Measured on 2026-10-24 with Python 3.14.8, Pillow 13.0.0,
and the native Tk launcher on Linux. Five fresh processes per version used
isolated XDG state, no imported game, and a 1440×844 window (1440×772 scene plus
72px footer). Measurements cover Tk creation through the first scene; they
exclude Python interpreter/import time and Rust host startup or verification.
Filesystem caches were not flushed. Desktop activity can affect the results.

| Median operation | Before | After |
| --- | ---: | ---: |
| Window construction | 470.96 ms | 291.51 ms |
| First composed scene | 1143.23 ms | 698.19 ms |
| Unchanged-viewport render call | 510.13 ms | 0.01 ms |

The first scene appeared **38.9% sooner** in this local measurement.
Repeated render calls reuse the existing scene rather than recomposing it.
Run `python3 scripts/benchmark_launcher.py --runs 5` with a display to repeat
the current-version measurement. Raw samples and native screenshots remain local;
every historical before/after sample had the same composed RGBA pixel hash.

Startup now decodes independent PNGs with at most four threads, prepares only
the selected app icon, and synthesizes audio on first use. Full-quality animation
frames are prepared incrementally after the first scene and cached at their
original viewport scale. Opening Achievements loads only the current map's
icons; reopened manuals reuse decoded illustrations and paper textures.

All **132 preexisting PNG files remained byte-identical**, including dimensions,
alpha and color pixels. An independent comparison against the original Python
implementation also matched every decoded scene asset, both scene layers at
three viewport sizes, and every original animation frame at three scales.
Audio tests compare synthesized WAV payloads against the original PCM exactly.
The full native interaction checks and 32 launcher unit tests passed.

The [new trophy](../../launcher/assets/achievement-trophy.png) is an additional
1254×1254 transparent PNG, copied byte-for-byte from its generated master. The
asset exporter reproduces all 26 runtime PNG exports, preserving the trophy's
full generated resolution. The local native verification script exercises its
full-size and compact pages.
