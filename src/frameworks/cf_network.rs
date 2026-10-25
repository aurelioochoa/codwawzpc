/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The CoreFoundation Network framework.

pub mod cf_http_message;
pub mod cf_net_service;

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/CFNetwork.framework/CFNetwork",
    aliases: &[],
    class_exports: &[cf_net_service::CLASSES],
    constant_exports: &[cf_http_message::CONSTANTS],
    function_exports: &[cf_http_message::FUNCTIONS, cf_net_service::FUNCTIONS],
};
