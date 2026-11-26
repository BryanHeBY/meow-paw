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
To run the real managed-kernel test separately, set `MEOW_BIN` to the local
kernel executable and run:

```sh
cargo test --locked --test managed_meow -- --ignored
```

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
glyphs, so a suitable system font must be installed. Navigation, shell controls,
and fixed labels and descriptions on Home, Proxies, Settings, Rules, and Logs are
localized. Live statuses, engine-provided content, and other page content remain
in English or their original language.
