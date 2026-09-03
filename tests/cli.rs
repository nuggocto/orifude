use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
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
    run_in_state(state.path(), arguments)
}

fn run_in_state<I, S>(state: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_orifude"));
    command
        .args(arguments)
        .env("ORIFUDE_TEST_ROOT", state)
        .env("XDG_DATA_HOME", state.join("xdg-data"))
        .env("XDG_CONFIG_HOME", state.join("xdg-config"))
        .env("XDG_CACHE_HOME", state.join("xdg-cache"))
        .env("APPDATA", state.join("appdata"))
        .env("LOCALAPPDATA", state.join("localappdata"))
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
            "Usage: orifude [OPTIONS] | verify PATH | solve PATH | pack COMMAND\n",
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
        for available_command in ["verify PATH", "solve PATH", "pack install PATH"] {
            assert!(help.contains(available_command));
        }
        for unavailable_command in ["play", "daily"] {
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
fn author_commands_validate_and_solve_the_example_pack() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("puzzles/example-pack");

    let verified = run([OsStr::new("verify"), example.as_os_str()]);
    assert!(verified.status.success());
    assert!(utf8(&verified.stdout).contains("Verified 3 puzzle(s) in pack paper-garden"));
    assert!(verified.stderr.is_empty());

    let solved = run([OsStr::new("solve"), example.as_os_str()]);
    let report = utf8(&solved.stdout);
    assert!(solved.status.success());
    assert!(report.contains("[puzzle.folded-leaves]"));
    assert!(report.contains("solution = ["));
    assert!(report.contains("{ kind = \"fold\", direction = \"left\", crease = 2 }"));
    assert!(toml::from_str::<toml::Value>(report).is_ok());
    assert!(solved.stderr.is_empty());
}

#[test]
fn malformed_pack_reports_bounded_safe_diagnostics() {
    let state = tempfile::tempdir().expect("isolated malformed pack");
    let pack = state.path().join("bad-pack");
    std::fs::create_dir_all(pack.join("puzzles")).expect("pack directory");
    std::fs::write(
        pack.join("pack.toml"),
        concat!(
            "format_version = 1\n",
            "id = \"bad-pack\"\n",
            "title = \"Bad pack\"\n",
            "authors = []\n",
            "license = \"Apache-2.0\"\n",
            "puzzles = [\"bad-paper\"]\n",
        ),
    )
    .expect("pack metadata");
    std::fs::write(pack.join("puzzles/bad-paper.toml"), b"not valid TOML\n")
        .expect("invalid puzzle");

    let output = run([OsStr::new("verify"), pack.as_os_str()]);
    let stderr = utf8(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("failed validation"));
    assert!(stderr.contains("puzzle"));
    assert!(stderr.len() < 4096);
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn rejected_archive_paths_cannot_write_terminal_controls() {
    let state = tempfile::tempdir().expect("isolated hostile archive");
    let archive = std::fs::File::create(state.path().join("hostile.zip")).expect("archive file");
    let mut writer = zip::ZipWriter::new(archive);
    writer
        .start_file(
            "notes/bad\u{1b}[31m.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .expect("hostile path is representable in ZIP metadata");
    writer.write_all(b"note").expect("archive entry");
    writer.finish().expect("finished archive");

    let output = run([
        OsStr::new("verify"),
        state.path().join("hostile.zip").as_os_str(),
    ]);
    let stderr = utf8(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("notes/bad?[31m.txt"));
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn local_pack_lifecycle_is_visible_across_processes() {
    let state = tempfile::tempdir().expect("isolated pack lifecycle");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("puzzles/example-pack");

    let installed = run_in_state(
        state.path(),
        [
            OsStr::new("pack"),
            OsStr::new("install"),
            example.as_os_str(),
        ],
    );
    assert!(installed.status.success());
    assert_eq!(utf8(&installed.stdout), "Installed pack paper-garden.\n");

    let listed = run_in_state(state.path(), ["pack", "list"]);
    assert!(listed.status.success());
    assert_eq!(utf8(&listed.stdout), "paper-garden\tPaper garden\n");

    let removed = run_in_state(state.path(), ["pack", "remove", "paper-garden"]);
    assert!(removed.status.success());
    assert_eq!(
        utf8(&removed.stdout),
        "Removed pack paper-garden. Saved progress was kept.\n"
    );

    let empty = run_in_state(state.path(), ["pack", "list"]);
    assert!(empty.status.success());
    assert_eq!(utf8(&empty.stdout), "No puzzle packs are installed.\n");
    assert!(state.path().join("data/orifude.sqlite3").is_file());
    assert!(!state.path().join("xdg-data").exists());
}

#[test]
fn storage_failures_cannot_write_terminal_controls() {
    let state = tempfile::tempdir().expect("isolated hostile database");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("puzzles/example-pack");
    let installed = run_in_state(
        state.path(),
        [
            OsStr::new("pack"),
            OsStr::new("install"),
            example.as_os_str(),
        ],
    );
    assert!(installed.status.success());

    let connection = rusqlite::Connection::open(state.path().join("data/orifude.sqlite3"))
        .expect("test database");
    connection
        .execute_batch(
            "CREATE TRIGGER hostile_remove BEFORE DELETE ON pack_registry
             BEGIN SELECT RAISE(ABORT, 'forced \u{1b}[31m failure'); END;",
        )
        .expect("hostile database trigger");
    drop(connection);

    let output = run_in_state(state.path(), ["pack", "remove", "paper-garden"]);
    let stderr = utf8(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("forced ?[31m failure"));
    assert!(!stderr.contains('\u{1b}'));
    assert!(stderr.len() <= 16 * 1024 + 1);
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
