# Wi-Fi host service initialization after map selection

Investigation date: 2026-10-22. Binary: the research `re/kiwi` used by the
existing Ghidra export. This note covers the second failure, after the original
"Connection not available" check is passed.

The host facade rejects the game's default Bonjour domain. The game supplies
`local` without a trailing dot; the initial `CFNetServiceRegisterWithOptions`
implementation accepts only `local.`. This produces an immediate false return
before registration and before the facade's success log. This is a concrete
host compatibility defect, independent of the earlier UDP route probe. The
static call path establishes how it can produce the observed failure; a
runtime error log and replay are still needed to distinguish it from another
earlier failure on the same run.

## Direct binary evidence

The Mach-O image has `__TEXT` vmaddr `0x1000`, fileoff `0`. Reading guest address
`0x4f748` therefore reads file offset `0x4e748`. Its little-endian word is
`0x00063e48`. Address `0x63e48` contains the NUL-terminated ASCII bytes
`6c 6f 63 61 6c 00` (`local`). This read uses the original binary, not a guessed
Ghidra string label.

`all.c` around line 55330, function `FUN_0004f5c0` at `0x0004f5c0`, selects:

```c
iVar7 = DAT_0004f748;              // points to "local"
if (*DAT_0004f74c != 0)
    iVar7 = *DAT_0004f74c;         // optional caller-provided domain
uVar4 = CFStringCreateWithCString(0, iVar7, 0x600);
uVar8 = CFNetServiceCreate(allocator, uVar4, kind, name, port);
```

`FUN_0004f4ac` at `0x4f4ac` stores the registration arguments in globals. Its
fourth argument becomes `*DAT_0004f534`, the domain override later read through
`DAT_0004f74c`. Both constant pointers identify `0xa0d220`. The host-discovery
caller passes zero as this fourth argument, so the default `local` string is
selected.

The facade's `CFNetServiceCreate` maps an empty domain to `local.` but initially
preserves every other supplied domain. Its registration function then uses
`domain.eq_ignore_ascii_case("local.")` as a prerequisite. The game's `local`
therefore fails with `kCFNetServicesErrorBadArgument` (`-72004`). The only
registration `log_dbg!` follows `Session::register`, so an empty CFNetService
log does not establish that no CF function was called.

## Caller path and failure propagation

The relevant Ghidra function names and addresses are:

| Address | Function | Role |
| --- | --- | --- |
| `0x1a8ecc` | `CIwNetworkLobbyWiFi_v1` | Starts the host lobby through the base lobby method. |
| `0x1654f4` | `CIwNetworkLobby_v1` | Obtains discovery through vtable offset `0x34`, calls its vtable entry 0, and creates the lobby room only if that call succeeds. |
| `0x1a9154` | `CIwNetworkLobbyWiFi_v13` | Lazily creates the Bonjour discovery object. |
| `0x1e2ef8` | `CIwNetworkDiscoveryBonjour_v0` | Checks ZeroConf availability, then calls registration with port, host name, service type, domain override `0`, and one TXT item. |
| `0x255454` | `FUN_00255454` | Loads `s3eExtZeroConf` if necessary and dispatches its registration operation. |
| `0x3c164` | `FUN_0003c164` | Extension registration wrapper leading to `FUN_0004f4ac`. |
| `0x4f4ac` | `FUN_0004f4ac` | Packs TXT, stores arguments, and synchronously dispatches `FUN_0004f5c0` to the main thread. |
| `0x4f5c0` | `FUN_0004f5c0` | Creates/configures the CF service and registers it. |

`FUN_0004f5c0` calls `CFNetServiceSetTXTData`, then `CFNetServiceSetClient`, then
schedules on the current run loop and calls `CFNetServiceRegisterWithOptions`.
Only a successful registration writes its service pointer through
`DAT_0004f76c` and returns zero. On failure it cancels the service and returns
one. `FUN_0004f4ac` returns the pointer global, initially set to zero.

`CIwNetworkDiscoveryBonjour_v0` stores the resulting pointer at object offset
`0x48`. If it is zero, the method sends the lobby's failure notification and
returns zero. `CIwNetworkLobby_v1` consequently never creates a lobby room and
returns false. `CIwNetworkLobbyWiFi_v1` sends the failure notification as well.
This explains an immediate service-init failure without a new game socket
being opened after map selection.

Ghidra sometimes marks wrappers `void` or omits forwarded arguments; the call
path above follows the actual stored globals and dispatcher table as well as
the recovered C. The registration extension table at the `s3eExtGet` getter
contains `0x3c164` in its final slot (`0xa0019c`), consistent with that dispatch.

## Runtime evidence available before correction

In `/tmp/codwawz-coop/host2.log`, GameKit loading succeeds at line 8633:
`s3eExtGet("s3eIPhoneGameKit", ...) = 0`. ZeroConf loading succeeds at line 8664:
`s3eExtGet("s3eExtZeroConf", ...) = 0`, and the GameKit state is `01 00 01`.
Thus the original availability failure has already been passed without a
guest networking patch.

Between that ZeroConf lookup and the subsequent repeated timer/device events,
the trace contains no new socket creation, bind, or listen operation. Startup
socket operations, including the UDP route probe to `216.239.59.147:0`, precede
this lookup by thousands of lines. Those startup probes are a separate
compatibility question and do not refute the domain defect.

## Narrow correction and distinguishing check

Normalize the local domain at `CFNetServiceCreate`: empty, `local`, and `local.`
should all produce canonical `local.` (case-insensitive matching). Continue
rejecting unsupported domains when registering with the relay. Add a pure
normalization regression test, including an unrelated domain left unchanged
so it can still be rejected.

`CFNetServiceBrowserSearchForServices` already explicitly accepts empty,
`local`, and `local.` case-insensitively; it has no equivalent rejection bug.
The existing service-kind canonicalization also already accounts for the
game's `_CODWAW_IPHONE._tcp` without a trailing dot.

For the control replay, log failed registration's actual domain, type, port,
and TXT length before returning `-72004`. A `domain="local"` failure confirms
this branch at runtime. After domain normalization, expect
`CFNetServiceRegisterWithOptions` success and the game's own host lobby, then
validate a second browser can discover and resolve the listing. The root agent
owns these changes and the game replay; this diagnostic leaf changes no code.
