use super::{Snapshot, change};
use crate::system_proxy::{Consent, ProxyError, Status, SystemProxy};
use std::{num::NonZeroU16, path::Path, sync::Mutex};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProxyState {
    #[default]
    Checking,
    Unavailable,
    Off,
    On(NonZeroU16),
    Recovery,
    Blocked,
}
pub(super) enum ProxyAction {
    Enable(Consent),
    Restore,
    Refresh,
}
trait Manager {
    fn owns_operation(&self) -> bool;
    fn status(&self) -> Result<Status, ProxyError>;
    fn activate(&self, port: NonZeroU16, consent: Consent) -> Result<(), ProxyError>;
    fn restore(&self) -> Result<(), ProxyError>;
}
impl Manager for SystemProxy {
    fn owns_operation(&self) -> bool {
        self.owns_operation()
    }
    fn status(&self) -> Result<Status, ProxyError> {
        self.status()
    }
    fn activate(&self, port: NonZeroU16, consent: Consent) -> Result<(), ProxyError> {
        self.activate(port, consent)
    }
    fn restore(&self) -> Result<(), ProxyError> {
        self.restore()
    }
}
pub(super) struct ProxyControl {
    manager: Result<Box<dyn Manager>, ProxyError>,
}
impl ProxyControl {
    pub(super) fn discover(dir: Option<&Path>) -> Self {
        Self {
            manager: dir
                .ok_or(ProxyError::Unsupported)
                .and_then(SystemProxy::discover)
                .map(|manager| Box::new(manager) as Box<dyn Manager>),
        }
    }
    pub(super) fn publish(&self, snapshot: &Mutex<Snapshot>, action_error: Option<String>) {
        let (state, status_error) = match self.manager.as_ref().map_err(|e| e.to_string()) {
            Err(e) => (ProxyState::Unavailable, Some(e)),
            Ok(manager) => match manager.status() {
                Ok(Status::Off) => (ProxyState::Off, None),
                Ok(Status::Owned(port)) if manager.owns_operation() => (ProxyState::On(port), None),
                Ok(Status::RecoveryPending | Status::Owned(_) | Status::InterruptedOrChanged) => {
                    (ProxyState::Recovery, None)
                }
                Err(e) => (ProxyState::Blocked, Some(e.to_string())),
            },
        };
        change(snapshot, |snap| {
            snap.system_proxy = state;
            snap.system_proxy_error = action_error.or(status_error);
        });
    }
    pub(super) fn handle(
        &self,
        action: ProxyAction,
        snapshot: &Mutex<Snapshot>,
        mut ready: impl FnMut() -> Result<NonZeroU16, String>,
    ) {
        let outcome = (|| {
            let manager = self.manager.as_ref().map_err(ToString::to_string)?;
            match action {
                ProxyAction::Enable(consent) => {
                    if consent != Consent::GrantedByUser {
                        return Err(ProxyError::ConsentRequired.to_string());
                    }
                    match manager.status().map_err(|e| e.to_string())? {
                        Status::Off => {}
                        Status::Owned(_) if manager.owns_operation() => return Ok(()),
                        _ => return Err(ProxyError::Conflict.to_string()),
                    }
                    let port = ready()?;
                    if let Err(error) = manager.activate(port, consent) {
                        if manager.owns_operation()
                            && let Err(restore) = manager.restore()
                        {
                            return Err(format!("{error} {restore}"));
                        }
                        return Err(error.to_string());
                    }
                    let after = ready();
                    if after.as_ref().ok() != Some(&port) {
                        let reason = after
                            .err()
                            .unwrap_or_else(|| "Paw's mixed listener changed.".into());
                        manager.restore().map_err(|e| format!("{reason} {e}"))?;
                        return Err(format!("{reason} Paw restored the desktop proxy."));
                    }
                    Ok(())
                }
                ProxyAction::Restore => manager.restore(),
                ProxyAction::Refresh => Ok(()),
            }
            .map_err(|e| e.to_string())
        })();
        self.publish(snapshot, outcome.err());
    }
    /// Normal stop/quit only restores this worker's operation. Merely finding
    /// another (including post-crash) journal is never permission to write.
    pub(super) fn restore_current(&self, snapshot: &Mutex<Snapshot>) -> bool {
        let Ok(manager) = &self.manager else {
            return true;
        };
        if !manager.owns_operation() {
            return true;
        }
        let err = manager.restore().err().map(|e| e.to_string());
        let ok = err.is_none();
        self.publish(snapshot, err);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    const LIVE_PORT: NonZeroU16 = NonZeroU16::new(18790).unwrap();

    struct Fake {
        status: Cell<Status>,
        owns: Cell<bool>,
        activates: Cell<usize>,
        restores: Cell<usize>,
    }
    #[derive(Clone)]
    struct RefFake(Rc<Fake>);
    impl Manager for RefFake {
        fn owns_operation(&self) -> bool {
            self.0.owns.get()
        }
        fn status(&self) -> Result<Status, ProxyError> {
            Ok(self.0.status.get())
        }
        fn activate(&self, port: NonZeroU16, _: Consent) -> Result<(), ProxyError> {
            self.0.activates.set(self.0.activates.get() + 1);
            self.0.status.set(Status::Owned(port));
            self.0.owns.set(true);
            Ok(())
        }
        fn restore(&self) -> Result<(), ProxyError> {
            self.0.restores.set(self.0.restores.get() + 1);
            if self.0.status.get() == Status::InterruptedOrChanged {
                return Err(ProxyError::Conflict);
            }
            self.0.status.set(Status::Off);
            self.0.owns.set(false);
            Ok(())
        }
    }
    fn fixture(state: Status) -> (ProxyControl, Rc<Fake>, Mutex<Snapshot>) {
        let fake = Rc::new(Fake {
            status: Cell::new(state),
            owns: Cell::new(false),
            activates: Cell::new(0),
            restores: Cell::new(0),
        });
        (
            ProxyControl {
                manager: Ok(Box::new(RefFake(Rc::clone(&fake)))),
            },
            fake,
            Mutex::new(Snapshot::default()),
        )
    }
    #[test]
    fn enable_accepts_only_explicit_user_and_actual_ready_owned_port() {
        let (worker, fake, snap) = fixture(Status::Off);
        worker.publish(&snap, None);
        worker.handle(ProxyAction::Enable(Consent::NotGranted), &snap, || {
            panic!("must not poll listener")
        });
        worker.handle(ProxyAction::Enable(Consent::GrantedByUser), &snap, || {
            Err("not owned".into())
        });
        assert_eq!(fake.activates.get(), 0);
        assert!(snap.lock().unwrap().system_proxy_error.is_some());
        worker.handle(ProxyAction::Enable(Consent::GrantedByUser), &snap, || {
            Ok(LIVE_PORT)
        });
        assert_eq!(fake.activates.get(), 1);
        assert_eq!(snap.lock().unwrap().system_proxy, ProxyState::On(LIVE_PORT));
        assert!(worker.restore_current(&snap));
        assert_eq!(fake.restores.get(), 1);
        assert_eq!(snap.lock().unwrap().system_proxy, ProxyState::Off);
    }
    #[test]
    fn listener_exit_during_apply_restores_before_reporting_error() {
        let (worker, fake, snap) = fixture(Status::Off);
        let mut first = true;
        worker.handle(ProxyAction::Enable(Consent::GrantedByUser), &snap, || {
            std::mem::replace(&mut first, false)
                .then_some(LIVE_PORT)
                .ok_or_else(|| "listener stopped".into())
        });
        assert_eq!(fake.activates.get(), 1);
        assert_eq!(fake.restores.get(), 1);
        let state = snap.lock().unwrap();
        assert_eq!(state.system_proxy, ProxyState::Off);
        assert!(state.system_proxy_error.is_some());
    }

    #[test]
    fn startup_and_exit_do_not_recover_another_session_but_user_can() {
        let (worker, fake, snap) = fixture(Status::Owned(LIVE_PORT));
        worker.publish(&snap, None);
        assert_eq!(snap.lock().unwrap().system_proxy, ProxyState::Recovery);
        assert!(worker.restore_current(&snap));
        assert_eq!(fake.restores.get(), 0);
        worker.handle(ProxyAction::Restore, &snap, || {
            panic!("recovery needs no running core")
        });
        assert_eq!(fake.restores.get(), 1);
        assert_eq!(snap.lock().unwrap().system_proxy, ProxyState::Off);
    }
    #[test]
    fn conflicts_are_visible_and_fail_the_clean_shutdown_gate() {
        let (worker, fake, snap) = fixture(Status::Off);
        worker.handle(ProxyAction::Enable(Consent::GrantedByUser), &snap, || {
            Ok(LIVE_PORT)
        });
        fake.status.set(Status::InterruptedOrChanged);
        assert!(!worker.restore_current(&snap));
        let snap = snap.lock().unwrap();
        assert_eq!(snap.system_proxy, ProxyState::Recovery);
        assert!(snap.system_proxy_error.is_some());
    }
}
