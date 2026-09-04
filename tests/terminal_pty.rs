#![cfg(all(
    any(target_os = "linux", target_os = "macos", windows),
    feature = "isolated-test-paths"
))]

use std::fs;
use std::path::Path;
use std::time::Duration;

use orifude::generator::{CURRENT_GENERATOR_COMPATIBILITY_VERSION, CalendarDate};
use orifude::storage::{AppPaths, Storage};

#[cfg(any(unix, windows))]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const ENTER_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049h";
const LEAVE_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
#[cfg(unix)]
const ENABLE_LINE_WRAP: &[u8] = b"\x1b[?7h";

#[test]
fn shipped_binary_restores_the_terminal_after_normal_exit() {
    let state = tempfile::tempdir().expect("isolated terminal state");
    assert_terminal_restored(Path::new(env!("CARGO_BIN_EXE_orifude")), state.path());
}

#[test]
fn new_player_learns_solves_restarts_and_replays_in_the_shipped_binary() {
    let state = tempfile::tempdir().expect("isolated player state");
    let binary = Path::new(env!("CARGO_BIN_EXE_orifude"));
    let replay_steps = [
        PtyStep {
            input: b"\r\rjll\r\r\r\r\r\rjl\r\rv",
            wait_for: b"fresh paper",
        },
        PtyStep {
            input: b"?",
            wait_for: b"comparison",
        },
        PtyStep {
            input: b"?\r",
            wait_for: b"row 2, column 2",
        },
        PtyStep {
            input: b"\r",
            wait_for: b"exactly.",
        },
    ];
    let first = run_in_native_pty_scripted(binary, state.path(), &replay_steps, b"\x1bqy");
    assert!(first.status_success, "first player journey exits cleanly");
    assert!(
        find(&first.bytes, b"fresh paper").is_some()
            && find(&first.bytes, b"row 2, column 2").is_some()
            && find(&first.bytes, b"exactly.").is_some(),
        "the saved paper is replayed from fresh paper through its final comparison"
    );

    let paths = AppPaths::injected(
        state.path().join("data"),
        state.path().join("config"),
        state.path().join("cache"),
    );
    {
        let storage = Storage::open(paths.clone()).expect("saved player state opens");
        assert!(storage.settings().expect("settings").lesson_complete);
        let progress = storage
            .progress("orifude-journey", "first-drop")
            .expect("progress read")
            .expect("journey completion is durable");
        assert_eq!(progress.attempt_count, 1);
        let replay = storage
            .best_replay("orifude-journey", "first-drop")
            .expect("replay read")
            .expect("best replay is durable");
        assert!(
            replay
                .replay()
                .execute(replay.puzzle())
                .expect("stored replay executes")
                .result()
                .is_success()
        );
    }

    let returning_steps = [
        PtyStep {
            input: b"jjjj\r\r",
            wait_for: b"fresh",
        },
        PtyStep {
            input: b"\r",
            wait_for: b"row 2",
        },
        PtyStep {
            input: b"\r",
            wait_for: b"exactly.",
        },
        PtyStep {
            input: b"x",
            wait_for: b"included.",
        },
    ];
    let second = run_in_native_pty_scripted(binary, state.path(), &returning_steps, b"\r\x1bqy");
    assert!(
        second.status_success,
        "returning player journey exits cleanly"
    );
    assert!(
        find(&second.bytes, b"included.").is_some(),
        "the returning player can revisit, replay, and export the saved paper"
    );
}

#[test]
fn revised_journey_completion_survives_a_real_player_restart() {
    use orifude::domain::paper::{BrushRule, Dimensions};
    use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
    use orifude::domain::replay::Replay;

    let state = tempfile::tempdir().expect("isolated revised player state");
    let paths = configured_returning_player(state.path());
    let dimensions = Dimensions::new(4, 4).unwrap();
    let old_position = dimensions.coordinate(0, 0).unwrap();
    let obsolete = Puzzle::new(
        PuzzleSpec::new(
            PuzzleIdentity::new("orifude-journey", "first-drop").unwrap(),
            4,
            4,
        )
        .with_target_cells(vec![dimensions.cell_id(old_position).unwrap()])
        .with_allowed_brushes(vec![BrushRule::Dot])
        .with_budgets(0, 1),
    )
    .unwrap();
    let mut old_attempt = obsolete.start();
    old_attempt.stamp_dot(old_position).unwrap();
    Storage::open(paths.clone())
        .unwrap()
        .record_completion(&obsolete, &Replay::from_attempt(&old_attempt), 1, 0, false)
        .unwrap();

    let binary = Path::new(env!("CARGO_BIN_EXE_orifude"));
    let first = run_in_native_pty_scripted(
        binary,
        state.path(),
        &[PtyStep {
            input: b"\r\rjl\r\r",
            wait_for: b"Congratulations",
        }],
        b"qy",
    );
    assert!(first.status_success);
    let journey = orifude::packs::validate_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("puzzles/journey"),
    )
    .unwrap();
    let current = journey
        .puzzles()
        .iter()
        .find(|paper| paper.puzzle().identity().puzzle_id() == "first-drop")
        .unwrap();
    assert!(
        Storage::open(paths)
            .unwrap()
            .completion_matches(current.puzzle())
            .unwrap()
    );

    let second = run_in_native_pty_scripted(
        binary,
        state.path(),
        &[
            PtyStep {
                input: b"jjjj\r\r",
                wait_for: b"fresh",
            },
            PtyStep {
                input: b"\r",
                wait_for: b"row 2",
            },
        ],
        b"\x1bqy",
    );
    assert!(
        second.status_success,
        "restart replays the current revision"
    );
}

#[test]
fn shipped_player_exercises_preview_undo_reset_and_saved_result_navigation() {
    let state = tempfile::tempdir().expect("isolated player state");
    let paths = configured_returning_player(state.path());
    let steps = [
        PtyStep {
            input: b"\r\r ",
            wait_for: b"UNFOL",
        },
        PtyStep {
            input: b"jlb\rub\rr",
            wait_for: b"Smooth",
        },
    ];
    let output = run_in_native_pty_scripted(
        Path::new(env!("CARGO_BIN_EXE_orifude")),
        state.path(),
        &steps,
        b"nryb\r\r\rqy",
    );

    assert!(
        output.status_success,
        "complete control journey exits cleanly"
    );
    assert!(
        find(&output.bytes, b"UNFOL").is_some() && find(&output.bytes, b"REVIEW").is_some(),
        "Space opens the documented preview"
    );
    assert!(
        find(&output.bytes, b"Smooth").is_some(),
        "reset confirmation is shown"
    );
    let progress = Storage::open(paths)
        .expect("saved player state opens")
        .progress("orifude-journey", "first-drop")
        .expect("progress read")
        .expect("journey completion is durable");
    assert_eq!(progress.attempt_count, 1);
}

#[test]
fn injected_daily_paper_is_stable_in_the_shipped_binary() {
    let binary = Path::new(env!("CARGO_BIN_EXE_orifude"));
    let date = CalendarDate::new(2026, 9, 2).expect("test date");
    let mut identities = Vec::new();
    for _ in 0..2 {
        let state = tempfile::tempdir().expect("isolated daily state");
        let paths = configured_returning_player(state.path());
        let steps = [PtyStep {
            input: b"j\r",
            wait_for: b"FOLDED",
        }];
        let output = run_in_native_pty_scripted(binary, state.path(), &steps, b"qy");
        assert!(output.status_success, "daily journey exits cleanly");
        let history = Storage::open(paths)
            .expect("daily state opens")
            .daily_history(date, CURRENT_GENERATOR_COMPATIBILITY_VERSION)
            .expect("daily history read")
            .expect("daily selection was recorded");
        identities.push((history.pack_id, history.puzzle_id));
    }

    assert_eq!(identities[0], identities[1]);
}

#[test]
fn malformed_installed_pack_is_reported_without_terminal_corruption() {
    let state = tempfile::tempdir().expect("isolated pack state");
    let paths = AppPaths::injected(
        state.path().join("data"),
        state.path().join("config"),
        state.path().join("cache"),
    );
    let source = state.path().join("source-pack");
    write_test_pack(&source);
    let mut storage = Storage::open(paths.clone()).expect("pack storage opens");
    let mut settings = storage.settings().expect("settings load");
    settings.lesson_complete = true;
    settings.instant_reveal = true;
    storage.save_settings(settings).expect("settings save");
    storage
        .install_pack(&source, 1)
        .expect("valid pack installs");
    drop(storage);
    let managed = fs::read_dir(paths.managed_packs())
        .expect("managed pack directory opens")
        .find_map(|entry| {
            let entry = entry.ok()?;
            entry.file_type().ok()?.is_dir().then_some(entry.path())
        })
        .expect("installed pack directory");
    fs::write(managed.join("puzzles/berry.toml"), "not valid TOML = [")
        .expect("managed pack is corrupted for the boundary test");

    let steps = [
        PtyStep {
            input: b"jjj\r",
            wait_for: b"Terminal pack",
        },
        PtyStep {
            input: b"\r",
            wait_for: b"fingerprint",
        },
    ];
    let output = run_in_native_pty_scripted(
        Path::new(env!("CARGO_BIN_EXE_orifude")),
        state.path(),
        &steps,
        b"\rqy",
    );
    assert!(
        output.status_success,
        "malformed-pack journey exits cleanly"
    );
    assert!(
        find(&output.bytes, b"fingerprint").is_some(),
        "pack validation failure is visible inside the restored TUI"
    );
    assert!(find(&output.bytes, LEAVE_ALTERNATE_SCREEN).is_some());
}

#[test]
fn shipped_binary_recovers_after_starting_below_the_minimum_size() {
    let state = tempfile::tempdir().expect("isolated resize state");
    let output = run_in_native_pty_resize(Path::new(env!("CARGO_BIN_EXE_orifude")), state.path());

    assert!(output.status_success, "resized terminal exits cleanly");
    assert!(
        find(&output.bytes, b"Resize").is_some(),
        "the stable resize message is shown"
    );
    assert!(
        find(&output.bytes, b"Match").is_some(),
        "interactive content returns after resize"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn shipped_binary_path_with_spaces_restores_the_terminal() {
    use std::os::unix::fs::symlink;

    let state = tempfile::tempdir().expect("isolated terminal state");
    let linked_binary = state.path().join("orifude terminal smoke");
    symlink(env!("CARGO_BIN_EXE_orifude"), &linked_binary).expect("test binary link");

    assert_terminal_restored(&linked_binary, state.path());
}

fn assert_terminal_restored(binary: &Path, state: &Path) {
    let output = run_in_native_pty(binary, state, b"qy");

    assert!(output.status_success, "terminal child did not exit cleanly");
    let enter = find(&output.bytes, ENTER_ALTERNATE_SCREEN).expect("alternate screen is entered");
    let leave = find(&output.bytes, LEAVE_ALTERNATE_SCREEN).expect("alternate screen is left");
    assert!(enter < leave, "terminal restoration follows acquisition");
    assert!(
        find(&output.bytes, SHOW_CURSOR).is_some(),
        "cursor is shown"
    );
    #[cfg(unix)]
    assert!(
        find(&output.bytes, ENABLE_LINE_WRAP).is_some(),
        "line wrapping is restored"
    );
    assert!(
        state.join("data/orifude.sqlite3").is_file(),
        "the terminal child keeps its database inside the test directory"
    );
}

fn configured_returning_player(state: &Path) -> AppPaths {
    let paths = AppPaths::injected(
        state.join("data"),
        state.join("config"),
        state.join("cache"),
    );
    let mut storage = Storage::open(paths.clone()).expect("player state opens");
    let mut settings = storage.settings().expect("settings load");
    settings.lesson_complete = true;
    settings.instant_reveal = true;
    storage.save_settings(settings).expect("settings save");
    drop(storage);
    paths
}

fn write_test_pack(root: &Path) {
    fs::create_dir_all(root.join("puzzles")).expect("pack directories");
    fs::write(
        root.join("pack.toml"),
        "format_version = 1\nid = \"terminal-pack\"\ntitle = \"Terminal pack\"\nauthors = [\"Ada\"]\nlicense = \"Apache-2.0\"\npuzzles = [\"berry\"]\n",
    )
    .expect("pack metadata");
    fs::write(
        root.join("puzzles/berry.toml"),
        "format_version = 1\nid = \"berry\"\ntitle = \"Berry\"\nwidth = 4\nheight = 4\ntarget = [\"#...\", \"....\", \"....\", \"....\"]\nfolds = []\nbrushes = [{ kind = \"dot\" }]\nfold_budget = 0\nstroke_budget = 1\n",
    )
    .expect("pack puzzle");
}

struct PtyOutput {
    status_success: bool,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct PtyStep<'a> {
    input: &'a [u8],
    wait_for: &'a [u8],
}

#[derive(Clone, Copy)]
enum PtyPlan<'a> {
    Immediate(&'a [u8]),
    Scripted {
        steps: &'a [PtyStep<'a>],
        final_input: &'a [u8],
    },
    Resize,
}

struct OutputLog {
    bytes: std::sync::Mutex<Vec<u8>>,
    changed: std::sync::Condvar,
}

impl OutputLog {
    fn new() -> Self {
        Self {
            bytes: std::sync::Mutex::new(Vec::new()),
            changed: std::sync::Condvar::new(),
        }
    }

    fn record(&self, bytes: &[u8]) {
        self.bytes.lock().expect("output log lock").extend(bytes);
        self.changed.notify_all();
    }

    fn wait_for(&self, needle: &[u8]) -> Result<(), String> {
        self.wait_for_after(needle, 0)
    }

    fn wait_for_after(&self, needle: &[u8], after: usize) -> Result<(), String> {
        let started = std::time::Instant::now();
        let mut bytes = self.bytes.lock().map_err(|_| "output log failed")?;
        loop {
            let search_start = after
                .saturating_sub(needle.len().saturating_sub(1))
                .min(bytes.len());
            let observed = find(&bytes[search_start..], needle)
                .is_some_and(|position| search_start + position + needle.len() > after);
            if observed {
                return Ok(());
            }
            let Some(remaining) = Duration::from_secs(5).checked_sub(started.elapsed()) else {
                return Err(format!(
                    "terminal output did not contain {:?}; output: {}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&bytes)
                ));
            };
            let (next, timeout) = self
                .changed
                .wait_timeout(bytes, remaining)
                .map_err(|_| "output log failed")?;
            bytes = next;
            let observed = find(&bytes[search_start..], needle)
                .is_some_and(|position| search_start + position + needle.len() > after);
            if timeout.timed_out() && !observed {
                return Err(format!(
                    "terminal output did not contain {:?}; output: {}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&bytes)
                ));
            }
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("output log lock").clone()
    }

    fn len(&self) -> usize {
        self.bytes.lock().expect("output log lock").len()
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
}

#[cfg(unix)]
fn run_in_native_pty(binary: &Path, state: &Path, input: &[u8]) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Immediate(input))
}

#[cfg(unix)]
fn run_in_native_pty_scripted(
    binary: &Path,
    state: &Path,
    steps: &[PtyStep<'_>],
    final_input: &[u8],
) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Scripted { steps, final_input })
}

#[cfg(unix)]
fn run_in_native_pty_resize(binary: &Path, state: &Path) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Resize)
}

#[cfg(unix)]
fn run_in_native_pty_with_plan(binary: &Path, state: &Path, plan: PtyPlan<'_>) -> PtyOutput {
    use std::io::Read;
    use std::sync::Arc;
    use std::thread;

    let mut command = unix_pty_command(binary, state, matches!(plan, PtyPlan::Resize));
    let mut child = command.spawn().expect("native script PTY starts");
    let mut stdout = child.stdout.take().expect("PTY output");
    let mut stderr = child.stderr.take().expect("PTY errors");
    let output = Arc::new(OutputLog::new());
    let stdout_log = Arc::clone(&output);
    let output_reader = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stdout.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            stdout_log.record(&buffer[..read]);
        }
        Ok::<_, std::io::Error>(())
    });
    let stderr_log = Arc::clone(&output);
    let error_reader = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stderr.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            stderr_log.record(&buffer[..read]);
        }
        Ok::<_, std::io::Error>(())
    });
    if let Err(error) = output.wait_for(ENTER_ALTERNATE_SCREEN) {
        let _kill_result = child.kill();
        let _wait_result = child.wait();
        output_reader
            .join()
            .expect("PTY output reader joins")
            .expect("PTY output is readable");
        error_reader
            .join()
            .expect("PTY error reader joins")
            .expect("PTY errors are readable");
        panic!("terminal child did not enter its alternate screen: {error}");
    }
    let mut stdin = child.stdin.take().expect("PTY input");
    let drive_result = drive_plan(&mut stdin, output.as_ref(), plan, || {
        output.wait_for(b"Resize")?;
        fs::write(state.join("resize-now"), b"resize")
            .map_err(|error| format!("resize signal failed: {error}"))?;
        output.wait_for(b"Match")
    });
    if let Err(error) = drive_result {
        let _kill_result = child.kill();
        let _wait_result = child.wait();
        drop(stdin);
        output_reader
            .join()
            .expect("PTY output reader joins")
            .expect("PTY output is readable");
        error_reader
            .join()
            .expect("PTY error reader joins")
            .expect("PTY errors are readable");
        panic!("{error}");
    }
    drop(stdin);
    let completion = wait_for_unix_child(&mut child);
    output_reader
        .join()
        .expect("PTY output reader joins")
        .expect("PTY output is readable");
    error_reader
        .join()
        .expect("PTY error reader joins")
        .expect("PTY errors are readable");
    let bytes = output.snapshot();
    let status = completion.unwrap_or_else(|error| {
        panic!(
            "{error}; terminal output: {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    PtyOutput {
        status_success: status.success(),
        bytes,
    }
}

#[cfg(unix)]
fn wait_for_unix_child(
    child: &mut std::process::Child,
) -> Result<std::process::ExitStatus, String> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < PROCESS_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let _kill_result = child.kill();
                let _wait_result = child.wait();
                return Err(format!("terminal child exceeded {PROCESS_TIMEOUT:?}"));
            }
            Err(error) => {
                let _kill_result = child.kill();
                let _wait_result = child.wait();
                return Err(format!("terminal child status failed: {error}"));
            }
        }
    }
}

#[cfg(unix)]
fn unix_pty_command(binary: &Path, state: &Path, start_small: bool) -> std::process::Command {
    use std::process::{Command, Stdio};

    let launch = if start_small {
        "stty cols 59 rows 19; (attempt=0; while [ ! -e \"$ORIFUDE_RESIZE_SIGNAL\" ] && [ \"$attempt\" -lt 500 ]; do sleep 0.01; attempt=$((attempt + 1)); done; if [ -e \"$ORIFUDE_RESIZE_SIGNAL\" ]; then stty cols 100 rows 30 </dev/tty; fi) & exec \"$ORIFUDE_SMOKE_BINARY\""
    } else {
        "stty cols 100 rows 30; exec \"$ORIFUDE_SMOKE_BINARY\""
    };
    let mut command = Command::new("script");
    #[cfg(target_os = "linux")]
    command.args(["--quiet", "--return", "--command"]);
    #[cfg(target_os = "linux")]
    command
        .arg(launch)
        .arg("/dev/null")
        .env("ORIFUDE_SMOKE_BINARY", binary);
    #[cfg(target_os = "macos")]
    command
        .arg("-q")
        .arg("/dev/null")
        .arg("/bin/sh")
        .arg("-c")
        .arg(launch)
        .env("ORIFUDE_SMOKE_BINARY", binary);
    command
        .env("TERM", "xterm-256color")
        .env("ORIFUDE_TEST_ROOT", state)
        .env("ORIFUDE_TEST_DATE", "2026-09-02")
        .env("ORIFUDE_TEST_UNIX_SECONDS", "1788307200")
        .env("ORIFUDE_RESIZE_SIGNAL", state.join("resize-now"))
        .env("SHELL", "/bin/sh")
        .env("XDG_DATA_HOME", state.join("data"))
        .env("XDG_CONFIG_HOME", state.join("config"))
        .env("XDG_CACHE_HOME", state.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[cfg(windows)]
fn run_in_native_pty(binary: &Path, state: &Path, input_bytes: &[u8]) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Immediate(input_bytes))
}

#[cfg(windows)]
fn run_in_native_pty_scripted(
    binary: &Path,
    state: &Path,
    steps: &[PtyStep<'_>],
    final_input: &[u8],
) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Scripted { steps, final_input })
}

#[cfg(windows)]
fn run_in_native_pty_resize(binary: &Path, state: &Path) -> PtyOutput {
    run_in_native_pty_with_plan(binary, state, PtyPlan::Resize)
}

#[cfg(windows)]
fn run_in_native_pty_with_plan(binary: &Path, state: &Path, plan: PtyPlan<'_>) -> PtyOutput {
    use std::io::Read;
    use std::sync::Arc;
    use std::thread;

    use conpty_oxide::blocking::Command;
    use conpty_oxide::{SessionOptions, Size};

    let mut command = Command::new(binary);
    command
        .env("TERM", "xterm-256color")
        .env("ORIFUDE_TEST_ROOT", state)
        .env("ORIFUDE_TEST_DATE", "2026-09-02")
        .env("ORIFUDE_TEST_UNIX_SECONDS", "1788307200")
        .env("APPDATA", state.join("config"))
        .env("LOCALAPPDATA", state.join("data"));
    let size = if matches!(plan, PtyPlan::Resize) {
        Size::try_new(59, 19)
    } else {
        Size::try_new(80, 24)
    }
    .expect("valid PTY size");
    let options = SessionOptions::new().size(size);
    let parts = command
        .spawn_with(options)
        .expect("native ConPTY starts")
        .into_parts();
    let mut child = parts.child;
    let mut input = parts.input;
    let mut output = parts.output;
    let controller = parts.controller;
    let log = Arc::new(OutputLog::new());
    let reader_log = Arc::clone(&log);
    let reader = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            let read = output.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            reader_log.record(&buffer[..read]);
        }
        Ok::<_, std::io::Error>(())
    });
    if let Err(error) = log.wait_for(ENTER_ALTERNATE_SCREEN) {
        let _kill_result = child.kill();
        drop(input);
        drop(controller);
        reader
            .join()
            .expect("PTY reader joins")
            .expect("PTY output is readable");
        panic!("terminal child did not enter its alternate screen: {error}");
    }
    let drive_result = drive_plan(&mut input, log.as_ref(), plan, || {
        log.wait_for(b"Resize")?;
        controller
            .resize(Size::try_new(100, 30).expect("valid resized PTY"))
            .map_err(|error| format!("PTY resize failed: {error}"))?;
        log.wait_for(b"Match")
    });
    if let Err(error) = drive_result {
        let _kill_result = child.kill();
        drop(input);
        drop(controller);
        reader
            .join()
            .expect("PTY reader joins")
            .expect("PTY output is readable");
        panic!("{error}");
    }
    let started = std::time::Instant::now();
    let completion = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < PROCESS_TIMEOUT => {
                thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let result = child
                    .kill()
                    .map_err(|error| format!("timed-out terminal child could not stop: {error}"));
                break result.and(Err(format!("terminal child exceeded {PROCESS_TIMEOUT:?}")));
            }
            Err(error) => {
                let _kill_result = child.kill();
                break Err(format!("terminal child status failed: {error}"));
            }
        }
    };
    drop(input);
    drop(controller);
    reader
        .join()
        .expect("PTY reader joins")
        .expect("PTY output is readable");
    let bytes = log.snapshot();
    let status = completion.unwrap_or_else(|error| panic!("{error}"));

    PtyOutput {
        status_success: status.success(),
        bytes,
    }
}

fn write_input(writer: &mut dyn std::io::Write, bytes: &[u8]) -> Result<(), String> {
    writer
        .write_all(bytes)
        .and_then(|()| writer.flush())
        .map_err(|error| format!("PTY input failed: {error}"))
}

fn write_scripted_input(
    writer: &mut dyn std::io::Write,
    output: &OutputLog,
    steps: &[PtyStep<'_>],
    final_input: &[u8],
) -> Result<(), String> {
    for step in steps {
        let after = output.len();
        write_input(writer, step.input)?;
        output.wait_for_after(step.wait_for, after)?;
    }
    write_input(writer, final_input)
}

fn drive_plan(
    writer: &mut dyn std::io::Write,
    output: &OutputLog,
    plan: PtyPlan<'_>,
    resize: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match plan {
        PtyPlan::Immediate(input) => write_input(writer, input),
        PtyPlan::Scripted { steps, final_input } => {
            write_scripted_input(writer, output, steps, final_input)
        }
        PtyPlan::Resize => {
            resize()?;
            write_input(writer, b"qy")
        }
    }
}
