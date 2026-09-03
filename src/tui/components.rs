use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::storage::GlyphMode;
use crate::{content, content::JourneyGroup};

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
        Self::styled_block(title, profile.border(), profile.title(), profile)
    }

    pub(crate) fn highlighted_block(title: &str, profile: StyleProfile) -> Block<'_> {
        Self::styled_block(
            title,
            profile.border(),
            StyleProfile::highlighted_title(),
            profile,
        )
    }

    fn styled_block(
        title: &str,
        border_style: Style,
        title_style: Style,
        profile: StyleProfile,
    ) -> Block<'_> {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, title_style));
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
        title: &str,
        profile: StyleProfile,
    ) {
        let card = centered(area, 32, 9);
        FocusList::new(&BRANCH_CHOICES, selected).render(frame, card, title, profile);
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

pub(crate) struct BranchGrowth;

impl BranchGrowth {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        completed_groups: usize,
        profile: StyleProfile,
    ) {
        let completed_groups = completed_groups.min(content::journey_groups().len());
        if area.height < 8 || area.width < 34 {
            frame.render_widget(
                Paragraph::new(branch_caption(completed_groups))
                    .style(profile.paper())
                    .alignment(Alignment::Center),
                area,
            );
            return;
        }

        let card = centered(area, 42, 12);
        let art = branch_art(completed_groups, profile.glyph_mode())
            .into_iter()
            .map(|line| Line::styled(line, profile.paper()))
            .chain(std::iter::once(Line::styled(
                branch_caption(completed_groups),
                profile.title(),
            )))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(art)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            card,
        );
    }
}

fn branch_caption(completed_groups: usize) -> String {
    completed_groups
        .checked_sub(1)
        .and_then(|index| content::journey_groups().get(index))
        .map_or_else(
            || "The branch is waiting for its first leaf. [0/8]".to_owned(),
            |group| {
                format!(
                    "The branch holds {}. [{completed_groups}/8]",
                    group.gift.label()
                )
            },
        )
}

fn branch_art(completed: usize, glyph_mode: GlyphMode) -> Vec<String> {
    let visible = |at: usize, symbol: &'static str| if completed >= at { symbol } else { " " };
    match glyph_mode {
        GlyphMode::Unicode => vec![
            format!(
                "                  {}     {}",
                visible(8, "◇"),
                visible(8, "◇")
            ),
            format!(
                "             {}  ╭──────{}",
                visible(7, "●"),
                visible(2, "◆")
            ),
            format!(
                "          {} ╭───╯   {}",
                visible(3, "●"),
                visible(7, "●")
            ),
            format!(
                "       {}────╯      ╭────{}",
                visible(1, "◆"),
                visible(2, "◆")
            ),
            format!(
                "  ─────╯       {}──╯  {}",
                visible(6, "◆"),
                visible(5, "v")
            ),
            format!("       {}        {}", visible(4, "△"), visible(3, "●")),
            "        \u{2572}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2571}".to_owned(),
            "             \u{2572}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2571}".to_owned(),
        ],
        GlyphMode::Ascii => vec![
            format!(
                "                  {}     {}",
                visible(8, "<>"),
                visible(8, "<>")
            ),
            format!(
                "             {}  .-------{}",
                visible(7, "o"),
                visible(2, "<>")
            ),
            format!(
                "          {} .---'   {}",
                visible(3, "o"),
                visible(7, "o")
            ),
            format!(
                "       {}----'      .----{}",
                visible(1, "<>"),
                visible(2, "<>")
            ),
            format!(
                "  -----'       {}--'  {}",
                visible(6, "<>"),
                visible(5, "v")
            ),
            format!("       {}        {}", visible(4, "/\\"), visible(3, "o")),
            "        `-------------'".to_owned(),
            "             `-----'".to_owned(),
        ],
    }
}

pub(crate) struct CompletionCourier;

impl CompletionCourier {
    pub(crate) fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        group: &JourneyGroup,
        profile: StyleProfile,
    ) {
        if area.height < 9 || area.width < 40 {
            let card = centered(area, 56, 7);
            frame.render_widget(Clear, card);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(
                        format!("/)_/)  {} is complete.", group.title),
                        profile.title(),
                    ),
                    Line::from(format!("I carried {} home.", group.gift.label())),
                    Line::from(""),
                    Line::from("Enter returns to the changed branch."),
                ])
                .block(Paper::block("A paper joins the branch", profile))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
                card,
            );
            return;
        }
        let card = centered(area, 56, 9);
        frame.render_widget(Clear, card);
        let squirrel = ["  /)_/)", " ( o.o)", " /|_ _|", "(_/ \\__)"];
        let gift = group.gift.label();
        let body = vec![
            Line::from(""),
            Line::styled(squirrel[0], profile.paper()),
            Line::styled(squirrel[1], profile.paper()),
            Line::styled(squirrel[2], profile.paper()),
            Line::styled(squirrel[3], profile.paper()),
            Line::styled(
                format!("{} is complete. I carried {gift} home.", group.title),
                profile.title(),
            ),
            Line::from("Enter returns to the changed branch."),
        ];
        frame.render_widget(
            Paragraph::new(body)
                .block(Paper::block("A paper joins the branch", profile))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            card,
        );
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
    pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, focused: bool, focused_text: &str) {
        let text = if focused {
            focused_text
        } else {
            "Terminal focus is elsewhere. Orifude is waiting."
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(StyleProfile::muted())
                .alignment(Alignment::Center),
            area,
        );
    }
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
        Self::render_with_height(frame, host, title, body, footer, style, profile, 10);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_with_height(
        frame: &mut Frame<'_>,
        host: Rect,
        title: &str,
        body: Vec<Line<'_>>,
        footer: &str,
        style: Style,
        profile: StyleProfile,
        height: u16,
    ) {
        let area = centered(host, 56, height);
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
    pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, message: &str, profile: StyleProfile) {
        Dialog::render_with_height(
            frame,
            area,
            "Keyboard help",
            vec![Line::from(message)],
            "Esc or Enter closes help",
            profile.border(),
            profile,
            area.height.min(18),
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
            Overlay::Help(message) => HelpPanel::render(frame, area, message.as_str(), profile),
            Overlay::Quit => Dialog::render(
                frame,
                area,
                "Leave Orifude?",
                vec![Line::from(
                    "Saved progress is safe. An unfinished paper is not saved. Leave Orifude?",
                )],
                "y or Enter leaves; n or Esc stays",
                profile.border(),
                profile,
            ),
            Overlay::Error(message) => {
                ErrorPanel::render(frame, area, message.as_str(), profile);
            }
            Overlay::Reset => Dialog::render(
                frame,
                area,
                "Smooth this paper flat?",
                vec![Line::from("Every action on this attempt will be cleared.")],
                "y or Enter resets; n or Esc keeps the draft",
                profile.border(),
                profile,
            ),
            Overlay::Export(lines) => Dialog::render(
                frame,
                area,
                "Text keepsake",
                lines.iter().map(|line| Line::from(line.as_str())).collect(),
                "Copy the text, then Enter or Esc returns",
                profile.border(),
                profile,
            ),
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
                BranchChoices::render(
                    frame,
                    Rect::new(0, 0, 30, 12),
                    0,
                    "Home | Journey 0/40 | Saved no",
                    profile,
                );
                TerminalMark::render(
                    frame,
                    Rect::new(30, 0, 30, 12),
                    MARK_FRAME_COUNT - 1,
                    profile,
                );
                StatusBar::render(
                    frame,
                    Rect::new(0, 18, 60, 2),
                    true,
                    "Up/Down move | Enter open | ? help | q quit",
                );
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
    fn branch_growth_has_safe_ascii_and_named_progress() {
        assert!(
            branch_art(8, GlyphMode::Ascii)
                .iter()
                .all(|line| line.is_ascii())
        );
        assert_eq!(
            branch_caption(0),
            "The branch is waiting for its first leaf. [0/8]"
        );
        assert!(branch_caption(8).contains("the full canopy"));

        let backend = TestBackend::new(44, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        terminal
            .draw(|frame| BranchGrowth::render(frame, Rect::new(0, 0, 44, 12), 0, profile))
            .expect("branch renders");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .fold(String::new(), |mut text, cell| {
                text.push_str(cell.symbol());
                text
            })
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("The branch is waiting for its first leaf. [0/8]"));
    }

    #[test]
    fn completion_courier_names_the_group_gift_and_return_action() {
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        for height in [8, 20] {
            let backend = TestBackend::new(60, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| {
                    CompletionCourier::render(
                        frame,
                        Rect::new(0, 0, 60, height),
                        &content::journey_groups()[0],
                        profile,
                    );
                })
                .expect("completion card renders");
            let text = terminal.backend().buffer().content().iter().fold(
                String::new(),
                |mut text, cell| {
                    text.push_str(cell.symbol());
                    text
                },
            );

            assert!(text.contains("Ink on paper is complete"));
            assert!(text.contains("a first leaf"));
            assert!(text.contains("Enter returns"));
            assert!(text.is_ascii());
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
