use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    BrushRule, Column, Coordinate, Fold, LineStroke, PaperAction, Row, StrokeAxis,
};
use crate::domain::puzzle::Puzzle;
use crate::domain::replay::Replay;
use crate::domain::score::AttemptResult;
use crate::generator::CalendarDate;
use crate::storage::KeyBindings;

const RESULT_REVEAL_LIMIT: Duration = Duration::from_millis(1_100);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaySource {
    Lesson,
    Journey(usize),
    Daily {
        day: CalendarDate,
        generator_version: u16,
    },
    Endless,
    Pack,
    Keepsake,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Draft {
    Fold(usize),
    Brush(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SessionEvent {
    None,
    Render,
    ConfirmReset,
    Save,
    Back,
    Replay,
    Export([String; 3]),
    Error(String),
}

pub(crate) struct PlaySession {
    title: Box<str>,
    description: Box<str>,
    cues: Box<[Box<str>]>,
    source: PlaySource,
    attempt: Attempt,
    cursor: Coordinate,
    draft: Option<Draft>,
    unfolded_preview: bool,
    target_visible: bool,
    result: Option<AttemptResult>,
    reveal_started: Option<Instant>,
    saved: bool,
}

pub(crate) struct RevealFrame {
    pub(crate) geometry: Attempt,
    pub(crate) opened_folds: usize,
    pub(crate) total_folds: usize,
    pub(crate) complete: bool,
}

impl PlaySession {
    pub(crate) fn new(
        puzzle: &Puzzle,
        title: impl Into<Box<str>>,
        description: impl Into<Box<str>>,
        cues: Vec<Box<str>>,
        source: PlaySource,
    ) -> Self {
        let cursor = puzzle
            .dimensions()
            .coordinate(0, 0)
            .expect("validated paper contains its origin");
        Self {
            title: title.into(),
            description: description.into(),
            cues: cues.into_boxed_slice(),
            source,
            attempt: puzzle.start(),
            cursor,
            draft: None,
            unfolded_preview: false,
            target_visible: false,
            result: None,
            reveal_started: None,
            saved: false,
        }
    }

    pub(crate) fn from_replay(
        puzzle: &Puzzle,
        replay: &Replay,
        title: impl Into<Box<str>>,
    ) -> Result<Self, String> {
        let mut session = Self::new(
            puzzle,
            title,
            "A replay kept in local storage.",
            Vec::new(),
            PlaySource::Keepsake,
        );
        session.attempt = replay
            .execute(session.puzzle())
            .map_err(|error| error.to_string())?;
        session.result = Some(session.attempt.result());
        session.saved = true;
        Ok(session)
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn cues(&self) -> &[Box<str>] {
        &self.cues
    }

    pub(crate) const fn source(&self) -> &PlaySource {
        &self.source
    }

    pub(crate) const fn attempt(&self) -> &Attempt {
        &self.attempt
    }

    pub(crate) const fn cursor(&self) -> Coordinate {
        self.cursor
    }

    pub(crate) const fn draft(&self) -> Option<Draft> {
        self.draft
    }

    pub(crate) const fn unfolded_preview(&self) -> bool {
        self.unfolded_preview
    }

    pub(crate) const fn target_visible(&self) -> bool {
        self.target_visible
    }

    pub(crate) const fn result(&self) -> Option<AttemptResult> {
        self.result
    }

    pub(crate) const fn saved(&self) -> bool {
        self.saved
    }

    pub(crate) const fn puzzle(&self) -> &Puzzle {
        self.attempt.puzzle()
    }

    pub(crate) fn replay(&self) -> Replay {
        Replay::from_attempt(&self.attempt)
    }

    pub(crate) fn preview_action(&self) -> Option<PaperAction> {
        match self.draft? {
            Draft::Fold(index) => self
                .puzzle()
                .allowed_folds()
                .get(index)
                .copied()
                .map(PaperAction::Fold),
            Draft::Brush(index) => self
                .puzzle()
                .allowed_brushes()
                .get(index)
                .copied()
                .and_then(|rule| brush_action(rule, self.cursor, self.puzzle())),
        }
    }

    pub(crate) fn animation_active(&self, now: Instant) -> bool {
        self.reveal_started
            .is_some_and(|started| now.saturating_duration_since(started) < RESULT_REVEAL_LIMIT)
    }

    pub(crate) fn reveal_frame(&self, now: Instant) -> RevealFrame {
        let folds = self
            .attempt
            .actions()
            .filter_map(|action| match action {
                PaperAction::Fold(fold) => Some(fold),
                PaperAction::Dot(_) | PaperAction::Line(_) => None,
            })
            .collect::<Vec<_>>();
        let total_folds = folds.len();
        let opened_folds = self.reveal_started.map_or(total_folds, |started| {
            let elapsed = now.saturating_duration_since(started);
            if elapsed >= RESULT_REVEAL_LIMIT {
                total_folds
            } else {
                (elapsed.as_millis() as usize)
                    .saturating_mul(total_folds)
                    .checked_div(RESULT_REVEAL_LIMIT.as_millis() as usize)
                    .unwrap_or(total_folds)
                    .min(total_folds)
            }
        });
        let remaining = total_folds.saturating_sub(opened_folds);
        let mut geometry = self.puzzle().start();
        for fold in folds.into_iter().take(remaining) {
            geometry
                .apply(PaperAction::Fold(fold))
                .expect("result reveal replays validated fold geometry");
        }
        RevealFrame {
            geometry,
            opened_folds,
            total_folds,
            complete: opened_folds == total_folds,
        }
    }

    pub(crate) fn tick(&mut self, now: Instant) -> SessionEvent {
        if self.reveal_started.is_none() {
            return SessionEvent::None;
        }
        if !self.animation_active(now) {
            self.reveal_started = None;
        }
        SessionEvent::Render
    }

    pub(crate) const fn mark_saved(&mut self) {
        self.saved = true;
    }

    pub(crate) fn reset(&mut self) {
        self.attempt.reset();
        self.draft = None;
        self.unfolded_preview = false;
        self.target_visible = false;
        self.result = None;
        self.reveal_started = None;
        self.saved = false;
    }

    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        bindings: KeyBindings,
        now: Instant,
        reduced_motion: bool,
    ) -> SessionEvent {
        if self.reveal_started.take().is_some() {
            return SessionEvent::Render;
        }
        if matches!(key.code, KeyCode::Char('t')) {
            self.target_visible = !self.target_visible;
            return SessionEvent::Render;
        }
        if let Some(result) = self.result {
            return self.handle_result_key(key.code, result, bindings);
        }
        if matches!(key.code, KeyCode::Esc) {
            if self.draft.take().is_some() {
                return SessionEvent::Render;
            }
            return SessionEvent::Back;
        }
        if matches!(key.code, KeyCode::Enter) {
            if self.draft.is_some() {
                let Some(action) = self.preview_action() else {
                    return SessionEvent::Error(
                        "That brush footprint does not fit at this cursor.".to_owned(),
                    );
                };
                return match self.attempt.apply(action) {
                    Ok(()) => {
                        self.draft = None;
                        SessionEvent::Render
                    }
                    Err(error) => SessionEvent::Error(error.to_string()),
                };
            }
            let result = self.attempt.result();
            self.result = Some(result);
            self.reveal_started =
                (!reduced_motion && self.attempt.fold_count().get() > 0).then_some(now);
            return if result.is_success() {
                SessionEvent::Save
            } else {
                SessionEvent::Render
            };
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.adjust_or_move(-1, 0),
            KeyCode::Right | KeyCode::Char('l') => self.adjust_or_move(1, 0),
            KeyCode::Up | KeyCode::Char('k') => self.adjust_or_move(0, -1),
            KeyCode::Down | KeyCode::Char('j') => self.adjust_or_move(0, 1),
            KeyCode::Tab => self.cycle_action(1),
            KeyCode::BackTab => self.cycle_action(-1),
            KeyCode::Char(character) if character == bindings.fold => {
                if self.puzzle().allowed_folds().is_empty() {
                    SessionEvent::Error("This paper has no fold action.".to_owned())
                } else {
                    self.draft = Some(Draft::Fold(0));
                    SessionEvent::Render
                }
            }
            KeyCode::Char(character) if character == bindings.brush => {
                if self.puzzle().allowed_brushes().is_empty() {
                    SessionEvent::Error("This paper has no brush action.".to_owned())
                } else {
                    self.draft = Some(Draft::Brush(0));
                    SessionEvent::Render
                }
            }
            KeyCode::Char(character) if character == bindings.undo => match self.attempt.undo() {
                Ok(()) => SessionEvent::Render,
                Err(error) => SessionEvent::Error(error.to_string()),
            },
            KeyCode::Char(character) if character == bindings.reset => SessionEvent::ConfirmReset,
            KeyCode::Char(character) if character == bindings.preview => {
                self.unfolded_preview = !self.unfolded_preview;
                SessionEvent::Render
            }
            _ => SessionEvent::None,
        }
    }

    fn handle_result_key(
        &mut self,
        code: KeyCode,
        result: AttemptResult,
        bindings: KeyBindings,
    ) -> SessionEvent {
        match code {
            KeyCode::Esc => SessionEvent::Back,
            KeyCode::Left | KeyCode::Char('h') => self.adjust_or_move(-1, 0),
            KeyCode::Right | KeyCode::Char('l') => self.adjust_or_move(1, 0),
            KeyCode::Up | KeyCode::Char('k') => self.adjust_or_move(0, -1),
            KeyCode::Down | KeyCode::Char('j') => self.adjust_or_move(0, 1),
            KeyCode::Enter if result.is_success() && self.saved => SessionEvent::Back,
            KeyCode::Enter if result.is_success() && !self.saved => SessionEvent::Save,
            KeyCode::Enter if !result.is_success() => {
                self.result = None;
                SessionEvent::Render
            }
            KeyCode::Char(character) if character == bindings.reset => {
                self.reset();
                SessionEvent::Render
            }
            KeyCode::Char('v') if self.saved && !matches!(self.source, PlaySource::Lesson) => {
                SessionEvent::Replay
            }
            KeyCode::Char('x') if self.saved && !matches!(self.source, PlaySource::Lesson) => {
                SessionEvent::Export(self.text_export())
            }
            _ => SessionEvent::None,
        }
    }

    fn text_export(&self) -> [String; 3] {
        let result = self.attempt.result();
        let score = result.score();
        [
            format!("Orifude - {}", self.title),
            format!(
                "Solved in {} fold(s) and {} stroke(s).",
                score.folds().get(),
                score.strokes().get()
            ),
            "No solution actions included.".to_owned(),
        ]
    }

    fn cycle_action(&mut self, direction: isize) -> SessionEvent {
        let fold_count = if self.attempt.fold_count() < self.puzzle().fold_budget() {
            self.puzzle().allowed_folds().len()
        } else {
            0
        };
        let brush_count = if self.attempt.stroke_count() < self.puzzle().stroke_budget() {
            self.puzzle().allowed_brushes().len()
        } else {
            0
        };
        let choice_count = fold_count + brush_count;
        if choice_count == 0 {
            self.draft = None;
            return SessionEvent::Error(
                "No fold or brush action remains. Enter opens the paper.".to_owned(),
            );
        }

        let current = match self.draft {
            Some(Draft::Fold(index)) if index < fold_count => Some(index),
            Some(Draft::Brush(index)) if index < brush_count => Some(fold_count + index),
            Some(Draft::Fold(_) | Draft::Brush(_)) | None => None,
        };
        let next = current.map_or_else(
            || {
                if direction < 0 { choice_count - 1 } else { 0 }
            },
            |current| wrap(current, direction, choice_count),
        );
        self.draft = Some(if next < fold_count {
            Draft::Fold(next)
        } else {
            Draft::Brush(next - fold_count)
        });
        SessionEvent::Render
    }

    fn cycle_draft_kind(&mut self, direction: isize) -> SessionEvent {
        let (current, count, fold) = match self.draft {
            Some(Draft::Fold(index)) => (index, self.puzzle().allowed_folds().len(), true),
            Some(Draft::Brush(index)) => (index, self.puzzle().allowed_brushes().len(), false),
            None => return SessionEvent::None,
        };
        let next = wrap(current, direction, count);
        self.draft = Some(if fold {
            Draft::Fold(next)
        } else {
            Draft::Brush(next)
        });
        SessionEvent::Render
    }

    fn adjust_or_move(&mut self, horizontal: isize, vertical: isize) -> SessionEvent {
        if matches!(self.draft, Some(Draft::Fold(_))) {
            return self.cycle_draft_kind(if horizontal + vertical < 0 { -1 } else { 1 });
        }
        let dimensions = self.puzzle().dimensions();
        let row = move_axis(self.cursor.row().get(), vertical, dimensions.height().get());
        let column = move_axis(
            self.cursor.column().get(),
            horizontal,
            dimensions.width().get(),
        );
        self.cursor = Coordinate::new(
            Row::new(row).expect("bounded cursor row"),
            Column::new(column).expect("bounded cursor column"),
        );
        SessionEvent::Render
    }
}

fn brush_action(rule: BrushRule, cursor: Coordinate, puzzle: &Puzzle) -> Option<PaperAction> {
    match rule {
        BrushRule::Dot => Some(PaperAction::Dot(cursor)),
        BrushRule::Line { axis, length } => {
            let dimensions = puzzle.dimensions();
            let (row, column) = (cursor.row().get(), cursor.column().get());
            let offset = length.checked_sub(1)?;
            let end = match axis {
                StrokeAxis::Horizontal
                    if column.checked_add(offset)? < dimensions.width().get() =>
                {
                    dimensions.coordinate(row, column + offset).ok()?
                }
                StrokeAxis::Horizontal if column >= offset => {
                    dimensions.coordinate(row, column - offset).ok()?
                }
                StrokeAxis::Vertical if row.checked_add(offset)? < dimensions.height().get() => {
                    dimensions.coordinate(row + offset, column).ok()?
                }
                StrokeAxis::Vertical if row >= offset => {
                    dimensions.coordinate(row - offset, column).ok()?
                }
                StrokeAxis::Horizontal | StrokeAxis::Vertical => return None,
            };
            Some(PaperAction::Line(LineStroke::new(cursor, end)))
        }
    }
}

fn move_axis(current: u8, direction: isize, count: u8) -> u8 {
    match direction.cmp(&0) {
        std::cmp::Ordering::Less => current.checked_sub(1).unwrap_or(count - 1),
        std::cmp::Ordering::Greater => current
            .checked_add(1)
            .filter(|value| *value < count)
            .unwrap_or(0),
        std::cmp::Ordering::Equal => current,
    }
}

fn wrap(current: usize, direction: isize, count: usize) -> usize {
    debug_assert!(count > 0);
    if direction < 0 {
        current.checked_sub(1).unwrap_or(count - 1)
    } else {
        (current + 1) % count
    }
}

pub(crate) fn fold_label(fold: Fold) -> String {
    format!("{:?} at crease {}", fold.direction(), fold.crease())
}

pub(crate) fn brush_label(brush: BrushRule) -> String {
    match brush {
        BrushRule::Dot => "dot".to_owned(),
        BrushRule::Line { axis, length } => format!("{axis:?} line of {length}"),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, KeyModifiers};

    use super::*;
    use crate::content;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn draft_survives_unrelated_input_and_applies_only_on_confirmation() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let now = Instant::now();
        session.handle_key(key(KeyCode::Char('f')), KeyBindings::default(), now, true);
        let draft = session.draft();
        assert!(draft.is_some());
        assert_eq!(session.attempt().action_count().get(), 0);

        session.handle_key(key(KeyCode::Char('?')), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), draft);
        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);
        assert_eq!(session.attempt().action_count().get(), 1);
    }

    #[test]
    fn tab_moves_through_the_lesson_actions() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let now = Instant::now();

        assert_eq!(
            session.handle_key(key(KeyCode::Tab), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert_eq!(session.draft(), Some(Draft::Fold(0)));

        session.handle_key(key(KeyCode::Tab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
        session.handle_key(key(KeyCode::BackTab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Fold(0)));

        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);
        assert_eq!(session.attempt().action_count().get(), 1);
        session.handle_key(key(KeyCode::Tab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
    }

    #[test]
    fn tab_explains_when_no_action_budget_remains() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        for &action in paper.solution() {
            session.attempt.apply(action).expect("recorded action");
        }

        let event = session.handle_key(
            key(KeyCode::Tab),
            KeyBindings::default(),
            Instant::now(),
            true,
        );

        assert_eq!(
            event,
            SessionEvent::Error(
                "No fold or brush action remains. Enter opens the paper.".to_owned()
            )
        );
        assert_eq!(session.draft(), None);
    }

    #[test]
    fn failed_comparison_returns_to_the_same_attempt() {
        let paper = content::journey().remove(0);
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(0),
        );
        let now = Instant::now();
        let state = session.attempt().state_key();
        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);
        assert!(session.result().is_some_and(|result| !result.is_success()));
        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);
        assert_eq!(session.attempt().state_key(), state);
    }

    #[test]
    fn default_preview_and_compact_target_controls_are_observable_state() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let now = Instant::now();

        assert_eq!(
            session.handle_key(key(KeyCode::Char(' ')), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert!(session.unfolded_preview());
        assert_eq!(
            session.handle_key(key(KeyCode::Char('t')), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert!(session.target_visible());
    }

    #[test]
    fn a_saved_result_exports_three_lines_and_enter_returns() {
        let paper = content::journey().remove(0);
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(0),
        );
        for &action in paper.solution() {
            session.attempt.apply(action).expect("recorded action");
        }
        let now = Instant::now();
        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Save
        );
        session.mark_saved();

        let SessionEvent::Export(lines) =
            session.handle_key(key(KeyCode::Char('x')), KeyBindings::default(), now, true)
        else {
            panic!("saved result exports text");
        };
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| !line.contains('\n')));
        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Back
        );
    }

    #[test]
    fn any_key_completes_the_result_reveal_without_changing_the_result() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        for &action in paper.solution() {
            session.attempt.apply(action).expect("recorded action");
        }
        let now = Instant::now();
        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, false),
            SessionEvent::Save
        );
        assert!(session.animation_active(now));
        let result = session.result();

        assert_eq!(
            session.handle_key(key(KeyCode::Char('x')), KeyBindings::default(), now, false,),
            SessionEvent::Render
        );
        assert!(!session.animation_active(now));
        assert_eq!(session.result(), result);
    }

    #[test]
    fn reduced_motion_skips_the_folded_result_reveal() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        for &action in paper.solution() {
            session.attempt.apply(action).expect("recorded action");
        }
        let now = Instant::now();

        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Save
        );
        assert!(!session.animation_active(now));
        assert!(session.reveal_frame(now).complete);
    }

    #[test]
    fn result_reveal_opens_validated_folds_one_at_a_time() {
        let paper = content::journey().remove(10);
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(10),
        );
        for &action in paper.solution() {
            session.attempt.apply(action).expect("recorded action");
        }
        let started = Instant::now();
        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), started, false);

        let folded = session.reveal_frame(started);
        assert_eq!(folded.opened_folds, 0);
        assert_eq!(folded.total_folds, 2);
        assert!(!folded.complete);

        let halfway = session.reveal_frame(started + Duration::from_millis(600));
        assert_eq!(halfway.opened_folds, 1);
        assert_eq!(halfway.geometry.fold_count().get(), 1);

        let opened = session.reveal_frame(started + RESULT_REVEAL_LIMIT);
        assert!(opened.complete);
        assert_eq!(
            opened.geometry.state_key(),
            paper.puzzle().start().state_key()
        );
        assert!(session.result().is_some_and(AttemptResult::is_success));
    }

    #[test]
    fn brush_draft_tracks_the_cursor_and_rejects_an_unplaceable_line() {
        use crate::domain::puzzle::{PuzzleIdentity, PuzzleSpec};

        let identity = PuzzleIdentity::new("test-pack", "line-brush").unwrap();
        let puzzle = Puzzle::new(
            PuzzleSpec::new(identity, 4, 4)
                .with_allowed_brushes(vec![BrushRule::Line {
                    axis: StrokeAxis::Horizontal,
                    length: 4,
                }])
                .with_budgets(0, 1),
        )
        .unwrap();
        let mut session = PlaySession::new(
            &puzzle,
            "Line brush",
            "Test paper",
            Vec::new(),
            PlaySource::Pack,
        );
        let now = Instant::now();
        session.handle_key(key(KeyCode::Char('b')), KeyBindings::default(), now, true);
        assert!(session.preview_action().is_some());

        session.handle_key(key(KeyCode::Right), KeyBindings::default(), now, true);
        assert_eq!(session.cursor().column().get(), 1);
        assert!(session.preview_action().is_none());
        assert!(matches!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Error(_)
        ));
        assert!(session.result().is_none());
        assert_eq!(session.attempt().action_count().get(), 0);
    }
}
