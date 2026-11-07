# Controller weapon input — 2026-11-07

The delayed re-aim and interrupted sprint reload were reproducible defects
in the port's input adapter. The controller bindings delivered the expected
actions. The comparison here is against the supported **iPhone 1.5.0** game,
whose weapon logic runs in the host, rather than the console World at War.

## Original game behavior

The original player accepts reload while moving. A reload requested during
sprint returns the player to walking and starts the reload animation;
forward movement can continue. Reloading and continuing the sprint pose
at the same time are different requests.

The game controls when an animation can transition to or from aiming.
Firing, lowering the weapon and other weapon animations can temporarily
prevent a transition. The host now follows those rules without adding its
own one-second retry delay or changing weapon animation speed.

These findings come from the accepted executable and gameplay traces:
`CKeyboardInput` posts `Event_PlayerAction` at `0x15893c`; its dispatch path
at `0x213f8c` calls the entity synchronously. The player action handler at
`0x2088cc` writes animation inputs through `0x150e08`. These are state
inputs, not a queue of future toggles. Reload action 7 uses the original
weapon reload path at `0x20878c`.

## Port fixes

- Retry a held aim or sprint request on the next game frame if the original
  animation has not accepted it. Previously a rejected request suppressed
  further attempts for one second, including after a quick trigger release
  and press.
- Give aim priority over sprint. End sprint first, then request aim when
  the game reaches walking. Previously aim and another sprint request
  were posted from the same player state; aim alone is ignored during sprint. Action
  6 starts sprint rather than toggling it off. To end it while the stick is
  held forward, briefly release the game's movement pad, then restore the
  held direction when the game leaves its run state.
- Cancel the controller's clicked sprint when aiming, firing, reloading,
  starting a grenade, using the knife or switching weapons. Previously the
  latch remained set while the stick was forward, so it restarted sprint
  during reload or after releasing aim. Another sprint click now starts a
  new sprint; forward movement continues throughout.

## Verification

The old and updated executables ran the same scripted SDL controller
sequence on Nacht der Untoten with the starting pistol, the default 60 FPS
cap, separate disposable saves, default bindings and aim assist off.
Physical controllers were excluded from the test process using SDL's
`SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT` hint so live inputs could not
affect the replay.
The replay asserts synthetic focus before controller events so desktop
focus changes cannot silently discard its inputs.

The [input sequence](smoke-test/controller-weapon-input.txt) aimed, fired,
released aim, quickly pressed aim again, then
reloaded while walking and while sprinting. Native player-state and action
traces were compared with captured gameplay and ammo counters. The old
build restarted sprint after reload had begun and failed to regain aim
within the second quick aim hold. The updated build regained the aim state
and completed the sprint reload while continuing forward movement: reserve
ammo fell from 31 to 30, whereas the interrupted old-build reload left it
at 31.

A further check clicked sprint again, aimed out of sprint, fired and
quickly re-aimed. The updated trace reached aim before that shot, and its
final capture showed the sights raised. The same trace assertions reject
the old executable because it restarts sprint during the reload.
Trace logging uses
`CODWAWZPC_TRACE=touchHLE::codwawzpc::native`; frame captures use
`CODWAWZPC_CAPTURE_DIR`. Pass the sequence through
`CODWAWZPC_INPUT_SCRIPT` with `--controls=desktop --auto-aim=off` and private
XDG directories pointing to an imported installation.

Unit regressions cover trigger release and re-press, retry after rejected
stance requests, aim priority, sprint permission and cancellation of the
controller latch without losing movement. Gameplay validation is limited
to this weapon and map; the original per-weapon animation restrictions
remain in effect.
