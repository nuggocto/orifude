mod app;
mod components;
mod event;
mod layout;
mod style;
mod terminal;
mod text;
mod view;

use std::error::Error;
use std::fmt;
use std::io;
use std::time::Instant;

use app::{App, AppAction};
pub use event::EventError;
use event::{EventPump, RuntimeEvent};
use style::{StyleProfile, TerminalEnvironment, detect_color};
use terminal::TerminalSession;

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
    let paths = runtime_paths().map_err(TuiError::Paths)?;
    let mut storage = Storage::open(paths).map_err(TuiError::Storage)?;
    let settings = storage.settings().map_err(TuiError::Storage)?;
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
    let mut app = App::new(settings, started);
    let run_result = run_shell(
        &mut terminal,
        &events,
        &mut storage,
        &mut app,
        detected_color,
        color_disabled,
        started,
    );
    let shutdown_result = events.shutdown().map_err(TuiError::Events);
    let restore_result = terminal.restore().map_err(TuiError::Terminal);

    run_result.and(shutdown_result).and(restore_result)
}

fn runtime_paths() -> Result<AppPaths, PathError> {
    #[cfg(feature = "isolated-test-paths")]
    if let Some(root) = std::env::var_os("ORIFUDE_TEST_ROOT") {
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() {
            return Err(PathError);
        }
        return Ok(AppPaths::injected(
            root.join("data"),
            root.join("config"),
            root.join("cache"),
        ));
    }

    AppPaths::platform()
}

fn run_shell(
    terminal: &mut TerminalSession,
    events: &EventPump,
    storage: &mut Storage,
    app: &mut App,
    detected_color: style::ColorCapability,
    color_disabled: bool,
    started: Instant,
) -> Result<(), TuiError> {
    render(terminal, app, detected_color, color_disabled, started)?;
    events.set_animation_active(app.animation_active(started));

    loop {
        let Some(event) = events.next().map_err(TuiError::Events)? else {
            return Err(TuiError::EventWorkerStopped);
        };
        let now = match event {
            RuntimeEvent::Tick(now) => now,
            RuntimeEvent::Key(_) | RuntimeEvent::Resize(..) | RuntimeEvent::Focus(_) => {
                Instant::now()
            }
        };
        let previous_settings = app.settings();
        let action = match event {
            RuntimeEvent::Key(key) => app.handle_key(key),
            RuntimeEvent::Resize(width, height) => {
                terminal.resize(width, height).map_err(TuiError::Terminal)?;
                AppAction::Render
            }
            RuntimeEvent::Focus(focused) => app.set_focused(focused),
            RuntimeEvent::Tick(now) => app.handle_tick(now),
        };

        let should_render = match action {
            AppAction::None => false,
            AppAction::Render => true,
            AppAction::SaveSettings(settings) => {
                if let Err(error) = storage.save_settings(settings) {
                    app.restore_settings(previous_settings);
                    app.show_error(&error);
                }
                true
            }
            AppAction::Exit => break,
        };
        events.set_animation_active(app.animation_active(now));
        if should_render {
            render(terminal, app, detected_color, color_disabled, now)?;
        }
    }
    Ok(())
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
