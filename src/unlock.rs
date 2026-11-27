//! Explicit service reachability checks over a capability-advertising managed core.

pub const ROUTE_LIMIT: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    Cancelling,
    Unsupported,
    NoRoutes,
    Complete,
    Cancelled,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Passed { elapsed_ms: u32 },
    Http(u16),
    TimedOut,
    Failed,
    Cancelled,
    NotRun,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub route: String,
    pub checks: [Status; 2],
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub phase: Phase,
    pub rows: Vec<Row>,
    #[cfg(any(target_os = "linux", windows, test))]
    active: Option<transport::Ticket>,
}

impl State {
    #[cfg(any(target_os = "linux", windows, test))]
    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }
}

#[cfg(any(target_os = "linux", windows, test))]
mod transport;
#[cfg(any(target_os = "linux", windows, test))]
pub(crate) use transport::Client;
#[cfg(any(target_os = "linux", windows))]
pub(crate) use transport::{Job, routes};
