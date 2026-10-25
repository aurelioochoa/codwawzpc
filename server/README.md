# codwawzpc relay

The relay lets codwawzpc PCs discover each other's Wi-Fi games and exchange game
traffic through one TCP connection per player. It does not contact DemonWare or
Activision. Players use the game's Co-Op → Local (Wi-Fi) menus; configure each
client's co-op server to this machine's `HOST:27960`.

From the repository root:

```sh
cargo run --locked --release -p codwawzpc-server -- --listen 0.0.0.0:27960
```

`--listen ADDR:PORT` defaults to `0.0.0.0:27960`. Use
`--listen 127.0.0.1:27960` for local testing. Open TCP port 27960 in the hosting
machine's firewall and forward that TCP port if it is behind a router. Clients
need only outbound TCP access. Stop the process with Ctrl-C, or stop its container.

Build the Docker image **from the repository root**, because the shared protocol
crate lives outside `server/`:

```sh
docker build -f server/Dockerfile -t codwawzpc-server .
docker run --rm -p 27960:27960/tcp codwawzpc-server
```

The image uses a Rust build stage and a Debian runtime, runs as UID/GID 65534,
and has no native game dependencies. It needs no volume or database. A restart
ends all sessions and clears listings.

The server assigns addresses `10.77.0.1` through `10.77.0.254`. Player names are
unique, control characters are removed, and names are limited to 63 UTF-8 bytes.
Listings are sent to existing players and to newcomers; each player may publish
64 services and hold 1024 streams. UDP unicast and broadcast are relayed; broadcast
is copied to all other players. TCP stream IDs are translated per endpoint so
simultaneous connections cannot collide. Source IPs always come from the joined
player's assigned address, regardless of the incoming frame's source IP.

Protocol version 1 uses a big-endian four-byte frame length (body maximum 64 KiB),
a one-byte tag, big-endian integers, UTF-8 strings prefixed by a two-byte length,
and binary payloads prefixed by a four-byte length. A connection must begin with
`Hello`; a different version produces a readable error and closes the connection.
The server sends `Ping` every five seconds, drops clients silent for 15 seconds,
and immediately removes their listings and closes their streams on disconnect.
Outbound queues are bounded; a slow reader is disconnected rather than allowing
unbounded memory growth. The relay does not encrypt or authenticate sessions;
anyone able to connect can see lobby listings and send traffic to other players.
Use a trusted network or a private VPN for a private game.

Run the protocol, lifecycle, and loopback TCP integration tests:

```sh
cargo test --locked -p codwawzpc_coop_proto -p codwawzpc-server
```

The tests bind ephemeral ports only on `127.0.0.1`. Passing these tests establishes
relay behavior on loopback; it does not establish gameplay across the internet.
