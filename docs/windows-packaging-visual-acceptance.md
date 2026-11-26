# Windows packaging and visual acceptance

This is a local, opt-in runbook. The CI workflow remains the formatting,
Clippy and headless test matrix on `ubuntu-24.04` and `windows-2025`; it does
not initialize or build the core, create a release package, or capture a GUI.
The steps below require a Windows GUI session and local build prerequisites.
They are not evidence of a successful Windows run by themselves.

## Package provenance

1. Check the Paw commit and the recorded core gitlink with `git rev-parse HEAD`
   and `git ls-tree HEAD vendor/meow-rs`. Initialize the submodule locally with
   `git submodule update --init --recursive` when dependencies are available,
   then compare `git -C vendor/meow-rs rev-parse HEAD` with the gitlink and
   check both worktrees are clean. Do not record local paths or private data in
   evidence. At Paw commit `c2078538c9eac252a6ed3bbe24f86d1c8ee4f485`, the
   recorded core commit is `4954a8d6f9d3ac602d6d5e0e22433ac892cc390f`.
2. When builds are appropriate, `python tools/build-with-core.py` builds the
   pinned core `meow-app`/`meow` and Paw with `--locked --release`, and copies
   `meow.exe` beside `meow-paw.exe` in `target/release`. The script requires an
   initialized submodule but does **not** verify its checkout matches the
   gitlink or is clean. Check those explicitly before building. Paw's Cargo
   package version is `0.1.0` at this commit; the gitlink identifies the core,
   not a verified core version or binary signature. Record both commits and
   SHA-256 hashes of the two resulting binaries for the candidate package.
3. Copy both executables together to a clean package directory. Paw looks for
   adjacent `meow.exe` only after an explicit `MEOW_BIN` and saved engine
   selection; the sidecar is a separate process. Do not include preferences,
   profiles, subscriptions, credentials, build caches or local paths in the
   package or evidence. Verify hashes after copying. There is no installer,
   signing, artifact upload or release job in the current workflow.

## Synthetic Chinese window captures

Build a separate visual-test executable only when resources permit, for example
`cargo build --locked --features visual-test --bin meow-paw`. This feature is
required for `--screenshot`; the release packaging command above enables only
`desktop`. With Python and a functioning Windows GUI, run:

```powershell
python tools/capture-zh-fixture.py target/debug/meow-paw.exe .paw-local/zh-evidence
```

Create `.paw-local` first; the final output directory must not exist. The
wrapper copies the executable alone into a temporary directory, clears
`MEOW_BIN`, redirects user configuration directories, and invokes the
window-render capture with `--no-preferences --no-tray --locale zh-CN` for
`overview`, `proxies`, and `settings`. It supplies no profile or `--start`,
so the expected fixture is an empty, stopped state with no engine. The
application itself disables preferences and tray for screenshots and captures
its own window after a 1.4-second delay; this is not desktop-wide capture.
The wrapper checks PNG headers and nonzero dimensions, not full image integrity,
content, translation, pixel determinism or privacy. It times out each launch
and only publishes after all three header checks pass. The copied executable
must be independently runnable; missing adjacent DLLs or GUI support can make
capture fail. Do not substitute a desktop screenshot or bypass isolation.

Manually inspect all three files for the intended page, Chinese navigation and
shell controls, readable CJK glyphs, clipping, and absence of personal data.
Page content is only partly localized. Record OS, display scaling, font,
commit, binary hash, feature and results without real paths or secrets. Rendering
may vary with fonts, scaling and timing; no baseline or pixel comparison is
provided. Delete local evidence if it contains unexpected data. The offline
wrapper contract can be checked with `python -m unittest discover -s tests -p
"test_capture_zh_fixture.py"` without launching Paw.

## Operational checks

On a disposable Windows test environment, verify the clean package starts and
exits, and that the adjacent core is discovered with no override or saved
engine path. For a managed engine check, use only a synthetic local profile and
a test-owned core. The ignored test
`isolated_session_proxies_loopback_traffic_and_cleans_up` in
`tests/managed_meow.rs` exercises a synthetic DIRECT/REJECT group, loopback
traffic, mode selection and listener cleanup; it requires `MEOW_BIN` and a
local core and binds local ports. Run that named test alone when authorized
and dependencies are available; other ignored tests have separate requirements.
Verify process cleanup and tray/window behavior separately on Windows; the
existing `tools/tray-smoke.py` is Linux/X11-specific. Do not change system
proxy, Clash, DNS, VPN, routes or external network settings, and do not use
live configurations. A stopped-state screenshot does not prove managed-core,
tray, installer or release behavior.
