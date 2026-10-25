/* This Source Code Form is subject to the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! Virtual ports are independent of host ports, so multiple games can coexist.
use codwawzpc_coop_proto::{Proto, VAddr};
use std::{collections::HashMap, io, net::SocketAddr};

#[derive(Default)]
pub struct AddressMap {
    bound: HashMap<(Proto, u16), SocketAddr>,
    peers: HashMap<(Proto, SocketAddr), VAddr>,
    next: u16,
}
impl AddressMap {
    pub fn bind(&mut self, proto: Proto, requested: u16, real: SocketAddr) -> io::Result<u16> {
        let port = if requested == 0 {
            let mut chosen = None;
            for _ in 0..16384 {
                self.next = if self.next < 49152 || self.next == u16::MAX {
                    49152
                } else {
                    self.next + 1
                };
                if !self.bound.contains_key(&(proto, self.next)) {
                    chosen = Some(self.next);
                    break;
                }
            }
            chosen.ok_or_else(|| {
                io::Error::new(io::ErrorKind::AddrInUse, "virtual ports exhausted")
            })?
        } else {
            requested
        };
        if self.bound.contains_key(&(proto, port)) {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "virtual port already bound",
            ));
        }
        self.bound.insert((proto, port), real);
        Ok(port)
    }
    pub fn bound(&self, proto: Proto, port: u16) -> Option<SocketAddr> {
        self.bound.get(&(proto, port)).copied()
    }
    pub fn local_vport(&self, proto: Proto, real: SocketAddr) -> Option<u16> {
        self.bound
            .iter()
            .find_map(|(&(p, v), &r)| (p == proto && r == real).then_some(v))
    }
    pub fn unbind(&mut self, proto: Proto, port: u16) {
        self.bound.remove(&(proto, port));
    }
    pub fn peer(&mut self, proto: Proto, real: SocketAddr, virtual_addr: VAddr) {
        self.peers.insert((proto, real), virtual_addr);
    }
    pub fn inbound(&self, proto: Proto, real: SocketAddr) -> Option<VAddr> {
        self.peers.get(&(proto, real)).copied()
    }
    pub fn forget_peer(&mut self, proto: Proto, real: SocketAddr) {
        self.peers.remove(&(proto, real));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_then_lookup() {
        let mut map = AddressMap::default();
        let real = "127.0.0.1:2000".parse().unwrap();
        let port = map.bind(Proto::Udp, 0, real).unwrap();
        assert!(port >= 49152);
        assert_eq!(map.bound(Proto::Udp, port), Some(real));
        assert_eq!(map.local_vport(Proto::Udp, real), Some(port));
        assert!(map.bind(Proto::Udp, port, real).is_err());
        map.unbind(Proto::Udp, port);
        assert_eq!(map.bound(Proto::Udp, port), None);
    }
    #[test]
    fn inbound_proxy_maps_back_to_peer() {
        let mut map = AddressMap::default();
        let real = "127.0.0.1:2000".parse().unwrap();
        let v = VAddr {
            ip: [10, 77, 0, 2],
            port: 1234,
        };
        map.peer(Proto::Tcp, real, v);
        assert_eq!(map.inbound(Proto::Tcp, real), Some(v));
        map.forget_peer(Proto::Tcp, real);
        assert_eq!(map.inbound(Proto::Tcp, real), None);
    }
    #[test]
    fn tcp_and_udp_proxy_namespaces_are_independent() {
        let mut map = AddressMap::default();
        let real = "127.0.0.1:2000".parse().unwrap();
        let tcp = VAddr {
            ip: [10, 77, 0, 2],
            port: 100,
        };
        let udp = VAddr {
            ip: [10, 77, 0, 3],
            port: 200,
        };
        map.peer(Proto::Tcp, real, tcp);
        map.peer(Proto::Udp, real, udp);
        assert_eq!(map.inbound(Proto::Tcp, real), Some(tcp));
        assert_eq!(map.inbound(Proto::Udp, real), Some(udp));
        map.forget_peer(Proto::Tcp, real);
        assert_eq!(map.inbound(Proto::Udp, real), Some(udp));
    }
    #[test]
    fn unknown_proxy_port_is_none() {
        assert_eq!(
            AddressMap::default().inbound(Proto::Tcp, "127.0.0.1:1234".parse().unwrap()),
            None
        );
    }
}
