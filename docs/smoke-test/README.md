# Four-map smoke test — 2026-10-21

**Result: four-map CLI smoke-tested prototype.** All four maps load into
solo play and respond to the desktop controls. Later-round behavior,
long-term stability and multiplayer remain unverified.

Machine: Arch Linux, kernel 7.1.4, AMD Radeon RX 6700S (Mesa 26.2.5,
radeonsi), GNOME Wayland (SDL on XWayland). Build: `codwawzpc` at the commit
that adds this file. Game: `COD_Zombies_v1.5.0_os30.ipa` (IPA SHA-256
`a2b6af38…d782`), imported with `codwawzpc import`.

## Method

Each check ran the real `codwawzpc run` with private XDG directories. Input
came from a timed script (`CODWAWZPC_INPUT_SCRIPT`) that injects SDL key,
mouse-button, mouse-motion, wheel, resize and focus events into the same
queue as physical input, and menu touches through the guest's touch path.
Frames were saved with `CODWAWZPC_CAPTURE_DIR`. Evidence:

- **Movement and look:** view changes between frames captured before and
  after each input (horizontal image shift measured for look).
- **Combat:** HUD counters — the magazine and reserve counts after a click
  and a reload, the grenade count after G, the points after a purchase.
- **Sound:** the game's own PipeWire stream, recorded with
  `parec --monitor-stream` and measured, not listened to.

The game window was on screen during the runs, but no person was playing.

## Checklist

| Check | Nacht der Untoten | Verrückt | Shi No Numa | Der Riese |
| --- | --- | --- | --- | --- |
| Loads into solo play (~11–15 s) | pass | pass | pass | pass |
| Rendering updates | pass | pass | pass | pass |
| Movement (W, or S in Shi No Numa) | pass | pass | pass | pass |
| Mouse look, right then back | pass | pass | pass | pass |
| Fire (left click) | pass | pass | pass | pass |
| Reload (R) | pass | pass | pass | pass |
| Aim down sights (right click) | pass | pass | pass | pass |
| Grenade (G) | pass | pass | pass¹ | pass |
| Knife (V) | pass | pass | pass | pass |
| Use (E) | pass² | button pressed³ | — | — |
| Game sound (seconds above −50 dBFS) | 55/69 | 55/69 | 56/69 | 55/69 |
| Escape pauses, Return to Main Menu → Confirm | pass | pass | pass | pass |

¹ A first run threw the grenade at a wall it had just walked into and the
player died (Game Over screen, Return to Main Menu worked); rerun walking
backwards passed.
² Bought the Kar98k wall weapon: 500 → 300 points, weapon equipped, "Buy
Ammo" prompt shown. The mouse wheel then switched to the pistol and back.
³ The hand button visibly pressed at an M1 Garand (600 points, unaffordable).

Once, on Nacht der Untoten:

| Check | Result |
| --- | --- |
| Clicking menus with the released pointer (2× window) | pass |
| Resize to 1200×500 and 640×640, aspect preserved | pass |
| Focus loss while holding W and fire: input released, mouse uncaptured; frames 0.9 s and 2.9 s later differ by 0.65 % RMSE (flicker only) | pass |
| Focus regained: Tab and W work again | pass |
| Native setting (Invert Y Axis) persists across a restart | pass |
| `--fullscreen` (1600×900, pillarboxed) and `--fps` (game ~45 FPS, compositor 60) | pass |
| Co-Op shows "Connection not available"; Extras menus work offline | pass |

## Not verified

- Automatic fire with an automatic weapon (needs points for one; the
  double-tap-and-hold gesture is implemented from the game's own help text).
- Barricade repair with E (zombies did not reach the chosen window in time;
  E was verified on a wall weapon, which uses the same button).
- How the controls feel to a person, real window-manager focus changes
  (a synthetic SDL focus event was used), and sound by ear.
- Later rounds, long sessions, multiplayer, and the upstream iOS TestApp
  integration test (needs touchHLE's iOS LLVM toolchain).

## Local evidence

Game screenshots and recordings remain private and are excluded from the public
source repository. The checklist above records the historical test results.
