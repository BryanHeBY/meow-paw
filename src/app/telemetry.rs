//! Read-only polling must not delay the controller owner or its Stop path.

use super::{Snapshot, TrafficRate, retain_inventory};
use crate::engine::controller::{Connection, Controller, Inventory, Traffic};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(super) struct Telemetry {
    reader: Option<Reader>,
    next: Option<Instant>,
}

impl Telemetry {
    /// The next read must observe a new owner or a controller mutation.
    /// Discard any older receipt, even if the socket cannot cancel mid-read.
    pub(super) fn invalidate(&mut self) {
        self.expedite();
        if let Some(pending) = self
            .reader
            .as_ref()
            .and_then(|reader| reader.pending.as_ref())
        {
            pending.invalidated.store(true, Ordering::Release);
        }
    }

    /// An ordinary probe leaves the current read usable. Request another
    /// sooner without starving telemetry during a serial group probe.
    pub(super) fn expedite(&mut self) {
        self.next = None;
    }

    pub(super) fn poll(
        &mut self,
        controller: &Controller,
        generation: u64,
        snapshot: &Mutex<Snapshot>,
        cancelled: impl Fn() -> bool,
    ) {
        if self.reader.is_none() && self.is_due() {
            self.reader = Reader::start().ok();
            if self.reader.is_none() {
                self.defer();
            }
        }
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        match reader.completed.try_recv() {
            Ok(sample) => {
                let pending = reader.pending.take().expect("one requested telemetry read");
                // Apply admission and cancellation share the owner snapshot
                // lock. Never publish a pre-apply or retired controller read.
                let mut state = snapshot.lock().expect("snapshot poisoned");
                if let Some(sample) = sample
                    && pending.generation == generation
                    && !pending.invalidated.load(Ordering::Acquire)
                    && !cancelled()
                    && !state.apply_pending()
                {
                    if self.next.is_some() {
                        self.next = Some(sample.at + SAMPLE_INTERVAL);
                    }
                    sample.publish(&mut state);
                } else {
                    self.next = None;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.reader = None;
                self.defer();
                return;
            }
        }
        let due = self.next.is_none_or(|next| Instant::now() >= next);
        if reader.pending.is_none() && due && !cancelled() {
            if reader.dispatch(controller, generation) {
                // None is reserved for a new expedite during this read.
                self.next = Some(Instant::now());
            } else {
                self.reader = None;
                self.defer();
            }
        }
    }

    fn is_due(&self) -> bool {
        self.next.is_none_or(|next| Instant::now() >= next)
    }

    fn defer(&mut self) {
        self.next = Some(Instant::now() + SAMPLE_INTERVAL);
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        // Closing the request channel retires the reader after its bounded
        // HTTP operation; joining it here would reintroduce Quit latency.
        self.invalidate();
    }
}

struct Reader {
    request: SyncSender<ReadJob>,
    completed: Receiver<Option<Sample>>,
    pending: Option<Pending>,
}

struct ReadJob {
    controller: Controller,
    invalidated: Arc<AtomicBool>,
}

struct Pending {
    generation: u64,
    invalidated: Arc<AtomicBool>,
}

impl Reader {
    fn start() -> io::Result<Self> {
        let (request, work) = mpsc::sync_channel::<ReadJob>(1);
        let (done, completed) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("paw-telemetry".into())
            .spawn(move || {
                for job in work {
                    let sample = Sample::read(&job.controller, &job.invalidated);
                    if done.send(sample).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            request,
            completed,
            pending: None,
        })
    }

    fn dispatch(&mut self, controller: &Controller, generation: u64) -> bool {
        let invalidated = Arc::new(AtomicBool::new(false));
        if self
            .request
            .try_send(ReadJob {
                controller: controller.clone(),
                invalidated: invalidated.clone(),
            })
            .is_err()
        {
            return false;
        }
        self.pending = Some(Pending {
            generation,
            invalidated,
        });
        true
    }
}

struct Sample {
    at: Instant,
    inventory: Option<Arc<Inventory>>,
    connections: Option<Arc<Vec<Connection>>>,
    traffic: Option<Traffic>,
}

impl Sample {
    fn read(controller: &Controller, invalidated: &AtomicBool) -> Option<Self> {
        let valid = || !invalidated.load(Ordering::Acquire);
        if !valid() {
            return None;
        }
        let inventory = controller.inventory().ok().map(Arc::new);
        if !valid() {
            return None;
        }
        let connections = controller.connections().ok().map(Arc::new);
        if !valid() {
            return None;
        }
        let traffic = controller.traffic().ok();
        valid().then(|| Self {
            at: Instant::now(),
            inventory,
            connections,
            traffic,
        })
    }

    fn publish(self, snapshot: &mut Snapshot) {
        let Self {
            at,
            inventory,
            connections,
            traffic,
        } = self;
        snapshot.last_update =
            (inventory.is_some() || connections.is_some() || traffic.is_some()).then_some(at);
        snapshot.traffic_history.record(
            at,
            traffic.as_ref().map(|t| TrafficRate {
                up: t.up,
                down: t.down,
            }),
        );
        // Keep the last good inventory on a transient controller failure.
        retain_inventory(snapshot, inventory);
        snapshot.connection_history.record(connections.as_ref());
        snapshot.connections = connections;
        snapshot.traffic = traffic;
        snapshot.revision = snapshot.revision.wrapping_add(1);
    }
}
