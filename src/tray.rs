//! Only a confirmed native reopen control is allowed to hold the app open.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub struct Signals {
    ready: AtomicBool,
    show: AtomicBool,
    quit: AtomicBool,
}

impl Signals {
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn take_quit(&self) -> bool {
        self.quit.swap(false, Ordering::AcqRel)
    }

    pub fn take_show(&self) -> bool {
        self.show.swap(false, Ordering::AcqRel)
    }
}

pub fn start(locale: meow_paw::i18n::Locale) -> Arc<Signals> {
    let signals = Arc::new(Signals::default());
    #[cfg(target_os = "linux")]
    linux::start(signals.clone(), locale);
    #[cfg(windows)]
    windows::start(signals.clone());
    #[cfg(not(target_os = "linux"))]
    let _ = locale;
    signals
}

#[cfg(any(target_os = "linux", windows))]
use crate::brand::tray_pixel as paw_pixel;

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use meow_paw::i18n::Locale;
    use std::time::Duration;
    mod menu;
    use zbus::{
        blocking::{Proxy, connection::Builder},
        zvariant::OwnedValue,
    };

    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    const WATCHER_PATH: &str = "/StatusNotifierWatcher";
    const ITEM_PATH: &str = "/StatusNotifierItem";
    const MENU_PATH: &str = "/MenuBar";

    struct Item(Arc<Signals>);

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Item {
        #[zbus(property)]
        fn category(&self) -> &'static str {
            "ApplicationStatus"
        }

        #[zbus(property)]
        fn id(&self) -> &'static str {
            "meow-paw"
        }

        #[zbus(property)]
        fn title(&self) -> &'static str {
            "Meow Paw"
        }

        #[zbus(property)]
        fn status(&self) -> &'static str {
            "Active"
        }

        #[zbus(property)]
        fn item_is_menu(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn menu(&self) -> zbus::zvariant::ObjectPath<'static> {
            zbus::zvariant::ObjectPath::try_from(MENU_PATH).unwrap()
        }

        #[zbus(property)]
        fn icon_name(&self) -> &'static str {
            ""
        }

        #[zbus(property)]
        fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
            [16_i32, 24, 32]
                .into_iter()
                .map(|size| {
                    let mut bytes = Vec::new();
                    for y in 0..size {
                        for x in 0..size {
                            let color = paw_pixel(x, y, size);
                            bytes.extend(color);
                        }
                    }
                    (size, size, bytes)
                })
                .collect()
        }

        fn activate(&self, _x: i32, _y: i32) {
            self.0.show.store(true, Ordering::Release);
        }

        fn secondary_activate(&self, x: i32, y: i32) {
            self.activate(x, y);
        }

        fn context_menu(&self, _x: i32, _y: i32) {}
    }

    pub(super) fn start(signals: Arc<Signals>, locale: Locale) {
        let _ = std::thread::Builder::new()
            .name("paw-status-notifier".into())
            .spawn(move || {
                let _ = run(&signals, locale);
                signals.ready.store(false, Ordering::Release);
            });
    }

    fn run(signals: &Arc<Signals>, locale: Locale) -> zbus::Result<()> {
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let bus = Builder::session()?
            .method_timeout(Duration::from_millis(800))
            .name(name.as_str())?
            .serve_at(ITEM_PATH, Item(signals.clone()))?
            .serve_at(MENU_PATH, menu::Menu::new(signals.clone(), locale))?
            .build()?;
        let dbus = Proxy::new(
            &bus,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )?;
        let mut previous = None;
        loop {
            // Query by owner to detect host/watcher disappearance or restart,
            // without a stale proxy property cache. Re-register after a restart.
            let owner = dbus.call::<_, _, String>("GetNameOwner", &(WATCHER,)).ok();
            if owner != previous {
                signals.ready.store(false, Ordering::Release);
                previous = match owner {
                    Some(owner) => {
                        let watcher = Proxy::new(&bus, owner.as_str(), WATCHER_PATH, WATCHER)?;
                        watcher
                            .call::<_, _, ()>("RegisterStatusNotifierItem", &(name.as_str(),))
                            .ok()
                            .map(|_| owner.clone())
                    }
                    None => None,
                };
            }
            let host = previous
                .as_deref()
                .and_then(|owner| {
                    let props =
                        Proxy::new(&bus, owner, WATCHER_PATH, "org.freedesktop.DBus.Properties")
                            .ok()?;
                    let value: OwnedValue = props
                        .call("Get", &(WATCHER, "IsStatusNotifierHostRegistered"))
                        .ok()?;
                    bool::try_from(value).ok()
                })
                .unwrap_or(false);
            signals.ready.store(host, Ordering::Release);
            std::thread::sleep(Duration::from_millis(800));
        }
    }
}
