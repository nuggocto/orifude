use std::fmt;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

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

#[derive(Debug)]
/// A terminal input failure.
pub enum EventError {
    /// The operating system terminal input call failed.
    Io(io::Error),
}

impl fmt::Display for EventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "terminal input failed: {error}"),
        }
    }
}

impl std::error::Error for EventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
        }
    }
}

/// Reads terminal input on the calling thread and merges in finished
/// generation jobs, animation ticks, and periodic size checks.
///
/// The idle poll interval bounds how long a finished job waits to be seen.
pub(crate) struct EventPump {
    work_sender: Sender<u64>,
    work_ready: Receiver<u64>,
    pending: Option<RuntimeEvent>,
    schedule: Schedule,
}

/// Wakes the shell when a generation job ends.
#[derive(Clone)]
pub(crate) struct EventNotifier {
    sender: Sender<u64>,
}

impl EventNotifier {
    pub(crate) fn work_ready(&self, job_id: u64) {
        // A closed receiver means the shell has stopped; nothing is left to wake.
        let _closed = self.sender.send(job_id);
    }

    #[cfg(test)]
    pub(crate) fn channel() -> (Self, Receiver<u64>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender }, receiver)
    }
}

impl EventPump {
    pub(crate) fn start() -> Result<Self, EventError> {
        // The first poll installs Crossterm's resize source. Fail before play
        // starts if the terminal cannot report input.
        event::poll(Duration::ZERO).map_err(EventError::Io)?;
        let size = crossterm::terminal::size().map_err(EventError::Io)?;
        let (work_sender, work_ready) = mpsc::channel();
        Ok(Self {
            work_sender,
            work_ready,
            pending: None,
            schedule: Schedule::new(Instant::now(), size),
        })
    }

    pub(crate) fn notifier(&self) -> EventNotifier {
        EventNotifier {
            sender: self.work_sender.clone(),
        }
    }

    pub(crate) fn set_animation_active(&mut self, active: bool) {
        self.schedule.set_animation_active(active, Instant::now());
    }

    /// Blocks until the next event. A finished job comes before pending
    /// input, so a generated paper never waits behind a burst of keys.
    pub(crate) fn next(&mut self) -> Result<RuntimeEvent, EventError> {
        loop {
            if let Ok(job_id) = self.work_ready.try_recv() {
                return Ok(RuntimeEvent::WorkReady(job_id));
            }
            if let Some(event) = self.pending.take() {
                return Ok(event);
            }

            let timeout = self.schedule.timeout(Instant::now());
            if event::poll(timeout).map_err(EventError::Io)?
                && let Some(event) = translate_event(&event::read().map_err(EventError::Io)?)
            {
                return self.coalesce_resizes(event);
            }

            let now = Instant::now();
            if self.schedule.size_check_due(now) {
                let size = crossterm::terminal::size().map_err(EventError::Io)?;
                if let Some(event) = self.schedule.observe_size(size, now) {
                    return Ok(event);
                }
            }
            if let Some(event) = self.schedule.tick_due(now) {
                return Ok(event);
            }
        }
    }

    /// Replaces a run of resize events with the newest one, so a dragged
    /// window costs one redraw. The first other event waits for the next call.
    fn coalesce_resizes(&mut self, mut event: RuntimeEvent) -> Result<RuntimeEvent, EventError> {
        while let RuntimeEvent::Resize(width, height) = event {
            self.schedule.observed_size = (width, height);
            if !event::poll(Duration::ZERO).map_err(EventError::Io)? {
                break;
            }
            match translate_event(&event::read().map_err(EventError::Io)?) {
                Some(next @ RuntimeEvent::Resize(..)) => event = next,
                Some(other) => {
                    self.pending = Some(other);
                    break;
                }
                None => {}
            }
        }
        Ok(event)
    }
}

/// Deadlines for animation ticks and the size check, kept apart from
/// terminal I/O so the timing rules are testable.
struct Schedule {
    animation_active: bool,
    next_tick: Instant,
    observed_size: (u16, u16),
    next_size_check: Instant,
}

impl Schedule {
    fn new(now: Instant, size: (u16, u16)) -> Self {
        Self {
            animation_active: false,
            next_tick: now + ACTIVE_TICK_INTERVAL,
            observed_size: size,
            next_size_check: now + SIZE_POLL_INTERVAL,
        }
    }

    fn set_animation_active(&mut self, active: bool, now: Instant) {
        if active && !self.animation_active {
            self.next_tick = now + ACTIVE_TICK_INTERVAL;
        }
        self.animation_active = active;
    }

    fn timeout(&self, now: Instant) -> Duration {
        let event_timeout = if self.animation_active {
            self.next_tick.saturating_duration_since(now)
        } else {
            IDLE_POLL_INTERVAL
        };
        event_timeout.min(self.next_size_check.saturating_duration_since(now))
    }

    fn size_check_due(&self, now: Instant) -> bool {
        now >= self.next_size_check
    }

    fn observe_size(&mut self, size: (u16, u16), now: Instant) -> Option<RuntimeEvent> {
        self.next_size_check = now + SIZE_POLL_INTERVAL;
        if size == self.observed_size {
            return None;
        }
        self.observed_size = size;
        Some(RuntimeEvent::Resize(size.0, size.1))
    }

    fn tick_due(&mut self, now: Instant) -> Option<RuntimeEvent> {
        if !self.animation_active || now < self.next_tick {
            return None;
        }
        self.next_tick = now + ACTIVE_TICK_INTERVAL;
        Some(RuntimeEvent::Tick(now))
    }
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
    use std::time::{Duration, Instant};

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{
        ACTIVE_TICK_INTERVAL, EventNotifier, IDLE_POLL_INTERVAL, RuntimeEvent, SIZE_POLL_INTERVAL,
        Schedule, translate_event,
    };

    #[test]
    fn idle_schedule_never_ticks_and_still_checks_size() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start, (80, 24));

        assert_eq!(schedule.timeout(start), IDLE_POLL_INTERVAL);
        assert_eq!(schedule.tick_due(start + Duration::from_secs(5)), None);

        let later = start + SIZE_POLL_INTERVAL;
        assert!(schedule.size_check_due(later));
        assert_eq!(schedule.observe_size((80, 24), later), None);
        assert!(!schedule.size_check_due(later));
        assert_eq!(
            schedule.observe_size((120, 40), later + SIZE_POLL_INTERVAL),
            Some(RuntimeEvent::Resize(120, 40))
        );
    }

    #[test]
    fn animation_ticks_start_one_interval_after_activation() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start, (80, 24));
        // Activate well before the first size check so the tick sets the timeout.
        let activated = start + Duration::from_millis(1);
        schedule.set_animation_active(true, activated);

        assert_eq!(schedule.timeout(activated), ACTIVE_TICK_INTERVAL);
        assert_eq!(schedule.tick_due(activated), None);
        let due = activated + ACTIVE_TICK_INTERVAL;
        assert_eq!(schedule.tick_due(due), Some(RuntimeEvent::Tick(due)));
        assert_eq!(schedule.tick_due(due), None, "one tick per interval");

        schedule.set_animation_active(false, due);
        assert_eq!(schedule.tick_due(due + ACTIVE_TICK_INTERVAL), None);
    }

    #[test]
    fn staying_active_keeps_the_existing_tick_deadline() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start, (80, 24));
        schedule.set_animation_active(true, start);
        schedule.set_animation_active(true, start + ACTIVE_TICK_INTERVAL / 2);

        let due = start + ACTIVE_TICK_INTERVAL;
        assert_eq!(schedule.tick_due(due), Some(RuntimeEvent::Tick(due)));
    }

    #[test]
    fn only_presses_resizes_and_focus_reach_the_shell() {
        let press = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE);
        let mut release = press;
        release.kind = KeyEventKind::Release;

        assert_eq!(
            translate_event(&Event::Key(press)),
            Some(RuntimeEvent::Key(press))
        );
        assert_eq!(translate_event(&Event::Key(release)), None);
        assert_eq!(translate_event(&Event::Paste("ink".into())), None);
        assert_eq!(
            translate_event(&Event::Resize(100, 30)),
            Some(RuntimeEvent::Resize(100, 30))
        );
        assert_eq!(
            translate_event(&Event::FocusLost),
            Some(RuntimeEvent::Focus(false))
        );
    }

    #[test]
    fn a_notifier_outliving_the_shell_does_not_fail() {
        let (notifier, receiver) = EventNotifier::channel();
        notifier.work_ready(3);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)), Ok(3));
        drop(receiver);
        notifier.work_ready(4);
    }
}
