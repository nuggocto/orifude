use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{Coordinate, Row};
use crate::storage::{ColorMode, GlyphMode, KeyBindings};

use super::app::{App, Overlay, Screen, action_label, key_label};
use super::components::{
    BRANCH_CARD_WIDTH, BranchChoices, BranchGrowth, CompletionCourier, DialogLayer, Paper,
    StatusBar, TerminalMark, courier_art,
};
use super::layout::{LayoutMode, MINIMUM_HEIGHT, MINIMUM_WIDTH, ShellLayout, centered};
use super::session::{Draft, PlaySession, PlaySource};
use super::style::StyleProfile;
use super::text::SafeText;

mod boards;

use boards::{BoardMode, BoardView, action_coordinate};

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
                    app.next_journey_index().is_some(),
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
            Line::from("Your paper is being prepared on this computer."),
            Line::from("Esc cancels."),
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
    BoardView {
        puzzle: paper.puzzle(),
        attempt: &attempt,
        cursor,
        focus_row: cursor.map_or(Row::new(0).expect("first paper row"), Coordinate::row),
        preview,
        mode: comparison_reveal.map_or(BoardMode::Folded, BoardMode::Comparison),
        ink: attempt.ink(),
        target_visible: false,
    }
    .render_wide(frame, regions[0], profile);
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

#[allow(clippy::too_many_arguments)]
fn render_session(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    now: Instant,
    group_completion: Option<&crate::content::JourneyGroup>,
    next_journey: bool,
) {
    let status_height = if area.width >= 80 { 7 } else { 6 };
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(status_height)])
        .split(area);
    let reveal = session.result().map(|_| session.reveal_frame(now));
    let boards = if let Some(reveal) = reveal.as_ref() {
        BoardView {
            puzzle: session.puzzle(),
            attempt: &reveal.geometry,
            cursor: Some(session.cursor()),
            focus_row: session.comparison_row(),
            preview: None,
            mode: if reveal.complete {
                BoardMode::Comparison(session.puzzle().dimensions().cell_count())
            } else {
                BoardMode::Folded
            },
            ink: session.attempt().ink(),
            target_visible: session.target_visible(),
        }
    } else {
        BoardView {
            puzzle: session.puzzle(),
            attempt: session.attempt(),
            cursor: Some(session.cursor()),
            focus_row: session.cursor().row(),
            preview: session.preview_action(),
            mode: if session.unfolded_preview() {
                BoardMode::Unfolded
            } else {
                BoardMode::Folded
            },
            ink: session.attempt().ink(),
            target_visible: session.target_visible(),
        }
    };
    let target_area = boards.render(frame, regions[0], profile);
    if reveal.is_none() && matches!(session.source(), PlaySource::Lesson) {
        render_lesson_coach(frame, target_area, session, bindings, profile);
    }
    let reveal_state = reveal
        .as_ref()
        .map(|reveal| (reveal.opened_folds, reveal.total_folds, reveal.complete));
    render_session_status(frame, regions[1], session, bindings, profile, reveal_state);
    if reveal.as_ref().is_some_and(|reveal| reveal.complete) && session.saved() {
        if let Some(group) = group_completion {
            CompletionCourier::render(frame, regions[0], group, profile, next_journey);
        } else if !matches!(session.source(), PlaySource::Keepsake) {
            render_success_card(frame, regions[0], session, bindings, profile, next_journey);
        }
    }
}

fn render_success_card(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    next_journey: bool,
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
    let next = if next_journey {
        format!("Tab next{separator}")
    } else {
        String::new()
    };
    let controls = if matches!(session.source(), PlaySource::Lesson) {
        vec![format!(
            "Enter returns to the branch{separator}{} retries",
            bindings.reset
        )]
    } else if next_journey && area.width < 74 {
        vec![
            format!("Tab next{separator}Enter back"),
            format!(
                "{} retry{separator}v replay{separator}x keepsake",
                bindings.reset
            ),
        ]
    } else {
        vec![format!(
            "{next}Enter back{separator}{} retry{separator}v replay{separator}x keepsake",
            bindings.reset,
        )]
    };
    let preferred_height = if area.width >= 74 { 7 } else { 9 };
    let height = area.height.min(preferred_height);
    let card = centered(area, 74, height);
    frame.render_widget(Clear, card);
    let lines = [
        Line::styled(headline, profile.title()).alignment(Alignment::Center),
        Line::from(detail).alignment(Alignment::Center),
        Line::styled(encouragement, profile.paper()).alignment(Alignment::Center),
    ]
    .into_iter()
    .chain(
        controls
            .into_iter()
            .map(|line| Line::styled(line, StyleProfile::muted()).alignment(Alignment::Center)),
    )
    .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(Paper::block("Paper complete", profile))
            .wrap(Wrap { trim: true }),
        card,
    );
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
        return vec![Line::from(format!(
            "Opening crease {opened}/{total}; the final comparison follows."
        ))];
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
        let mut lines = vec![Line::from(ready)];
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
        let next = if app.next_journey_index().is_some() {
            format!("Tab next{separator}")
        } else {
            String::new()
        };
        return if width >= 90 || (next.is_empty() && width >= 70) {
            format!(
                "{next}Enter back{separator}{} retry{separator}v replay{separator}x keepsake{separator}{} help{separator}{} quit",
                bindings.reset, bindings.help, bindings.quit
            )
        } else {
            format!(
                "{next}Enter back{separator}{} retry{separator}v{separator}x{separator}{}{separator}{} quit",
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
mod tests;
