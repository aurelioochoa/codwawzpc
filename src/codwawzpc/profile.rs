/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The single supported game: Call of Duty: Zombies for iPhone, version 1.5.0.
//!
//! Three researched repacks share these exact bytes and differ only in crack
//! markers, signatures and Info.plist packaging, so identity is the payload
//! rather than the ZIP container.

pub const BUNDLE_ID: &str = "com.activision.callofduty";
/// The game's window title.
pub const TITLE: &str = "Call of Duty: World at War: Zombies Mobile";
pub const VERSION: &str = "1.5.0";
pub const EXECUTABLE: &str = "kiwi";

pub struct Map {
    pub name: &'static str,
    pub archives: [&'static str; 2],
}

pub const MAPS: [Map; 4] = [
    Map {
        name: "Nacht der Untoten",
        archives: ["_nacht.dz", "_nacht_full.dz"],
    },
    Map {
        name: "Verrückt",
        archives: ["_verruckt.dz", "_verruckt_full.dz"],
    },
    Map {
        name: "Shi No Numa",
        archives: ["_shinonuma.dz", "_shinonuma_full.dz"],
    },
    Map {
        name: "Der Riese",
        archives: ["_derriese.dz", "_derriese_full.dz"],
    },
];

/// App-relative path and SHA-256 of every verified payload file.
pub const PAYLOAD: [(&str, &str); 17] = [
    (
        "kiwi",
        "4c940898943a1ac63318083b17bc04219251ad4b3cd2b846eaf41c1702f98f0d",
    ),
    (
        "_main.dz",
        "261632adae37f33fa167ef6e22a662a9fd304ffdeb59788db7afa058412e164e",
    ),
    (
        "_nacht.dz",
        "3bcd8aade69cbb65518e8624d796aecf2719814d6f1b80f53ba81908b172ff1b",
    ),
    (
        "_nacht_full.dz",
        "377f6cab4cf64d84f0a1363e002ba6bc4a7f5c1c2db3a78540b61d9468cf6996",
    ),
    (
        "_verruckt.dz",
        "d96389f12abe3c566b5dc50ec1e4615a3c60ca55f57576eff64a9300a64c6d94",
    ),
    (
        "_verruckt_full.dz",
        "6bfca495372b91ad98dd01d8a306ff09ea69829980d4128431eebd274f2449b1",
    ),
    (
        "_shinonuma.dz",
        "54cea58e553413dbc33f458c647df25397d708810d2bfbf75fe3bc47e1289082",
    ),
    (
        "_shinonuma_full.dz",
        "22ecda04a3afc7f5ae682736bce94cd657c1faff8b8921f3dd171ff81dfae63d",
    ),
    (
        "_derriese.dz",
        "5d82b999474420ad4788dfd2898379692f25aa98502d68e1f7176ed855e35f76",
    ),
    (
        "_derriese_full.dz",
        "303db70c8266683411172998f433293132f2fe8fe3f624594f62d1ac730bfa04",
    ),
    (
        "videos/intro.mp4",
        "a7cadf062086418992fcfc6e4b9de96f3644336c52803a5cd3f563142df7dc88",
    ),
    (
        "MainWindow.nib",
        "66f7914acce93d803582432722e842fbf42cc2587a11909a49953d6a85dcac14",
    ),
    (
        "PkgInfo",
        "82502191c9484b04d685374f9879a0066069c49b8acae7a04b01d38d07e8eca0",
    ),
    (
        "Default.png",
        "77f8d1f433ce75d65ce719e20dcfe7a9d60bd9a8892d81270171965ecd22264b",
    ),
    (
        "Icon.png",
        "79d7707abd936efd6a80ab946a16ed0bf5fab9e19a0df476fd56670df348fe4e",
    ),
    (
        "Icon-72.png",
        "b4c83accc66be6c1a6a752e674c7b5c5e465146a6f11a23e2972b4186ee9b2e7",
    ),
    (
        "IconLarge4.png",
        "3c01e20da916e3390bd293975e82db11cffc785bf629bf48ae806cb34062970c",
    ),
];
