/* This Source Code Form is subject to the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The relay is presented as a single Wi-Fi interface to the guest.
use crate::libc::{errno::set_errno, sys::socket::sockaddr};
use crate::{
    dyld::FunctionExports,
    export_c_func,
    mem::{MutPtr, Ptr, SafeRead},
    Environment,
};
#[derive(Copy, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
struct ifaddrs {
    next: MutPtr<ifaddrs>,
    name: MutPtr<u8>,
    flags: u32,
    addr: MutPtr<sockaddr>,
    netmask: MutPtr<sockaddr>,
    destination: MutPtr<sockaddr>,
    data: MutPtr<u8>,
}
unsafe impl SafeRead for ifaddrs {}
fn getifaddrs(env: &mut Environment, ifap: MutPtr<MutPtr<ifaddrs>>) -> i32 {
    if ifap.is_null() {
        set_errno(env, 14);
        return -1;
    }
    env.mem.write(ifap, Ptr::null());
    let Some(session) = crate::codwawzpc::coop::session().filter(|s| s.is_connected()) else {
        return -1;
    };
    let block = env.mem.alloc(80).cast::<u8>();
    let name = block + 28;
    env.mem.bytes_at_mut(name, 4).copy_from_slice(b"en0\0");
    let addr = (block + 32).cast();
    let mask = (block + 48).cast();
    let broadcast = (block + 64).cast();
    env.mem
        .write(addr, sockaddr::from_ipv4_parts(session.ip, 0));
    env.mem
        .write(mask, sockaddr::from_ipv4_parts([255, 255, 255, 0], 0));
    env.mem
        .write(broadcast, sockaddr::from_ipv4_parts([10, 77, 0, 255], 0));
    let value = ifaddrs {
        next: Ptr::null(),
        name,
        flags: 0x1 | 0x2 | 0x40 | 0x8000,
        addr,
        netmask: mask,
        destination: broadcast,
        data: Ptr::null(),
    };
    env.mem.write(block.cast(), value);
    env.mem.write(ifap, block.cast());
    set_errno(env, 0);
    0
}
fn freeifaddrs(env: &mut Environment, ifp: MutPtr<ifaddrs>) {
    if !ifp.is_null() {
        env.mem.free(ifp.cast());
    }
}
pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(getifaddrs(_)),
    export_c_func!(freeifaddrs(_)),
];
