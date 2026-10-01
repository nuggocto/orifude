use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::domain::attempt::Attempt;
use crate::domain::paper::{Coordinate, Row};
use crate::storage::{BindingSlot, ColorMode, GlyphMode, KeyBindings};

use super::app::{App, Screen, action_label, key_label};
use super::components::{
    BranchChoices, BranchGrowth, CompletionCourier, DialogLayer, Paper, StatusBar, TerminalMark,
    courier_art, gift_mark,
};
use super::layout::{LayoutMode, MINIMUM_HEIGHT, MINIMUM_WIDTH, ShellLayout, centered};
use super::session::{Draft, PlaySession, PlaySource, action_coordinate};
use super::style::StyleProfile;
use super::text::SafeText;

mod boards;

use boards::{BoardMode, BoardView};

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
            let separator = match profile.glyph_mode() {
                GlyphMode::Unicode => "·",
                GlyphMode::Ascii => "|",
            };
            let title = format!(
                "Home {separator} Journey {completed}/{}",
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
                    app.group_completion(),
                    app.next_journey_index().is_some(),
                    app.overlay().is_none(),
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
        // During play a dialog centers over the boards; elsewhere it covers
        // the artwork and leaves the menu in view.
        let overlay_area = if app.screen() == Screen::Play {
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
    // Fit the card to its words so the welcome does not float in empty space.
    // Small terminals keep every line and give up the extra margin instead.
    let line_count = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let roomy = area.width >= 80 && area.height >= line_count.saturating_add(4);
    let (block, card) = if roomy {
        (
            Paper::block("How this paper works", profile).padding(Padding::new(2, 2, 1, 1)),
            centered(area, 78, line_count.saturating_add(4)),
        )
    } else {
        (Paper::block("How this paper works", profile), area)
    };
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        card,
    );
}

fn render_journey(frame: &mut Frame<'_>, area: Rect, app: &App, profile: StyleProfile) {
    let (done, open, locked) = match profile.glyph_mode() {
        GlyphMode::Unicode => ('●', '○', '·'),
        GlyphMode::Ascii => ('*', 'o', '.'),
    };
    // Chapter headings sit between the papers but are never selectable, so
    // remember where each selectable row landed.
    let mut items = Vec::new();
    let mut rows = Vec::new();
    for (group_index, group) in crate::content::journey_groups().iter().enumerate() {
        if group_index > 0 {
            items.push(ListItem::new(""));
        }
        let papers = group.first_paper..group.first_paper + group.paper_count;
        let mut heading = vec![Span::styled(
            format!("{}  {}", group_index + 1, group.title),
            profile.title(),
        )];
        // The chapter's gift stays a dormant bud until every paper is done.
        let gift = if papers.clone().all(|index| app.journey_complete(index)) {
            Span::styled(
                format!("  {}", gift_mark(group_index, profile.glyph_mode())),
                profile.ink_mark(),
            )
        } else {
            Span::styled(format!("  {locked}"), StyleProfile::muted())
        };
        heading.push(gift);
        heading.push(Span::styled(
            format!(" {}", group.gift.label()),
            StyleProfile::muted(),
        ));
        items.push(ListItem::new(Line::from(heading)));
        for index in papers {
            let Some(paper) = app.journey().get(index) else {
                continue;
            };
            let (mark, style) = if app.journey_complete(index) {
                (done, profile.ink_mark())
            } else if app.journey_unlocked(index) {
                (open, profile.ink())
            } else {
                (locked, StyleProfile::muted())
            };
            let label = format!(
                "{}.{}  {}",
                group_index + 1,
                index - group.first_paper + 1,
                paper.title()
            );
            rows.push(items.len());
            items.push(ListItem::new(choice_line(
                index == app.selection(),
                vec![
                    Span::styled(mark.to_string(), style),
                    Span::styled(format!(" {label}"), style),
                ],
                profile,
            )));
        }
    }
    items.push(ListItem::new(""));
    rows.push(items.len());
    items.push(ListItem::new(choice_line(
        app.selection() == app.journey().len(),
        vec![Span::styled("Back to the branch", profile.ink())],
        profile,
    )));

    let title = crate::content::journey_group(app.selection()).map_or_else(
        || "Journey".to_owned(),
        |(group_index, group)| format!("Journey {}: {}", group_index + 1, group.title),
    );
    let mechanic = crate::content::journey_group(app.selection())
        .map_or("Return to the branch.", |(_, group)| group.mechanic);
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(mechanic)
            .style(StyleProfile::muted())
            .wrap(Wrap { trim: true }),
        regions[0],
    );
    render_rows(
        frame,
        regions[1],
        &title,
        items,
        &rows,
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
    // Border, padding, and the selection marker leave this many columns.
    let width = usize::from(area.width.saturating_sub(6));
    let mut items = Vec::new();
    let mut rows = Vec::new();
    let mut choice = |items: &mut Vec<ListItem<'static>>, label: String, detail: String| {
        let index = rows.len();
        rows.push(items.len());
        // Measured in terminal columns, so a wide pack title cannot push the
        // score off the row.
        let detail_columns = detail.width();
        let label = SafeText::external_display(&label, 160, profile.glyph_mode()).fit_columns(
            width.saturating_sub(detail_columns).saturating_sub(2),
            profile.glyph_mode(),
        );
        let gap = width
            .saturating_sub(label.as_str().width())
            .saturating_sub(detail_columns)
            .max(2);
        items.push(ListItem::new(choice_line(
            index == app.selection(),
            vec![
                Span::styled(label.as_str().to_owned(), profile.ink()),
                Span::raw(" ".repeat(gap)),
                Span::styled(detail, StyleProfile::muted()),
            ],
            profile,
        )));
    };

    for progress in app.recent() {
        let (label, missing) = keepsake_label(app, progress);
        let detail = if missing {
            "pack removed; replay kept".to_owned()
        } else {
            format!(
                "{}, {}",
                counted(progress.best_folds, "fold"),
                counted(progress.best_strokes, "stroke")
            )
        };
        choice(&mut items, label, detail);
    }
    let empty = app.recent().is_empty() && app.keepsake_offset() == 0 && !app.keepsake_has_more();
    if empty {
        items.push(ListItem::new(Line::styled(
            "No keepsakes yet. Each finished paper waits here to be replayed.",
            StyleProfile::muted(),
        )));
        items.push(ListItem::new(""));
    } else {
        if app.keepsake_has_more() {
            choice(&mut items, "Older keepsakes".to_owned(), String::new());
        }
        if app.keepsake_offset() > 0 {
            choice(&mut items, "Newer keepsakes".to_owned(), String::new());
        }
        items.push(ListItem::new(""));
    }
    choice(&mut items, "Back to the branch".to_owned(), String::new());

    let first = app.keepsake_offset().saturating_add(1);
    let last = app
        .keepsake_offset()
        .saturating_add(app.recent().len() as u64);
    let title = if app.recent().is_empty() {
        "Saved keepsakes".to_owned()
    } else {
        format!("Saved keepsakes {first}-{last}")
    };
    render_rows(frame, area, &title, items, &rows, app.selection(), profile);
}

/// Names a saved paper the way the player met it, and reports whether its
/// community pack has since been removed.
fn keepsake_label(app: &App, progress: &crate::storage::PuzzleProgress) -> (String, bool) {
    match progress.pack_id.as_ref() {
        "orifude-journey" => {
            let label = app
                .journey()
                .iter()
                .position(|paper| paper.puzzle().identity().puzzle_id() == &*progress.puzzle_id)
                .and_then(|index| {
                    let (group_index, group) = crate::content::journey_group(index)?;
                    Some(format!(
                        "Journey {}.{}  {}",
                        group_index + 1,
                        index - group.first_paper + 1,
                        app.journey()[index].title()
                    ))
                });
            (
                label.unwrap_or_else(|| format!("Journey  {}", progress.puzzle_id)),
                false,
            )
        }
        "orifude-daily" => ("Daily paper".to_owned(), false),
        "orifude-endless" => ("Endless garden paper".to_owned(), false),
        pack_id => app
            .packs()
            .iter()
            .find(|pack| pack.id.as_ref() == pack_id)
            .map_or_else(
                || (format!("{pack_id}  {}", progress.puzzle_id), true),
                |pack| (format!("{}  {}", pack.title, progress.puzzle_id), false),
            ),
    }
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
    let preferences = [
        ("Color", color.to_owned()),
        ("Symbols", glyphs.to_owned()),
        ("Reduced motion", on_off(settings.reduced_motion).to_owned()),
        ("Instant reveal", on_off(settings.instant_reveal).to_owned()),
    ];
    let keys = BindingSlot::ALL.map(|slot| (slot.label(), key_label(bindings.key(slot))));
    let mut items = Vec::new();
    let mut rows = Vec::new();
    let mut choice = |items: &mut Vec<ListItem<'static>>, label: &str, value: String| {
        let selected = rows.len() == app.selection();
        rows.push(items.len());
        items.push(ListItem::new(choice_line(
            selected,
            vec![
                Span::styled(format!("{label:<16}"), profile.ink()),
                Span::styled(value, profile.paper()),
            ],
            profile,
        )));
    };
    items.push(ListItem::new(Line::styled(
        "Display (Left/Right changes)",
        profile.title(),
    )));
    for (label, value) in preferences {
        choice(&mut items, label, value);
    }
    items.push(ListItem::new(""));
    items.push(ListItem::new(Line::styled(
        "Keys (Enter rebinds)",
        profile.title(),
    )));
    for (label, value) in keys {
        choice(&mut items, label, value);
    }
    items.push(ListItem::new(""));
    choice(&mut items, "Back to the branch", String::new());

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
    render_rows(
        frame,
        regions[0],
        "Settings and keys",
        items,
        &rows,
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
            Line::from(vec![
                Span::styled(
                    format!("Teaching frame {} of {total}", step + 1),
                    profile.title(),
                ),
                Span::raw("   "),
                step_track(step, total, profile),
            ]),
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

/// Filled marks for frames already shown, hollow marks for those ahead.
fn step_track(step: usize, total: usize, profile: StyleProfile) -> Span<'static> {
    let (seen, ahead) = match profile.glyph_mode() {
        GlyphMode::Unicode => ('●', '○'),
        GlyphMode::Ascii => ('*', 'o'),
    };
    let track = (0..total)
        .map(|index| if index <= step { seen } else { ahead }.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    Span::styled(track, profile.paper())
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
    // A dialog drawn on top replaces the completion card instead of stacking.
    show_cards: bool,
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
    if show_cards && reveal.as_ref().is_some_and(|reveal| reveal.complete) && session.saved() {
        let paper_height = session.puzzle().dimensions().height().get();
        if let Some(group) = group_completion {
            let host = below_grids(regions[0], paper_height, 12);
            CompletionCourier::render(frame, host, group, profile, next_journey);
        } else if !matches!(session.source(), PlaySource::Keepsake) {
            let host = below_grids(regions[0], paper_height, 7);
            render_success_card(frame, host, session, bindings, profile, next_journey);
        }
    }
}

/// Returns the space under the board grids when a card of `card_height` fits
/// there, so the opened paper stays visible; otherwise the whole board area.
fn below_grids(area: Rect, paper_height: u8, card_height: u16) -> Rect {
    // Each grid sits under its top border and column ruler.
    let top = area
        .y
        .saturating_add(3)
        .saturating_add(u16::from(paper_height));
    let room = area.bottom().saturating_sub(1).saturating_sub(top);
    if room >= card_height {
        Rect::new(area.x, top, area.width, room)
    } else {
        area
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
    // The lesson only marks itself complete; it leaves no keepsake behind.
    let encouragement = if matches!(session.source(), PlaySource::Lesson) {
        "The journey's first paper is waiting."
    } else if result.meets_par() == Some(false) {
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
    let mut lines = vec![
        Line::styled(headline, profile.title()).alignment(Alignment::Center),
        Line::from(detail).alignment(Alignment::Center),
        Line::styled(encouragement, profile.paper()).alignment(Alignment::Center),
    ];
    // Set the controls apart only when that cannot push one out of the card.
    if usize::from(height) >= lines.len() + controls.len() + 3 {
        lines.push(Line::from(""));
    }
    lines.extend(
        controls
            .into_iter()
            .map(|line| Line::styled(line, StyleProfile::muted()).alignment(Alignment::Center)),
    );
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

    // Stay inside the border and padding, which keeps the bubble off the edge.
    let inner_width = target_area.width.saturating_sub(4);
    // The border and the column ruler sit above the grid rows.
    let grid_bottom = target_area
        .y
        .saturating_add(2)
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
        target_area.x.saturating_add(2),
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
    let counters = [
        Span::styled("   Folds ", StyleProfile::muted()),
        Span::styled(
            format!(
                "{}/{}",
                session.attempt().fold_count().get(),
                session.puzzle().fold_budget().get()
            ),
            profile.ink(),
        ),
        Span::styled("   Ink ", StyleProfile::muted()),
        Span::styled(
            format!(
                "{}/{}",
                session.attempt().stroke_count().get(),
                session.puzzle().stroke_budget().get()
            ),
            profile.ink(),
        ),
    ];
    // A long or wide title shares one row with the counters, so the ready tool
    // and guidance below keep their rows. Borders and padding take four columns.
    let title_columns = usize::from(area.width.saturating_sub(4))
        .saturating_sub(counters.iter().map(Span::width).sum());
    let title = SafeText::external_display(&session.title(), 80, profile.glyph_mode())
        .fit_columns(title_columns, profile.glyph_mode());
    let mut lines = vec![Line::from(
        std::iter::once(Span::styled(title.as_str().to_owned(), profile.title()))
            .chain(counters)
            .collect::<Vec<_>>(),
    )];
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

/// Renders a list whose headings and blank lines cannot be selected.
/// `choice_rows` holds the list row of each selectable choice, in order.
fn render_rows(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: Vec<ListItem<'static>>,
    choice_rows: &[usize],
    selected: usize,
    profile: StyleProfile,
) {
    let mut state = ListState::default();
    state.select(choice_rows.get(selected).copied());
    frame.render_stateful_widget(
        List::new(items).block(Paper::block(title, profile)),
        area,
        &mut state,
    );
}

/// A selectable row: the marked, highlighted form when selected, otherwise
/// the given spans after the marker's width of space.
fn choice_line(selected: bool, spans: Vec<Span<'static>>, profile: StyleProfile) -> Line<'static> {
    let marker = match profile.glyph_mode() {
        GlyphMode::Unicode => "›",
        GlyphMode::Ascii => ">",
    };
    if selected {
        let text = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        Line::styled(format!("{marker} {text}"), profile.active())
    } else {
        Line::from(
            std::iter::once(Span::raw("  "))
                .chain(spans)
                .collect::<Vec<_>>(),
        )
    }
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
        Line::from(""),
        Line::from("Press Ctrl+C to quit."),
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
