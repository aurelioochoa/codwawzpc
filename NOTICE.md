# codwawzpc notices

codwawzpc is derived from redegpc at commit
`85735d7fd00d9ff294b96e87191d77eec6745aee`, based on touchHLE at
`ca1de89c578e79746f33c1332e28efd106c334ba`. The public repository starts with a fresh, audited source snapshot. Original
source attribution is retained in copyright headers, this notice and
[upstream credits](README.touchHLE.md). Dependencies are locked in
`Cargo.lock`; native dependencies use pinned vendor gitlinks (Dynarmic, SDL,
OpenAL Soft and stb). New host and compatibility code is provided under
MPL-2.0, following the source file notices. See LICENSE,
dev-scripts/gpl-3.0.txt and README.touchHLE.md for upstream source and
combined-binary terms. The documented combined binary is GPL-3.0-or-later;
individual components retain their licenses.

Do not remove touchHLE_dylibs/COPYING.*, touchHLE_fonts/LICENSE.*, vendor
licenses, or the runtime-generated dependency notices (available through
`codwawzpc --copyright`). Distribution must include the runtime-resource
notices and meet all dependency license obligations. FFmpeg is an external
runtime dependency used for the intro movie; it is not vendored or
redistributed, and its installed build has its own license terms.

Call of Duty: Zombies and its game files are the property of their respective
owners. No game executable, archive or extracted game data is included in this
source tree, and the program never downloads them. `codwawzpc import` copies
files from an IPA you supply into private local application storage.
Generated launcher artwork is included; downloaded official reference images,
gameplay captures, game archives and private development records are excluded.
Achievement labels and descriptions are compatibility metadata; German, Italian
and Spanish text is project-authored translation. The game's original English
and French string-table files are supplied only by the user's imported game.
See SOURCE_POLICY.md for the publication boundary.
