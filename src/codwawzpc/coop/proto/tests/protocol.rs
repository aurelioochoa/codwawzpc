use codwawzpc_coop_proto::*;
fn frames() -> Vec<Frame> {
    let a = VAddr {
        ip: [10, 77, 0, 1],
        port: 1234,
    };
    let b = VAddr {
        ip: [10, 77, 0, 2],
        port: 4321,
    };
    let service = Service {
        name: "game 🧟".into(),
        kind: "_CODWAW_IPHONE._tcp.".into(),
        port: 1234,
        txt: vec![0, 1, 255],
    };
    vec![
        Frame::Hello {
            version: VERSION,
            name: "Player".into(),
        },
        Frame::Welcome {
            ip: a.ip,
            name: "Player".into(),
        },
        Frame::Error {
            message: "error".into(),
        },
        Frame::Register {
            id: 1,
            service: service.clone(),
        },
        Frame::Unregister { id: 1 },
        Frame::Listed {
            owner: a.ip,
            id: 1,
            service,
        },
        Frame::Unlisted { owner: a.ip, id: 1 },
        Frame::Datagram {
            src: a,
            dst: b,
            payload: vec![0, 1, 255],
        },
        Frame::StreamOpen {
            stream: 1,
            src: a,
            dst: b,
        },
        Frame::StreamAccept { stream: 1 },
        Frame::StreamData {
            stream: 1,
            payload: vec![255, 0, 3],
        },
        Frame::StreamClose { stream: 1 },
        Frame::Ping,
        Frame::Pong,
    ]
}
#[test]
fn every_frame_round_trips() {
    let mut combined = Vec::new();
    for frame in frames() {
        let mut out = Vec::new();
        encode(&frame, &mut out);
        assert_eq!(decode(&out).unwrap(), Some((frame, out.len())));
        combined.extend(out);
    }
    for frame in frames() {
        let (actual, len) = decode(&combined).unwrap().unwrap();
        assert_eq!(actual, frame);
        combined.drain(..len);
    }
    assert!(combined.is_empty());
}
#[test]
fn partial_input_needs_more() {
    for frame in frames() {
        let mut out = Vec::new();
        encode(&frame, &mut out);
        for n in 0..out.len() {
            assert_eq!(decode(&out[..n]).unwrap(), None);
        }
    }
}
#[test]
fn oversized_frame_is_an_error() {
    assert!(decode(&65537u32.to_be_bytes()).is_err());
}
#[test]
fn unknown_tag_is_an_error() {
    assert!(decode(&[0, 0, 0, 1, 255]).is_err());
}
#[test]
fn malformed_fields_are_errors() {
    for bytes in [
        &[0, 0, 0, 0][..],
        &[0, 0, 0, 2, 13, 0],
        &[0, 0, 0, 2, 1, 0],
        &[0, 0, 0, 6, 1, 0, 1, 0, 1, 255],
    ] {
        assert!(decode(bytes).is_err(), "{bytes:?}");
    }
}
#[test]
fn player_names_are_sanitised() {
    assert_eq!(sanitise_name(""), "Player");
    assert_eq!(sanitise_name("\n\0\t"), "Player");
    assert_eq!(sanitise_name("a\nb\rc\td"), "abcd");
    let name = sanitise_name(&"🧟".repeat(30));
    assert!(name.len() <= 63);
    assert_eq!(name.chars().count(), 15);
}
#[test]
fn largest_payloads_and_rejected_encoding_preserve_bounds() {
    let a = VAddr {
        ip: [10, 77, 0, 1],
        port: 1234,
    };
    for frame in [
        Frame::Datagram {
            src: a,
            dst: a,
            payload: vec![255; MAX_DATAGRAM_PAYLOAD],
        },
        Frame::StreamData {
            stream: 1,
            payload: vec![0; MAX_STREAM_PAYLOAD],
        },
    ] {
        let mut bytes = vec![];
        try_encode(&frame, &mut bytes).unwrap();
        assert_eq!(bytes.len(), MAX_FRAME_LEN + 4);
        assert_eq!(decode(&bytes).unwrap(), Some((frame, bytes.len())));
    }
    let mut bytes = vec![7];
    assert!(try_encode(
        &Frame::StreamData {
            stream: 1,
            payload: vec![0; MAX_STREAM_PAYLOAD + 1]
        },
        &mut bytes
    )
    .is_err());
    assert_eq!(bytes, vec![7]);
    assert!(try_encode(
        &Frame::Error {
            message: "a".repeat(65536)
        },
        &mut bytes
    )
    .is_err());
    assert_eq!(bytes, vec![7]);
}
