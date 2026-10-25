# Individual achievement artwork

Every entry in the launcher's 106-achievement catalog has a separate built-in
`image_gen` request and a separate transparent trophy master. There is no atlas
slicing, shared generated base with numbered overlays, procedural replacement,
or CLI/API image-generation fallback.

The designs combine the original **2008 console World at War** trophy symbols
(including the Shi No Numa and Der Riese map packs), original game/concept
artwork, and the detailed weathered collectible treatment in the sibling
`redegpc` and `re4mepc` launchers. They are new generated interpretations, not
original game assets presented as new artwork.

## Reference review

- [Original console achievement images](https://strategywiki.org/wiki/Category:Call_of_Duty:_World_at_War_images): twenty original icons were downloaded and visually reviewed together. Matching icons were supplied as the primary image reference for their reinterpretations.
- [Original World at War concept-art gallery](https://callofduty.fandom.com/wiki/Call_of_Duty:_World_at_War/Concept_Art): Makin Atoll's dark teal moonlight, Berlin's restrained copper light, the Pacific trenches' smoky olive materials, and the original poster's weathered WWII silhouettes informed the shared direction.
- [Original Pack-a-Punch concept sheet](https://callofduty.fandom.com/wiki/File:PaP_Machine_ConceptArt_WaW.jpg): period vending-machine shapes, chrome, colored enamel and electrical coils informed upgrade-related objects.

Official reference downloads remain local and are excluded from this repository.
The links above identify them without redistributing the images. The generated
trophy masters are included in `sources/achievement-icons/`.

| Console motif | Launcher achievement IDs |
| --- | --- |
| Dead Air: radio, horizontal speaker and aerial | 001, 069 |
| Hammer Time: hammer crossing barricade boards | 013, 105 |
| Deadhead: skull in a circular crosshair | 024, 077; adapted for tiers 025–026 |
| Perk-a-Holic: four fanned perk bottles | 057 |
| Weapon of Minor Destruction: small mushroom cloud | 071 |
| Big Baller: tied money bag | 072 |
| Big Brawler: hand gripping a knife | 073; magnum detail reflects the port's different trigger |
| It's a Trap!: toothed open trap | 075 |
| Fertilizer Man: skull with crossed-out eyes | 076 |
| Soul Survivor: lone soldier among graves | 084 |
| Elevate Your Senses: bell apparatus and electric burst | 089 |
| Locksmith: open padlock | 090 |
| Der Electrician: three circular pads linked by lightning | 091 |
| Frequent Flyer: bell apparatus and loop/eight | 092 |
| Pack Addict / Wacker Packer: weapon before and after upgrading | 093, 095 |
| 40 Knives: broad Bowie knife | 101 |
| The Might of the Monkey: grinning toy face and burst | 102 |
| Perkaholics Anonymous: crossed-out bottle | 103 |
| Acquire Waffle Weapons: two wonder weapons and monkey face | 104 |

Port-only challenges use relevant original World at War weapons, props,
locations and pickups within the same visual direction. Their names, unlock
conditions and points remain those in the port's catalog. Tiered achievements
have separate compositions and subjects instead of number-swapped copies.

## Files and reproduction

- `achievement-spec.json`: portable ID, source digest and reference-motif mapping.
- `sources/achievement-icons/NNN.png`: full generated image with original alpha.
- `../assets/achievement-icons/NNN.png`: 256×256 launcher image.
- `../assets/achievement-icons/manifest.json`: source/runtime SHA-256 digests and mapping.
- `../../docs/launcher/achievement-icons-contact.png`: labeled review sheet.

Private prompts and tool logs are excluded. Packaging and verification use only
the public specification and generated masters. After deliberately replacing a
master, review it and update its digest in `achievement-spec.json`, then run:

```sh
python3 launcher/artwork/build_achievement_icons.py
python3 scripts/verify_achievement_gallery.py
```

The packaging script validates all masters before replacing runtime images. It
only scales and centers the generated artwork. It never redraws subjects,
adds numbers to trophy art, or modifies source masters. The labels on the
contact sheet sit outside the icons and identify catalog IDs for review.
