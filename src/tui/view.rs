use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::app::{App, Screen};
use super::components::{
    BranchChoices, DialogLayer, RulesStep, SettingsPanel, StatusBar, TerminalMark,
};
use super::layout::{LayoutMode, MINIMUM_HEIGHT, MINIMUM_WIDTH, ShellLayout};
use super::style::StyleProfile;

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

    match app.screen() {
        Screen::Branch => {
            TerminalMark::render(frame, shell.mark, app.mark_frame(now), profile);
            BranchChoices::render(frame, shell.branch, app.selection(), profile);
        }
        Screen::Rules => {
            RulesStep::render(frame, content_area(shell), app.selection(), profile);
        }
        Screen::Settings => {
            SettingsPanel::render(
                frame,
                content_area(shell),
                app.settings(),
                app.selection(),
                profile,
            );
        }
    }
    StatusBar::render(frame, shell.status, app.focused(), profile);
    if let Some(overlay) = app.overlay() {
        let overlay_area = if mode == LayoutMode::Preferred {
            shell.mark
        } else {
            content_area(shell)
        };
        if mode == LayoutMode::Narrow {
            frame.render_widget(Clear, overlay_area);
        }
        DialogLayer::render(frame, overlay_area, overlay, profile);
    }
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
    use std::time::Instant;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::storage::{GlyphMode, Settings};
    use crate::tui::style::ColorCapability;

    #[test]
    fn preferred_narrow_and_resize_views_all_render() {
        let now = Instant::now();
        let app = App::new(Settings::default(), now);
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);

        for (width, height) in [(80, 24), (60, 20), (59, 19)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("view renders");
        }
    }

    #[test]
    fn resize_does_not_discard_dialog_or_focus_state() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let selected = app.selection();

        for (width, height) in [(80, 24), (59, 19), (60, 20), (100, 36)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let profile = StyleProfile::new(ColorCapability::Ansi16, GlyphMode::Unicode);
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("view renders");
        }

        assert_eq!(app.selection(), selected);
        assert!(app.overlay().is_some());
    }

    #[test]
    fn preferred_dialog_keeps_the_branch_readable_beside_it() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);

        for (width, height) in [(80, 24), (160, 60)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render(frame, &app, profile, now))
                .expect("view renders");
            let rendered = rendered_text(&terminal);

            assert!(rendered.contains("Leave Orifude?"));
            assert!(rendered.contains("Home branch"));
            assert!(rendered.contains("Continue the journey"));
        }
    }

    #[test]
    fn narrow_dialog_clears_the_stacked_branch_behind_it() {
        let now = Instant::now();
        let mut app = App::new(Settings::default(), now);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Unicode);
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");

        terminal
            .draw(|frame| render(frame, &app, profile, now))
            .expect("view renders");
        let rendered = rendered_text(&terminal);

        assert!(rendered.contains("Leave Orifude?"));
        assert!(!rendered.contains("Home branch"));
        assert!(!rendered.contains("Terminal settings"));
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
}
