# Proxy endpoint checks
Paw's Unlock page runs two opt-in HTTPS 204 connectivity checks through selected
concrete proxy nodes. A successful 204 shows reachability to that endpoint
through that proxy; it is not proof of streaming access or geographic region.
Tests do not select a connected node, change routing, or update latency history.

The bundled Meow advertises the feature only on a loopback TCP controller with
a nonempty secret. A stale configuration generation, DIRECT, unresolved groups,
and configured dialer chains are not used. If proxy-routed DNS would be needed
to look up a proxy origin, Meow requires a cached or static answer instead.

## Local contract test
The default Rust tests use synthetic HTTP replies. For the Linux live smoke,
build the pinned Meow and Paw's library test executable; get the latter path
from `cargo test --locked --lib --no-run`. Run the fixture in a private user
and network namespace with only loopback enabled:

```sh
unshare -Urn sh -c 'ip link set lo up &&
  python3 -B tools/route-check-loopback.py "$@"' -- /path/to/meow /path/to/paw-library-test-binary
```

This launches a disposable Meow process and a fake SOCKS listener that records
exactly one destination and intentionally rejects the connection. It verifies
that valid requests use the selected adapter and that invalid controls do not
dial. It does not contact either public endpoint. Meow's in-memory core tests
cover successful TLS and 204 replies.
