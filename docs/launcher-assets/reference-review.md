# Shi No Numa launcher asset direction

Selected reference: [04 — Shi No Numa Field Radio with Wunderwaffe](../launcher-concepts/04-shi-no-numa-field-radio-wunderwaffe.png).

This stage produces individual artwork layers and a static composition preview. The preview is an asset-layout study; runtime actions are implemented in a subsequent launcher stage.

## Sibling launchers inspected

| Project | Sources inspected | Pattern carried forward |
| --- | --- | --- |
| `re4mepc` | `docs/launcher-preview.png`, `docs/launcher-assets.md`, `re4pc/launcher_ui.py` | Clean background artwork, separate title and utility icons, live text and focusable controls. |
| `redegpc` | `docs/launcher/overview-1440x810.png`, `launcher/artwork/export_assets.py`, `launcher/README.md` | Preserve generated originals, export reusable transparent assets, record dimensions and SHA-256 metadata, inspect assets on light and dark backgrounds. |
| `bio3dpc` | `docs/launcher-preview.png`, `docs/launcher-art.md`, `docs/launcher-asset-prompts.md`, `bio3dpc/launcher.py` | Tactile equipment becomes the interface; blank generated control skins support live labels, pointer states and keyboard focus. Separate environmental and lamp layers allow later animation. |

## Asset contract

The environment is opaque. Weapon, lantern, title, radio case, blank button plates, status lamp and utility glyphs are separate transparent PNGs. Each artwork asset is generated individually, preserving its original alpha. The radio front and button plates have no embedded labels. Settings, saved games, import and readiness text remain live launcher content. The title lettering is decorative artwork.

The original concept is retained. Generated masters are copied into `launcher/artwork/sources/`; reusable exports and their manifest belong in `launcher/assets/`. Exporting may crop clear margins and resize proportionally, but does not repaint, remove backgrounds or synthesize artwork. Preview composition only places the resulting assets and labels.

Preserve the concept's dark left title area, misty jade swamp, timber stilt huts, warm lantern at right, electric-blue Wunderwaffe tubes and low horizontal olive radio with crimson Play plate. No game files are required for this artwork stage.

## Utility-screen redesign

The settings and installation pages in `redegpc/launcher/ui.py` and its native
installation capture supplied the explanatory structure: concrete import
requirements, three ordered steps, live readiness, local storage, and useful
failure guidance. `re4mepc/re4pc/case_dialog.py`, its settings capture and
`tools/preview_dialogs.py` supplied the illustrated sidebar, paper reading
surface, scroll behavior and persistent action-footer pattern.

The codwawzpc implementation retains its own olive field-radio palette and
native Tk controls. Four new individual built-in generations supply a radio
maintenance kit, steel archive box, wooden supply crate and quiet paper texture.
Their local prompt set is `launcher/artwork/utility-prompts.json`. Every master
and exported asset is saved in this project and included in its integrity
manifest. The reference games' settings and capabilities were not copied;
explanations describe only the preferences and import requirements this host
actually supports.
