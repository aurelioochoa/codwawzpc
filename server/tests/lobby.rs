use codwawzpc_coop_proto::*;
use codwawzpc_server::lobby::*;
fn service() -> Service {
    Service {
        name: "Game".into(),
        kind: "_CODWAW_IPHONE._tcp.".into(),
        port: 123,
        txt: vec![0, 255],
    }
}
fn pair() -> (Lobby, ClientId, ClientId) {
    let mut lobby = Lobby::default();
    let a = lobby.join("Alice").unwrap();
    let b = lobby.join("Bob").unwrap();
    lobby.take_outgoing();
    (lobby, a, b)
}
fn addr(n: u8) -> VAddr {
    VAddr {
        ip: [10, 77, 0, n],
        port: 123,
    }
}
#[test]
fn join_assigns_unique_addresses_and_names() {
    let mut l = Lobby::default();
    let a = l.join("🧟").unwrap();
    let b = l.join("🧟").unwrap();
    assert_ne!(a, b);
    assert_eq!(l.client(a).unwrap().ip, addr(1).ip);
    assert_eq!(l.client(b).unwrap().name, "🧟 (2)");
    let first = "a".repeat(63);
    l.join(&first).unwrap();
    let fourth = l.join(&first).unwrap();
    assert_eq!(l.client(fourth).unwrap().name.len(), 63);
    assert_eq!(l.take_outgoing().len(), 4);
    l.leave(a);
    assert_eq!(l.join("New").unwrap(), a);
}
#[test]
fn register_is_broadcast_to_others_and_sent_to_late_joiners() {
    let (mut l, a, b) = pair();
    let out = l.handle(
        a,
        Frame::Register {
            id: 7,
            service: service(),
        },
    );
    assert_eq!(
        out,
        vec![Outgoing {
            to: b,
            frame: Frame::Listed {
                owner: addr(1).ip,
                id: 7,
                service: service()
            }
        }]
    );
    let c = l.join("Charlie").unwrap();
    let out = l.take_outgoing();
    assert_eq!(out.len(), 2);
    assert!(matches!(&out[0], Outgoing { to, frame: Frame::Welcome { .. } } if *to == c));
    assert_eq!(out[1].to, c);
    assert!(matches!(out[1].frame, Frame::Listed { id: 7, .. }));
}
#[test]
fn unregister_and_leave_unlist() {
    let (mut l, a, b) = pair();
    l.handle(
        a,
        Frame::Register {
            id: 7,
            service: service(),
        },
    );
    let out = l.handle(a, Frame::Unregister { id: 7 });
    assert_eq!(
        out,
        vec![Outgoing {
            to: b,
            frame: Frame::Unlisted {
                owner: addr(1).ip,
                id: 7
            }
        }]
    );
    l.handle(
        a,
        Frame::Register {
            id: 8,
            service: service(),
        },
    );
    assert!(matches!(l.leave(a)[0].frame, Frame::Unlisted { id: 8, .. }));
    assert!(l.leave(a).is_empty());
}
#[test]
fn hello_with_other_version_is_rejected() {
    let (mut l, a, _) = pair();
    let out = l.handle(
        a,
        Frame::Hello {
            version: VERSION + 1,
            name: "Alice".into(),
        },
    );
    assert!(matches!(&out[0].frame,Frame::Error { message } if message.contains("version")));
}
#[test]
fn lobby_full_after_254() {
    let mut l = Lobby::default();
    for _ in 0..254 {
        l.join("Player").unwrap();
    }
    assert!(l.join("overflow").is_err());
}
#[test]
fn datagram_is_routed_by_destination_ip() {
    let (mut l, a, b) = pair();
    let out = l.handle(
        a,
        Frame::Datagram {
            src: addr(99),
            dst: addr(2),
            payload: vec![7],
        },
    );
    assert_eq!(
        out,
        vec![Outgoing {
            to: b,
            frame: Frame::Datagram {
                src: addr(1),
                dst: addr(2),
                payload: vec![7]
            }
        }]
    );
    assert!(l
        .handle(
            a,
            Frame::Datagram {
                src: addr(1),
                dst: addr(99),
                payload: vec![]
            }
        )
        .is_empty());
}
#[test]
fn stream_open_accept_data_close_are_routed() {
    let (mut l, a, b) = pair();
    let out = l.handle(
        a,
        Frame::StreamOpen {
            stream: 42,
            src: addr(99),
            dst: addr(2),
        },
    );
    let remote = match out[0].frame {
        Frame::StreamOpen { stream, src, .. } => {
            assert_eq!(src, addr(1));
            stream
        }
        _ => panic!(),
    };
    assert_eq!(out[0].to, b);
    assert_eq!(
        l.handle(b, Frame::StreamAccept { stream: remote }),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamAccept { stream: 42 }
        }]
    );
    assert_eq!(
        l.handle(
            a,
            Frame::StreamData {
                stream: 42,
                payload: vec![9]
            }
        ),
        vec![Outgoing {
            to: b,
            frame: Frame::StreamData {
                stream: remote,
                payload: vec![9]
            }
        }]
    );
    assert_eq!(
        l.handle(b, Frame::StreamClose { stream: remote }),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamClose { stream: 42 }
        }]
    );
    assert!(matches!(
        l.handle(
            a,
            Frame::StreamData {
                stream: 42,
                payload: vec![]
            }
        )[0]
        .frame,
        Frame::StreamClose { stream: 42 }
    ));
}
#[test]
fn stream_to_unknown_peer_is_refused() {
    let (mut l, a, _) = pair();
    assert_eq!(
        l.handle(
            a,
            Frame::StreamOpen {
                stream: 1,
                src: addr(1),
                dst: addr(99)
            }
        ),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamClose { stream: 1 }
        }]
    );
}
#[test]
fn client_disconnect_removes_listing_and_closes_streams() {
    let (mut l, a, b) = pair();
    l.handle(
        a,
        Frame::Register {
            id: 1,
            service: service(),
        },
    );
    let out = l.handle(
        a,
        Frame::StreamOpen {
            stream: 9,
            src: addr(1),
            dst: addr(2),
        },
    );
    let remote = match out[0].frame {
        Frame::StreamOpen { stream, .. } => stream,
        _ => panic!(),
    };
    let out = l.leave(a);
    assert!(out.contains(&Outgoing {
        to: b,
        frame: Frame::Unlisted {
            owner: addr(1).ip,
            id: 1
        }
    }));
    assert!(out.contains(&Outgoing {
        to: b,
        frame: Frame::StreamClose { stream: remote }
    }));
}
#[test]
fn simultaneous_stream_numbers_do_not_collide() {
    let (mut l, a, b) = pair();
    let ab = l.handle(
        a,
        Frame::StreamOpen {
            stream: 1,
            src: addr(1),
            dst: addr(2),
        },
    );
    let ba = l.handle(
        b,
        Frame::StreamOpen {
            stream: 1,
            src: addr(2),
            dst: addr(1),
        },
    );
    assert!(matches!(
        ab[0].frame,
        Frame::StreamOpen {
            stream: u32::MAX,
            ..
        }
    ));
    assert!(matches!(
        ba[0].frame,
        Frame::StreamOpen {
            stream: u32::MAX,
            ..
        }
    ));
    assert_eq!(
        l.handle(b, Frame::StreamAccept { stream: u32::MAX }),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamAccept { stream: 1 }
        }]
    );
    assert_eq!(
        l.handle(a, Frame::StreamAccept { stream: u32::MAX }),
        vec![Outgoing {
            to: b,
            frame: Frame::StreamAccept { stream: 1 }
        }]
    );
}
#[test]
fn broadcast_routes_to_other_players_with_authenticated_source() {
    let (mut l, a, b) = pair();
    let c = l.join("Charlie").unwrap();
    l.take_outgoing();
    for broadcast in [[255; 4], [10, 77, 0, 255]] {
        let mut dst = addr(2);
        dst.ip = broadcast;
        let out = l.handle(
            a,
            Frame::Datagram {
                src: addr(99),
                dst,
                payload: vec![5],
            },
        );
        assert_eq!(
            out,
            vec![
                Outgoing {
                    to: b,
                    frame: Frame::Datagram {
                        src: addr(1),
                        dst: addr(2),
                        payload: vec![5]
                    }
                },
                Outgoing {
                    to: c,
                    frame: Frame::Datagram {
                        src: addr(1),
                        dst: addr(3),
                        payload: vec![5]
                    }
                }
            ]
        );
    }
}
#[test]
fn service_and_stream_limits_are_enforced_and_freed() {
    let (mut l, a, b) = pair();
    for id in 0..MAX_SERVICES_PER_CLIENT as u32 {
        l.handle(
            a,
            Frame::Register {
                id,
                service: service(),
            },
        );
    }
    assert!(matches!(
        l.handle(
            a,
            Frame::Register {
                id: 999,
                service: service()
            }
        )[0]
        .frame,
        Frame::Error { .. }
    ));
    assert!(matches!(
        l.handle(
            a,
            Frame::Register {
                id: 0,
                service: service()
            }
        )[0]
        .frame,
        Frame::Listed { .. }
    ));
    l.handle(a, Frame::Unregister { id: 0 });
    assert!(matches!(
        l.handle(
            a,
            Frame::Register {
                id: 999,
                service: service()
            }
        )[0]
        .frame,
        Frame::Listed { .. }
    ));
    for stream in 0..MAX_STREAMS_PER_CLIENT as u32 {
        assert!(matches!(
            l.handle(
                a,
                Frame::StreamOpen {
                    stream,
                    src: addr(1),
                    dst: addr(2)
                }
            )[0]
            .frame,
            Frame::StreamOpen { .. }
        ));
    }
    assert_eq!(
        l.handle(
            a,
            Frame::StreamOpen {
                stream: 9999,
                src: addr(1),
                dst: addr(2)
            }
        ),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamClose { stream: 9999 }
        }]
    );
    l.handle(a, Frame::StreamClose { stream: 0 });
    assert_eq!(
        l.handle(
            a,
            Frame::StreamOpen {
                stream: 9999,
                src: addr(1),
                dst: addr(2)
            }
        )[0]
        .to,
        b
    );
}
#[test]
fn stream_handles_cannot_be_used_by_an_unrelated_player() {
    let (mut l, a, _) = pair();
    let c = l.join("Charlie").unwrap();
    l.take_outgoing();
    l.handle(
        a,
        Frame::StreamOpen {
            stream: 7,
            src: addr(1),
            dst: addr(2),
        },
    );
    assert_eq!(
        l.handle(
            c,
            Frame::StreamData {
                stream: 7,
                payload: vec![9]
            }
        ),
        vec![Outgoing {
            to: c,
            frame: Frame::StreamClose { stream: 7 }
        }]
    );
    assert!(l
        .handle(c, Frame::StreamClose { stream: u32::MAX })
        .is_empty());
    assert_eq!(
        l.handle(
            a,
            Frame::StreamData {
                stream: 7,
                payload: vec![9]
            }
        )[0]
        .to,
        a
    );
}
#[test]
fn stream_to_own_virtual_address_uses_distinct_endpoints() {
    let (mut l, a, _) = pair();
    let out = l.handle(
        a,
        Frame::StreamOpen {
            stream: 7,
            src: addr(1),
            dst: addr(1),
        },
    );
    assert_eq!(
        out,
        vec![Outgoing {
            to: a,
            frame: Frame::StreamOpen {
                stream: u32::MAX,
                src: addr(1),
                dst: addr(1)
            }
        }]
    );
    assert_eq!(
        l.handle(a, Frame::StreamAccept { stream: u32::MAX }),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamAccept { stream: 7 }
        }]
    );
    assert_eq!(
        l.handle(
            a,
            Frame::StreamData {
                stream: 7,
                payload: vec![9]
            }
        ),
        vec![Outgoing {
            to: a,
            frame: Frame::StreamData {
                stream: u32::MAX,
                payload: vec![9]
            }
        }]
    );
}
