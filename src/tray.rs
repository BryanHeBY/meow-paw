//! Only a confirmed native reopen control is allowed to hold the app open.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub struct Signals {
    ready: AtomicBool,
    show: AtomicBool,
}

impl Signals {
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn take_show(&self) -> bool {
        self.show.swap(false, Ordering::AcqRel)
    }
}

pub fn start() -> Arc<Signals> {
    let signals = Arc::new(Signals::default());
    #[cfg(target_os = "linux")]
    linux::start(signals.clone());
    #[cfg(windows)]
    windows::start(signals.clone());
    signals
}

#[cfg(any(target_os = "linux", windows))]
fn paw_pixel(x: i32, y: i32, size: i32) -> [u8; 4] {
    let (u, v) = (
        (x as f32 + 0.5) / size as f32,
        (y as f32 + 0.5) / size as f32,
    );
    let paw = [
        (0.24, 0.43, 0.067, 0.080),
        (0.40, 0.31, 0.074, 0.095),
        (0.60, 0.31, 0.074, 0.095),
        (0.76, 0.43, 0.067, 0.080),
        (0.50, 0.63, 0.192, 0.143),
    ]
    .iter()
    .any(|(cx, cy, rx, ry)| ((u - cx) / rx).powi(2) + ((v - cy) / ry).powi(2) <= 1.0);
    if paw {
        [255, 16, 36, 61]
    } else if (u - 0.5).powi(2) + (v - 0.5).powi(2) < 0.219 {
        [255, 255, 221, 120]
    } else {
        [0; 4]
    }
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::time::Duration;
    use zbus::{
        blocking::{Proxy, connection::Builder},
        zvariant::OwnedValue,
    };

    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    const WATCHER_PATH: &str = "/StatusNotifierWatcher";
    const ITEM_PATH: &str = "/StatusNotifierItem";

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

        fn context_menu(&self, x: i32, y: i32) {
            self.activate(x, y);
        }
    }

    pub(super) fn start(signals: Arc<Signals>) {
        let _ = std::thread::Builder::new()
            .name("paw-status-notifier".into())
            .spawn(move || {
                let _ = run(&signals);
                signals.ready.store(false, Ordering::Release);
            });
    }

    fn run(signals: &Arc<Signals>) -> zbus::Result<()> {
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let bus = Builder::session()?
            .method_timeout(Duration::from_millis(800))
            .name(name.as_str())?
            .serve_at(ITEM_PATH, Item(signals.clone()))?
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
