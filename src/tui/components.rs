use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::storage::{ColorMode, GlyphMode, Settings};

use super::app::{MARK_FRAME_COUNT, Overlay};
use super::layout::centered;
use super::style::StyleProfile;

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

// Each Braille cell carries a 2-by-4 sample from the supplied monochrome mark.
// The two fixed sizes preserve its silhouette without reading an image at runtime.
const MEDIUM_MARK_WIDTH: usize = 40;
const MEDIUM_MARK: [&str; 12] = [
    "              ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡀",
    "         ⣀⣤⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣶⣤⣄⡀",
    "      ⣠⣶⣿⣿⣿⣿⣿⡿⠿⠛⠛⠛⠉⠙⠛⠛⠿⣿⣿⣿⣿⣿⣿⣿⣿⣆",
    "    ⣠⣾⣿⣿⣿⣿⠟⠋⠁           ⠙⢿⣿⣿⣿⡿⡟⠋",
    "   ⣰⣿⣿⣿⣿⠋        ⣽⣆⣸⣆     ⠈⠑⠈⠁   ⢤⣤⡀",
    "  ⢀⣿⣿⣿⣿⠃    ⣀⣠⣤⣤⣾⣿⣿⣟⣿⣻⣦⡀    ⠤⠄ ⢠⡤  ⠉",
    "  ⠘⣿⣿⣿⡏  ⢠⣶⣿⣿⣿⣿⣿⣿⣿⠋⠉⡉⠁⠈⠁     ⡷⠂ ⣤⠄  ⠻",
    "   ⢻⣿⣿⣷⡀ ⣿⣿⣿⣿⣿⣯⣻⣿⠿⠤⢾⡄     ⡀⣠⠞⠁⢀⣄  ⠖⢀⠆",
    "    ⠻⣿⣿⣷⣤⣹⣿⣿⣿⣿⣿⣼⣿⣂       ⣠⣏⡠⠤⠤⠂ ⠤⠚⠈⡀",
    "     ⠈⠛⢿⣿⣿⣿⣿⣿⣿⠟⠛⠛⠛⠛⠛⠻⠶⡶⠾⠟⠉⢠⣤   ⠖⢀⠤⠊",
    "        ⠈⠙⠻⠿⣿⣿⣿⣶⣶⣤⠤   ⠈⠓  ⢀⣀⣠⠤⠚⠊⠁",
    "             ⠈⠉⠉⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉",
];
const LARGE_MARK_WIDTH: usize = 48;
const LARGE_MARK: [&str; 15] = [
    "                    ⢀⣀⣀⣀⣀⣀",
    "            ⢀⣀⣤⣴⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣶⣶⣤⣄⡀",
    "         ⣠⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣦⣀",
    "      ⢀⣴⣾⣿⣿⣿⣿⣿⣿⠿⠛⠋⠉⠁     ⠈⠉⠛⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡕",
    "     ⣴⣿⣿⣿⣿⣿⡿⠟⠉               ⠈⠻⣿⡿⣿⣿⠿⣝⠃⠈",
    "    ⣼⣿⣿⣿⣿⡿⠋         ⢸⣷⡀⢸⣆      ⠈⠙⠂⠉⠁   ⢀⣤⣄",
    "   ⢸⣿⣿⣿⣿⡿⠁        ⣀⣠⣞⣿⣿⣿⣿⢷⣦⡀     ⢀⣀   ⣀⡀⠈⠉⠳",
    "   ⣾⣿⣿⣿⣿⠃   ⢀⣤⣶⣾⣿⣿⣿⣿⣿⣿⣿⡷⠿⠗⠛⠷     ⠈⠙⡄⢀⠄⠉⠁   ⣠⡀",
    "   ⢸⣿⣿⣿⣿   ⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⡏ ⢀⡜         ⢠⣏⠥ ⠲⠗   ⢉⠁",
    "   ⠈⢿⣿⣿⣿⡆ ⢰⣿⣿⣿⣿⣿⣿⣦⡙⣿⡿⠷⠶⣿⡦      ⢄⢀⡴⠉ ⢠⣤  ⢈⠟⢁⠞",
    "    ⠈⢻⣿⣿⣿⣦⣀⢻⣿⣿⣿⣿⣿⣿⣵⣿⣇      ⡀  ⢀⡾⣁⣀⣀⣠⣂⣀⣀⠤⠚⠊⠁",
    "      ⠙⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠿⠿⠷⠶⠶⣦⣤⣤⣤⣴⠾⠛⠉⠁    ⣤⡄ ⣀⠔⠁",
    "        ⠈⠛⠿⣿⣿⣿⣿⣿⣧⣤⣀⡀     ⠈⠑⢄⡈⠉⠁ ⠛⠋  ⣠⣂⠴⠊⠁",
    "            ⠉⠛⠻⠿⣿⣿⣿⣿⣿⣿⣂⣀⣀⣀⣀⣀⣀⣀⣀⣠⡤⠴⠒⠋⠉",
    "                  ⠉⠉⠉⠛⠛⠛⠛⠛⠛⠛⠉⠉⠁",
];
const ASCII_MARK_WIDTH: usize = 33;
const ASCII_MARK: [&str; 10] = [
    "        .----------------.",
    "    .--'                  `-.",
    "  .'       /\\ /\\             `.",
    " /    ____/  V  \\__      *     \\",
    "|   /'         o   `-.  /       |",
    "|  /         .---'   `-*        |",
    " \\ |  .---. /        /          /",
    "  \\ \\/     \\\\  /\\   *          /",
    "   `.`      `-/__\\-------..--'",
    "      `----------------'",
];
const BRANCH_CHOICES: [&str; 7] = [
    "Continue the journey",
    "Today's paper",
    "Endless garden",
    "Puzzle packs",
    "Keepsakes",
    "How to play",
    "Terminal settings",
];

pub(crate) struct Paper;

impl Paper {
    pub(crate) fn block(title: &str, profile: StyleProfile) -> Block<'_> {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(profile.border())
            .title(Span::styled(title, profile.title()));
        match profile.glyph_mode() {
            GlyphMode::Unicode => block.border_set(border::ROUNDED),
            GlyphMode::Ascii => block.border_set(ASCII_BORDER),
        }
    }
}

pub(crate) struct FocusList<'a> {
    choices: &'a [&'a str],
    selected: usize,
}

impl<'a> FocusList<'a> {
    pub(crate) const fn new(choices: &'a [&'a str], selected: usize) -> Self {
        Self { choices, selected }
    }

    pub(crate) fn render(
        self,
        frame: &mut Frame<'_>,
        area: Rect,
        title: &str,
        profile: StyleProfile,
    ) {
        let marker = match profile.glyph_mode() {
            GlyphMode::Unicode => "›",
            GlyphMode::Ascii => ">",
        };
        let items = self.choices.iter().enumerate().map(|(index, choice)| {
            let line = if index == self.selected {
                Line::styled(format!("{marker} {choice}"), profile.active())
            } else {
                Line::styled(format!("  {choice}"), profile.ink())
            };
            ListItem::new(line)
        });
        frame.render_widget(List::new(items).block(Paper::block(title, profile)), area);
    }
}

pub(crate) struct BranchChoices;

impl BranchChoices {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        selected: usize,
        profile: StyleProfile,
    ) {
        let card = centered(area, 32, 9);
        FocusList::new(&BRANCH_CHOICES, selected).render(frame, card, "Home branch", profile);
        if card.bottom().saturating_add(1) < area.bottom() {
            let note = Rect::new(card.x, card.bottom().saturating_add(1), card.width, 1);
            frame.render_widget(
                Paragraph::new("Seven paths, entirely offline.")
                    .style(StyleProfile::muted())
                    .alignment(Alignment::Center),
                note,
            );
        }
    }
}

pub(crate) struct TerminalMark;

impl TerminalMark {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        mark_frame: usize,
        profile: StyleProfile,
    ) {
        if area.height < 14 || area.width < 40 {
            render_compact_mark(frame, area, mark_frame, profile);
            return;
        }

        match profile.glyph_mode() {
            GlyphMode::Unicode if area.width >= 52 && area.height >= 20 => render_full_mark(
                frame,
                area,
                &LARGE_MARK,
                LARGE_MARK_WIDTH,
                mark_frame,
                profile,
            ),
            GlyphMode::Unicode => render_full_mark(
                frame,
                area,
                &MEDIUM_MARK,
                MEDIUM_MARK_WIDTH,
                mark_frame,
                profile,
            ),
            GlyphMode::Ascii => render_full_mark(
                frame,
                area,
                &ASCII_MARK,
                ASCII_MARK_WIDTH,
                mark_frame,
                profile,
            ),
        }
    }
}

fn render_full_mark(
    frame: &mut Frame<'_>,
    area: Rect,
    source: &[&str],
    source_width: usize,
    mark_frame: usize,
    profile: StyleProfile,
) {
    let mark_height = u16::try_from(source.len()).unwrap_or(area.height);
    let mark_width = u16::try_from(source_width).unwrap_or(area.width);
    let card = centered(area, mark_width, mark_height.saturating_add(2));

    let lines = match profile.glyph_mode() {
        GlyphMode::Unicode => reveal_braille(source, source_width, mark_frame, profile),
        GlyphMode::Ascii => reveal_ascii(source, source_width, mark_frame, profile),
    };
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left),
        Rect::new(card.x, card.y, card.width, mark_height),
    );
    frame.render_widget(
        Paragraph::new("O R I F U D E")
            .style(profile.title())
            .alignment(Alignment::Center),
        Rect::new(card.x, card.y.saturating_add(mark_height), card.width, 1),
    );
    let caption = if mark_frame >= MARK_FRAME_COUNT - 1 {
        "A small paper has arrived."
    } else {
        "Ink is settling into the paper."
    };
    frame.render_widget(
        Paragraph::new(caption)
            .style(StyleProfile::muted())
            .alignment(Alignment::Center),
        Rect::new(
            card.x,
            card.y.saturating_add(mark_height).saturating_add(1),
            card.width,
            1,
        ),
    );
}

fn render_compact_mark(
    frame: &mut Frame<'_>,
    area: Rect,
    mark_frame: usize,
    profile: StyleProfile,
) {
    let final_frame = mark_frame >= MARK_FRAME_COUNT - 1;
    let (mark, caption) = match (profile.glyph_mode(), final_frame) {
        (GlyphMode::Unicode, true) => ("◇  O R I F U D E  ─────╯ •", "A small paper has arrived."),
        (GlyphMode::Unicode, false) => ("◇  O R I F U D E  ──", "Ink is settling into the paper."),
        (GlyphMode::Ascii, true) => ("<> O R I F U D E  -----' *", "A small paper has arrived."),
        (GlyphMode::Ascii, false) => ("<> O R I F U D E  --", "Ink is settling into the paper."),
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(mark, profile.paper()),
            Line::styled(caption, StyleProfile::muted()),
        ])
        .alignment(Alignment::Center),
        area,
    );
}

fn reveal_braille(
    source: &[&str],
    source_width: usize,
    mark_frame: usize,
    profile: StyleProfile,
) -> Vec<Line<'static>> {
    let dot_width = source_width.saturating_mul(2);
    let dot_height = source.len().saturating_mul(4);
    source
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let mut revealed = String::with_capacity(row.len());
            for (column_index, character) in row.chars().enumerate() {
                let code = u32::from(character);
                if !(0x2800..=0x28ff).contains(&code) {
                    revealed.push(character);
                    continue;
                }
                let Ok(final_dots) = u8::try_from(code - 0x2800) else {
                    revealed.push(character);
                    continue;
                };
                let mut visible_dots = 0_u8;
                for (dot_x, dot_y, bit) in BRAILLE_DOTS {
                    let x = column_index.saturating_mul(2).saturating_add(dot_x);
                    let y = row_index.saturating_mul(4).saturating_add(dot_y);
                    if final_dots & bit != 0
                        && ink_has_arrived(x, y, dot_width, dot_height, mark_frame)
                    {
                        visible_dots |= bit;
                    }
                }
                revealed.push(char::from_u32(0x2800 + u32::from(visible_dots)).unwrap_or(' '));
            }
            Line::styled(revealed, profile.paper())
        })
        .collect()
}

fn reveal_ascii(
    source: &[&str],
    source_width: usize,
    mark_frame: usize,
    profile: StyleProfile,
) -> Vec<Line<'static>> {
    source
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let revealed = row
                .chars()
                .enumerate()
                .map(|(column_index, character)| {
                    if character == ' '
                        || ink_has_arrived(
                            column_index,
                            row_index,
                            source_width,
                            source.len(),
                            mark_frame,
                        )
                    {
                        character
                    } else {
                        ' '
                    }
                })
                .collect::<String>();
            Line::styled(revealed, profile.paper())
        })
        .collect()
}

const BRAILLE_DOTS: [(usize, usize, u8); 8] = [
    (0, 0, 1),
    (0, 1, 2),
    (0, 2, 4),
    (1, 0, 8),
    (1, 1, 16),
    (1, 2, 32),
    (0, 3, 64),
    (1, 3, 128),
];

fn ink_has_arrived(x: usize, y: usize, width: usize, height: usize, mark_frame: usize) -> bool {
    if mark_frame >= MARK_FRAME_COUNT - 1 {
        return true;
    }
    let sweep = x.saturating_add(height.saturating_sub(1).saturating_sub(y));
    let jitter = (x.saturating_mul(11) ^ y.saturating_mul(7)) % 4;
    let distance = sweep.saturating_mul(4).saturating_add(jitter);
    let furthest = width
        .saturating_add(height)
        .saturating_sub(2)
        .saturating_mul(4)
        .saturating_add(3);
    let ink_head_start = MARK_FRAME_COUNT / 4;
    let reached = mark_frame
        .saturating_add(ink_head_start)
        .saturating_mul(furthest)
        / MARK_FRAME_COUNT.saturating_add(ink_head_start);
    distance <= reached
}

pub(crate) struct StatusBar;

impl StatusBar {
    pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, focused: bool, profile: StyleProfile) {
        let separator = match profile.glyph_mode() {
            GlyphMode::Unicode => " · ",
            GlyphMode::Ascii => " | ",
        };
        let text = if focused {
            ["Up/Down move", "Enter open", "? help", "q quit"].join(separator)
        } else {
            "Terminal focus is elsewhere. Orifude is waiting.".to_owned()
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(StyleProfile::muted())
                .alignment(Alignment::Center),
            area,
        );
    }
}

pub(crate) struct RulesStep;

impl RulesStep {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        selected: usize,
        profile: StyleProfile,
    ) {
        let inner = Paper::block("How to play", profile).inner(area);
        frame.render_widget(Paper::block("How to play", profile), area);
        let copy = [
            Line::styled("A small sheet is waiting.", profile.title()),
            Line::from(""),
            Line::from("Fold the paper, brush ink through its stacked layers,"),
            Line::from("then unfold it to match the target exactly."),
            Line::from(""),
            Line::styled(
                "The complete interactive lesson arrives with the puzzle screen.",
                StyleProfile::muted(),
            ),
        ];
        frame.render_widget(
            Paragraph::new(Vec::from(copy)).wrap(Wrap { trim: true }),
            inner,
        );

        let buttons = ["Back to the branch", "Open lesson preview"];
        let controls = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(4),
            inner.width,
            4.min(inner.height),
        );
        FocusList::new(&buttons, selected).render(frame, controls, "Actions", profile);
    }
}

pub(crate) struct SettingsPanel;

impl SettingsPanel {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        settings: Settings,
        selected: usize,
        profile: StyleProfile,
    ) {
        let color = match settings.color_mode {
            ColorMode::Auto => "Automatic",
            ColorMode::Color => "Color",
            ColorMode::Monochrome => "Monochrome",
        };
        let glyphs = match settings.glyph_mode {
            GlyphMode::Unicode => "Unicode",
            GlyphMode::Ascii => "ASCII only",
        };
        let reduced_motion = enabled(settings.reduced_motion);
        let instant_reveal = enabled(settings.instant_reveal);
        let choices = [
            format!("Color: {color}"),
            format!("Symbols: {glyphs}"),
            format!("Reduced motion: {reduced_motion}"),
            format!("Instant reveal: {instant_reveal}"),
            "Back to the branch".to_owned(),
        ];
        let references = choices.iter().map(String::as_str).collect::<Vec<_>>();
        FocusList::new(&references, selected).render(frame, area, "Terminal settings", profile);
    }
}

fn enabled(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

pub(crate) struct Dialog;

impl Dialog {
    fn render(
        frame: &mut Frame<'_>,
        host: Rect,
        title: &str,
        body: Vec<Line<'_>>,
        footer: &str,
        style: Style,
        profile: StyleProfile,
    ) {
        let area = centered(host, 56, 10);
        frame.render_widget(Clear, area);
        let block = Paper::block(title, profile).border_style(style);
        frame.render_widget(
            Paragraph::new(body)
                .block(block)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left),
            area,
        );
        let footer_area = Rect::new(
            area.x.saturating_add(2),
            area.bottom().saturating_sub(2),
            area.width.saturating_sub(4),
            1,
        );
        frame.render_widget(
            Paragraph::new(footer)
                .style(StyleProfile::muted())
                .alignment(Alignment::Center),
            footer_area,
        );
    }
}

pub(crate) struct HelpPanel;

impl HelpPanel {
    pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, profile: StyleProfile) {
        Dialog::render(
            frame,
            area,
            "Keyboard help",
            vec![
                Line::from("Up/Down or j/k moves focus."),
                Line::from("Enter opens the focused path."),
                Line::from("Esc returns or cancels. q asks before leaving."),
                Line::from("? opens and closes this help."),
            ],
            "Esc, Enter, or ? closes help",
            profile.border(),
            profile,
        );
    }
}

pub(crate) struct ErrorPanel;

impl ErrorPanel {
    pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, message: &str, profile: StyleProfile) {
        Dialog::render(
            frame,
            area,
            "The paper stayed put",
            vec![Line::from(message)],
            "Enter or Esc returns",
            profile.error(),
            profile,
        );
    }
}

pub(crate) struct DialogLayer;

impl DialogLayer {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        overlay: &Overlay,
        profile: StyleProfile,
    ) {
        match overlay {
            Overlay::Help => HelpPanel::render(frame, area, profile),
            Overlay::Quit => Dialog::render(
                frame,
                area,
                "Leave Orifude?",
                vec![Line::from(
                    "Your saved settings are already tucked away. Leave the branch?",
                )],
                "y or Enter leaves; n or Esc stays",
                profile.border(),
                profile,
            ),
            Overlay::Error(message) => {
                ErrorPanel::render(frame, area, message.as_str(), profile);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use super::*;
    use crate::storage::GlyphMode;
    use crate::tui::style::ColorCapability;

    #[test]
    fn ascii_components_only_place_ascii_symbols_in_the_buffer() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        terminal
            .draw(|frame| {
                BranchChoices::render(frame, Rect::new(0, 0, 30, 12), 0, profile);
                TerminalMark::render(
                    frame,
                    Rect::new(30, 0, 30, 12),
                    MARK_FRAME_COUNT - 1,
                    profile,
                );
                StatusBar::render(frame, Rect::new(0, 18, 60, 2), true, profile);
            })
            .expect("render succeeds");

        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.symbol().is_ascii())
        );
    }

    #[test]
    fn opening_ink_wash_adds_detail_and_finishes_with_the_complete_mark() {
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        let opening = reveal_braille(&MEDIUM_MARK, MEDIUM_MARK_WIDTH, 0, profile);
        let finished = reveal_braille(
            &MEDIUM_MARK,
            MEDIUM_MARK_WIDTH,
            MARK_FRAME_COUNT - 1,
            profile,
        );

        assert!(braille_dot_count(&opening) > 0);
        assert!(braille_dot_count(&opening) < braille_dot_count(&finished));
        assert_eq!(
            finished.iter().map(Line::to_string).collect::<Vec<_>>(),
            MEDIUM_MARK.map(str::to_owned)
        );
    }

    #[test]
    fn artwork_rows_stay_inside_their_declared_widths() {
        for (rows, width) in [
            (MEDIUM_MARK.as_slice(), MEDIUM_MARK_WIDTH),
            (LARGE_MARK.as_slice(), LARGE_MARK_WIDTH),
            (ASCII_MARK.as_slice(), ASCII_MARK_WIDTH),
        ] {
            assert!(rows.iter().all(|row| row.chars().count() <= width));
        }
    }

    #[test]
    fn ascii_mark_is_centered_in_its_render_area() {
        let width = 48;
        let height = 14;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        terminal
            .draw(|frame| {
                TerminalMark::render(
                    frame,
                    Rect::new(0, 0, width, height),
                    MARK_FRAME_COUNT - 1,
                    profile,
                );
            })
            .expect("render succeeds");

        let buffer = terminal.backend().buffer();
        let mark_height = u16::try_from(ASCII_MARK.len()).expect("mark height fits");
        let card_y = (height - mark_height.saturating_add(2)) / 2;
        let mut left = width;
        let mut right = 0;
        for y in card_y..card_y + mark_height {
            for x in 0..width {
                if buffer[(x, y)].symbol() != " " {
                    left = left.min(x);
                    right = right.max(x);
                }
            }
        }
        let left_margin = left;
        let right_margin = width - right - 1;

        assert!(left_margin.abs_diff(right_margin) <= 1);
    }

    fn braille_dot_count(lines: &[Line<'_>]) -> u32 {
        let mut count = 0;
        for line in lines {
            for character in line.to_string().chars() {
                count += u32::from(character)
                    .checked_sub(0x2800)
                    .filter(|dots| *dots <= 0xff)
                    .map_or(0, u32::count_ones);
            }
        }
        count
    }
}
