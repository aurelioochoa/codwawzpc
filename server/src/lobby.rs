//! Deterministic lobby state. All callers serialize access; no networking here.
use codwawzpc_coop_proto::{sanitise_name, truncate_utf8, Frame, Service, VAddr, VERSION};
use std::collections::BTreeMap;
pub type ClientId = u8;
pub const MAX_SERVICES_PER_CLIENT: usize = 64;
pub const MAX_STREAMS_PER_CLIENT: usize = 1024;
#[derive(Debug, Clone)]
pub struct Client {
    pub ip: [u8; 4],
    pub name: String,
    services: BTreeMap<u32, Service>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub to: ClientId,
    pub frame: Frame,
}
#[derive(Debug)]
struct Stream {
    a: (ClientId, u32),
    b: (ClientId, u32),
    accepted: bool,
}
#[derive(Default)]
pub struct Lobby {
    clients: BTreeMap<ClientId, Client>,
    streams: Vec<Stream>,
    pending: Vec<Outgoing>,
}
impl Lobby {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn client(&self, id: ClientId) -> Option<&Client> {
        self.clients.get(&id)
    }
    /// Drain Welcome and snapshot listings queued by `join`.
    pub fn take_outgoing(&mut self) -> Vec<Outgoing> {
        std::mem::take(&mut self.pending)
    }
    pub fn join(&mut self, name: impl AsRef<str>) -> Result<ClientId, String> {
        let id = (1..=254)
            .find(|id| !self.clients.contains_key(id))
            .ok_or("lobby full (254 players)")?;
        let base = sanitise_name(name.as_ref());
        let mut name = base.clone();
        let mut n = 2;
        while self.clients.values().any(|c| c.name == name) {
            let suffix = format!(" ({n})");
            name = base.clone();
            truncate_utf8(&mut name, 63 - suffix.len());
            name.push_str(&suffix);
            n += 1;
        }
        let ip = [10, 77, 0, id];
        self.pending.push(Outgoing {
            to: id,
            frame: Frame::Welcome {
                ip,
                name: name.clone(),
            },
        });
        for client in self.clients.values() {
            for (&service_id, service) in &client.services {
                self.pending.push(Outgoing {
                    to: id,
                    frame: Frame::Listed {
                        owner: client.ip,
                        id: service_id,
                        service: service.clone(),
                    },
                });
            }
        }
        self.clients.insert(
            id,
            Client {
                ip,
                name,
                services: BTreeMap::new(),
            },
        );
        Ok(id)
    }
    fn broadcast(&self, except: ClientId, frame: Frame) -> Vec<Outgoing> {
        self.clients
            .keys()
            .filter(|&&id| id != except)
            .map(|&to| Outgoing {
                to,
                frame: frame.clone(),
            })
            .collect()
    }
    fn error(to: ClientId, message: impl Into<String>) -> Vec<Outgoing> {
        vec![Outgoing {
            to,
            frame: Frame::Error {
                message: message.into(),
            },
        }]
    }
    fn close(to: ClientId, stream: u32) -> Vec<Outgoing> {
        vec![Outgoing {
            to,
            frame: Frame::StreamClose { stream },
        }]
    }
    pub fn leave(&mut self, id: ClientId) -> Vec<Outgoing> {
        let Some(client) = self.clients.remove(&id) else {
            return vec![];
        };
        self.pending.retain(|o| o.to != id);
        let mut out = Vec::new();
        for service_id in client.services.keys() {
            out.extend(self.broadcast(
                id,
                Frame::Unlisted {
                    owner: client.ip,
                    id: *service_id,
                },
            ));
        }
        self.streams.retain(|s| {
            let peer = if s.a.0 == id {
                Some(s.b)
            } else if s.b.0 == id {
                Some(s.a)
            } else {
                None
            };
            if let Some((to, stream)) = peer {
                out.push(Outgoing {
                    to,
                    frame: Frame::StreamClose { stream },
                });
                false
            } else {
                true
            }
        });
        out
    }
    fn stream_index(&self, id: ClientId, stream: u32) -> Option<usize> {
        self.streams
            .iter()
            .position(|s| s.a == (id, stream) || s.b == (id, stream))
    }
    fn count_streams(&self, id: ClientId) -> usize {
        self.streams
            .iter()
            .filter(|s| s.a.0 == id || s.b.0 == id)
            .count()
    }
    pub fn handle(&mut self, id: ClientId, frame: Frame) -> Vec<Outgoing> {
        let Some(client) = self.clients.get(&id) else {
            return vec![];
        };
        let ip = client.ip;
        match frame {
            Frame::Hello { version, .. } => Self::error(
                id,
                if version != VERSION {
                    format!("protocol version mismatch: client {version}, server {VERSION}")
                } else {
                    "Hello already received".into()
                },
            ),
            Frame::Ping => vec![Outgoing {
                to: id,
                frame: Frame::Pong,
            }],
            Frame::Pong => vec![],
            Frame::Register {
                id: service_id,
                mut service,
            } => {
                if service.kind != "_CODWAW_IPHONE._tcp."
                    || service.port == 0
                    || service.txt.len() > 4096
                {
                    return Self::error(id, "invalid service type, port, or TXT data");
                }
                if client.services.len() >= MAX_SERVICES_PER_CLIENT
                    && !client.services.contains_key(&service_id)
                {
                    return Self::error(id, "service limit reached");
                }
                service.name = sanitise_name(&service.name);
                self.clients
                    .get_mut(&id)
                    .unwrap()
                    .services
                    .insert(service_id, service.clone());
                self.broadcast(
                    id,
                    Frame::Listed {
                        owner: ip,
                        id: service_id,
                        service,
                    },
                )
            }
            Frame::Unregister { id: service_id } => {
                if self
                    .clients
                    .get_mut(&id)
                    .unwrap()
                    .services
                    .remove(&service_id)
                    .is_some()
                {
                    self.broadcast(
                        id,
                        Frame::Unlisted {
                            owner: ip,
                            id: service_id,
                        },
                    )
                } else {
                    vec![]
                }
            }
            Frame::Datagram {
                mut src,
                dst,
                payload,
            } => {
                src.ip = ip;
                if dst.ip == [255; 4] || dst.ip == [10, 77, 0, 255] {
                    self.clients
                        .iter()
                        .filter(|(&to, _)| to != id)
                        .map(|(&to, client)| Outgoing {
                            to,
                            frame: Frame::Datagram {
                                src,
                                dst: VAddr {
                                    ip: client.ip,
                                    port: dst.port,
                                },
                                payload: payload.clone(),
                            },
                        })
                        .collect()
                } else if let Some(to) = self.peer(dst) {
                    vec![Outgoing {
                        to,
                        frame: Frame::Datagram { src, dst, payload },
                    }]
                } else {
                    vec![]
                }
            }
            Frame::StreamOpen {
                stream,
                mut src,
                dst,
            } => {
                if self.stream_index(id, stream).is_some() {
                    return Self::error(id, "stream identifier already in use");
                }
                let Some(to) = self.peer(dst) else {
                    return Self::close(id, stream);
                };
                if self.count_streams(id) >= MAX_STREAMS_PER_CLIENT
                    || self.count_streams(to) >= MAX_STREAMS_PER_CLIENT
                {
                    return Self::close(id, stream);
                }
                let remote = (1..=u32::MAX)
                    .rev()
                    .find(|&n| self.stream_index(to, n).is_none())
                    .unwrap();
                self.streams.push(Stream {
                    a: (id, stream),
                    b: (to, remote),
                    accepted: false,
                });
                src.ip = ip;
                vec![Outgoing {
                    to,
                    frame: Frame::StreamOpen {
                        stream: remote,
                        src,
                        dst,
                    },
                }]
            }
            Frame::StreamAccept { stream } => {
                let Some(index) = self.stream_index(id, stream) else {
                    return Self::close(id, stream);
                };
                let s = &mut self.streams[index];
                if s.b != (id, stream) || s.accepted {
                    return Self::error(id, "unexpected stream accept");
                }
                s.accepted = true;
                vec![Outgoing {
                    to: s.a.0,
                    frame: Frame::StreamAccept { stream: s.a.1 },
                }]
            }
            Frame::StreamData { stream, payload } => {
                let Some(index) = self.stream_index(id, stream) else {
                    return Self::close(id, stream);
                };
                let s = &self.streams[index];
                if !s.accepted {
                    return Self::error(id, "stream is not accepted");
                }
                let (to, remote) = if s.a == (id, stream) { s.b } else { s.a };
                vec![Outgoing {
                    to,
                    frame: Frame::StreamData {
                        stream: remote,
                        payload,
                    },
                }]
            }
            Frame::StreamClose { stream } => {
                let Some(index) = self.stream_index(id, stream) else {
                    return vec![];
                };
                let s = self.streams.swap_remove(index);
                let (to, remote) = if s.a == (id, stream) { s.b } else { s.a };
                Self::close(to, remote)
            }
            _ => Self::error(id, "client sent a server-only frame"),
        }
    }
    fn peer(&self, addr: VAddr) -> Option<ClientId> {
        if addr.ip[..3] != [10, 77, 0] {
            return None;
        }
        self.clients.get(&addr.ip[3]).map(|_| addr.ip[3])
    }
}
