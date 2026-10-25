# Joining a Local (Wi-Fi) game: a field report

How a guest joins a host's Local (Wi-Fi) co-op game in COD Zombies 1.5.0, why
the join failed under codwawzpc after discovery already worked, and the two
host fixes that made two codwawzpc windows play a round together over the
`codwawzpc-server` relay. Investigated on 2026-10-22. Addresses are in the
1.5.0 executable `kiwi` (SHA-256 `4c940898…`); the loader functions below
`0x40000` are in the same image.

Each statement is marked by how it is known:

- **Observed**: read from the executable, the decompile or a run-time log,
  reproducible with the method given.
- **Confirmed**: observed, and seen to behave that way in the running game.
- **Inferred**: the most likely reading of what was observed, not proven.
- **Unknown**: not established; listed under [Open questions](#open-questions).

Earlier stages are covered elsewhere: why Co-Op said "Connection not
available" in [multiplayer.md](multiplayer.md#why-co-op-failed), and why
the host lobby failed to register its Bonjour service in
[coop-service-init.md](coop-service-init.md).

## Contents

- [Symptom](#symptom)
- [The join, step by step](#the-join-step-by-step)
- [Cause 1: the connect finished too early](#cause-1-the-connect-finished-too-early)
- [Cause 2: the relay turned a close into a reset](#cause-2-the-relay-turned-a-close-into-a-reset)
- [Result](#result)
- [The join messages](#the-join-messages)
- [Port byte order](#port-byte-order)
- [Open questions](#open-questions)
- [Follow-up](#follow-up)
- [Method and tools](#method-and-tools)

## Symptom

**Confirmed.** With the host waiting in its lobby ("Waiting for players"), the
guest chose Co-Op → Local (Wi-Fi) → Join Private, typed the host's profile
name and pressed Ok. It showed "Searching for host", then "NO GAME FOUND".

The game name to type is the host's **profile name**, case sensitive (the
screen says so). The host registers its Bonjour service as `PF` followed by
that name, for example `PFPlayer8328` (**Observed**).

## The join, step by step

**Observed** in the guest's trace (`touchHLE::frameworks::cf_network::cf_net_service`
plus temporary per-call logging):

1. `CFNetServiceBrowserSearchForServices` for `_CODWAW_IPHONE._tcp`.
2. The browse callback delivers the host's listing.
3. The game reads its name, domain and type, creates a new service from
   them, then calls `SetClient`, `ScheduleWithRunLoop` and
   `ResolveWithTimeout`.
4. After the resolve callback it reads the target host, TXT data, port and
   addressing, then stops and invalidates the browser. The loader logs
   `resolved: 'PFPlayer8328' 'local.' '_CODWAW_IPHONE._tcp.' 'codwawzpc-1.local.' 9816`.
5. It opens a TCP socket and connects to `10.77.0.1` on the host's listening
   port.

So discovery and resolution worked; the failure came after the connect.

## Cause 1: the connect finished too early

**Observed** in the guest's socket trace: the guest connected, sent 4 bytes
then 34 bytes, and closed the socket with no socket call in between. It had
a listening socket of its own (fd 6) and never read from the connection.

**Observed** in the decompile. The game's `CIwNetworkSocketS3E_v4`
(`0x1a9740`) calls `s3eSocketConnect` with a callback, then
`s3eSocketGetError`:

```c
thunk_FUN_002bdd38(param_1[0x111], auStack_120, uVar1, param_1); // s3eSocketConnect
iVar2 = thunk_FUN_002bdd60();                                    // s3eSocketGetError
if (iVar2 == 0x3e9 || iVar2 == 1000) {
  return 1;                      // in progress or would block: keep the socket
}
thunk_FUN_002bdd24(param_1[0x111]);  // anything else: s3eSocketClose
```

The loader's `s3eSocketConnect` (stub `0x2bdd38` → `FUN_000318d4` →
`FUN_00032008` → `FUN_00031c48`) calls the BSD `connect` and sorts the
result:

| `connect` result | Loader behaviour |
| --- | --- |
| 0 | Posts the success callback at once; `GetError` stays 0 |
| -1, `EINPROGRESS` (36) or `EAGAIN` (35) | Pending; the loader calls `connect` again later |
| -1, `EALREADY` (37) | Still pending |
| -1, `EISCONN` (56) | Connected; posts the success callback |
| other errors | Posts the failure callback |

codwawzpc's relayed `connect` waited for the relay's `StreamAccept` and
returned 0. The loader then ran the connect callback, which sent the join
request (**Inferred** from the order of calls), and returned with error 0.
The game read 0 as neither "in progress" nor "would block", and closed the
socket.

On iOS, a nonblocking `connect` returns `EINPROGRESS`, so this branch was
never taken (**Inferred**). **Fix:** a relayed TCP `connect` still
establishes the stream first, but now returns -1 with `EINPROGRESS`
(`src/libc/sys/socket.rs`). The loader's next `connect` gets `EISCONN`, which
the existing code already returned for a connected socket. **Confirmed**: the
connection stays open and both sides exchange messages.

## Cause 2: the relay turned a close into a reset

Before cause 1 was found, the host's side of the same join showed a second
problem. **Observed** in the host's trace: the host accepted the
connection, read the guest's 4 + 34 bytes, wrote a 4-byte reply, and its next
`send` failed with `EPIPE` (Broken pipe).

**Observed** in the relay client (`src/codwawzpc/coop/relay.rs`): when a
`StreamClose` arrived for an established stream, it shut down the read side
of its tunnel socket. The pump reading that socket then stopped, every clone
was dropped, and the host game's socket was fully closed.

Over real TCP the guest's close is a FIN. The host can still write, and on a
LAN its first few writes land in the send buffer before any reset comes back.
The relay closed the socket at once instead, so the second write failed.

**Fix:** a remote close is a half-close. The writer thread drains queued
bytes and then shuts down only the write side, so the game reads EOF. The
pump keeps reading the game's later writes and discards them, because the
stream no longer exists. The unit test `peer_close_is_a_half_close` reproduces
the failure: it hit `BrokenPipe` on the second write before the fix and
passes after it.

With cause 1 fixed, the game no longer closes after its first message, but
the half-close is still how the relay should treat a closed peer.

## Result

**Confirmed**, with two windows on one PC and the relay on `127.0.0.1`:

- Both lobbies list Player8328 and Player9512. The host shows Kick and
  Start Game; the guest shows "Waiting to start".
- Start Game loads Nacht der Untoten for both. The guest sees the host's
  character labelled with its name and points, both unlock "Together, my
  brother!", and both show the same Game Over scoreboard.
- The guest quits mid-round: the host keeps playing alone.
- The host quits mid-round: the guest shows "You're on your own, soldier..."
  then "Host migrated to Player9512", and keeps playing.

Not tried at the time: two machines over the internet, Quick Game, three or
four players. See [Follow-up](#follow-up) for what was tried later.

## The join messages

**Observed** relay payloads (hex). Every message is a 4-byte little-endian
length followed by the body.

Guest → host, 34 bytes, sent first:

```text
00                    message type (Inferred: join request)
0a 00 00 00           name length 10
50 6c 61 79 65 72 39 35 31 32   "Player9512"
02
02 00 4d 0a           10.77.0.2, as a little-endian u32
26 fd                 0xfd26 = 64806, the guest's listening port
23 85 cd f9           Inferred: the guest's id
00 00 00 00 00 05 00 fd
```

Host → guest, in order:

| Length | Starts with | Notes |
| --- | --- | --- |
| 42 | `02 01 00 00 00 67 f0 cc f9 …` | Contains both profile names and the guest's address (**Inferred**: join accepted) |
| 5 | `32 00 00 00 00` | |
| 13 | `12 02 00 00 00 67 f0 cc f9 23 85 cd f9` | Both ids from above |
| 25 | `09 00 00 00 00 b0 00 00 00 …` | Repeats later |
| 1 | `0b` | Sent by both sides repeatedly (**Inferred**: keep-alive) |

The guest's listening port is in the request, but the host answered on the
same connection and never connected back to it. What that listener is for was
**Unknown** at the time; with more players it carries a full mesh (see
[Follow-up](#follow-up)).

## Port byte order

**Observed:** the host bound its listener to port 24870 (`sin_port` bytes
`61 26`) but registered the service with port 9825 (`0x2661`), the same two
bytes swapped. The guest connected to 24870. So the game passes
`htons(port)` to `CFNetServiceCreate` and uses `CFNetServiceGetPortNumber` as
is, and the value round-trips through the relay unchanged. The `sockaddr`
that `CFNetServiceGetAddressing` builds stores the registered value in network
byte order, so it would name port 9825. Later traces showed that the guest
does call `CFNetServiceGetAddressing` in the join flow, but connects to the
`GetPortNumber` value: the host registered `PFPlayer8328:9783` and the guest
opened its stream to port 14118 (`0x3726`, the swap of `0x2637`), not 9783.
So the swapped port in the addressing is never used (**Observed**).

## Open questions

Answered in [Follow-up](#follow-up): the GameKit lookup (reachability), the
guest's listener (mesh), `CFNetServiceGetAddressing` (read, port unused), UDP
lag (there is no UDP during play) and the four-player split (a relay queue).
Still open:

- The meaning of the remaining message fields, including the message the
  host sends every player every 2.0 s during play (see
  [multiplayer.md](multiplayer.md#open-problems)).

## Follow-up

Later on 2026-10-22, with the relay on `127.0.0.1` and on a second machine
on the LAN (the server's Docker image on an aarch64 Orange Pi, over Wi-Fi):

- **GameKit lookup: reachability.** **Confirmed** by changing one thing per
  run. With `getifaddrs` failing, Host Private still opened the lobby and a
  full join worked; the game never called `getifaddrs`. With `0.0.0.0`
  unreachable, or reported as `Reachable | IsDirect`, Host Private showed
  "CONNECTION NOT AVAILABLE". Table in
  [multiplayer.md](multiplayer.md#2-wi-fi-requires-the-gamekit-extension).
- **The guest's listener: the mesh.** **Observed** with four players: each
  new guest opened a stream to the host and one to every earlier guest's
  listener (10.77.0.3 → .2; 10.77.0.4 → .2 and .3).
- **No UDP during play.** **Observed** with per-frame relay logging: the only
  datagrams were 12-byte NAT-PMP/uPnP probes at startup; lobby and play are
  TCP stream data at about 6–50 frames per second. Relay latency is in
  [multiplayer.md](multiplayer.md#latency).
- **Quick Game.** **Confirmed**: Select Level, then the first player hosts
  (`QFnacht`) and later players join; a countdown starts the round.
- **Three or four players.** **Confirmed** once with four: all four played
  Round 1 together, and the others migrated when the host quit. In a second
  run the session split at the end of loading: the second player closed its
  streams to the third and fourth, then the host closed its own to them.

On 2026-10-23, with the stream id and every control frame in the relay log:

- **The split was the relay's.** **Confirmed.** A guest's relay closed a
  working guest-to-guest stream because its per-stream queue (8 frames)
  overflowed on a burst of tiny frames; its log read `incoming queue full`
  13–42 µs after the burst's last frame. About 9 s later the host dropped one
  of the two guests. With the queue bounded by bytes instead, 14 of 14
  four-player runs kept all four players (4 of 16 split before). Details in
  [multiplayer.md](multiplayer.md#four-player-split-fixed).
- **The host notices a broken mesh.** **Observed** in the three runs where a
  link between two guests broke (one on 2026-10-22, two on 2026-10-23): the
  host closed its stream to one of those guests 9.0–9.1 s later, and that
  guest then closed its remaining streams. In a fourth run the broken link
  was a guest's stream to the host itself, and nothing followed. **Inferred:**
  a guest reports a lost peer to the host, which waits about 9 s before
  removing a player.

## Method and tools

- **Two windows:** separate `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and
  `XDG_STATE_HOME` trees per player, each run with
  `--coop-server=127.0.0.1:27960 --player-name=…` and a
  `CODWAWZPC_INPUT_SCRIPT`, against
  `target/release/codwawzpc-server --listen 127.0.0.1:27960`. Captures were
  read as images. Tap coordinates are in
  [multiplayer.md](multiplayer.md#reproducing).
- **Four-player runs:** `re-tools/coop/pair.sh` per window (host first, the
  three guests 9–10 s later), then `streams.py` on the four logs to see which
  stream closed first, who closed it and what it received before.
- **Traces:** `CODWAWZPC_TRACE=touchHLE::codwawzpc::coop::relay,touchHLE::libc::sys::socket,touchHLE::libc::posix_io`,
  plus `touchHLE::frameworks::cf_network::cf_net_service`. The per-call
  CFNetService logging and the payload hex dumps were temporary and have been
  removed.
- **Loader stubs:** each `s3e*` call in the game goes through a stub of the
  form `ldr r12, [pc]; ldr pc, [pc]; <target>; <dispatcher 0x38678>`. Reading
  the third word of a stub in `kiwi` (file offset = address − `0x1000`) gives
  the loader function, for example `0x2bdd38` → `0x318d4` (`s3eSocketConnect`),
  `0x2bdd60` → `0x20794` (`s3eSocketGetError`), `0x2bdd24` → `0x31a00`
  (`s3eSocketClose`).
- **Decompile:** `~/Downloads/codwaw-research/re/all.c` (Ghidra headless),
  searched with `awk '/^\/\/ ==== FUN_00031c48 /{p=1} p{print} p&&/^}/{exit}'`.
