/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Co-op over the game's Local (Wi-Fi) menus (see `docs/multiplayer.md`).

pub(crate) mod addr_map;
pub(crate) mod relay;
pub(crate) mod traffic;
pub use relay::{Listing, ListingEvent, Session};
use std::sync::OnceLock;
static SESSION: OnceLock<Session> = OnceLock::new();
pub fn start(server: &str, name: &str) {
    if server.is_empty() {
        return;
    }
    match Session::connect(server, name) {
        Ok(session) => {
            log!(
                "Co-op relay connected as {} ({:?})",
                session.name,
                session.ip
            );
            let _ = SESSION.set(session);
        }
        Err(error) => {
            log!("Co-op relay unavailable: {error}; continuing offline");
        }
    }
}
pub fn session() -> Option<&'static Session> {
    SESSION.get()
}

/// Legacy services must never be resolved or contacted by this host.
pub fn forbidden_host(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "demonware.net" || name.ends_with(".demonware.net")
}
