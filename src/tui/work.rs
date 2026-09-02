use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::content;
use crate::generator::{GenerationOutcome, GenerationSeed};
use crate::solver::CancellationFlag;

use super::event::EventNotifier;

pub(crate) struct WorkManager {
    next_id: u64,
    active: Option<ActiveJob>,
}

struct ActiveJob {
    id: u64,
    cancel: Arc<CancellationFlag>,
    result: Arc<Mutex<Option<GenerationOutcome>>>,
    worker: JoinHandle<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkError {
    Busy,
    InvalidPolicy,
    StartFailed,
    WorkerPanicked,
    ResultUnavailable,
}

impl std::fmt::Display for WorkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => formatter.write_str("another paper is already being generated"),
            Self::InvalidPolicy => formatter.write_str("the local generation policy is invalid"),
            Self::StartFailed => formatter.write_str("the paper generator could not start"),
            Self::WorkerPanicked => formatter.write_str("the paper generator stopped unexpectedly"),
            Self::ResultUnavailable => {
                formatter.write_str("the generated paper result is unavailable")
            }
        }
    }
}

impl std::error::Error for WorkError {}

impl WorkManager {
    pub(crate) const fn new() -> Self {
        Self {
            next_id: 1,
            active: None,
        }
    }

    pub(crate) fn start(
        &mut self,
        notifier: EventNotifier,
        pack_id: &'static str,
        seed: GenerationSeed,
    ) -> Result<u64, WorkError> {
        if self.active.is_some() {
            return Err(WorkError::Busy);
        }
        let generator = content::generator(pack_id).map_err(|_| WorkError::InvalidPolicy)?;
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        let cancel = Arc::new(CancellationFlag::new());
        let result = Arc::new(Mutex::new(None));
        let worker_cancel = Arc::clone(&cancel);
        let worker_result = Arc::clone(&result);
        let worker = thread::Builder::new()
            .name("orifude-paper-generator".to_owned())
            .spawn(move || {
                let outcome = generator.generate(seed, worker_cancel.as_ref());
                if let Ok(mut slot) = worker_result.lock() {
                    *slot = Some(outcome);
                }
                let _notification = notifier.work_ready(id);
            })
            .map_err(|_| WorkError::StartFailed)?;
        self.active = Some(ActiveJob {
            id,
            cancel,
            result,
            worker,
        });
        Ok(id)
    }

    pub(crate) fn finish(&mut self, id: u64) -> Result<Option<GenerationOutcome>, WorkError> {
        let Some(active) = self.active.take() else {
            return Ok(None);
        };
        if active.id != id {
            self.active = Some(active);
            return Ok(None);
        }
        active
            .worker
            .join()
            .map_err(|_| WorkError::WorkerPanicked)?;
        let outcome = active
            .result
            .lock()
            .map_err(|_| WorkError::ResultUnavailable)?
            .take()
            .ok_or(WorkError::ResultUnavailable)?;
        Ok(Some(outcome))
    }

    pub(crate) fn cancel(&mut self) -> Result<(), WorkError> {
        let Some(active) = self.active.take() else {
            return Ok(());
        };
        active.cancel.cancel();
        active.worker.join().map_err(|_| WorkError::WorkerPanicked)
    }
}

impl Drop for WorkManager {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel.cancel();
            let _joined = active.worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{GenerationOutcome, GenerationSeed};

    use super::*;

    #[test]
    fn owned_job_returns_one_verified_result_and_can_be_reused() {
        let mut work = WorkManager::new();
        let id = work
            .start(
                EventNotifier::isolated(),
                "orifude-endless",
                GenerationSeed::current(7),
            )
            .expect("job starts");
        let outcome = work
            .finish(id)
            .expect("owned worker joins")
            .expect("active result");
        assert!(matches!(outcome, GenerationOutcome::Generated { .. }));

        let second = work
            .start(
                EventNotifier::isolated(),
                "orifude-endless",
                GenerationSeed::current(8),
            )
            .expect("manager accepts another job after join");
        assert!(work.finish(second).expect("second worker joins").is_some());
    }

    #[test]
    fn cancellation_always_joins_the_owned_job() {
        let mut work = WorkManager::new();
        let id = work
            .start(
                EventNotifier::isolated(),
                "orifude-endless",
                GenerationSeed::current(9),
            )
            .expect("job starts");
        work.cancel().expect("cancelled worker joins");
        assert!(
            work.finish(id)
                .expect("stale notification is ignored")
                .is_none()
        );
        assert!(work.cancel().is_ok());
    }
}
