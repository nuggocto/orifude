//! Board layout and cell rendering share one description of the visible paper.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    Coordinate, FoldDirection, InkPattern, MAX_PHYSICAL_CELLS, PaperAction, Row,
};
use crate::domain::puzzle::Puzzle;
use crate::storage::GlyphMode;
use crate::tui::components::Paper;
use crate::tui::session::fold_label;
use crate::tui::style::StyleProfile;

pub(super) enum BoardMode {
    Folded,
    Unfolded,
    Comparison(usize),
}

pub(super) struct BoardView<'a> {
    pub(super) puzzle: &'a Puzzle,
    pub(super) attempt: &'a Attempt,
    pub(super) cursor: Option<Coordinate>,
    pub(super) focus_row: Row,
    // The stack inspector also shows the selected fold during an unfolded preview.
    pub(super) preview: Option<PaperAction>,
    pub(super) mode: BoardMode,
    pub(super) ink: InkPattern,
    pub(super) target_visible: bool,
}

impl BoardView<'_> {
    pub(super) fn render(&self, frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) -> Rect {
        if self.compact_required(area) {
            self.render_compact(frame, area, profile);
            Rect::default()
        } else {
            self.render_wide(frame, area, profile)
        }
    }

    fn compact_required(&self, area: Rect) -> bool {
        if area.width < 80 {
            return true;
        }
        let regions = wide_regions(area);
        let dimensions = self.puzzle.dimensions();
        let grid_width = u16::from(dimensions.width().get()) * 2 + 2;
        let grid_height = u16::from(dimensions.height().get()) + 2;
        regions[0].width < grid_width || regions[1].width < grid_width || area.height < grid_height
    }

    fn render_compact(&self, frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
        let regions = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(area);
        let (title, grid) = if self.target_visible {
            ("PATTERN TO MATCH", target_grid(self.puzzle, profile))
        } else {
            (self.title(true, false), self.grid(profile))
        };
        render_grid_window(
            frame,
            regions[0],
            title,
            grid,
            self.focus_row.get(),
            true,
            profile,
        );
        self.render_stack(frame, regions[1], profile);
    }

    pub(super) fn render_wide(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        profile: StyleProfile,
    ) -> Rect {
        let regions = wide_regions(area);
        let target_title = if regions[0].width >= 24 {
            "Pattern to match"
        } else {
            "Goal"
        };
        render_grid(
            frame,
            regions[0],
            target_title,
            target_grid(self.puzzle, profile),
            false,
            profile,
        );
        let active = self.cursor.is_some();
        let title = self.title(active, regions[1].width < 28);
        render_grid(
            frame,
            regions[1],
            title,
            self.grid(profile),
            active,
            profile,
        );
        self.render_stack(frame, regions[2], profile);
        regions[0]
    }

    fn title(&self, active: bool, short: bool) -> &'static str {
        match (&self.mode, active, short) {
            (BoardMode::Folded, true, true) => "PAPER",
            (BoardMode::Folded, true, false) => "FOLDED PAPER",
            (BoardMode::Folded, false, _) => "Folded paper",
            (BoardMode::Unfolded, true, true) => "PREVIEW",
            (BoardMode::Unfolded, true, false) => "UNFOLDED PREVIEW",
            (BoardMode::Unfolded, false, _) => "Unfolded ink preview",
            (BoardMode::Comparison(_), true, true) => "RESULT",
            (BoardMode::Comparison(_), true, false) => "OPENED COMPARISON",
            (BoardMode::Comparison(_), false, _) => "Opened comparison",
        }
    }

    fn grid(&self, profile: StyleProfile) -> Vec<Line<'static>> {
        match self.mode {
            BoardMode::Folded => {
                folded_grid(self.attempt, self.cursor, self.preview, self.ink, profile)
            }
            BoardMode::Unfolded => unfolded_grid(self.attempt, self.ink, profile),
            BoardMode::Comparison(revealed) => {
                comparison_grid(self.attempt, self.puzzle, self.ink, revealed, profile)
            }
        }
    }

    fn render_stack(&self, frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
        render_stack(
            frame,
            area,
            self.attempt,
            self.cursor,
            self.preview,
            self.ink,
            profile,
        );
    }
}

fn wide_regions(area: Rect) -> [Rect; 3] {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(40),
            Constraint::Percentage(24),
        ])
        .areas(area)
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

pub(super) fn folded_grid(
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

pub(super) fn action_coordinate(action: PaperAction) -> Option<Coordinate> {
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
