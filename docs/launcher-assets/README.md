# Shi No Numa launcher artwork

Individual assets for the selected [Shi No Numa field-radio concept with Wunderwaffe](../launcher-concepts/04-shi-no-numa-field-radio-wunderwaffe.png), generated with the built-in image generation tool. They now power [the native launcher](../../launcher/README.md). The composed preview here remains the original static layout study; [native verification instructions](../launcher/README.md) document the implementation.

- [Composed preview](composition-preview.png)
- [Individual asset contact sheet](asset-contact-sheet.png), showing transparency against light and dark backgrounds
- [Sibling launcher reference review](reference-review.md)
- [Monkey sound preview](monkey-windup.wav), synthesized from the runtime sound code at 65% volume
- [Portable asset specification](../../launcher/artwork/asset-spec.json)
- [Export manifest](../../launcher/assets/manifest.json), including original and export sizes, SHA-256 digests and alpha measurements

## Individual assets

| Asset | Purpose |
| --- | --- |
| [swamp-background.png](../../launcher/assets/swamp-background.png) | Clean opaque swamp, timber hut and wet workbench; no title, foreground weapon, lamp or controls. |
| [wunderwaffe.png](../../launcher/assets/wunderwaffe.png) | Separate complete DG-2 interpretation, with blue electrical tubes. |
| [lantern.png](../../launcher/assets/lantern.png) | Separate lit kerosene lantern. |
| [monkey-bomb.png](../../launcher/assets/monkey-bomb.png) | Transparent seated clockwork monkey with brass cymbals; runtime motion uses cached rotation frames. |
| [maintenance-rail.png](../../launcher/assets/maintenance-rail.png) | Blank generated brass-edged gunmetal footer rail, sliced through its quiet center at runtime. |
| [rail-button.png](../../launcher/assets/rail-button.png) | Blank generated tactile footer button plate; live labels and icons remain separate. |
| [rail-switches.png](../../launcher/assets/rail-switches.png) | Generated lit and unlit brass switch poses for Sound and Motion. |
| [radio-housing.png](../../launcher/assets/radio-housing.png) | Empty olive field-equipment case with straps and buckles. |
| [play-plate.png](../../launcher/assets/play-plate.png) | Blank distressed crimson button plate. |
| [utility-plate.png](../../launcher/assets/utility-plate.png) | Blank olive utility button plate, reused for the three secondary actions. |
| [title.png](../../launcher/assets/title.png) | Transparent stacked title lettering with the iPHONE PC PORT subtitle. |
| [status-lamp.png](../../launcher/assets/status-lamp.png) | Amber pilot lamp for the readiness area. |
| [icon-settings.png](../../launcher/assets/icon-settings.png) | Cream cog glyph. |
| [icon-saves.png](../../launcher/assets/icon-saves.png) | Cream floppy-disk glyph. |
| [icon-import.png](../../launcher/assets/icon-import.png) | Cream folder glyph. |
| [field-radio.png](../../launcher/assets/field-radio.png) | Transparent radio maintenance kit for settings. |
| [field-archive.png](../../launcher/assets/field-archive.png) | Transparent steel document box for saved games. |
| [field-supply.png](../../launcher/assets/field-supply.png) | Transparent supply crate for installation and import. |
| [field-paper.png](../../launcher/assets/field-paper.png) | Opaque, quiet aged-paper texture for scrollable manual pages. |
| [achievement-trophy.png](../../launcher/assets/achievement-trophy.png) | Full-resolution transparent survivor trophy for the Achievements manual: helmeted undead skull, blue electrical coils, brass laurel and an ammunition-box plinth. |
| [app-icon-wunderwaffe.png](../../launcher/assets/app-icon-wunderwaffe.png) | Brass medallion with blue electrical coils; default app icon. |
| [app-icon-undead.png](../../launcher/assets/app-icon-undead.png) | Ivory zombie skull with amber eyes on crimson metal. |
| [app-icon-field-radio.png](../../launcher/assets/app-icon-field-radio.png) | Olive field radio with an amber tuning dial. |
| [app-icon-mystery-box.png](../../launcher/assets/app-icon-mystery-box.png) | Open mystery box with Ray Gun, Wunderwaffe DG-2, M2 flamethrower and Monkey Bomb. |

All assets except the swamp background and field paper have genuine generated alpha. Each master was generated in its own call from the reference; the rail switches intentionally share one matched two-state sheet. Original unmodified PNGs live in [launcher/artwork/sources](../../launcher/artwork/sources/). These are generated interpretations, not extracted game files or official artwork.

The app-icon prompt set requests one square, brass-rimmed WWII medallion per
subject, with strong silhouettes and transparent surroundings: blue coils,
an amber-eyed skull, an olive radio, and the mystery box with all four special
weapons. The weapon selection follows the project's
[game achievement descriptions](../achievements.md). Each icon exports as a
256×256 RGBA PNG and an ICO containing 16, 24, 32, 48, 64, 128 and 256px images.
The launcher advertises several sizes to the window manager and lets users
switch icons from its footer. The ICO exports are also available for shortcuts.

The asset-export script crops transparent outer margins, proportionally resizes and adds clear padding. It preserves the generated alpha and does not repaint pixels or remove backgrounds. For the three rail assets it uses the measured visible bounds so negligible generated fringe does not distort their proportions. The manifest also records content bounds measured at alpha >= 8/255. The preview crops to those measured bounds with a small halo margin. Lamp glow remains part of the lamp asset. The complete rail generation prompts are in `launcher/artwork/rail-prompts.json` (local, intentionally ignored).

The new trophy is marked `preserve_original` in the asset specification: the
exporter copies its 1254×1254 PNG byte-for-byte, including its generated alpha.
Only display-time fitting crops clear margins and scales it to the sidebar.
The artwork was generated for this project; its prompts remain local.
Existing artwork exports retain
their original dimensions and pixels.

The title is decorative artwork. PLAY, Settings & controls, Saved games, Import game and readiness labels are live native text. Hover, pressed, disabled and keyboard-focus presentation is composed over the same blank plates. No button text is embedded in those PNGs.

## Rebuild and verify

Requires Python 3 with Pillow. Preview labels use installed URW Base35 or DejaVu fonts. The scripts run from any working directory.

```sh
python3 launcher/artwork/build_assets.py
python3 scripts/verify_launcher_assets.py
```

The exporter never changes the generated masters. It rebuilds the exported assets, manifest and both review images. Verification checks every source and export against the manifest, requires the full asset set and real transparency, and decodes both previews. Visual review is recorded in the local `launcher/artwork/GATES.md` ledger.

Generation prompts and the gate ledger remain local under the repository's existing ignore rules. Rebuilding and verifying the exported artwork uses the portable asset specification and does not require those local agent files.
