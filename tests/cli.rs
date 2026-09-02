use std::ffi::OsStr;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(5);

fn run<I, S>(arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let state = tempfile::tempdir().expect("isolated command state");
    let mut command = Command::new(env!("CARGO_BIN_EXE_orifude"));
    command
        .args(arguments)
        .env("XDG_DATA_HOME", state.path().join("data"))
        .env("XDG_CONFIG_HOME", state.path().join("config"))
        .env("XDG_CACHE_HOME", state.path().join("cache"))
        .env("APPDATA", state.path().join("config"))
        .env("LOCALAPPDATA", state.path().join("data"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the Orifude binary should start");
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .expect("the Orifude output should be readable");
            }
            Ok(None) if started.elapsed() < PROCESS_TIMEOUT => {
                thread::sleep(PROCESS_POLL_INTERVAL);
            }
            Ok(None) => {
                let _kill_result = child.kill();
                let output = child
                    .wait_with_output()
                    .expect("a timed-out Orifude process should stop");
                panic!(
                    "the Orifude process exceeded {PROCESS_TIMEOUT:?}; stderr: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(error) => {
                let _kill_result = child.kill();
                let _wait_result = child.wait();
                panic!("the Orifude process status should be readable: {error}");
            }
        }
    }
}

fn utf8(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("command output should be UTF-8")
}

fn assert_usage_error(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        utf8(&output.stderr),
        concat!(
            "error: unsupported command-line arguments\n\n",
            "Usage: orifude [OPTIONS]\n",
            "For more information, try '--help'.\n",
        )
    );
}

#[test]
fn starting_without_an_interactive_terminal_fails_without_control_sequences() {
    let output = run(&[] as &[&str]);
    let stderr = utf8(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("needs an interactive terminal"));
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn help_only_advertises_available_behavior() {
    for argument in ["-h", "--help"] {
        let output = run([argument]);
        let help = utf8(&output.stdout);

        assert!(output.status.success());
        assert!(help.contains("Usage: orifude [OPTIONS]"));
        assert!(help.contains("-h, --help"));
        assert!(help.contains("-V, --version"));
        for unavailable_command in ["play", "daily", "pack"] {
            assert!(!help.contains(unavailable_command));
        }
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn version_comes_from_the_package_metadata() {
    for argument in ["-V", "--version"] {
        let output = run([argument]);

        assert!(output.status.success());
        assert_eq!(
            utf8(&output.stdout),
            format!("orifude {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn unsupported_arguments_have_a_stable_usage_status() {
    for arguments in [&[""][..], &["play"][..], &["--help", "extra"][..]] {
        let output = run(arguments);

        assert_usage_error(&output);
    }
}

#[test]
fn unsupported_arguments_are_not_reflected_to_the_terminal() {
    let output = run(["--bad\u{1b}[31m"]);
    let stderr = utf8(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr.contains('\u{1b}'));
    assert!(!stderr.contains("--bad"));
}

#[cfg(unix)]
#[test]
fn non_utf8_arguments_are_rejected_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let argument = OsString::from_vec(vec![b'-', b'-', 0xff]);
    let output = run([argument]);

    assert_usage_error(&output);
}

#[cfg(windows)]
#[test]
fn non_utf8_arguments_are_rejected_without_reflection() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let argument = OsString::from_wide(&[u16::from(b'-'), u16::from(b'-'), 0xd800]);
    let output = run([argument]);

    assert_usage_error(&output);
}
