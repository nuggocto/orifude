use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    BrushRule, Column, Coordinate, Fold, LineStroke, PaperAction, Row, StackView, StrokeAxis,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReplayProgress {
    pub(crate) completed_actions: usize,
    pub(crate) total_actions: usize,
}

struct ReplayPlayback {
    actions: Box<[PaperAction]>,
    completed_actions: usize,
}

pub(crate) struct PlaySession {
    title: Box<str>,
    description: Box<str>,
    cues: Box<[Box<str>]>,
    source: PlaySource,
    attempt: Attempt,
    cursor: Coordinate,
    comparison_row: Row,
    draft: Option<Draft>,
    unfolded_preview: bool,
    target_visible: bool,
    result: Option<AttemptResult>,
    reveal_started: Option<Instant>,
    action_feedback: Option<Box<str>>,
    saved: bool,
    replay_playback: Option<ReplayPlayback>,
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
        let attempt = puzzle.start();
        let draft = first_available_draft(&attempt);
        Self {
            title: title.into(),
            description: description.into(),
            cues: cues.into_boxed_slice(),
            source,
            attempt,
            cursor,
            comparison_row: cursor.row(),
            draft,
            unfolded_preview: false,
            target_visible: false,
            result: None,
            reveal_started: None,
            action_feedback: None,
            saved: false,
            replay_playback: None,
        }
    }

    pub(crate) fn from_replay(
        puzzle: &Puzzle,
        replay: &Replay,
        title: impl Into<Box<str>>,
    ) -> Result<Self, String> {
        let completed_attempt = replay.execute(puzzle).map_err(|error| error.to_string())?;
        if !completed_attempt.result().is_success() {
            return Err("The stored replay does not solve this paper.".to_owned());
        }
        let mut session = Self::new(
            puzzle,
            title,
            "A replay kept in local storage.",
            Vec::new(),
            PlaySource::Keepsake,
        );
        session.draft = None;
        session.saved = true;
        session.replay_playback = Some(ReplayPlayback {
            actions: replay.actions().into(),
            completed_actions: 0,
        });
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

    pub(crate) const fn comparison_row(&self) -> Row {
        self.comparison_row
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

    pub(crate) fn replay_progress(&self) -> Option<ReplayProgress> {
        self.replay_playback
            .as_ref()
            .map(|playback| ReplayProgress {
                completed_actions: playback.completed_actions,
                total_actions: playback.actions.len(),
            })
    }

    pub(crate) fn action_feedback(&self) -> Option<&str> {
        self.action_feedback.as_deref()
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
        let mut render = false;
        if self.reveal_started.is_some() {
            render = true;
        }
        if self
            .reveal_started
            .is_some_and(|started| now.saturating_duration_since(started) >= RESULT_REVEAL_LIMIT)
        {
            self.reveal_started = None;
        }
        if render {
            SessionEvent::Render
        } else {
            SessionEvent::None
        }
    }

    pub(crate) const fn mark_saved(&mut self) {
        self.saved = true;
    }

    pub(crate) fn reset(&mut self) {
        if self.replay_playback.is_some() {
            self.set_replay_position(0);
            return;
        }
        self.attempt.reset();
        self.draft = first_available_draft(&self.attempt);
        self.unfolded_preview = false;
        self.target_visible = false;
        self.result = None;
        self.reveal_started = None;
        self.action_feedback = None;
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
        if self.replay_playback.is_some() {
            return self.handle_replay_key(key.code, bindings, now, reduced_motion);
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
            if self.draft.is_some() && !self.selected_tool_available() {
                self.draft = first_available_draft(&self.attempt);
            }
            if self.draft.is_some() {
                let Some(action) = self.preview_action() else {
                    return SessionEvent::Error(
                        "That brush footprint does not fit at this cursor.".to_owned(),
                    );
                };
                return match self.attempt.apply(action) {
                    Ok(()) => {
                        self.draft = first_available_draft(&self.attempt);
                        self.action_feedback =
                            Some(action_feedback(&self.attempt, action, self.draft));
                        SessionEvent::Render
                    }
                    Err(error) => SessionEvent::Error(error.to_string()),
                };
            }
            let mut result = self.attempt.result();
            if !result.is_success()
                && matches!(self.source, PlaySource::Journey(index) if index > 0)
            {
                self.attempt.mark_hint_used();
                result = self.attempt.result();
            }
            self.comparison_row = self.cursor.row();
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
                if self.puzzle().allowed_folds().is_empty()
                    || self.attempt.fold_count() >= self.puzzle().fold_budget()
                {
                    SessionEvent::Error("No fold remains on this paper.".to_owned())
                } else if let Some(index) = first_applicable_fold(&self.attempt) {
                    self.draft = Some(Draft::Fold(index));
                    SessionEvent::Render
                } else {
                    SessionEvent::Error("No fold fits the current paper state.".to_owned())
                }
            }
            KeyCode::Char(character) if character == bindings.brush => {
                if self.puzzle().allowed_brushes().is_empty()
                    || self.attempt.stroke_count() >= self.puzzle().stroke_budget()
                {
                    SessionEvent::Error("No brush mark remains on this paper.".to_owned())
                } else {
                    self.draft = Some(Draft::Brush(0));
                    SessionEvent::Render
                }
            }
            KeyCode::Char(character) if character == bindings.undo => match self.attempt.undo() {
                Ok(()) => {
                    self.draft = first_available_draft(&self.attempt);
                    self.action_feedback = None;
                    SessionEvent::Render
                }
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
            KeyCode::Up | KeyCode::Char('k') => self.move_comparison_row(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_comparison_row(1),
            KeyCode::Enter if result.is_success() && self.saved => SessionEvent::Back,
            KeyCode::Enter if result.is_success() && !self.saved => SessionEvent::Save,
            KeyCode::Enter if !result.is_success() => {
                self.result = None;
                self.draft = first_available_draft(&self.attempt);
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

    fn handle_replay_key(
        &mut self,
        code: KeyCode,
        bindings: KeyBindings,
        now: Instant,
        reduced_motion: bool,
    ) -> SessionEvent {
        match code {
            KeyCode::Esc => SessionEvent::Back,
            KeyCode::Left | KeyCode::Char('h') => self.rewind_replay(),
            KeyCode::Up | KeyCode::Char('k') if self.result.is_some() => {
                self.move_comparison_row(-1)
            }
            KeyCode::Down | KeyCode::Char('j') if self.result.is_some() => {
                self.move_comparison_row(1)
            }
            KeyCode::Enter if self.result.is_some() => SessionEvent::Back,
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                self.advance_replay(now, reduced_motion)
            }
            KeyCode::Char(character) if character == bindings.reset => {
                self.set_replay_position(0);
                SessionEvent::Render
            }
            KeyCode::Char('x') if self.result.is_some() => SessionEvent::Export(self.text_export()),
            _ => SessionEvent::None,
        }
    }

    fn advance_replay(&mut self, now: Instant, reduced_motion: bool) -> SessionEvent {
        let progress = self
            .replay_progress()
            .expect("replay input requires replay state");
        if progress.completed_actions < progress.total_actions {
            self.set_replay_position(progress.completed_actions + 1);
            return SessionEvent::Render;
        }
        if self.result.is_none() {
            let result = self.attempt.result();
            debug_assert!(result.is_success());
            self.comparison_row = self.cursor.row();
            self.result = Some(result);
            self.reveal_started =
                (!reduced_motion && self.attempt.fold_count().get() > 0).then_some(now);
            return SessionEvent::Render;
        }
        SessionEvent::None
    }

    fn rewind_replay(&mut self) -> SessionEvent {
        let progress = self
            .replay_progress()
            .expect("replay input requires replay state");
        if self.result.is_some() {
            self.set_replay_position(progress.completed_actions);
            return SessionEvent::Render;
        }
        let Some(previous) = progress.completed_actions.checked_sub(1) else {
            return SessionEvent::None;
        };
        self.set_replay_position(previous);
        SessionEvent::Render
    }

    fn set_replay_position(&mut self, completed_actions: usize) {
        let action_count = self
            .replay_playback
            .as_ref()
            .expect("replay position requires replay state")
            .actions
            .len();
        assert!(completed_actions <= action_count);
        self.attempt.reset();
        self.cursor = self
            .puzzle()
            .dimensions()
            .coordinate(0, 0)
            .expect("validated paper contains its origin");
        for index in 0..completed_actions {
            let action = self
                .replay_playback
                .as_ref()
                .expect("replay state remains present")
                .actions[index];
            self.attempt
                .apply(action)
                .expect("a validated replay prefix remains executable");
            if let Some(coordinate) = action_coordinate(action) {
                self.cursor = coordinate;
            }
        }
        self.replay_playback
            .as_mut()
            .expect("replay state remains present")
            .completed_actions = completed_actions;
        self.comparison_row = self.cursor.row();
        self.result = None;
        self.reveal_started = None;
        self.unfolded_preview = false;
        self.action_feedback = completed_actions.checked_sub(1).map(|index| {
            let action = self
                .replay_playback
                .as_ref()
                .expect("replay state remains present")
                .actions[index];
            replay_action_feedback(action, completed_actions, action_count)
        });
    }

    fn move_comparison_row(&mut self, direction: isize) -> SessionEvent {
        let row = move_axis(
            self.comparison_row.get(),
            direction,
            self.puzzle().dimensions().height().get(),
        );
        self.comparison_row = Row::new(row).expect("bounded comparison row");
        SessionEvent::Render
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
        let action_count = fold_count + brush_count;
        let choice_count = action_count + 1;

        let current = match self.draft {
            Some(Draft::Fold(index)) if index < fold_count => Some(index),
            Some(Draft::Brush(index)) if index < brush_count => Some(fold_count + index),
            Some(Draft::Fold(_) | Draft::Brush(_)) => None,
            None => Some(action_count),
        };
        let next = current.map_or_else(
            || {
                if direction < 0 { choice_count - 1 } else { 0 }
            },
            |current| wrap(current, direction, choice_count),
        );
        self.draft = if next < fold_count {
            Some(Draft::Fold(next))
        } else if next < action_count {
            Some(Draft::Brush(next - fold_count))
        } else {
            None
        };
        SessionEvent::Render
    }

    fn selected_tool_available(&self) -> bool {
        match self.draft {
            Some(Draft::Fold(index)) => {
                self.attempt.fold_count() < self.puzzle().fold_budget()
                    && index < self.puzzle().allowed_folds().len()
            }
            Some(Draft::Brush(index)) => {
                self.attempt.stroke_count() < self.puzzle().stroke_budget()
                    && index < self.puzzle().allowed_brushes().len()
            }
            None => false,
        }
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

fn action_coordinate(action: PaperAction) -> Option<Coordinate> {
    match action {
        PaperAction::Dot(coordinate) => Some(coordinate),
        PaperAction::Line(line) => Some(line.start()),
        PaperAction::Fold(_) => None,
    }
}

fn replay_action_feedback(
    action: PaperAction,
    completed_actions: usize,
    action_count: usize,
) -> Box<str> {
    let action = match action {
        PaperAction::Fold(fold) => {
            format!("Fold {} at crease {}.", fold.direction(), fold.crease())
        }
        PaperAction::Dot(coordinate) => format!(
            "Place a dot at row {}, column {}.",
            coordinate.row().get() + 1,
            coordinate.column().get() + 1
        ),
        PaperAction::Line(line) => format!(
            "Draw from row {}, column {} to row {}, column {}.",
            line.start().row().get() + 1,
            line.start().column().get() + 1,
            line.end().row().get() + 1,
            line.end().column().get() + 1
        ),
    };
    let next = if completed_actions == action_count {
        "Enter opens the paper."
    } else {
        "Enter or Right shows the next step."
    };
    format!("Replay {completed_actions}/{action_count}: {action} {next}").into_boxed_str()
}

fn first_available_draft(attempt: &Attempt) -> Option<Draft> {
    if attempt.result().is_success() {
        return None;
    }
    if let Some(index) = first_applicable_fold(attempt) {
        return Some(Draft::Fold(index));
    }
    if attempt.stroke_count() < attempt.puzzle().stroke_budget()
        && !attempt.puzzle().allowed_brushes().is_empty()
    {
        return Some(Draft::Brush(0));
    }
    None
}

fn first_applicable_fold(attempt: &Attempt) -> Option<usize> {
    if attempt.fold_count() >= attempt.puzzle().fold_budget() {
        return None;
    }

    let mut probe = attempt.clone();
    attempt
        .puzzle()
        .allowed_folds()
        .iter()
        .position(|fold| probe.apply(PaperAction::Fold(*fold)).is_ok())
}

fn action_feedback(attempt: &Attempt, action: PaperAction, next: Option<Draft>) -> Box<str> {
    match action {
        PaperAction::Fold(_) => format!("Fold complete. {}", next_tool_detail(attempt, next)),
        PaperAction::Dot(coordinate) => {
            let mut stack = StackView::new();
            attempt
                .stack_at(coordinate, &mut stack)
                .expect("an applied dot has an in-bounds stack");
            let count = stack.len();
            let layer = if count == 1 { "layer" } else { "layers" };
            format!(
                "Ink reached {count} {layer}. {}",
                next_tool_detail(attempt, next)
            )
        }
        PaperAction::Line(_) => format!("Ink line complete. {}", next_tool_detail(attempt, next)),
    }
    .into_boxed_str()
}

fn next_tool_detail(attempt: &Attempt, next: Option<Draft>) -> String {
    match next {
        Some(Draft::Fold(_)) => "Another fold is ready.".to_owned(),
        Some(Draft::Brush(index)) => match attempt.puzzle().allowed_brushes().get(index) {
            Some(BrushRule::Dot) => "The dot brush is ready.".to_owned(),
            Some(BrushRule::Line { .. }) => "The line brush is ready.".to_owned(),
            None => "The next tool is ready.".to_owned(),
        },
        None => "Enter opens the paper.".to_owned(),
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
    fn a_fresh_paper_has_a_useful_tool_ready_for_enter() {
        let lesson = content::lesson();
        let mut lesson_session = PlaySession::new(
            lesson.puzzle(),
            lesson.title(),
            lesson.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let now = Instant::now();

        assert_eq!(lesson_session.draft(), Some(Draft::Fold(0)));
        assert_eq!(
            lesson_session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true,),
            SessionEvent::Render
        );
        assert_eq!(lesson_session.attempt().fold_count().get(), 1);
        assert_eq!(lesson_session.draft(), Some(Draft::Brush(0)));
        assert!(!lesson_session.animation_active(now));
        assert_eq!(
            lesson_session.action_feedback(),
            Some("Fold complete. The dot brush is ready.")
        );

        let first_drop = content::journey().remove(0);
        let first_drop_session = PlaySession::new(
            first_drop.puzzle(),
            first_drop.title(),
            first_drop.description(),
            Vec::new(),
            PlaySource::Journey(0),
        );
        assert_eq!(first_drop_session.draft(), Some(Draft::Brush(0)));
    }

    #[test]
    fn tab_moves_through_tools_and_the_open_paper_state() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let now = Instant::now();

        assert_eq!(session.draft(), Some(Draft::Fold(0)));

        session.handle_key(key(KeyCode::Tab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
        session.handle_key(key(KeyCode::BackTab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Fold(0)));

        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);
        assert_eq!(session.attempt().action_count().get(), 1);
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
        session.handle_key(key(KeyCode::Tab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), None);
        session.handle_key(key(KeyCode::BackTab), KeyBindings::default(), now, true);
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
    }

    #[test]
    fn no_remaining_action_keeps_open_paper_ready() {
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

        assert_eq!(event, SessionEvent::Render);
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
        session.handle_key(key(KeyCode::Esc), KeyBindings::default(), now, true);
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

    #[test]
    fn a_completed_action_leaves_readable_feedback_without_starting_an_animation() {
        let paper = content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let started = Instant::now();

        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), started, false,),
            SessionEvent::Render
        );
        assert_eq!(
            session.action_feedback(),
            Some("Fold complete. The dot brush is ready.")
        );
        assert!(!session.animation_active(started));

        session.handle_key(key(KeyCode::Down), KeyBindings::default(), started, false);
        assert_eq!(
            session.action_feedback(),
            Some("Fold complete. The dot brush is ready.")
        );

        session.handle_key(
            key(KeyCode::Char('u')),
            KeyBindings::default(),
            started,
            false,
        );
        assert!(session.action_feedback().is_none());
    }

    #[test]
    fn a_dot_reports_the_inked_layers_and_readies_open_paper() {
        let paper = content::journey().remove(0);
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(0),
        );
        let now = Instant::now();
        session.handle_key(key(KeyCode::Down), KeyBindings::default(), now, false);
        session.handle_key(key(KeyCode::Right), KeyBindings::default(), now, false);

        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, false),
            SessionEvent::Render
        );
        assert_eq!(session.draft(), None);
        assert_eq!(
            session.action_feedback(),
            Some("Ink reached 1 layer. Enter opens the paper.")
        );
        assert!(!session.animation_active(now));
    }

    #[test]
    fn a_missed_journey_paper_reveals_its_hint_and_records_use() {
        let paper = content::journey().remove(1);
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(1),
        );
        let now = Instant::now();

        assert!(!session.attempt().hints_used());
        session.handle_key(key(KeyCode::Esc), KeyBindings::default(), now, true);
        session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true);

        assert!(session.result().is_some_and(|result| !result.is_success()));
        assert!(session.attempt().hints_used());

        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert_eq!(session.draft(), Some(Draft::Brush(0)));
        assert!(session.result().is_none());
    }

    #[test]
    fn automatic_fold_selection_only_readies_applicable_folds() {
        let now = Instant::now();
        let mut checked_folds = 0_usize;

        for (index, paper) in content::journey().into_iter().enumerate() {
            if paper.puzzle().fold_budget().get() < 2 {
                continue;
            }
            let mut session = PlaySession::new(
                paper.puzzle(),
                paper.title(),
                paper.description(),
                paper.cues().to_vec(),
                PlaySource::Journey(index),
            );

            while matches!(session.draft(), Some(Draft::Fold(_))) {
                let action = session
                    .preview_action()
                    .expect("an automatically selected fold has a preview");
                let mut probe = session.attempt().clone();
                assert!(
                    probe.apply(action).is_ok(),
                    "{} automatically selected an illegal {action:?}",
                    paper.puzzle().identity().puzzle_id()
                );
                assert_eq!(
                    session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
                    SessionEvent::Render
                );
                checked_folds += 1;
                if matches!(session.draft(), Some(Draft::Fold(_))) {
                    assert_eq!(
                        session.handle_key(
                            key(KeyCode::Char(KeyBindings::default().fold)),
                            KeyBindings::default(),
                            now,
                            true,
                        ),
                        SessionEvent::Render
                    );
                }
            }
        }

        assert!(
            checked_folds > 1,
            "the catalog must exercise repeated folds"
        );
    }

    #[test]
    fn a_replay_starts_fresh_and_steps_through_the_saved_actions() {
        let paper = content::journey().remove(0);
        let mut attempt = paper.puzzle().start();
        for &action in paper.solution() {
            attempt.apply(action).expect("recorded action applies");
        }
        let replay = Replay::from_attempt(&attempt);
        let mut session = PlaySession::from_replay(paper.puzzle(), &replay, paper.title())
            .expect("recorded replay loads");
        let now = Instant::now();

        assert_eq!(session.attempt().action_count().get(), 0);
        assert!(session.result().is_none());
        assert_eq!(
            session.handle_key(key(KeyCode::Down), KeyBindings::default(), now, true),
            SessionEvent::None
        );
        assert_eq!(
            session.handle_key(key(KeyCode::Right), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert_eq!(session.attempt().action_count().get(), 1);
        assert!(session.result().is_none());
        assert_eq!(
            session.handle_key(key(KeyCode::Char('r')), KeyBindings::default(), now, true,),
            SessionEvent::Render
        );
        assert_eq!(session.attempt().action_count().get(), 0);
        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert_eq!(session.attempt().action_count().get(), 1);
        assert!(session.result().is_none());
        assert_eq!(
            session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert!(session.result().is_some_and(AttemptResult::is_success));

        assert_eq!(
            session.handle_key(key(KeyCode::Left), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert!(session.result().is_none());
        assert_eq!(session.attempt().action_count().get(), 1);
        assert_eq!(
            session.handle_key(key(KeyCode::Left), KeyBindings::default(), now, true),
            SessionEvent::Render
        );
        assert_eq!(session.attempt().action_count().get(), 0);
    }

    #[test]
    fn replay_playback_rejects_a_valid_but_unsuccessful_action_list() {
        let paper = content::journey().remove(0);
        let replay = Replay::new(
            crate::domain::replay::ReplayMetadata::current(paper.puzzle()),
            Vec::new(),
        )
        .expect("empty replay is structurally valid");

        let Err(error) = PlaySession::from_replay(paper.puzzle(), &replay, paper.title()) else {
            panic!("an unsuccessful replay must not enter playback");
        };
        assert_eq!(error, "The stored replay does not solve this paper.");
    }

    #[test]
    fn a_successful_replay_without_actions_opens_normally() {
        use crate::domain::puzzle::{PuzzleIdentity, PuzzleSpec};
        use crate::domain::replay::ReplayMetadata;

        let puzzle = Puzzle::new(PuzzleSpec::new(
            PuzzleIdentity::new("test-pack", "blank-paper").unwrap(),
            4,
            4,
        ))
        .unwrap();
        let replay = Replay::new(ReplayMetadata::current(&puzzle), Vec::new()).unwrap();
        let mut session = PlaySession::from_replay(&puzzle, &replay, "Blank paper").unwrap();

        assert_eq!(
            session.replay_progress(),
            Some(ReplayProgress {
                completed_actions: 0,
                total_actions: 0,
            })
        );
        assert_eq!(
            session.handle_key(
                key(KeyCode::Enter),
                KeyBindings::default(),
                Instant::now(),
                true,
            ),
            SessionEvent::Render
        );
        assert!(session.result().is_some_and(AttemptResult::is_success));
    }

    #[test]
    fn every_official_paper_is_playable_through_the_session_tools() {
        let now = Instant::now();
        for (index, paper) in content::journey().into_iter().enumerate() {
            let mut session = PlaySession::new(
                paper.puzzle(),
                paper.title(),
                paper.description(),
                paper.cues().to_vec(),
                PlaySource::Journey(index),
            );
            for &action in paper.solution() {
                ready_action(&mut session, action);
                assert_eq!(session.preview_action(), Some(action));
                assert_eq!(
                    session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
                    SessionEvent::Render,
                    "{} should apply its recorded action through the player session",
                    paper.puzzle().identity().puzzle_id()
                );
            }
            assert_eq!(session.draft(), None);
            assert_eq!(
                session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
                SessionEvent::Save,
                "{} should open as a successful paper",
                paper.puzzle().identity().puzzle_id()
            );
            assert!(session.result().is_some_and(AttemptResult::is_success));
        }
    }

    #[test]
    fn every_official_replay_can_be_walked_forward_to_its_result() {
        let now = Instant::now();
        for paper in content::journey() {
            let mut attempt = paper.puzzle().start();
            for &action in paper.solution() {
                attempt.apply(action).expect("recorded action applies");
            }
            let replay = Replay::from_attempt(&attempt);
            let mut session = PlaySession::from_replay(paper.puzzle(), &replay, paper.title())
                .expect("recorded replay loads");

            for completed in 1..=paper.solution().len() {
                assert_eq!(
                    session.handle_key(key(KeyCode::Right), KeyBindings::default(), now, true,),
                    SessionEvent::Render
                );
                assert_eq!(session.attempt().action_count().get() as usize, completed);
                assert!(session.result().is_none());
            }
            assert_eq!(
                session.handle_key(key(KeyCode::Enter), KeyBindings::default(), now, true),
                SessionEvent::Render
            );
            assert!(session.result().is_some_and(AttemptResult::is_success));
        }
    }

    fn ready_action(session: &mut PlaySession, action: PaperAction) {
        match action {
            PaperAction::Fold(fold) => {
                let index = session
                    .puzzle()
                    .allowed_folds()
                    .iter()
                    .position(|candidate| *candidate == fold)
                    .expect("recorded fold is selectable");
                session.draft = Some(Draft::Fold(index));
            }
            PaperAction::Dot(coordinate) => {
                let index = session
                    .puzzle()
                    .allowed_brushes()
                    .iter()
                    .position(|candidate| *candidate == BrushRule::Dot)
                    .expect("recorded dot brush is selectable");
                session.cursor = coordinate;
                session.draft = Some(Draft::Brush(index));
            }
            PaperAction::Line(line) => {
                let (axis, length) = line
                    .axis_and_length()
                    .expect("recorded line has valid geometry");
                let index = session
                    .puzzle()
                    .allowed_brushes()
                    .iter()
                    .position(|candidate| *candidate == BrushRule::Line { axis, length })
                    .expect("recorded line brush is selectable");
                session.cursor = line.start();
                session.draft = Some(Draft::Brush(index));
            }
        }
    }
}
