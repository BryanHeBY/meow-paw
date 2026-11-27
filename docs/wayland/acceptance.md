# Isolated Wayland tray and focus probe

This opt-in probe runs the stopped Paw application with a synthetic DIRECT-only
profile, a test StatusNotifierWatcher, headless Sway and Fcitx5. It uses a fresh
user and network namespace, private D-Bus session and temporary HOME, XDG and
runtime directories. It does not use the logged-in desktop or start Meow.
The network namespace is left without configured interfaces; no host network
configuration is changed.

First check host prerequisites without starting a desktop:

```sh
python3 tools/wayland/acceptance.py --check
```

The probe requires Sway with the wlroots headless and pixman backends, swaymsg,
Fcitx5 and fcitx5-remote, D-Bus, busctl, unshare, and working unprivileged
user and network namespaces. Executable discovery alone does not establish
that those backends, namespaces or the graphical application will run. Tool
discovery and execution use `/usr/bin:/bin`, independent of the caller's PATH.

Set `CARGO_TARGET_DIR` to an approved build cache. Build the application and
existing test watcher there, then explicitly start the probe:

```sh
CARGO_BUILD_JOBS=1 cargo build --locked --features visual-test --bin meow-paw --example tray_test_watcher
python3 tools/wayland/acceptance.py --run \
  --app "$CARGO_TARGET_DIR/debug/meow-paw" \
  --watcher "$CARGO_TARGET_DIR/debug/examples/tray_test_watcher"
```

A pass means the private watcher registered Paw's exact SNI service, the
initial Paw window closed while Paw stayed alive, SNI activation reopened one
window, repeated activation did not create a duplicate, and the reopened
Paw-owned window received Sway focus. Fcitx5 service readiness is checked,
but **actual Fcitx5 preedit, commit, and text entry are not tested**. A pass
is not evidence of composition working in Paw, of behavior in a real panel or
other compositor, or of the running managed-engine case. Missing dependencies,
failed setup and timeouts are failures, not acceptance passes. Xvfb is not a
Wayland or IME substitute.

Run the deterministic offline checks separately:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/wayland -v
```

They test the diagnostic, isolation fixture and SNI/focus decision logic using
synthetic inputs; they do not start or validate a Wayland compositor or IME.
