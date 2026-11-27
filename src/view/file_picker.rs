//! File chooser lifetimes are scoped to one view and page visit.
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Engine,
    Profile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Request(u64, Kind);

#[derive(Default)]
pub(super) struct Picker {
    revision: u64,
    pending: Option<Request>,
    failure: Option<(Kind, Failure)>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) enum Failure {
    Backend,
    NoLocalFile,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Completion {
    Cancelled,
    Failed(Failure),
    Selected(PathBuf),
}

impl Completion {
    pub(super) fn from_reply<Inner, Outer>(
        reply: Result<Result<Option<Vec<PathBuf>>, Inner>, Outer>,
    ) -> Self {
        match reply {
            Ok(Ok(None)) => Self::Cancelled,
            Ok(Ok(Some(paths))) => paths
                .into_iter()
                .next()
                .map_or(Self::Failed(Failure::NoLocalFile), Self::Selected),
            Ok(Err(_)) | Err(_) => Self::Failed(Failure::Backend),
        }
    }
}

impl Picker {
    pub(super) fn begin(&mut self, kind: Kind) -> Option<Request> {
        if self.pending.is_some() {
            return None;
        }
        self.revision = self.revision.wrapping_add(1);
        let request = Request(self.revision, kind);
        self.pending = Some(request);
        self.failure = None;
        Some(request)
    }

    pub(super) fn leave_page(&mut self) {
        self.pending = None;
        self.failure = None;
    }

    pub(super) fn finish(
        &mut self,
        request: Request,
        completion: Completion,
    ) -> Option<Completion> {
        if self.pending != Some(request) {
            return None;
        }
        self.pending = None;
        self.failure = match completion {
            Completion::Failed(failure) => Some((request.1, failure)),
            _ => None,
        };
        Some(completion)
    }

    pub(super) fn failure(&self, kind: Kind) -> Option<Failure> {
        self.failure
            .filter(|&(owner, _)| owner == kind)
            .map(|(_, failure)| failure)
    }

    pub(super) fn waiting(&self, kind: Kind) -> bool {
        self.pending.is_some_and(|request| request.1 == kind)
    }

    pub(super) fn cancel_wait(&mut self, kind: Kind) -> bool {
        if !self.waiting(kind) {
            return false;
        }
        self.pending = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooser_distinguishes_selection_cancel_and_both_failure_layers() {
        let decode = Completion::from_reply::<&str, &str>;
        assert_eq!(decode(Ok(Ok(None))), Completion::Cancelled);
        assert_eq!(
            decode(Ok(Ok(Some(vec![])))),
            Completion::Failed(Failure::NoLocalFile)
        );
        assert_eq!(
            decode(Ok(Err("backend error"))),
            Completion::Failed(Failure::Backend)
        );
        assert_eq!(
            decode(Err("sender gone")),
            Completion::Failed(Failure::Backend)
        );
        assert_eq!(
            decode(Ok(Ok(Some(vec![PathBuf::from("fixture.yaml")])))),
            Completion::Selected(PathBuf::from("fixture.yaml"))
        );
    }

    #[test]
    fn chooser_keeps_cancel_quiet_and_quarantines_stale_results() {
        for kind in [Kind::Engine, Kind::Profile] {
            let mut picker = Picker::default();
            let old = picker.begin(kind).unwrap();
            assert!(picker.begin(kind).is_none());
            assert!(picker.waiting(kind));
            assert!(picker.cancel_wait(kind));
            assert!(!picker.waiting(kind));
            let abandoned = picker.begin(kind).unwrap();
            picker.leave_page();
            let active = picker.begin(kind).unwrap();
            assert_eq!(
                picker.finish(abandoned, Completion::Selected("stale-nav.yaml".into())),
                None
            );
            assert_eq!(
                picker.finish(old, Completion::Selected(PathBuf::from("stale.yaml"))),
                None
            );
            assert_eq!(
                picker.finish(old, Completion::Failed(Failure::Backend)),
                None
            );
            assert!(picker.failure(kind).is_none());
            assert_eq!(
                picker.finish(active, Completion::Failed(Failure::Backend)),
                Some(Completion::Failed(Failure::Backend))
            );
            assert_eq!(picker.failure(kind), Some(Failure::Backend));
            let retry = picker.begin(kind).unwrap();
            assert!(picker.failure(kind).is_none());
            assert_eq!(
                picker.finish(retry, Completion::Cancelled),
                Some(Completion::Cancelled)
            );
            assert!(picker.failure(kind).is_none());
        }
    }
}
