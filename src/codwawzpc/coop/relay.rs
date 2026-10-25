/* This Source Code Form is subject to the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! One bounded relay connection; only worker threads touch its blocking I/O.
use super::addr_map::AddressMap;
use super::traffic::{control_line, frame_line, Dir, Kind, Meter};
use codwawzpc_coop_proto::{
    decode, try_encode, Frame, Proto, Service, VAddr, MAX_FRAME_LEN, VERSION,
};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        mpsc::{self, Sender, SyncSender},
        Arc, Mutex, Weak,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub owner: [u8; 4],
    pub id: u32,
    pub service: Service,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListingEvent {
    Added(Listing),
    Removed { owner: [u8; 4], id: u32 },
}

/// Most bytes a stream may hold for a game that is not reading. The game
/// sends bursts of tiny frames, so the bound counts bytes, not frames.
const MAX_QUEUED_BYTES: usize = 256 * 1024;

struct Tunnel {
    writer: TcpStream,
    incoming: Sender<Vec<u8>>,
    /// Bytes in `incoming` not yet taken by the writer thread.
    queued: Arc<AtomicUsize>,
    accepted: Option<SyncSender<bool>>,
    proxy: Option<SocketAddr>,
}
impl Tunnel {
    /// Queues bytes for the game, or returns false when the game has fallen
    /// [MAX_QUEUED_BYTES] behind.
    fn deliver(&self, payload: Vec<u8>) -> bool {
        let len = payload.len();
        if self.queued.fetch_add(len, Ordering::AcqRel) + len > MAX_QUEUED_BYTES {
            self.queued.fetch_sub(len, Ordering::AcqRel);
            return false;
        }
        self.incoming.send(payload).is_ok()
    }
}
struct Shared {
    connected: AtomicBool,
    outgoing: SyncSender<Frame>,
    socket: TcpStream,
    map: Mutex<AddressMap>,
    listings: Mutex<HashMap<([u8; 4], u32), Listing>>,
    events: Mutex<Vec<ListingEvent>>,
    streams: Mutex<HashMap<u32, Tunnel>>,
    udp: Mutex<HashMap<VAddr, UdpSocket>>,
    next: AtomicU32,
    meter: Mutex<Meter>,
}
pub struct Session {
    pub ip: [u8; 4],
    pub name: String,
    shared: Arc<Shared>,
}

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn write_frame(stream: &mut TcpStream, frame: &Frame) -> io::Result<()> {
    let mut bytes = Vec::new();
    try_encode(frame, &mut bytes).map_err(|e| io_error(e.to_string()))?;
    stream.write_all(&bytes)
}
fn read_deadline(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(bytes) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                bytes = &mut bytes[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn read_frame(stream: &mut TcpStream) -> io::Result<Frame> {
    read_frame_until(stream, Instant::now() + Duration::from_secs(10))
}
fn read_frame_until(stream: &mut TcpStream, deadline: Instant) -> io::Result<Frame> {
    let mut prefix = [0; 4];
    read_deadline(stream, &mut prefix, deadline)?;
    let len = u32::from_be_bytes(prefix) as usize;
    if len == 0 || len > MAX_FRAME_LEN {
        return Err(io_error("invalid relay frame length"));
    }
    let mut buf = Vec::with_capacity(len + 4);
    buf.extend(prefix);
    buf.resize(len + 4, 0);
    read_deadline(stream, &mut buf[4..], deadline)?;
    decode(&buf)
        .map_err(|e| io_error(e.to_string()))?
        .map(|(f, _)| f)
        .ok_or_else(|| io_error("incomplete relay frame"))
}
impl Session {
    pub fn connect(server: &str, name: &str) -> io::Result<Self> {
        let host = server.rsplit_once(':').map_or(server, |(host, _)| host);
        if super::forbidden_host(host) {
            return Err(io_error("legacy DemonWare servers are disabled"));
        }
        // DNS runs on a worker so an unavailable resolver cannot stall startup.
        let (tx, rx) = mpsc::sync_channel(1);
        let server = server.to_string();
        thread::spawn(move || {
            let _ = tx.send(server.to_socket_addrs().map(|i| i.collect::<Vec<_>>()));
        });
        let addresses = rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| io_error("relay DNS timed out"))??;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut connection = None;
        for addr in addresses {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            if let Ok(s) = TcpStream::connect_timeout(&addr, left) {
                connection = Some(s);
                break;
            }
        }
        let mut socket = connection.ok_or_else(|| {
            io::Error::new(io::ErrorKind::ConnectionRefused, "co-op relay unavailable")
        })?;
        socket.set_nodelay(true)?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        write_frame(
            &mut socket,
            &Frame::Hello {
                version: VERSION,
                name: codwawzpc_coop_proto::sanitise_name(name),
            },
        )?;
        let (ip, name) =
            match read_frame_until(&mut socket, Instant::now() + Duration::from_secs(3))? {
                Frame::Welcome { ip, name }
                    if ip[..3] == [10, 77, 0] && ip[3] > 0 && ip[3] < 255 =>
                {
                    (ip, name)
                }
                Frame::Error { message } => return Err(io_error(message)),
                _ => return Err(io_error("relay did not send a valid Welcome")),
            };
        socket.set_read_timeout(Some(Duration::from_secs(10)))?;
        let (tx, rx) = mpsc::sync_channel(256);
        let shared = Arc::new(Shared {
            connected: AtomicBool::new(true),
            outgoing: tx,
            socket: socket.try_clone()?,
            map: Mutex::new(AddressMap::default()),
            listings: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            streams: Mutex::new(HashMap::new()),
            udp: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
            meter: Mutex::new(Meter::new(Instant::now())),
        });
        let mut writer = socket.try_clone()?;
        let weak = Arc::downgrade(&shared);
        thread::spawn(move || {
            loop {
                let frame = match rx.recv_timeout(Duration::from_secs(3)) {
                    Ok(frame) => frame,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if !weak
                            .upgrade()
                            .is_some_and(|s| s.connected.load(Ordering::Acquire))
                        {
                            break;
                        }
                        Frame::Ping
                    }
                    Err(_) => break,
                };
                if write_frame(&mut writer, &frame).is_err() {
                    break;
                }
            }
            if let Some(s) = weak.upgrade() {
                disconnect(&s);
            }
        });
        let weak = Arc::downgrade(&shared);
        thread::spawn(move || {
            loop {
                let frame = match read_frame(&mut socket) {
                    Ok(f) => f,
                    Err(e) => {
                        log!("Co-op relay read failed: {e}");
                        break;
                    }
                };
                let Some(s) = weak.upgrade() else {
                    break;
                };
                if let Err(e) = handle_frame(&s, ip, frame) {
                    log!("Co-op relay frame failed: {e}");
                    break;
                }
            }
            if let Some(s) = weak.upgrade() {
                disconnect(&s);
            }
        });
        Ok(Self { ip, name, shared })
    }
    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Acquire)
    }
    pub fn listings(&self) -> Vec<Listing> {
        self.shared
            .listings
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }
    pub fn take_listing_events(&self) -> Vec<ListingEvent> {
        std::mem::take(&mut *self.shared.events.lock().unwrap())
    }
    pub fn register(&self, service: Service) -> u32 {
        let id = self.next_id();
        let _ = enqueue(&self.shared, Frame::Register { id, service });
        id
    }
    pub fn unregister(&self, id: u32) {
        let _ = enqueue(&self.shared, Frame::Unregister { id });
    }
    fn next_id(&self) -> u32 {
        self.shared.next.fetch_add(1, Ordering::Relaxed) & 0x7fffffff
    }
    pub fn bind(&self, proto: Proto, port: u16, real: SocketAddr) -> io::Result<u16> {
        let port = self.shared.map.lock().unwrap().bind(proto, port, real)?;
        debug_assert_eq!(self.local_vport(proto, real), Some(port));
        Ok(port)
    }
    pub fn unbind(&self, proto: Proto, port: u16) {
        self.shared.map.lock().unwrap().unbind(proto, port);
    }
    pub fn local_vport(&self, proto: Proto, real: SocketAddr) -> Option<u16> {
        self.shared.map.lock().unwrap().local_vport(proto, real)
    }
    pub fn inbound(&self, proto: Proto, real: SocketAddr) -> Option<VAddr> {
        self.shared.map.lock().unwrap().inbound(proto, real)
    }
    pub fn send_datagram(&self, port: u16, dst: VAddr, payload: &[u8]) -> io::Result<usize> {
        if payload.len() > codwawzpc_coop_proto::MAX_DATAGRAM_PAYLOAD {
            return Err(io_error("datagram too large"));
        }
        enqueue(
            &self.shared,
            Frame::Datagram {
                src: VAddr { ip: self.ip, port },
                dst,
                payload: payload.to_vec(),
            },
        )?;
        note(&self.shared, Dir::Out, None, payload);
        Ok(payload.len())
    }
    pub fn connect_stream(&self, port: u16, dst: VAddr) -> io::Result<(TcpStream, u16)> {
        if !self.is_connected() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let guest = TcpStream::connect(listener.local_addr()?)?;
        let (proxy, _) = listener.accept()?;
        proxy.set_nodelay(true)?;
        proxy.set_write_timeout(Some(Duration::from_secs(2)))?;
        let local = guest.local_addr()?;
        let port = self.bind(Proto::Tcp, port, local)?;
        let id = self.next_id();
        let (tx, rx) = mpsc::sync_channel(1);
        install_tunnel(&self.shared, id, &proxy, Some(tx), None)?;
        let result = enqueue(
            &self.shared,
            Frame::StreamOpen {
                stream: id,
                src: VAddr { ip: self.ip, port },
                dst,
            },
        )
        .and_then(|_| {
            if rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false) {
                Ok(())
            } else {
                Err(io::ErrorKind::ConnectionRefused.into())
            }
        });
        if let Err(e) = result {
            send_close(&self.shared, id);
            close_tunnel(&self.shared, id);
            self.unbind(Proto::Tcp, port);
            return Err(e);
        }
        pump(Arc::downgrade(&self.shared), id, proxy);
        Ok((guest, port))
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        disconnect(&self.shared);
    }
}
fn enqueue(s: &Shared, frame: Frame) -> io::Result<()> {
    if !s.connected.load(Ordering::Acquire) {
        return Err(io::ErrorKind::NotConnected.into());
    }
    if let Some(line) = control_line(Dir::Out, &frame, now_micros()) {
        log_dbg!("{line}");
    }
    s.outgoing.try_send(frame).map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "relay queue full or disconnected",
        )
    })
}
fn disconnect(s: &Shared) {
    if !s.connected.swap(false, Ordering::AcqRel) {
        return;
    }
    log!("Co-op relay disconnected");
    let _ = s.socket.shutdown(Shutdown::Both);
    for (_, t) in s.streams.lock().unwrap().drain() {
        let _ = t.writer.shutdown(Shutdown::Both);
        if let Some(tx) = t.accepted {
            let _ = tx.try_send(false);
        }
    }
    s.udp.lock().unwrap().clear();
    *s.map.lock().unwrap() = AddressMap::default();
    let old = std::mem::take(&mut *s.listings.lock().unwrap());
    let mut events = s.events.lock().unwrap();
    events.clear();
    for ((owner, id), _) in old {
        events.push(ListingEvent::Removed { owner, id });
    }
}
fn close_tunnel(s: &Shared, id: u32) {
    if let Some(t) = s.streams.lock().unwrap().remove(&id) {
        let _ = t.writer.shutdown(Shutdown::Both);
        if let Some(tx) = t.accepted {
            let _ = tx.try_send(false);
        }
        if let Some(real) = t.proxy {
            s.map.lock().unwrap().forget_peer(Proto::Tcp, real);
        }
    }
}
// A close must be delivered or the connection must close, so peers never
// retain an orphan stream when a bounded queue fills.
fn send_close(s: &Shared, id: u32) {
    if enqueue(s, Frame::StreamClose { stream: id }).is_err() {
        disconnect(s);
    }
}
fn install_tunnel(
    s: &Arc<Shared>,
    id: u32,
    socket: &TcpStream,
    accepted: Option<SyncSender<bool>>,
    proxy: Option<SocketAddr>,
) -> io::Result<()> {
    if s.streams.lock().unwrap().len() >= 1024 {
        return Err(io_error("stream limit reached"));
    }
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let queued = Arc::new(AtomicUsize::new(0));
    let mut writer = socket.try_clone()?;
    writer.set_write_timeout(Some(Duration::from_secs(2)))?;
    s.streams.lock().unwrap().insert(
        id,
        Tunnel {
            writer: socket.try_clone()?,
            incoming: tx,
            queued: queued.clone(),
            accepted,
            proxy,
        },
    );
    let weak = Arc::downgrade(s);
    thread::spawn(move || {
        while let Ok(payload) = rx.recv() {
            queued.fetch_sub(payload.len(), Ordering::AcqRel);
            if let Err(e) = writer.write_all(&payload) {
                log_dbg!("Stream {id}: write to the game failed ({e}), closing");
                if let Some(s) = weak.upgrade() {
                    send_close(&s, id);
                    close_tunnel(&s, id);
                }
                break;
            }
        }
        // The sender is removed on a remote close; drain accepted bytes before
        // delivering EOF to the guest.
        let _ = writer.shutdown(Shutdown::Write);
    });
    Ok(())
}
fn pump(weak: Weak<Shared>, id: u32, mut reader: TcpStream) {
    thread::spawn(move || {
        let mut buf = [0; 16384];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => {
                    log_dbg!("Stream {id}: the game closed it");
                    break;
                }
                Err(e) => {
                    log_dbg!("Stream {id}: read from the game failed ({e})");
                    break;
                }
                Ok(n) => n,
            };
            let Some(s) = weak.upgrade() else {
                break;
            };
            if !s.streams.lock().unwrap().contains_key(&id) {
                continue;
            }
            // Backpressure is bounded: terminate this stream if relay falls behind.
            if enqueue(
                &s,
                Frame::StreamData {
                    stream: id,
                    payload: buf[..n].to_vec(),
                },
            )
            .is_err()
            {
                log_dbg!("Stream {id}: relay queue full, closing");
                break;
            }
            note(&s, Dir::Out, Some(id), &buf[..n]);
        }
        if let Some(s) = weak.upgrade() {
            if s.streams.lock().unwrap().contains_key(&id) {
                send_close(&s, id);
                close_tunnel(&s, id);
            }
        }
    });
}
/// Logs one relayed game frame with a wall-clock time, a fingerprint and its
/// stream, so two windows on one PC can be matched to measure relay latency.
fn note(s: &Shared, dir: Dir, stream: Option<u32>, payload: &[u8]) {
    log_dbg!("{}", frame_line(dir, stream, payload, now_micros()));
    let kind = if stream.is_some() {
        Kind::Stream
    } else {
        Kind::Datagram
    };
    let report = s
        .meter
        .lock()
        .unwrap()
        .record(Instant::now(), dir, kind, payload.len());
    if let Some(report) = report {
        log_dbg!("{report}");
    }
}
fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_micros())
}
fn handle_frame(s: &Arc<Shared>, ip: [u8; 4], frame: Frame) -> io::Result<()> {
    if let Some(line) = control_line(Dir::In, &frame, now_micros()) {
        log_dbg!("{line}");
    }
    match frame {
        Frame::Ping => enqueue(s, Frame::Pong)?,
        Frame::Pong => {}
        Frame::Listed { owner, id, service } => {
            let listing = Listing { owner, id, service };
            s.listings
                .lock()
                .unwrap()
                .insert((owner, id), listing.clone());
            let mut events = s.events.lock().unwrap();
            if events.len() < 16384 {
                events.push(ListingEvent::Added(listing));
            } else {
                return Err(io_error("listing event overflow"));
            }
        }
        Frame::Unlisted { owner, id } => {
            if s.listings.lock().unwrap().remove(&(owner, id)).is_some() {
                let mut events = s.events.lock().unwrap();
                if events.len() >= 16384 {
                    return Err(io_error("listing event overflow"));
                }
                events.push(ListingEvent::Removed { owner, id });
            }
        }
        Frame::Datagram { src, dst, payload } if dst.ip == ip => {
            note(s, Dir::In, None, &payload);
            let target = s.map.lock().unwrap().bound(Proto::Udp, dst.port);
            if let Some(target) = target {
                let mut proxies = s.udp.lock().unwrap();
                if !proxies.contains_key(&src) {
                    if proxies.len() >= 4096 {
                        return Ok(());
                    }
                    let sock = UdpSocket::bind("127.0.0.1:0")?;
                    s.map
                        .lock()
                        .unwrap()
                        .peer(Proto::Udp, sock.local_addr()?, src);
                    proxies.insert(src, sock);
                }
                proxies[&src].send_to(&payload, target)?;
            }
        }
        Frame::StreamOpen { stream, src, dst } if dst.ip == ip => {
            let target = s.map.lock().unwrap().bound(Proto::Tcp, dst.port);
            let socket =
                target.and_then(|a| TcpStream::connect_timeout(&a, Duration::from_secs(1)).ok());
            if let Some(socket) = socket {
                socket.set_nodelay(true)?;
                socket.set_write_timeout(Some(Duration::from_secs(2)))?;
                let real = socket.local_addr()?;
                s.map.lock().unwrap().peer(Proto::Tcp, real, src);
                install_tunnel(s, stream, &socket, None, Some(real))?;
                enqueue(s, Frame::StreamAccept { stream })?;
                pump(Arc::downgrade(s), stream, socket);
            } else {
                enqueue(s, Frame::StreamClose { stream })?;
            }
        }
        Frame::StreamAccept { stream } => {
            if let Some(t) = s.streams.lock().unwrap().get_mut(&stream) {
                if let Some(tx) = t.accepted.take() {
                    let _ = tx.try_send(true);
                }
            }
        }
        Frame::StreamData { stream, payload } => {
            note(s, Dir::In, Some(stream), &payload);
            let failure = if let Some(t) = s.streams.lock().unwrap().get(&stream) {
                !t.deliver(payload)
            } else {
                false
            };
            if failure {
                log_dbg!("Stream {stream}: incoming queue full, closing");
                send_close(s, stream);
                close_tunnel(s, stream);
            }
        }
        Frame::StreamClose { stream } => {
            if let Some(t) = s.streams.lock().unwrap().remove(&stream) {
                // A remote close is a half-close, as over real TCP: the guest
                // reads EOF after the queued bytes, and its later writes are
                // discarded by the pump instead of failing with EPIPE.
                if let Some(tx) = t.accepted {
                    let _ = tx.try_send(false);
                    let _ = t.writer.shutdown(Shutdown::Both);
                }
                if let Some(real) = t.proxy {
                    s.map.lock().unwrap().forget_peer(Proto::Tcp, real);
                }
                // Dropping the incoming sender lets its writer drain in order.
            }
        }
        Frame::Error { message } => return Err(io_error(message)),
        _ => return Err(io_error("unexpected relay frame")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    fn test_shared() -> (Arc<Shared>, Receiver<Frame>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let (tx, rx) = mpsc::sync_channel(16);
        let shared = Shared {
            connected: AtomicBool::new(true),
            outgoing: tx,
            socket,
            map: Mutex::new(AddressMap::default()),
            listings: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            streams: Mutex::new(HashMap::new()),
            udp: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
            meter: Mutex::new(Meter::new(Instant::now())),
        };
        (Arc::new(shared), rx)
    }
    /// The game's end of a tunnel and the relay's end.
    fn game_and_proxy() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let game = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (proxy, _) = listener.accept().unwrap();
        (game, proxy)
    }
    #[test]
    fn burst_of_small_stream_frames_is_delivered_without_closing() {
        // Four-player sessions send bursts of 4- and 11-byte frames; the
        // relay used to close the stream when more than 8 were queued.
        let (shared, outgoing) = test_shared();
        let (mut game, proxy) = game_and_proxy();
        install_tunnel(&shared, 7, &proxy, None, None).unwrap();
        let mut sent = Vec::new();
        for i in 0..1000u32 {
            let payload = i.to_le_bytes().repeat(3);
            sent.extend_from_slice(&payload);
            handle_frame(
                &shared,
                [10, 77, 0, 2],
                Frame::StreamData { stream: 7, payload },
            )
            .unwrap();
        }
        game.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut received = vec![0; sent.len()];
        game.read_exact(&mut received).unwrap();
        assert_eq!(received, sent);
        assert!(outgoing.try_recv().is_err(), "the relay closed the stream");
    }
    #[test]
    fn stream_is_closed_when_the_game_stops_reading() {
        let (shared, outgoing) = test_shared();
        let (_game, proxy) = game_and_proxy();
        install_tunnel(&shared, 7, &proxy, None, None).unwrap();
        // Far more than the kernel's socket buffers and the relay's queue.
        for _ in 0..512 {
            let payload = vec![0; codwawzpc_coop_proto::MAX_STREAM_PAYLOAD];
            handle_frame(
                &shared,
                [10, 77, 0, 2],
                Frame::StreamData { stream: 7, payload },
            )
            .unwrap();
        }
        assert!(matches!(
            outgoing.recv_timeout(Duration::from_secs(5)),
            Ok(Frame::StreamClose { stream: 7 })
        ));
    }
    #[test]
    fn legacy_server_is_refused_before_dns() {
        let error = Session::connect("codzombies-iphone-live.auth.mmp3.DEMONWARE.NET.:27960", "A")
            .err()
            .unwrap();
        assert!(error.to_string().contains("disabled"));
    }
    #[test]
    fn full_queue_disconnects_instead_of_losing_stream_close() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let (tx, _rx) = mpsc::sync_channel(1);
        tx.try_send(Frame::Ping).unwrap();
        let shared = Shared {
            connected: AtomicBool::new(true),
            outgoing: tx,
            socket,
            map: Mutex::new(AddressMap::default()),
            listings: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            streams: Mutex::new(HashMap::new()),
            udp: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
            meter: Mutex::new(Meter::new(Instant::now())),
        };
        send_close(&shared, 1);
        assert!(!shared.connected.load(Ordering::Acquire));
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    }
    #[test]
    fn partial_frame_has_an_absolute_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut reader = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut writer, _) = listener.accept().unwrap();
        let sender = thread::spawn(move || {
            for _ in 0..20 {
                if writer.write_all(&[0]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let start = Instant::now();
        assert!(read_frame_until(&mut reader, start + Duration::from_millis(25)).is_err());
        assert!(start.elapsed() < Duration::from_millis(150));
        drop(reader);
        sender.join().unwrap();
    }
    #[test]
    fn relay_connect_failure_is_reported_not_fatal() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(Session::connect(&addr.to_string(), "Tester").is_err());
    }
    #[test]
    fn relay_udp_and_tcp_use_virtual_addresses() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let shutdown = stop.clone();
        let server = thread::spawn(move || codwawzpc_server::serve(listener, shutdown));
        {
            let a = Session::connect(&address.to_string(), "A").unwrap();
            let b = Session::connect(&address.to_string(), "B").unwrap();
            let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
            udp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            b.bind(Proto::Udp, 1234, udp.local_addr().unwrap()).unwrap();
            a.send_datagram(
                2345,
                VAddr {
                    ip: b.ip,
                    port: 1234,
                },
                b"hello",
            )
            .unwrap();
            let mut buf = [0; 32];
            let (n, src) = udp.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..n], b"hello");
            assert_eq!(
                b.inbound(Proto::Udp, src),
                Some(VAddr {
                    ip: a.ip,
                    port: 2345
                })
            );
            let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
            b.bind(Proto::Tcp, 3456, tcp.local_addr().unwrap()).unwrap();
            let (mut outgoing, _) = a
                .connect_stream(
                    0,
                    VAddr {
                        ip: b.ip,
                        port: 3456,
                    },
                )
                .unwrap();
            outgoing
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let (mut incoming, src) = tcp.accept().unwrap();
            incoming
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            assert_eq!(b.inbound(Proto::Tcp, src).unwrap().ip, a.ip);
            outgoing.write_all(b"ping").unwrap();
            incoming.read_exact(&mut buf[..4]).unwrap();
            assert_eq!(&buf[..4], b"ping");
            incoming.write_all(b"pong").unwrap();
            outgoing.read_exact(&mut buf[..4]).unwrap();
            assert_eq!(&buf[..4], b"pong");
            assert!(a
                .connect_stream(
                    0,
                    VAddr {
                        ip: [10, 77, 0, 254],
                        port: 1
                    }
                )
                .is_err());
        }
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }
    #[test]
    fn peer_close_is_a_half_close() {
        // The game's join request connects, sends and closes at once; the
        // host still replies on that socket, as it would over real TCP.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let shutdown = stop.clone();
        let server = thread::spawn(move || codwawzpc_server::serve(listener, shutdown));
        {
            let a = Session::connect(&address.to_string(), "A").unwrap();
            let b = Session::connect(&address.to_string(), "B").unwrap();
            let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
            b.bind(Proto::Tcp, 3456, tcp.local_addr().unwrap()).unwrap();
            let (mut outgoing, _) = a
                .connect_stream(
                    0,
                    VAddr {
                        ip: b.ip,
                        port: 3456,
                    },
                )
                .unwrap();
            outgoing.write_all(b"join").unwrap();
            drop(outgoing);
            let (mut incoming, _) = tcp.accept().unwrap();
            incoming
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buf = Vec::new();
            incoming.read_to_end(&mut buf).unwrap();
            assert_eq!(buf, b"join");
            thread::sleep(Duration::from_millis(200));
            incoming.write_all(b"ok").unwrap();
            thread::sleep(Duration::from_millis(200));
            incoming.write_all(b"reply").unwrap();
            assert!(a.is_connected() && b.is_connected());
        }
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }
}
