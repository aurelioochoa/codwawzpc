# Graphics and iPad HD asset investigation

The iPad HD edition contains higher-resolution weapon textures and normal
maps that could improve the existing four-map iPhone host. Static analysis
of user-supplied copies on 2026-10-26 confirmed these differences. Importing
HD assets, running the HD executable and rendering its additional lighting
are **not implemented or gameplay-tested**.

The proposed starting point is a selective, optional asset import while
retaining the supported iPhone executable and all four maps. Replacing the
whole installation with the HD edition would lose content and invalidate
the current executable-specific patches.

## Builds examined

These findings describe the exact copies identified below; they should not
be assumed to apply to every release or repack with the same version label.

| Field | iPhone | iPad HD |
| --- | --- | --- |
| Bundle identifier | `com.activision.callofduty` | `com.activision.kiwipad` |
| Bundle version | `1.5.0` | `1.5.0` |
| Minimum iOS | `3.0` | `3.2` |
| Architecture | 32-bit ARMv6 | 32-bit ARMv6 |
| Mach-O encryption flag | `cryptid=0` | `cryptid=0` |
| Map archives present | Nacht, Verrückt, Shi No Numa, Der Riese | Nacht, Verrückt |
| Archive files | 9 | 5 |
| Decoded archive entries | 76 | 45 |
| Texture resource records | 384 | 263 |

All 17 payload hashes in the supported [iPhone profile](../src/codwawzpc/profile.rs)
match the examined iPhone copy. Both executables are unencrypted, but their
bytes and addresses differ. The HD executable is not an alternative accepted
by the current importer.

The HD copy contains `main.dz`, `nacht.dz`, `nacht_full.dz`, `verruckt.dz`
and `verruckt_full.dz`. The corresponding iPhone archive names begin with
`_`, and its installation also has `_shinonuma.dz`, `_shinonuma_full.dz`,
`_derriese.dz` and `_derriese_full.dz`.

The HD data includes a small Shi No Numa achievement definition, but no
Shi No Numa level archive. That definition does not establish map support.
The shared `1.5.0` version label does not establish equivalent content.

### Source fingerprints

| Input | SHA-256 |
| --- | --- |
| iPhone IPA | `a2b6af381d9975949c572a1c477aa6d5de71a5d421de4114f7daca154432d782` |
| iPhone executable | `4c940898943a1ac63318083b17bc04219251ad4b3cd2b846eaf41c1702f98f0d` |
| HD IPA | `9f48358796bbf6aec552465c620aa1cdb8736c6e7e9fad7ae5ae04c2acbb299c` |
| HD executable | `7e5b3253f4e85c3f70c4c25781f5174254745429c68c2d3ed4d98ae2444ba90c` |

IPA hashes identify the examined packaging; executable and resource hashes
are needed when comparing repacks.

## Confirmed asset differences

### Weapon textures and normal maps

Several weapon atlases increase from 256×256 to 512×512: twice the width
and height, or four times the texels. The HD edition also supplies matching
normal maps that describe surface direction for lighting.

| Representative atlas | iPhone texture ID | iPhone stored size | HD diffuse ID | HD stored size | HD normal-map ID |
| --- | --- | --- | --- | --- | --- |
| Pistol | `a8df092a` | 256×256 | `8fd3da80` | 512×512 | `8fd3da8b` |
| Rifle | `6a8066fe` | 256×256 | `91196eb3` | 512×512 | `91196ebe` |
| Weapon atlas | `00cb8d7e` | 256×256 | `0251c154` | 512×512 | `0251c15f` |

These labels describe the decoded images. IDs are hashed resource names,
not recovered source filenames or a completed runtime replacement manifest.
Stored sizes describe the decoded texture storage, which can differ from
the logical dimensions in a resource header.

Local side-by-side comparisons displayed decoded source pixels at equal
sizes using nearest-neighbor enlargement. No AI upscaling or sharpening was
applied. The examples show additional engraving, grip and material detail,
but some HD diffuse artwork removes lighting already painted into the
iPhone texture. Using it without the intended normal-map lighting can make
the result look flatter. Some atlas layouts also change, so texture
coordinates must be checked before replacement.

The player bundle, `ingame/player/player.group.bin`, has:

| Measurement | iPhone | HD |
| --- | --- | --- |
| Decoded bundle bytes | 5,052,245 | 20,641,600 |
| Texture records | 52 | 86 |
| Model records | 108 | 95 |

Forty player textures share a resource ID across the editions, and all
forty have byte-identical serialized texture bodies. Twelve iPhone texture
IDs are absent from the HD bundle; 46 HD IDs are absent from the iPhone
bundle. The HD-only set includes upgraded diffuse atlases and normal maps;
it does not represent 46 proven interchangeable texture upgrades.

### Models, materials and animations

The HD player bundle changes models, materials and animations as well as
textures. There are 94 shared player model IDs; none has a byte-identical
serialized body. This establishes changed model resources, but does not by
itself establish a higher polygon count or compatibility with the iPhone
animations, materials and skeletons.

In `ingame/zombiesmodelsger/zombiesmodelsger.group.bin`, all five shared
model bodies change. The main diffuse texture resource is replaced, while
its compressed storage remains 1024×1024, and the HD bundle adds a normal
map. Its larger bundle is therefore not evidence of a resolution increase
for every texture.

### Environment textures

Most matching environment textures are byte-identical:

| Environment bundle | iPhone textures | HD textures | Shared IDs | Identical serialized texture bodies | HD-only IDs |
| --- | --- | --- | --- | --- | --- |
| Nacht | 26 | 31 | 22 | 21 | 9 |
| Verrückt | 36 | 37 | 36 | 35 | 1 |

Counts refer to each map's `environment/environment.group.bin`. There are
additional or renamed textures and changed models and materials, but the
examined edition is not a comprehensive high-resolution map texture pack.

There are no HD level archives for Shi No Numa or Der Riese in this copy.
Compatible shared weapon replacements could potentially benefit all four
maps, while later exclusive assets would continue using their iPhone data.
That cross-map use still needs runtime testing.

### Shader resources

The HD-only `ingame/shaders/shaders.group.bin` contains two
`CIwGxShaderTechnique` resources, IDs `108f79ae` and `5a319a19`. Their
serialized bodies are identical; their resource IDs differ.

The shader source:

- Samples a diffuse texture and a second texture containing a normal map.
- Uses vertex normals, tangents and bitangents to calculate lighting in
  tangent space.
- Combines ambient and directional diffuse lighting with the diffuse texture.
- Applies distance fog using the provided fog parameters.

This confirms the intended shader behavior. Static presence does not
establish which objects select these techniques during gameplay or whether
the executable can fall back to another rendering path.

## Current host limitations

The host implements OpenGL ES 1 guest calls through its renderer.
[`EAGLContext`](../src/frameworks/opengles/eagl.rs) rejects APIs other than
OpenGL ES 1. OpenGL 2.1 on the desktop side does not mean the guest has an
OpenGL ES 2 shader API.

The HD executable adds these graphics imports compared with the iPhone
executable:

| Import | Purpose | Current host implementation |
| --- | --- | --- |
| `glDiscardFramebufferEXT` | Discard framebuffer attachment contents | Missing |
| `glRenderbufferStorageMultisampleAPPLE` | Allocate multisampled renderbuffer storage | Missing |
| `glResolveMultisampleFramebufferAPPLE` | Resolve a multisampled framebuffer | Missing |

Imports establish additional API requirements, not proof of which paths
execute in every session. The HD executable also adds Game Center classes,
popover support and other platform imports. No HD boot attempt or working
fallback renderer was established during this investigation.

The supported [gameplay hooks](../src/codwawzpc/native.rs), menu navigation
and other patches use addresses from the verified iPhone executable. A full
HD executable port would need its own profile, address mapping and checks;
accepting its IPA alone would not make those features work.

## Graphics settings

These options work with the supported iPhone executable and do not need the
HD edition. Each defaults to the original look. They are run options and
launcher settings (Settings → Graphics); [`graphics.rs`](../src/codwawzpc/graphics.rs)
documents the mechanisms.

| Setting | Implementation |
| --- | --- |
| `--render-scale=1–4` | touchHLE's `scale_hack`: guest renderbuffers, viewport and scissor are multiplied, while the game keeps seeing 480×320. The window is sized for the display, not for the scale. When the composited frame is larger than the window it is mipmapped and sampled trilinearly, so the extra samples become supersampling anti-aliasing. |
| `--texture-filter` | The game uploads full PVRTC or RGBA mip chains for world textures and sets `GL_LINEAR_MIPMAP_NEAREST`; interface textures use `GL_LINEAR`. Only mipmapped minification filters are replaced, by `GL_LINEAR_MIPMAP_LINEAR` plus `GL_TEXTURE_MAX_ANISOTROPY_EXT` for 2×–16×. |
| `--widescreen=16:10\|16:9` | The iPhone screen's long side is reported as 512 or 568 points. The Marmalade loader sizes its window, GL view and renderbuffer from `UIScreen` and the renderbuffer query, and the game lays out for it. The field of view is widened so the vertical view matches 3:2 (50° becomes 55° at 16:9, see [`fov.rs`](../src/codwawzpc/fov.rs)). |
| `--gamma`, `--contrast`, `--sharpening` | A GLSL 1.20 pass on the final presented quad in the GL 2.1 renderer: four-neighbour sharpening with the detail clamped to ±0.08, contrast around mid grey, then gamma. At the original values the pass is skipped. |

Observed in Nacht der Untoten on 2026-10-27, from scripted runs with
captures compared at the same spawn view:

- Render scale 2× and 4× visibly sharpen geometry, weapon edges and HUD
  text. At 4× shown in a 720×480 window, stair-stepped edges are smoothed.
- 16× anisotropic filtering keeps the floor's tile seams and grain visible
  at an angle where the original filter blurs them.
- 16:9 keeps the original vertical framing and adds scene at both sides.
  The HUD's ammo and grenade counters move to the right edge; the crosshair
  and round number stay centred. Title, main, options, level select,
  extras, profile, co-op, achievements, leaderboards and pause screens
  fill the width at 16:10 and 16:9 and remain usable with keyboard
  navigation.
- On a Radeon RX 6700S (Mesa radeonsi), 1×–3× held 60 FPS; 4× dipped to
  about 54 FPS. Render scale 3× with 16:10, 8× anisotropic filtering and
  picture adjustments together held 57–60 FPS.

Limitations and next steps:

- **Menu layout in widescreen.** GUI screens are built at startup against
  the wider screen. Full-screen frames and right-anchored elements (the
  level select's description panel) follow it, but bitmaps and panel frames
  authored 480 points wide do not. [`widescreen.rs`](../src/codwawzpc/widescreen.rs)
  widens those placed at the left of a screen-wide parent, which covers the
  front-end backdrop, title and highlight bars and info panels; their art
  is stretched by up to 18%. Backdrops and bars are widened in screens not
  yet shown, judged from the widths their parents ask for, so they are full
  width from a screen's first frame. Panel frames are widened only once
  their screen is laid out, one frame after it appears: widening frames of
  screens not yet shown stopped the title screen from taking input, for a
  reason not yet found. Menu contents keep their 3:2 positions, so
  items sized relative to a widened panel (the co-op connection selector)
  grow with it. The achievements and leaderboards menus were checked; the
  per-map achievement lists, credits and in-level game-over screens have
  not been.
- **Aiming zoom in widescreen.** Each weapon's `aimfov` applies across the
  wider mean dimension, so aiming down sights is about 10% tighter
  vertically at 16:9 than at 3:2. It is not yet corrected.
- **Presentation cost at high scales.** The host presents through
  `glReadPixels` and a texture upload (forced composition), which costs
  most at 4×. Keeping the frame on the GPU would remove the copy.
- Touch mode, co-op and the other three maps have not been tested with
  widescreen.
- A large window, screenshot or clip alone does not show that the game
  rendered at a higher internal resolution; compare captures at the same
  view and window size.

The host also has a [PVRTC decoding path](../src/gles/util.rs). It provides
a possible place to investigate texture replacement, but it does not
currently identify or substitute HD donor textures.

## Proposed implementation sequence

### 1. One optional weapon replacement

Retain the supported iPhone executable, installation and four maps. Match
one HD pistol diffuse atlas to its original texture and implement an opt-in
replacement. Possible approaches are substitution at texture upload or a
selective archive resource overlay; neither is implemented.

Validate the weapon at the same camera position before and after replacement:

- Texture coordinates and material alignment, including both hands.
- Aiming, firing, reloading and weapon changes.
- Transparency, mip levels, filtering and lighting appearance.
- Frame time and memory use.
- Original appearance when disabled, and a usable fallback if the donor
  resource is missing or incompatible.

A complete bundle swap is unsuitable as the first experiment: the HD bundle
changes interdependent resources and omits some iPhone content.

### 2. Expand verified asset mappings

After the first replacement works, map further diffuse resources and test
their use across all four maps. Keep original assets where the HD edition
has no counterpart. A future local HD importer should validate the donor,
record its identity and import only resources with demonstrated compatibility.
The existing supported-game checks should continue to validate the original
iPhone installation.

### 3. Reproduce HD surface lighting

Normal maps require a renderer path that supplies the matching vertex/tangent
data, materials, lighting parameters and fog. Investigate a host shader path
or a narrow Marmalade shader bridge. Copying shader text alone is insufficient.
Confirm visual results in gameplay before describing this as HD lighting
support.

### 4. Evaluate a full HD executable port

Treat this as a separate compatibility project: executable profile, missing
platform and graphics APIs, gameplay hooks, input, menus, saves and co-op
all need review. Establish its actual content coverage and runtime behavior
before considering it an alternative to the four-map iPhone host.

The [graphics settings](#graphics-settings) are independent of these
asset-import stages.

## Method, verification and publication scope

The investigation checked ZIP CRCs, property lists, Mach-O headers,
undefined imports and file hashes. It decoded the Marmalade `DTRZ` archives,
then parsed resource groups and compared resources by type, ID and bytes.
Texture resources included compressed PVRTC and uncompressed/paletted data;
PVRTC previews used the decoder already included by the project.

Verification covered:

- All 121 decoded archive entries, independently reconstructed with the
  earlier research archive decoder and compared byte-for-byte.
- All 105 resource groups, checked against their declared block boundaries.
- All 647 texture records, decoded and checked against their saved PNG pixels.
- A deliberately corrupted group length, which the parser rejected.
- Manual inspection of equal-display-size weapon comparisons and shader source.

Texture counts are resource records per group, not counts of globally unique
images. Byte-identical claims above refer to serialized texture bodies, not
similar-looking thumbnails. Model-body differences do not establish triangle
counts, runtime compatibility or an improvement to every mesh.

No asset transplant, HD gameplay run, frame-rate comparison or runtime visual
quality claim follows from these static checks. The investigation did not
change the installed game, saves, settings or runtime source.

The repository records compatibility metadata and findings. Extracted
textures, shader payloads, executables, raw comparison images and local
research artifacts remain outside it under the [source publication policy](../SOURCE_POLICY.md).
Future verification should use user-supplied copies identified by the
fingerprints above, keep decoded assets local, and compare originals and
replacements at the same camera and display settings.
