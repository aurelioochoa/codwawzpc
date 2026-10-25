/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `sys/socket.h` (Sockets)
//!
//! We currently support blocking TCP and UDP guest sockets on IPv4 addresses.
//!
//! Because fine grain control is needed, those are implemented as
//! _non-blocking_ host sockets. Moreover, app usage of select() is
//! (optimistically) assumed to check for data readiness before calling
//! any of blocking functions.
//! (Check related functions for more details and remediation.)
//!
//! Other note: Rust std::net APIs are "too high level" sometimes,
//! thus some workarounds need to be implemented.
//! (e.g. [TcpListener] does both bind() and listen() on a call
//! to [TcpListener::bind])
//!
//! Useful resources:
//! - [Beej's Guide to Network Programming](https://beej.us/guide/bgnet/html/index-wide.html)

use crate::codwawzpc::coop;
use crate::dyld::{export_c_func, FunctionExports};
use crate::libc::errno::{set_errno, EBADF, ECONNRESET, EINVAL, ENETUNREACH, EPROTONOSUPPORT};
use crate::libc::posix_io::{close, find_or_create_socket, is_socket, FileDescriptor};
use crate::libc::time::timeval;
use crate::mem::{
    guest_size_of, ConstPtr, ConstVoidPtr, GuestUSize, MutPtr, MutVoidPtr, Ptr, SafeRead,
};
use crate::Environment;
use codwawzpc_coop_proto::{Proto, VAddr};

use crate::abi::DotDotDot;
use crate::libc::netdb::{socklen_t, IPPROTO_TCP, IPPROTO_UDP};
use std::collections::{HashMap, HashSet};
use std::io;
use std::io::{Read, Write};
use std::net::{SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

pub const AF_INET: i32 = 2;
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;

const SOL_SOCKET: i32 = 0xffff;
const SO_DEBUG: i32 = 0x1;
const SO_REUSEADDR: i32 = 0x4;
const SO_BROADCAST: i32 = 0x20;
const SO_ERROR: i32 = 0x1007;

#[allow(non_camel_case_types)]
pub type sa_family_t = u8;

const FD_SETSIZE: i32 = 1024;

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct sockaddr {
    sa_len: u8,
    sa_family: sa_family_t,
    sa_data: [u8; 14],
}
unsafe impl SafeRead for sockaddr {}
impl sockaddr {
    /// Makes an IPv4 sockaddr from 4 bytes for ip and a port.
    ///
    /// Port is expected to be native endian and
    /// will be converted to big endian internally.
    pub fn from_ipv4_parts(octets: [u8; 4], port: u16) -> Self {
        let mut addr = sockaddr {
            sa_len: 16,
            sa_family: AF_INET as u8,
            sa_data: [0; 14],
        };
        addr.sa_data[0..2].copy_from_slice(&port.to_be_bytes());
        addr.sa_data[2..6].copy_from_slice(&octets);
        addr
    }
    /// Returns 4 bytes for ip and a port.
    ///
    /// Port is returned in the native endian format.
    fn to_ipv4_parts(self) -> ([u8; 4], u16) {
        assert!(self.sa_len == 16 || self.sa_len == 0);
        assert_eq!(self.sa_family, AF_INET as u8);
        let port = u16::from_be_bytes([self.sa_data[0], self.sa_data[1]]);
        let ip = [
            self.sa_data[2],
            self.sa_data[3],
            self.sa_data[4],
            self.sa_data[5],
        ];
        (ip, port)
    }
    fn from_sockaddr_v4(addr: &SocketAddr) -> Self {
        // Only IPV4 for the moment
        assert!(addr.is_ipv4());
        let SocketAddr::V4(ipv4addr) = addr else {
            unreachable!()
        };
        sockaddr::from_ipv4_parts(ipv4addr.ip().octets(), ipv4addr.port())
    }
    pub fn to_sockaddr_v4(self) -> SocketAddrV4 {
        let (ip, port) = self.to_ipv4_parts();
        SocketAddrV4::new(ip.into(), port)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct fd_set {
    // 32 4-byte ints should be enough for 1024 file descriptors
    fds_bits: [i32; 32],
}
unsafe impl SafeRead for fd_set {}

struct SocketHostObject {
    /// Type of the socket, [SOCK_STREAM] for TCP or [SOCK_DGRAM] for UDP
    type_: i32,
    virtual_port: Option<u16>,
    virtual_peer: Option<VAddr>,
    owns_binding: bool,
    /// Set of options
    options: HashSet<i32>,
    /// TCP socket which is yet to be connected
    tcp_listener: Option<TcpListener>,
    /// TCP socket which was connected on host, but not (yet) on the guest side
    pending_tcp_stream: Option<TcpStream>,
    /// Already connected TCP socket
    tcp_stream: Option<TcpStream>,
    /// UDP socket
    udp_socket: Option<UdpSocket>,
}

#[derive(Default)]
pub struct State {
    sockets: HashMap<i32, SocketHostObject>,
}
impl State {
    fn get(env: &Environment) -> &Self {
        &env.libc_state.socket
    }
    fn get_mut(env: &mut Environment) -> &mut Self {
        &mut env.libc_state.socket
    }
}

fn socket(env: &mut Environment, domain: i32, type_: i32, protocol: i32) -> FileDescriptor {
    // TODO: handle errno properly
    set_errno(env, 0);

    if !env.options.network_access {
        log_dbg!(
            "Network access is disabled, socket({}, {}, {}) => -1",
            domain,
            type_,
            protocol
        );
        set_errno(env, EPROTONOSUPPORT);
        return -1;
    }

    assert_eq!(domain, AF_INET);
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);
    assert!(protocol == IPPROTO_TCP || protocol == IPPROTO_UDP || protocol == 0);

    let fd = find_or_create_socket(env);
    assert!(!State::get(env).sockets.contains_key(&fd));
    let host_object = SocketHostObject {
        type_,
        virtual_port: None,
        virtual_peer: None,
        owns_binding: false,
        options: Default::default(),
        tcp_listener: None,
        pending_tcp_stream: None,
        tcp_stream: None,
        udp_socket: None,
    };
    State::get_mut(env).sockets.insert(fd, host_object);

    log_dbg!("socket({}, {}, {}) => {}", domain, type_, protocol, fd);
    fd
}

fn ioctl(env: &mut Environment, fd: i32, request: u32, _args: DotDotDot) -> i32 {
    if !is_socket(env, fd) {
        set_errno(env, EBADF);
        return -1;
    }
    if request == 0x8004667e {
        return 0;
    } // FIONBIO; all host sockets are nonblocking.
    set_errno(env, crate::libc::errno::ENOTSUP);
    log_dbg!("Unsupported socket ioctl({fd}, {request:#x})");
    -1
}

fn getsockopt(
    env: &mut Environment,
    socket: i32,
    level: i32,
    option_name: i32,
    option_value: MutVoidPtr,
    option_len: MutPtr<socklen_t>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "getsockopt({}, {:#x}, {:#x}, {:?}, {:?})",
        socket,
        level,
        option_name,
        option_value,
        option_len
    );

    let Some(host) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    if level != SOL_SOCKET || env.mem.read(option_len) < 4 {
        set_errno(env, EINVAL);
        return -1;
    }
    let value = match option_name {
        SO_ERROR => {
            if coop::session().is_some_and(|s| !s.is_connected()) {
                ECONNRESET
            } else {
                0
            }
        }
        0x1001 | 0x1002 => 65536,
        0x1008 => host.type_,
        SO_REUSEADDR | SO_BROADCAST | 8 | 0x10 | 0x1022 => {
            i32::from(host.options.contains(&option_name))
        }
        _ => {
            set_errno(env, 42);
            return -1;
        }
    };
    env.mem.write(option_value.cast::<i32>(), value);
    env.mem.write(option_len, 4);

    0 // Success
}

fn setsockopt(
    env: &mut Environment,
    socket: i32,
    level: i32,
    option_name: i32,
    option_value: ConstVoidPtr,
    option_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "setsockopt({}, {:#x}, {:#x}, {:?}, {})",
        socket,
        level,
        option_name,
        option_value,
        option_len
    );

    if !State::get(env).sockets.contains_key(&socket) {
        set_errno(env, EBADF);
        return -1;
    }
    if option_len != 4 {
        set_errno(env, EINVAL);
        return -1;
    }
    let enabled = env.mem.read(option_value.cast::<i32>()) != 0;
    if level == IPPROTO_TCP && option_name == 1 {
        if let Some(stream) = State::get(env).sockets[&socket].tcp_stream.as_ref() {
            if let Err(e) = stream.set_nodelay(enabled) {
                return socket_error(env, e);
            }
        }
        return 0;
    }
    // Loopback proxy sockets do not need OS keepalive/routing changes; the
    // relay heartbeat owns liveness. The guest's buffer hints are advisory.
    if level != SOL_SOCKET
        || ![
            SO_DEBUG,
            SO_REUSEADDR,
            SO_BROADCAST,
            8,
            0x10,
            0x1001,
            0x1002,
            0x1022,
        ]
        .contains(&option_name)
    {
        set_errno(env, 42);
        return -1;
    }
    let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
    if enabled {
        host.options.insert(option_name);
    } else {
        host.options.remove(&option_name);
    }
    if option_name == SO_BROADCAST && coop::session().is_none() {
        if let Some(udp) = host.udp_socket.as_ref() {
            if let Err(e) = udp.set_broadcast(enabled) {
                return socket_error(env, e);
            }
        }
    }

    0 // Success
}

fn getsockname(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    let Some(host) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let result = if let Some(session) = coop::session() {
        host.virtual_port
            .map(|port| sockaddr::from_ipv4_parts(session.ip, port))
    } else {
        host.udp_socket
            .as_ref()
            .and_then(|s| s.local_addr().ok())
            .or_else(|| host.tcp_listener.as_ref().and_then(|s| s.local_addr().ok()))
            .or_else(|| host.tcp_stream.as_ref().and_then(|s| s.local_addr().ok()))
            .map(|addr| sockaddr::from_sockaddr_v4(&addr))
    };
    let Some(addr) = result else {
        set_errno(env, EINVAL);
        return -1;
    };
    write_address(env, address, address_len, addr);
    0
}

fn getpeername(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    let Some(host) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let result = if coop::session().is_some() {
        host.virtual_peer
            .map(|v| sockaddr::from_ipv4_parts(v.ip, v.port))
    } else {
        host.tcp_stream
            .as_ref()
            .and_then(|s| s.peer_addr().ok())
            .map(|a| sockaddr::from_sockaddr_v4(&a))
    };
    let Some(addr) = result else {
        set_errno(env, 57);
        return -1;
    };
    write_address(env, address, address_len, addr);
    0
}
fn write_address(
    env: &mut Environment,
    address: MutPtr<sockaddr>,
    len: MutPtr<socklen_t>,
    value: sockaddr,
) {
    if address.is_null() || len.is_null() {
        return;
    }
    let count = env.mem.read(len).min(guest_size_of::<sockaddr>());
    let bytes = unsafe { std::slice::from_raw_parts((&value as *const sockaddr).cast::<u8>(), 16) };
    env.mem
        .bytes_at_mut(address.cast(), count)
        .copy_from_slice(&bytes[..count as usize]);
    env.mem.write(len, guest_size_of::<sockaddr>());
}
fn socket_error(env: &mut Environment, error: io::Error) -> i32 {
    let errno = match error.kind() {
        io::ErrorKind::WouldBlock => crate::libc::errno::EAGAIN,
        io::ErrorKind::AddrInUse => 48,
        io::ErrorKind::ConnectionRefused => 61,
        io::ErrorKind::NotConnected => 57,
        io::ErrorKind::TimedOut => 60,
        io::ErrorKind::ConnectionReset => 54,
        _ => 5,
    };
    log_dbg!("socket failed: {error}");
    set_errno(env, errno);
    -1
}

fn bind(
    env: &mut Environment,
    socket: i32,
    address: ConstPtr<sockaddr>,
    address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    assert_eq!(address_len, guest_size_of::<sockaddr>());
    let sockaddr_val = env.mem.read(address);
    log_dbg!(
        "bind({}, {:?} ({:?}), {})",
        socket,
        address,
        sockaddr_val,
        address_len
    );

    let socket_address = sockaddr_val.to_sockaddr_v4();
    if let Some(session) = coop::session() {
        if !session.is_connected() {
            set_errno(env, 50);
            return -1;
        }
        let ip = socket_address.ip().octets();
        if ip != [0; 4] && ip != session.ip {
            set_errno(env, 49);
            return -1;
        }
        let result = (|| -> io::Result<()> {
            let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
            if host.virtual_port.is_some() {
                return Err(io::ErrorKind::AddrInUse.into());
            }
            let (proto, real) = if type_ == SOCK_STREAM {
                let listener = TcpListener::bind("127.0.0.1:0")?;
                listener.set_nonblocking(true)?;
                let real = listener.local_addr()?;
                host.tcp_listener = Some(listener);
                (Proto::Tcp, real)
            } else {
                let udp = UdpSocket::bind("127.0.0.1:0")?;
                udp.set_nonblocking(true)?;
                let real = udp.local_addr()?;
                host.udp_socket = Some(udp);
                (Proto::Udp, real)
            };
            let port = session.bind(proto, socket_address.port(), real)?;
            host.virtual_port = Some(port);
            host.owns_binding = true;
            Ok(())
        })();
        return match result {
            Ok(()) => 0,
            Err(e) => socket_error(env, e),
        };
    }
    let type_str = match type_ {
        SOCK_STREAM => "TCP",
        SOCK_DGRAM => "UDP",
        _ => unreachable!(),
    };
    log_dbg!("bind: {} socket address {:?}", type_str, socket_address);

    // re-borrow
    let socket_host_object = State::get(env).sockets.get(&socket).unwrap();
    match type_ {
        SOCK_STREAM => {
            assert!(socket_host_object.tcp_listener.is_none());
            let host_socket = TcpListener::bind(socket_address).unwrap();
            // We set host socket as non-blocking in order to have
            // more control of how and when it's used
            host_socket.set_nonblocking(true).unwrap();
            // TODO: set options
            State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .tcp_listener = Some(host_socket);
        }
        SOCK_DGRAM => {
            assert!(socket_host_object.udp_socket.is_none());
            let host_socket = UdpSocket::bind(socket_address).unwrap();
            // We set host socket as non-blocking in order to have
            // more control of how and when it's used
            host_socket.set_nonblocking(true).unwrap();
            for &option in &socket_host_object.options {
                if option == SO_BROADCAST {
                    host_socket.set_broadcast(true).unwrap();
                }
            }
            State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .udp_socket = Some(host_socket);
        }
        _ => unreachable!(),
    }

    0 // Success
}

fn listen(env: &mut Environment, socket: i32, backlog: i32) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM);

    log!(
        "Warning: listen(socket: {}, backlog: {}), ignoring",
        socket,
        backlog
    );
    0 // Success
}

fn connect(
    env: &mut Environment,
    socket: i32,
    address: ConstPtr<sockaddr>,
    address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    assert_eq!(address_len, guest_size_of::<sockaddr>());
    let sockaddr_val = env.mem.read(address);
    log_dbg!(
        "connect({:?} ({:?}), {})",
        address,
        sockaddr_val,
        address_len
    );

    let socket_address = sockaddr_val.to_sockaddr_v4();
    log_dbg!("connect: socket address {:?}", socket_address);

    if let Some(session) = coop::session() {
        let dst = VAddr {
            ip: socket_address.ip().octets(),
            port: socket_address.port(),
        };
        // UDP connect only chooses a default destination; it sends no traffic.
        // The loader uses an external numeric address to query its local IP.
        if type_ == SOCK_STREAM && dst.ip[..3] != [10, 77, 0] {
            set_errno(env, 61);
            return -1;
        }
        if type_ == SOCK_DGRAM {
            let result = (|| -> io::Result<()> {
                let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
                if host.virtual_port.is_none() {
                    let udp = UdpSocket::bind("127.0.0.1:0")?;
                    udp.set_nonblocking(true)?;
                    let port = session.bind(Proto::Udp, 0, udp.local_addr()?)?;
                    host.udp_socket = Some(udp);
                    host.virtual_port = Some(port);
                    host.owns_binding = true;
                }
                host.virtual_peer = Some(dst);
                Ok(())
            })();
            return match result {
                Ok(()) => 0,
                Err(e) => socket_error(env, e),
            };
        }
        let requested = {
            let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
            if host.tcp_stream.is_some() {
                set_errno(env, 56);
                return -1;
            }
            let requested = host.virtual_port.unwrap_or(0);
            if host.owns_binding {
                session.unbind(Proto::Tcp, requested);
            }
            host.tcp_listener = None;
            host.virtual_port = None;
            host.owns_binding = false;
            requested
        };
        return match session.connect_stream(requested, dst) {
            Ok((stream, port)) => {
                if let Err(e) = stream.set_nonblocking(true) {
                    return socket_error(env, e);
                }
                let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
                host.tcp_stream = Some(stream);
                host.virtual_port = Some(port);
                host.virtual_peer = Some(dst);
                host.owns_binding = true;
                // Report a nonblocking connect in progress: the loader treats
                // an immediate success as a failure, and polls connect until
                // it returns EISCONN (below).
                set_errno(env, crate::libc::errno::EINPROGRESS);
                -1
            }
            Err(e) => socket_error(env, e),
        };
    }
    assert!(State::get(env)
        .sockets
        .get(&socket)
        .unwrap()
        .tcp_stream
        .is_none());
    let host_stream = TcpStream::connect(socket_address).unwrap();
    // We set host socket as non-blocking in order to have
    // more control of how and when it's used
    host_stream.set_nonblocking(true).unwrap();
    State::get_mut(env)
        .sockets
        .get_mut(&socket)
        .unwrap()
        .tcp_stream = Some(host_stream);

    0 // Success
}

fn select(
    env: &mut Environment,
    n_fds: i32,
    read_fds: MutPtr<fd_set>,
    write_fds: MutPtr<fd_set>,
    error_fds: MutPtr<fd_set>,
    timeout: MutPtr<timeval>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    if !(0..=FD_SETSIZE).contains(&n_fds) {
        set_errno(env, EINVAL);
        return -1;
    }

    if n_fds == 0 {
        // Apparently, this is a portable way of using select() as usleep()
        // https://stackoverflow.com/questions/3125645/why-use-select-instead-of-sleep
        // ¯\_(ツ)_/¯
        assert!(read_fds.is_null());
        assert!(write_fds.is_null());
        assert!(error_fds.is_null());
        let timeval = env.mem.read(timeout);
        let duration = Duration::from_secs(timeval.tv_sec.try_into().unwrap())
            + Duration::from_micros(timeval.tv_usec.try_into().unwrap());
        log_dbg!("select() used as sleep for {:?}", duration);
        env.sleep(duration);
        return 0;
    }

    let should_block = if !timeout.is_null() {
        let timeval = env.mem.read(timeout);
        let tv_sec = timeval.tv_sec;
        let tv_usec = timeval.tv_usec;
        if tv_sec == 0 && tv_usec == 0 {
            // Happy path, just polling once
            false
        } else {
            log_dbg!("TODO: Ignore non-zero timeout {:?} in select()", timeval);
            true
        }
    } else {
        true
    };

    let mut count = 0;

    if !read_fds.is_null() {
        let mut read_set = env.mem.read(read_fds);
        log_dbg!("select: read_set before {:?}", read_set);
        count += process_set(env, &mut read_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in read_set at fd: {}", fd);
            // Only sockets for now
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_DGRAM => {
                    let Some(udp_socket) = socket_host_object.udp_socket.as_ref() else {
                        return false;
                    };
                    // Peek just one byte to check if we have some data
                    let mut buf = [0; 1];
                    match udp_socket.peek(&mut buf) {
                        Ok(received) => {
                            log_dbg!("select: Socket {} peeked {} bytes", fd, received);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // On Windows, if we receive more bytes than we peek,
                        // it will error, but it means that there is some data!
                        Err(ref e)
                            if cfg!(target_os = "windows") && e.raw_os_error() == Some(10040) =>
                        {
                            // 10040 code is WSAEMSGSIZE
                            log_dbg!(
                                "[Windows case] select: received {} bytes (at least)",
                                buf.len()
                            );
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            log_dbg!("select: Socket {} would block on peeking, continue.", fd);
                            let _ = should_block; // Guest run loop retries readiness.
                            false
                        }
                        Err(e) => {
                            panic!("select: Peek for socket {fd} failed: {e:?}")
                        }
                    }
                }
                SOCK_STREAM => {
                    if socket_host_object.tcp_stream.is_none() {
                        // If we don't have a TCP stream it probably means
                        // that a listener is waiting for connection
                        if socket_host_object.pending_tcp_stream.is_some() {
                            *bits |= 1 << bit_index;
                            return true;
                        }
                        let Some(listener) = socket_host_object.tcp_listener.as_ref() else {
                            return false;
                        };
                        // The listener is non-blocking,
                        // so we can try to accept
                        match listener.accept() {
                            Ok((stream, addr)) => {
                                log!("select: New client: {}", addr);
                                // We set host socket as non-blocking in order
                                // to have more control of how and when it's
                                // used
                                stream.set_nonblocking(true).unwrap();
                                // We already accepted the connection on
                                // the host, but we need to postpone new
                                // guest fd creation up until guest calls
                                // accept()
                                assert!(socket_host_object.pending_tcp_stream.is_none());
                                State::get_mut(env)
                                    .sockets
                                    .get_mut(&fd)
                                    .unwrap()
                                    .pending_tcp_stream = Some(stream);
                                // Set bit back
                                *bits |= 1 << bit_index;
                                return true;
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                // No incoming connection is ready
                                log_dbg!("select: TCP listener for socket {} would block on accepting, continue.", fd);
                                let _ = should_block; // Guest run loop retries readiness.
                                return false;
                            }
                            Err(e) => {
                                panic!("select: Socket {fd} has error accepting connection: {e}");
                            }
                        }
                    }
                    let stream = socket_host_object.tcp_stream.as_ref().unwrap();
                    // Peek just one byte to check if we have some data
                    let mut buf = [0; 1];
                    match stream.peek(&mut buf) {
                        Ok(received) => {
                            log_dbg!("select: received {} bytes (at least)", received);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // On Windows, if we receive more bytes than we peek,
                        // it will error, but it means that there is some data!
                        Err(ref e)
                            if cfg!(target_os = "windows") && e.raw_os_error() == Some(10040) =>
                        {
                            // 10040 code is WSAEMSGSIZE
                            log_dbg!(
                                "[Windows case] select: received {} bytes (at least)",
                                buf.len()
                            );
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        // As tested on macOS, this marks socket as readable
                        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                            log!("select: Peek for socket {}: ConnectionReset", fd);
                            // Set bit back
                            *bits |= 1 << bit_index;
                            true
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            log_dbg!(
                                "select: TCP stream for socket {} would block on peeking, continue.",
                                fd
                            );
                            let _ = should_block; // Guest run loop retries readiness.
                            false
                        }
                        Err(e) => {
                            panic!("select: Peek for socket {fd} failed: {e}")
                        }
                    }
                }
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: read_set after {:?}", read_set);
        env.mem.write(read_fds, read_set);
    }

    if !write_fds.is_null() {
        let mut write_set = env.mem.read(write_fds);
        log_dbg!("select: write_set before {:?}", write_set);
        count += process_set(env, &mut write_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in write_set at fd: {}", fd);
            // Only sockets for now
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_STREAM => {
                    // As we cannot "peek" on write, we just check
                    // if TCP stream exist or not
                    // TODO: find a better way
                    if socket_host_object.tcp_stream.is_some() {
                        // Set bit back
                        *bits |= 1 << bit_index;
                        true
                    } else {
                        let _ = should_block; // Guest run loop retries readiness.
                        false
                    }
                }
                SOCK_DGRAM => {
                    // As we cannot "peek" on write, we just check
                    // if UDP socket exist or not
                    // TODO: find a better way
                    if socket_host_object.udp_socket.is_some() {
                        // Set bit back
                        *bits |= 1 << bit_index;
                        true
                    } else {
                        let _ = should_block; // Guest run loop retries readiness.
                        false
                    }
                }
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: write_set after {:?}", write_set);
        env.mem.write(write_fds, write_set);
    }

    if !error_fds.is_null() {
        let mut error_set = env.mem.read(error_fds);
        log_dbg!("select: error_set before {:?}", error_set);
        count += process_set(env, &mut error_set, n_fds, |env, fd, bits, bit_index| {
            log_dbg!("select: bit set in error_set at fd: {}", fd);
            // Only sockets for now
            assert!(is_socket(env, fd));
            // Clean bit in the set for the current socket
            *bits &= !(1 << bit_index);
            let socket_host_object = State::get(env).sockets.get(&fd).unwrap();
            let type_ = socket_host_object.type_;
            match type_ {
                SOCK_STREAM => {
                    let Some(stream) = socket_host_object.tcp_stream.as_ref() else {
                        return false;
                    };
                    match stream.take_error() {
                        Ok(None) => {
                            log_dbg!("No error on TCP socket {}", fd);
                            false
                        }
                        Ok(Some(_)) => {
                            *bits |= 1 << bit_index;
                            true
                        }
                        Err(error) => panic!("TCP socket {fd} take_error failed: {error:?}"),
                    }
                }
                SOCK_DGRAM => false,
                _ => unimplemented!(),
            }
        });
        log_dbg!("select: error_set after {:?}", error_set);
        env.mem.write(error_fds, error_set);
    }

    count
}

fn process_set<F: Fn(&mut Environment, FileDescriptor, &mut i32, i32) -> bool>(
    env: &mut Environment,
    set: &mut fd_set,
    n_fds: i32,
    process_bit: F,
) -> i32 {
    let mut fds_bits = set.fds_bits;
    let mut count = 0;
    'outer: for (i, bits) in fds_bits.iter_mut().enumerate() {
        for bit_index in 0..32i32 {
            let fd: FileDescriptor = (i as i32) * 32 + bit_index;
            if fd > n_fds {
                break 'outer;
            }
            if (*bits & (1 << bit_index)) != 0 && process_bit(env, fd, bits, bit_index) {
                count += 1;
            }
        }
    }
    set.fds_bits = fds_bits;
    count
}

fn accept(
    env: &mut Environment,
    socket: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> FileDescriptor {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(socket_host_object) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = socket_host_object.type_;
    assert!(type_ == SOCK_STREAM);

    let pending = State::get_mut(env)
        .sockets
        .get_mut(&socket)
        .unwrap()
        .pending_tcp_stream
        .take();
    let stream = match pending {
        Some(s) => s,
        None => match State::get(env)
            .sockets
            .get(&socket)
            .unwrap()
            .tcp_listener
            .as_ref()
            .unwrap()
            .accept()
        {
            Ok((stream, _)) => stream,
            Err(e) => return socket_error(env, e),
        },
    };
    if let Err(e) = stream.set_nonblocking(true) {
        return socket_error(env, e);
    }
    let addr = match stream.peer_addr() {
        Ok(a) => a,
        Err(e) => return socket_error(env, e),
    };
    let local = State::get(env).sockets.get(&socket).unwrap().virtual_port;
    let peer = coop::session().and_then(|s| s.inbound(Proto::Tcp, addr));
    let guest_addr = if coop::session().is_some() {
        let Some(v) = peer else {
            State::get_mut(env)
                .sockets
                .get_mut(&socket)
                .unwrap()
                .pending_tcp_stream = Some(stream);
            set_errno(env, crate::libc::errno::EAGAIN);
            return -1;
        };
        sockaddr::from_ipv4_parts(v.ip, v.port)
    } else {
        sockaddr::from_sockaddr_v4(&addr)
    };
    let new_fd = find_or_create_socket(env);
    State::get_mut(env).sockets.insert(
        new_fd,
        SocketHostObject {
            type_: SOCK_STREAM,
            virtual_port: local,
            virtual_peer: peer,
            owns_binding: false,
            options: Default::default(),
            tcp_listener: None,
            pending_tcp_stream: None,
            tcp_stream: Some(stream),
            udp_socket: None,
        },
    );
    write_address(env, address, address_len, guest_addr);
    new_fd
}

fn recv(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
) -> i32 {
    recvfrom(env, socket, buffer, length, flags, Ptr::null(), Ptr::null())
}

fn recvfrom(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
    address: MutPtr<sockaddr>,
    address_len: MutPtr<socklen_t>,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "recvfrom({}, {:?}, {}, {}, {:?}, {:?})",
        socket,
        buffer,
        length,
        flags,
        address,
        address_len
    );

    if !State::get(env).sockets.contains_key(&socket) {
        set_errno(env, EBADF);
        log!(
            "Warning: recvfrom({}, ...) failed for unknown socket, returning -1",
            socket
        );
        return -1;
    }

    let type_ = State::get(env).sockets.get(&socket).unwrap().type_;
    assert!(type_ == SOCK_STREAM || type_ == SOCK_DGRAM);

    // The loader passes MSG_TRUNC on its nonblocking UDP reads. Input
    // truncation is advisory here; MSG_PEEK must leave the packet queued.
    if flags & !(0x2 | 0x10 | 0x80) != 0 {
        set_errno(env, crate::libc::errno::ENOTSUP);
        return -1;
    }

    let host = State::get(env).sockets.get(&socket).unwrap();
    if (type_ == SOCK_DGRAM && host.udp_socket.is_none())
        || (type_ == SOCK_STREAM && host.tcp_stream.is_none())
    {
        set_errno(env, 57);
        return -1;
    }
    let (num_bytes_read, addr) = match type_ {
        SOCK_DGRAM => {
            let udp_socket = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at_mut(buffer.cast(), length);
            let result = if flags & 2 != 0 {
                udp_socket.peek_from(buf)
            } else {
                udp_socket.recv_from(buf)
            };
            let (read, addr) = match result {
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data is ready
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    set_errno(env, crate::libc::errno::EAGAIN);
                    return -1;
                }
                Err(e) => return socket_error(env, e),
            };
            let guest_addr = if let Some(session) = coop::session() {
                let Some(v) = session.inbound(Proto::Udp, addr) else {
                    set_errno(env, crate::libc::errno::EAGAIN);
                    return -1;
                };
                sockaddr::from_ipv4_parts(v.ip, v.port)
            } else {
                sockaddr::from_sockaddr_v4(&addr)
            };
            if !address.is_null() {
                env.mem.write(address, guest_addr);
                assert_eq!(guest_size_of::<sockaddr>(), env.mem.read(address_len));
                env.mem.write(address_len, guest_size_of::<sockaddr>());
            }
            (read, Ok(addr))
        }
        SOCK_STREAM => {
            assert!(address.is_null());
            assert!(address_len.is_null());
            let mut tcp_stream = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .tcp_stream
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at_mut(buffer.cast(), length);
            let result = if flags & 2 != 0 {
                tcp_stream.peek(buf)
            } else {
                tcp_stream.read(buf)
            };
            let read = match result {
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    set_errno(env, ECONNRESET);
                    log!("recvfrom: TCP socket {}: ConnectionReset => -1", socket);
                    return -1;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data is ready
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    set_errno(env, crate::libc::errno::EAGAIN);
                    return -1;
                }
                Err(e) => return socket_error(env, e),
            };
            (read, tcp_stream.peer_addr())
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "recvfrom: Socket {} received {} bytes from addr {:?}",
        socket,
        num_bytes_read,
        addr.ok()
    );
    num_bytes_read.try_into().unwrap()
}

fn send(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(host) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = host.type_;
    if type_ == SOCK_DGRAM {
        let host = State::get(env).sockets.get(&socket).unwrap();
        if let (Some(session), Some(port), Some(dst)) =
            (coop::session(), host.virtual_port, host.virtual_peer)
        {
            return match session.send_datagram(port, dst, env.mem.bytes_at(buffer.cast(), length)) {
                Ok(n) => n as i32,
                Err(e) => socket_error(env, e),
            };
        }
        set_errno(env, 57);
        return -1;
    }
    assert!(type_ == SOCK_STREAM);

    if State::get(env).sockets[&socket].tcp_stream.is_none() {
        set_errno(env, 57);
        return -1;
    }
    assert_eq!(flags, 0); // TODO

    let num_bytes_written = match type_ {
        SOCK_STREAM => {
            let mut tcp_stream = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .tcp_stream
                .as_ref()
                .unwrap();
            let buf = env.mem.bytes_at(buffer.cast(), length);
            match tcp_stream.write(buf) {
                Ok(written) => written,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    set_errno(env, crate::libc::errno::EAGAIN);
                    return -1;
                }
                Err(e) => return socket_error(env, e),
            }
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "send: written {} bytes to TCP socket {}",
        num_bytes_written,
        socket
    );
    num_bytes_written.try_into().unwrap()
}

/// With a relay session, datagrams reach only the relay's virtual network
/// (`10.77.0.0/24`) and broadcast; anything else has no route.
fn relay_datagram_errno(dst: [u8; 4]) -> Option<i32> {
    if dst[..3] == [10, 77, 0] || dst == [255; 4] {
        None
    } else {
        Some(ENETUNREACH)
    }
}

fn sendto(
    env: &mut Environment,
    socket: i32,
    buffer: MutVoidPtr,
    length: GuestUSize,
    flags: i32,
    dest_address: MutPtr<sockaddr>,
    dest_address_len: socklen_t,
) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    let Some(host) = State::get(env).sockets.get(&socket) else {
        set_errno(env, EBADF);
        return -1;
    };
    let type_ = host.type_;
    assert!(type_ == SOCK_DGRAM);

    assert_eq!(flags, 0); // TODO

    assert_eq!(dest_address_len, guest_size_of::<sockaddr>());
    let sockaddr_val = env.mem.read(dest_address);
    let socket_address = sockaddr_val.to_sockaddr_v4();
    log_dbg!(
        "sendto({}, {:?}, {}, {}, {:?} ({:?}, {:?}), {})",
        socket,
        buffer,
        length,
        flags,
        dest_address,
        sockaddr_val,
        socket_address,
        dest_address_len
    );

    if let Some(session) = coop::session() {
        let dst = VAddr {
            ip: socket_address.ip().octets(),
            port: socket_address.port(),
        };
        if let Some(errno) = relay_datagram_errno(dst.ip) {
            set_errno(env, errno);
            return -1;
        }
        let binding = (|| -> io::Result<u16> {
            let host = State::get_mut(env).sockets.get_mut(&socket).unwrap();
            if let Some(port) = host.virtual_port {
                return Ok(port);
            }
            let udp = UdpSocket::bind("127.0.0.1:0")?;
            udp.set_nonblocking(true)?;
            let port = session.bind(Proto::Udp, 0, udp.local_addr()?)?;
            host.udp_socket = Some(udp);
            host.virtual_port = Some(port);
            host.owns_binding = true;
            Ok(port)
        })();
        let port = match binding {
            Ok(p) => p,
            Err(e) => return socket_error(env, e),
        };
        return match session.send_datagram(port, dst, env.mem.bytes_at(buffer.cast(), length)) {
            Ok(n) => n as i32,
            Err(e) => socket_error(env, e),
        };
    }
    let num_bytes_written = match type_ {
        SOCK_DGRAM => {
            if State::get(env)
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .is_none()
            {
                // For the case of broadcast we allow a lazy host UDP socket
                // creation
                assert!(socket_address.ip().is_broadcast());
                // TODO: is it a correct address to bind?
                let host_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
                assert!(host_socket.local_addr().unwrap().ip().is_unspecified());
                // We set host socket as non-blocking in order to have
                // more control of how and when it's used
                host_socket.set_nonblocking(true).unwrap();
                for &option in &State::get(env).sockets.get(&socket).unwrap().options {
                    if option == SO_BROADCAST {
                        host_socket.set_broadcast(true).unwrap();
                    }
                }
                assert!(host_socket.broadcast().unwrap());
                State::get_mut(env)
                    .sockets
                    .get_mut(&socket)
                    .unwrap()
                    .udp_socket = Some(host_socket);
            }
            let udp_socket = env
                .libc_state
                .socket
                .sockets
                .get(&socket)
                .unwrap()
                .udp_socket
                .as_ref()
                .unwrap();
            if socket_address.ip().is_broadcast() {
                assert!(udp_socket.local_addr().unwrap().ip().is_unspecified());
            }
            let buf = env.mem.bytes_at(buffer.cast(), length);
            match udp_socket.send_to(buf, socket_address) {
                Ok(written) => written,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // TODO: if this happened, take a deep breath and do:
                    // - block guest thread with a new [ThreadBlock] type
                    // - poll for data in thread scheduling part
                    // - write/read/accept/etc data once it is ready
                    // - unblock guest thread
                    unimplemented!("sendto: UDP socket {} would block on sending, block current guest thread {}.", socket, env.current_thread)
                }
                Err(e) => panic!("sendto: Socket {socket} encountered IO error: {e}"),
            }
        }
        _ => unreachable!(),
    };
    log_dbg!(
        "sendto: written {} bytes to UDP socket {} (address {:?})",
        num_bytes_written,
        socket,
        socket_address
    );
    num_bytes_written.try_into().unwrap()
}

const SHUT_RDWR: i32 = 2;
fn shutdown(env: &mut Environment, socket: i32, how: i32) -> i32 {
    log_dbg!("shutdown({}, {})", socket, how);
    assert_eq!(how, SHUT_RDWR);
    close(env, socket)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(socket(_, _, _)),
    export_c_func!(ioctl(_, _, _)),
    export_c_func!(getsockopt(_, _, _, _, _)),
    export_c_func!(setsockopt(_, _, _, _, _)),
    export_c_func!(getsockname(_, _, _)),
    export_c_func!(getpeername(_, _, _)),
    export_c_func!(bind(_, _, _)),
    export_c_func!(listen(_, _)),
    export_c_func!(connect(_, _, _)),
    export_c_func!(select(_, _, _, _, _)),
    export_c_func!(accept(_, _, _)),
    export_c_func!(recv(_, _, _, _)),
    export_c_func!(recvfrom(_, _, _, _, _, _)),
    export_c_func!(send(_, _, _, _)),
    export_c_func!(sendto(_, _, _, _, _, _)),
    export_c_func!(shutdown(_, _)),
];

/// A helper to close a socket, not a part of API
pub fn close_socket(env: &mut Environment, socket: i32) -> bool {
    let Some(host) = State::get_mut(env).sockets.remove(&socket) else {
        return false;
    };
    if host.owns_binding {
        if let (Some(session), Some(port)) = (coop::session(), host.virtual_port) {
            session.unbind(
                if host.type_ == SOCK_DGRAM {
                    Proto::Udp
                } else {
                    Proto::Tcp
                },
                port,
            );
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_datagrams_outside_the_virtual_network_are_unreachable() {
        assert_eq!(relay_datagram_errno([8, 8, 8, 8]), Some(ENETUNREACH));
        assert_eq!(relay_datagram_errno([192, 168, 1, 1]), Some(ENETUNREACH));
        assert_eq!(relay_datagram_errno([10, 77, 1, 2]), Some(ENETUNREACH));
    }

    #[test]
    fn relay_datagrams_to_peers_and_broadcast_are_sent() {
        assert_eq!(relay_datagram_errno([10, 77, 0, 3]), None);
        assert_eq!(relay_datagram_errno([255, 255, 255, 255]), None);
    }
}
