# Co-op and online play

COD Zombies 1.5.0 has 2–4 player co-op over Bluetooth, the local Wi-Fi
network and the internet (Activision's DemonWare service, long shut down).
**Local (Wi-Fi) co-op now works between codwawzpc windows** through the
`codwawzpc-server` relay (see [Playing co-op](#playing-co-op)); Bluetooth and
Online do not. This page records what is known about the game's networking,
what was fixed, and the plan for the rest.
Addresses are in the 1.5.0 executable `kiwi` (SHA-256 `4c940898…`).

## Goals

Agreed on 2026-10-22:

- Players: codwawzpc on PCs only. Real iPhones running the game are out of
  scope, so the host may replace the iOS transports with anything that works.
- Network: the internet, between friends in different places.
- Order: the **Local (Wi-Fi)** path first, then the in-game **Online**
  (DemonWare) menus.
- Server: a self-hostable service anyone can run for their group.

## The game's three transports

The Co-Op screen (Main Menu → Co-Op (2–4 Players)) has a **Connection Type**
selector that cycles through:

| Type | Menu items | Game classes | iOS / engine APIs |
| --- | --- | --- | --- |
| Bluetooth (2 players) | Host a game, Join a game | `CIwNetworkLobbyBluetooth`, `CIwNetworkManagerBluetooth`, `CIwNetworkSocketBluetooth`, `CIwNetworkDiscoveryBluetooth` | `GKSession` through Marmalade's `s3eIPhoneGameKit` extension |
| Local (Wi-Fi) | Host and join | `CIwNetworkLobbyWiFi`, `CIwNetworkLobbyRoomWiFi`, `CIwNetworkManagerS3E`, `CIwNetworkSocketS3E`, `CIwNetworkDiscoveryBonjour`, `CIwNetworkServiceBonjour` | Bonjour (`CFNetService*`, service type `_CODWAW_IPHONE._tcp`) and BSD sockets through `s3eSocket` |
| Online | Quick Game, Host Private, Join Private | `CIwNetworkLobbyDemonware`, `CIwNetworkManagerDemonware`, `CIwNetworkDiscoveryDemonware`, `IwNetworkAuthManagerDemonware`, `CIwNetworkLobbyInternet`, `CIwNetworkSocketStun`, `s3eStunSessionImpl` | DemonWare `bdAuthService`, `bdLobbyService`, `bdMatchMaking`, `bdStats`, `bdMessaging`, `bdContentStreaming` (crypto: Tiger-192, 3DES, HMAC-SHA1); STUN and UPnP for peer traffic |

DemonWare configuration embedded in the executable: `TitleID=0x5D8`,
`DLCTitleID=0x14B2`,
`AuthServer="codzombies-iphone-live.auth.mmp3.demonware.net"` (a `-dev`
variant also exists). On 2026-10-22 the live host name still resolved
(185.34.107.28); nothing was sent to it. Strings such as
`STUNBasePortuPnPFailed` and `LS_COOP_HOST_MIGRATED` show peer-to-peer play
with host migration.

The executable imports `CFNetService*`, `CFNetServiceBrowser*`,
`_OBJC_CLASS_$_GKSession`, `SCNetworkReachabilityCreateWithAddress`,
`SCNetworkReachabilityGetFlags`, `NSURLConnection`, `gethostbyname`,
`gethostname`, `connect`, `bind`, `accept`, `getsockname`, `getpeername` and
`getsockopt`.

## Why Co-Op failed

### 1. The network looks unreachable

The game probes `0.0.0.0` with `SCNetworkReachability` six times at
startup. touchHLE only reported link-local addresses as reachable, so the
game decided there was no network: the Co-Op screen opened on Bluetooth, and
every type ended in "Connection not available". With network access on but
the probe still failing, the title screen sometimes ignored input in
scripted runs.

Reporting `0.0.0.0` as reachable fixes this: Co-Op then opens on
**Online** and offers **Local (Wi-Fi)**. The flags must be `Reachable` alone.
With `Reachable | IsDirect` (an earlier version of this page said so), Host
Private shows "CONNECTION NOT AVAILABLE" again; checked on 2026-10-22 by
switching only that flag (`sc_network_reachability.rs`).

### 2. Wi-Fi requires the GameKit extension

Tapping Quick Game with Local (Wi-Fi) selected still shows "Connection not
available", **before any socket call**. The path:

- `CFrontendStateCooperative_v7` (`0x1db038`) handles `quick_game`,
  `host_private_game` and `join_private_game`, then calls `FUN_001db1d8`.
- `FUN_001db1d8` reads the connection type (`*(*ptr + 0xc)`: 0 Bluetooth,
  1 Wi-Fi, 2 Online) and maps it to a network-manager kind: 1, 2 or 5.
  Online also needs a logged-in flag (`+0x2f7`). It calls
  `FUN_0012671c(kind)` and shows the popup when that returns 0. On success
  it calls `FUN_00125444(kind, …)`, which sets up the session; for kind 5 it
  also sets up STUN.
- `FUN_0012671c(2)` (Wi-Fi) passes only when an s3e device query returns 3
  (most likely the Windows simulator) **or** `FUN_00254afc()` returns 1. It
  then also checks a config boolean (`+0x54` → byte `+0x10`).
- `FUN_00254afc` asks `s3eExtGet("s3eIPhoneGameKit", table, 0x30)`. Its state
  bytes are at `0xa1812b` (available, —, checked). In a memory dump
  (`runs/dump2.mem.40000`) they read `00 00 01`: checked, not available.
- Kinds 1 and 5 use `FUN_00126864`: two s3e queries (arguments 2 and 1)
  through the dispatcher at `0x38678`. Kind 5 fails when either returns 0 or
  the first returns 8.

The loader's extension table is at `*0x22974`: 21 entries of `0xa8` bytes
each, laid out as:

| Offset | Contents |
| --- | --- |
| `+8` | Lowercase djb2 hash of the name |
| `+0xc` | Available |
| `+0xd` | Failed |
| `+0x10` | Name |
| `+0x14` | Init |
| `+0x18` | Terminate |
| `+0x1c` | Get |

GameKit is entry 16 (hash `0x2e417555`): no init function, get function
`0x49e0`, which copies the function table when the size is ≤ 0x30.
`s3eExtGet` is implemented at `FUN_00021c98`. A run-time probe on the `s3eExtGet` stub
showed the GameKit lookup was **never made**: an earlier check failed first.
The lookup happens (returns 0, state `01 00 01`) once the host reports a
network. **Reachability alone is what the game needs** (isolated on
2026-10-22, one change per run, judged by Host Private's screen):

| `getifaddrs` | `0.0.0.0` reachability | Host Private |
| --- | --- | --- |
| `en0` with the relay address | `Reachable` | Lobby, service registered |
| Fails (-1) | `Reachable` | Lobby, service registered |
| `en0` with the relay address | Not reachable (the old behaviour) | CONNECTION NOT AVAILABLE |
| `en0` with the relay address | `Reachable \| IsDirect` | CONNECTION NOT AVAILABLE |

The game never called `getifaddrs` in these runs, nor in a full join with
it disabled on both sides, which still worked. The `en0` interface is kept
for other flows that might ask.

The loader also wraps `GKSession` itself
(`FUN_00048b28`: `initWithSessionID:displayName:sessionMode:`,
`setDelegate:`, `setDisconnectTimeout:`, `setAvailable:`).

### 3. What the host was missing

| Piece | Before | Now |
| --- | --- | --- |
| Network access | Off, no switch | On only while a co-op relay session is connected |
| BSD sockets | TCP and UDP, `select`, `accept`, `send(to)`, `recv(from)` | Rewritten at the boundary to relay addresses `10.77.0.N` |
| `getaddrinfo` | Implemented | DemonWare names refused |
| `gethostbyname` | Stub, returns NULL | Own and listed `codwawzpc-N` names |
| `getifaddrs` | Stub, returns -1 | One `en0` with the relay address |
| `gethostname` | `touchHLE` | `codwawzpc-N` |
| `CFNetService*` | Missing | Backed by the relay's listings (`cf_net_service.rs`) |
| `GKSession` | Missing | Missing (Bluetooth co-op) |
| `NSURLConnection` | Gated on `network_access` | Same |

## Making Local (Wi-Fi) work

Past the availability check, three more host problems stopped a join:

1. **Bonjour domain.** The game registers in domain `local` (no trailing
   dot); the facade accepted only `local.`, so the host lobby failed with
   "Unable to connect to the game services". Details in
   [coop-service-init.md](coop-service-init.md).
2. **Nonblocking connect.** The loader's `s3eSocketConnect`
   (`FUN_00031c48`) expects `connect` to return -1 with `EINPROGRESS` (36),
   then polls it until `EISCONN` (56). An immediate 0 makes it run the
   connect callback at once and report error 0, and
   `CIwNetworkSocketS3E_v4` (`0x1a9740`) then closes the socket: the guest
   sent its join request and hung up. Relayed TCP connects now report
   `EINPROGRESS`.
3. **Half-close.** When one side closes, the relay used to close the other
   side's socket completely, so its next `send` failed with `EPIPE`. A
   remote close is now a half-close: the reader gets EOF after the queued
   bytes, and later writes are discarded, as over real TCP.

The full investigation, with evidence and the observed messages, is in
[coop-join.md](coop-join.md). In short: the guest connects to the host's
advertised port and sends a 4-byte little-endian length and a message
(type 0, its profile name, its address and listening port); the host answers
on the same connection, and both then exchange keep-alives (`01 00 00 00 0b`).
The host passes `htons(port)` to `CFNetServiceCreate`, and the guest uses
`CFNetServiceGetPortNumber` as is, so the byte-swapped value round-trips.

## Playing co-op

1. Run a relay that every player can reach (see `server/README.md`):
   `codwawzpc-server --listen 0.0.0.0:27960`, or the Docker image.
2. Each player sets the co-op server, with `--coop-server=HOST:27960`
   (and optionally `--player-name=NAME`) or in the launcher's Co-op settings.
3. Host: Co-Op → Connection Type **Local (Wi-Fi)** → Host Private → pick a
   map. The lobby shows "Waiting for players".
4. Others: Co-Op → Local (Wi-Fi) → Join Private, then type the host's
   **profile name** (case sensitive), then Ok.
5. The host presses Start Game.

**Quick Game** also works: it opens Select Level; the first player finds no
game ("Searching for host"), hosts one itself (service `QF` + map, for
example `QFnacht`), and later players with the same map join it. The lobby
then counts down ("Game starting in: 10") and starts without Start Game.

### What was tested

All windows ran on one PC (this project has no second PC with the game
yet). On 2026-10-22:

| Relay | Players | Result |
| --- | --- | --- |
| `127.0.0.1` | 2, Host/Join Private | Join, lobby, Nacht der Untoten; both see each other and the same scoreboard; "Together, my brother!" unlocks. Guest quits: host plays on. Host quits: "Host migrated to …", guest plays on. |
| Another machine on the LAN (Docker image on an aarch64 Orange Pi, over Wi-Fi) | 2, Host/Join Private | 5 of 6 joins worked; one guest showed "Unable to connect to all players." (not reproduced, see below). Rounds played with matching Game Over scoreboards on both sides, a revive included. |
| `127.0.0.1` and the LAN relay | 2, Quick Game | Host hosts, guest joins, countdown, round starts; host quits and the guest migrates. |
| `127.0.0.1` | 4, Quick Game | Before the relay fix below, 4 of 16 runs lost a player or split at the end of loading. After it, 14 of 14 runs kept all four together: they played Round 1 with each other's name tags in view until the host quit, and the others migrated to the next host. |

Not tested: two PCs, the internet, several waves in a row (scripted players
die in round 1; see [Open problems](#open-problems)).

### Latency

The game sends **no UDP during Local (Wi-Fi) play**: all game traffic is
TCP between the players (about 6–50 relay frames per second each way,
under 1.5 KB per 5 s). The only datagrams are its NAT-PMP/uPnP probes at
startup. So a separate UDP path in the relay would carry nothing and is
not planned.

One-way latency through the relay, measured by matching each frame in the
sender's and receiver's logs (both windows on one PC, one clock):

| Relay | Median | p90 | p99 |
| --- | --- | --- | --- |
| `127.0.0.1`, before the fix below | 5.3 ms | 9.4 ms | 10.0 ms |
| `127.0.0.1`, after | 0.12 ms | 0.17 ms | 0.21 ms |
| LAN over Wi-Fi, before | 7–8 ms | 11–17 ms | 25–28 ms |
| LAN over Wi-Fi, after | 1.7–2.8 ms | 4–19 ms | 23–24 ms |

The server used to poll its event queue every 10 ms, which added 0–10 ms to
every frame; it now waits on the queue (`server/src/net.rs`, test
`relayed_frames_are_forwarded_without_polling_delay`). Over the internet,
expect the two legs to the server (player → relay → player) plus TCP's
head-of-line delay after a lost packet, which the game's own TCP traffic
would also have.

### Four-player split (fixed)

With four players, a session sometimes lost a guest at the end of loading
("… has left the game" on the others; earlier runs split into two pairs).
The relay caused it, not the game. Each relayed stream queued incoming data
for the game in a channel of **8 frames**, and dropped the stream when that
was full. Guests exchange bursts of tiny frames (a 4-byte length, then an
11- to 15-byte message, repeated; up to about 20 frames within a few
milliseconds), so a burst
that arrived faster than the writer thread drained it overflowed the queue
and the relay closed a working connection. When the broken link was
between two guests, the game's host dropped one of them about 9 s later;
when it was a guest's link to the host, that guest lost the host.

Evidence (2026-10-23, 28 four-player Quick Game runs on `127.0.0.1`, each
window logging its stream frames with their ids):

- Each early close came 13–42 µs after the last frame of such a burst, from
  the receiving guest's own relay. With close reasons logged (one split),
  it read `Stream 2: incoming queue full, closing`, and the host's close of
  that guest followed 9.07 s later.
- The unit test `burst_of_small_stream_frames_is_delivered_without_closing`
  (1000 frames of 12 bytes into one stream) failed 3 of 3 times before the
  fix and passed 20 of 20 after it.
- Runs: 4 of 16 split before (1 of 2 on 2026-10-22, 3 of 14 on 2026-10-23), 0 of 14 after.

The queue is now bounded by bytes (256 KiB per stream, `MAX_QUEUED_BYTES` in
`src/codwawzpc/coop/relay.rs`), so tiny frames cannot fill it, and a game that
stops reading still has its stream closed
(`stream_is_closed_when_the_game_stops_reading`).

### Open problems

- **"Unable to connect to all players."** on one guest join through the LAN
  relay (1 of 6, no trace was on). Not seen since: 84 more guest joins on
  `127.0.0.1` with relay and Bonjour tracing all had their stream to the host
  accepted, and no relay error frames.
- **A standing player's model snaps every 2 s** on the other players'
  screens. Measured from 100 ms captures of a guest watching the idle host:
  the host's position (its name tag) does not move, but its pose jumps at
  2.0 s intervals, 0.1–0.2 s after each periodic message the host sends every
  player every 2.0 s (49 bytes at first, growing to 109 bytes during the
  round). The relay delivers those messages in under 1 ms, and the snaps stay
  with the game's own 45 FPS cap (`--frame-rate=45`), so neither the relay
  nor the 60 FPS change causes them. Whether the game does the same on an
  iPhone is unknown; the next step is decoding that message.
- **The server's per-client queue is counted in messages** (128 in
  `server/src/net.rs`), and a full queue disconnects that client. This is
  the same pattern as the stream queue fixed above, but with room for several
  bursts; no run has hit it, and it may matter over slow internet links.
- **Crash with knife spam while turning** (not co-op): pressing V every
  0.4 s while turning 120°/s crashes the game thread with co-op off too
  (4 of 4; script `runs/coop-solo-spin.txt`). The jump is to address 0 (the
  log shows "null-page access at 0x0"; PC reads 4 after the failed fetch).
  Investigated on 2026-10-23, not fixed:
  - It happens inside a host-posted melee event (`native::frame` →
    `0x15893c`, action 11): the melee hit test `FUN_00206e7c` returns
    through a saved return address that has become 0. The slot is at the
    same stack address (`0x6c4dc0`) every time; with the parameter setter
    `FUN_00150e08` replaced by host code the crash moved to that function's
    caller, at the same address.
  - Only the hit path is affected (the knife finds a target, so the function
    computes a direction and an angle). In the failing call the frames above
    the slot hold what looks like a frame of the loader's allocator
    (`FUN_0002ed9c`, return address `0x2edec`) instead of the caller's.
  - Ruled out: the setter's own store (a host probe saw no write into the
    stack) and the errno write in the game's `acos` (`0xa59b60`, a global).
  - Not established: whether other code ran on this stack at a stale stack
    pointer while the hit test was live, or the hit test was entered with a
    return address of 0. Watching the slot needs a debugger, and with
    `--gdb` the game stays on its title screen in this build.
- **Several waves** not verified: no scripted strategy kept both players
  alive past round 1, so round-to-round sync was only seen for round 1.

With a relay session, the guest cannot reach real addresses: TCP to
anything outside `10.77.0.0/24` is refused (`ECONNREFUSED`, 61), and UDP
`sendto` outside it, except to `255.255.255.255`, fails with errno 51
(`ENETUNREACH`; it was 50, `ENETDOWN`, until 2026-10-23, but the relay network
is up and only that destination has no route). `bind` while the relay is
disconnected still fails with 50. The game's uPnP, NAT-PMP and STUN probes go nowhere. Without
a co-op server, network access stays off. DemonWare names are always
refused.

## Plan

### Phase 1: Local (Wi-Fi) over the internet

1. Find why `s3eIPhoneGameKit` reports unavailable, and fix it in the host
   (or with a narrow guest patch), so Wi-Fi Host and Join get past the
   popup.
2. Make network access a real setting instead of the `CODWAWZPC_NETWORK`
   variable.
3. Implement `CFNetService` / `CFNetServiceBrowser` (register, browse,
   resolve, TXT data, run-loop scheduling, client callbacks) on the host,
   backed by the server. A hosted game is listed on the server, and other
   players' join lists show it. Optional: real mDNS on the LAN.
4. Relay game traffic through the server, so nobody needs port forwarding.
   The host maps each peer's address to a relay tunnel, and the game's
   netcode is untouched. Decide first whether a direct connection is tried
   before the relay.
5. `codwawzpc-server`: one self-hostable binary (and a Docker image) for the
   lobby, listings and relay. Co-op settings (server address, player name)
   go in the CLI and the launcher.
6. Test with two game windows on one machine, then between two machines
   over the internet. Check joining, a full round, a player leaving, and
   host migration. *Done on one machine, with the relay on loopback and on
   another LAN machine; two PCs and the internet are still untested (see
   [What was tested](#what-was-tested)).*

### Phase 2: Online (DemonWare)

1. Map the DemonWare client (REA and Ghidra): auth ticket exchange, lobby
   connection, message framing and crypto, matchmaking (create, find, join
   sessions, invites), and stats and profile calls the menus need.
2. Point the game at the server: answer the `AuthServer` host name (and any
   lobby address it returns) in the host's `getaddrinfo` / `gethostbyname`.
3. Teach `codwawzpc-server` enough of auth, lobby and matchmaking for Quick
   Game, Host Private and Join Private. Make STUN either hit the server or
   go through the Phase 1 relay.

## Reproducing

Research files live outside the repo in `~/Downloads/codwaw-research`:

| Path | Contents |
| --- | --- |
| `re/all.c` | Ghidra decompile of the whole executable |
| `re/vtables.txt`, `re/rtti.json` | Class vtables |
| `runs/dump2.mem.40000` | 4 GiB guest memory dump; data segment decoded |
| `re-tools/` | `run.sh`, `dec.sh` (Ghidra headless), `findptr.py`, `mem.py` |
| `ext/rea-cli` | [REA](https://github.com/morluto/rea) |

String-key names are in
[string-keys.md](string-keys.md) (`LS_COOP_*`,
`LS_DEMONWARE_*`).

A run that opens Co-Op, switches to Local (Wi-Fi) and taps Quick Game:

```text
500 focus gained
12000 tap 420 300
14000 tap 420 300
16000 capture
16500 tap 120 118
19000 capture
19500 tap 330 33
21000 capture
22000 tap 90 118
24000 capture
35500 quit
```

```sh
RUNARGS=--coop-server=127.0.0.1:27960 ~/Downloads/codwaw-research/re-tools/run.sh coop7 \
  ~/Downloads/codwaw-research/runs/coop7.txt \
  CODWAWZPC_TRACE=touchHLE::libc::sys::socket,touchHLE::libc::netdb
```

For two players, run two windows with separate `XDG_DATA_HOME`,
`XDG_CONFIG_HOME` and `XDG_STATE_HOME` trees. The connection type is saved
per profile and cycles Online → Local (Wi-Fi) → Bluetooth on each tap at
(330, 80). Useful taps: Host Private (90, 158), Join Private (90, 198), the
name bar (150, 118), Ok and Start Game (450, 296). Relay and Bonjour
logging: `touchHLE::codwawzpc::coop::relay` and
`touchHLE::frameworks::cf_network::cf_net_service`.

`runs/coop-host.txt` and `runs/coop-join.txt` are the scripts for a
two-window run that hosts, joins (as host profile Player8328) and starts a
round. Start the join window about 2 s after the host. The join script
assumes the guest profile's saved connection type is already Local (Wi-Fi).

More players: a tree whose `data/codwawzpc` holds only `installations` (a
link to an existing install) and `current.plist` starts a fresh profile with
a new random name, whose connection type is already Local (Wi-Fi). Quick
Game is at (90, 118); `runs/coop-qg-host.txt` and `runs/coop-qg-guest.txt`
pick Nacht der Untoten (start the others about 8–10 s after the host).

With `CODWAWZPC_TRACE=touchHLE::codwawzpc::coop::relay`, the relay logs
every relayed game frame (direction, datagram or stream, size, FNV-1a
fingerprint, wall-clock time in µs, and for streams the local stream id, as
in `Out Stream 4 B fp df37217a at 1792733378342308 us on stream 2`), every
control frame it sends or receives with its time
(`Out StreamClose { stream: 2 } at … us`), why it closed a stream (`the game
closed it`, `incoming queue full`, a failed read or write), and a rate
summary every 5 s. Each end numbers a stream differently: the opener chooses
its id and the server picks the accepter's. Tools in `re-tools/coop/`:

| Tool | Use |
| --- | --- |
| `pair.sh <tree> <name> <script> <run> [server]` | One window with its own XDG tree; output in `$OUT/runs/<run>` |
| `gen.py` | Builds long scripts from `coop-host.txt` / `coop-join.txt` |
| `latency.py host.log guest.log` | One-way latency per direction, from matched fingerprints |
| `timeline.py H=log G=log …` | Stream open/accept/close of several players on one clock |
| `streams.py [N] H=log G=log …` | Each player's streams: peer, who opened and who closed each one and when, and the last N frames received before the close |
| `gaps.py H=log …` | Silences over 1.5 s in each player's sends |

For test runs, set `ALSOFT_DRIVERS=null` (through `TRACE_ENV` with
`pair.sh`) so OpenAL opens no audio device and the windows stay silent, and
pass each tree's profile name as `--player-name` so relay logs and the game
agree on names. Run options such as `--frame-rate` are saved into the tree's
settings.

Taps work more reliably than keys at the title screen in scripted runs.
`CODWAWZPC_TRACE` takes a comma-separated list of module paths whose debug
logging to enable.
