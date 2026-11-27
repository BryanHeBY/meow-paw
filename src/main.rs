use gpui_kit::{
    AnyWindowHandle, App, Application, Bounds, QuitMode, WindowBounds, WindowOptions, px, size,
};
use meow_paw::i18n::{Locale, Message, system_locale};
use std::path::PathBuf;
use std::{cell::Cell, rc::Rc, time::Duration};

mod brand;
mod preferences;
mod tray;
mod view;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut profile = None;
    let mut page = view::Page::Overview;
    let mut start = false;
    let mut remember = true;
    let mut use_tray = true;
    let mut locale_override = None;
    let mut mixed_port = None;
    #[cfg(feature = "visual-test")]
    let mut screenshot = None;
    #[cfg(feature = "visual-test")]
    let mut proxy_fixture = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--profile") => profile = args.next().map(PathBuf::from),
            Some("--page") => {
                if let Some(name) = args.next() {
                    page = view::Page::from_name(&name.to_string_lossy()).unwrap_or(page);
                }
            }
            Some("--locale") => {
                let value = args.next().filter(|value| {
                    !value.is_empty() && !value.to_string_lossy().starts_with("--")
                });
                let Some(value) = value else {
                    eprintln!("--locale requires a locale value");
                    std::process::exit(2);
                };
                locale_override = Some(value.to_string_lossy().into_owned());
            }
            Some("--mixed-port") => {
                mixed_port = args
                    .next()
                    .and_then(|value| value.to_str()?.parse::<std::num::NonZeroU16>().ok());
                if mixed_port.is_none() {
                    eprintln!("--mixed-port requires a port from 1 to 65535");
                    std::process::exit(2);
                }
            }
            Some("--start") => start = true,
            Some("--no-tray") => use_tray = false,
            Some("--no-preferences") => remember = false,
            #[cfg(feature = "visual-test")]
            Some("--screenshot") => screenshot = args.next().map(PathBuf::from),
            #[cfg(feature = "visual-test")]
            Some("--fixture-proxies") => proxy_fixture = true,
            _ => {
                eprintln!(
                    "Usage: meow-paw [--profile FILE] [--mixed-port PORT] [--start] [--page PAGE] [--locale LOCALE] [--no-preferences] [--no-tray]"
                );
                std::process::exit(2);
            }
        }
    }
    let binary = std::env::var_os("MEOW_BIN").map(PathBuf::from);
    #[cfg(feature = "visual-test")]
    let capturing = screenshot.is_some();
    #[cfg(not(feature = "visual-test"))]
    let capturing = false;
    #[cfg(feature = "visual-test")]
    if proxy_fixture
        && (!capturing
            || page != view::Page::Proxies
            || binary.is_some()
            || start
            || profile.is_some())
    {
        eprintln!(
            "--fixture-proxies requires --page proxies --screenshot FILE and no engine, profile, or --start"
        );
        std::process::exit(2);
    }
    #[cfg(not(feature = "visual-test"))]
    let proxy_fixture = false;
    if capturing {
        use_tray = false;
    }
    let (mut prefs, warning) =
        preferences::Preferences::discover(preferences_enabled(remember, capturing));
    prefs.set_mixed_port_override(mixed_port);
    let locale = select_locale(
        &prefs,
        locale_override.as_deref(),
        system_locale().as_deref(),
    );
    let (binary, profile) = prefs.resolve(binary, profile);
    let binary = if proxy_fixture {
        None
    } else {
        binary.or_else(bundled_engine)
    };
    let app: Application = gpui_kit::application().with_assets(brand::PawAssets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let (_handle, paw) = gpui_kit::open_window(window_options(cx), cx, move |window, cx| {
            window.set_window_title(locale.message(Message::AppTitle));
            let paw = view::open(
                binary,
                profile,
                (page, locale),
                start,
                (prefs, warning),
                window,
                cx,
            );
            #[cfg(feature = "visual-test")]
            if proxy_fixture {
                view::seed_proxy_fixture(&paw, cx);
            }
            paw
        })
        .expect("could not open Meow Paw");
        cx.activate(true);
        if use_tray {
            retain_when_tray_is_ready(_handle, paw, locale, cx);
        }
        #[cfg(feature = "visual-test")]
        if let Some(path) = screenshot {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1_400))
                    .await;
                let rendered = _handle.update(cx, |_, window, _| {
                    window
                        .render_to_image()
                        .and_then(|image| image.save(&path).map_err(Into::into))
                });
                if !matches!(rendered, Ok(Ok(()))) {
                    eprintln!("Paw visual capture failed: {rendered:?}");
                    std::process::exit(1);
                }
                cx.update(|app| app.quit());
            })
            .detach();
        }
    });
}

fn preferences_enabled(remember: bool, capturing: bool) -> bool {
    remember && !capturing
}

fn select_locale(
    prefs: &preferences::Preferences,
    cli: Option<&str>,
    system: Option<&str>,
) -> Locale {
    Locale::select(cli.or(prefs.language().locale_name()), system)
}

fn bundled_engine() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    engine_beside(&exe)
}

fn engine_beside(exe: &std::path::Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "meow.exe" } else { "meow" };
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

fn window_options(cx: &mut App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1120.), px(738.)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(640.), px(480.))),
        ..Default::default()
    }
}

fn retain_when_tray_is_ready(
    initial: AnyWindowHandle,
    paw: gpui_kit::Entity<view::Paw>,
    locale: Locale,
    cx: &mut App,
) {
    let signals = tray::start(locale);
    let visible = Rc::new(Cell::new(Some(initial)));
    let closed = visible.clone();
    let observer = cx.on_window_closed(move |_, id| {
        if closed.get().is_some_and(|handle| handle.window_id() == id) {
            closed.set(None);
        }
    });
    let mut retained = false;
    cx.spawn(async move |cx| {
        let _observer = observer;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(160))
                .await;
            cx.update(|cx| {
                let quit = signals.take_quit();
                let available = signals.ready();
                let show = (signals.take_show() && available) || (quit && visible.get().is_none());
                let restore = retained && !available && visible.get().is_none();
                if available && !retained {
                    cx.set_quit_mode(QuitMode::Explicit);
                }
                if show || restore {
                    let activated = visible.get().is_some_and(|handle| {
                        handle
                            .update(cx, |_, window, _| window.activate_window())
                            .is_ok()
                    });
                    if !activated {
                        let result = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
                            window.set_window_title(locale.message(Message::AppTitle));
                            view::rebind_window(&paw, window, cx);
                            paw.clone()
                        });
                        if let Ok((handle, _)) = result {
                            visible.set(Some(handle));
                            cx.activate(true);
                        } else if restore {
                            // No tray and no window means no remaining control.
                            cx.quit();
                        }
                    }
                }
                if retained && !available {
                    cx.set_quit_mode(QuitMode::Default);
                }
                retained = available;
                if quit {
                    paw.update(cx, |view, cx| view.request_quit(cx));
                }
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod bundled_tests {
    use super::*;
    use preferences::LanguagePreference;

    #[test]
    fn launch_language_uses_cli_then_saved_then_system() {
        let mut prefs = preferences::Preferences::default();
        assert_eq!(select_locale(&prefs, None, Some("zh-CN")), Locale::ZhCn);
        prefs.set_language(LanguagePreference::En).unwrap();
        assert_eq!(select_locale(&prefs, None, Some("zh-CN")), Locale::En);
        assert_eq!(
            select_locale(&prefs, Some("zh-CN"), Some("en")),
            Locale::ZhCn
        );
        assert_eq!(
            select_locale(&prefs, Some("unsupported"), Some("zh-CN")),
            Locale::En
        );
        assert_eq!(prefs.language(), LanguagePreference::En);
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        assert_eq!(select_locale(&prefs, None, Some("en")), Locale::ZhCn);
        assert_eq!(select_locale(&prefs, Some("en"), Some("zh-CN")), Locale::En);
        prefs.set_language(LanguagePreference::System).unwrap();
        assert_eq!(select_locale(&prefs, None, Some("en")), Locale::En);
        assert_eq!(select_locale(&prefs, None, None), Locale::En);
    }

    #[test]
    fn capture_and_no_preferences_disable_storage() {
        for (remember, capturing) in [(false, false), (false, true), (true, true)] {
            assert!(!preferences_enabled(remember, capturing));
            let (mut prefs, _) =
                preferences::Preferences::discover(preferences_enabled(remember, capturing));
            assert_eq!(prefs.language(), LanguagePreference::System);
            prefs.set_language(LanguagePreference::En).unwrap();
            assert_eq!(
                preferences::Preferences::discover(false).0.language(),
                LanguagePreference::System
            );
        }
        assert!(preferences_enabled(true, false));
    }

    #[test]
    fn only_looks_beside_the_paw_executable_for_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let paw = dir.path().join("meow-paw");
        assert!(engine_beside(&paw).is_none());
        let name = if cfg!(windows) { "meow.exe" } else { "meow" };
        let core = dir.path().join(name);
        std::fs::write(&core, "test file").unwrap();
        assert_eq!(engine_beside(&paw), Some(core));
    }
}
