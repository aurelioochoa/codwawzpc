//! TCP transport with bounded queues, heartbeat expiry, and serialized lobby access.
use crate::lobby::{ClientId, Lobby, Outgoing};
use codwawzpc_coop_proto::{decode, try_encode, Frame, MAX_FRAME_LEN, VERSION};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const HEARTBEAT: Duration = Duration::from_secs(5);
const SILENCE: Duration = Duration::from_secs(15);
/// Longest wait for a frame before accepts and heartbeats are checked.
const POLL: Duration = Duration::from_millis(10);
const QUEUE: usize = 128;
enum Event {
    Frame(u64, Frame),
    Closed(u64),
}
enum WriteMessage {
    Frames(Vec<Frame>),
    Reject(String),
}
struct Peer {
    client: Option<ClientId>,
    socket: TcpStream,
    tx: SyncSender<WriteMessage>,
    seen: Instant,
    ping: Instant,
}
fn start_peer(
    id: u64,
    socket: TcpStream,
    events: SyncSender<Event>,
    workers: &mut Vec<JoinHandle<()>>,
) -> io::Result<Peer> {
    socket.set_nodelay(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = socket.try_clone()?;
    let mut writer = socket.try_clone()?;
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let reader_events = events.clone();
    workers.push(thread::spawn(move || {
        let mut buffer = Vec::with_capacity(8192);
        let mut chunk = [0; 8192];
        'read: loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(_) => break,
            }
            loop {
                match decode(&buffer) {
                    Ok(Some((frame, len))) => {
                        buffer.drain(..len);
                        if reader_events.send(Event::Frame(id, frame)).is_err() {
                            break 'read;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break 'read,
                }
            }
            // One partial frame plus at most one read chunk; advertised lengths are checked by decode.
            if buffer.len() > MAX_FRAME_LEN + 4 {
                break;
            }
        }
        let _ = reader.shutdown(Shutdown::Both);
        let _ = reader_events.send(Event::Closed(id));
    }));
    workers.push(thread::spawn(move || {
        'write: while let Ok(message) = rx.recv() {
            let (frames, close) = match message {
                WriteMessage::Frames(frames) => (frames, false),
                WriteMessage::Reject(message) => (vec![Frame::Error { message }], true),
            };
            for frame in frames {
                let mut bytes = Vec::new();
                if try_encode(&frame, &mut bytes).is_err() || writer.write_all(&bytes).is_err() {
                    break 'write;
                }
            }
            if close {
                break;
            }
        }
        let _ = writer.shutdown(Shutdown::Both);
        let _ = events.send(Event::Closed(id));
    }));
    Ok(Peer {
        client: None,
        socket,
        tx,
        seen: Instant::now(),
        ping: Instant::now(),
    })
}
fn disconnect(id: u64, peers: &mut BTreeMap<u64, Peer>, lobby: &mut Lobby) -> Vec<Outgoing> {
    if let Some(peer) = peers.remove(&id) {
        let _ = peer.socket.shutdown(Shutdown::Both);
        peer.client
            .map_or_else(Vec::new, |client| lobby.leave(client))
    } else {
        vec![]
    }
}
fn dispatch(out: Vec<Outgoing>, peers: &mut BTreeMap<u64, Peer>, lobby: &mut Lobby) {
    // One lobby transaction is one queue item per recipient. In particular, a
    // valid late-join snapshot may exceed QUEUE frames and must remain atomic.
    let mut batches: BTreeMap<ClientId, Vec<Frame>> = BTreeMap::new();
    for out in out {
        batches.entry(out.to).or_default().push(out.frame);
    }
    while let Some((to, frames)) = batches.pop_first() {
        let target = peers
            .iter()
            .find(|(_, peer)| peer.client == Some(to))
            .map(|(&id, _)| id);
        if let Some(id) = target {
            if peers[&id]
                .tx
                .try_send(WriteMessage::Frames(frames))
                .is_err()
            {
                for out in disconnect(id, peers, lobby) {
                    batches.entry(out.to).or_default().push(out.frame);
                }
            }
        }
    }
}
/// Serve until `shutdown` is set. The supplied listener determines the bind address.
/// All reader/writer workers are stopped and joined before return.
pub fn serve(listener: TcpListener, shutdown: Arc<AtomicBool>) {
    serve_with_timing(listener, shutdown, HEARTBEAT, SILENCE);
}
fn serve_with_timing(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    heartbeat: Duration,
    silence: Duration,
) {
    if let Err(error) = listener.set_nonblocking(true) {
        eprintln!("cannot configure relay listener: {error}");
        return;
    }
    let (events, rx) = mpsc::sync_channel(512);
    let mut peers = BTreeMap::new();
    let mut lobby = Lobby::default();
    let mut workers = Vec::new();
    let mut next = 0u64;
    while !shutdown.load(Ordering::Relaxed) {
        // Bound each batch so floods cannot starve heartbeat/shutdown processing.
        for _ in 0..64 {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    if peers.len() >= 254 {
                        let _ = socket.set_write_timeout(Some(Duration::from_millis(250)));
                        let mut bytes = vec![];
                        let _ = try_encode(
                            &Frame::Error {
                                message: "lobby full (254 connections)".into(),
                            },
                            &mut bytes,
                        );
                        let _ = socket.write_all(&bytes);
                        continue;
                    }
                    next = next.wrapping_add(1);
                    match start_peer(next, socket, events.clone(), &mut workers) {
                        Ok(peer) => {
                            peers.insert(next, peer);
                        }
                        Err(error) => eprintln!("cannot initialize relay peer: {error}"),
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    eprintln!("relay accept failed: {error}");
                    break;
                }
            }
        }
        for n in 0..512 {
            // Wait for the first event rather than sleeping, so a frame is
            // relayed as soon as it arrives; the timeout keeps accepts and
            // heartbeats going while the relay is idle.
            let next = if n == 0 {
                rx.recv_timeout(POLL).ok()
            } else {
                rx.try_recv().ok()
            };
            let Some(event) = next else {
                break;
            };
            match event {
                Event::Closed(id) => {
                    let out = disconnect(id, &mut peers, &mut lobby);
                    dispatch(out, &mut peers, &mut lobby);
                }
                Event::Frame(id, frame) => {
                    let Some(peer) = peers.get_mut(&id) else {
                        continue;
                    };
                    peer.seen = Instant::now();
                    if let Some(client) = peer.client {
                        if matches!(frame, Frame::Hello { .. }) {
                            let _ = peer
                                .tx
                                .try_send(WriteMessage::Reject("Hello already received".into()));
                            let client = peer.client.take().unwrap();
                            peers.remove(&id);
                            let out = lobby.leave(client);
                            dispatch(out, &mut peers, &mut lobby);
                            continue;
                        }
                        let out = lobby.handle(client, frame);
                        dispatch(out, &mut peers, &mut lobby);
                    } else {
                        let result = match frame {
                            Frame::Hello { version, name } if version == VERSION => {
                                lobby.join(name)
                            }
                            Frame::Hello { version, .. } => Err(format!(
                                "protocol version mismatch: client {version}, server {VERSION}"
                            )),
                            _ => Err("first frame must be Hello".into()),
                        };
                        match result {
                            Ok(client) => {
                                peer.client = Some(client);
                                let out = lobby.take_outgoing();
                                dispatch(out, &mut peers, &mut lobby);
                            }
                            Err(message) => {
                                let _ = peer.tx.try_send(WriteMessage::Reject(message));
                                peers.remove(&id);
                            }
                        }
                    }
                }
            }
        }
        let now = Instant::now();
        let mut drops = vec![];
        for (&id, peer) in &mut peers {
            if now.duration_since(peer.seen) >= silence {
                drops.push(id);
            } else if now.duration_since(peer.ping) >= heartbeat {
                peer.ping = now;
                if peer
                    .tx
                    .try_send(WriteMessage::Frames(vec![Frame::Ping]))
                    .is_err()
                {
                    drops.push(id);
                }
            }
        }
        for id in drops {
            let out = disconnect(id, &mut peers, &mut lobby);
            dispatch(out, &mut peers, &mut lobby);
        }
        let mut i = 0;
        while i < workers.len() {
            if workers[i].is_finished() {
                let _ = workers.swap_remove(i).join();
            } else {
                i += 1;
            }
        }
    }
    for (_, peer) in peers {
        let _ = peer.socket.shutdown(Shutdown::Both);
    }
    // Wake any event producers blocked on a full queue before joining workers.
    drop(rx);
    drop(events);
    for worker in workers {
        let _ = worker.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_peer_expires_and_releases_its_address() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = shutdown.clone();
        let worker = thread::spawn(move || {
            serve_with_timing(
                listener,
                flag,
                Duration::from_millis(30),
                Duration::from_millis(120),
            )
        });
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut hello = vec![];
        try_encode(
            &Frame::Hello {
                version: VERSION,
                name: "Silent".into(),
            },
            &mut hello,
        )
        .unwrap();
        peer.write_all(&hello).unwrap();
        let mut bytes = vec![];
        peer.read_to_end(&mut bytes).unwrap();
        assert!(matches!(
            decode(&bytes).unwrap().unwrap().0,
            Frame::Welcome {
                ip: [10, 77, 0, 1],
                ..
            }
        ));
        assert!(bytes.windows(5).any(|w| w == [0, 0, 0, 1, 13]));
        let mut replacement = TcpStream::connect(address).unwrap();
        replacement
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        replacement.write_all(&hello).unwrap();
        let mut prefix = [0; 4];
        replacement.read_exact(&mut prefix).unwrap();
        let n = u32::from_be_bytes(prefix) as usize;
        let mut welcome = prefix.to_vec();
        welcome.resize(n + 4, 0);
        replacement.read_exact(&mut welcome[4..]).unwrap();
        assert!(matches!(
            decode(&welcome).unwrap().unwrap().0,
            Frame::Welcome {
                ip: [10, 77, 0, 1],
                ..
            }
        ));
        shutdown.store(true, Ordering::Relaxed);
        worker.join().unwrap();
    }
    #[test]
    fn snapshot_is_batched_and_slow_peer_is_disconnected() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut remote = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        remote
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (socket, _) = listener.accept().unwrap();
        let mut lobby = Lobby::new();
        let client = lobby.join("Slow").unwrap();
        lobby.take_outgoing();
        let (tx, rx) = mpsc::sync_channel(1);
        let mut peers = BTreeMap::from([(
            1,
            Peer {
                client: Some(client),
                socket,
                tx,
                seen: Instant::now(),
                ping: Instant::now(),
            },
        )]);
        // A healthy late join may have more than 128 existing listings. The
        // snapshot occupies one bounded queue slot, preserving every frame.
        dispatch(
            (0..QUEUE + 1)
                .map(|_| Outgoing {
                    to: client,
                    frame: Frame::Ping,
                })
                .collect(),
            &mut peers,
            &mut lobby,
        );
        assert!(peers.contains_key(&1));
        match rx.recv().unwrap() {
            WriteMessage::Frames(frames) => assert_eq!(frames.len(), QUEUE + 1),
            _ => panic!(),
        }
        // Without a writer draining this queue, the next transaction triggers
        // deterministic slow-reader cleanup instead of blocking the server.
        dispatch(
            vec![Outgoing {
                to: client,
                frame: Frame::Ping,
            }],
            &mut peers,
            &mut lobby,
        );
        dispatch(
            vec![Outgoing {
                to: client,
                frame: Frame::Ping,
            }],
            &mut peers,
            &mut lobby,
        );
        assert!(peers.is_empty());
        assert!(lobby.client(client).is_none());
        let mut byte = [0];
        assert_eq!(remote.read(&mut byte).unwrap(), 0);
    }
}
