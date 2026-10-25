/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
fn main() {
    if let Err(error) = touchHLE::codwawzpc::cli(std::env::args_os().skip(1)) {
        eprintln!("codwawzpc: {error}");
        std::process::exit(1);
    }
}
