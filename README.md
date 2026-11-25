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
