/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `netdb.h`

use crate::dyld::FunctionExports;
use crate::export_c_func;
use crate::libc::sys::socket::{sockaddr, AF_INET, SOCK_STREAM};
use crate::mem::{guest_size_of, ConstPtr, MutPtr, Ptr, SafeRead};
use crate::Environment;

const AI_PASSIVE: i32 = 0x1;

pub const IPPROTO_TCP: i32 = 6;
pub const IPPROTO_UDP: i32 = 17;

const EAI_FAIL: i32 = 4;

#[allow(non_camel_case_types)]
pub type socklen_t = u32;

#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
#[repr(C)]
struct hostent {
    h_name: MutPtr<u8>,
    h_aliases: MutPtr<MutPtr<u8>>,
    h_addrtype: i32,
    h_length: i32,
    h_addr_list: MutPtr<MutPtr<u8>>,
}
unsafe impl SafeRead for hostent {}
#[derive(Default)]
pub struct State {
    allocations: Vec<MutPtr<u8>>,
}

use crate::codwawzpc::coop::forbidden_host;
fn virtual_ip(name: &str) -> Option<[u8; 4]> {
    let session = crate::codwawzpc::coop::session()?;
    let name = name.trim_end_matches('.');
    if name.eq_ignore_ascii_case("localhost")
        || name.eq_ignore_ascii_case(&format!("codwawzpc-{}", session.ip[3]))
        || name.eq_ignore_ascii_case(&format!("codwawzpc-{}.local", session.ip[3]))
    {
        return Some(session.ip);
    }
    for listing in session.listings() {
        if name.eq_ignore_ascii_case(&format!("codwawzpc-{}", listing.owner[3]))
            || name.eq_ignore_ascii_case(&format!("codwawzpc-{}.local", listing.owner[3]))
            || name == listing.service.name
        {
            return Some(listing.owner);
        }
    }
    name.parse::<std::net::Ipv4Addr>()
        .ok()
        .filter(|ip| ip.octets()[..3] == [10, 77, 0])
        .map(|ip| ip.octets())
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct addrinfo {
    ai_flags: i32,
    ai_family: i32,
    ai_socktype: i32,
    ai_protocol: i32,
    ai_addrlen: socklen_t,
    ai_canonname: MutPtr<u8>,
    ai_addr: MutPtr<sockaddr>,
    ai_next: MutPtr<addrinfo>,
}
unsafe impl SafeRead for addrinfo {}

fn getaddrinfo(
    env: &mut Environment,
    node_name: MutPtr<u8>,
    serv_name: MutPtr<u8>,
    hints: ConstPtr<addrinfo>,
    res: MutPtr<MutPtr<addrinfo>>,
) -> i32 {
    if !node_name.is_null() && forbidden_host(env.mem.cstr_at_utf8(node_name).unwrap_or("")) {
        log!("Refusing legacy DemonWare lookup");
        return EAI_FAIL;
    }
    if !env.options.network_access {
        log_dbg!(
            "Network access is disabled, getaddrinfo({:?}, {:?}, {:?}, {:?}) -> EAI_FAIL",
            node_name,
            serv_name,
            hints,
            res
        );
        return EAI_FAIL;
    }

    let hint = if hints.is_null() {
        addrinfo {
            ai_flags: 0,
            ai_family: AF_INET,
            ai_socktype: SOCK_STREAM,
            ai_protocol: 0,
            ai_addrlen: 0,
            ai_canonname: Ptr::null(),
            ai_addr: Ptr::null(),
            ai_next: Ptr::null(),
        }
    } else {
        env.mem.read(hints)
    };
    if hint.ai_family != 0 && hint.ai_family != AF_INET {
        return EAI_FAIL;
    }
    let port = if serv_name.is_null() {
        0
    } else {
        match env.mem.cstr_at_utf8(serv_name).unwrap_or("").parse::<u16>() {
            Ok(p) => p,
            Err(_) => return EAI_FAIL,
        }
    };
    let ip = if node_name.is_null() {
        if hint.ai_flags & AI_PASSIVE != 0 {
            [0; 4]
        } else {
            crate::codwawzpc::coop::session().map_or([127, 0, 0, 1], |s| s.ip)
        }
    } else {
        let name = env.mem.cstr_at_utf8(node_name).unwrap_or("");
        match virtual_ip(name) {
            Some(ip) => ip,
            None => return EAI_FAIL,
        }
    };
    let mut addr_info = hint;
    addr_info.ai_family = AF_INET;
    addr_info.ai_canonname = Ptr::null();
    addr_info.ai_next = Ptr::null();
    addr_info.ai_addr = env.mem.alloc_and_write(sockaddr::from_ipv4_parts(ip, port));
    addr_info.ai_addrlen = guest_size_of::<sockaddr>();
    let tmp = env.mem.alloc_and_write(addr_info);
    env.mem.write(res, tmp);

    0 // Success
}

fn freeaddrinfo(env: &mut Environment, addrinfo: MutPtr<addrinfo>) {
    let addrinfo_val = env.mem.read(addrinfo);
    assert!(addrinfo_val.ai_next.is_null()); // TODO
    let ai_addrlen = addrinfo_val.ai_addrlen;
    assert_eq!(ai_addrlen, guest_size_of::<sockaddr>());
    env.mem.free(addrinfo_val.ai_addr.cast());
    env.mem.free(addrinfo.cast());
}

fn gethostbyname(env: &mut Environment, name: ConstPtr<u8>) -> MutPtr<hostent> {
    let name_text = env.mem.cstr_at_utf8(name).unwrap_or("").to_string();
    if forbidden_host(&name_text) {
        log!("Refusing legacy DemonWare lookup: {name_text}");
        return Ptr::null();
    }
    if !env.options.network_access {
        return Ptr::null();
    }
    let Some(ip) = virtual_ip(&name_text) else {
        return Ptr::null();
    };
    for p in std::mem::take(&mut env.libc_state.netdb.allocations) {
        env.mem.free(p.cast());
    }
    let name = env.mem.alloc_and_write_cstr(name_text.as_bytes());
    let bytes = env.mem.alloc_and_write(u32::from_le_bytes(ip)).cast::<u8>();
    let aliases = env.mem.alloc_and_write(MutPtr::<u8>::null());
    let addresses = env.mem.alloc(8).cast::<MutPtr<u8>>();
    env.mem.write(addresses, bytes);
    env.mem.write(addresses + 1, MutPtr::<u8>::null());
    let result = env.mem.alloc_and_write(hostent {
        h_name: name,
        h_aliases: aliases.cast(),
        h_addrtype: AF_INET,
        h_length: 4,
        h_addr_list: addresses.cast(),
    });
    env.libc_state.netdb.allocations =
        vec![name, bytes, aliases.cast(), addresses.cast(), result.cast()];
    log_dbg!("gethostbyname({name_text:?}) -> {ip:?}");
    result
}

fn gethostent(_env: &mut Environment) -> MutPtr<hostent> {
    log!("TODO: gethostent() => NULL");
    // TODO: set h_errno
    Ptr::null()
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(getaddrinfo(_, _, _, _)),
    export_c_func!(freeaddrinfo(_)),
    export_c_func!(gethostbyname(_)),
    export_c_func!(gethostent()),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demonware_names_are_refused_case_insensitively() {
        for name in [
            "demonware.net",
            "codzombies-iphone-live.auth.mmp3.demonware.net",
            "X.DEMONWARE.NET.",
        ] {
            assert!(forbidden_host(name));
        }
        for name in [
            "localhost",
            "relay.example.org",
            "notdemonware.net",
            "demonware.net.evil",
        ] {
            assert!(!forbidden_host(name));
        }
    }
}
