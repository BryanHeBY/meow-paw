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
To run the real managed-kernel test separately, set `MEOW_BIN` to the local
kernel executable and run:

```sh
cargo test --locked --test managed_meow -- --ignored
```
