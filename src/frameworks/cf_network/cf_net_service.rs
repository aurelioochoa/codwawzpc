/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Relay-backed Bonjour services. Guest callbacks run exclusively on the main run loop.

use codwawzpc_coop_proto::Service;
fn local_domain(domain: &str) -> bool {
    domain.is_empty() || domain.trim_end_matches('.').eq_ignore_ascii_case("local")
}
fn same_kind(a: &str, b: &str) -> bool {
    a.trim_end_matches('.')
        .eq_ignore_ascii_case(b.trim_end_matches('.'))
}

#[derive(Clone, Debug, PartialEq)]
struct BrowseItem {
    owner: [u8; 4],
    id: u32,
    service: Service,
}
#[derive(Default)]
struct BrowseQueue {
    known: BTreeMap<([u8; 4], u32), BrowseItem>,
}
impl BrowseQueue {
    fn reconcile(
        &mut self,
        own: [u8; 4],
        kind: &str,
        items: &[BrowseItem],
    ) -> Vec<(BrowseItem, u32)> {
        let wanted: BTreeMap<_, _> = items
            .iter()
            .filter(|i| i.owner != own && same_kind(&i.service.kind, kind))
            .map(|i| ((i.owner, i.id), i.clone()))
            .collect();
        let mut events: Vec<_> = self
            .known
            .iter()
            .filter(|(key, item)| wanted.get(key) != Some(item))
            .map(|(_, i)| (i.clone(), 8))
            .collect();
        events.extend(
            wanted
                .iter()
                .filter(|(key, item)| self.known.get(key) != Some(item))
                .map(|(_, i)| (i.clone(), 0)),
        );
        self.known = wanted;
        let count = events.len();
        for (index, (_, flags)) in events.iter_mut().enumerate() {
            if index + 1 < count {
                *flags |= 1;
            }
        }
        events
    }
    fn apply(&mut self, own: [u8; 4], kind: &str, event: &ListingEvent) -> Vec<(BrowseItem, u32)> {
        match event {
            ListingEvent::Added(l) => {
                if l.owner == own || !same_kind(&l.service.kind, kind) {
                    return Vec::new();
                }
                let item = BrowseItem::from(l.clone());
                if self.known.get(&(l.owner, l.id)) == Some(&item) {
                    return Vec::new();
                }
                let old = self.known.insert((l.owner, l.id), item.clone());
                let mut events = Vec::new();
                if let Some(old) = old {
                    events.push((old, 9));
                }
                events.push((item, 0));
                events
            }
            ListingEvent::Removed { owner, id } => self
                .known
                .remove(&(*owner, *id))
                .into_iter()
                .map(|i| (i, 8))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(owner: u8, id: u32) -> BrowseItem {
        BrowseItem {
            owner: [10, 77, 0, owner],
            id,
            service: Service {
                name: format!("Player {owner}"),
                kind: "_CODWAW_IPHONE._tcp.".into(),
                port: 1234,
                txt: vec![1, b'x'],
            },
        }
    }
    #[test]
    fn accepts_the_games_undotted_local_domain() {
        for domain in ["", "local", "local.", "LOCAL."] {
            assert!(local_domain(domain));
        }
        for domain in ["example.org", "local.evil", "notlocal"] {
            assert!(!local_domain(domain));
        }
    }
    #[test]
    fn browse_reports_existing_then_new_then_removed() {
        let mut queue = BrowseQueue::default();
        let a = item(2, 1);
        let b = item(3, 4);
        assert_eq!(
            queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[a.clone()]),
            vec![(a.clone(), 0)]
        );
        assert!(queue
            .reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[a.clone()])
            .is_empty());
        assert_eq!(
            queue.reconcile(
                [10, 77, 0, 1],
                "_CODWAW_IPHONE._tcp.",
                &[a.clone(), b.clone()]
            ),
            vec![(b.clone(), 0)]
        );
        assert_eq!(
            queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[b]),
            vec![(a, 8)]
        );
    }
    #[test]
    fn own_service_not_browsed() {
        let mut queue = BrowseQueue::default();
        assert_eq!(
            queue.reconcile(
                [10, 77, 0, 1],
                "_CODWAW_IPHONE._tcp",
                &[item(1, 1), item(2, 1)]
            ),
            vec![(item(2, 1), 0)]
        );
    }
    #[test]
    fn browse_batch_flags_and_kind_filter() {
        let mut queue = BrowseQueue::default();
        let mut wrong = item(4, 1);
        wrong.service.kind = "_http._tcp.".into();
        assert_eq!(
            queue.reconcile(
                [10, 77, 0, 1],
                "_codwaw_iphone._TCP",
                &[item(2, 1), item(3, 1), wrong]
            ),
            vec![(item(2, 1), 1), (item(3, 1), 0)]
        );
        assert_eq!(
            queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[]),
            vec![(item(2, 1), 9), (item(3, 1), 8)]
        );
    }
    #[test]
    fn browse_txt_change_replaces_existing_service() {
        let mut queue = BrowseQueue::default();
        let old = item(2, 1);
        queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[old.clone()]);
        let mut new = old.clone();
        new.service.txt = vec![1, b'y'];
        assert_eq!(
            queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[new.clone()]),
            vec![(old, 9), (new, 0)]
        );
    }
    #[test]
    fn browse_distinguishes_equal_names_by_owner_and_id() {
        let mut queue = BrowseQueue::default();
        let a = item(2, 1);
        let mut b = item(3, 1);
        b.service = a.service.clone();
        let mut c = b.clone();
        c.id = 2;
        assert_eq!(
            queue.reconcile(
                [10, 77, 0, 1],
                "_CODWAW_IPHONE._tcp.",
                &[a.clone(), b.clone(), c.clone()]
            ),
            vec![(a.clone(), 1), (b, 1), (c.clone(), 0)]
        );
        assert_eq!(
            queue.reconcile([10, 77, 0, 1], "_CODWAW_IPHONE._tcp.", &[a, c]),
            vec![(
                BrowseItem {
                    owner: [10, 77, 0, 3],
                    id: 1,
                    service: item(2, 1).service
                },
                8
            )]
        );
    }
    #[test]
    fn relay_events_preserve_transient_peer_and_ignore_duplicates() {
        let mut queue = BrowseQueue::default();
        let own = [10, 77, 0, 1];
        let kind = "_CODWAW_IPHONE._tcp.";
        let a = item(2, 1);
        let added = ListingEvent::Added(Listing {
            owner: a.owner,
            id: a.id,
            service: a.service.clone(),
        });
        assert_eq!(queue.apply(own, kind, &added), vec![(a.clone(), 0)]);
        assert!(queue.apply(own, kind, &added).is_empty());
        assert_eq!(
            queue.apply(
                own,
                kind,
                &ListingEvent::Removed {
                    owner: a.owner,
                    id: a.id
                }
            ),
            vec![(a, 8)]
        );
        assert!(queue.reconcile(own, kind, &[]).is_empty());
        let self_listing = item(1, 2);
        assert!(queue
            .apply(
                own,
                kind,
                &ListingEvent::Added(Listing {
                    owner: own,
                    id: 2,
                    service: self_listing.service
                })
            )
            .is_empty());
        assert!(queue
            .apply(own, kind, &ListingEvent::Removed { owner: own, id: 2 })
            .is_empty());
    }
}

use crate::abi::{CallFromHost, GuestFunction};
use crate::codwawzpc::coop::{session, Listing, ListingEvent};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::cf_allocator::CFAllocatorRef;
use crate::frameworks::core_foundation::cf_data::{
    CFDataCreate, CFDataGetBytePtr, CFDataGetLength, CFDataRef,
};
use crate::frameworks::core_foundation::cf_run_loop::{
    CFRunLoopGetMain, CFRunLoopMode, CFRunLoopRef,
};
use crate::frameworks::core_foundation::cf_string::CFStringRef;
use crate::frameworks::core_foundation::{CFIndex, CFOptionFlags, CFTypeRef};
use crate::frameworks::foundation::{ns_run_loop, ns_string};
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr, SafeRead};
use crate::objc::{
    id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
};
use crate::Environment;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

type CFNetServiceRef = CFTypeRef;
type CFNetServiceBrowserRef = CFTypeRef;
const UNKNOWN: i32 = -72000;
const NOT_FOUND: i32 = -72002;
const BAD_ARGUMENT: i32 = -72004;
const TIMEOUT: i32 = -72007;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct StreamError {
    domain: i32,
    error: i32,
}
unsafe impl SafeRead for StreamError {}
fn error(env: &mut Environment, out: MutPtr<StreamError>, code: i32) -> bool {
    if code != 0 {
        log_dbg!("CFNetService error {code}");
    }
    if !out.is_null() {
        env.mem.write(
            out,
            StreamError {
                domain: if code == 0 { 0 } else { 10 },
                error: code,
            },
        );
    }
    code == 0
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct ClientContext {
    version: CFIndex,
    info: MutVoidPtr,
    retain: GuestFunction,
    release: GuestFunction,
    description: GuestFunction,
}
unsafe impl SafeRead for ClientContext {}
impl ClientContext {
    fn empty() -> Self {
        Self {
            version: 0,
            info: MutVoidPtr::null(),
            retain: GuestFunction::null_ptr(),
            release: GuestFunction::null_ptr(),
            description: GuestFunction::null_ptr(),
        }
    }
    fn acquire(self, env: &mut Environment) -> Self {
        let mut result = self;
        if !self.retain.to_ptr().is_null() {
            let callback = self.retain;
            result.info = callback.call_from_host(env, (self.info,));
        }
        result
    }
    fn dispose(self, env: &mut Environment) {
        if !self.release.to_ptr().is_null() {
            let callback = self.release;
            () = callback.call_from_host(env, (self.info,));
        }
    }
}
fn context(env: &mut Environment, ptr: ConstPtr<ClientContext>) -> Option<ClientContext> {
    let ctx = if ptr.is_null() {
        ClientContext::empty()
    } else {
        env.mem.read(ptr)
    };
    if ctx.version != 0 {
        return None;
    }
    Some(ctx.acquire(env))
}

struct NetObject {
    callback: GuestFunction,
    context: ClientContext,
    // Mode names are copied; run loops are immortal in this implementation.
    scheduled: Vec<(CFRunLoopRef, String)>,
    generation: u64,
    body: Body,
}
enum Body {
    Service(ServiceState),
    Browser(BrowserState),
}
struct ServiceState {
    service: Service,
    domain: CFStringRef,
    name: CFStringRef,
    kind: CFStringRef,
    target: CFStringRef,
    txt: CFDataRef,
    addresses: CFTypeRef,
    owner: Option<[u8; 4]>,
    listing_id: Option<u32>,
    registration: Option<u32>,
    resolving: Option<Instant>,
    pending: VecDeque<i32>,
}
#[derive(Default)]
struct BrowserState {
    invalidated: bool,
    searching: Option<String>,
    queue: BrowseQueue,
    pending: VecDeque<(BrowseItem, u32)>,
    failure: Option<i32>,
}
impl HostObject for NetObject {}

pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation _touchHLE_CFNetService: NSObject
- (())dealloc { destroy(env, this); }
@end
@implementation _touchHLE_CFNetServiceBrowser: NSObject
- (())dealloc { destroy(env, this); }
@end
};
fn destroy(env: &mut Environment, object: id) {
    let obj = env.objc.borrow_mut::<NetObject>(object);
    let ctx = std::mem::replace(&mut obj.context, ClientContext::empty());
    let mut owned = Vec::new();
    if let Body::Service(s) = &mut obj.body {
        if let Some(id) = s.registration.take() {
            if let Some(session) = session() {
                session.unregister(id);
            }
        }
        owned.extend([s.domain, s.name, s.kind, s.target, s.txt, s.addresses]);
    }
    ctx.dispose(env);
    for object in owned {
        release(env, object);
    }
    env.objc.dealloc_object(object, &mut env.mem);
}
fn allocate(
    env: &mut Environment,
    class: &str,
    body: Body,
    callback: GuestFunction,
    context: ClientContext,
) -> id {
    let class = env.objc.get_known_class(class, &mut env.mem);
    env.objc.alloc_object(
        class,
        Box::new(NetObject {
            callback,
            context,
            scheduled: Vec::new(),
            generation: 0,
            body,
        }),
        &mut env.mem,
    )
}
fn service(env: &Environment, object: id) -> &ServiceState {
    match &env.objc.borrow::<NetObject>(object).body {
        Body::Service(s) => s,
        _ => panic!("expected CFNetService"),
    }
}
fn service_mut(env: &mut Environment, object: id) -> &mut ServiceState {
    match &mut env.objc.borrow_mut::<NetObject>(object).body {
        Body::Service(s) => s,
        _ => panic!("expected CFNetService"),
    }
}
fn browser_mut(env: &mut Environment, object: id) -> &mut BrowserState {
    match &mut env.objc.borrow_mut::<NetObject>(object).body {
        Body::Browser(b) => b,
        _ => panic!("expected CFNetServiceBrowser"),
    }
}
fn data(env: &mut Environment, bytes: &[u8]) -> CFDataRef {
    let ptr = env.mem.alloc(bytes.len().try_into().unwrap());
    if !bytes.is_empty() {
        env.mem
            .bytes_at_mut(ptr.cast(), bytes.len().try_into().unwrap())
            .copy_from_slice(bytes);
    }
    let result = CFDataCreate(
        env,
        ConstPtr::null(),
        ptr.cast().cast_const(),
        bytes.len().try_into().unwrap(),
    );
    env.mem.free(ptr);
    result
}
fn read_data(env: &mut Environment, object: CFDataRef) -> Vec<u8> {
    if object.is_null() {
        return Vec::new();
    }
    let length = CFDataGetLength(env, object);
    if length <= 0 {
        return Vec::new();
    }
    let ptr = CFDataGetBytePtr(env, object);
    env.mem.bytes_at(ptr, length as u32).to_vec()
}
fn make_service(
    env: &mut Environment,
    domain: String,
    service: Service,
    owner: Option<[u8; 4]>,
    listing_id: Option<u32>,
) -> id {
    let mut service = service;
    service.kind = if same_kind(&service.kind, "_CODWAW_IPHONE._tcp.") {
        "_CODWAW_IPHONE._tcp.".into()
    } else {
        format!("{}.", service.kind.trim_end_matches('.'))
    };
    let domain = ns_string::from_rust_string(env, domain);
    let name = ns_string::from_rust_string(env, service.name.clone());
    let kind = ns_string::from_rust_string(env, service.kind.clone());
    let txt = data(env, &service.txt);
    allocate(
        env,
        "_touchHLE_CFNetService",
        Body::Service(ServiceState {
            service,
            domain,
            name,
            kind,
            target: nil,
            txt,
            addresses: nil,
            owner,
            listing_id,
            registration: None,
            resolving: None,
            pending: VecDeque::new(),
        }),
        GuestFunction::null_ptr(),
        ClientContext::empty(),
    )
}
fn CFNetServiceCreate(
    env: &mut Environment,
    _allocator: CFAllocatorRef,
    domain: CFStringRef,
    kind: CFStringRef,
    name: CFStringRef,
    port: i32,
) -> CFNetServiceRef {
    if domain.is_null() || kind.is_null() || name.is_null() || !(0..=65535).contains(&port) {
        return nil;
    }
    let domain = ns_string::to_rust_string(env, domain).into_owned();
    let kind = ns_string::to_rust_string(env, kind).into_owned();
    let name = ns_string::to_rust_string(env, name).into_owned();
    if kind.is_empty() {
        return nil;
    }
    make_service(
        env,
        if local_domain(&domain) {
            "local.".into()
        } else {
            domain
        },
        Service {
            name,
            kind,
            port: port as u16,
            txt: Vec::new(),
        },
        None,
        None,
    )
}
fn CFNetServiceSetClient(
    env: &mut Environment,
    object: id,
    callback: GuestFunction,
    ptr: ConstPtr<ClientContext>,
) -> bool {
    let ctx = if callback.to_ptr().is_null() {
        ClientContext::empty()
    } else {
        let Some(ctx) = context(env, ptr) else {
            return false;
        };
        ctx
    };
    let obj = env.objc.borrow_mut::<NetObject>(object);
    obj.generation += 1;
    let old = std::mem::replace(&mut obj.context, ctx);
    obj.callback = callback;
    old.dispose(env);
    true
}
fn schedule(env: &mut Environment, object: id, run_loop: CFRunLoopRef, mode: CFRunLoopMode) {
    if run_loop.is_null() || mode.is_null() {
        return;
    }
    let mode = ns_string::to_rust_string(env, mode).into_owned();
    let obj = env.objc.borrow_mut::<NetObject>(object);
    if matches!(&obj.body, Body::Browser(b) if b.invalidated) {
        return;
    }
    if !obj.scheduled.contains(&(run_loop, mode.clone())) {
        obj.scheduled.push((run_loop, mode));
        ns_run_loop::add_net_service_source(env, run_loop, object);
    }
}
fn unschedule(env: &mut Environment, object: id, run_loop: CFRunLoopRef, mode: CFRunLoopMode) {
    if mode.is_null() {
        return;
    }
    let mode = ns_string::to_rust_string(env, mode).into_owned();
    let obj = env.objc.borrow_mut::<NetObject>(object);
    obj.scheduled.retain(|(r, m)| *r != run_loop || *m != mode);
    if !obj.scheduled.iter().any(|(r, _)| *r == run_loop) {
        ns_run_loop::remove_net_service_source(env, run_loop, object);
    }
}
fn CFNetServiceScheduleWithRunLoop(
    env: &mut Environment,
    object: id,
    run_loop: CFRunLoopRef,
    mode: CFRunLoopMode,
) {
    schedule(env, object, run_loop, mode);
}
fn CFNetServiceUnscheduleFromRunLoop(
    env: &mut Environment,
    object: id,
    run_loop: CFRunLoopRef,
    mode: CFRunLoopMode,
) {
    unschedule(env, object, run_loop, mode);
}
fn CFNetServiceBrowserScheduleWithRunLoop(
    env: &mut Environment,
    object: id,
    run_loop: CFRunLoopRef,
    mode: CFRunLoopMode,
) {
    schedule(env, object, run_loop, mode);
}
fn CFNetServiceBrowserUnscheduleFromRunLoop(
    env: &mut Environment,
    object: id,
    run_loop: CFRunLoopRef,
    mode: CFRunLoopMode,
) {
    unschedule(env, object, run_loop, mode);
}
fn CFNetServiceCancel(env: &mut Environment, object: id) {
    let obj = env.objc.borrow_mut::<NetObject>(object);
    obj.generation += 1;
    let s = service_mut(env, object);
    s.pending.clear();
    s.resolving = None;
    if let Some(id) = s.registration.take() {
        if let Some(session) = session() {
            session.unregister(id);
        }
    }
}
fn CFNetServiceRegisterWithOptions(
    env: &mut Environment,
    object: id,
    options: CFOptionFlags,
    out: MutPtr<StreamError>,
) -> bool {
    if options & !1 != 0 {
        return error(env, out, BAD_ARGUMENT);
    }
    let Some(session) = session().filter(|s| s.is_connected()) else {
        return error(env, out, UNKNOWN);
    };
    if service(env, object).registration.is_some() || service(env, object).resolving.is_some() {
        return error(env, out, -72003);
    }
    let domain = ns_string::to_rust_string(env, service(env, object).domain);
    let s = service(env, object);
    if !local_domain(&domain)
        || !same_kind(&s.service.kind, "_CODWAW_IPHONE._tcp.")
        || s.service.port == 0
        || s.service.txt.len() > 4096
    {
        return error(env, out, BAD_ARGUMENT);
    }
    let resolved_name = if s.service.name.is_empty() {
        session.name.clone()
    } else {
        codwawzpc_coop_proto::sanitise_name(&s.service.name)
    };
    if resolved_name != s.service.name {
        let name = ns_string::from_rust_string(env, resolved_name.clone());
        let old_name = service(env, object).name;
        release(env, old_name);
        let s = service_mut(env, object);
        s.name = name;
        s.service.name = resolved_name;
    }
    let s = service_mut(env, object);
    s.owner = Some(session.ip);
    s.registration = Some(session.register(s.service.clone()));
    s.pending.push_back(0);
    log_dbg!(
        "CFNetService registered {:?}: {}:{}",
        object,
        s.service.name,
        s.service.port
    );
    error(env, out, 0)
}
fn CFNetServiceGetName(env: &mut Environment, object: id) -> CFStringRef {
    service(env, object).name
}
fn CFNetServiceGetType(env: &mut Environment, object: id) -> CFStringRef {
    service(env, object).kind
}
fn CFNetServiceGetDomain(env: &mut Environment, object: id) -> CFStringRef {
    service(env, object).domain
}
fn CFNetServiceGetPortNumber(env: &mut Environment, object: id) -> i32 {
    service(env, object).service.port.into()
}
fn CFNetServiceGetTargetHost(env: &mut Environment, object: id) -> CFStringRef {
    service(env, object).target
}
fn CFNetServiceGetTXTData(env: &mut Environment, object: id) -> CFDataRef {
    service(env, object).txt
}
fn CFNetServiceGetAddressing(env: &mut Environment, object: id) -> CFTypeRef {
    service(env, object).addresses
}
fn CFNetServiceSetTXTData(env: &mut Environment, object: id, txt: CFDataRef) -> bool {
    let bytes = read_data(env, txt);
    if bytes.len() > 4096 {
        return false;
    }
    let replacement = data(env, &bytes);
    let s = service_mut(env, object);
    let old = std::mem::replace(&mut s.txt, replacement);
    s.service.txt = bytes;
    if let Some(id) = s.registration {
        if let Some(session) = session().filter(|s| s.is_connected()) {
            session.unregister(id);
            s.registration = Some(session.register(s.service.clone()));
        }
    }
    release(env, old);
    true
}
fn CFNetServiceResolveWithTimeout(
    env: &mut Environment,
    object: id,
    timeout: f64,
    out: MutPtr<StreamError>,
) -> bool {
    if !timeout.is_finite() || timeout < 0.0 {
        return error(env, out, BAD_ARGUMENT);
    }
    if session().is_none_or(|s| !s.is_connected()) {
        return error(env, out, UNKNOWN);
    }
    if service(env, object).resolving.is_some() || service(env, object).registration.is_some() {
        return error(env, out, -72003);
    }
    let duration = Duration::try_from_secs_f64(if timeout == 0.0 { 1.0e9 } else { timeout }).ok();
    let Some(deadline) = duration.and_then(|d| Instant::now().checked_add(d)) else {
        return error(env, out, BAD_ARGUMENT);
    };
    service_mut(env, object).resolving = Some(deadline);
    error(env, out, 0)
}
fn CFNetServiceBrowserCreate(
    env: &mut Environment,
    _allocator: CFAllocatorRef,
    callback: GuestFunction,
    ptr: ConstPtr<ClientContext>,
) -> CFNetServiceBrowserRef {
    let Some(ctx) = context(env, ptr) else {
        return nil;
    };
    allocate(
        env,
        "_touchHLE_CFNetServiceBrowser",
        Body::Browser(BrowserState::default()),
        callback,
        ctx,
    )
}
fn CFNetServiceBrowserSearchForServices(
    env: &mut Environment,
    object: id,
    domain: CFStringRef,
    kind: CFStringRef,
    out: MutPtr<StreamError>,
) -> bool {
    if domain.is_null() || kind.is_null() {
        return error(env, out, BAD_ARGUMENT);
    }
    let domain = ns_string::to_rust_string(env, domain).into_owned();
    let kind = ns_string::to_rust_string(env, kind).into_owned();
    if !domain.is_empty()
        && !domain.eq_ignore_ascii_case("local.")
        && !domain.eq_ignore_ascii_case("local")
        || kind.is_empty()
    {
        return error(env, out, BAD_ARGUMENT);
    }
    let Some(session) = session().filter(|s| s.is_connected()) else {
        return error(env, out, UNKNOWN);
    };
    let b = browser_mut(env, object);
    if b.invalidated {
        return error(env, out, -72005);
    }
    if b.searching.is_some() {
        return error(env, out, -72003);
    }
    // Drain history to browsers that were already searching before starting
    // this browser. Old transient registrations must not appear as new peers.
    let main = CFRunLoopGetMain(env);
    let sources = ns_run_loop::net_service_sources(env, main);
    let events = session.take_listing_events();
    let listings: Vec<_> = session
        .listings()
        .into_iter()
        .map(BrowseItem::from)
        .collect();
    stage_browser_events(env, &sources, session.ip, true, &listings, &events);
    let b = browser_mut(env, object);
    b.searching = Some(kind.clone());
    b.failure = None;
    b.queue = BrowseQueue::default();
    b.pending = b.queue.reconcile(session.ip, &kind, &listings).into();
    log_dbg!("CFNetServiceBrowser search {:?}: {}", object, kind);
    error(env, out, 0)
}
fn CFNetServiceBrowserStopSearch(env: &mut Environment, object: id, out: MutPtr<StreamError>) {
    env.objc.borrow_mut::<NetObject>(object).generation += 1;
    let b = browser_mut(env, object);
    b.searching = None;
    b.pending.clear();
    b.queue = BrowseQueue::default();
    b.failure = None;
    error(env, out, 0);
}
fn CFNetServiceBrowserInvalidate(env: &mut Environment, object: id) {
    // Removing the run loop's reference may destroy the browser, so hold one.
    retain(env, object);
    CFNetServiceBrowserStopSearch(env, object, MutPtr::null());
    browser_mut(env, object).invalidated = true;
    let obj = env.objc.borrow_mut::<NetObject>(object);
    let scheduled = std::mem::take(&mut obj.scheduled);
    let ctx = std::mem::replace(&mut obj.context, ClientContext::empty());
    obj.callback = GuestFunction::null_ptr();
    for (run_loop, _) in scheduled {
        ns_run_loop::remove_net_service_source(env, run_loop, object);
    }
    ctx.dispose(env);
    release(env, object);
}
impl From<Listing> for BrowseItem {
    fn from(l: Listing) -> Self {
        Self {
            owner: l.owner,
            id: l.id,
            service: l.service,
        }
    }
}
fn resolve(env: &mut Environment, object: id, listings: &[BrowseItem], connected: bool) {
    let s = service(env, object);
    let Some(deadline) = s.resolving else {
        return;
    };
    let found = listings
        .iter()
        .find(|l| {
            same_kind(&l.service.kind, &s.service.kind)
                && l.service.name == s.service.name
                && s.owner.is_none_or(|owner| owner == l.owner)
                && s.listing_id.is_none_or(|id| id == l.id)
        })
        .cloned();
    if let Some(found) = found {
        let txt = data(env, &found.service.txt);
        let target =
            ns_string::from_rust_string(env, format!("codwawzpc-{}.local.", found.owner[3]));
        let mut address = [0u8; 16];
        address[0] = 16;
        address[1] = 2;
        address[2..4].copy_from_slice(&found.service.port.to_be_bytes());
        address[4..8].copy_from_slice(&found.owner);
        let addr_data = data(env, &address);
        let addresses: id = msg_class![env; NSMutableArray new];
        () = msg![env; addresses addObject:addr_data];
        release(env, addr_data);
        let s = service_mut(env, object);
        let old = [s.txt, s.target, s.addresses];
        s.txt = txt;
        s.target = target;
        s.addresses = addresses;
        s.owner = Some(found.owner);
        s.listing_id = Some(found.id);
        s.service = found.service;
        s.resolving = None;
        s.pending.push_back(0);
        for object in old {
            release(env, object);
        }
    } else if !connected || Instant::now() >= deadline {
        let s = service_mut(env, object);
        s.resolving = None;
        s.pending
            .push_back(if connected { TIMEOUT } else { NOT_FOUND });
    }
}
fn is_scheduled(env: &Environment, object: id, main: id) -> bool {
    env.objc
        .borrow::<NetObject>(object)
        .scheduled
        .iter()
        .any(|(r, _)| *r == main)
}
fn stage_browser_events(
    env: &mut Environment,
    objects: &[id],
    own: [u8; 4],
    connected: bool,
    listings: &[BrowseItem],
    events: &[ListingEvent],
) {
    for &object in objects {
        if !matches!(env.objc.borrow::<NetObject>(object).body, Body::Browser(_)) {
            continue;
        }

        let b = browser_mut(env, object);
        if let Some(kind) = b.searching.clone() {
            // Reconcile snapshots for late browsers and feed relay events
            // for transient register/unregister pairs between loop passes.
            for event in events {
                b.pending.extend(b.queue.apply(own, &kind, event));
            }
            b.pending.extend(b.queue.reconcile(own, &kind, listings));
            if !connected {
                b.failure = Some(UNKNOWN);
            }
        }
    }
}

/// Called by NSRunLoop on the emulated main thread, never by the relay thread.
pub fn deliver_callbacks(env: &mut Environment) {
    if env.current_thread != 0 {
        return;
    }
    let main = CFRunLoopGetMain(env);
    let objects = ns_run_loop::net_service_sources(env, main);
    // Keep the entire snapshot alive: callbacks may cancel or unschedule peers.
    for &object in &objects {
        retain(env, object);
    }
    let relay = session();
    let connected = relay.is_some_and(|s| s.is_connected());
    let own = relay.map_or([0; 4], |s| s.ip);
    let events = relay.map_or_else(Vec::new, |s| s.take_listing_events());
    let listings: Vec<_> = relay.map_or_else(Vec::new, |s| {
        s.listings().into_iter().map(BrowseItem::from).collect()
    });
    // Preparation is separate from delivery, so callbacks cannot make a newly
    // requested resolution or registration complete within the same pass.
    for &object in &objects {
        if !is_scheduled(env, object, main) {
            continue;
        }
        if matches!(env.objc.borrow::<NetObject>(object).body, Body::Service(_)) {
            let s = service_mut(env, object);
            if !connected && s.registration.take().is_some() {
                s.pending.clear();
                s.pending.push_back(UNKNOWN);
            }
            resolve(env, object, &listings, connected);
        }
    }
    stage_browser_events(env, &objects, own, connected, &listings, &events);
    let batches: Vec<_> = objects
        .into_iter()
        .map(|object| {
            let generation = env.objc.borrow::<NetObject>(object).generation;
            // Bound the batch: work initiated by callbacks runs next loop pass.
            let budget = match &env.objc.borrow::<NetObject>(object).body {
                Body::Service(s) => s.pending.len(),
                Body::Browser(b) => b.pending.len() + usize::from(b.failure.is_some()),
            };
            (object, generation, budget)
        })
        .collect();
    for (object, generation, budget) in batches {
        for _ in 0..budget {
            if !is_scheduled(env, object, main)
                || env.objc.borrow::<NetObject>(object).generation != generation
            {
                break;
            }
            let obj = env.objc.borrow_mut::<NetObject>(object);
            let callback = obj.callback;
            if callback.to_ptr().is_null() {
                break;
            }
            let ctx = obj.context;
            let event = match &mut obj.body {
                Body::Service(s) => s.pending.pop_front().map(|code| (None, 0, code)),
                Body::Browser(b) => {
                    if let Some((item, flags)) = b.pending.pop_front() {
                        Some((
                            Some(item),
                            (flags & !1) | u32::from(!b.pending.is_empty()),
                            0,
                        ))
                    } else {
                        b.failure.take().map(|code| {
                            b.searching = None;
                            (None, 0, code)
                        })
                    }
                }
            };
            let Some((item, flags, code)) = event else {
                break;
            };
            let error = env.mem.alloc_and_write(StreamError {
                domain: if code == 0 { 0 } else { 10 },
                error: code,
            });
            let retained_info = !ctx.retain.to_ptr().is_null();
            let info = if retained_info { ctx.acquire(env) } else { ctx };
            if !is_scheduled(env, object, main)
                || env.objc.borrow::<NetObject>(object).generation != generation
            {
                if retained_info {
                    info.dispose(env);
                }
                env.mem.free(error.cast());
                break;
            }
            if matches!(env.objc.borrow::<NetObject>(object).body, Body::Service(_)) {
                () = callback.call_from_host(env, (object, error, info.info));
            } else {
                log_dbg!(
                    "browse callback {:?}: {:?} flags {flags} code {code}",
                    object,
                    item
                );
                let result = item.map_or(nil, |i| {
                    make_service(env, "local.".into(), i.service, Some(i.owner), Some(i.id))
                });
                () = callback.call_from_host(env, (object, flags, result, error, info.info));
                release(env, result);
            }
            if retained_info {
                info.dispose(env);
            }
            env.mem.free(error.cast());
        }
        release(env, object);
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFNetServiceCreate(_, _, _, _, _)),
    export_c_func!(CFNetServiceSetClient(_, _, _)),
    export_c_func!(CFNetServiceScheduleWithRunLoop(_, _, _)),
    export_c_func!(CFNetServiceUnscheduleFromRunLoop(_, _, _)),
    export_c_func!(CFNetServiceRegisterWithOptions(_, _, _)),
    export_c_func!(CFNetServiceCancel(_)),
    export_c_func!(CFNetServiceGetName(_)),
    export_c_func!(CFNetServiceGetType(_)),
    export_c_func!(CFNetServiceGetDomain(_)),
    export_c_func!(CFNetServiceGetPortNumber(_)),
    export_c_func!(CFNetServiceGetTargetHost(_)),
    export_c_func!(CFNetServiceGetTXTData(_)),
    export_c_func!(CFNetServiceGetAddressing(_)),
    export_c_func!(CFNetServiceSetTXTData(_, _)),
    export_c_func!(CFNetServiceResolveWithTimeout(_, _, _)),
    export_c_func!(CFNetServiceBrowserCreate(_, _, _)),
    export_c_func!(CFNetServiceBrowserScheduleWithRunLoop(_, _, _)),
    export_c_func!(CFNetServiceBrowserUnscheduleFromRunLoop(_, _, _)),
    export_c_func!(CFNetServiceBrowserSearchForServices(_, _, _, _)),
    export_c_func!(CFNetServiceBrowserStopSearch(_, _)),
    export_c_func!(CFNetServiceBrowserInvalidate(_)),
];
