use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::storage::{ColorMode, GlyphMode, Settings};

use super::text::SafeText;

pub(crate) const MARK_FRAME_COUNT: usize = 24;
const MARK_REVEAL_LIMIT: Duration = Duration::from_millis(1_100);
const BRANCH_CHOICE_COUNT: usize = 7;
const SETTING_CHOICE_COUNT: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Screen {
    Branch,
    Rules,
    Settings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Overlay {
    Help,
    Quit,
    Error(SafeText),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    None,
    Render,
    SaveSettings(Settings),
    Exit,
}

pub(crate) struct App {
    screen: Screen,
    overlay: Option<Overlay>,
    selection: usize,
    settings: Settings,
    focused: bool,
    reveal_started: Option<Instant>,
}

impl App {
    pub(crate) fn new(settings: Settings, now: Instant) -> Self {
        let reveal_started = (!settings.reduced_motion && !settings.instant_reveal).then_some(now);
        Self {
            screen: Screen::Branch,
            overlay: None,
            selection: 0,
            settings,
            focused: true,
            reveal_started,
        }
    }

    pub(crate) const fn screen(&self) -> Screen {
        self.screen
    }

    pub(crate) fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub(crate) const fn selection(&self) -> usize {
        self.selection
    }

    pub(crate) const fn settings(&self) -> Settings {
        self.settings
    }

    pub(crate) const fn focused(&self) -> bool {
        self.focused
    }

    pub(crate) fn animation_active(&self, now: Instant) -> bool {
        self.reveal_started
            .is_some_and(|started| now.saturating_duration_since(started) < MARK_REVEAL_LIMIT)
    }

    pub(crate) fn mark_frame(&self, now: Instant) -> usize {
        let Some(started) = self.reveal_started else {
            return MARK_FRAME_COUNT - 1;
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed >= MARK_REVEAL_LIMIT {
            return MARK_FRAME_COUNT - 1;
        }
        let elapsed_millis = elapsed.as_millis() as usize;
        elapsed_millis.saturating_mul(MARK_FRAME_COUNT - 1) / MARK_REVEAL_LIMIT.as_millis() as usize
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let reveal_cancelled = self.reveal_started.take().is_some();
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return AppAction::Exit;
        }

        let action = if self.overlay.is_some() {
            self.handle_overlay_key(key.code)
        } else {
            match key.code {
                KeyCode::Char('?') => {
                    self.overlay = Some(Overlay::Help);
                    AppAction::Render
                }
                KeyCode::Char('q') => {
                    self.overlay = Some(Overlay::Quit);
                    AppAction::Render
                }
                KeyCode::Esc => self.back(),
                KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Left | KeyCode::Char('h') => self.adjust_setting(-1),
                KeyCode::Right | KeyCode::Char('l') => self.adjust_setting(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
                _ => AppAction::None,
            }
        };
        if reveal_cancelled && action == AppAction::None {
            AppAction::Render
        } else {
            action
        }
    }

    pub(crate) fn handle_tick(&mut self, now: Instant) -> AppAction {
        let Some(started) = self.reveal_started else {
            return AppAction::None;
        };
        if now.saturating_duration_since(started) >= MARK_REVEAL_LIMIT {
            self.reveal_started = None;
        }
        AppAction::Render
    }

    pub(crate) fn set_focused(&mut self, focused: bool) -> AppAction {
        if self.focused == focused {
            return AppAction::None;
        }
        self.focused = focused;
        AppAction::Render
    }

    pub(crate) fn show_error(&mut self, error: &dyn std::fmt::Display) {
        self.overlay = Some(Overlay::Error(SafeText::external_display(
            error,
            160,
            self.settings.glyph_mode,
        )));
    }

    pub(crate) fn restore_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    fn handle_overlay_key(&mut self, code: KeyCode) -> AppAction {
        match self.overlay.as_ref() {
            Some(Overlay::Quit) if matches!(code, KeyCode::Char('y') | KeyCode::Enter) => {
                AppAction::Exit
            }
            Some(Overlay::Quit) if matches!(code, KeyCode::Char('n' | 'q') | KeyCode::Esc) => {
                self.overlay = None;
                AppAction::Render
            }
            Some(Overlay::Help | Overlay::Error(_))
                if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?')) =>
            {
                self.overlay = None;
                AppAction::Render
            }
            Some(_) | None => AppAction::None,
        }
    }

    fn move_selection(&mut self, direction: isize) -> AppAction {
        let count = match self.screen {
            Screen::Branch => BRANCH_CHOICE_COUNT,
            Screen::Rules => 2,
            Screen::Settings => SETTING_CHOICE_COUNT,
        };
        self.selection = wrap_selection(self.selection, direction, count);
        AppAction::Render
    }

    fn adjust_setting(&mut self, direction: isize) -> AppAction {
        if self.screen != Screen::Settings || self.selection == SETTING_CHOICE_COUNT - 1 {
            return AppAction::None;
        }
        self.change_setting(direction)
    }

    fn activate(&mut self) -> AppAction {
        match self.screen {
            Screen::Branch => match self.selection {
                5 => {
                    self.screen = Screen::Rules;
                    self.selection = 0;
                    AppAction::Render
                }
                6 => {
                    self.screen = Screen::Settings;
                    self.selection = 0;
                    AppAction::Render
                }
                _ => {
                    self.overlay = Some(Overlay::Error(SafeText::internal(
                        "This path is still folded. The branch is ready for the next piece.",
                    )));
                    AppAction::Render
                }
            },
            Screen::Rules if self.selection == 0 => {
                self.screen = Screen::Branch;
                self.selection = 5;
                AppAction::Render
            }
            Screen::Rules => {
                self.overlay = Some(Overlay::Error(SafeText::internal(
                    "The interactive paper lesson arrives with the puzzle screen.",
                )));
                AppAction::Render
            }
            Screen::Settings if self.selection == SETTING_CHOICE_COUNT - 1 => {
                self.screen = Screen::Branch;
                self.selection = 6;
                AppAction::Render
            }
            Screen::Settings => self.change_setting(1),
        }
    }

    fn change_setting(&mut self, direction: isize) -> AppAction {
        match self.selection {
            0 => {
                self.settings.color_mode = cycle_color(self.settings.color_mode, direction);
            }
            1 => {
                self.settings.glyph_mode = match self.settings.glyph_mode {
                    GlyphMode::Unicode => GlyphMode::Ascii,
                    GlyphMode::Ascii => GlyphMode::Unicode,
                };
            }
            2 => self.settings.reduced_motion = !self.settings.reduced_motion,
            3 => self.settings.instant_reveal = !self.settings.instant_reveal,
            _ => return AppAction::None,
        }
        if self.settings.reduced_motion || self.settings.instant_reveal {
            self.reveal_started = None;
        }
        AppAction::SaveSettings(self.settings)
    }

    fn back(&mut self) -> AppAction {
        if self.screen == Screen::Branch {
            self.overlay = Some(Overlay::Quit);
        } else {
            let prior_screen = self.screen;
            self.screen = Screen::Branch;
            self.selection = match prior_screen {
                Screen::Rules => 5,
                Screen::Settings => 6,
                Screen::Branch => 0,
            };
        }
        AppAction::Render
    }
}

fn wrap_selection(current: usize, direction: isize, count: usize) -> usize {
    debug_assert!(count > 0);
    if direction < 0 {
        current.checked_sub(1).unwrap_or(count - 1)
    } else {
        (current + 1) % count
    }
}

fn cycle_color(current: ColorMode, direction: isize) -> ColorMode {
    let index = match current {
        ColorMode::Auto => 0,
        ColorMode::Color => 1,
        ColorMode::Monochrome => 2,
    };
    match wrap_selection(index, direction, 3) {
        0 => ColorMode::Auto,
        1 => ColorMode::Color,
        _ => ColorMode::Monochrome,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn focus_wraps_and_overlays_take_input_priority() {
        let mut app = App::new(Settings::default(), Instant::now());
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selection(), BRANCH_CHOICE_COUNT - 1);

        app.handle_key(key(KeyCode::Char('?')));
        let selected = app.selection();
        assert!(matches!(app.overlay(), Some(Overlay::Help)));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selection(), selected);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.overlay().is_none());
    }

    #[test]
    fn settings_changes_are_returned_for_durable_storage() {
        let mut app = App::new(Settings::default(), Instant::now());
        for _ in 0..6 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen(), Screen::Settings);

        let AppAction::SaveSettings(settings) = app.handle_key(key(KeyCode::Right)) else {
            panic!("a settings change should be persisted");
        };
        assert_eq!(settings.color_mode, ColorMode::Color);
        assert_eq!(settings.glyph_mode, GlyphMode::Unicode);
    }

    #[test]
    fn any_key_finishes_the_bounded_mark_reveal() {
        let started = Instant::now();
        let mut app = App::new(Settings::default(), started);
        assert!(app.animation_active(started));

        let action = app.handle_key(key(KeyCode::Char('x')));

        assert!(!app.animation_active(started));
        assert_eq!(app.mark_frame(started), MARK_FRAME_COUNT - 1);
        assert_eq!(action, AppAction::Render);
    }

    #[test]
    fn reduced_motion_and_instant_modes_start_with_complete_information() {
        for settings in [
            Settings {
                reduced_motion: true,
                ..Settings::default()
            },
            Settings {
                instant_reveal: true,
                ..Settings::default()
            },
        ] {
            let now = Instant::now();
            let app = App::new(settings, now);
            assert!(!app.animation_active(now));
            assert_eq!(app.mark_frame(now), MARK_FRAME_COUNT - 1);
        }
    }

    #[test]
    fn mark_animation_advances_monotonically_and_finishes_at_its_limit() {
        let started = Instant::now();
        let app = App::new(Settings::default(), started);
        let halfway = started + MARK_REVEAL_LIMIT / 2;
        let finished = started + MARK_REVEAL_LIMIT;

        assert_eq!(app.mark_frame(started), 0);
        assert!(app.mark_frame(halfway) > app.mark_frame(started));
        assert!(app.mark_frame(halfway) < MARK_FRAME_COUNT - 1);
        assert_eq!(app.mark_frame(finished), MARK_FRAME_COUNT - 1);
        assert!(!app.animation_active(finished));
    }
}
