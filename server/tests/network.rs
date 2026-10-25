use codwawzpc_coop_proto::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
struct Server {
    addr: std::net::SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = shutdown.clone();
        let handle = thread::spawn(move || codwawzpc_server::serve(listener, flag));
        Self {
            addr,
            shutdown,
            handle: Some(handle),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.handle.take().unwrap().join().unwrap();
    }
}
fn send(s: &mut TcpStream, f: Frame) {
    let mut bytes = vec![];
    encode(&f, &mut bytes);
    s.write_all(&bytes).unwrap();
}
fn recv(s: &mut TcpStream) -> Frame {
    loop {
        let mut len = [0; 4];
        s.read_exact(&mut len).unwrap();
        let n = u32::from_be_bytes(len) as usize;
        assert!(n <= MAX_FRAME_LEN);
        let mut bytes = len.to_vec();
        bytes.resize(4 + n, 0);
        s.read_exact(&mut bytes[4..]).unwrap();
        let (frame, _) = decode(&bytes).unwrap().unwrap();
        if frame == Frame::Ping {
            send(s, Frame::Pong);
        } else {
            return frame;
        }
    }
}
fn connect(server: &Server, name: &str) -> TcpStream {
    let mut s = TcpStream::connect(server.addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    send(
        &mut s,
        Frame::Hello {
            version: VERSION,
            name: name.into(),
        },
    );
    assert!(matches!(recv(&mut s), Frame::Welcome { .. }));
    s
}
#[test]
fn two_real_clients_list_and_relay() {
    let server = Server::start();
    let mut a = connect(&server, "Alice");
    let mut b = connect(&server, "Bob");
    let service = Service {
        name: "Game".into(),
        kind: "_CODWAW_IPHONE._tcp.".into(),
        port: 1234,
        txt: vec![0, 255],
    };
    send(
        &mut a,
        Frame::Register {
            id: 7,
            service: service.clone(),
        },
    );
    assert_eq!(
        recv(&mut b),
        Frame::Listed {
            owner: [10, 77, 0, 1],
            id: 7,
            service
        }
    );
    let src = VAddr {
        ip: [10, 77, 0, 1],
        port: 4321,
    };
    let dst = VAddr {
        ip: [10, 77, 0, 2],
        port: 1234,
    };
    send(
        &mut a,
        Frame::Datagram {
            src,
            dst,
            payload: vec![0, 5, 255],
        },
    );
    assert_eq!(
        recv(&mut b),
        Frame::Datagram {
            src,
            dst,
            payload: vec![0, 5, 255]
        }
    );
    send(
        &mut a,
        Frame::StreamOpen {
            stream: 42,
            src,
            dst,
        },
    );
    let remote = match recv(&mut b) {
        Frame::StreamOpen { stream, .. } => stream,
        _ => panic!(),
    };
    send(&mut b, Frame::StreamAccept { stream: remote });
    assert_eq!(recv(&mut a), Frame::StreamAccept { stream: 42 });
    send(
        &mut a,
        Frame::StreamData {
            stream: 42,
            payload: vec![9],
        },
    );
    assert_eq!(
        recv(&mut b),
        Frame::StreamData {
            stream: remote,
            payload: vec![9]
        }
    );
    drop(a);
    let one = recv(&mut b);
    let two = recv(&mut b);
    assert!([one.clone(), two.clone()].contains(&Frame::Unlisted {
        owner: src.ip,
        id: 7
    }));
    assert!([one, two].contains(&Frame::StreamClose { stream: remote }));
}
#[test]
fn wrong_version_and_non_hello_are_rejected() {
    let server = Server::start();
    for frame in [
        Frame::Hello {
            version: VERSION + 1,
            name: "Wrong".into(),
        },
        Frame::Ping,
    ] {
        let mut s = TcpStream::connect(server.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        send(&mut s, frame);
        assert!(matches!(recv(&mut s), Frame::Error { .. }));
        let mut b = [0];
        assert_eq!(s.read(&mut b).unwrap(), 0);
    }
}
#[test]
fn malformed_peer_does_not_stop_other_clients() {
    let server = Server::start();
    let mut bad = TcpStream::connect(server.addr).unwrap();
    bad.write_all(&65537u32.to_be_bytes()).unwrap();
    let mut good = connect(&server, "Good");
    send(&mut good, Frame::Ping);
    assert_eq!(recv(&mut good), Frame::Pong);
}
#[test]
fn relayed_frames_are_forwarded_without_polling_delay() {
    let server = Server::start();
    let mut a = connect(&server, "Alice");
    let mut b = connect(&server, "Bob");
    a.set_nodelay(true).unwrap();
    let src = VAddr {
        ip: [10, 77, 0, 1],
        port: 4321,
    };
    let dst = VAddr {
        ip: [10, 77, 0, 2],
        port: 1234,
    };
    let mut delays = vec![];
    for i in 0..30u8 {
        // Vary the phase against any fixed polling interval in the server.
        thread::sleep(Duration::from_millis(3 + u64::from(i % 7)));
        let sent = std::time::Instant::now();
        send(
            &mut a,
            Frame::Datagram {
                src,
                dst,
                payload: vec![i],
            },
        );
        assert!(matches!(recv(&mut b), Frame::Datagram { .. }));
        delays.push(sent.elapsed());
    }
    delays.sort();
    let median = delays[delays.len() / 2];
    assert!(
        median < Duration::from_millis(2),
        "median relay delay {median:?} on loopback"
    );
}
