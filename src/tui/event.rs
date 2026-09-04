use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

pub(crate) const EVENT_QUEUE_CAPACITY: usize = 256;
const ACTIVE_TICK_INTERVAL: Duration = Duration::from_nanos(33_333_334);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SIZE_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Focus(bool),
    Tick(Instant),
    WorkReady(u64),
}

impl RuntimeEvent {
    fn can_coalesce(self) -> bool {
        matches!(self, Self::Resize(..) | Self::Tick(..))
    }

    fn same_coalescing_class(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Resize(..), Self::Resize(..)) | (Self::Tick(..), Self::Tick(..))
        )
    }
}

#[derive(Debug)]
/// A terminal input worker or bounded queue failure.
pub enum EventError {
    /// The operating system terminal input call failed.
    Io(io::Error),
    /// The synchronized queue was poisoned or stopped accepting work.
    QueueUnavailable,
    /// The owned event worker panicked and was joined.
    WorkerPanicked,
}

impl fmt::Display for EventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "terminal input failed: {error}"),
            Self::QueueUnavailable => {
                formatter.write_str("terminal input queue became unavailable")
            }
            Self::WorkerPanicked => formatter.write_str("terminal input worker panicked"),
        }
    }
}

impl std::error::Error for EventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::QueueUnavailable | Self::WorkerPanicked => None,
        }
    }
}

struct QueueState {
    events: VecDeque<RuntimeEvent>,
    work_ready: Option<u64>,
}

impl QueueState {
    fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(EVENT_QUEUE_CAPACITY),
            work_ready: None,
        }
    }

    fn coalesce(&mut self, event: RuntimeEvent) -> bool {
        if !event.can_coalesce() {
            return false;
        }

        if let Some(existing) = self
            .events
            .iter_mut()
            .find(|existing| existing.same_coalescing_class(event))
        {
            *existing = event;
            true
        } else {
            false
        }
    }
}

struct SharedQueue {
    state: Mutex<QueueState>,
    not_empty: Condvar,
    not_full: Condvar,
    shutdown: AtomicBool,
    animation_active: AtomicBool,
}

impl SharedQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState::new()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            shutdown: AtomicBool::new(false),
            animation_active: AtomicBool::new(false),
        }
    }

    fn send(&self, event: RuntimeEvent) -> Result<(), EventError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| EventError::QueueUnavailable)?;

        if state.coalesce(event) {
            self.not_empty.notify_one();
            return Ok(());
        }

        while state.events.len() == EVENT_QUEUE_CAPACITY && !self.shutdown.load(Ordering::Acquire) {
            state = self
                .not_full
                .wait(state)
                .map_err(|_| EventError::QueueUnavailable)?;
            if state.coalesce(event) {
                self.not_empty.notify_one();
                return Ok(());
            }
        }

        if self.shutdown.load(Ordering::Acquire) {
            return Err(EventError::QueueUnavailable);
        }

        debug_assert!(state.events.len() < EVENT_QUEUE_CAPACITY);
        state.events.push_back(event);
        self.not_empty.notify_one();
        Ok(())
    }

    fn receive(&self) -> Result<Option<RuntimeEvent>, EventError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| EventError::QueueUnavailable)?;

        while state.events.is_empty()
            && state.work_ready.is_none()
            && !self.shutdown.load(Ordering::Acquire)
        {
            state = self
                .not_empty
                .wait(state)
                .map_err(|_| EventError::QueueUnavailable)?;
        }

        let event = state
            .work_ready
            .take()
            .map(RuntimeEvent::WorkReady)
            .or_else(|| state.events.pop_front());
        if event.is_some() {
            self.not_full.notify_one();
        }
        Ok(event)
    }

    fn notify_work_ready(&self, job_id: u64) -> Result<(), EventError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| EventError::QueueUnavailable)?;
        if self.shutdown.load(Ordering::Acquire) {
            return Err(EventError::QueueUnavailable);
        }
        state.work_ready = Some(job_id);
        self.not_empty.notify_one();
        Ok(())
    }

    fn begin_shutdown(&self) {
        // Serialize the predicate change with checking it and entering either wait.
        // Even a poisoned queue must wake its waiters so they can report failure.
        let _state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.shutdown.store(true, Ordering::Release);
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}

pub(crate) struct EventPump {
    shared: Arc<SharedQueue>,
    failure: Arc<Mutex<Option<EventError>>>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub(crate) struct EventNotifier {
    shared: Arc<SharedQueue>,
}

impl EventNotifier {
    pub(crate) fn work_ready(&self, job_id: u64) -> Result<(), EventError> {
        self.shared.notify_work_ready(job_id)
    }

    #[cfg(test)]
    pub(crate) fn isolated() -> Self {
        Self {
            shared: Arc::new(SharedQueue::new()),
        }
    }
}

impl EventPump {
    pub(crate) fn start() -> Result<Self, EventError> {
        let shared = Arc::new(SharedQueue::new());
        let failure = Arc::new(Mutex::new(None));
        let (ready_sender, ready_receiver) = mpsc::sync_channel(0);
        let worker_shared = Arc::clone(&shared);
        let worker_failure = Arc::clone(&failure);
        let worker = thread::Builder::new()
            .name("orifude-terminal-events".to_owned())
            .spawn(move || {
                match panic::catch_unwind(AssertUnwindSafe(|| {
                    // The first poll installs Crossterm's resize source. Keep
                    // initialization and every later poll/read on this worker.
                    match event::poll(Duration::ZERO) {
                        Ok(_) => ready_sender
                            .send(Ok(()))
                            .map_err(|_| EventError::QueueUnavailable)?,
                        Err(error) => {
                            ready_sender
                                .send(Err(error))
                                .map_err(|_| EventError::QueueUnavailable)?;
                            return Ok(());
                        }
                    }
                    run_event_worker(&worker_shared)
                })) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => record_failure(&worker_failure, error),
                    Err(_panic) => record_failure(&worker_failure, EventError::WorkerPanicked),
                }
                worker_shared.begin_shutdown();
            })
            .map_err(EventError::Io)?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                shared,
                failure,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                shared.begin_shutdown();
                let _ = worker.join();
                Err(EventError::Io(error))
            }
            Err(_) => {
                shared.begin_shutdown();
                let _ = worker.join();
                let error = failure
                    .lock()
                    .map_err(|_| EventError::QueueUnavailable)?
                    .take()
                    .unwrap_or(EventError::WorkerPanicked);
                Err(error)
            }
        }
    }

    pub(crate) fn next(&self) -> Result<Option<RuntimeEvent>, EventError> {
        let event = self.shared.receive()?;
        if let Some(error) = self
            .failure
            .lock()
            .map_err(|_| EventError::QueueUnavailable)?
            .take()
        {
            return Err(error);
        }
        Ok(event)
    }

    pub(crate) fn notifier(&self) -> EventNotifier {
        EventNotifier {
            shared: Arc::clone(&self.shared),
        }
    }

    pub(crate) fn set_animation_active(&self, active: bool) {
        self.shared
            .animation_active
            .store(active, Ordering::Release);
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), EventError> {
        self.shared.begin_shutdown();
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| EventError::WorkerPanicked)
    }
}

impl Drop for EventPump {
    fn drop(&mut self) {
        self.shared.begin_shutdown();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn record_failure(failure: &Mutex<Option<EventError>>, error: EventError) {
    if let Ok(mut failure) = failure.lock() {
        *failure = Some(error);
    }
}

fn run_event_worker(shared: &SharedQueue) -> Result<(), EventError> {
    let mut animation_was_active = false;
    let mut next_tick = Instant::now() + ACTIVE_TICK_INTERVAL;
    let mut observed_size = crossterm::terminal::size().map_err(EventError::Io)?;
    let mut next_size_check = Instant::now() + SIZE_POLL_INTERVAL;

    while !shared.shutdown.load(Ordering::Acquire) {
        let animation_active = shared.animation_active.load(Ordering::Acquire);
        if animation_active && !animation_was_active {
            next_tick = Instant::now() + ACTIVE_TICK_INTERVAL;
        }
        animation_was_active = animation_active;

        let event_timeout = if animation_active {
            next_tick.saturating_duration_since(Instant::now())
        } else {
            IDLE_POLL_INTERVAL
        };
        let timeout = event_timeout.min(next_size_check.saturating_duration_since(Instant::now()));

        if event::poll(timeout).map_err(EventError::Io)?
            && let Some(runtime_event) = translate_event(&event::read().map_err(EventError::Io)?)
        {
            if let RuntimeEvent::Resize(width, height) = runtime_event {
                observed_size = (width, height);
            }
            shared.send(runtime_event)?;
        }

        let now = Instant::now();
        if now >= next_size_check {
            let size = crossterm::terminal::size().map_err(EventError::Io)?;
            if size != observed_size {
                observed_size = size;
                shared.send(RuntimeEvent::Resize(size.0, size.1))?;
            }
            next_size_check = now + SIZE_POLL_INTERVAL;
        }

        if animation_active && now >= next_tick {
            shared.send(RuntimeEvent::Tick(now))?;
            next_tick = now + ACTIVE_TICK_INTERVAL;
        }
    }

    Ok(())
}

fn translate_event(event: &Event) -> Option<RuntimeEvent> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            Some(RuntimeEvent::Key(*key))
        }
        Event::Resize(width, height) => Some(RuntimeEvent::Resize(*width, *height)),
        Event::FocusGained => Some(RuntimeEvent::Focus(true)),
        Event::FocusLost => Some(RuntimeEvent::Focus(false)),
        Event::Key(_) | Event::Mouse(_) | Event::Paste(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{EVENT_QUEUE_CAPACITY, EventError, EventPump, RuntimeEvent, SharedQueue};

    fn key(character: char) -> RuntimeEvent {
        RuntimeEvent::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
    }

    #[test]
    fn resize_and_tick_replace_pending_events_without_reordering_keys() {
        let queue = SharedQueue::new();
        let first_tick = std::time::Instant::now();
        let newest_tick = first_tick + std::time::Duration::from_millis(10);
        queue.send(key('a')).expect("first key");
        queue
            .send(RuntimeEvent::Resize(80, 24))
            .expect("first resize");
        queue.send(key('b')).expect("second key");
        queue
            .send(RuntimeEvent::Tick(first_tick))
            .expect("first tick");
        queue
            .send(RuntimeEvent::Resize(120, 40))
            .expect("newest resize");
        queue
            .send(RuntimeEvent::Tick(newest_tick))
            .expect("newest tick");

        assert_eq!(queue.receive().expect("receive"), Some(key('a')));
        assert_eq!(
            queue.receive().expect("receive"),
            Some(RuntimeEvent::Resize(120, 40))
        );
        assert_eq!(queue.receive().expect("receive"), Some(key('b')));
        assert_eq!(
            queue.receive().expect("receive"),
            Some(RuntimeEvent::Tick(newest_tick))
        );
    }

    #[test]
    fn shutdown_wakes_both_waiters_at_the_predicate_to_wait_boundary() {
        use std::sync::mpsc;
        use std::time::Duration;

        for full in [false, true] {
            let queue = Arc::new(SharedQueue::new());
            if full {
                for _ in 0..EVENT_QUEUE_CAPACITY {
                    queue.send(key('x')).expect("fill queue");
                }
            }
            // Hold the mutex at the exact boundary shared by send and receive:
            // the queue requires waiting, and shutdown is still false.
            let state = queue.state.lock().expect("queue state");
            assert!(!queue.shutdown.load(super::Ordering::Acquire));
            let (started, starting) = mpsc::channel();
            let (finished, done) = mpsc::channel();
            let shutdown_queue = Arc::clone(&queue);
            let shutdown = std::thread::spawn(move || {
                started.send(()).expect("announce shutdown");
                shutdown_queue.begin_shutdown();
                finished.send(()).expect("announce completion");
            });
            starting
                .recv_timeout(Duration::from_secs(2))
                .expect("shutdown starts");
            let early = done.recv_timeout(Duration::from_millis(100));
            let condition = if full {
                &queue.not_full
            } else {
                &queue.not_empty
            };
            let (state, timeout) = condition
                .wait_timeout_while(state, Duration::from_secs(2), |_| {
                    !queue.shutdown.load(super::Ordering::Acquire)
                })
                .expect("shutdown releases waiter");
            drop(state);
            shutdown.join().expect("shutdown joins");
            assert!(
                matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
                "shutdown must not notify between the predicate check and wait"
            );
            assert!(!timeout.timed_out(), "waiter must observe shutdown");
        }
    }

    #[test]
    fn shutdown_still_completes_when_the_queue_mutex_is_poisoned() {
        let queue = Arc::new(SharedQueue::new());
        let poisoned = Arc::clone(&queue);
        let worker = std::thread::spawn(move || {
            let _state = poisoned.state.lock().expect("queue state");
            panic!("injected queue failure");
        });
        assert!(worker.join().is_err());
        queue.begin_shutdown();
        assert!(matches!(queue.receive(), Err(EventError::QueueUnavailable)));
        assert!(queue.shutdown.load(super::Ordering::Acquire));
    }

    #[test]
    fn shutdown_releases_a_sender_blocked_by_backpressure() {
        let queue = Arc::new(SharedQueue::new());
        for _ in 0..EVENT_QUEUE_CAPACITY {
            queue.send(key('x')).expect("fill queue");
        }

        let sender_queue = Arc::clone(&queue);
        let sender = std::thread::spawn(move || sender_queue.send(key('y')));
        queue.begin_shutdown();

        assert!(sender.join().expect("sender joins").is_err());
    }

    #[test]
    fn backpressure_preserves_the_key_that_waited_for_capacity() {
        let queue = Arc::new(SharedQueue::new());
        for _ in 0..EVENT_QUEUE_CAPACITY {
            queue.send(key('x')).expect("fill queue");
        }

        let sender_queue = Arc::clone(&queue);
        let sender = std::thread::spawn(move || sender_queue.send(key('y')));
        assert_eq!(queue.receive().expect("free one slot"), Some(key('x')));
        sender
            .join()
            .expect("sender joins")
            .expect("waiting key enters queue");
        for _ in 1..EVENT_QUEUE_CAPACITY {
            assert_eq!(queue.receive().expect("queued key"), Some(key('x')));
        }
        assert_eq!(queue.receive().expect("waiting key"), Some(key('y')));
    }

    #[test]
    fn queue_never_grows_past_its_documented_capacity() {
        let mut state = super::QueueState::new();
        for _ in 0..EVENT_QUEUE_CAPACITY {
            state.events.push_back(key('x'));
        }

        assert_eq!(state.events.len(), EVENT_QUEUE_CAPACITY);
        assert_eq!(state.events.capacity(), EVENT_QUEUE_CAPACITY);
    }

    #[test]
    fn completed_work_stays_observable_without_waiting_for_queue_capacity() {
        let queue = SharedQueue::new();
        for _ in 0..EVENT_QUEUE_CAPACITY {
            queue.send(key('x')).expect("fill queue");
        }

        queue
            .notify_work_ready(7)
            .expect("bounded work slot remains available");
        assert_eq!(
            queue.receive().expect("work result"),
            Some(RuntimeEvent::WorkReady(7))
        );
        assert_eq!(
            queue.receive().expect("keys remain ordered"),
            Some(key('x'))
        );
    }

    #[test]
    fn worker_failure_preempts_events_that_were_already_queued() {
        let shared = Arc::new(SharedQueue::new());
        shared.send(key('x')).expect("queued before failure");
        shared.begin_shutdown();
        let pump = EventPump {
            shared,
            failure: Arc::new(Mutex::new(Some(EventError::WorkerPanicked))),
            worker: None,
        };

        assert!(matches!(pump.next(), Err(EventError::WorkerPanicked)));
    }
}
