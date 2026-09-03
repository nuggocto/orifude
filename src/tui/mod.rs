mod app;
mod clock;
mod components;
mod event;
mod layout;
mod session;
mod style;
mod terminal;
mod text;
mod view;
mod work;

use std::error::Error;
use std::fmt;
use std::io;
use std::time::Instant;

use app::{App, AppAction};
use clock::{Clock, ClockError};
pub use event::EventError;
use event::{EventPump, RuntimeEvent};
use style::{StyleProfile, TerminalEnvironment, detect_color};
use terminal::TerminalSession;
use work::{WorkError, WorkManager};

use crate::storage::{AppPaths, PathError, Storage, StorageError};

#[derive(Debug)]
/// A failure while preparing, running, or restoring the terminal application.
pub enum TuiError {
    /// Per-user application directories could not be resolved.
    Paths(PathError),
    /// Local settings or progress storage could not be opened or read.
    Storage(StorageError),
    /// Terminal acquisition, drawing, resizing, or restoration failed.
    Terminal(io::Error),
    /// The owned input worker or its queue failed.
    Events(EventError),
    /// The local calendar date or injected test clock was unavailable.
    Clock(ClockError),
    /// The owned puzzle generation worker failed.
    Work(WorkError),
    /// The input worker ended without a normal shutdown request or error.
    EventWorkerStopped,
}

impl fmt::Display for TuiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paths(_) => formatter.write_str("could not find Orifude's local directories"),
            Self::Storage(_) => formatter.write_str("could not open Orifude's local progress"),
            Self::Terminal(_) => formatter.write_str("could not open the terminal interface"),
            Self::Events(_) => formatter.write_str("could not read terminal input"),
            Self::Clock(_) => formatter.write_str("could not read the local calendar date"),
            Self::Work(_) => formatter.write_str("could not generate a local paper"),
            Self::EventWorkerStopped => {
                formatter.write_str("the terminal input worker stopped unexpectedly")
            }
        }
    }
}

impl Error for TuiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Paths(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Terminal(error) => Some(error),
            Self::Events(error) => Some(error),
            Self::Clock(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::EventWorkerStopped => None,
        }
    }
}

/// Opens the offline terminal shell and keeps ownership until restoration.
///
/// # Errors
///
/// Returns a typed path, storage, terminal, or input failure after attempting
/// to restore the caller's terminal state.
pub fn play() -> Result<(), TuiError> {
    TerminalSession::check_interactive().map_err(TuiError::Terminal)?;
    let paths = AppPaths::runtime().map_err(TuiError::Paths)?;
    let mut storage = Storage::open(paths).map_err(TuiError::Storage)?;
    let settings = storage.settings().map_err(TuiError::Storage)?;
    let progress = storage.progress_page(0).map_err(TuiError::Storage)?;
    let packs = storage.registered_packs().map_err(TuiError::Storage)?;
    let journey_done = crate::content::journey()
        .iter()
        .map(|paper| storage.completion_matches(paper.puzzle()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(TuiError::Storage)?;
    let clock = Clock::runtime().map_err(TuiError::Clock)?;
    let snapshot = clock.now().map_err(TuiError::Clock)?;
    let environment = TerminalEnvironment::capture();
    let detected_color = detect_color(&environment);
    let color_disabled = environment.color_disabled();
    let mut terminal = TerminalSession::open().map_err(TuiError::Terminal)?;
    let mut events = match EventPump::start() {
        Ok(events) => events,
        Err(error) => {
            let _restore_result = terminal.restore();
            return Err(TuiError::Events(error));
        }
    };
    let started = Instant::now();
    let mut app = App::with_state(
        settings,
        progress,
        packs,
        journey_done,
        snapshot.date,
        snapshot.unix_seconds.cast_unsigned(),
        started,
    );
    let mut work = WorkManager::new();
    let run_result = {
        let mut shell = Shell {
            terminal: &mut terminal,
            events: &events,
            storage: &mut storage,
            app: &mut app,
            clock: &clock,
            work: &mut work,
            detected_color,
            color_disabled,
        };
        shell.run(started)
    };
    let work_result = work.cancel().map_err(TuiError::Work);
    let shutdown_result = events.shutdown().map_err(TuiError::Events);
    let restore_result = terminal.restore().map_err(TuiError::Terminal);

    run_result
        .and(work_result)
        .and(shutdown_result)
        .and(restore_result)
}

struct Shell<'a> {
    terminal: &'a mut TerminalSession,
    events: &'a EventPump,
    storage: &'a mut Storage,
    app: &'a mut App,
    clock: &'a Clock,
    work: &'a mut WorkManager,
    detected_color: style::ColorCapability,
    color_disabled: bool,
}

enum LoopControl {
    Continue { render: bool },
    Exit,
}

impl Shell<'_> {
    fn run(&mut self, started: Instant) -> Result<(), TuiError> {
        self.render(started)?;
        self.events
            .set_animation_active(self.app.animation_active(started));

        loop {
            let Some(event) = self.events.next().map_err(TuiError::Events)? else {
                return Err(TuiError::EventWorkerStopped);
            };
            let now = match event {
                RuntimeEvent::Tick(now) => now,
                RuntimeEvent::Key(_)
                | RuntimeEvent::Resize(..)
                | RuntimeEvent::Focus(_)
                | RuntimeEvent::WorkReady(_) => Instant::now(),
            };
            let previous_settings = self.app.settings();
            let action = self.dispatch_event(event, now)?;
            let LoopControl::Continue {
                render: should_render,
            } = self.handle_action(action, previous_settings)?
            else {
                return Ok(());
            };
            self.events
                .set_animation_active(self.app.animation_active(now));
            if should_render {
                self.render(now)?;
            }
        }
    }

    fn dispatch_event(&mut self, event: RuntimeEvent, now: Instant) -> Result<AppAction, TuiError> {
        Ok(match event {
            RuntimeEvent::Key(key) => {
                let snapshot = self.clock.now().map_err(TuiError::Clock)?;
                self.app.set_local_date(snapshot.date);
                self.app.handle_key(key, now)
            }
            RuntimeEvent::Resize(width, height) => {
                self.terminal
                    .resize(width, height)
                    .map_err(TuiError::Terminal)?;
                AppAction::Render
            }
            RuntimeEvent::Focus(focused) => self.app.set_focused(focused),
            RuntimeEvent::Tick(now) => self.app.handle_tick(now),
            RuntimeEvent::WorkReady(id) => {
                let Some(outcome) = self.work.finish(id).map_err(TuiError::Work)? else {
                    return Ok(AppAction::None);
                };
                if let Some((day, version, puzzle)) = self.app.generation_finished(id, outcome)
                    && let Err(error) = self.storage.record_daily(day, version, &puzzle, false)
                {
                    self.app.show_error(&error);
                }
                AppAction::Render
            }
        })
    }

    fn handle_action(
        &mut self,
        action: AppAction,
        previous_settings: crate::storage::Settings,
    ) -> Result<LoopControl, TuiError> {
        let render = match action {
            AppAction::None => false,
            AppAction::Render => true,
            AppAction::SaveSettings(settings) => {
                if let Err(error) = self.storage.save_settings(settings) {
                    self.app.restore_settings(previous_settings);
                    self.app.show_error(&error);
                } else {
                    self.app.settings_saved(settings);
                }
                true
            }
            AppAction::SaveCompletion => {
                self.save_completion()?;
                true
            }
            AppAction::StartGeneration {
                pack_id,
                seed,
                source,
            } => {
                match self.work.start(self.events.notifier(), pack_id, seed) {
                    Ok(id) => self.app.generation_started(id, source),
                    Err(error) => {
                        self.app.generation_cancelled();
                        self.app.show_error(&error);
                    }
                }
                true
            }
            AppAction::CancelGeneration => {
                self.work.cancel().map_err(TuiError::Work)?;
                self.app.generation_cancelled();
                true
            }
            AppAction::LoadPack(pack_id) => {
                match self.storage.load_pack(&pack_id) {
                    Ok(Some(pack)) => self.app.open_pack(pack),
                    Ok(None) => self
                        .app
                        .show_error(&"That installed pack is no longer available."),
                    Err(error) => self.app.show_error(&error),
                }
                self.storage.clear_loaded_pack();
                true
            }
            AppAction::LoadReplay { pack_id, puzzle_id } => {
                match self.storage.best_replay(&pack_id, &puzzle_id) {
                    Ok(Some(replay)) => self.app.open_replay(&replay),
                    Ok(None) => self
                        .app
                        .show_error(&"That keepsake is no longer available."),
                    Err(error) => self.app.show_error(&error),
                }
                true
            }
            AppAction::LoadKeepsakes(offset) => {
                match self.storage.progress_page(offset) {
                    Ok(page) => self.app.keepsakes_loaded(page, offset),
                    Err(error) => self.app.show_error(&error),
                }
                true
            }
            AppAction::Exit => return Ok(LoopControl::Exit),
        };
        Ok(LoopControl::Continue { render })
    }

    fn save_completion(&mut self) -> Result<(), TuiError> {
        let Some((puzzle, replay, source, undo_count, hints_used)) = self.app.completion_request()
        else {
            self.app
                .show_error(&"The finished paper could not be read.");
            return Ok(());
        };
        let snapshot = self.clock.now().map_err(TuiError::Clock)?;
        let saved = match source {
            session::PlaySource::Daily {
                day,
                generator_version,
            } => self.storage.record_daily_completion(
                crate::storage::DailyKey {
                    day,
                    generator_version,
                },
                &puzzle,
                &replay,
                snapshot.unix_seconds,
                undo_count,
                hints_used,
            ),
            _ => self.storage.record_completion(
                &puzzle,
                &replay,
                snapshot.unix_seconds,
                undo_count,
                hints_used,
            ),
        };
        match saved {
            Ok(progress) => self.app.completion_saved(progress),
            Err(error) => self.app.show_error(&error),
        }
        Ok(())
    }

    fn render(&mut self, now: Instant) -> Result<(), TuiError> {
        render(
            self.terminal,
            self.app,
            self.detected_color,
            self.color_disabled,
            now,
        )
    }
}

fn render(
    terminal: &mut TerminalSession,
    app: &App,
    detected_color: style::ColorCapability,
    color_disabled: bool,
    now: Instant,
) -> Result<(), TuiError> {
    let profile = StyleProfile::resolve(app.settings(), detected_color, color_disabled);
    terminal
        .draw(|frame| view::render(frame, app, profile, now))
        .map_err(TuiError::Terminal)
}
