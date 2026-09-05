use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    Coordinate, FoldDirection, InkPattern, MAX_PHYSICAL_CELLS, PaperAction, Row,
};
use crate::storage::{ColorMode, GlyphMode, KeyBindings};

use super::app::{App, Overlay, Screen, action_label, key_label};
use super::components::{
    BRANCH_CARD_WIDTH, BranchChoices, BranchGrowth, CompletionCourier, DialogLayer, Paper,
    StatusBar, TerminalMark, courier_art,
};
use super::layout::{LayoutMode, MINIMUM_HEIGHT, MINIMUM_WIDTH, ShellLayout, centered};
use super::session::{Draft, PlaySession, PlaySource, fold_label};
use super::style::StyleProfile;
use super::text::SafeText;

pub(crate) fn render(frame: &mut Frame<'_>, app: &App, profile: StyleProfile, now: Instant) {
    let area = frame.area();
    let mode = LayoutMode::for_area(area);
    if mode == LayoutMode::ResizeMessage {
        render_resize_message(frame, area, profile);
        return;
    }

    let shell = ShellLayout::new(area, mode).expect("interactive layout has shell regions");
    frame.render_widget(
        Paragraph::new(Line::styled("ORIFUDE", profile.title())).alignment(Alignment::Center),
        shell.title,
    );
    let content = content_area(shell);

    match app.screen() {
        Screen::Capabilities => render_capabilities(frame, content, profile),
        Screen::Branch => {
            let mark_frame = app.mark_frame(now);
            if mark_frame < super::app::MARK_FRAME_COUNT - 1 {
                TerminalMark::render(frame, shell.mark, mark_frame, profile);
            } else {
                BranchGrowth::render(frame, shell.mark, app.completed_group_count(), profile);
            }
            let completed = (0..app.journey().len())
                .filter(|index| app.journey_complete(*index))
                .count();
            let saved = if app.recent().is_empty() { "no" } else { "yes" };
            let detailed_title = format!(
                "Home | Journey {completed}/{} | Saved {saved}",
                app.journey().len()
            );
            let card_width = shell.branch.width.min(BRANCH_CARD_WIDTH);
            let title =
                if detailed_title.chars().count().saturating_add(2) <= usize::from(card_width) {
                    detailed_title
                } else {
                    format!("Home | Journey {completed}/{}", app.journey().len())
                };
            BranchChoices::render(frame, shell.branch, app.selection(), &title, profile);
        }
        Screen::Journey => render_journey(frame, content, app, profile),
        Screen::Play => {
            if let Some(session) = app.session() {
                render_session(
                    frame,
                    content,
                    session,
                    app.settings().bindings,
                    profile,
                    now,
                    app.group_completion(),
                );
            }
        }
        Screen::Packs => render_packs(frame, content, app, profile),
        Screen::PackPuzzles => render_pack_papers(frame, content, app, profile),
        Screen::Keepsakes => render_keepsakes(frame, content, app, profile),
        Screen::HowTo => render_walkthrough(frame, content, app, profile),
        Screen::Settings => render_settings(frame, content, app, profile),
        Screen::Loading => render_loading(frame, content, app, profile),
    }
    let status = status_text(app, profile.glyph_mode(), shell.status.width);
    StatusBar::render(frame, shell.status, app.focused(), &status);
    if let Some(overlay) = app.overlay() {
        let overlay_area = if app.screen() == Screen::Play && matches!(overlay, Overlay::Help(_)) {
            content
        } else if mode == LayoutMode::Preferred {
            shell.mark
        } else {
            content
        };
        if mode == LayoutMode::Narrow {
            frame.render_widget(Clear, overlay_area);
        }
        DialogLayer::render(frame, overlay_area, overlay, profile);
    }
}

fn render_capabilities(frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
    let lines = if area.width >= 80 {
        vec![
            Line::styled("The paper is ready.", profile.title()),
            Line::from(""),
            Line::from("Match the paper to the shown pattern."),
            Line::from(""),
            Line::from("1  An available tool is ready as soon as the paper arrives."),
            Line::from("2  For a fold, + shows the moving side. Enter folds it."),
            Line::from("3  Move @ with arrows. The brush inks every layer underneath."),
            Line::from("4  Enter places ink. When the target matches, Enter opens the paper."),
            Line::from(""),
            Line::from("Tab changes tools. Esc readies Open paper. ? explains every tool."),
            Line::from(""),
            Line::styled("Enter starts the lesson. Esc leaves.", profile.paper()),
        ]
    } else {
        vec![
            Line::styled("The paper is ready.", profile.title()),
            Line::from("Match the paper to the shown pattern."),
            Line::from(""),
            Line::from("1  An available tool is already ready."),
            Line::from("2  + shows a fold's moving side. Enter folds."),
            Line::from("3  Arrows move @. Enter brushes every layer."),
            Line::from("4  When the target matches, Enter opens the paper."),
            Line::from(""),
            Line::from("Tab changes tools. Esc readies Open paper."),
            Line::from("? explains the tools. u undoes. r resets."),
            Line::styled("Enter starts the lesson. Esc leaves.", profile.paper()),
        ]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Paper::block("How this paper works", profile))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_journey(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let mut choices = app
        .journey()
        .iter()
        .enumerate()
        .map(|(index, paper)| {
            let state = if app.journey_complete(index) {
                "complete"
            } else if app.journey_unlocked(index) {
                "open"
            } else {
                "locked"
            };
            let (group_number, paper_number) = crate::content::journey_group(index)
                .map_or((0, 0), |(group_index, group)| {
                    (group_index + 1, index - group.first_paper + 1)
                });
            format!(
                "{group_number}.{paper_number}  {}  [{state}]",
                paper.title()
            )
        })
        .collect::<Vec<_>>();
    choices.push("Back to the branch".to_owned());
    let Some((group_index, group)) = crate::content::journey_group(app.selection()) else {
        render_owned_focus(frame, area, "Journey", &choices, app.selection(), profile);
        return;
    };
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(group.mechanic)
            .style(StyleProfile::muted())
            .wrap(Wrap { trim: true }),
        regions[0],
    );
    let title = format!("Journey {}: {}", group_index + 1, group.title);
    render_owned_focus(
        frame,
        regions[1],
        &title,
        &choices,
        app.selection(),
        profile,
    );
}

fn render_packs(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let mut choices = app
        .packs()
        .iter()
        .map(|pack| {
            SafeText::external_display(&pack.title, 80, profile.glyph_mode())
                .as_str()
                .to_owned()
        })
        .collect::<Vec<_>>();
    if choices.is_empty() {
        choices.push("No packs are installed - Enter returns".to_owned());
    } else {
        choices.push("Back to the branch".to_owned());
    }
    render_owned_focus(
        frame,
        area,
        "Installed puzzle packs",
        &choices,
        app.selection(),
        profile,
    );
}

fn render_pack_papers(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let mut choices = app
        .pack_papers()
        .iter()
        .map(|paper| {
            SafeText::external_display(&paper.title, 80, profile.glyph_mode())
                .as_str()
                .to_owned()
        })
        .collect::<Vec<_>>();
    choices.push("Back to installed packs".to_owned());
    render_owned_focus(
        frame,
        area,
        "Pack papers",
        &choices,
        app.selection(),
        profile,
    );
}

fn render_keepsakes(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let installed = |pack_id: &str| app.packs().iter().any(|pack| pack.id.as_ref() == pack_id);
    let mut choices = app
        .recent()
        .iter()
        .map(|progress| {
            let missing = if installed(&progress.pack_id)
                || matches!(
                    progress.pack_id.as_ref(),
                    "orifude-journey" | "orifude-daily" | "orifude-endless"
                ) {
                ""
            } else {
                "  [pack missing; replay kept]"
            };
            format!(
                "{}/{}  best {}F {}S{missing}",
                progress.pack_id, progress.puzzle_id, progress.best_folds, progress.best_strokes
            )
        })
        .collect::<Vec<_>>();
    if choices.is_empty() && app.keepsake_offset() == 0 && !app.keepsake_has_more() {
        choices.push("No keepsakes yet - Enter returns".to_owned());
    } else {
        if app.keepsake_has_more() {
            choices.push("Older keepsakes".to_owned());
        }
        if app.keepsake_offset() > 0 {
            choices.push("Newer keepsakes".to_owned());
        }
        choices.push("Back to the branch".to_owned());
    }
    let first = app.keepsake_offset().saturating_add(1);
    let last = app
        .keepsake_offset()
        .saturating_add(app.recent().len() as u64);
    let title = if app.recent().is_empty() {
        "Saved keepsakes".to_owned()
    } else {
        format!("Saved keepsakes {first}-{last}")
    };
    render_owned_focus(frame, area, &title, &choices, app.selection(), profile);
}

fn render_settings(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let settings = app.settings();
    let bindings = settings.bindings;
    let color = match settings.color_mode {
        ColorMode::Auto => "Automatic",
        ColorMode::Color => "Color",
        ColorMode::Monochrome => "Monochrome",
    };
    let glyphs = match settings.glyph_mode {
        GlyphMode::Unicode => "Unicode",
        GlyphMode::Ascii => "ASCII only",
    };
    let choices = vec![
        format!("Color: {color}"),
        format!("Symbols: {glyphs}"),
        format!("Reduced motion: {}", on_off(settings.reduced_motion)),
        format!("Instant reveal: {}", on_off(settings.instant_reveal)),
        format!("Fold key: {}", bindings.fold),
        format!("Brush key: {}", bindings.brush),
        format!("Undo key: {}", bindings.undo),
        format!("Reset key: {}", bindings.reset),
        format!("Preview key: {}", key_label(bindings.preview)),
        format!("Help key: {}", bindings.help),
        format!("Quit key: {}", bindings.quit),
        "Back to the branch".to_owned(),
    ];
    let regions = if app.binding_capture().is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(0)])
            .split(area)
    };
    render_owned_focus(
        frame,
        regions[0],
        "Settings and keys",
        &choices,
        app.selection(),
        profile,
    );
    if app.binding_capture().is_some() {
        frame.render_widget(
            Paragraph::new("Press one unused key, or Esc to cancel.")
                .style(profile.paper())
                .block(Paper::block("Change binding", profile)),
            regions[1],
        );
    }
}

fn render_loading(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("Folding a new paper...", profile.title()),
            Line::from(""),
            Line::from(format!("Local date: {}", app.local_date())),
            Line::from("The bounded generator and solver are working offline."),
            Line::from("Esc cancels and joins the worker."),
        ])
        .block(Paper::block("Preparing paper", profile))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_walkthrough(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let (paper, attempt, step, total) = app.walkthrough();
    let fold = paper.solution().first().copied();
    let brush = paper.solution().get(1).copied();
    let brush_cursor = brush.and_then(action_coordinate);
    let (cursor, preview, comparison_reveal, caption) = match step {
        0 => (
            None,
            None,
            None,
            "The target is the opened sheet. The fresh paper starts flat and dry.".to_owned(),
        ),
        1 => (
            None,
            fold,
            None,
            "Fold tool: every + cell crosses the named crease when you press Enter.".to_owned(),
        ),
        2 => (
            None,
            None,
            None,
            "The moving side settles on top of the other side, making a stack.".to_owned(),
        ),
        3 => (
            brush_cursor,
            None,
            None,
            "Move @ to inspect one stack. Its layers read from bottom to top.".to_owned(),
        ),
        4 => (
            brush_cursor,
            None,
            None,
            "Brush tool: a dot or line inks every layer inside its preview.".to_owned(),
        ),
        _ => {
            let result = attempt.result();
            (
                None,
                None,
                Some(paper.puzzle().dimensions().cell_count()),
                format!(
                    "Open paper compares every cell: {} missing (?), {} extra (!).",
                    result.comparison().missing().len(),
                    result.comparison().extra().len()
                ),
            )
        }
    };
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(6)])
        .split(area);
    render_attempt_boards(
        frame,
        regions[0],
        paper.puzzle(),
        &attempt,
        cursor,
        preview,
        false,
        comparison_reveal,
        attempt.ink(),
        profile,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("Teaching frame {} of {total}", step + 1),
                profile.title(),
            ),
            Line::from(caption),
            Line::styled(
                "Left/Right or Enter steps; Esc returns.",
                StyleProfile::muted(),
            ),
        ])
        .block(Paper::block("How to play", profile))
        .wrap(Wrap { trim: true }),
        regions[1],
    );
}

fn render_session(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    now: Instant,
    group_completion: Option<&crate::content::JourneyGroup>,
) {
    let status_height = if area.width >= 80 { 7 } else { 6 };
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(status_height)])
        .split(area);
    let reveal = session.result().map(|_| session.reveal_frame(now));
    let target_area = if let Some(reveal) = reveal.as_ref() {
        render_session_boards(
            frame,
            regions[0],
            session.puzzle(),
            &reveal.geometry,
            Some(session.cursor()),
            session.comparison_row(),
            None,
            false,
            reveal
                .complete
                .then_some(session.puzzle().dimensions().cell_count()),
            session.attempt().ink(),
            session.target_visible(),
            profile,
        )
    } else {
        render_session_boards(
            frame,
            regions[0],
            session.puzzle(),
            session.attempt(),
            Some(session.cursor()),
            session.cursor().row(),
            session.preview_action(),
            session.unfolded_preview(),
            None,
            session.attempt().ink(),
            session.target_visible(),
            profile,
        )
    };
    if reveal.is_none() && matches!(session.source(), PlaySource::Lesson) {
        render_lesson_coach(frame, target_area, session, bindings, profile);
    }
    let reveal_state = reveal
        .as_ref()
        .map(|reveal| (reveal.opened_folds, reveal.total_folds, reveal.complete));
    render_session_status(frame, regions[1], session, bindings, profile, reveal_state);
    if reveal.as_ref().is_some_and(|reveal| reveal.complete) && session.saved() {
        if let Some(group) = group_completion {
            CompletionCourier::render(frame, regions[0], group, profile);
        } else if !matches!(session.source(), PlaySource::Keepsake) {
            render_success_card(frame, regions[0], session, bindings, profile);
        }
    }
}

fn render_success_card(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
) {
    let result = session.result().expect("success card requires a result");
    debug_assert!(result.is_success());
    let score = result.score();
    let (headline, detail) = if matches!(session.source(), PlaySource::Lesson) {
        (
            "Congratulations, your first paper matches.".to_owned(),
            "The branch is ready for the journey.".to_owned(),
        )
    } else {
        match (session.puzzle().par(), result.meets_par()) {
            (Some(reference), Some(true)) => (
                "Congratulations, the opened paper matches.".to_owned(),
                format!(
                    "Reference path found: {} and {}.",
                    counted(reference.folds().get(), "fold"),
                    counted(reference.strokes().get(), "stroke")
                ),
            ),
            (Some(reference), Some(false)) => (
                "Congratulations, you found the pattern.".to_owned(),
                format!(
                    "You used {} and {}; the reference is {} and {}.",
                    counted(score.folds().get(), "fold"),
                    counted(score.strokes().get(), "stroke"),
                    counted(reference.folds().get(), "fold"),
                    counted(reference.strokes().get(), "stroke")
                ),
            ),
            (None, None) => (
                "Congratulations, the opened paper matches.".to_owned(),
                format!(
                    "Solved with {} and {}.",
                    counted(score.folds().get(), "fold"),
                    counted(score.strokes().get(), "stroke")
                ),
            ),
            (Some(_), None) | (None, Some(_)) => (
                "Congratulations, the opened paper matches.".to_owned(),
                "The reference score is unavailable.".to_owned(),
            ),
        }
    };
    let encouragement = if result.meets_par() == Some(false) {
        "A shorter path is still hiding, but this one is safely yours."
    } else {
        "Your keepsake is saved safely."
    };
    let separator = match profile.glyph_mode() {
        GlyphMode::Unicode => " · ",
        GlyphMode::Ascii => " | ",
    };
    let controls = if matches!(session.source(), PlaySource::Lesson) {
        format!(
            "Enter returns to the branch{separator}{} retries",
            bindings.reset
        )
    } else {
        format!(
            "Enter back{separator}{} retry{separator}v replay{separator}x keepsake",
            bindings.reset,
        )
    };
    let preferred_height = if area.width >= 74 { 7 } else { 9 };
    let height = area.height.min(preferred_height);
    let card = centered(area, 74, height);
    frame.render_widget(Clear, card);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(headline, profile.title()).alignment(Alignment::Center),
            Line::from(detail).alignment(Alignment::Center),
            Line::styled(encouragement, profile.paper()).alignment(Alignment::Center),
            Line::styled(controls, StyleProfile::muted()).alignment(Alignment::Center),
        ])
        .block(Paper::block("Paper complete", profile))
        .wrap(Wrap { trim: true }),
        card,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_session_boards(
    frame: &mut Frame<'_>,
    area: Rect,
    puzzle: &crate::domain::puzzle::Puzzle,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    focus_row: Row,
    preview: Option<PaperAction>,
    unfolded: bool,
    comparison_reveal: Option<usize>,
    ink: InkPattern,
    target_visible: bool,
    profile: StyleProfile,
) -> Rect {
    if compact_boards_required(area, puzzle) {
        render_compact_boards(
            frame,
            area,
            puzzle,
            attempt,
            cursor,
            focus_row,
            preview,
            unfolded,
            comparison_reveal,
            ink,
            target_visible,
            profile,
        );
        Rect::default()
    } else {
        render_attempt_boards(
            frame,
            area,
            puzzle,
            attempt,
            cursor,
            preview,
            unfolded,
            comparison_reveal,
            ink,
            profile,
        )
    }
}

fn compact_boards_required(area: Rect, puzzle: &crate::domain::puzzle::Puzzle) -> bool {
    if area.width < 80 {
        return true;
    }
    let regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(40),
            Constraint::Percentage(24),
        ])
        .split(area);
    let dimensions = puzzle.dimensions();
    let grid_width = u16::from(dimensions.width().get()).saturating_mul(2) + 2;
    let grid_height = u16::from(dimensions.height().get()) + 2;
    regions[0].width < grid_width || regions[1].width < grid_width || area.height < grid_height
}

#[allow(clippy::too_many_arguments)]
fn render_compact_boards(
    frame: &mut Frame<'_>,
    area: Rect,
    puzzle: &crate::domain::puzzle::Puzzle,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    focus_row: Row,
    preview: Option<PaperAction>,
    unfolded: bool,
    comparison_reveal: Option<usize>,
    ink: InkPattern,
    target_visible: bool,
    profile: StyleProfile,
) {
    let regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(area);
    let focus_row = focus_row.get();
    if target_visible {
        render_grid_window(
            frame,
            regions[0],
            "PATTERN TO MATCH",
            target_grid(puzzle, profile),
            focus_row,
            true,
            profile,
        );
    } else {
        let (title, grid) = if let Some(revealed) = comparison_reveal {
            (
                "OPENED COMPARISON",
                comparison_grid(attempt, puzzle, ink, revealed, profile),
            )
        } else if unfolded {
            ("UNFOLDED PREVIEW", unfolded_grid(attempt, ink, profile))
        } else {
            (
                "FOLDED PAPER",
                folded_grid(attempt, cursor, preview, ink, profile),
            )
        };
        render_grid_window(frame, regions[0], title, grid, focus_row, true, profile);
    }
    render_stack(frame, regions[1], attempt, cursor, preview, ink, profile);
}

#[allow(clippy::too_many_arguments)]
fn render_attempt_boards(
    frame: &mut Frame<'_>,
    area: Rect,
    puzzle: &crate::domain::puzzle::Puzzle,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    preview: Option<PaperAction>,
    unfolded: bool,
    comparison_reveal: Option<usize>,
    ink: InkPattern,
    profile: StyleProfile,
) -> Rect {
    let regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(40),
            Constraint::Percentage(24),
        ])
        .split(area);
    let target_title = if regions[0].width >= 24 {
        "Pattern to match"
    } else {
        "Goal"
    };
    render_grid(
        frame,
        regions[0],
        target_title,
        target_grid(puzzle, profile),
        false,
        profile,
    );
    let active = cursor.is_some();
    let title = if comparison_reveal.is_some() {
        if active && regions[1].width < 28 {
            "RESULT"
        } else if active {
            "OPENED COMPARISON"
        } else {
            "Opened comparison"
        }
    } else if unfolded {
        if active && regions[1].width < 28 {
            "PREVIEW"
        } else if active {
            "UNFOLDED PREVIEW"
        } else {
            "Unfolded ink preview"
        }
    } else if active && regions[1].width < 28 {
        "PAPER"
    } else if active {
        "FOLDED PAPER"
    } else {
        "Folded paper"
    };
    let grid = if let Some(revealed) = comparison_reveal {
        comparison_grid(attempt, puzzle, ink, revealed, profile)
    } else if unfolded {
        unfolded_grid(attempt, ink, profile)
    } else {
        folded_grid(attempt, cursor, preview, ink, profile)
    };
    render_grid(frame, regions[1], title, grid, active, profile);
    render_stack(frame, regions[2], attempt, cursor, preview, ink, profile);
    regions[0]
}

fn render_lesson_coach(
    frame: &mut Frame<'_>,
    target_area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
) {
    const SQUIRREL_WIDTH: u16 = 10;
    const GAP_WIDTH: u16 = 1;
    const MINIMUM_BUBBLE_WIDTH: u16 = 18;
    const MINIMUM_COACH_HEIGHT: u16 = 7;
    const COACH_HEIGHT: u16 = 7;

    let inner_width = target_area.width.saturating_sub(2);
    let grid_bottom = target_area
        .y
        .saturating_add(1)
        .saturating_add(u16::from(session.puzzle().dimensions().height().get()));
    let coach_top = grid_bottom.saturating_add(1);
    let inner_bottom = target_area.bottom().saturating_sub(1);
    let available_height = inner_bottom.saturating_sub(coach_top);
    let minimum_width = SQUIRREL_WIDTH
        .saturating_add(GAP_WIDTH)
        .saturating_add(MINIMUM_BUBBLE_WIDTH);
    if inner_width < minimum_width || available_height < MINIMUM_COACH_HEIGHT {
        return;
    }

    let coach_area = Rect::new(
        target_area.x.saturating_add(1),
        coach_top,
        inner_width,
        available_height.min(COACH_HEIGHT),
    );
    let regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(SQUIRREL_WIDTH),
            Constraint::Length(GAP_WIDTH),
            Constraint::Min(MINIMUM_BUBBLE_WIDTH),
        ])
        .split(coach_area);
    frame.render_widget(
        Paragraph::new(
            courier_art(profile.glyph_mode())
                .iter()
                .map(|line| Line::from(*line))
                .collect::<Vec<_>>(),
        )
        .style(profile.paper())
        .alignment(Alignment::Center),
        regions[0],
    );
    let pointer = Rect::new(
        regions[1].x,
        regions[1].y.saturating_add(regions[1].height.min(4) / 2),
        1,
        1,
    );
    frame.render_widget(Paragraph::new("<").style(profile.paper()), pointer);
    frame.render_widget(
        Paragraph::new(lesson_coach_message(session, bindings))
            .block(Paper::block("Squirrel says", profile))
            .wrap(Wrap { trim: true }),
        regions[2],
    );
}

fn lesson_coach_message(session: &PlaySession, bindings: KeyBindings) -> String {
    let fold_count = session.attempt().fold_count().get();
    let stroke_count = session.attempt().stroke_count().get();
    match (fold_count, stroke_count) {
        (0, 0) => match session.draft() {
            Some(Draft::Fold(_)) => {
                "The fold tool is ready.\nPress Enter to fold the + side.".to_owned()
            }
            Some(Draft::Brush(_)) => {
                "The fold comes first.\nPress Shift+Tab to go back.".to_owned()
            }
            None => "Open comes later.\nPress Tab to ready the fold.".to_owned(),
        },
        (0, _) => format!(
            "The ink came before the fold.\nPress {} to undo it.",
            bindings.undo
        ),
        (_, 0) => {
            let Some(ink_coordinate) = lesson_ink_coordinate(session) else {
                return format!(
                    "The target cells are not stacked.\nPress {} and try the fold again.",
                    bindings.undo
                );
            };
            if session.cursor() != ink_coordinate {
                return format!(
                    "Move @ to row {}, column {} with the arrow keys.",
                    ink_coordinate.row().get() + 1,
                    ink_coordinate.column().get() + 1
                );
            }
            match session.draft() {
                Some(Draft::Brush(_)) => {
                    "The dot brush is ready.\nEnter inks both layers.".to_owned()
                }
                Some(Draft::Fold(_)) => {
                    "That fold is done.\nPress Tab for the dot brush.".to_owned()
                }
                None => "You found the stack.\nPress Tab for the dot brush.".to_owned(),
            }
        }
        (_, _) if session.attempt().result().is_success() => {
            "The target is inked.\nPress Enter to open.".to_owned()
        }
        (_, _) => format!(
            "That dot missed the target.\nPress {} to undo the last step.",
            bindings.undo
        ),
    }
}

fn lesson_ink_coordinate(session: &PlaySession) -> Option<Coordinate> {
    let mut target_ids = session.puzzle().target().cell_ids();
    let first = target_ids.next()?;
    let coordinate = session.attempt().physical_cell(first)?.coordinate();
    for id in target_ids {
        if session.attempt().physical_cell(id)?.coordinate() != coordinate {
            return None;
        }
    }
    Some(coordinate)
}

fn render_grid(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: Vec<Line<'static>>,
    active: bool,
    profile: StyleProfile,
) {
    let block = if active {
        Paper::highlighted_block(title, profile)
    } else {
        Paper::block(title, profile)
    };
    frame.render_widget(
        Paragraph::new(rows)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

fn render_grid_window(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: Vec<Line<'static>>,
    focus_row: u8,
    active: bool,
    profile: StyleProfile,
) {
    let visible = usize::from(area.height.saturating_sub(2)).min(rows.len());
    let focus = usize::from(focus_row).min(rows.len().saturating_sub(1));
    let start = focus
        .saturating_add(1)
        .saturating_sub(visible)
        .min(rows.len().saturating_sub(visible));
    let end = start.saturating_add(visible);
    let title = if visible < rows.len() {
        format!("{title} rows {}-{end}/{}", start + 1, rows.len())
    } else {
        title.to_owned()
    };
    render_grid(
        frame,
        area,
        &title,
        rows.into_iter().skip(start).take(visible).collect(),
        active,
        profile,
    );
}

fn target_grid(
    puzzle: &crate::domain::puzzle::Puzzle,
    profile: StyleProfile,
) -> Vec<Line<'static>> {
    let dimensions = puzzle.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            grid_line(
                (0..dimensions.width().get())
                    .map(|column| {
                        let coordinate =
                            dimensions.coordinate(row, column).expect("grid coordinate");
                        let id = dimensions.cell_id(coordinate).expect("grid identity");
                        if puzzle.target().contains(id) {
                            ('#', profile.paper())
                        } else {
                            ('.', StyleProfile::muted())
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

fn unfolded_grid(attempt: &Attempt, ink: InkPattern, profile: StyleProfile) -> Vec<Line<'static>> {
    let dimensions = attempt.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            grid_line(
                (0..dimensions.width().get())
                    .map(|column| {
                        let coordinate =
                            dimensions.coordinate(row, column).expect("grid coordinate");
                        let id = dimensions.cell_id(coordinate).expect("grid identity");
                        if ink.contains(id) {
                            (ink_symbol(profile.glyph_mode()), profile.ink_mark())
                        } else {
                            ('.', StyleProfile::muted())
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

fn comparison_grid(
    attempt: &Attempt,
    puzzle: &crate::domain::puzzle::Puzzle,
    ink: InkPattern,
    revealed: usize,
    profile: StyleProfile,
) -> Vec<Line<'static>> {
    let dimensions = attempt.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            grid_line(
                (0..dimensions.width().get())
                    .map(|column| {
                        let coordinate =
                            dimensions.coordinate(row, column).expect("grid coordinate");
                        let id = dimensions.cell_id(coordinate).expect("grid identity");
                        if id.index() >= revealed {
                            return (' ', Style::default());
                        }
                        match (puzzle.target().contains(id), ink.contains(id)) {
                            (true, true) => ('#', profile.ink_mark()),
                            (true, false) => ('?', profile.error()),
                            (false, true) => ('!', profile.error()),
                            (false, false) => ('.', StyleProfile::muted()),
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

fn folded_grid(
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    preview: Option<PaperAction>,
    ink_pattern: InkPattern,
    profile: StyleProfile,
) -> Vec<Line<'static>> {
    let dimensions = attempt.dimensions();
    let footprint = preview_footprint(preview, dimensions);
    let mut stacks = [(0_u8, false); MAX_PHYSICAL_CELLS];
    for id in attempt.cell_ids() {
        let physical = attempt
            .physical_cell(id)
            .expect("attempt exposes every physical cell");
        let position = dimensions
            .cell_id(physical.coordinate())
            .expect("cell position");
        let (count, ink) = &mut stacks[position.index()];
        *count += 1;
        *ink |= ink_pattern.contains(id);
    }
    (0..dimensions.height().get())
        .map(|row| {
            grid_line(
                (0..dimensions.width().get())
                    .map(|column| {
                        let coordinate =
                            dimensions.coordinate(row, column).expect("grid coordinate");
                        let position = dimensions.cell_id(coordinate).expect("grid position");
                        let (count, ink) = stacks[position.index()];
                        if cursor == Some(coordinate) && ink {
                            (ink_cursor_symbol(profile.glyph_mode()), profile.ink_mark())
                        } else if cursor == Some(coordinate) {
                            ('@', profile.active())
                        } else if footprint.contains(&coordinate) {
                            ('+', profile.paper())
                        } else if ink {
                            (ink_symbol(profile.glyph_mode()), profile.ink_mark())
                        } else if count == 0 {
                            (' ', Style::default())
                        } else if count == 1 {
                            ('o', profile.ink())
                        } else {
                            (
                                char::from_digit(u32::from(count.min(9)), 10).unwrap_or('9'),
                                profile.ink(),
                            )
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

fn grid_line(cells: Vec<(char, Style)>) -> Line<'static> {
    Line::from(
        cells
            .into_iter()
            .flat_map(|(symbol, style)| [Span::styled(symbol.to_string(), style), Span::raw(" ")])
            .collect::<Vec<_>>(),
    )
}

const fn ink_symbol(glyph_mode: GlyphMode) -> char {
    match glyph_mode {
        GlyphMode::Unicode => '●',
        GlyphMode::Ascii => '*',
    }
}

const fn ink_cursor_symbol(glyph_mode: GlyphMode) -> char {
    match glyph_mode {
        GlyphMode::Unicode => '◉',
        GlyphMode::Ascii => '&',
    }
}

fn preview_footprint(
    preview: Option<PaperAction>,
    dimensions: crate::domain::paper::Dimensions,
) -> Vec<Coordinate> {
    match preview {
        Some(PaperAction::Dot(coordinate)) => vec![coordinate],
        Some(PaperAction::Line(line)) => {
            let start = line.start();
            let end = line.end();
            if start.row() == end.row() {
                (start.column().get()..=end.column().get())
                    .filter_map(|column| {
                        crate::domain::paper::Column::new(column)
                            .ok()
                            .map(|column| Coordinate::new(start.row(), column))
                    })
                    .collect()
            } else {
                (start.row().get()..=end.row().get())
                    .filter_map(|row| {
                        crate::domain::paper::Row::new(row)
                            .ok()
                            .map(|row| Coordinate::new(row, start.column()))
                    })
                    .collect()
            }
        }
        Some(PaperAction::Fold(fold)) => (0..dimensions.height().get())
            .flat_map(|row| {
                (0..dimensions.width().get()).filter_map(move |column| {
                    let moving = match fold.direction() {
                        FoldDirection::Left => column >= fold.crease(),
                        FoldDirection::Right => column < fold.crease(),
                        FoldDirection::Up => row >= fold.crease(),
                        FoldDirection::Down => row < fold.crease(),
                    };
                    if moving {
                        dimensions.coordinate(row, column).ok()
                    } else {
                        None
                    }
                })
            })
            .collect(),
        None => Vec::new(),
    }
}

fn action_coordinate(action: PaperAction) -> Option<Coordinate> {
    match action {
        PaperAction::Dot(coordinate) => Some(coordinate),
        PaperAction::Line(line) => Some(line.start()),
        PaperAction::Fold(_) => None,
    }
}

fn render_stack(
    frame: &mut Frame<'_>,
    area: Rect,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    preview: Option<PaperAction>,
    ink: InkPattern,
    profile: StyleProfile,
) {
    let mut lines = Vec::new();
    if let Some(coordinate) = cursor {
        let mut stack = crate::domain::paper::StackView::new();
        attempt
            .stack_at(coordinate, &mut stack)
            .expect("cursor stack is inside the paper");
        lines.push(Line::styled(
            format!(
                "row {}, column {}",
                coordinate.row().get() + 1,
                coordinate.column().get() + 1
            ),
            profile.title(),
        ));
        if stack.is_empty() {
            lines.push(Line::from("empty"));
        }
        for (layer, id) in stack.cell_ids().iter().enumerate() {
            let ink = if ink.contains(*id) { " ink" } else { "" };
            lines.push(Line::from(format!("{layer}: cell {}{ink}", id.get())));
        }
    } else {
        lines.push(Line::from("Select a cell to inspect its layers."));
    }
    if let Some(PaperAction::Fold(fold)) = preview {
        lines.push(Line::from(""));
        lines.push(Line::styled(fold_label(fold), profile.paper()));
    }
    let title = if area.width < 22 {
        "Low to high"
    } else {
        "Stack, bottom to top"
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Paper::block(title, profile))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_session_status(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    reveal: Option<(usize, usize, bool)>,
) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            SafeText::external_display(&session.title(), 80, profile.glyph_mode())
                .as_str()
                .to_owned(),
            profile.title(),
        ),
        Span::from(format!(
            "  Folds {}/{}  Ink {}/{}",
            session.attempt().fold_count().get(),
            session.puzzle().fold_budget().get(),
            session.attempt().stroke_count().get(),
            session.puzzle().stroke_budget().get()
        )),
    ])];
    if session.result().is_some() {
        lines.extend(result_status_lines(
            session, bindings, profile, reveal, area.width,
        ));
    } else {
        lines.extend(active_status_lines(session, bindings, profile, area.width));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Paper::block("Paper", profile))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn result_status_lines(
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    reveal: Option<(usize, usize, bool)>,
    width: u16,
) -> Vec<Line<'static>> {
    if let Some((opened, total, false)) = reveal {
        return vec![
            Line::from(format!(
                "Opening crease {opened}/{total}; the final comparison follows."
            )),
            Line::styled(
                if session.saved() {
                    "The matched result is already saved safely."
                } else {
                    "The matched result is being saved before confirmation."
                },
                profile.paper(),
            ),
        ];
    }
    let result = session.result().expect("result status requires a result");
    let comparison = result.comparison();
    let mut lines = vec![Line::from(format!(
        "Opened paper: {} missing (?) and {} extra (!).",
        comparison.missing().len(),
        comparison.extra().len()
    ))];
    if !result.is_success() {
        lines.push(Line::styled(
            format!(
                "Up/Down inspect rows; Enter returns to the attempt; {} starts over.",
                bindings.reset
            ),
            profile.error(),
        ));
    } else if !session.saved() {
        lines.push(Line::styled(
            "Matched. Saving before success is confirmed...",
            profile.paper(),
        ));
    } else if matches!(session.source(), super::session::PlaySource::Lesson) {
        lines.push(Line::styled(
            "Lesson complete. Enter returns to the home branch.",
            profile.paper(),
        ));
    } else if matches!(session.source(), super::session::PlaySource::Keepsake) {
        lines.push(Line::styled(
            "Replay complete. This saved paper matches exactly.",
            profile.paper(),
        ));
    } else {
        lines.push(Line::styled(
            saved_score_line(session, result, width),
            profile.paper(),
        ));
    }
    lines
}

fn saved_score_line(
    session: &PlaySession,
    result: crate::domain::score::AttemptResult,
    width: u16,
) -> String {
    let score = result.score();
    match (session.puzzle().par(), result.meets_par()) {
        (Some(reference), Some(true)) if width >= 80 => format!(
            "Saved. You matched the reference: {}, {}.",
            counted(reference.folds().get(), "fold"),
            counted(reference.strokes().get(), "stroke")
        ),
        (Some(reference), Some(true)) => format!(
            "Saved. Reference matched: {}F/{}S.",
            reference.folds().get(),
            reference.strokes().get()
        ),
        (Some(reference), Some(false)) if width >= 80 => format!(
            "Saved. You used {}, {}; reference: {}, {}.",
            counted(score.folds().get(), "fold"),
            counted(score.strokes().get(), "stroke"),
            counted(reference.folds().get(), "fold"),
            counted(reference.strokes().get(), "stroke")
        ),
        (Some(reference), Some(false)) => format!(
            "Saved. Used {}F/{}S; reference {}F/{}S.",
            score.folds().get(),
            score.strokes().get(),
            reference.folds().get(),
            reference.strokes().get()
        ),
        (None, None) if width >= 80 => format!(
            "Saved in {} and {}.",
            counted(score.folds().get(), "fold"),
            counted(score.strokes().get(), "stroke")
        ),
        (None, None) => format!(
            "Saved in {}F/{}S.",
            score.folds().get(),
            score.strokes().get()
        ),
        (Some(_), None) | (None, Some(_)) => {
            "Saved. The reference score is unavailable.".to_owned()
        }
    }
}

fn counted(count: u8, noun: &str) -> String {
    let suffix = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{suffix}")
}

fn active_status_lines(
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    width: u16,
) -> Vec<Line<'static>> {
    if let Some(progress) = session.replay_progress() {
        let line = session.action_feedback().map_or_else(
            || {
                if progress.total_actions == 0 {
                    return "Replay ready: this paper has no recorded actions. Enter opens it."
                        .to_owned();
                }
                format!(
                    "Replay ready: fresh paper. Enter or Right shows step 1 of {}.",
                    progress.total_actions
                )
            },
            str::to_owned,
        );
        return vec![Line::styled(line, profile.paper())];
    }
    let ready = ready_tool_line(session, width < 80, profile.glyph_mode());
    let history = action_history(session.attempt());
    let cue_limit = if width >= 80 {
        120
    } else {
        usize::from(width.saturating_sub(4))
    };
    let authored_cue = session
        .cues()
        .get(usize::from(session.attempt().action_count().get()));
    let guidance = if matches!(session.source(), PlaySource::Lesson) {
        let message = lesson_coach_message(session, bindings).replace('\n', " ");
        Some((
            "Guide",
            SafeText::external_display(&message, cue_limit, profile.glyph_mode()),
        ))
    } else if let Some(cue) = authored_cue {
        let label = if matches!(session.source(), PlaySource::Journey(0)) {
            "Guide"
        } else {
            "Hint"
        };
        Some((
            label,
            SafeText::external_display(cue, cue_limit, profile.glyph_mode()),
        ))
    } else if matches!(session.source(), PlaySource::Journey(_)) {
        session.attempt().hints_used().then(|| {
            (
                "Hint",
                SafeText::external_display(&session.description(), cue_limit, profile.glyph_mode()),
            )
        })
    } else if session.description().is_empty() {
        None
    } else {
        Some((
            "Note",
            SafeText::external_display(&session.description(), cue_limit, profile.glyph_mode()),
        ))
    };
    let guidance = guidance.map(|(label, text)| format!("{label}: {}", text.as_str()));
    if width >= 80 {
        let mut lines = vec![Line::from(ready)];
        if let Some(feedback) = session.action_feedback() {
            lines.push(Line::styled(
                format!("Last step: {feedback}"),
                profile.paper(),
            ));
        }
        if let Some(history) = history {
            lines.push(Line::styled(history, StyleProfile::muted()));
        }
        if let Some(guidance) = guidance {
            lines.push(Line::styled(guidance, profile.paper()));
        }
        lines
    } else {
        let mut lines = vec![Line::from(
            session.action_feedback().map_or(ready, str::to_owned),
        )];
        if let Some(guidance) = guidance {
            lines.push(Line::styled(guidance, profile.paper()));
        }
        lines
    }
}

fn ready_tool_line(session: &PlaySession, compact: bool, glyphs: GlyphMode) -> String {
    let separator = if glyphs == GlyphMode::Unicode {
        " · "
    } else {
        " | "
    };
    match session.draft() {
        Some(Draft::Fold(index)) => session
            .puzzle()
            .allowed_folds()
            .get(index)
            .copied()
            .map_or_else(
                || "Fold unavailable. Tab chooses another tool.".to_owned(),
                |fold| {
                    if compact {
                        format!(
                            "Ready: Fold {}, crease {}{separator}Enter folds",
                            fold.direction(),
                            fold.crease()
                        )
                    } else {
                        format!(
                            "Ready: Fold {}, crease {}{separator}+ moves{separator}Enter folds",
                            fold.direction(),
                            fold.crease()
                        )
                    }
                },
            ),
        Some(Draft::Brush(index)) => session
            .puzzle()
            .allowed_brushes()
            .get(index)
            .copied()
            .map_or_else(
                || "Brush unavailable. Tab chooses another tool.".to_owned(),
                |brush| match (brush, compact) {
                    (crate::domain::paper::BrushRule::Dot, true) => {
                        format!("Ready: Dot brush{separator}Enter inks")
                    }
                    (crate::domain::paper::BrushRule::Dot, false) => {
                        format!(
                            "Ready: Dot brush{separator}Arrows move @{separator}Enter inks every layer"
                        )
                    }
                    (crate::domain::paper::BrushRule::Line { axis, length }, true) => {
                        format!("Ready: {length}-cell {axis} line{separator}Enter inks")
                    }
                    (crate::domain::paper::BrushRule::Line { axis, length }, false) => format!(
                        "Ready: {length}-cell {axis} line{separator}Arrows move @{separator}Enter inks its preview"
                    ),
                },
            ),
        None if compact => format!("Ready: Open paper{separator}Enter compares"),
        None => format!("Ready: Open paper{separator}Enter unfolds and compares"),
    }
}

fn action_history(attempt: &Attempt) -> Option<String> {
    let actions = attempt.actions().collect::<Vec<_>>();
    if actions.is_empty() {
        return None;
    }
    let start = actions.len().saturating_sub(3);
    let labels = actions[start..]
        .iter()
        .copied()
        .map(action_label)
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = if start > 0 { "... " } else { "" };
    Some(format!("Made: {prefix}{labels}"))
}

fn status_text(app: &App, glyphs: GlyphMode, width: u16) -> String {
    let separator = if glyphs == GlyphMode::Unicode {
        " · "
    } else {
        " | "
    };
    let bindings = app.settings().bindings;
    match app.screen() {
        Screen::Capabilities => format!(
            "Enter start{separator}{} help{separator}{} quit",
            bindings.help, bindings.quit
        ),
        Screen::Play => play_status_text(app, separator, width),
        Screen::HowTo => format!(
            "Left/Right step{separator}Enter next{separator}Esc back{separator}{} help",
            bindings.help
        ),
        Screen::Loading => format!(
            "Esc cancel{separator}{} help{separator}{} quit",
            bindings.help, bindings.quit
        ),
        Screen::Branch
        | Screen::Journey
        | Screen::Packs
        | Screen::PackPuzzles
        | Screen::Keepsakes
        | Screen::Settings => format!(
            "Up/Down move{separator}Enter open{separator}{} help{separator}{} quit",
            bindings.help, bindings.quit
        ),
    }
}

fn play_status_text(app: &App, separator: &str, width: u16) -> String {
    let bindings = app.settings().bindings;
    let Some(session) = app.session() else {
        return format!("{} help{separator}{} quit", bindings.help, bindings.quit);
    };
    if session.replay_progress().is_some() {
        return replay_status_text(session, bindings, separator, width);
    }
    if let Some(result) = session.result() {
        if !result.is_success() {
            return if width >= 70 {
                format!(
                    "Up/Down inspect{separator}Enter retry{separator}{} reset{separator}{} help{separator}{} quit",
                    bindings.reset, bindings.help, bindings.quit
                )
            } else {
                format!(
                    "Up/Down{separator}Enter retry{separator}{} reset{separator}{}{separator}{} quit",
                    bindings.reset, bindings.help, bindings.quit
                )
            };
        }
        if !session.saved() {
            return format!(
                "Saving{separator}{} help{separator}{} quit",
                bindings.help, bindings.quit
            );
        }
        if matches!(session.source(), PlaySource::Lesson) {
            return format!(
                "Enter branch{separator}{} retry{separator}{} help{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            );
        }
        return if width >= 70 {
            format!(
                "Enter back{separator}{} retry{separator}v replay{separator}x keepsake{separator}{} help{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            )
        } else {
            format!(
                "Enter back{separator}{} retry{separator}v{separator}x{separator}{}{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            )
        };
    }
    let draft = session.draft();
    if width < 80 {
        let target = if session.target_visible() {
            "paper"
        } else {
            "target"
        };
        let controls = match (draft, width >= 70) {
            (Some(Draft::Fold(_)), true) => {
                format!("Arrows change fold{separator}Tab tool/open{separator}Enter fold")
            }
            (Some(Draft::Brush(_)), true) => {
                format!("Arrows move @{separator}Tab tool/open{separator}Enter ink")
            }
            (None, true) => {
                format!("Arrows inspect{separator}Tab tool/open{separator}Enter open")
            }
            (Some(Draft::Fold(_)), false) => {
                format!("Arrows fold{separator}Tab{separator}Enter")
            }
            (Some(Draft::Brush(_)), false) => {
                format!("Arrows @{separator}Tab{separator}Enter ink")
            }
            (None, false) => format!("@{separator}Tab tool{separator}Enter open"),
        };
        return format!(
            "{controls}{separator}t {target}{separator}{} help{separator}{} quit",
            bindings.help, bindings.quit
        );
    }
    let controls = match draft {
        Some(Draft::Fold(_)) => "Arrows change fold",
        Some(Draft::Brush(_)) => "Arrows move @",
        None => "Arrows inspect",
    };
    let enter = match draft {
        Some(Draft::Fold(_)) => "Enter fold",
        Some(Draft::Brush(_)) => "Enter ink",
        None => "Enter open",
    };
    format!(
        "{controls}{separator}Tab tool/open{separator}{enter}{separator}{} help{separator}{} quit",
        bindings.help, bindings.quit
    )
}

fn replay_status_text(
    session: &PlaySession,
    bindings: KeyBindings,
    separator: &str,
    width: u16,
) -> String {
    if session.result().is_some() {
        return if width >= 80 {
            format!(
                "Left rewind{separator}Up/Down inspect{separator}Enter back{separator}{} restart{separator}x keepsake{separator}{} help{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            )
        } else {
            format!(
                "Left{separator}Up/Down{separator}Enter back{separator}{} restart{separator}{}{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            )
        };
    }
    if session
        .replay_progress()
        .is_some_and(|progress| progress.total_actions == 0)
    {
        return format!(
            "Enter open{separator}{} restart{separator}{} help{separator}{} quit",
            bindings.reset, bindings.help, bindings.quit
        );
    }
    if width >= 70 {
        format!(
            "Left/Right step{separator}Enter next/open{separator}{} restart{separator}{} help{separator}{} quit",
            bindings.reset, bindings.help, bindings.quit
        )
    } else {
        format!(
            "Left/Right{separator}Enter next{separator}{} restart{separator}{}{separator}{} quit",
            bindings.reset, bindings.help, bindings.quit
        )
    }
}

fn render_owned_focus(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    choices: &[String],
    selected: usize,
    profile: StyleProfile,
) {
    let marker = if profile.glyph_mode() == GlyphMode::Unicode {
        "›"
    } else {
        ">"
    };
    let items = choices.iter().enumerate().map(|(index, choice)| {
        let safe = SafeText::external_display(choice, 160, profile.glyph_mode());
        let line = if index == selected {
            Line::styled(format!("{marker} {}", safe.as_str()), profile.active())
        } else {
            Line::styled(format!("  {}", safe.as_str()), profile.ink())
        };
        ListItem::new(line)
    });
    let mut state = ListState::default();
    state.select((selected < choices.len()).then_some(selected));
    frame.render_stateful_widget(
        List::new(items).block(Paper::block(title, profile)),
        area,
        &mut state,
    );
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn content_area(shell: ShellLayout) -> Rect {
    let x = shell.mark.x.min(shell.branch.x);
    let y = shell.mark.y.min(shell.branch.y);
    Rect::new(
        x,
        y,
        shell
            .mark
            .right()
            .max(shell.branch.right())
            .saturating_sub(x),
        shell
            .mark
            .bottom()
            .max(shell.branch.bottom())
            .saturating_sub(y),
    )
}

fn render_resize_message(frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
    let lines = vec![
        Line::styled("Orifude is keeping your place.", profile.title()),
        Line::from(""),
        Line::from(format!(
            "Resize this terminal to at least {MINIMUM_WIDTH} columns by {MINIMUM_HEIGHT} rows."
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(profile.ink())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::generator::CalendarDate;
    use crate::storage::{GlyphMode, ProgressPage, PuzzleProgress, Settings};
    use crate::tui::style::ColorCapability;

    #[test]
    fn every_layout_renders_first_launch_without_losing_the_capability_message() {
        let now = Instant::now();
        let app = App::new(Settings::default(), now);
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        for (width, height) in [(80, 24), (60, 20), (59, 19)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("view renders");
            if width >= MINIMUM_WIDTH && height >= MINIMUM_HEIGHT {
                let text = rendered_text(&terminal);
                assert!(text.contains("Match the paper to the shown pattern."));
                assert!(text.contains("An available tool"));
                assert!(text.contains("Enter starts the lesson"));
                assert!(text.contains("Enter start"));
                assert!(!text.contains("Up/Down move"));
            }
        }
    }

    #[test]
    fn engine_session_renders_target_folded_paper_and_stack() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            now,
        );
        app.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('f'),
                crossterm::event::KeyModifiers::NONE,
            ),
            now,
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Ansi16, GlyphMode::Unicode);
        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("view renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("Pattern to match"));
        assert!(text.contains("FOLDED PAPER"));
        assert!(!text.contains("[ACTIVE]"));
        assert!(text.contains("Stack, bottom to top"));
        assert!(text.contains('+'));
        assert!(text.contains("Right at crease 2"));
        assert!(text.contains("Guide: The fold tool is ready."));
        assert!(text.contains("Squirrel says"));
        assert!(text.contains("Press Enter to fold the + side."));
    }

    #[test]
    fn lesson_coach_tracks_the_next_player_action() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let text = player_text(&app, now);
        assert!(text.contains("Squirrel says"));
        assert!(text.contains("The fold tool is ready."));
        assert!(player_text_with_glyphs(&app, now, GlyphMode::Ascii).is_ascii());

        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("Move @ to row 2, column 3"));

        press(&mut app, crossterm::event::KeyCode::Down, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        assert!(player_text(&app, now).contains("The dot brush is ready."));

        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("Enter opens the paper."));
    }

    #[test]
    fn lesson_coach_leads_the_player_back_from_misplaced_ink() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        press(&mut app, crossterm::event::KeyCode::Char('b'), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        let text = player_text(&app, now);
        assert!(text.contains("The ink came"));
        assert!(text.contains("fold."));

        press(&mut app, crossterm::event::KeyCode::Char('u'), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("That dot missed the target."));

        press(&mut app, crossterm::event::KeyCode::Char('u'), now);
        assert!(player_text(&app, now).contains("Move @ to row 2, column 3"));
    }

    #[test]
    fn ascii_profile_keeps_branch_and_player_buffers_ascii_only() {
        let now = Instant::now();
        let settings = Settings {
            glyph_mode: GlyphMode::Ascii,
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        let profile = StyleProfile::new(ColorCapability::Ansi16, GlyphMode::Ascii);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("branch renders");
        let branch = rendered_text(&terminal);
        assert!(branch.is_ascii());
        assert!(branch.contains("Home | Journey 0/40 | Saved no"));

        app.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            now,
        );
        app.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            now,
        );
        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("paper renders");
        assert!(rendered_text(&terminal).is_ascii());
    }

    #[test]
    fn branch_card_keeps_the_saved_status_complete() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        };
        let app = App::with_state(
            settings,
            ProgressPage {
                entries: vec![PuzzleProgress {
                    pack_id: "community-paper".into(),
                    puzzle_id: "first".into(),
                    attempt_count: 1,
                    best_folds: 0,
                    best_strokes: 1,
                    best_replay_id: 1,
                    updated_at_unix_seconds: 1,
                }],
                has_more: false,
            },
            Vec::new(),
            vec![true; crate::content::journey().len()],
            CalendarDate::new(2026, 9, 3).expect("valid date"),
            1,
            now,
        );

        let text = menu_text(&app, now, 100, 30);
        assert!(text.contains("Home | Journey 40/40 | Saved yes"));
    }

    #[test]
    fn completed_branch_is_readable_in_every_visual_profile_without_a_resident_squirrel() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        };
        let app = App::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            Vec::new(),
            vec![true; crate::content::journey().len()],
            CalendarDate::new(2026, 9, 3).unwrap(),
            1,
            now,
        );
        for (capability, glyphs) in [
            (ColorCapability::TrueColor, GlyphMode::Unicode),
            (ColorCapability::Ansi256, GlyphMode::Unicode),
            (ColorCapability::Ansi16, GlyphMode::Ascii),
            (ColorCapability::Monochrome, GlyphMode::Unicode),
            (ColorCapability::Monochrome, GlyphMode::Ascii),
        ] {
            let backend = TestBackend::new(100, 30);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let profile = StyleProfile::new(capability, glyphs);
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("completed branch renders");
            let text = rendered_text(&terminal);
            assert!(text.contains("the full canopy. [8/8]"));
            assert!(text.contains("Home | Journey 40/40"));
            assert!(!text.contains("/)_/)"));
            if glyphs == GlyphMode::Ascii {
                assert!(text.is_ascii());
            }
        }
    }

    #[test]
    fn preferred_minimum_keeps_branch_progress_and_stack_heading_complete() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        let branch = menu_text(&app, now, 80, 24)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(branch.contains("The branch is waiting for its first leaf."));
        assert!(branch.contains("[0/8]"));
        assert!(branch.contains("Home | Journey 0/40"));

        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        let paper = menu_text(&app, now, 80, 24);
        assert!(paper.contains("Low to high"));
        assert!(!paper.contains("Stack, bottom to to"));
    }

    #[test]
    fn minimum_player_layout_keeps_the_ready_tool_and_first_paper_cue_visible() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        for _ in 0..2 {
            app.handle_key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                ),
                now,
            );
        }
        let cue_prefix = app.session().expect("paper session").cues()[0]
            .chars()
            .take(24)
            .collect::<String>();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("minimum player renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("Ready: Dot brush"));
        assert!(text.contains(&cue_prefix));
        assert!(!text.contains("Actions: none"));
        assert!(text.contains("PAPER"));
        assert!(!text.contains("[ACTIVE]"));
        assert!(text.contains("Low to high"));
        assert!(text.contains("q quit"));
    }

    #[test]
    fn later_journey_papers_reveal_a_hint_only_after_a_missed_opening() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::with_state(
            settings,
            ProgressPage {
                entries: Vec::new(),
                has_more: false,
            },
            Vec::new(),
            vec![true; crate::content::journey().len()],
            CalendarDate::new(2026, 9, 3).expect("valid date"),
            1,
            now,
        );
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Down, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let fresh = menu_text(&app, now, 80, 24);
        assert!(!fresh.contains("The brush follows @; this paper stays flat."));
        assert!(!fresh.contains("Clue:"));
        assert!(!fresh.contains("Hint:"));

        press(&mut app, crossterm::event::KeyCode::Esc, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let after_miss = menu_text(&app, now, 80, 24);
        assert!(after_miss.contains("Hint: The brush follows @; this paper stays flat."));
        assert!(app.session().expect("paper session").attempt().hints_used());
    }

    #[test]
    fn compact_maximum_board_keeps_the_cursor_visible_and_switches_to_the_target() {
        use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};

        let identity = PuzzleIdentity::new("test-pack", "large-paper").unwrap();
        let puzzle = Puzzle::new(PuzzleSpec::new(identity, 12, 12)).unwrap();
        let mut session = PlaySession::new(
            &puzzle,
            "Large paper",
            "A layout boundary paper.",
            Vec::new(),
            PlaySource::Pack,
        );
        let now = Instant::now();
        for _ in 0..11 {
            session.handle_key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Down,
                    crossterm::event::KeyModifiers::NONE,
                ),
                KeyBindings::default(),
                now,
                true,
            );
        }
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        let session_area = Rect::new(2, 4, 56, 13);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    session_area,
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("compact paper renders");
        let paper = rendered_text(&terminal);
        assert!(paper.contains("FOLDED PAPER rows"));
        assert!(paper.contains("-12/12"));
        assert!(paper.contains('@'));
        assert!(!paper.contains("PATTERN TO MATCH"));

        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('t'),
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            now,
            true,
        );
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 56, 13),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("compact target renders");
        let target = rendered_text(&terminal);
        assert!(target.contains("PATTERN TO MATCH rows"));
        assert!(target.contains("-12/12"));
        assert!(!target.contains("FOLDED PAPER"));
    }

    #[test]
    fn failed_large_result_scrolls_without_moving_the_stack_cursor() {
        let paper = &crate::content::journey()[39];
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(39),
        );
        let now = Instant::now();
        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            now,
            true,
        );
        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            now,
            true,
        );
        assert!(session.result().is_some_and(|result| !result.is_success()));
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        let session_area = Rect::new(2, 4, 56, 13);
        let boards_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(6)])
            .split(session_area)[0];
        let stack_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(boards_area)[1];
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    session_area,
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("failed result renders");
        let first_result = rendered_text(&terminal);
        let first_stack = rendered_area_text(&terminal, stack_area);
        assert!(first_result.contains("OPENED COMPARISON rows 1-5/8"));

        for _ in 0..7 {
            session.handle_key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Down,
                    crossterm::event::KeyModifiers::NONE,
                ),
                KeyBindings::default(),
                now,
                true,
            );
        }
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    session_area,
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("scrolled failed result renders");
        let result = rendered_text(&terminal)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(result.contains("OPENED COMPARISON rows 4-8/8"));
        assert_eq!(rendered_area_text(&terminal, stack_area), first_stack);
        assert!(result.contains("Up/Down inspect rows"));
    }

    #[test]
    fn journey_mechanic_wraps_at_the_supported_minimum() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        for _ in 0..20 {
            press(&mut app, crossterm::event::KeyCode::Down, now);
        }

        let text = menu_text(&app, now, 60, 20)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            text.contains("Make later creases possible by choosing the earlier fold first."),
            "{text}"
        );
        assert!(text.contains("Journey 5: Fold order"), "{text}");
    }

    #[test]
    fn long_focus_lists_scroll_the_selected_choice_into_view() {
        let choices = (0..40)
            .map(|index| format!("paper-{index}"))
            .collect::<Vec<_>>();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_owned_focus(
                    frame,
                    Rect::new(0, 0, 60, 10),
                    "Papers",
                    &choices,
                    30,
                    profile,
                );
            })
            .expect("long menu renders");

        assert!(rendered_text(&terminal).contains("› paper-30"));
    }

    #[test]
    fn minimum_settings_capture_keeps_the_selected_binding_and_prompt_visible() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        for _ in 0..6 {
            press(&mut app, crossterm::event::KeyCode::Down, now);
        }
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        for _ in 0..10 {
            press(&mut app, crossterm::event::KeyCode::Down, now);
        }
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let text = menu_text(&app, now, 60, 20);
        assert!(text.contains("› Quit key: q"));
        assert!(text.contains("Press one unused key"));
    }

    #[test]
    fn minimum_saved_result_keeps_its_return_and_export_controls_visible() {
        let paper = &crate::content::journey()[0];
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Journey(0),
        );
        let now = Instant::now();
        for code in [
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Char('b'),
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Enter,
        ] {
            session.handle_key(
                crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
                KeyBindings::default(),
                now,
                true,
            );
        }
        session.mark_saved();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 56, 13),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("minimum result renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("Paper complete"));
        assert!(text.contains("Congratulations, the opened paper matches."));
        assert!(text.contains("Reference path found: 0 folds and 1 stroke."));
        assert!(text.contains("Enter back"));
        assert!(text.contains("x keepsake"));
        assert!(text.is_ascii());
    }

    #[test]
    fn ascii_cursor_preserves_whether_its_stack_contains_ink() {
        let paper = &crate::content::journey()[0];
        let mut attempt = paper.puzzle().start();
        let coordinate = paper
            .puzzle()
            .dimensions()
            .coordinate(0, 0)
            .expect("paper origin");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        let dry = folded_grid(&attempt, Some(coordinate), None, attempt.ink(), profile);
        attempt
            .stamp_dot(coordinate)
            .expect("the origin is an occupied stack");
        let cursor_on_ink = folded_grid(&attempt, Some(coordinate), None, attempt.ink(), profile);
        let placed_ink = folded_grid(&attempt, None, None, attempt.ink(), profile);

        let symbol = |lines: &[Line<'static>]| {
            lines[0].spans[0]
                .content
                .chars()
                .next()
                .expect("grid cell has one symbol")
        };
        assert_ne!(symbol(&dry), symbol(&cursor_on_ink));
        assert_ne!(symbol(&placed_ink), symbol(&cursor_on_ink));
        assert!(symbol(&cursor_on_ink).is_ascii());
    }

    #[test]
    fn above_reference_success_is_congratulatory_and_explains_the_shorter_path() {
        use crate::domain::paper::{BrushRule, CellId, FoldCount, StrokeCount};
        use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
        use crate::domain::score::Par;

        let puzzle = Puzzle::new(
            PuzzleSpec::new(
                PuzzleIdentity::new("test-pack", "two-strokes").expect("valid identity"),
                4,
                4,
            )
            .with_target_cells(vec![CellId::new(0).expect("valid cell")])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(0, 2)
            .with_par(Par::new(
                FoldCount::new(0).expect("valid fold count"),
                StrokeCount::new(1).expect("valid stroke count"),
            )),
        )
        .expect("valid test puzzle");
        let mut session = PlaySession::new(
            &puzzle,
            "Patient dot",
            "The same place can hold another touch.",
            Vec::new(),
            PlaySource::Pack,
        );
        let now = Instant::now();
        for code in [
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Enter,
        ] {
            session.handle_key(
                crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
                KeyBindings::default(),
                now,
                true,
            );
        }
        assert_eq!(
            session
                .result()
                .and_then(crate::domain::score::AttemptResult::meets_par),
            Some(false)
        );
        session.mark_saved();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 76, 17),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("above-reference result renders");
        let text = rendered_text(&terminal)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Congratulations, you found the pattern."));
        assert!(text.contains("the reference is 0 folds and 1 stroke."));
        assert!(text.contains("A shorter path is still hiding"));
    }

    #[test]
    fn saved_replay_steps_from_fresh_paper_to_the_opened_result() {
        let paper = &crate::content::journey()[0];
        let mut attempt = paper.puzzle().start();
        for &action in paper.solution() {
            attempt.apply(action).expect("recorded action applies");
        }
        let replay = crate::domain::replay::Replay::from_attempt(&attempt);
        let mut session = PlaySession::from_replay(paper.puzzle(), &replay, paper.title())
            .expect("recorded replay loads");
        let now = Instant::now();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 76, 17),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("saved replay renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("FOLDED PAPER"));
        assert!(text.contains("Replay ready: fresh paper."));
        assert!(!text.contains("OPENED COMPARISON"));
        assert!(
            replay_status_text(&session, KeyBindings::default(), " · ", 60).contains("Left/Right")
        );

        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            now,
            true,
        );
        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            now,
            true,
        );
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 76, 17),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                    None,
                );
            })
            .expect("completed replay renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("OPENED COMPARISON"));
        assert!(text.contains("Replay complete."));
        assert!(!text.contains("Paper complete"));
        assert!(!text.contains("Congratulations"));
        let minimum_footer = replay_status_text(&session, KeyBindings::default(), " · ", 56);
        assert!(minimum_footer.contains("Left"));
        assert!(minimum_footer.contains("Enter back"));
        assert!(minimum_footer.contains("q quit"));
        assert!(minimum_footer.chars().count() <= 56, "{minimum_footer}");
    }

    #[test]
    fn replay_without_actions_explains_that_enter_opens_the_paper() {
        use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
        use crate::domain::replay::{Replay, ReplayMetadata};

        let puzzle = Puzzle::new(PuzzleSpec::new(
            PuzzleIdentity::new("test-pack", "blank-paper").unwrap(),
            4,
            4,
        ))
        .unwrap();
        let replay = Replay::new(ReplayMetadata::current(&puzzle), Vec::new()).unwrap();
        let session = PlaySession::from_replay(&puzzle, &replay, "Blank paper").unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);

        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 76, 17),
                    &session,
                    KeyBindings::default(),
                    profile,
                    Instant::now(),
                    None,
                );
            })
            .expect("empty replay renders");

        assert!(rendered_text(&terminal).contains("no recorded actions. Enter opens it."));
        assert!(
            replay_status_text(&session, KeyBindings::default(), " · ", 76).contains("Enter open")
        );
    }

    #[test]
    fn saved_lesson_footer_names_the_actual_return_action() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        for code in [
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Enter,
        ] {
            press(&mut app, code, now);
        }
        app.settings_saved(app.settings());

        let footer = play_status_text(&app, " · ", 76);
        assert!(footer.contains("Enter branch"), "{footer}");
        assert!(!footer.contains("Enter open"), "{footer}");
    }

    #[test]
    fn contextual_play_help_defines_tools_at_supported_sizes() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Char('?'), now);
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        for (width, height) in [(80, 24), (60, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("help renders");

            let text = rendered_text(&terminal)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(text.contains("Move and act"), "{text}");
            assert!(text.contains("Goal, tools, and result"), "{text}");
            assert!(
                text.contains("Pattern to match The opened result, not the moves"),
                "{text}"
            );
            assert!(text.contains("Fold + crosses a crease"), "{text}");
            assert!(
                text.contains("Brush Dot or line inks each previewed stack"),
                "{text}"
            );
            assert!(text.contains("score is a guide"), "{text}");
            assert!(text.contains("Esc or Enter closes help"), "{text}");
        }
    }

    #[test]
    fn applied_fold_keeps_static_feedback_without_covering_the_paper() {
        let paper = crate::content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let started = Instant::now();
        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            started,
            false,
        );

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 2, 96, 26),
                    &session,
                    KeyBindings::default(),
                    profile,
                    started,
                    None,
                );
            })
            .expect("fold feedback renders");
        let first = rendered_text(&terminal);
        assert!(first.contains("Last step: Fold complete. The dot brush is ready."));
        assert!(!first.contains("paper  ›   crease"));

        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 2, 96, 26),
                    &session,
                    KeyBindings::default(),
                    profile,
                    started + std::time::Duration::from_secs(2),
                    None,
                );
            })
            .expect("static feedback renders later");
        let later = rendered_text(&terminal);
        assert!(later.contains("Last step: Fold complete. The dot brush is ready."));
        assert!(!later.contains("paper  ›   crease"));
    }

    #[test]
    fn placed_ink_stays_visible_in_the_paper_and_stack() {
        let paper = crate::content::lesson();
        let mut session = PlaySession::new(
            paper.puzzle(),
            paper.title(),
            paper.description(),
            Vec::new(),
            PlaySource::Lesson,
        );
        let started = Instant::now();
        session.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            KeyBindings::default(),
            started,
            true,
        );
        for code in [
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Enter,
        ] {
            session.handle_key(
                crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
                KeyBindings::default(),
                started,
                false,
            );
        }

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 56, 13),
                    &session,
                    KeyBindings::default(),
                    profile,
                    started,
                    None,
                );
            })
            .expect("ink state renders");
        let text = rendered_text(&terminal);
        assert!(text.contains('◉'));
        assert!(text.contains("Ink reached 2 layers. Enter opens the paper."));
        assert!(text.contains("0: cell 6 ink"));
        assert!(text.contains("1: cell 5 ink"));
    }

    #[test]
    fn how_to_frames_show_the_fold_stack_ink_and_comparison_states() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        for _ in 0..5 {
            press(&mut app, crossterm::event::KeyCode::Down, now);
        }
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let expected = [
            "fresh paper starts flat and dry",
            "every + cell crosses",
            "settles on top",
            "bottom to top",
            "inks every layer",
            "Open paper compares every cell: 0 missing (?), 0 extra (!)",
        ];
        for (index, message) in expected.into_iter().enumerate() {
            assert!(
                menu_text(&app, now, 100, 30).contains(message),
                "teaching frame {index} should show {message}"
            );
            if index + 1 < expected.len() {
                press(&mut app, crossterm::event::KeyCode::Right, now);
            }
        }
    }

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .fold(String::new(), |mut text, cell| {
                text.push_str(cell.symbol());
                text
            })
    }

    fn rendered_area_text(terminal: &Terminal<TestBackend>, area: Rect) -> String {
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    fn player_text(app: &App, now: Instant) -> String {
        player_text_with_glyphs(app, now, GlyphMode::Unicode)
    }

    fn player_text_with_glyphs(app: &App, now: Instant, glyph_mode: GlyphMode) -> String {
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, glyph_mode);
        terminal
            .draw(|frame| render(frame, app, profile, now))
            .expect("player view renders");
        rendered_text(&terminal)
    }

    fn menu_text(app: &App, now: Instant, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| render(frame, app, profile, now))
            .expect("menu renders");
        rendered_text(&terminal)
    }

    fn press(app: &mut App, code: crossterm::event::KeyCode, now: Instant) {
        app.handle_key(
            crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
            now,
        );
    }
}
