use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::content;
use crate::domain::attempt::Attempt;
use crate::domain::paper::PaperAction;
use crate::generator::{
    CURRENT_GENERATOR_COMPATIBILITY_VERSION, CalendarDate, GenerationOutcome, GenerationSeed,
};
use crate::packs::ValidatedPack;
use crate::storage::{
    ColorMode, DecodedReplay, GlyphMode, KeyBindings, ProgressPage, PuzzleProgress, RegisteredPack,
    Settings,
};

use super::session::{PlaySession, PlaySource, SessionEvent};
use super::text::SafeText;

pub(crate) const MARK_FRAME_COUNT: usize = 24;
const MARK_REVEAL_LIMIT: std::time::Duration = std::time::Duration::from_millis(1_100);
const BRANCH_CHOICE_COUNT: usize = 7;
const SETTING_CHOICE_COUNT: usize = 12;
const WALKTHROUGH_FRAME_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Screen {
    Capabilities,
    Branch,
    Journey,
    Play,
    Packs,
    PackPuzzles,
    Keepsakes,
    HowTo,
    Settings,
    Loading,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Overlay {
    Help(Box<[SafeText]>),
    Quit,
    Reset,
    Export([SafeText; 3]),
    Error(SafeText),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    None,
    Render,
    SaveSettings(Settings),
    SaveCompletion,
    StartGeneration {
        pack_id: &'static str,
        seed: GenerationSeed,
        source: PlaySource,
    },
    CancelGeneration,
    LoadPack(Box<str>),
    LoadReplay {
        pack_id: Box<str>,
        puzzle_id: Box<str>,
    },
    LoadKeepsakes(u64),
    Exit,
}

pub(crate) struct App {
    screen: Screen,
    overlay: Option<Overlay>,
    selection: usize,
    settings: Settings,
    focused: bool,
    reveal_started: Option<Instant>,
    journey: Vec<content::BuiltInPaper>,
    journey_done: Vec<bool>,
    recent: Vec<PuzzleProgress>,
    keepsake_offset: u64,
    keepsake_has_more: bool,
    packs: Vec<RegisteredPack>,
    pack_papers: Vec<PackPaper>,
    session: Option<PlaySession>,
    walkthrough_step: usize,
    binding_capture: Option<usize>,
    local_date: CalendarDate,
    endless_seed: u64,
    pending_generation: Option<(u64, PlaySource)>,
    group_completion: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct PackPaper {
    pub(crate) puzzle: crate::domain::puzzle::Puzzle,
    pub(crate) title: Box<str>,
    pub(crate) description: Box<str>,
    pub(crate) cues: Vec<Box<str>>,
}

impl App {
    #[cfg(test)]
    pub(crate) fn new(settings: Settings, now: Instant) -> Self {
        let journey_done = vec![false; content::journey().len()];
        Self::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            Vec::new(),
            journey_done,
            CalendarDate::new(2026, 1, 1).expect("fixed test date"),
            1,
            now,
        )
    }

    pub(crate) fn with_state(
        settings: Settings,
        progress: ProgressPage,
        packs: Vec<RegisteredPack>,
        journey_done: Vec<bool>,
        local_date: CalendarDate,
        endless_seed: u64,
        now: Instant,
    ) -> Self {
        let first_launch = !settings.lesson_complete;
        let reveal_started = (!settings.reduced_motion && !settings.instant_reveal).then_some(now);
        let journey = content::journey();
        assert_eq!(
            journey_done.len(),
            journey.len(),
            "journey progress must cover the built-in catalog"
        );
        Self {
            screen: if first_launch {
                Screen::Capabilities
            } else {
                Screen::Branch
            },
            overlay: None,
            selection: 0,
            settings,
            focused: true,
            reveal_started,
            journey,
            journey_done,
            recent: progress.entries,
            keepsake_offset: 0,
            keepsake_has_more: progress.has_more,
            packs,
            pack_papers: Vec::new(),
            session: None,
            walkthrough_step: 0,
            binding_capture: None,
            local_date,
            endless_seed,
            pending_generation: None,
            group_completion: None,
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

    pub(crate) fn journey(&self) -> &[content::BuiltInPaper] {
        &self.journey
    }

    pub(crate) fn recent(&self) -> &[PuzzleProgress] {
        &self.recent
    }

    pub(crate) const fn keepsake_offset(&self) -> u64 {
        self.keepsake_offset
    }

    pub(crate) const fn keepsake_has_more(&self) -> bool {
        self.keepsake_has_more
    }

    pub(crate) fn keepsakes_loaded(&mut self, page: ProgressPage, offset: u64) {
        self.recent = page.entries;
        self.keepsake_offset = offset;
        self.keepsake_has_more = page.has_more;
        self.selection = 0;
    }

    pub(crate) fn packs(&self) -> &[RegisteredPack] {
        &self.packs
    }

    pub(crate) fn pack_papers(&self) -> &[PackPaper] {
        &self.pack_papers
    }

    pub(crate) const fn session(&self) -> Option<&PlaySession> {
        self.session.as_ref()
    }

    pub(crate) const fn binding_capture(&self) -> Option<usize> {
        self.binding_capture
    }

    pub(crate) const fn local_date(&self) -> CalendarDate {
        self.local_date
    }

    pub(crate) const fn set_local_date(&mut self, date: CalendarDate) {
        self.local_date = date;
    }

    pub(crate) fn journey_complete(&self, index: usize) -> bool {
        self.journey_done.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn journey_unlocked(&self, index: usize) -> bool {
        self.journey_done.get(index).is_some()
            && (self.journey_complete(index) || index == 0 || self.journey_complete(index - 1))
    }

    pub(crate) fn completed_group_count(&self) -> usize {
        content::journey_groups()
            .iter()
            .take_while(|group| {
                (group.first_paper..group.first_paper + group.paper_count)
                    .all(|index| self.journey_complete(index))
            })
            .count()
    }

    pub(crate) fn group_completion(&self) -> Option<&'static content::JourneyGroup> {
        self.group_completion
            .and_then(|index| content::journey_groups().get(index))
    }

    pub(crate) fn walkthrough(&self) -> (content::BuiltInPaper, Attempt, usize, usize) {
        let paper = content::lesson();
        let total = WALKTHROUGH_FRAME_COUNT;
        let actions = match self.walkthrough_step {
            0 | 1 => 0,
            2 | 3 => 1,
            _ => paper.solution().len(),
        };
        let mut attempt = paper.puzzle().start();
        for &action in paper.solution().iter().take(actions) {
            attempt
                .apply(action)
                .expect("the recorded teaching sequence is engine-verified");
        }
        (paper, attempt, self.walkthrough_step, total)
    }

    pub(crate) fn animation_active(&self, now: Instant) -> bool {
        self.reveal_started
            .is_some_and(|started| now.saturating_duration_since(started) < MARK_REVEAL_LIMIT)
            || self
                .session
                .as_ref()
                .is_some_and(|session| session.animation_active(now))
    }

    pub(crate) fn mark_frame(&self, now: Instant) -> usize {
        let Some(started) = self.reveal_started else {
            return MARK_FRAME_COUNT - 1;
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed >= MARK_REVEAL_LIMIT {
            return MARK_FRAME_COUNT - 1;
        }
        (elapsed.as_millis() as usize).saturating_mul(MARK_FRAME_COUNT - 1)
            / MARK_REVEAL_LIMIT.as_millis() as usize
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent, now: Instant) -> AppAction {
        let reveal_cancelled = self.reveal_started.take().is_some();
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return AppAction::Exit;
        }
        if self.binding_capture.is_some() {
            return self.capture_binding(key.code);
        }
        if self.overlay.is_some() {
            return self.handle_overlay_key(key.code);
        }
        if matches!(key.code, KeyCode::Char(character) if character == self.settings.bindings.help)
        {
            let help = self
                .help_lines()
                .into_iter()
                .map(|line| SafeText::external_display(&line, 96, self.settings.glyph_mode))
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.overlay = Some(Overlay::Help(help));
            return AppAction::Render;
        }
        if matches!(key.code, KeyCode::Char(character) if character == self.settings.bindings.quit)
        {
            self.overlay = Some(Overlay::Quit);
            return AppAction::Render;
        }

        let action = match self.screen {
            Screen::Play => {
                let Some(session) = self.session.as_mut() else {
                    return self.internal_error("The active paper is unavailable.");
                };
                let event = session.handle_key(
                    key,
                    self.settings.bindings,
                    now,
                    self.settings.reduced_motion || self.settings.instant_reveal,
                );
                if self.group_completion.is_some() && session.result().is_none() {
                    self.group_completion = None;
                }
                self.handle_session_event(event)
            }
            Screen::Capabilities => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.open_lesson();
                    AppAction::Render
                }
                KeyCode::Esc => {
                    self.overlay = Some(Overlay::Quit);
                    AppAction::Render
                }
                _ => AppAction::None,
            },
            Screen::Loading => match key.code {
                KeyCode::Esc => AppAction::CancelGeneration,
                _ => AppAction::None,
            },
            _ => self.handle_menu_key(key.code),
        };
        if reveal_cancelled && action == AppAction::None {
            AppAction::Render
        } else {
            action
        }
    }

    pub(crate) fn handle_tick(&mut self, now: Instant) -> AppAction {
        let mut render = false;
        if let Some(started) = self.reveal_started {
            if now.saturating_duration_since(started) >= MARK_REVEAL_LIMIT {
                self.reveal_started = None;
            }
            render = true;
        }
        if let Some(session) = self.session.as_mut()
            && session.tick(now) == SessionEvent::Render
        {
            render = true;
        }
        if render {
            AppAction::Render
        } else {
            AppAction::None
        }
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

    pub(crate) fn settings_saved(&mut self, settings: Settings) {
        self.settings = settings;
        if let Some(session) = self.session.as_mut()
            && matches!(session.source(), PlaySource::Lesson)
        {
            session.mark_saved();
        }
    }

    pub(crate) fn completion_request(
        &self,
    ) -> Option<(
        crate::domain::puzzle::Puzzle,
        crate::domain::replay::Replay,
        PlaySource,
        u64,
        bool,
    )> {
        let session = self.session.as_ref()?;
        Some((
            session.puzzle().clone(),
            session.replay(),
            session.source().clone(),
            session.attempt().undo_count(),
            session.attempt().hints_used(),
        ))
    }

    pub(crate) fn completion_saved(&mut self, progress: PuzzleProgress) {
        for (index, paper) in self.journey.iter().enumerate() {
            if paper.puzzle().identity().pack_id() == progress.pack_id.as_ref()
                && paper.puzzle().identity().puzzle_id() == progress.puzzle_id.as_ref()
            {
                let was_complete = self.journey_done[index];
                self.journey_done[index] = true;
                if !was_complete
                    && let Some((group_index, group)) = content::journey_group(index)
                    && index + 1 == group.first_paper + group.paper_count
                    && (group.first_paper..group.first_paper + group.paper_count)
                        .all(|paper_index| self.journey_done[paper_index])
                {
                    self.group_completion = Some(group_index);
                }
            }
        }
        if self.keepsake_offset == 0 {
            let page_was_full = self.recent.len() == crate::storage::PROGRESS_PAGE_SIZE;
            let was_on_page = self.recent.iter().any(|existing| {
                existing.pack_id == progress.pack_id && existing.puzzle_id == progress.puzzle_id
            });
            self.recent.retain(|existing| {
                existing.pack_id != progress.pack_id || existing.puzzle_id != progress.puzzle_id
            });
            self.recent.insert(0, progress);
            self.recent.truncate(crate::storage::PROGRESS_PAGE_SIZE);
            self.keepsake_has_more |= page_was_full && !was_on_page;
        }
        if let Some(session) = self.session.as_mut() {
            session.mark_saved();
        }
    }

    pub(crate) fn generation_started(&mut self, id: u64, source: PlaySource) {
        self.pending_generation = Some((id, source));
    }

    pub(crate) fn generation_cancelled(&mut self) {
        self.pending_generation = None;
        self.screen = Screen::Branch;
        self.selection = 0;
    }

    pub(crate) fn generation_finished(
        &mut self,
        id: u64,
        outcome: GenerationOutcome,
    ) -> Option<(CalendarDate, u16, crate::domain::puzzle::Puzzle)> {
        let (expected, source) = self.pending_generation.take()?;
        if id != expected {
            self.show_error(&"A finished generation job did not match the active request.");
            self.screen = Screen::Branch;
            return None;
        }
        match outcome {
            GenerationOutcome::Generated { puzzle, .. } => {
                let generated = puzzle.puzzle().clone();
                let title = match source {
                    PlaySource::Daily { .. } => "Today's paper",
                    PlaySource::Endless => "Endless paper",
                    _ => "Generated paper",
                };
                let daily = match source {
                    PlaySource::Daily {
                        day,
                        generator_version,
                    } => Some((day, generator_version, generated.clone())),
                    _ => None,
                };
                self.session = Some(PlaySession::new(
                    &generated,
                    title,
                    "A deterministic paper made entirely on this device.",
                    vec!["Use the target, then work through the production paper engine.".into()],
                    source,
                ));
                self.screen = Screen::Play;
                self.selection = 0;
                daily
            }
            GenerationOutcome::Exhausted { .. } => {
                self.show_error(
                    &"The bounded generator exhausted its search. Try another endless paper.",
                );
                self.screen = Screen::Branch;
                None
            }
            GenerationOutcome::Cancelled { .. } => {
                self.screen = Screen::Branch;
                None
            }
            GenerationOutcome::Invalid(error) => {
                self.show_error(&error);
                self.screen = Screen::Branch;
                None
            }
        }
    }

    pub(crate) fn open_pack(&mut self, pack: &ValidatedPack) {
        self.pack_papers = pack
            .puzzles()
            .iter()
            .map(|content| PackPaper {
                puzzle: content.puzzle().clone(),
                title: content.title().into(),
                description: content
                    .description()
                    .unwrap_or("A paper from an installed pack.")
                    .into(),
                cues: content.tutorial_cues().to_vec(),
            })
            .collect();
        self.screen = Screen::PackPuzzles;
        self.selection = 0;
    }

    pub(crate) fn open_replay(&mut self, decoded: &DecodedReplay) {
        let title = self.replay_title(decoded.puzzle());
        match PlaySession::from_replay(decoded.puzzle(), decoded.replay(), title) {
            Ok(session) => {
                self.group_completion = None;
                self.session = Some(session);
                self.screen = Screen::Play;
                self.selection = 0;
            }
            Err(error) => self.show_error(&error),
        }
    }

    fn replay_title(&self, puzzle: &crate::domain::puzzle::Puzzle) -> Box<str> {
        let identity = puzzle.identity();
        self.session
            .as_ref()
            .filter(|session| session.puzzle().identity() == identity)
            .map(|session| session.title().into())
            .or_else(|| {
                self.journey
                    .iter()
                    .find(|paper| paper.puzzle().identity() == identity)
                    .map(|paper| paper.title().into())
            })
            .or_else(|| {
                self.pack_papers
                    .iter()
                    .find(|paper| paper.puzzle.identity() == identity)
                    .map(|paper| paper.title.clone())
            })
            .unwrap_or_else(|| identity.puzzle_id().into())
    }

    fn handle_menu_key(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Esc => self.back(),
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Left | KeyCode::Char('h') => self.adjust_or_walk(-1),
            KeyCode::Right | KeyCode::Char('l') => self.adjust_or_walk(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            _ => AppAction::None,
        }
    }

    fn handle_overlay_key(&mut self, code: KeyCode) -> AppAction {
        match self.overlay.as_ref() {
            Some(Overlay::Quit) if matches!(code, KeyCode::Char('y') | KeyCode::Enter) => {
                AppAction::Exit
            }
            Some(Overlay::Reset) if matches!(code, KeyCode::Char('y') | KeyCode::Enter) => {
                if let Some(session) = self.session.as_mut() {
                    session.reset();
                }
                self.overlay = None;
                AppAction::Render
            }
            Some(Overlay::Quit | Overlay::Reset)
                if matches!(code, KeyCode::Char('n') | KeyCode::Esc)
                    || matches!(
                        (self.overlay.as_ref(), code),
                        (Some(Overlay::Quit), KeyCode::Char(character))
                            if character == self.settings.bindings.quit
                    ) =>
            {
                self.overlay = None;
                AppAction::Render
            }
            Some(Overlay::Help(_) | Overlay::Error(_) | Overlay::Export(_))
                if matches!(code, KeyCode::Esc | KeyCode::Enter)
                    || matches!(
                        (self.overlay.as_ref(), code),
                        (Some(Overlay::Help(_)), KeyCode::Char(character))
                            if character == self.settings.bindings.help
                    ) =>
            {
                self.overlay = None;
                AppAction::Render
            }
            Some(_) | None => AppAction::None,
        }
    }

    fn handle_session_event(&mut self, event: SessionEvent) -> AppAction {
        match event {
            SessionEvent::None => AppAction::None,
            SessionEvent::Render => AppAction::Render,
            SessionEvent::ConfirmReset => {
                self.overlay = Some(Overlay::Reset);
                AppAction::Render
            }
            SessionEvent::Save => {
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| matches!(session.source(), PlaySource::Lesson))
                {
                    self.settings.lesson_complete = true;
                    AppAction::SaveSettings(self.settings)
                } else {
                    AppAction::SaveCompletion
                }
            }
            SessionEvent::Back => {
                self.session = None;
                self.group_completion = None;
                self.screen = Screen::Branch;
                self.selection = 0;
                AppAction::Render
            }
            SessionEvent::Replay => {
                let Some(session) = self.session.as_ref() else {
                    return AppAction::None;
                };
                self.group_completion = None;
                AppAction::LoadReplay {
                    pack_id: session.puzzle().identity().pack_id().into(),
                    puzzle_id: session.puzzle().identity().puzzle_id().into(),
                }
            }
            SessionEvent::Export(lines) => {
                self.overlay = Some(Overlay::Export(lines.map(|line| {
                    SafeText::external_display(&line, 160, self.settings.glyph_mode)
                })));
                AppAction::Render
            }
            SessionEvent::Error(error) => {
                self.show_error(&error);
                AppAction::Render
            }
        }
    }

    fn move_selection(&mut self, direction: isize) -> AppAction {
        let count = self.selection_count();
        if count > 0 {
            self.selection = wrap_selection(self.selection, direction, count);
        }
        AppAction::Render
    }

    fn selection_count(&self) -> usize {
        match self.screen {
            Screen::Branch => BRANCH_CHOICE_COUNT,
            Screen::Journey => self.journey.len() + 1,
            Screen::Packs => self.packs.len() + 1,
            Screen::PackPuzzles => self.pack_papers.len() + 1,
            Screen::Keepsakes => {
                self.recent.len()
                    + usize::from(self.keepsake_has_more)
                    + usize::from(self.keepsake_offset > 0)
                    + 1
            }
            Screen::Settings => SETTING_CHOICE_COUNT,
            Screen::HowTo => 2,
            Screen::Capabilities | Screen::Play | Screen::Loading => 1,
        }
    }

    fn adjust_or_walk(&mut self, direction: isize) -> AppAction {
        if self.screen == Screen::Settings && self.selection < 4 {
            return self.change_setting(direction);
        }
        if self.screen == Screen::HowTo {
            let total = WALKTHROUGH_FRAME_COUNT;
            self.walkthrough_step = wrap_selection(self.walkthrough_step, direction, total);
            return AppAction::Render;
        }
        AppAction::None
    }

    fn activate(&mut self) -> AppAction {
        match self.screen {
            Screen::Branch => self.activate_branch(),
            Screen::Journey => self.activate_journey(),
            Screen::Packs => {
                if self.selection == self.packs.len() {
                    self.back()
                } else {
                    AppAction::LoadPack(self.packs[self.selection].id.clone())
                }
            }
            Screen::PackPuzzles => self.activate_pack_paper(),
            Screen::Keepsakes => {
                if self.selection < self.recent.len() {
                    let progress = &self.recent[self.selection];
                    AppAction::LoadReplay {
                        pack_id: progress.pack_id.clone(),
                        puzzle_id: progress.puzzle_id.clone(),
                    }
                } else {
                    let mut index = self.recent.len();
                    if self.keepsake_has_more && self.selection == index {
                        return AppAction::LoadKeepsakes(
                            self.keepsake_offset
                                .saturating_add(crate::storage::PROGRESS_PAGE_SIZE as u64),
                        );
                    }
                    index += usize::from(self.keepsake_has_more);
                    if self.keepsake_offset > 0 && self.selection == index {
                        return AppAction::LoadKeepsakes(
                            self.keepsake_offset
                                .saturating_sub(crate::storage::PROGRESS_PAGE_SIZE as u64),
                        );
                    }
                    self.back()
                }
            }
            Screen::HowTo if self.selection == 1 => self.back(),
            Screen::HowTo => {
                let total = WALKTHROUGH_FRAME_COUNT;
                self.walkthrough_step = (self.walkthrough_step + 1) % total;
                AppAction::Render
            }
            Screen::Settings if self.selection == SETTING_CHOICE_COUNT - 1 => self.back(),
            Screen::Settings if self.selection < 4 => self.change_setting(1),
            Screen::Settings => {
                self.binding_capture = Some(self.selection - 4);
                AppAction::Render
            }
            Screen::Capabilities | Screen::Play | Screen::Loading => AppAction::None,
        }
    }

    fn activate_branch(&mut self) -> AppAction {
        match self.selection {
            0 => {
                self.screen = Screen::Journey;
                self.selection = 0;
                AppAction::Render
            }
            1 => {
                let source = PlaySource::Daily {
                    day: self.local_date,
                    generator_version: CURRENT_GENERATOR_COMPATIBILITY_VERSION,
                };
                self.screen = Screen::Loading;
                AppAction::StartGeneration {
                    pack_id: "orifude-daily",
                    seed: GenerationSeed::for_date(self.local_date),
                    source,
                }
            }
            2 => {
                self.endless_seed = self.endless_seed.wrapping_add(1);
                let source = PlaySource::Endless;
                self.screen = Screen::Loading;
                AppAction::StartGeneration {
                    pack_id: "orifude-endless",
                    seed: GenerationSeed::current(self.endless_seed),
                    source,
                }
            }
            3 => {
                self.screen = Screen::Packs;
                self.selection = 0;
                AppAction::Render
            }
            4 => {
                self.screen = Screen::Keepsakes;
                self.selection = 0;
                AppAction::LoadKeepsakes(0)
            }
            5 => {
                self.screen = Screen::HowTo;
                self.selection = 0;
                self.walkthrough_step = 0;
                AppAction::Render
            }
            _ => {
                self.screen = Screen::Settings;
                self.selection = 0;
                AppAction::Render
            }
        }
    }

    fn activate_journey(&mut self) -> AppAction {
        if self.selection == self.journey.len() {
            return self.back();
        }
        if !self.journey_unlocked(self.selection) {
            return self.internal_error("That paper unlocks after the previous one is complete.");
        }
        let paper = &self.journey[self.selection];
        self.session = Some(PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            paper.cues().to_vec(),
            PlaySource::Journey(self.selection),
        ));
        self.screen = Screen::Play;
        self.selection = 0;
        AppAction::Render
    }

    fn activate_pack_paper(&mut self) -> AppAction {
        if self.selection == self.pack_papers.len() {
            return self.back();
        }
        let paper = self.pack_papers[self.selection].clone();
        self.session = Some(PlaySession::new(
            &paper.puzzle,
            paper.title,
            paper.description,
            paper.cues,
            PlaySource::Pack,
        ));
        self.screen = Screen::Play;
        self.selection = 0;
        AppAction::Render
    }

    fn open_lesson(&mut self) {
        let paper = content::lesson();
        self.session = Some(PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            paper.cues().to_vec(),
            PlaySource::Lesson,
        ));
        self.screen = Screen::Play;
        self.selection = 0;
    }

    fn change_setting(&mut self, direction: isize) -> AppAction {
        match self.selection {
            0 => self.settings.color_mode = cycle_color(self.settings.color_mode, direction),
            1 => {
                self.settings.glyph_mode = match self.settings.glyph_mode {
                    GlyphMode::Unicode => GlyphMode::Ascii,
                    GlyphMode::Ascii => GlyphMode::Unicode,
                }
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

    fn capture_binding(&mut self, code: KeyCode) -> AppAction {
        let Some(binding) = self.binding_capture.take() else {
            return AppAction::None;
        };
        if matches!(code, KeyCode::Esc) {
            return AppAction::Render;
        }
        let KeyCode::Char(character) = code else {
            return self.internal_error("Bindings use one key character.");
        };
        let mut bindings = self.settings.bindings;
        *binding_slot(&mut bindings, binding) = character;
        if !bindings.is_conflict_free() {
            return self.internal_error(
                "That key is already used or reserved for movement or result actions.",
            );
        }
        self.settings.bindings = bindings;
        AppAction::SaveSettings(self.settings)
    }

    fn back(&mut self) -> AppAction {
        match self.screen {
            Screen::Branch | Screen::Capabilities => {
                self.overlay = Some(Overlay::Quit);
            }
            Screen::PackPuzzles => {
                self.screen = Screen::Packs;
                self.pack_papers.clear();
                self.selection = 0;
            }
            Screen::Loading => return AppAction::CancelGeneration,
            _ => {
                self.screen = Screen::Branch;
                self.selection = 0;
            }
        }
        AppAction::Render
    }

    fn internal_error(&mut self, message: &'static str) -> AppAction {
        self.overlay = Some(Overlay::Error(SafeText::internal(message)));
        AppAction::Render
    }

    fn help_lines(&self) -> Vec<String> {
        let keys = self.settings.bindings;
        match self.screen {
            Screen::Capabilities => vec![
                "Learn by doing".to_owned(),
                "Enter starts with a ready fold.".to_owned(),
                "Arrows move @. Enter uses the ready tool.".to_owned(),
                "Tab changes tools. Esc readies Open paper.".to_owned(),
                "Fold: every + cell crosses its crease.".to_owned(),
                "Brush: ink reaches every layer in its preview.".to_owned(),
            ],
            Screen::Play
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.replay_progress().is_some()) =>
            {
                vec![
                    "Saved replay".to_owned(),
                    "Enter / Right      Show the next saved action".to_owned(),
                    "Left               Rewind one action".to_owned(),
                    "After the last action, Enter opens the paper.".to_owned(),
                    "Up / Down          Inspect opened comparison rows".to_owned(),
                    format!("{} restart   x keepsake   Esc back", keys.reset),
                ]
            }
            Screen::Play => vec![
                "Move and act".to_owned(),
                "Arrows / h j k l   Move @ or choose a fold".to_owned(),
                "Enter              Use the ready tool or open".to_owned(),
                "Tab / Shift+Tab   Change tool; Esc readies Open".to_owned(),
                "Goal, tools, and result".to_owned(),
                "Pattern to match   The opened result, not the moves".to_owned(),
                format!("{} Fold    + crosses a crease and stacks on top", keys.fold),
                format!(
                    "{} Brush   Dot or line inks each previewed stack",
                    keys.brush
                ),
                format!(
                    "{} preview   {} undo   {} reset   t boards",
                    key_label(keys.preview),
                    keys.undo,
                    keys.reset
                ),
                "Result   ? missing · ! extra · score is a guide".to_owned(),
            ],
            Screen::Settings => vec![
                "Settings".to_owned(),
                "Up / Down          Choose a setting".to_owned(),
                "Left / Right       Change a display choice".to_owned(),
                "Enter              Capture one unused key".to_owned(),
                "Space may be used for preview.".to_owned(),
                "h j k l t v x stay reserved for play.".to_owned(),
            ],
            Screen::HowTo => vec![
                "How to play".to_owned(),
                "Left / Right       Previous / next frame".to_owned(),
                "Enter              Next frame".to_owned(),
                "Esc                Return to the branch".to_owned(),
            ],
            _ => vec![
                "Navigation".to_owned(),
                "Up / Down or j / k   Move through choices".to_owned(),
                "Enter                Open the selected path".to_owned(),
                "Esc                  Return".to_owned(),
                format!("{} help   {} quit", keys.help, keys.quit),
            ],
        }
    }
}

pub(crate) fn key_label(key: char) -> String {
    if key == ' ' {
        "Space".to_owned()
    } else {
        key.to_string()
    }
}

fn binding_slot(bindings: &mut KeyBindings, index: usize) -> &mut char {
    match index {
        0 => &mut bindings.fold,
        1 => &mut bindings.brush,
        2 => &mut bindings.undo,
        3 => &mut bindings.reset,
        4 => &mut bindings.preview,
        5 => &mut bindings.help,
        _ => &mut bindings.quit,
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

pub(crate) fn action_label(action: PaperAction) -> String {
    match action {
        PaperAction::Fold(fold) => super::session::fold_label(fold),
        PaperAction::Dot(coordinate) => format!(
            "Dot at row {}, column {}",
            coordinate.row().get() + 1,
            coordinate.column().get() + 1
        ),
        PaperAction::Line(line) => format!(
            "Line from {},{} to {},{}",
            line.start().row().get() + 1,
            line.start().column().get() + 1,
            line.end().row().get() + 1,
            line.end().column().get() + 1
        ),
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
    fn first_launch_enters_the_engine_backed_lesson() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        assert_eq!(app.screen(), Screen::Capabilities);
        app.handle_key(key(KeyCode::Enter), now);
        assert_eq!(app.screen(), Screen::Play);
        assert!(matches!(
            app.session().map(PlaySession::source),
            Some(PlaySource::Lesson)
        ));
    }

    #[test]
    fn official_replays_keep_the_paper_title() {
        let app = App::new(Settings::default(), Instant::now());
        let paper = content::journey().remove(0);

        assert_eq!(app.replay_title(paper.puzzle()).as_ref(), paper.title());
    }

    #[test]
    fn journey_lock_follows_durable_progress() {
        let now = Instant::now();
        let first = content::journey().remove(0);
        let progress = PuzzleProgress {
            pack_id: first.puzzle().identity().pack_id().into(),
            puzzle_id: first.puzzle().identity().puzzle_id().into(),
            attempt_count: 1,
            best_folds: 0,
            best_strokes: 1,
            best_replay_id: 1,
            updated_at_unix_seconds: 1,
        };
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut journey_done = vec![false; content::journey().len()];
        journey_done[0] = true;
        let app = App::with_state(
            settings,
            ProgressPage {
                entries: vec![progress],
                has_more: false,
            },
            Vec::new(),
            journey_done,
            CalendarDate::new(2026, 9, 2).unwrap(),
            1,
            now,
        );
        assert!(app.journey_unlocked(1));
        assert!(!app.journey_unlocked(2));
    }

    #[test]
    fn completed_journey_papers_remain_open_after_catalog_growth() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut journey_done = vec![false; content::journey().len()];
        journey_done[5] = true;
        journey_done[10] = true;
        let app = App::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            Vec::new(),
            journey_done,
            CalendarDate::new(2026, 9, 2).unwrap(),
            1,
            now,
        );

        assert!(app.journey_unlocked(5));
        assert!(app.journey_unlocked(10));
        assert!(!app.journey_unlocked(4));
    }

    #[test]
    fn finishing_a_group_announces_its_gift_only_once() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        };
        let mut journey_done = vec![false; content::journey().len()];
        journey_done[..4].fill(true);
        let mut app = App::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            Vec::new(),
            journey_done,
            CalendarDate::new(2026, 9, 3).unwrap(),
            1,
            now,
        );
        let last = &app.journey()[4];
        let progress = PuzzleProgress {
            pack_id: last.puzzle().identity().pack_id().into(),
            puzzle_id: last.puzzle().identity().puzzle_id().into(),
            attempt_count: 1,
            best_folds: 0,
            best_strokes: 3,
            best_replay_id: 1,
            updated_at_unix_seconds: 1,
        };

        app.completion_saved(progress.clone());
        assert_eq!(app.completed_group_count(), 1);
        assert_eq!(
            app.group_completion().map(|group| group.title),
            Some("Ink on paper")
        );

        app.group_completion = None;
        app.completion_saved(progress);
        assert!(app.group_completion().is_none());
    }

    #[test]
    fn replay_request_consumes_the_group_completion_card() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        app.screen = Screen::Journey;
        assert_eq!(app.activate_journey(), AppAction::Render);
        app.group_completion = Some(0);

        assert!(matches!(
            app.handle_session_event(SessionEvent::Replay),
            AppAction::LoadReplay { .. }
        ));
        assert!(app.group_completion().is_none());
    }

    #[test]
    fn reduced_motion_starts_without_an_opening_animation() {
        let now = Instant::now();
        let app = App::new(
            Settings {
                reduced_motion: true,
                ..Settings::default()
            },
            now,
        );

        assert!(!app.animation_active(now));
        assert_eq!(app.mark_frame(now), MARK_FRAME_COUNT - 1);
    }

    #[test]
    fn conflicting_binding_is_rejected_without_changing_settings() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        app.screen = Screen::Settings;
        app.selection = 4;
        app.activate();
        let before = app.settings();
        app.handle_key(key(KeyCode::Char('h')), now);
        assert_eq!(app.settings(), before);
        assert!(matches!(app.overlay(), Some(Overlay::Error(_))));
    }

    #[test]
    fn help_and_focus_changes_preserve_the_active_draft() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(key(KeyCode::Enter), now);
        app.handle_key(key(KeyCode::Char('f')), now);
        let draft = app.session().and_then(PlaySession::draft);
        assert!(draft.is_some());

        app.handle_key(key(KeyCode::Char('?')), now);
        assert!(matches!(app.overlay(), Some(Overlay::Help(_))));
        app.set_focused(false);
        app.handle_key(key(KeyCode::Esc), now);

        assert_eq!(app.session().and_then(PlaySession::draft), draft);
        assert!(!app.focused());
    }

    #[test]
    fn reset_dialog_preserves_or_clears_the_attempt_only_after_a_choice() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        for code in [
            KeyCode::Enter,
            KeyCode::Char('f'),
            KeyCode::Enter,
            KeyCode::Char('r'),
        ] {
            app.handle_key(key(code), now);
        }
        assert!(matches!(app.overlay(), Some(Overlay::Reset)));
        assert_eq!(app.session().unwrap().attempt().action_count().get(), 1);

        app.handle_key(key(KeyCode::Char('n')), now);
        assert_eq!(app.session().unwrap().attempt().action_count().get(), 1);
        app.handle_key(key(KeyCode::Char('r')), now);
        app.handle_key(key(KeyCode::Char('y')), now);
        assert_eq!(app.session().unwrap().attempt().action_count().get(), 0);
    }

    #[test]
    fn any_key_finishes_the_bounded_mark_reveal() {
        let started = Instant::now();
        let mut app = App::new(Settings::default(), started);
        let action = app.handle_key(key(KeyCode::Char('x')), started);
        assert!(!app.animation_active(started));
        assert_eq!(app.mark_frame(started), MARK_FRAME_COUNT - 1);
        assert_eq!(action, AppAction::Render);
    }

    #[test]
    fn exhausted_generation_returns_a_visible_recoverable_error() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.screen = Screen::Loading;
        app.generation_started(7, PlaySource::Endless);
        app.generation_finished(
            7,
            GenerationOutcome::Exhausted {
                seed: GenerationSeed::current(12),
                last_rejection: crate::generator::CandidateRejection::Trivial,
                stats: crate::generator::GenerationStats::default(),
            },
        );

        assert_eq!(app.screen(), Screen::Branch);
        assert!(matches!(app.overlay(), Some(Overlay::Error(_))));
    }

    #[test]
    fn walkthrough_steps_through_engine_state_and_ends_on_a_solved_comparison() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        app.screen = Screen::HowTo;
        let expected_action_counts = [0, 0, 1, 1, 2, 2];
        for (step, expected_actions) in expected_action_counts.into_iter().enumerate() {
            let (_, attempt, actual_step, total) = app.walkthrough();
            assert_eq!(actual_step, step);
            assert_eq!(total, WALKTHROUGH_FRAME_COUNT);
            assert_eq!(usize::from(attempt.action_count().get()), expected_actions);
            if step + 1 == WALKTHROUGH_FRAME_COUNT {
                assert!(attempt.result().is_success());
            }
            if step + 1 < total {
                app.handle_key(key(KeyCode::Right), now);
            }
        }
    }

    #[test]
    fn overlay_shortcuts_toggle_the_dialog_they_opened() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);

        app.handle_key(key(KeyCode::Char('?')), now);
        assert!(matches!(app.overlay(), Some(Overlay::Help(_))));
        app.handle_key(key(KeyCode::Char('?')), now);
        assert!(app.overlay().is_none());

        app.handle_key(key(KeyCode::Char('q')), now);
        assert!(matches!(app.overlay(), Some(Overlay::Quit)));
        app.handle_key(key(KeyCode::Char('q')), now);
        assert!(app.overlay().is_none());
    }

    #[test]
    fn text_keepsake_sanitizes_each_structured_line_without_losing_breaks() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        let lines = [
            "Orifude - paper\u{1b}".to_owned(),
            "Solved in 0 fold(s) and 1 stroke(s).".to_owned(),
            "No solution actions included.".to_owned(),
        ];

        assert_eq!(
            app.handle_session_event(SessionEvent::Export(lines)),
            AppAction::Render
        );
        let Some(Overlay::Export(lines)) = app.overlay() else {
            panic!("export overlay is visible");
        };
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| !line.as_str().contains('\n')));
        assert!(lines.iter().all(|line| !line.as_str().contains('\u{1b}')));
    }

    #[test]
    fn saving_while_an_older_keepsake_page_is_cached_keeps_that_page_coherent() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let older = PuzzleProgress {
            pack_id: "old-pack".into(),
            puzzle_id: "old-paper".into(),
            attempt_count: 1,
            best_folds: 0,
            best_strokes: 1,
            best_replay_id: 1,
            updated_at_unix_seconds: 1,
        };
        let mut app = App::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: true,
            },
            Vec::new(),
            vec![false; content::journey().len()],
            CalendarDate::new(2026, 9, 2).unwrap(),
            1,
            now,
        );
        app.keepsakes_loaded(
            ProgressPage {
                entries: vec![older.clone()],
                has_more: false,
            },
            crate::storage::PROGRESS_PAGE_SIZE as u64,
        );
        let saved = PuzzleProgress {
            pack_id: "new-pack".into(),
            puzzle_id: "new-paper".into(),
            updated_at_unix_seconds: 2,
            ..older.clone()
        };

        app.completion_saved(saved);
        assert_eq!(
            app.keepsake_offset(),
            crate::storage::PROGRESS_PAGE_SIZE as u64
        );
        assert_eq!(app.recent(), &[older]);

        app.screen = Screen::Branch;
        app.selection = 4;
        assert_eq!(app.activate(), AppAction::LoadKeepsakes(0));
    }

    #[test]
    fn reveal_skip_is_consumed_before_lesson_acknowledgement() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(key(KeyCode::Enter), now);
        for code in [
            KeyCode::Char('f'),
            KeyCode::Enter,
            KeyCode::Char('j'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('b'),
            KeyCode::Enter,
        ] {
            app.handle_key(key(code), now);
        }
        let action = app.handle_key(key(KeyCode::Enter), now);
        let AppAction::SaveSettings(settings) = action else {
            panic!("a solved lesson saves its completion setting");
        };
        app.settings_saved(settings);

        assert_eq!(app.handle_key(key(KeyCode::Enter), now), AppAction::Render);
        assert_eq!(app.screen(), Screen::Play);
        assert_eq!(app.handle_key(key(KeyCode::Enter), now), AppAction::Render);
        assert_eq!(app.screen(), Screen::Branch);
    }

    #[test]
    fn completion_request_keeps_live_attempt_metadata() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        app.screen = Screen::Journey;
        app.activate_journey();
        for code in [
            KeyCode::Char('j'),
            KeyCode::Char('l'),
            KeyCode::Char('b'),
            KeyCode::Enter,
            KeyCode::Char('u'),
            KeyCode::Char('b'),
            KeyCode::Enter,
        ] {
            app.handle_key(key(code), now);
        }

        assert_eq!(
            app.handle_key(key(KeyCode::Enter), now),
            AppAction::SaveCompletion
        );
        let (_, replay, _, undo_count, hints_used) =
            app.completion_request().expect("completion request");
        assert_eq!(undo_count, 1);
        assert!(!hints_used);
        assert_eq!(replay.actions().len(), 1);
    }

    #[test]
    fn keepsake_navigation_requests_one_bounded_page_at_a_time() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let progress = PuzzleProgress {
            pack_id: "quiet-pack".into(),
            puzzle_id: "berry".into(),
            attempt_count: 1,
            best_folds: 0,
            best_strokes: 1,
            best_replay_id: 1,
            updated_at_unix_seconds: 1,
        };
        let mut app = App::with_state(
            settings,
            ProgressPage {
                entries: vec![progress],
                has_more: true,
            },
            Vec::new(),
            vec![false; content::journey().len()],
            CalendarDate::new(2026, 9, 2).unwrap(),
            1,
            now,
        );
        app.screen = Screen::Keepsakes;
        app.selection = 1;
        assert_eq!(
            app.activate(),
            AppAction::LoadKeepsakes(crate::storage::PROGRESS_PAGE_SIZE as u64)
        );

        app.keepsakes_loaded(
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            crate::storage::PROGRESS_PAGE_SIZE as u64,
        );
        assert_eq!(app.activate(), AppAction::LoadKeepsakes(0));
    }
}
