use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::boards::folded_grid;
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
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Char('f'), now);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let profile = StyleProfile::new(ColorCapability::Ansi16, GlyphMode::Unicode);
    terminal
        .draw(|frame| render(frame, &app, profile, now))
        .expect("view renders");
    let text = rendered_text(&terminal);
    assert!(text.contains("Pattern to match"));
    assert!(text.contains("Folded paper"));
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
    press(&mut app, KeyCode::Enter, now);

    let text = player_text(&app, now);
    assert!(text.contains("Squirrel says"));
    assert!(text.contains("The fold tool is ready."));
    assert!(player_text_with_glyphs(&app, now, GlyphMode::Ascii).is_ascii());

    press(&mut app, KeyCode::Enter, now);
    assert!(player_text(&app, now).contains("Move @ to row 2, column 3"));

    press(&mut app, KeyCode::Down, now);
    press(&mut app, KeyCode::Right, now);
    press(&mut app, KeyCode::Right, now);
    assert!(player_text(&app, now).contains("The dot brush is ready."));

    press(&mut app, KeyCode::Enter, now);
    assert!(player_text(&app, now).contains("Enter opens the paper."));
}

#[test]
fn lesson_coach_leads_the_player_back_from_misplaced_ink() {
    let now = Instant::now();
    let mut app = App::new(Settings::default(), now);
    press(&mut app, KeyCode::Enter, now);

    press(&mut app, KeyCode::Char('b'), now);
    press(&mut app, KeyCode::Enter, now);
    let text = player_text(&app, now);
    assert!(text.contains("The ink came"));
    assert!(text.contains("fold."));

    press(&mut app, KeyCode::Char('u'), now);
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Right, now);
    press(&mut app, KeyCode::Right, now);
    press(&mut app, KeyCode::Enter, now);
    assert!(player_text(&app, now).contains("That dot missed the target."));

    press(&mut app, KeyCode::Char('u'), now);
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
    assert!(branch.contains("Home | Journey 0/40"));

    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);
    terminal
        .draw(|frame| render(frame, &app, profile, now))
        .expect("paper renders");
    assert!(rendered_text(&terminal).is_ascii());
}

#[test]
fn branch_card_title_names_complete_journey_progress() {
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
    assert!(text.contains("Home · Journey 40/40"));
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
        assert!(text.contains("the full canopy."));
        assert!(text.contains("8 of 8"));
        assert!(text.contains("Journey 40/40"));
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
    assert!(branch.contains("0 of 8"));
    assert!(branch.contains("Home · Journey 0/40"));

    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);
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
        press(&mut app, KeyCode::Enter, now);
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
    assert!(text.contains("Folded paper"));
    assert!(!text.contains("[ACTIVE]"));
    assert!(text.contains("Low to high"));
    assert!(text.contains("q quit"));
}

#[test]
fn compact_tool_selection_stays_visible_after_an_action() {
    use KeyCode;
    let now = Instant::now();
    let mut app = App::new(
        Settings {
            reduced_motion: true,
            ..Settings::default()
        },
        now,
    );
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);
    for (key, ready) in [
        (KeyCode::Tab, "Ready: Open paper"),
        (KeyCode::BackTab, "Ready: Dot brush"),
    ] {
        press(&mut app, key, now);
        for (width, height) in [(60, 20), (80, 24)] {
            assert!(menu_text(&app, now, width, height).contains(ready));
        }
    }
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
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Down, now);
    press(&mut app, KeyCode::Enter, now);

    let fresh = menu_text(&app, now, 80, 24);
    assert!(!fresh.contains("The brush follows @; this paper stays flat."));
    assert!(!fresh.contains("Clue:"));
    assert!(!fresh.contains("Hint:"));

    press(&mut app, KeyCode::Esc, now);
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);

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
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("compact paper renders");
    let paper = rendered_text(&terminal);
    assert!(paper.contains("Folded paper rows"));
    assert!(paper.contains("-12/12"));
    assert!(paper.contains('@'));
    assert!(!paper.contains("PATTERN TO MATCH"));

    session.handle_key(
        KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("compact target renders");
    let target = rendered_text(&terminal);
    assert!(target.contains("Pattern to match rows"));
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
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        KeyBindings::default(),
        now,
        true,
    );
    session.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("failed result renders");
    let first_result = rendered_text(&terminal);
    let first_stack = rendered_area_text(&terminal, stack_area);
    assert!(first_result.contains("Opened comparison rows 1-5/8"));

    for _ in 0..7 {
        session.handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("scrolled failed result renders");
    let result = rendered_text(&terminal)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(result.contains("Opened comparison rows 4-8/8"));
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
    press(&mut app, KeyCode::Enter, now);
    for _ in 0..20 {
        press(&mut app, KeyCode::Down, now);
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
        press(&mut app, KeyCode::Down, now);
    }
    press(&mut app, KeyCode::Enter, now);
    for _ in 0..10 {
        press(&mut app, KeyCode::Down, now);
    }
    press(&mut app, KeyCode::Enter, now);

    let text = menu_text(&app, now, 60, 20);
    assert!(text.contains("› Quit            q"));
    assert!(text.contains("Press one unused key"));
}

#[test]
fn failed_opening_does_not_claim_a_match_or_a_save() {
    use KeyCode;

    let now = Instant::now();
    let mut app = App::new(Settings::default(), now);
    for code in [KeyCode::Enter, KeyCode::Enter, KeyCode::Esc, KeyCode::Enter] {
        press(&mut app, code, now);
    }
    let text = menu_text(&app, now, 100, 30);
    assert!(text.contains("Opening crease"));
    assert!(!text.contains("matched result"));
    assert!(!text.contains("being saved"));

    let text = menu_text(&app, now + std::time::Duration::from_secs(2), 100, 30);
    assert!(text.contains("2 missing (?) and 0 extra (!)"));
    assert!(text.contains("Enter returns to the attempt"));
}

#[test]
fn minimum_saved_result_keeps_next_return_and_export_controls_visible() {
    use KeyCode;

    let paper = &crate::content::journey()[0];
    let now = Instant::now();
    let mut app = App::new(
        Settings {
            lesson_complete: true,
            reduced_motion: true,
            glyph_mode: GlyphMode::Ascii,
            ..Settings::default()
        },
        now,
    );
    for code in [
        KeyCode::Enter,
        KeyCode::Enter,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Enter,
        KeyCode::Enter,
    ] {
        press(&mut app, code, now);
    }
    app.completion_saved(PuzzleProgress {
        pack_id: paper.puzzle().identity().pack_id().into(),
        puzzle_id: paper.puzzle().identity().puzzle_id().into(),
        attempt_count: 1,
        best_folds: 0,
        best_strokes: 1,
        best_replay_id: 1,
        updated_at_unix_seconds: 1,
    });
    let render_compact = |app: &App| {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let profile = StyleProfile::new(ColorCapability::Monochrome, GlyphMode::Ascii);
        terminal
            .draw(|frame| render(frame, app, profile, now))
            .expect("compact result renders");
        rendered_text(&terminal)
    };
    let text = render_compact(&app);
    assert!(text.contains("Paper complete"));
    assert!(text.contains("0 folds and 1 stroke"));
    assert!(text.contains("Tab next"));
    assert!(text.contains("Enter back"));
    assert!(text.contains("x keepsake"));
    assert!(text.is_ascii());

    press(&mut app, KeyCode::Char('?'), now);
    let help = render_compact(&app);
    assert!(help.contains("Open the next Journey paper"));
    assert!(help.contains("Esc or Enter closes help"));
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
fn above_reference_success_shows_the_result_and_reference_score() {
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
    for code in [KeyCode::Enter, KeyCode::Tab, KeyCode::Enter, KeyCode::Enter] {
        session.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("above-reference result renders");
    let text = rendered_text(&terminal)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Paper complete"));
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
                false,
                true,
            );
        })
        .expect("saved replay renders");
    let text = rendered_text(&terminal);
    assert!(text.contains("Folded paper"));
    assert!(text.contains("Replay ready: fresh paper."));
    assert!(!text.contains("OPENED COMPARISON"));
    assert!(replay_status_text(&session, KeyBindings::default(), " · ", 60).contains("Left/Right"));

    session.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        KeyBindings::default(),
        now,
        true,
    );
    session.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("completed replay renders");
    let text = rendered_text(&terminal);
    assert!(text.contains("Opened comparison"));
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
                false,
                true,
            );
        })
        .expect("empty replay renders");

    assert!(rendered_text(&terminal).contains("no recorded actions. Enter opens it."));
    assert!(replay_status_text(&session, KeyBindings::default(), " · ", 76).contains("Enter open"));
}

#[test]
fn saved_lesson_footer_names_the_actual_return_action() {
    let now = Instant::now();
    let mut app = App::new(Settings::default(), now);
    for code in [
        KeyCode::Enter,
        KeyCode::Enter,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Enter,
        KeyCode::Enter,
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
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Char('?'), now);
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
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
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
                false,
                true,
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
                false,
                true,
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
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        KeyBindings::default(),
        started,
        true,
    );
    for code in [
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Enter,
    ] {
        session.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
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
                false,
                true,
            );
        })
        .expect("ink state renders");
    let text = rendered_text(&terminal);
    assert!(text.contains('◉'));
    assert!(text.contains("Ready: Open paper"));
    // Both inked layers name where they began on the open sheet.
    assert!(text.contains("1 ● r2 c3"));
    assert!(text.contains("2 ● r2 c2"), "{text}");
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
        press(&mut app, KeyCode::Down, now);
    }
    press(&mut app, KeyCode::Enter, now);

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
            press(&mut app, KeyCode::Right, now);
        }
    }
}

#[test]
fn boards_number_rows_and_columns_used_by_written_positions() {
    let now = Instant::now();
    let mut app = App::new(Settings::default(), now);
    press(&mut app, KeyCode::Enter, now);
    press(&mut app, KeyCode::Enter, now);
    for code in [KeyCode::Down, KeyCode::Right, KeyCode::Right] {
        press(&mut app, code, now);
    }

    let text = menu_text(&app, now, 100, 30);
    // Both grids carry the column ruler and the coach's "row 2" label.
    assert_eq!(text.matches("1 2 3 4").count(), 2, "{text}");
    assert!(text.contains(" 2 . # # ."), "{text}");
    // After the fold, the stack names where each of its layers began.
    assert!(text.contains("Row 2, column 3"), "{text}");
    assert!(text.contains("2 layers began at:"), "{text}");
    assert!(text.contains("1 ○ row 2, col 3"), "{text}");
    assert!(text.contains("2 ○ row 2, col 2"), "{text}");
    assert!(!text.contains("cell 6"), "{text}");
}

#[test]
fn completion_card_leaves_the_opened_paper_visible() {
    let now = Instant::now();
    let mut app = App::new(
        Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        },
        now,
    );
    for code in [
        KeyCode::Enter,
        KeyCode::Enter,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Enter,
        KeyCode::Enter,
    ] {
        press(&mut app, code, now);
    }
    let paper = &crate::content::journey()[0];
    app.completion_saved(PuzzleProgress {
        pack_id: paper.puzzle().identity().pack_id().into(),
        puzzle_id: paper.puzzle().identity().puzzle_id().into(),
        attempt_count: 1,
        best_folds: 0,
        best_strokes: 1,
        best_replay_id: 1,
        updated_at_unix_seconds: 1,
    });

    let text = menu_text(&app, now, 100, 30);
    assert!(text.contains("Paper complete"), "{text}");
    // The bottom row appears in the target and again in the opened paper.
    assert_eq!(text.matches(" 4 . . . .").count(), 2, "{text}");
}

#[test]
fn lesson_completion_does_not_promise_a_keepsake() {
    let now = Instant::now();
    let mut app = App::new(
        Settings {
            reduced_motion: true,
            ..Settings::default()
        },
        now,
    );
    for code in [
        KeyCode::Enter,
        KeyCode::Enter,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Enter,
        KeyCode::Enter,
    ] {
        press(&mut app, code, now);
    }
    app.settings_saved(app.settings());

    let text = menu_text(&app, now, 100, 30);
    assert!(
        text.contains("Congratulations, your first paper matches."),
        "{text}"
    );
    assert!(
        text.contains("The journey's first paper is waiting."),
        "{text}"
    );
    assert!(!text.contains("keepsake is saved"), "{text}");
}

#[test]
fn journey_groups_papers_under_chapters_with_their_gifts() {
    let now = Instant::now();
    let mut done = vec![false; crate::content::journey().len()];
    done[..5].fill(true);
    let app = App::with_state(
        Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        },
        ProgressPage {
            entries: Vec::new(),
            has_more: false,
        },
        Vec::new(),
        done,
        CalendarDate::new(2026, 9, 3).expect("valid date"),
        1,
        now,
    );
    let mut app = app;
    press(&mut app, KeyCode::Enter, now);
    for _ in 0..5 {
        press(&mut app, KeyCode::Down, now);
    }

    let text = menu_text(&app, now, 100, 30);
    assert!(text.contains("1  Ink on paper  ◆ a first leaf"), "{text}");
    assert!(
        text.contains("2  Across the crease  · a pair of leaves"),
        "{text}"
    );
    assert!(text.contains("● 1.5  Small sprig"), "{text}");
    assert!(text.contains("› ○ 2.1  Folded pair"), "{text}");
    assert!(text.contains("· 2.2  Low reflection"), "{text}");
}

#[test]
fn keepsakes_use_the_names_players_saw() {
    let now = Instant::now();
    let progress = |pack_id: &str, puzzle_id: &str| PuzzleProgress {
        pack_id: pack_id.into(),
        puzzle_id: puzzle_id.into(),
        attempt_count: 1,
        best_folds: 1,
        best_strokes: 2,
        best_replay_id: 1,
        updated_at_unix_seconds: 1,
    };
    let mut app = App::with_state(
        Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        },
        ProgressPage {
            entries: vec![
                progress("orifude-journey", "first-drop"),
                progress("orifude-daily", "paper-v1-d049608d2164f808-0"),
                progress("removed-pack", "berry"),
            ],
            has_more: false,
        },
        Vec::new(),
        vec![false; crate::content::journey().len()],
        CalendarDate::new(2026, 9, 3).expect("valid date"),
        1,
        now,
    );
    for _ in 0..4 {
        press(&mut app, KeyCode::Down, now);
    }
    press(&mut app, KeyCode::Enter, now);

    let text = menu_text(&app, now, 100, 30);
    assert!(text.contains("Journey 1.1  First drop"), "{text}");
    assert!(text.contains("1 fold, 2 strokes"), "{text}");
    assert!(text.contains("Daily paper"), "{text}");
    assert!(text.contains("removed-pack  berry"), "{text}");
    assert!(text.contains("pack removed; replay kept"), "{text}");
    assert!(!text.contains("orifude-journey"), "{text}");
}

#[test]
fn empty_keepsakes_explain_themselves_and_offer_the_way_back() {
    let now = Instant::now();
    let mut app = App::new(
        Settings {
            lesson_complete: true,
            reduced_motion: true,
            ..Settings::default()
        },
        now,
    );
    for _ in 0..4 {
        press(&mut app, KeyCode::Down, now);
    }
    press(&mut app, KeyCode::Enter, now);
    app.keepsakes_loaded(
        ProgressPage {
            entries: Vec::new(),
            has_more: false,
        },
        0,
    );

    let text = menu_text(&app, now, 60, 20);
    assert!(text.contains("No keepsakes yet."), "{text}");
    assert!(text.contains("› Back to the branch"), "{text}");
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

fn press(app: &mut App, code: KeyCode, now: Instant) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE), now);
}
