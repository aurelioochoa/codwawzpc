# Recovered string keys

The 111 string keys whose original names are known, out of 732. 108 were
found as text: in the executable (its data segment is encoded on disk and
was read from a memory dump), in a level's compiled Lua, or in a resource
group. Three were confirmed by hashing guessed names. For each, the
lowercased djb2 hash of the name equals the string's id in
`LocaliseHashes.dat`. The other 621 strings are known only by their ids. See
[languages-and-voices.md](languages-and-voices.md#recovered-key-names).

Indices and ids are those of the 1.5.0 tables. No game text is reproduced.

| Index | Id | Key | Name found in |
| ---: | --- | --- | --- |
| 20 | `fa32fdbc` | `LS_BUY_AMMO` | executable |
| 22 | `50df21bb` | `LS_MULTIPLAYER` | executable |
| 24 | `29d1742e` | `LS_DOWNLOADS` | executable |
| 73 | `d24050d9` | `LS_EDIT_PROFILE` | executable |
| 74 | `d5b6f2b6` | `LS_PASSWORD` | executable |
| 75 | `3368c1cc` | `LS_CREATE_NEW_ACCOUNT` | executable |
| 76 | `639b8ab5` | `LS_SYNC_MY_PROFILE` | executable |
| 77 | `d36aff11` | `LS_SYNCHRONISING` | executable |
| 78 | `b52ef3f8` | `LS_SYNC_SUCCESS` | executable |
| 84 | `ccc48198` | `LS_COOP_GT_LOCAL` | executable |
| 85 | `6c472812` | `LS_COOP_GT_ONLINE` | executable |
| 93 | `16b5b32e` | `LS_TUTORIAL_DLG01` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 94 | `16b5b32f` | `LS_TUTORIAL_DLG02` | not found as text; a guessed name filling a gap in the `LS_TUTORIAL_DLG` series that hashes to this id |
| 95 | `16b5b330` | `LS_TUTORIAL_DLG03` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 96 | `16b5b331` | `LS_TUTORIAL_DLG04` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 97 | `16b5b332` | `LS_TUTORIAL_DLG05` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 98 | `16b5b333` | `LS_TUTORIAL_DLG06` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 99 | `16b5b334` | `LS_TUTORIAL_DLG07` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 100 | `16b5b335` | `LS_TUTORIAL_DLG08` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 101 | `16b5b336` | `LS_TUTORIAL_DLG09` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 102 | `16b5b34e` | `LS_TUTORIAL_DLG10` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 103 | `16b5b34f` | `LS_TUTORIAL_DLG11` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 104 | `16b5b350` | `LS_TUTORIAL_DLG12` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 105 | `16b5b351` | `LS_TUTORIAL_DLG13` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 106 | `16b5b352` | `LS_TUTORIAL_DLG14` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 107 | `16b5b353` | `LS_TUTORIAL_DLG15` | Lua; executable; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 116 | `797de44e` | `LS_COOP_NOMAP` | executable |
| 118 | `13d9977b` | `LS_COOP_CONNECTING` | executable |
| 140 | `d9f616a3` | `LS_COOP_GT_BLUETOOTH` | executable |
| 141 | `1d80c671` | `LS_COOP_CONN_NOT_AVAILABLE` | executable |
| 142 | `e4a665cc` | `LS_COOP_CONN_NOT_AVAILABLE_DES` | executable |
| 238 | `391de264` | `LS_ACHIEVEMENT_OVERALL_TEXT` | executable |
| 239 | `16b5b354` | `LS_TUTORIAL_DLG16` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 240 | `16b5b355` | `LS_TUTORIAL_DLG17` | not found as text; a guessed name filling a gap in the `LS_TUTORIAL_DLG` series that hashes to this id |
| 241 | `16b5b356` | `LS_TUTORIAL_DLG18` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 292 | `3d56c25b` | `LS_ROUND_1` | executable |
| 295 | `9a687183` | `LS_ABOUT_VERSION` | executable |
| 296 | `593d5f96` | `LS_ABOUT_BUILDNUMBER` | executable |
| 306 | `a6a74e6a` | `LS_COOP_PLAYER_LEFT` | executable |
| 307 | `f0b799fd` | `LS_COOP_HOST_MIGRATED` | executable |
| 316 | `981f69e5` | `LS_UNABLE_TO_CONNECT` | executable |
| 317 | `196afd47` | `LS_ALREADY_IN_USE` | executable |
| 318 | `970438bc` | `LS_SUCCESS` | executable |
| 320 | `929c8ee6` | `LS_VALIDATING` | executable |
| 321 | `29d4ad69` | `LS_USER_AND_PASS_NOT_VALID` | executable |
| 341 | `5b7fc9fa` | `LS_YOU_HAVE_SURVIVED` | executable |
| 343 | `cb79096b` | `LS_YOU_HAVE_SURVIVED1` | executable |
| 349 | `f394eca5` | `LS_VALIDATING_LABEL` | executable |
| 351 | `7a317afb` | `LS_CREATE_ACCOUNT_SUCCESS` | executable |
| 352 | `16b5b357` | `LS_TUTORIAL_DLG19` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 353 | `16b5b36f` | `LS_TUTORIAL_DLG20` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 354 | `6f0cac29` | `LS_USERNAME_INVALID` | executable |
| 355 | `e737fe1c` | `LS_PASSWORD_INVALID` | executable |
| 356 | `5c820a8d` | `LS_USERNAME_CHARACTER_LIMIT` | executable |
| 357 | `c18d9f80` | `LS_PASSWORD_CHARACTER_LIMIT` | executable |
| 358 | `015d90c5` | `LS_USER_AND_PASS_CHAR_LIMIT` | executable |
| 359 | `296ba69b` | `LS_USERNAME_WRONG_CHARACTER` | executable |
| 361 | `4a2b857e` | `LS_USERNAME_NO_SWEARING` | executable |
| 374 | `90b94f1b` | `LS_NAT_NOT_SUPPORTED` | executable |
| 375 | `adf0d4b0` | `LS_COOP_PLAYER_LOST_CONNECTION` | executable |
| 376 | `eba60c4a` | `LS_COOP_PLAYER_KICKED` | executable |
| 377 | `00ad909f` | `LS_COOP_GAME_FULL` | executable |
| 378 | `27c25144` | `LS_COOP_GAME_ALREADY_STARTED` | executable |
| 392 | `d578a611` | `LS_CLEAR_STATS_WARNING_TEXT` | executable |
| 397 | `d927cab1` | `LS_DEMONWARE_SYNC_UNVALIDATED` | executable |
| 398 | `71178028` | `LS_DEMONWARE_NO_VIP_RECEIVED_CREATE` | executable |
| 399 | `2f97ceda` | `LS_DEMONWARE_NO_VIP_RECEIVED_EDIT` | executable |
| 400 | `5146236c` | `LS_DEMONWARE_NO_VIP_TRY_AGAIN` | executable |
| 405 | `d1d1fd3f` | `LS_UNABLE_TO_CONNECT_DW` | executable |
| 408 | `129daf3d` | `LS_BT_LONG_SEARCH` | executable |
| 409 | `bc1aee3e` | `LS_COOP_INITIALIZING` | executable |
| 410 | `637d8a5f` | `LS_COOP_NO_LONGER_CONNECT` | executable |
| 411 | `056563dd` | `LS_COOP_HOST_LEFT_RESTARTING` | executable |
| 460 | `b5504a64` | `LS_COOP_BT_OFF_TITLE` | executable |
| 461 | `cddbd6be` | `LS_COOP_BT_OFF_DES` | executable |
| 462 | `70f70079` | `LS_COOP_BT_NOT_AVAILABLE` | executable |
| 463 | `f07d9854` | `LS_COOP_SWITCH_OFF_BT` | executable |
| 464 | `0650600d` | `LS_COOP_SWITCH_OFF_WIFI` | executable |
| 466 | `0ea04f69` | `LS_DOWNLOADING_PERCENT` | executable |
| 472 | `207cf671` | `LS_GENERATOR_NEED_POWER` | executable |
| 518 | `50b281b4` | `LS_UNABLE_TO_GET_NEW_MAPS` | executable |
| 519 | `6563da3b` | `LS_NEW_MAPS_ERROR_DOWNLOADING` | executable |
| 521 | `27f11e08` | `LS_COOP_GAME_VERSION_INCOMPATIBLE` | executable |
| 529 | `810154c7` | `LS_PURCHASING_PERCENT` | executable |
| 530 | `16b5b370` | `LS_TUTORIAL_DLG21` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 531 | `16b5b371` | `LS_TUTORIAL_DLG22` | Lua; `derriese_shared.group.bin`, `nacht_shared.group.bin`, `shinonuma_shared.group.bin`, `verruckt_shared.group.bin` |
| 548 | `63dd1278` | `LS_DOWNLOAD_MAP` | executable |
| 573 | `e9953bd6` | `LS_RANK_FIRST` | executable |
| 574 | `3a3f19aa` | `LS_RANK_SECOND` | executable |
| 575 | `ea91e009` | `LS_RANK_THIRD` | executable |
| 576 | `e050dacc` | `LS_RANK_REST` | executable |
| 592 | `165c7616` | `LS_LITE_REFUSE_NEWMAP_PLAY` | executable |
| 594 | `ff303ffc` | `LS_BUY_TRANSACTION_FAILED` | executable |
| 595 | `256ffaea` | `LS_BUY_NEED_FIRMWARE` | executable |
| 596 | `6bc0ddcf` | `LS_BUY_GET_MAP` | executable |
| 597 | `7bf23930` | `LS_BUY_MAP` | executable |
| 599 | `442540b4` | `LS_BUY_CHOOSE_TYPE` | executable |
| 600 | `6568f10b` | `LS_BUY_DOWNLOAD_PREVIEW` | executable |
| 602 | `5b06c0ea` | `LS_BUY_CORRUPTED` | executable |
| 603 | `82db603a` | `LS_ALERT_UPDATE_MAP` | executable |
| 604 | `67199208` | `LS_ROUND_1_LITE` | executable |
| 605 | `546fc4bc` | `LS_ROUND_1_PREVIEW` | executable |
| 608 | `5dd6d70d` | `LS_NOT_AVAIL_LITE` | executable |
| 609 | `53e5a7a1` | `LS_NOT_AVAIL_PREVIEW` | executable |
| 622 | `ed20f6b4` | `LS_TUTORIAL_DLG19_IPAD` | not found as text; a guessed name filling a gap in the `LS_TUTORIAL_DLG` series that hashes to this id |
| 676 | `30d2dfda` | `LS_WUNDERWAFFE_GAME_FIND` | Lua; `shinonuma.group.bin` |
| 722 | `ee64d0d6` | `LS_TELEPORTER_LINK` | executable |
| 723 | `3d86c618` | `LS_TELEPORTER_BUY` | executable |
| 729 | `7669f654` | `LS_TELEPORTER_LINKING` | executable |
| 730 | `b2d1cf35` | `LS_TELEPORTER_TELEPORTING` | executable |
| 731 | `cb7dc28e` | `LS_TELEPORTER_TEMPUNAVAIL` | executable |
