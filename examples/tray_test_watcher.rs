//! An isolated StatusNotifierWatcher for the native window lifecycle smoke test.
#[cfg(target_os = "linux")]
fn main() -> zbus::Result<()> {
    use std::sync::Mutex;
    struct Watcher {
        item: Mutex<String>,
    }

    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, service: String) {
            *self.item.lock().expect("item lock") = service;
        }

        #[zbus(property)]
        fn registered_status_notifier_items(&self) -> Vec<String> {
            let item = self.item.lock().expect("item lock");
            if item.is_empty() {
                Vec::new()
            } else {
                vec![item.clone()]
            }
        }

        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            true
        }
    }

    let _connection = zbus::blocking::connection::Builder::session()?
        .name("org.kde.StatusNotifierWatcher")?
        .serve_at(
            "/StatusNotifierWatcher",
            Watcher {
                item: Mutex::new(String::new()),
            },
        )?
        .build()?;
    loop {
        std::thread::park();
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
