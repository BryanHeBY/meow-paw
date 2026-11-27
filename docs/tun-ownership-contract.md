# Paw TUN feasibility and ownership contract

This source review uses Paw `5e5fbc4645c009c5d30491d6ca5a6eb983d22044`, which pins Meow
`e3bfb51ea8b07935cb95126dbf755581470eb2cd` at `vendor/meow-rs`.
The pinned core has TUN support, including a Windows Wintun backend. The first
Paw slice is an offline ownership fixture. A live toggle depends on a core or
adapter boundary that can prove exclusive creation, report the acquired resources,
and recover them after either process dies.

## What the pin supports

| Boundary | Pinned behavior and consequence |
| --- | --- |
| Paw configuration | Managed launch and live apply share a renderer that rejects nonempty `listeners`, removes `dns.listen` and the top-level `interface-name`, and replaces `tun` with `enable: false`. Paw generates its own loopback mixed and authenticated controller endpoints. A future capture request must originate from explicit Paw session intent. Imported profiles and subscription refreshes cannot grant it. [Profile renderer](../src/profile.rs#L131), [session launch](../src/engine/session.rs#L42), [live apply](../src/engine/session.rs#L202). |
| Core capability and readiness | The default `full` build includes `listener-tun`. Offline `-t` validates the configuration and feature gates. Top-level TUN setup can fail, roll back its enabled state, and leave the core's regular listeners serving. Paw currently checks those listeners. Authenticated `GET /configs` reports whether the top-level TUN task is live. That response contains no stable adapter identity or per-effect host receipt. [Core features](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-app/Cargo.toml#L109), [offline validation](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-app/src/main.rs#L552), [startup](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-app/src/main.rs#L2404), [controller state](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-api/src/routes.rs#L1266), [Paw readiness](../src/engine/session.rs#L116). |
| Device and packet path | Meow terminates TUN traffic and dispatches it through its tunnel. With automatic routes and DNS hijacking disabled, it still creates or opens the device and assigns interface addresses. An assigned address can itself introduce a connected route. The pinned setup can reuse a same-name adapter and retry with alternate names or addresses. A display name is insufficient ownership evidence. [Device setup](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/mod.rs#L520), [Windows address setup](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/windows_device.rs#L20). |
| Host routes and DNS | Meow's auto-route path can continue after individual route additions fail. Its DNS guard can bind Windows loopback port 53 with auto-route, hijack and fake-IP geometry, and can change host resolver and firewall state across interfaces. Guard cleanup depends on in-process state and can touch shared settings. The guard cannot satisfy Paw's ownership contract. Core resolver use and per-packet interception are separate from changing the host resolver. [Routes](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/route.rs#L20), [listener DNS](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/mod.rs#L850), [host DNS guard](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/dns.rs#L144). |
| Wintun and privilege | The Windows build can embed Wintun 0.14.1. Runtime resolution checks the executable directory and working directory before extraction to a writable location. The build accepts a DLL override. A future privileged path must bind the selected core and effective DLL to verified identity, architecture, provenance, signature policy and a protected absolute location. A custom `MEOW_BIN`, an embedded fallback, or a DLL found by name alone cannot establish trust. Linux also needs a separately verified device and authorization boundary. [Wintun build](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/build.rs#L12), [DLL resolution](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-listener/src/tun/wintun.rs#L31). |
| Process lifetime | Paw's Windows Job Object terminates the child on Stop or owner loss. On Linux the child gets a parent-death signal, and normal Stop tries `SIGTERM` before killing it. These mechanisms contain the owned process. Host cleanup needs its own completion evidence. Meow removes its live TUN slot before awaiting teardown, and a timed-out controller request can keep running server-side. Pinned TUN reconfiguration can stop the old listener before starting a replacement. [Windows Stop](../src/engine/session/windows.rs#L161), [Linux Stop](../src/engine/session/linux.rs#L42), [core teardown](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-tunnel/src/tunnel.rs#L2544), [controller mutation](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-api/src/routes.rs#L1658), [TUN reconcile](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-api/src/routes.rs#L4835). |

The fork documents Administrator for Windows and root or `CAP_NET_ADMIN` for
Linux device setup. Any future authorization belongs inside the host-side
authority. Device access still needs to be verified on the target platform.
[Documented privilege](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/docs/tun.md#L44).

## Transaction and recovery

1. **Admit.** Paw supplies explicit session consent, desired capability and a
   generation. A single authenticated host-side authority owns the adapter and
   every host mutation, irrespective of whether its first implementation is
   inside Meow or a separately reviewed helper. Paw stays unprivileged and owns
   session intent, its child and published status. Meow owns packet processing.
   The authority must expose a create-only, generation-bound device contract.
   A helper would also need a supported packet handoff to Meow. The existing
   profile interface offers no such ownership and handoff contract.
2. **Serialize and preflight.** Hold one lease for the affected host scope
   across Paw processes. Identify the owner by a session ID, generation and a
   boot/process-instance identity that cannot be confused by PID reuse. Check
   effective adapter and address overlap, both address families' routes and
   policy, resolver and required port occupancy, and any traffic capture already
   in effect. For first live admission, active Clash, overlapping VPN capture,
   or an inconclusive inventory blocks TUN before writes. Leave foreign
   processes, adapters, capture, DNS and proxy settings alone. A process-name
   match or an idle API port alone cannot decide this. Recheck before applying
   capture and again before publishing readiness.
   A Paw lease does not coordinate with Clash. A platform operation must supply
   conditional or exclusive semantics for any resource whose ownership could
   change concurrently. Refuse any operation whose race cannot be contained.
   If foreign capture appears during a run, revoke Paw's owned capture through
   the same guarded Stop path. Preserve any effect whose safe removal becomes
   uncertain and report recovery required.
3. **Prepare and apply.** Before each mutation durably record its intent, inverse
   and independently discoverable identity under protected storage. Record the
   exact applied identity after the mutation and verify it by reading back.
   Include any implicit connected routes or addresses in the device contract.
   Use a stable interface ID and creation proof, plus per-effect identity and
   values. On Windows use the adapter GUID and validated LUID mapping. A route
   record needs family, scope/table, destination, next hop, metric and interface
   identity, with independent evidence of ownership. A snapshot or matching
   value alone proves neither ownership nor authority to restore. Store only
   minimal recovery metadata, with no profile contents or credentials. Make the
   write-ahead record and its advancement crash-safe before release of the lease.
4. **Commit.** Ready requires the intended generation's authenticated core
   readiness, independently observed device identity, and read-back of every
   required owned effect. A failed precondition, partial apply, cancellation or
   timeout goes to rollback or recovery. A delayed completion from an old
   generation cannot change the active generation. Keep host DNS, system proxy,
   global routes, standing privilege, services and registry out of the first
   live adapter experiment. Shared settings would need a separate reviewed
   contract with exact baseline, write provenance and atomic compare-and-restore.
   There is no safe fallback to replacing whole host tables or resolver lists.
5. **Stop and recover.** Fence old requests. While the core is available, revoke
   capture in reverse dependency order and wait for completion before releasing
   its data path and stopping the child. After a crash, reacquire the lease and
   prove that the old owner and in-flight writers are gone. Replay only the
   journal, including intents without an apply receipt. Remove an effect only if
   its creation proof, identity and expected state still match and the removal
   cannot break foreign dependencies. A verified absence is already clean.
   Unreadable state, a replacement, or a live or unknown owner leaves a recovery
   record. Keep dependencies of unresolved effects and block new capture until
   recovery completes. Verify the remaining state after cleanup. A core exit or
   a controller `tun.enable: false` alone cannot establish `Stopped`.
6. **Preserve restart intent.** Stop and restart preserve the selected Paw
   profile, its saved settings and each unrelated host resource. Preflight a
   replacement while the old generation runs. If preflight fails, keep the old
   generation. An explicit restart may stop it, verify cleanup, and then request
   a fresh generation with repeated checks. If the new start fails after that
   boundary, report the failure and its cleanup state. A crash or hot profile
   apply cannot turn capture back on. Paw's existing loopback session remains
   usable independently of TUN admission. [Current session worker](../src/app.rs#L830).

## First slice and exact QA

The implemented slice is the standalone [mock ownership fixture](../tests/fixtures/tun_ownership.py).
Its host is an in-memory map with conditional create and compare-and-remove. The
store survives a simulated coordinator loss. Its synthetic route and DNS rows
model separately owned entries without representing shared host settings. Run
with Python 3.10 or later. The fixture does not depend on a particular Meow binary.

```sh
git rev-parse HEAD
git ls-tree HEAD vendor/meow-rs
python3 -I -B tests/fixtures/tun_ownership.py -v
```

The fixture checks active or unknown foreign capture, conditional-create
collision, conflict at readiness, journal replay on either side of each write,
liveness proof, a same-value foreign replacement, a foreign dependency, a crash midway through
cleanup, and restart preservation with stale Stop fencing. It requires no Cargo
build, binaries, sockets, configuration, driver, elevation or GUI. It models
atomic ownership operations and crash boundaries. Persistence, authority
authentication, OS atomicity and native readiness are subsequent acceptance gates.

For later isolated QA, a trusted exact-architecture package and a generated
TUN-disabled loopback fixture can validate ordinary process and controller
behavior. An audited, offline synthetic `-t` probe can establish parser and
build-feature support on an exact executable. Neither exercises the driver.
Real Linux device access and Windows Wintun load, DAD, connected routes,
graceful stop, hard-kill recovery and Clash coexistence need separately authorized
validation in disposable environments. A create-only adapter and durable identity
receipt are the first live blockers even with Meow's `auto-route: false` and
`dns-hijack: []`. [Core DNS option](https://github.com/BryanHeBY/meow-rs/blob/e3bfb51ea8b07935cb95126dbf755581470eb2cd/crates/meow-config/src/lib.rs#L790).
Keep capture unavailable when those prerequisites or the package provenance
cannot be proved. All fixture and design work here is original and adds no
dependency or source from Clash or other GPL projects.
