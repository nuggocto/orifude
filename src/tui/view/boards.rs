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
        let grid_width = u16::from(dimensions.width().get()) * 2 + RULER_WIDTH + 4;
        let grid_height = u16::from(dimensions.height().get()) + 3;
        regions[0].width < grid_width || regions[1].width < grid_width || area.height < grid_height
    }

    fn render_compact(&self, frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
        let regions = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(area);
        let (title, grid) = if self.target_visible {
            ("Pattern to match", target_grid(self.puzzle, profile))
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
            0,
            true,
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
            0,
            true,
            active,
            profile,
        );
        self.render_stack(frame, regions[2], profile);
        regions[0]
    }

    // The active board is marked by its highlighted title, not by capitals.
    fn title(&self, active: bool, short: bool) -> &'static str {
        match (&self.mode, active, short) {
            (BoardMode::Folded, true, true) => "Paper",
            (BoardMode::Folded, _, _) => "Folded paper",
            (BoardMode::Unfolded, true, true) => "Preview",
            (BoardMode::Unfolded, true, false) => "Unfolded preview",
            (BoardMode::Unfolded, false, _) => "Unfolded ink preview",
            (BoardMode::Comparison(_), true, true) => "Result",
            (BoardMode::Comparison(_), _, _) => "Opened comparison",
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
            Constraint::Percentage(38),
            Constraint::Percentage(26),
        ])
        .areas(area)
}

/// Width of the row numbers and their gap at the start of each grid line.
const RULER_WIDTH: u16 = 3;

#[allow(clippy::too_many_arguments)]
fn render_grid(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: Vec<Line<'static>>,
    first_row: u8,
    column_ruler: bool,
    active: bool,
    profile: StyleProfile,
) {
    let block = if active {
        Paper::highlighted_block(title, profile)
    } else {
        Paper::block(title, profile)
    };
    frame.render_widget(
        Paragraph::new(with_rulers(rows, first_row, column_ruler))
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

/// Numbers the columns above the grid and each row at its start, so written
/// positions such as "row 2, column 3" can be found on the paper. Columns past
/// nine show their last digit to keep one cell per number.
fn with_rulers(rows: Vec<Line<'static>>, first_row: u8, column_ruler: bool) -> Vec<Line<'static>> {
    let columns = rows.first().map_or(0, |row| row.spans.len() / 2);
    let ruler = StyleProfile::muted();
    let header = std::iter::once(Span::styled(" ".repeat(usize::from(RULER_WIDTH)), ruler))
        .chain((1..=columns).map(|column| Span::styled(format!("{} ", column % 10), ruler)))
        .collect::<Vec<_>>();
    column_ruler
        .then(|| Line::from(header))
        .into_iter()
        .chain(rows.into_iter().zip(first_row..).map(|(row, index)| {
            let mut spans = vec![Span::styled(format!("{:>2} ", u16::from(index) + 1), ruler)];
            spans.extend(row.spans);
            Line::from(spans)
        }))
        .collect()
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
    // The column ruler takes the first line inside the border, unless the
    // paper must scroll; then every line shows paper and row numbers orient.
    let inner = usize::from(area.height.saturating_sub(2));
    let column_ruler = inner > rows.len();
    let visible = inner
        .saturating_sub(usize::from(column_ruler))
        .min(rows.len());
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
        u8::try_from(start).unwrap_or(u8::MAX),
        column_ruler,
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

const fn blank_symbol(glyph_mode: GlyphMode) -> char {
    match glyph_mode {
        GlyphMode::Unicode => '○',
        GlyphMode::Ascii => 'o',
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

fn render_stack(
    frame: &mut Frame<'_>,
    area: Rect,
    attempt: &Attempt,
    cursor: Option<Coordinate>,
    preview: Option<PaperAction>,
    ink: InkPattern,
    profile: StyleProfile,
) {
    // Inside the border and padding; long origin labels need 19 columns.
    let roomy = area.width >= 23;
    let mut lines = Vec::new();
    if let Some(coordinate) = cursor {
        let mut stack = crate::domain::paper::StackView::new();
        attempt
            .stack_at(coordinate, &mut stack)
            .expect("cursor stack is inside the paper");
        let (row, column) = (coordinate.row().get() + 1, coordinate.column().get() + 1);
        let heading = if roomy {
            format!("Row {row}, column {column}")
        } else {
            format!("Row {row}, col {column}")
        };
        lines.push(Line::styled(heading, profile.title()));
        // Narrow panels are short too: the numbered layers show the count.
        let layers = stack.cell_ids();
        let caption = match layers.len() {
            0 => "No paper here".to_owned(),
            _ if !roomy => "Began at:".to_owned(),
            1 => "1 layer began at:".to_owned(),
            count => format!("{count} layers began at:"),
        };
        lines.push(Line::styled(caption, StyleProfile::muted()));
        let dimensions = attempt.dimensions();
        for (layer, id) in layers.iter().enumerate() {
            let (mark, style) = if ink.contains(*id) {
                (ink_symbol(profile.glyph_mode()), profile.ink_mark())
            } else {
                (blank_symbol(profile.glyph_mode()), profile.ink())
            };
            let origin = dimensions
                .original_coordinate(*id)
                .expect("a stacked cell belongs to the paper");
            let (row, column) = (origin.row().get() + 1, origin.column().get() + 1);
            let place = if roomy {
                format!("row {row}, col {column}")
            } else {
                format!("r{row} c{column}")
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", layer + 1), StyleProfile::muted()),
                Span::styled(mark.to_string(), style),
                Span::styled(format!(" {place}"), profile.ink()),
            ]));
        }
    } else {
        lines.push(Line::from("Select a cell to inspect its layers."));
    }
    if let Some(PaperAction::Fold(fold)) = preview {
        lines.push(Line::from(""));
        lines.push(Line::styled(fold_label(fold), profile.paper()));
    }
    // The padded title needs its length plus two corners and one border dash.
    let title = if area.width < 25 {
        "Low to high"
    } else {
        "Stack, bottom to top"
    };
    let block = if roomy {
        Paper::block(title, profile)
    } else {
        Paper::block(title, profile).padding(ratatui::widgets::Padding::left(1))
    };
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
