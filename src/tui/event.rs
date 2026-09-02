use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

pub(crate) const EVENT_QUEUE_CAPACITY: usize = 256;
const ACTIVE_TICK_INTERVAL: Duration = Duration::from_nanos(33_333_334);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Focus(bool),
    Tick(Instant),
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
}

impl QueueState {
    fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(EVENT_QUEUE_CAPACITY),
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

        while state.events.is_empty() && !self.shutdown.load(Ordering::Acquire) {
            state = self
                .not_empty
                .wait(state)
                .map_err(|_| EventError::QueueUnavailable)?;
        }

        let event = state.events.pop_front();
        if event.is_some() {
            self.not_full.notify_one();
        }
        Ok(event)
    }

    fn begin_shutdown(&self) {
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

impl EventPump {
    pub(crate) fn start() -> Result<Self, EventError> {
        let shared = Arc::new(SharedQueue::new());
        let failure = Arc::new(Mutex::new(None));
        let worker_shared = Arc::clone(&shared);
        let worker_failure = Arc::clone(&failure);
        let worker = thread::Builder::new()
            .name("orifude-terminal-events".to_owned())
            .spawn(move || {
                match panic::catch_unwind(AssertUnwindSafe(|| run_event_worker(&worker_shared))) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => record_failure(&worker_failure, error),
                    Err(_panic) => record_failure(&worker_failure, EventError::WorkerPanicked),
                }
                worker_shared.begin_shutdown();
            })
            .map_err(EventError::Io)?;

        Ok(Self {
            shared,
            failure,
            worker: Some(worker),
        })
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

    while !shared.shutdown.load(Ordering::Acquire) {
        let animation_active = shared.animation_active.load(Ordering::Acquire);
        if animation_active && !animation_was_active {
            next_tick = Instant::now() + ACTIVE_TICK_INTERVAL;
        }
        animation_was_active = animation_active;

        let timeout = if animation_active {
            next_tick.saturating_duration_since(Instant::now())
        } else {
            IDLE_POLL_INTERVAL
        };

        if event::poll(timeout).map_err(EventError::Io)?
            && let Some(runtime_event) = translate_event(&event::read().map_err(EventError::Io)?)
        {
            shared.send(runtime_event)?;
        }

        if animation_active && Instant::now() >= next_tick {
            let now = Instant::now();
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
