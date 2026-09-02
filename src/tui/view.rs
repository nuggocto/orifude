use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{Coordinate, FoldDirection, InkPattern, PaperAction};
use crate::storage::{ColorMode, GlyphMode, KeyBindings};

use super::app::{App, Screen, action_label, key_label};
use super::components::{BranchChoices, DialogLayer, Paper, StatusBar, TerminalMark};
use super::layout::{LayoutMode, MINIMUM_HEIGHT, MINIMUM_WIDTH, ShellLayout};
use super::session::{Draft, PlaySession, PlaySource, brush_label, fold_label};
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
        Paragraph::new(Line::from(vec![
            Span::styled("ORIFUDE", profile.title()),
            Span::styled("  folding and ink, kept offline", StyleProfile::muted()),
        ]))
        .alignment(Alignment::Center),
        shell.title,
    );
    let content = content_area(shell);

    match app.screen() {
        Screen::Capabilities => render_capabilities(frame, content, profile),
        Screen::Branch => {
            TerminalMark::render(frame, shell.mark, app.mark_frame(now), profile);
            let completed = (0..app.journey().len())
                .filter(|index| app.journey_complete(*index))
                .count();
            let saved = if app.recent().is_empty() { "no" } else { "yes" };
            let title = format!(
                "Home | Journey {completed}/{} | Saved {saved}",
                app.journey().len()
            );
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
        let overlay_area = if mode == LayoutMode::Preferred {
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
            Line::from("Match the opened paper to the target."),
            Line::from(""),
            Line::from("1  Tab previews the fold. + shows the side that will move."),
            Line::from("2  Enter folds. Arrow keys move the @ cursor."),
            Line::from("3  Tab selects the brush. Enter places ink through the stack."),
            Line::from("4  With no action selected, Enter opens and checks the paper."),
            Line::from(""),
            Line::from("The message below the controls gives the next move. ? opens help."),
            Line::from(""),
            Line::styled("Enter starts the lesson. Esc leaves.", profile.paper()),
        ]
    } else {
        vec![
            Line::styled("The paper is ready.", profile.title()),
            Line::from("Match the opened paper to the target."),
            Line::from(""),
            Line::from("1  Tab previews the fold. + shows the moving side."),
            Line::from("2  Enter folds. Arrows move the @ cursor."),
            Line::from("3  Tab selects the brush. Enter places ink."),
            Line::from("4  Enter again opens and checks the paper."),
            Line::from(""),
            Line::from("The message below the controls gives the next move."),
            Line::from("? opens help. u undoes. r resets."),
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
            format!("{}  [{state}]", paper.title())
        })
        .collect::<Vec<_>>();
    choices.push("Back to the branch".to_owned());
    render_owned_focus(frame, area, "Journey", &choices, app.selection(), profile);
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
            "The target is the opened sheet. The fresh paper has no ink yet.".to_owned(),
        ),
        1 => (
            None,
            fold,
            None,
            "The crease names the fold. Every + cell is on the side that moves.".to_owned(),
        ),
        2 => (
            None,
            None,
            None,
            "The production fold engine places the moving side on the folded paper.".to_owned(),
        ),
        3 => (
            brush_cursor,
            None,
            None,
            "The stack lists layers from bottom to top at the selected cell.".to_owned(),
        ),
        4 => (
            brush_cursor,
            None,
            None,
            "One brush mark passes through every layer in the selected stack.".to_owned(),
        ),
        _ => {
            let result = attempt.result();
            (
                None,
                None,
                Some(paper.puzzle().dimensions().cell_count()),
                format!(
                    "Unfold and compare: {} missing, {} extra.",
                    result.comparison().missing().len(),
                    result.comparison().extra().len()
                ),
            )
        }
    };
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(4)])
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
) {
    let status_height = if area.width >= 80 {
        7
    } else if session.result().is_some() {
        6
    } else {
        5
    };
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
}

#[allow(clippy::too_many_arguments)]
fn render_session_boards(
    frame: &mut Frame<'_>,
    area: Rect,
    puzzle: &crate::domain::puzzle::Puzzle,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
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
    let focus_row = cursor.map_or(0, |coordinate| coordinate.row().get());
    if target_visible {
        render_grid_window(
            frame,
            regions[0],
            "TARGET [reference]",
            target_grid(puzzle),
            focus_row,
            true,
            profile,
        );
    } else {
        let (title, grid) = if let Some(revealed) = comparison_reveal {
            (
                "OPENED COMPARISON",
                comparison_grid(attempt, puzzle, ink, revealed),
            )
        } else if unfolded {
            ("UNFOLDED PREVIEW", unfolded_grid(attempt, ink))
        } else {
            ("FOLDED PAPER", folded_grid(attempt, cursor, preview, ink))
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
        "Target [reference]"
    } else {
        "Target [ref]"
    };
    render_grid(
        frame,
        regions[0],
        target_title,
        target_grid(puzzle),
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
        comparison_grid(attempt, puzzle, ink, revealed)
    } else if unfolded {
        unfolded_grid(attempt, ink)
    } else {
        folded_grid(attempt, cursor, preview, ink)
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
    const SQUIRREL_WIDTH: u16 = 8;
    const GAP_WIDTH: u16 = 1;
    const MINIMUM_BUBBLE_WIDTH: u16 = 21;
    const MINIMUM_COACH_HEIGHT: u16 = 5;
    const COACH_HEIGHT: u16 = 7;
    const SQUIRREL: [&str; 4] = ["  /)_/)", " ( o.o)", " /|_ _|", "(_/ \\__)"];

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
        Paragraph::new(SQUIRREL.into_iter().map(Line::from).collect::<Vec<_>>())
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
            Some(Draft::Fold(_)) => "The + side will move.\nPress Enter to fold.".to_owned(),
            Some(Draft::Brush(_)) => {
                "The fold comes first.\nPress Shift+Tab to go back.".to_owned()
            }
            None => "Press Tab first.\nIt previews the fold.".to_owned(),
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
                    "One dot marks both layers.\nPress Enter to place it.".to_owned()
                }
                Some(Draft::Fold(_)) => {
                    "That fold is done.\nPress Esc, then Tab for the dot.".to_owned()
                }
                None => "You found the stack.\nPress Tab for the dot.".to_owned(),
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
    rows: Vec<String>,
    active: bool,
    profile: StyleProfile,
) {
    let lines = rows.into_iter().map(Line::from).collect::<Vec<_>>();
    let block = if active {
        Paper::highlighted_block(title, profile)
    } else {
        Paper::block(title, profile)
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

fn render_grid_window(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: Vec<String>,
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

fn target_grid(puzzle: &crate::domain::puzzle::Puzzle) -> Vec<String> {
    let dimensions = puzzle.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            (0..dimensions.width().get())
                .map(|column| {
                    let coordinate = dimensions.coordinate(row, column).expect("grid coordinate");
                    let id = dimensions.cell_id(coordinate).expect("grid identity");
                    if puzzle.target().contains(id) {
                        '#'
                    } else {
                        '.'
                    }
                })
                .flat_map(|symbol| [symbol, ' '])
                .collect()
        })
        .collect()
}

fn unfolded_grid(attempt: &Attempt, ink: InkPattern) -> Vec<String> {
    let dimensions = attempt.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            (0..dimensions.width().get())
                .map(|column| {
                    let coordinate = dimensions.coordinate(row, column).expect("grid coordinate");
                    let id = dimensions.cell_id(coordinate).expect("grid identity");
                    if ink.contains(id) { '*' } else { '.' }
                })
                .flat_map(|symbol| [symbol, ' '])
                .collect()
        })
        .collect()
}

fn comparison_grid(
    attempt: &Attempt,
    puzzle: &crate::domain::puzzle::Puzzle,
    ink: InkPattern,
    revealed: usize,
) -> Vec<String> {
    let dimensions = attempt.dimensions();
    (0..dimensions.height().get())
        .map(|row| {
            (0..dimensions.width().get())
                .map(|column| {
                    let coordinate = dimensions.coordinate(row, column).expect("grid coordinate");
                    let id = dimensions.cell_id(coordinate).expect("grid identity");
                    if id.index() >= revealed {
                        return ' ';
                    }
                    match (puzzle.target().contains(id), ink.contains(id)) {
                        (true, true) => '#',
                        (true, false) => '?',
                        (false, true) => '!',
                        (false, false) => '.',
                    }
                })
                .flat_map(|symbol| [symbol, ' '])
                .collect()
        })
        .collect()
}

fn folded_grid(
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    preview: Option<PaperAction>,
    ink_pattern: InkPattern,
) -> Vec<String> {
    let dimensions = attempt.dimensions();
    let footprint = preview_footprint(preview, dimensions);
    (0..dimensions.height().get())
        .map(|row| {
            (0..dimensions.width().get())
                .map(|column| {
                    let coordinate = dimensions.coordinate(row, column).expect("grid coordinate");
                    if cursor == Some(coordinate) {
                        return '@';
                    }
                    if footprint.contains(&coordinate) {
                        return '+';
                    }
                    let mut count = 0_u8;
                    let mut ink = false;
                    for id in attempt.cell_ids() {
                        let physical = attempt
                            .physical_cell(id)
                            .expect("attempt exposes every physical cell");
                        if physical.coordinate() == coordinate {
                            count = count.saturating_add(1);
                            ink |= ink_pattern.contains(id);
                        }
                    }
                    if ink {
                        '*'
                    } else if count == 0 {
                        ' '
                    } else if count == 1 {
                        'o'
                    } else {
                        char::from_digit(u32::from(count.min(9)), 10).unwrap_or('9')
                    }
                })
                .flat_map(|symbol| [symbol, ' '])
                .collect()
        })
        .collect()
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
    let title = if area.width < 20 {
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
            "  {}F/{}  {}S/{}",
            session.attempt().fold_count().get(),
            session.puzzle().fold_budget().get(),
            session.attempt().stroke_count().get(),
            session.puzzle().stroke_budget().get()
        )),
    ])];
    if session.result().is_some() {
        lines.extend(result_status_lines(session, bindings, profile, reveal));
    } else {
        lines.extend(active_status_lines(session, bindings, profile, area.width));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Paper::block("Paper controls", profile))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn result_status_lines(
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    reveal: Option<(usize, usize, bool)>,
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
                "Enter returns to the unchanged attempt; {} starts over.",
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
    } else {
        let par = match result.meets_par() {
            Some(true) => " Par met.",
            Some(false) => " Solved above par.",
            None => "",
        };
        lines.push(Line::styled(
            format!(
                "Saved safely.{par}  Enter returns | {} retry | v replay | x text keepsake",
                bindings.reset
            ),
            profile.paper(),
        ));
    }
    lines
}

fn active_status_lines(
    session: &PlaySession,
    bindings: KeyBindings,
    profile: StyleProfile,
    width: u16,
) -> Vec<Line<'static>> {
    let draft = match session.draft() {
        Some(Draft::Fold(index)) => session
            .puzzle()
            .allowed_folds()
            .get(index)
            .copied()
            .map_or_else(|| "fold unavailable".to_owned(), fold_label),
        Some(Draft::Brush(index)) => session
            .puzzle()
            .allowed_brushes()
            .get(index)
            .copied()
            .map_or_else(|| "brush unavailable".to_owned(), brush_label),
        None => "none selected".to_owned(),
    };
    let history = action_history(session.attempt());
    let cue_limit = if width >= 80 {
        120
    } else {
        usize::from(width.saturating_sub(4))
    };
    let cue = session
        .cues()
        .get(usize::from(session.attempt().action_count().get()))
        .map_or_else(
            || SafeText::external_display(&session.description(), cue_limit, profile.glyph_mode()),
            |cue| SafeText::external_display(cue, cue_limit, profile.glyph_mode()),
        );
    if width >= 80 {
        vec![
            Line::from(format!("Paper action: {draft}")),
            Line::from(history),
            Line::styled(
                session_keys(profile.glyph_mode(), bindings),
                StyleProfile::muted(),
            ),
            Line::styled(cue.as_str().to_owned(), profile.paper()),
        ]
    } else {
        vec![
            Line::from(if session.draft().is_some() {
                format!("Action: {draft} | {history}")
            } else {
                history
            }),
            Line::styled(cue.as_str().to_owned(), profile.paper()),
        ]
    }
}

fn action_history(attempt: &Attempt) -> String {
    let actions = attempt.actions().collect::<Vec<_>>();
    if actions.is_empty() {
        return "Actions: none".to_owned();
    }
    let start = actions.len().saturating_sub(3);
    let labels = actions[start..]
        .iter()
        .copied()
        .map(action_label)
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = if start > 0 { "... " } else { "" };
    format!("Actions: {prefix}{labels}")
}

fn session_keys(glyphs: GlyphMode, bindings: KeyBindings) -> String {
    let separator = if glyphs == GlyphMode::Unicode {
        " · "
    } else {
        " | "
    };
    [
        format!("{} fold", bindings.fold),
        format!("{} brush", bindings.brush),
        "Tab next action".to_owned(),
        "Enter confirm/open".to_owned(),
        format!("{} undo", bindings.undo),
        format!("{} reset", bindings.reset),
        format!("{} preview", key_label(bindings.preview)),
    ]
    .join(separator)
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
        Screen::Play if width < 80 => format!(
            "@ move{separator}Tab action{separator}Enter{separator}t {}{separator}{} help{separator}{} quit",
            if app.session().is_some_and(PlaySession::target_visible) {
                "paper"
            } else {
                "target"
            },
            bindings.help,
            bindings.quit
        ),
        Screen::Play => format!(
            "Arrows move @{separator}Tab action{separator}Enter confirm/open{separator}{} help{separator}{} quit",
            bindings.help, bindings.quit
        ),
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
    use crate::storage::{GlyphMode, Settings};
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
                assert!(text.contains("Match the opened paper to the target."));
                assert!(text.contains("Tab previews the fold"));
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
        let cue_prefix = app.session().expect("lesson session").cues()[0]
            .chars()
            .take(24)
            .collect::<String>();
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
        assert!(text.contains("Target [reference]"));
        assert!(text.contains("FOLDED PAPER"));
        assert!(!text.contains("[ACTIVE]"));
        assert!(text.contains("Stack, bottom to top"));
        assert!(text.contains('+'));
        assert!(text.contains("Right at crease 2"));
        assert!(text.contains(&cue_prefix));
        assert!(text.contains("Squirrel says"));
        assert!(text.contains("Press Enter to fold."));
    }

    #[test]
    fn lesson_coach_tracks_the_next_player_action() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        let text = player_text(&app, now);
        assert!(text.contains("Squirrel says"));
        assert!(text.contains("Press Tab first."));
        assert!(player_text_with_glyphs(&app, now, GlyphMode::Ascii).is_ascii());

        press(&mut app, crossterm::event::KeyCode::Tab, now);
        assert!(player_text(&app, now).contains("Press Enter to fold."));

        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("Move @ to row 2, column 3"));

        press(&mut app, crossterm::event::KeyCode::Down, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        assert!(player_text(&app, now).contains("Press Tab for the dot."));

        press(&mut app, crossterm::event::KeyCode::Tab, now);
        assert!(player_text(&app, now).contains("One dot marks both layers."));

        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("Press Enter to open."));
    }

    #[test]
    fn lesson_coach_leads_the_player_back_from_misplaced_ink() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);

        press(&mut app, crossterm::event::KeyCode::Char('b'), now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        assert!(player_text(&app, now).contains("The ink came before the fold."));

        press(&mut app, crossterm::event::KeyCode::Char('u'), now);
        press(&mut app, crossterm::event::KeyCode::Tab, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Right, now);
        press(&mut app, crossterm::event::KeyCode::Tab, now);
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
        assert!(branch.contains("Home | Journey 0/3 | Saved no"));

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
    fn minimum_player_layout_keeps_history_and_the_active_cue_visible() {
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
        assert!(text.contains("Actions:"));
        assert!(text.contains(&cue_prefix));
        assert!(text.contains("PAPER"));
        assert!(!text.contains("[ACTIVE]"));
        assert!(text.contains("Low to high"));
        assert!(text.contains("q quit"));
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
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 56, 13),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                );
            })
            .expect("compact paper renders");
        let paper = rendered_text(&terminal);
        assert!(paper.contains("FOLDED PAPER rows 7-12/12"));
        assert!(paper.contains('@'));
        assert!(!paper.contains("TARGET [reference]"));

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
                );
            })
            .expect("compact target renders");
        let target = rendered_text(&terminal);
        assert!(target.contains("TARGET [reference] rows 7-12/12"));
        assert!(!target.contains("FOLDED PAPER"));
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
        let paper = crate::content::journey().remove(0);
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
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| {
                render_session(
                    frame,
                    Rect::new(2, 4, 56, 13),
                    &session,
                    KeyBindings::default(),
                    profile,
                    now,
                );
            })
            .expect("minimum result renders");
        let text = rendered_text(&terminal);
        assert!(text.contains("Enter returns"));
        assert!(text.contains("x text keepsake"));
    }

    #[test]
    fn contextual_play_help_keeps_its_final_binding_visible_at_preferred_minimum() {
        let now = Instant::now();
        let settings = Settings {
            lesson_complete: true,
            ..Settings::default()
        };
        let mut app = App::new(settings, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Enter, now);
        press(&mut app, crossterm::event::KeyCode::Char('?'), now);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("help renders");

        let text = rendered_text(&terminal)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("previews unfolded ink"), "{text}");
        assert!(text.contains("cancels or returns."), "{text}");
        assert!(text.contains("Esc or Enter closes help"), "{text}");
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
            "fresh paper has no ink",
            "Every + cell",
            "production fold engine",
            "bottom to top",
            "passes through every layer",
            "Unfold and compare: 0 missing, 0 extra",
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
