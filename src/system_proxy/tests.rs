use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone)]
struct Fake {
    state: Rc<RefCell<Snapshot>>,
    writes: Rc<Cell<usize>>,
    interrupt: Rc<Cell<bool>>,
}
impl Fake {
    fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(initial())),
            writes: Rc::default(),
            interrupt: Rc::default(),
        }
    }
}
fn initial() -> Snapshot {
    Snapshot(vec![None, Some("preexisting value".into())])
}
impl Backend for Fake {
    fn id(&self) -> &'static str {
        "fake-v1"
    }
    fn read(&self) -> Result<Snapshot, ProxyError> {
        Ok(self.state.borrow().clone())
    }
    fn desired(&self, before: &Snapshot, port: NonZeroU16) -> Result<Snapshot, ProxyError> {
        let mut after = before.clone();
        after.0[0] = Some(format!("127.0.0.1:{port}"));
        Ok(after)
    }
    fn apply(&self, from: &Snapshot, to: &Snapshot) -> Result<(), ProxyError> {
        if self.read()? != *from {
            return Err(ProxyError::Conflict);
        }
        self.writes.set(self.writes.get() + 1);
        if self.interrupt.get() {
            self.state.borrow_mut().0[0] = Some("interrupted-write".into());
            return Err(ProxyError::Conflict);
        }
        *self.state.borrow_mut() = to.clone();
        Ok(())
    }
}
fn fixture() -> (tempfile::TempDir, Fake, SystemProxy) {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let proxy = instance(&dir, &fake);
    (dir, fake, proxy)
}
fn instance(dir: &tempfile::TempDir, fake: &Fake) -> SystemProxy {
    SystemProxy {
        dir: dir.path().join("proxy"),
        backend: Box::new(fake.clone()),
        owner: RefCell::new(None),
    }
}
fn port() -> NonZeroU16 {
    NonZeroU16::new(7890).unwrap()
}

#[test]
fn no_consent_means_no_system_write_or_journal() {
    let (_dir, f, p) = fixture();
    assert!(matches!(
        p.activate(port(), Consent::NotGranted),
        Err(ProxyError::ConsentRequired)
    ));
    assert!(!p.dir.exists());
    assert_eq!(f.writes.get(), 0);
    assert_eq!(p.status().unwrap(), Status::Off);
}

#[test]
fn explicit_enable_persists_then_restart_can_restore_exact_original() {
    let (dir, f, p) = fixture();
    p.activate(port(), Consent::GrantedByUser).unwrap();
    assert!(p.dir.join("recovery.json").is_file());
    assert_eq!(p.status().unwrap(), Status::Owned(port()));
    assert_eq!(f.writes.get(), 1);
    assert!(matches!(
        instance(&dir, &f).restore(),
        Err(ProxyError::Busy)
    ));
    drop(p);
    let restarted = instance(&dir, &f);
    assert_eq!(restarted.status().unwrap(), Status::Owned(port()));
    restarted.restore().unwrap();
    assert_eq!(f.read().unwrap(), initial());
    assert!(!restarted.dir.join("recovery.json").exists());
    assert_eq!(f.writes.get(), 2);
}

#[test]
fn out_of_band_edit_blocks_restore_and_reenable() {
    let (_dir, f, p) = fixture();
    p.activate(port(), Consent::GrantedByUser).unwrap();
    f.state.borrow_mut().0[1] = Some("manual change".into());
    let after = f.read().unwrap();
    assert_eq!(p.status().unwrap(), Status::InterruptedOrChanged);
    assert!(matches!(p.restore(), Err(ProxyError::Conflict)));
    assert!(matches!(
        p.activate(port(), Consent::GrantedByUser),
        Err(ProxyError::Conflict)
    ));
    assert!(p.dir.join("recovery.json").is_file());
    assert_eq!(f.read().unwrap(), after);
    assert_eq!(f.writes.get(), 1);
}

#[test]
fn interrupted_partial_apply_retains_journal_and_does_not_overwrite() {
    let (dir, f, p) = fixture();
    f.interrupt.set(true);
    assert!(matches!(
        p.activate(port(), Consent::GrantedByUser),
        Err(ProxyError::Conflict)
    ));
    let after = f.read().unwrap();
    drop(p);
    let p = instance(&dir, &f);
    assert!(matches!(p.restore(), Err(ProxyError::Conflict)));
    assert_eq!(f.read().unwrap(), after);
    assert_eq!(f.writes.get(), 1);
    assert!(p.dir.join("recovery.json").is_file());
}

#[test]
fn crash_before_first_write_clears_journal_only() {
    let (_dir, f, p) = fixture();
    let lease = p.lock().unwrap();
    let original = f.read().unwrap();
    let expected = f.desired(&original, port()).unwrap();
    p.save(&Journal {
        version: 1,
        backend: f.id().into(),
        port: port(),
        original,
        expected,
    })
    .unwrap();
    drop(lease);
    p.restore().unwrap();
    assert!(!p.dir.join("recovery.json").exists());
    assert_eq!(f.writes.get(), 0);
}

#[test]
fn already_equal_foreign_settings_are_not_adopted() {
    let (_dir, f, p) = fixture();
    *f.state.borrow_mut() = f.desired(&initial(), port()).unwrap();
    assert!(matches!(
        p.activate(port(), Consent::GrantedByUser),
        Err(ProxyError::Conflict)
    ));
    assert_eq!(f.writes.get(), 0);
    assert!(!p.dir.join("recovery.json").exists());
}

#[test]
fn parallel_instance_cannot_enter_and_malformed_journal_is_never_used() {
    let (dir, f, p) = fixture();
    let lease = p.lock().unwrap();
    assert!(matches!(instance(&dir, &f).status(), Err(ProxyError::Busy)));
    fs::write(p.dir.join("recovery.json"), b"not a journal").unwrap();
    drop(lease);
    assert!(matches!(p.restore(), Err(ProxyError::InvalidJournal)));
    assert_eq!(f.writes.get(), 0);
}

#[test]
fn corrupt_expected_state_is_rejected_even_when_it_matches_the_live_state() {
    let (_dir, f, p) = fixture();
    let lease = p.lock().unwrap();
    let original = f.read().unwrap();
    let expected = Snapshot(vec![Some("unrelated endpoint".into()), None]);
    *f.state.borrow_mut() = expected.clone();
    p.save(&Journal {
        version: 1,
        backend: f.id().into(),
        port: port(),
        original,
        expected,
    })
    .unwrap();
    drop(lease);
    assert!(matches!(p.restore(), Err(ProxyError::InvalidJournal)));
    assert_eq!(f.writes.get(), 0);
}
