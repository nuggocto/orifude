use std::cell::Cell;
use std::io::{self, IsTerminal, Stdout, Write};
use std::panic;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableFocusChange, EnableFocusChange};
use crossterm::execute;
use crossterm::terminal::{
    DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::{TerminalOptions, Viewport};

static INSTALL_PANIC_HOOK: Once = Once::new();
static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
thread_local! {
    static TERMINAL_OWNED: Cell<bool> = const { Cell::new(false) };
}
const MAX_RENDER_WIDTH: u16 = 160;
const MAX_RENDER_HEIGHT: u16 = 60;

trait TerminalControl {
    fn enable_raw(&mut self) -> io::Result<()>;
    fn enter_alternate(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn disable_wrap(&mut self) -> io::Result<()>;
    fn enable_focus(&mut self) -> io::Result<()>;
    fn disable_focus(&mut self) -> io::Result<()>;
    fn enable_wrap(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn leave_alternate(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
}

#[derive(Default)]
struct SystemTerminal;

impl TerminalControl for SystemTerminal {
    fn enable_raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn enter_alternate(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Hide)
    }

    fn disable_wrap(&mut self) -> io::Result<()> {
        execute!(io::stdout(), DisableLineWrap)
    }

    fn enable_focus(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnableFocusChange)
    }

    fn disable_focus(&mut self) -> io::Result<()> {
        execute!(io::stdout(), DisableFocusChange)
    }

    fn enable_wrap(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnableLineWrap)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Show)
    }

    fn leave_alternate(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)
    }

    fn disable_raw(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Acquired(u8);

#[derive(Clone, Copy)]
enum TerminalCapability {
    Raw = 0,
    Alternate = 1,
    CursorHidden = 2,
    WrapDisabled = 3,
    FocusEnabled = 4,
}

impl Acquired {
    const fn mask(capability: TerminalCapability) -> u8 {
        1 << capability as u8
    }

    fn insert(&mut self, capability: TerminalCapability) {
        self.0 |= Self::mask(capability);
    }

    const fn contains(self, capability: TerminalCapability) -> bool {
        self.0 & Self::mask(capability) != 0
    }

    fn remove(&mut self, capability: TerminalCapability) {
        self.0 &= !Self::mask(capability);
    }
}

#[derive(Default)]
struct Lifecycle<C> {
    control: C,
    acquired: Acquired,
}

impl<C: TerminalControl> Lifecycle<C> {
    fn acquire(&mut self) -> io::Result<()> {
        let result = self.acquire_steps();
        if result.is_err() {
            let _restore_result = self.restore();
        }
        result
    }

    fn acquire_steps(&mut self) -> io::Result<()> {
        self.control.enable_raw()?;
        self.acquired.insert(TerminalCapability::Raw);
        self.control.enter_alternate()?;
        self.acquired.insert(TerminalCapability::Alternate);
        self.control.hide_cursor()?;
        self.acquired.insert(TerminalCapability::CursorHidden);
        self.control.disable_wrap()?;
        self.acquired.insert(TerminalCapability::WrapDisabled);
        self.control.enable_focus()?;
        self.acquired.insert(TerminalCapability::FocusEnabled);
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        if self.acquired.contains(TerminalCapability::FocusEnabled) {
            let result = self.control.disable_focus();
            clear_on_success(
                &mut self.acquired,
                TerminalCapability::FocusEnabled,
                &result,
            );
            remember_first(&mut first_error, result);
        }
        if self.acquired.contains(TerminalCapability::WrapDisabled) {
            let result = self.control.enable_wrap();
            clear_on_success(
                &mut self.acquired,
                TerminalCapability::WrapDisabled,
                &result,
            );
            remember_first(&mut first_error, result);
        }
        if self.acquired.contains(TerminalCapability::CursorHidden) {
            let result = self.control.show_cursor();
            clear_on_success(
                &mut self.acquired,
                TerminalCapability::CursorHidden,
                &result,
            );
            remember_first(&mut first_error, result);
        }
        if self.acquired.contains(TerminalCapability::Alternate) {
            let result = self.control.leave_alternate();
            clear_on_success(&mut self.acquired, TerminalCapability::Alternate, &result);
            remember_first(&mut first_error, result);
        }
        if self.acquired.contains(TerminalCapability::Raw) {
            let result = self.control.disable_raw();
            clear_on_success(&mut self.acquired, TerminalCapability::Raw, &result);
            remember_first(&mut first_error, result);
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn clear_on_success(
    acquired: &mut Acquired,
    capability: TerminalCapability,
    result: &io::Result<()>,
) {
    if result.is_ok() {
        acquired.remove(capability);
    }
}

fn remember_first(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

pub(crate) struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    lifecycle: Lifecycle<SystemTerminal>,
    viewport: Rect,
    restored: bool,
}

impl TerminalSession {
    pub(crate) fn check_interactive() -> io::Result<()> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::other(
                "Orifude needs an interactive terminal on standard input and output",
            ));
        }
        Ok(())
    }

    pub(crate) fn open() -> io::Result<Self> {
        Self::check_interactive()?;
        install_panic_hook();
        mark_terminal_active();
        let mut lifecycle = Lifecycle::<SystemTerminal>::default();
        if let Err(error) = lifecycle.acquire() {
            restore_opening_lifecycle(&mut lifecycle);
            return Err(error);
        }

        let (width, height) = match crossterm::terminal::size() {
            Ok(size) => size,
            Err(error) => {
                restore_opening_lifecycle(&mut lifecycle);
                return Err(error);
            }
        };
        let viewport = bounded_viewport(width, height);
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = match Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(viewport),
            },
        ) {
            Ok(terminal) => terminal,
            Err(error) => {
                restore_opening_lifecycle(&mut lifecycle);
                return Err(error);
            }
        };

        Ok(Self {
            terminal,
            lifecycle,
            viewport,
            restored: false,
        })
    }

    pub(crate) fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        let viewport = bounded_viewport(width, height);
        if viewport == self.viewport {
            return Ok(());
        }
        self.terminal.backend_mut().clear()?;
        self.terminal.resize(viewport)?;
        self.viewport = viewport;
        Ok(())
    }

    pub(crate) fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> io::Result<()> {
        self.terminal.draw(render).map(|_| ())
    }

    pub(crate) fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let result = self.lifecycle.restore();
        if result.is_ok() {
            self.restored = true;
            mark_terminal_inactive();
        }
        result
    }
}

fn bounded_viewport(width: u16, height: u16) -> Rect {
    let render_width = width.min(MAX_RENDER_WIDTH);
    let render_height = height.min(MAX_RENDER_HEIGHT);
    Rect::new(
        width.saturating_sub(render_width) / 2,
        height.saturating_sub(render_height) / 2,
        render_width,
        render_height,
    )
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.restore().is_err() && self.restore().is_err() {
            restore_process_terminal();
            mark_terminal_inactive();
        }
    }
}

fn restore_opening_lifecycle(lifecycle: &mut Lifecycle<SystemTerminal>) {
    if lifecycle.restore().is_err() && lifecycle.restore().is_err() {
        restore_process_terminal();
    }
    mark_terminal_inactive();
}

fn mark_terminal_active() {
    TERMINAL_OWNED.with(|owned| owned.set(true));
    TERMINAL_ACTIVE.store(true, Ordering::Release);
}

fn mark_terminal_inactive() {
    TERMINAL_ACTIVE.store(false, Ordering::Release);
    TERMINAL_OWNED.with(|owned| owned.set(false));
}

fn install_panic_hook() {
    INSTALL_PANIC_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |information| {
            let owned = TERMINAL_OWNED.with(Cell::get);
            if owned && TERMINAL_ACTIVE.swap(false, Ordering::AcqRel) {
                TERMINAL_OWNED.with(|terminal_owned| terminal_owned.set(false));
                restore_process_terminal();
            }
            previous(information);
        }));
    });
}

fn restore_process_terminal() {
    let mut stdout = io::stdout();
    let _focus_result = execute!(stdout, DisableFocusChange);
    let _wrap_result = execute!(stdout, EnableLineWrap);
    let _cursor_result = execute!(stdout, Show);
    let _screen_result = execute!(stdout, LeaveAlternateScreen);
    let _flush_result = stdout.flush();
    let _raw_result = disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{
        Acquired, Lifecycle, MAX_RENDER_HEIGHT, MAX_RENDER_WIDTH, TERMINAL_OWNED, TerminalControl,
        bounded_viewport,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Operation {
        EnableRaw,
        EnterAlternate,
        HideCursor,
        DisableWrap,
        EnableFocus,
        DisableFocus,
        EnableWrap,
        ShowCursor,
        LeaveAlternate,
        DisableRaw,
    }

    #[derive(Default)]
    struct FakeTerminal {
        calls: Vec<Operation>,
        fail_on: Option<Operation>,
    }

    impl FakeTerminal {
        fn call(&mut self, operation: Operation) -> io::Result<()> {
            self.calls.push(operation);
            if self.fail_on == Some(operation) {
                Err(io::Error::other("injected terminal failure"))
            } else {
                Ok(())
            }
        }
    }

    impl TerminalControl for FakeTerminal {
        fn enable_raw(&mut self) -> io::Result<()> {
            self.call(Operation::EnableRaw)
        }

        fn enter_alternate(&mut self) -> io::Result<()> {
            self.call(Operation::EnterAlternate)
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.call(Operation::HideCursor)
        }

        fn disable_wrap(&mut self) -> io::Result<()> {
            self.call(Operation::DisableWrap)
        }

        fn enable_focus(&mut self) -> io::Result<()> {
            self.call(Operation::EnableFocus)
        }

        fn disable_focus(&mut self) -> io::Result<()> {
            self.call(Operation::DisableFocus)
        }

        fn enable_wrap(&mut self) -> io::Result<()> {
            self.call(Operation::EnableWrap)
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.call(Operation::ShowCursor)
        }

        fn leave_alternate(&mut self) -> io::Result<()> {
            self.call(Operation::LeaveAlternate)
        }

        fn disable_raw(&mut self) -> io::Result<()> {
            self.call(Operation::DisableRaw)
        }
    }

    #[test]
    fn every_partial_acquisition_is_rolled_back() {
        let acquisition = [
            Operation::EnableRaw,
            Operation::EnterAlternate,
            Operation::HideCursor,
            Operation::DisableWrap,
            Operation::EnableFocus,
        ];

        for failed in acquisition {
            let mut lifecycle = Lifecycle {
                control: FakeTerminal {
                    fail_on: Some(failed),
                    ..FakeTerminal::default()
                },
                ..Lifecycle::default()
            };

            lifecycle.acquire().expect_err("failure is injected");
            let failed_at = lifecycle
                .control
                .calls
                .iter()
                .position(|operation| *operation == failed)
                .expect("failed operation is recorded");
            let expected_restorations = failed_at;
            assert_eq!(
                lifecycle.control.calls.len(),
                failed_at + 1 + expected_restorations,
                "failed at {failed:?}"
            );
            assert_eq!(lifecycle.acquired, Acquired::default());

            let expected_tail = match failed {
                Operation::EnableRaw => &[][..],
                Operation::EnterAlternate => &[Operation::DisableRaw][..],
                Operation::HideCursor => &[Operation::LeaveAlternate, Operation::DisableRaw][..],
                Operation::DisableWrap => &[
                    Operation::ShowCursor,
                    Operation::LeaveAlternate,
                    Operation::DisableRaw,
                ][..],
                Operation::EnableFocus => &[
                    Operation::EnableWrap,
                    Operation::ShowCursor,
                    Operation::LeaveAlternate,
                    Operation::DisableRaw,
                ][..],
                _ => unreachable!("only acquisition operations are injected"),
            };
            assert!(lifecycle.control.calls.ends_with(expected_tail));
        }
    }

    #[test]
    fn restoration_attempts_every_step_after_one_fails() {
        let mut lifecycle = Lifecycle::<FakeTerminal>::default();
        lifecycle.acquire().expect("acquisition succeeds");
        lifecycle.control.fail_on = Some(Operation::EnableWrap);

        lifecycle.restore().expect_err("one restoration fails");

        assert!(lifecycle.control.calls.contains(&Operation::DisableFocus));
        assert!(lifecycle.control.calls.contains(&Operation::EnableWrap));
        assert!(lifecycle.control.calls.contains(&Operation::ShowCursor));
        assert!(lifecycle.control.calls.contains(&Operation::LeaveAlternate));
        assert!(lifecycle.control.calls.contains(&Operation::DisableRaw));
    }

    #[test]
    fn failed_restoration_steps_are_retried() {
        let mut lifecycle = Lifecycle::<FakeTerminal>::default();
        lifecycle.acquire().expect("acquisition succeeds");
        lifecycle.control.fail_on = Some(Operation::EnableWrap);
        lifecycle.restore().expect_err("one restoration fails");

        lifecycle.control.fail_on = None;
        let calls_before_retry = lifecycle.control.calls.len();
        lifecycle
            .restore()
            .expect("the outstanding step is retried");

        assert_eq!(
            &lifecycle.control.calls[calls_before_retry..],
            &[Operation::EnableWrap]
        );
    }

    #[test]
    fn render_viewports_are_bounded_and_centered() {
        assert_eq!(
            bounded_viewport(80, 24),
            ratatui::layout::Rect::new(0, 0, 80, 24)
        );
        assert_eq!(
            bounded_viewport(1_000, 500),
            ratatui::layout::Rect::new(420, 220, MAX_RENDER_WIDTH, MAX_RENDER_HEIGHT)
        );
    }

    #[test]
    fn terminal_ownership_does_not_follow_the_event_worker_thread() {
        TERMINAL_OWNED.with(|owned| owned.set(true));
        let worker_owned = std::thread::spawn(|| TERMINAL_OWNED.with(std::cell::Cell::get))
            .join()
            .expect("worker joins");
        TERMINAL_OWNED.with(|owned| owned.set(false));

        assert!(!worker_owned);
    }
}
