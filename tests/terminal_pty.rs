#![cfg(all(
    any(target_os = "linux", target_os = "macos", windows),
    feature = "isolated-test-paths"
))]

use std::path::Path;
use std::time::Duration;

#[cfg(any(unix, windows))]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const ENTER_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049h";
const LEAVE_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
const ENABLE_LINE_WRAP: &[u8] = b"\x1b[?7h";

#[test]
fn shipped_binary_restores_the_terminal_after_normal_exit() {
    let state = tempfile::tempdir().expect("isolated terminal state");
    assert_terminal_restored(Path::new(env!("CARGO_BIN_EXE_orifude")), state.path());
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
    let output = run_in_native_pty(binary, state);

    assert!(output.status_success, "terminal child did not exit cleanly");
    let enter = find(&output.bytes, ENTER_ALTERNATE_SCREEN).expect("alternate screen is entered");
    let leave = find(&output.bytes, LEAVE_ALTERNATE_SCREEN).expect("alternate screen is left");
    assert!(enter < leave, "terminal restoration follows acquisition");
    assert!(
        find(&output.bytes, SHOW_CURSOR).is_some(),
        "cursor is shown"
    );
    assert!(
        find(&output.bytes, ENABLE_LINE_WRAP).is_some(),
        "line wrapping is restored"
    );
    assert!(
        state.join("data/orifude.sqlite3").is_file(),
        "the terminal child keeps its database inside the test directory"
    );
}

struct PtyOutput {
    status_success: bool,
    bytes: Vec<u8>,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
}

#[cfg(unix)]
fn run_in_native_pty(binary: &Path, state: &Path) -> PtyOutput {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Instant;

    let mut command = Command::new("script");
    #[cfg(target_os = "linux")]
    command.args(["--quiet", "--return", "--command"]);
    #[cfg(target_os = "linux")]
    command
        .arg("exec \"$ORIFUDE_SMOKE_BINARY\"")
        .arg("/dev/null")
        .env("ORIFUDE_SMOKE_BINARY", binary);
    #[cfg(target_os = "macos")]
    command.arg("-q").arg("/dev/null").arg(binary);
    command
        .env("TERM", "xterm-256color")
        .env("ORIFUDE_TEST_ROOT", state)
        .env("XDG_DATA_HOME", state.join("data"))
        .env("XDG_CONFIG_HOME", state.join("config"))
        .env("XDG_CACHE_HOME", state.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("native script PTY starts");
    child
        .stdin
        .take()
        .expect("PTY input")
        .write_all(b"qy")
        .expect("quit input is delivered");
    let started = Instant::now();
    let failure = loop {
        match child.try_wait() {
            Ok(Some(_)) => break None,
            Ok(None) if started.elapsed() < PROCESS_TIMEOUT => {
                thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let _kill_result = child.kill();
                break Some(format!("terminal child exceeded {PROCESS_TIMEOUT:?}"));
            }
            Err(error) => {
                let _kill_result = child.kill();
                break Some(format!("terminal child status failed: {error}"));
            }
        }
    };
    let output = child.wait_with_output().expect("PTY output is readable");
    if let Some(error) = failure {
        panic!(
            "{error}; terminal output: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    PtyOutput {
        status_success: output.status.success(),
        bytes,
    }
}

#[cfg(windows)]
fn run_in_native_pty(binary: &Path, state: &Path) -> PtyOutput {
    use std::io::{Read, Write};
    use std::thread;

    use conpty_oxide::blocking::Command;
    use conpty_oxide::{SessionOptions, Size};

    let mut command = Command::new(binary);
    command
        .env("TERM", "xterm-256color")
        .env("ORIFUDE_TEST_ROOT", state)
        .env("APPDATA", state.join("config"))
        .env("LOCALAPPDATA", state.join("data"));
    let options = SessionOptions::new().size(Size::try_new(80, 24).expect("valid PTY size"));
    let parts = command
        .spawn_with(options)
        .expect("native ConPTY starts")
        .into_parts();
    let mut child = parts.child;
    let mut input = parts.input;
    let mut output = parts.output;
    let controller = parts.controller;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).map(|_| bytes)
    });
    input.write_all(b"qy").expect("quit input is delivered");
    input.flush().expect("quit input is flushed");
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
    let bytes = reader
        .join()
        .expect("PTY reader joins")
        .expect("PTY output is readable");
    let status = completion.unwrap_or_else(|error| panic!("{error}"));

    PtyOutput {
        status_success: status.success(),
        bytes,
    }
}
