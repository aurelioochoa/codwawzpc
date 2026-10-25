/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `dlfcn.h` (`dlopen()` and friends)

use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr, Ptr, SafeRead};
use crate::Environment;
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    /// Guest C strings returned by `dladdr()`, which callers must not free.
    strings: HashMap<String, ConstPtr<u8>>,
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct Dl_info {
    dli_fname: ConstPtr<u8>,
    dli_fbase: MutVoidPtr,
    dli_sname: ConstPtr<u8>,
    dli_saddr: MutVoidPtr,
}
unsafe impl SafeRead for Dl_info {}

const RTLD_DEFAULT: MutVoidPtr = Ptr::from_bits(-2 as _);

fn is_known_library(path: &str) -> bool {
    crate::dyld::DYLIB_LIST
        .iter()
        .any(|dylib| dylib.path == path || dylib.aliases.contains(&path))
}

fn dlopen(env: &mut Environment, path: ConstPtr<u8>, _mode: i32) -> MutVoidPtr {
    if path.is_null() {
        return RTLD_DEFAULT;
    }
    // TODO: dlopen() support for real dynamic libraries.
    assert!(is_known_library(env.mem.cstr_at_utf8(path).unwrap()));
    // For convenience, use the path as the handle.
    // TODO: Find out whether the handle is truly opaque on iPhone OS, and if
    // not, where it points.
    path.cast_mut().cast()
}

fn dlsym(env: &mut Environment, handle: MutVoidPtr, symbol: ConstPtr<u8>) -> MutVoidPtr {
    assert!(
        handle == RTLD_DEFAULT || is_known_library(env.mem.cstr_at_utf8(handle.cast()).unwrap())
    );
    // For some reason, the symbols passed to dlsym() don't have the leading _.
    let symbol = format!("_{}", env.mem.cstr_at_utf8(symbol).unwrap());
    // TODO: error handling. dlsym() should just return NULL in this case, but
    // currently it's probably more useful to have the emulator crash if there's
    // no symbol found, since it most likely indicates a missing host function.
    // TODO: Symbol lookup should be scoped to the specific library requested,
    // where appropriate!
    let addr = env
        .dyld
        .create_proc_address(&mut env.mem, &mut env.cpu, &symbol)
        .unwrap_or_else(|_| panic!("dlsym() for unimplemented function {symbol}"));
    Ptr::from_bits(addr.addr_with_thumb_bit())
}

fn dlclose(env: &mut Environment, handle: MutVoidPtr) -> i32 {
    assert!(
        handle == RTLD_DEFAULT || is_known_library(env.mem.cstr_at_utf8(handle.cast()).unwrap())
    );
    0 // success
}

fn interned_cstr(env: &mut Environment, string: &str) -> ConstPtr<u8> {
    if let Some(&ptr) = env.libc_state.dlfcn.strings.get(string) {
        return ptr;
    }
    let ptr = env.mem.alloc_and_write_cstr(string.as_bytes()).cast_const();
    env.libc_state.dlfcn.strings.insert(string.to_string(), ptr);
    ptr
}

/// Describe the loaded image containing `addr`, and the nearest symbol at or
/// below it. Marmalade (S3E) games use this to locate their own executable.
fn dladdr(env: &mut Environment, addr: ConstPtr<u8>, info: MutPtr<Dl_info>) -> i32 {
    let addr = addr.to_bits() & !1;
    let Some((index, bin)) = env.bins.iter().enumerate().find(|(_, bin)| {
        bin.header_addr
            .is_some_and(|base| (base..bin.last_segment_end).contains(&addr))
    }) else {
        log_dbg!("dladdr({:#x}): not in a loaded image", addr);
        return 0;
    };
    let base = bin.header_addr.unwrap();
    let path = if index == 0 {
        env.bundle.executable_path().as_str().to_string()
    } else {
        bin.path.clone().unwrap_or_else(|| bin.name.clone())
    };
    let symbol = bin
        .exported_symbols
        .iter()
        .map(|(name, &value)| (name, value & !1))
        .filter(|&(_, value)| value <= addr && value >= base)
        .max_by_key(|&(_, value)| value)
        .map(|(name, value)| (name.clone(), value));
    let dli_fname = interned_cstr(env, &path);
    let (dli_sname, dli_saddr) = match symbol {
        Some((name, value)) => {
            // Like dlsym(), dladdr() reports names without the leading _.
            let name = name.strip_prefix('_').unwrap_or(&name).to_string();
            (interned_cstr(env, &name), Ptr::from_bits(value))
        }
        None => (Ptr::null(), Ptr::null()),
    };
    log_dbg!("dladdr({:#x}) => {} base {:#x}", addr, path, base);
    if !info.is_null() {
        env.mem.write(
            info,
            Dl_info {
                dli_fname,
                dli_fbase: Ptr::from_bits(base),
                dli_sname,
                dli_saddr,
            },
        );
    }
    1
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(dladdr(_, _)),
    export_c_func!(dlopen(_, _)),
    export_c_func!(dlsym(_, _)),
    export_c_func!(dlclose(_)),
];
