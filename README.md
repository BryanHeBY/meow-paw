# Meow Paw

The pinned kernel fork is the Git submodule at `vendor/meow-rs`.
Build the desktop app and its kernel together:

```sh
git submodule update --init --recursive
python3 tools/build-with-core.py
```

Run `meow-paw` (Windows: `meow-paw.exe`) in `target/release`. Keep the
adjacent `meow` (`meow.exe`) when copying or packaging the app.
Paw uses that bundled kernel if there is no saved custom engine path.
An explicit `MEOW_BIN` or a kernel selected in Settings still overrides
the bundle. The two binaries remain separate processes.

## Continuous integration

The Linux and Windows CI jobs check formatting, lint the desktop targets,
and execute the headless test suite with locked dependencies. They do not
build or publish the kernel submodule or run a graphical tray session.
See the [Windows packaging and visual acceptance runbook](docs/windows-packaging-visual-acceptance.md)
for local provenance checks and isolated synthetic captures.

For Omarchy on Apple Silicon (and other compatible Arch Linux ARM systems),
[build and install the native AArch64 package](docs/arch-arm64-package.md)
without compiling locally.
Paw listens on `127.0.0.1:7890` for its mixed proxy. The authenticated controller
still uses a dynamically allocated loopback port. To use a different mixed port,
save it under Settings → Meow Engine → Mixed proxy port, set `"mixed_port": PORT`
in Paw's `preferences.json`, or launch with `--mixed-port PORT` (temporary).
A Settings change takes effect on the next engine start. If the port is occupied, Paw reports the conflict without
stopping the listener or choosing a different port automatically.

To run the real managed-kernel test separately, set `MEOW_BIN` to the local
kernel executable and run:

```sh
cargo test --locked --test managed_meow -- --ignored
```

The owner-exit regressions start Paw's managed session with a synthetic
direct-only profile, terminate a test-owned supervisor, and verify that the
owned Meow exits and its mixed listener closes. The Windows test also checks
the controller listener. Both are opt-in and require a trusted local Meow
executable. The Linux test also requires pidfd support and
readable `/proc`. Run it with a private PID and loopback-only network namespace:

```sh
MEOW_BIN="$PWD/target/release/meow" \
  unshare -Urnpf --kill-child --mount-proc sh -c \
  'ip link set lo up && cargo test --offline --locked --test linux_owner_exit -- --ignored --exact owner_exit_stops_managed_core'
```

On Windows, point to a trusted native test fixture, not an executable
controlled by another application:

```powershell
$env:MEOW_BIN = 'C:\path\to\trusted\meow.exe'
cargo test --offline --locked --lib engine::session::windows::tests::windows_real_session_dies_after_abrupt_owner_exit -- --ignored --exact
```

The tests exercise process ownership, not the system proxy or interactive
desktop. A hard termination can still bypass normal temporary-directory
cleanup. On Linux Paw creates new session directories and generated
configuration with permissions no broader than `0700` and `0600`,
respectively.

## Language

The shell currently supports English and Simplified Chinese (`zh-CN`). Use
`--locale zh-CN` or `--locale en` to override the launch locale, for example
`meow-paw --locale zh-CN --page proxies`. The override is temporary and does
not change saved preferences. Page identifiers for `--page` remain English:
`home` (also `overview`), `proxies`, `profiles`, `connections`, `rules`,
`logs`, `unlock`, and `settings`.

Settings offers System, English, and 简体中文 as the saved language choice.
Changes take effect after restarting Paw, including the tray; the current launch
language is shown separately. A `--locale` override takes precedence over the
saved choice for that launch. When preferences are unavailable or disabled
(including `--no-preferences` and visual captures), language selection is disabled.

With System selected and no override, Linux uses the first nonempty `LC_ALL`,
`LC_MESSAGES`, or `LANG` value, in that order. Windows uses the user's regional locale. Locale
names such as `zh_CN.UTF-8` and `zh-Hans-CN` are recognized; missing or
unsupported values fall back to English. A present but unsupported override
also falls back to English. The shell relies on native font fallback for CJK
glyphs, so a suitable system font must be installed. Navigation, shell controls, and fixed labels on the desktop pages are
localized, including the traffic chart legend, profile states, and the Linux
tray menu. Engine messages, log contents, profile names, and proxy names
retain their original language. Some detailed status text remains in English.

## Linux tray verification

Paw stays running after the last window closes only while a compatible
StatusNotifier host is available. A normal tray activation reopens the window;
a compatible host can render the localized Show and Quit menu. Quit shuts
down the managed Meow process. The Linux tray does not change system proxy
settings.

For a disposable smoke test, use a fresh Xvfb display and new D-Bus session
inside a network namespace. With a built, trusted Meow binary at
`$PWD/target/release/meow`, the test exercises both the stopped and running cases:

```sh
cargo build --locked --features desktop --bin meow-paw
cargo build --locked --features visual-test --example tray_test_watcher
test_home=$(mktemp -d)
env -i PATH=/usr/bin:/bin LANG=C.UTF-8 HOME="$test_home" \
  MEOW_BIN="$PWD/target/release/meow" \
  DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent/paw-no-system-bus \
  timeout 100s unshare -Urn sh -c \
  'ip link set lo up && dbus-run-session -- xvfb-run -a python3 tools/tray-smoke.py'
```

This requires user/network namespaces, Xvfb, xdotool, libX11, D-Bus,
busctl, and the iproute2 tools. The smoke test checks registration, window
reopen, watcher-loss recovery, menu dispatch, and managed engine shutdown.
It does not establish that a real Waybar or Quickshell panel displays the
menu, that the window receives keyboard focus on Wayland, or that Fcitx5
composition works. Check those interactions separately on the target desktop.

## Local YAML import verification on X11

Use an already built, trusted Paw desktop binary. The tests create synthetic
files and a private Xvfb display. They refuse a namespace with interfaces other
than loopback and do not use your desktop's settings.

- `x11-file-picker-smoke.py` clicks **Profiles → Import YAML copies**. A private,
  activation-free D-Bus fixture returns cancel, a non-local URI, and then two
  local YAML files. Only the last adds managed copies, without modifying the
  source files or choosing a profile.
- `x11-xdnd-smoke.py` checks URI priority for mixed X11 drop offers, including
  both MIME offer orders, a four-type offer, text-only fallback and non-local
  rejection. Pass `--switch-profiles` before the binary to also use the real
  controls to choose each saved copy for the next start without launching Meow.
- `x11-profile-reload-smoke.py` takes trusted Paw and Meow binaries. It imports
  two managed copies and checks that importing preserves the current session
  and selection. Applying the copies live then yields DIRECT → REJECT → DIRECT
  against a local HTTP origin while keeping the same managed Meow process.

```sh
test_paw="$PWD/target/debug/meow-paw"
timeout 85s unshare -Urn sh -c 'ip link set lo up && python3 "$@"' sh \
  "$PWD/tools/x11-file-picker-smoke.py" "$test_paw"
timeout 85s unshare -Urn sh -c 'ip link set lo up && python3 "$@"' sh \
  "$PWD/tools/x11-xdnd-smoke.py" "$test_paw"
timeout 85s unshare -Urn sh -c 'ip link set lo up && python3 "$@"' sh \
  "$PWD/tools/x11-xdnd-smoke.py" --switch-profiles "$test_paw"
timeout 105s unshare -Urn sh -c 'ip link set lo up && python3 "$@"' sh \
  "$PWD/tools/x11-profile-reload-smoke.py" "$test_paw" /path/to/trusted/meow
```

Along with the namespace and X11 tools above, the fixtures need a C compiler,
libX11 headers, and, for the picker only, `pkg-config` and GIO headers. This does
not establish native Wayland, Windows, system-proxy, or TUN behavior.
