#[cfg(target_os = "linux")]
mod linux {
    use std::error::Error;
    use std::ffi::OsStr;
    use std::fs;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use orifude::domain::puzzle::Puzzle;
    use orifude::domain::replay::Replay;
    use orifude::storage::{AppPaths, Storage};

    const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
    const JOURNEY_PACK: &str = "orifude-journey";
    const JOURNEY_PAPER: &str = "first-drop";

    /// One terminal input and the text it must produce.
    struct Step {
        input: &'static [u8],
        wait_for: &'static [u8],
    }

    /// A new player finishes the lesson and first paper, then replays the
    /// saved completion to its final comparison.
    const FIRST_COMPLETION: &[Step] = &[
        Step {
            input: b"\r\rjll\r\r\r\r\r\rjl\r\rv",
            wait_for: b"fresh paper",
        },
        Step {
            input: b"\r",
            wait_for: b"row 2, column 2",
        },
        Step {
            input: b"\r",
            wait_for: b"exactly.",
        },
    ];

    /// A returning player opens the saved completion from keepsakes and
    /// replays it to its final comparison.
    const SAVED_REPLAY: &[Step] = &[
        Step {
            input: b"jjjj\r\r",
            wait_for: b"fresh",
        },
        Step {
            input: b"\r",
            wait_for: b"row 2",
        },
        Step {
            input: b"\r",
            wait_for: b"exactly.",
        },
    ];

    struct TestRoot(Option<PathBuf>);

    impl TestRoot {
        fn new() -> Result<Self, Box<dyn Error>> {
            let path = std::env::current_dir()?
                .join("target")
                .join(format!("orifude-release-lifecycle-{}", std::process::id()));
            if path.exists() {
                return Err("the private lifecycle root already exists".into());
            }
            fs::create_dir(&path)?;
            Ok(Self(Some(path)))
        }

        fn path(&self) -> &Path {
            self.0
                .as_deref()
                .expect("the lifecycle root is available until explicit cleanup")
        }

        fn app_paths(&self) -> AppPaths {
            AppPaths::injected(
                self.path().join("xdg-data/orifude"),
                self.path().join("xdg-config/orifude"),
                self.path().join("xdg-cache/orifude"),
            )
        }

        fn cleanup(mut self) -> Result<(), std::io::Error> {
            let path = self
                .0
                .take()
                .expect("the lifecycle root is cleaned exactly once");
            match fs::remove_dir_all(&path) {
                Ok(()) => Ok(()),
                Err(error) => {
                    self.0 = Some(path);
                    Err(error)
                }
            }
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            if let Some(path) = self.0.as_deref() {
                let _cleanup = fs::remove_dir_all(path);
            }
        }
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut arguments = std::env::args_os().skip(1);
        let baseline = absolute_binary(arguments.next(), "baseline")?;
        let candidate = absolute_binary(arguments.next(), "candidate")?;
        if arguments.next().is_some() {
            return Err("expected exactly two binary paths".into());
        }

        let root = TestRoot::new()?;
        let installed = root.path().join("bin/orifude");
        let example_pack = std::env::current_dir()?.join("puzzles/example-pack");

        install_binary(&baseline, &installed)?;
        expect_success(
            &run_binary(&installed, root.path(), ["--version"])?,
            "baseline startup",
        )?;
        expect_stdout(
            run_binary(
                &installed,
                root.path(),
                [
                    OsStr::new("pack"),
                    OsStr::new("install"),
                    example_pack.as_os_str(),
                ],
            )?,
            "Installed pack paper-garden.\n",
            "baseline install",
        )?;
        expect_pack_list(&installed, root.path())?;
        record_pack_completion(&root.app_paths(), &example_pack)?;
        play(&installed, root.path(), FIRST_COMPLETION, "baseline save")?;
        let saved = [
            read_saved(&root.app_paths(), "paper-garden", "first-seed")?,
            read_saved(&root.app_paths(), JOURNEY_PACK, JOURNEY_PAPER)?,
        ];

        install_binary(&candidate, &installed)?;
        expect_success(
            &run_binary(&installed, root.path(), ["--version"])?,
            "candidate startup",
        )?;
        expect_pack_list(&installed, root.path())?;
        play(&installed, root.path(), SAVED_REPLAY, "candidate replay")?;
        expect_saved(&root.app_paths(), &saved)?;

        install_binary(&baseline, &installed)?;
        expect_pack_list(&installed, root.path())?;
        play(&installed, root.path(), SAVED_REPLAY, "rollback replay")?;
        expect_saved(&root.app_paths(), &saved)?;

        fs::remove_file(&installed)?;
        expect_saved(&root.app_paths(), &saved)?;

        install_binary(&candidate, &installed)?;
        expect_stdout(
            run_binary(&installed, root.path(), ["pack", "remove", "paper-garden"])?,
            "Removed pack paper-garden. Saved progress was kept.\n",
            "candidate pack removal",
        )?;
        expect_saved(&root.app_paths(), &saved)?;
        let empty = run_binary(&installed, root.path(), ["pack", "list"])?;
        expect_stdout(
            empty,
            "No puzzle packs are installed.\n",
            "post-removal list",
        )?;

        fs::remove_file(&installed)?;
        if !root.app_paths().database().is_file() {
            return Err("removing the binary removed the saved database".into());
        }
        expect_saved(&root.app_paths(), &saved)?;

        root.cleanup()?;

        println!("direct_binary_lifecycle=pass");
        println!("saved_replay=played_by_each_binary");
        println!("saved_progress=preserved");
        println!("cleanup=pass");
        Ok(())
    }

    fn absolute_binary(
        value: Option<std::ffi::OsString>,
        name: &str,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let value = value.ok_or_else(|| format!("missing {name} binary path"))?;
        let path = PathBuf::from(value);
        if !path.is_absolute() || !path.is_file() {
            return Err(format!("{name} binary must be an absolute regular file").into());
        }
        Ok(path)
    }

    fn install_binary(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
        let parent = destination
            .parent()
            .ok_or("the install path has no parent directory")?;
        fs::create_dir_all(parent)?;
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        fs::copy(source, destination)?;
        Ok(())
    }

    fn run_binary<I, S>(binary: &Path, root: &Path, arguments: I) -> Result<Output, Box<dyn Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut child = Command::new(binary)
            .args(arguments)
            .env("XDG_DATA_HOME", root.join("xdg-data"))
            .env("XDG_CONFIG_HOME", root.join("xdg-config"))
            .env("XDG_CACHE_HOME", root.join("xdg-cache"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let started = Instant::now();
        loop {
            match child.try_wait()? {
                Some(_) => return Ok(child.wait_with_output()?),
                None if started.elapsed() < PROCESS_TIMEOUT => {
                    thread::sleep(Duration::from_millis(5));
                }
                None => {
                    let _kill_result = child.kill();
                    let output = child.wait_with_output()?;
                    return Err(format!(
                        "binary exceeded {PROCESS_TIMEOUT:?}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )
                    .into());
                }
            }
        }
    }

    fn expect_success(output: &Output, action: &str) -> Result<(), Box<dyn Error>> {
        if output.status.success() && output.stderr.is_empty() {
            return Ok(());
        }
        Err(format!(
            "{action} failed: status {:?}, stderr {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }

    fn expect_stdout(output: Output, expected: &str, action: &str) -> Result<(), Box<dyn Error>> {
        expect_success(&output, action)?;
        let actual = String::from_utf8(output.stdout)?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!("{action} returned unexpected output: {actual:?}").into())
        }
    }

    fn expect_pack_list(binary: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
        expect_stdout(
            run_binary(binary, root, ["pack", "list"])?,
            "paper-garden\tPaper garden\n",
            "pack list",
        )
    }

    /// One completion and the identity it was saved under.
    struct Saved {
        pack_id: &'static str,
        puzzle_id: &'static str,
        completion: (Puzzle, Replay),
    }

    /// Saves a pack completion through this helper's library, so later checks
    /// show that removing the pack keeps its progress. Its early timestamp keeps
    /// the journey completion first in the keepsakes list.
    fn record_pack_completion(paths: &AppPaths, example_pack: &Path) -> Result<(), Box<dyn Error>> {
        let pack = orifude::packs::validate_directory(example_pack)?;
        let paper = pack
            .puzzles()
            .first()
            .ok_or("the example pack has no paper")?;
        let replay = paper
            .solution()
            .ok_or("the example paper has no recorded solution")?;
        let mut storage = Storage::open(paths.clone())?;
        storage.record_completion(paper.puzzle(), replay, 1, 0, false)?;
        Ok(())
    }

    /// Reads one completion after checking that it is the only attempt and
    /// still solves its paper.
    fn read_saved(
        paths: &AppPaths,
        pack_id: &'static str,
        puzzle_id: &'static str,
    ) -> Result<Saved, Box<dyn Error>> {
        let storage = Storage::open(paths.clone())?;
        let progress = storage
            .progress(pack_id, puzzle_id)?
            .ok_or_else(|| format!("saved progress for {pack_id}/{puzzle_id} is missing"))?;
        if progress.attempt_count != 1 || progress.best_replay_id <= 0 {
            return Err(format!("saved progress for {pack_id}/{puzzle_id} changed").into());
        }
        let replay = storage
            .best_replay(pack_id, puzzle_id)?
            .ok_or_else(|| format!("the best replay for {pack_id}/{puzzle_id} is missing"))?;
        if !replay
            .replay()
            .execute(replay.puzzle())?
            .result()
            .is_success()
        {
            return Err(
                format!("the best replay for {pack_id}/{puzzle_id} no longer solves").into(),
            );
        }
        Ok(Saved {
            pack_id,
            puzzle_id,
            completion: (replay.puzzle().clone(), replay.replay().clone()),
        })
    }

    /// Checks that every earlier completion is still stored unchanged.
    fn expect_saved(paths: &AppPaths, saved: &[Saved]) -> Result<(), Box<dyn Error>> {
        for expected in saved {
            let current = read_saved(paths, expected.pack_id, expected.puzzle_id)?;
            if current.completion != expected.completion {
                return Err(format!(
                    "the best replay for {}/{} changed during the lifecycle",
                    expected.pack_id, expected.puzzle_id
                )
                .into());
            }
        }
        Ok(())
    }

    /// Plays one journey through the binary's own terminal entrypoint, then
    /// leaves with Escape and a confirmed quit.
    fn play(
        binary: &Path,
        root: &Path,
        steps: &[Step],
        action: &str,
    ) -> Result<(), Box<dyn Error>> {
        let mut child = Command::new("script")
            .args(["--quiet", "--return", "--command"])
            .arg("stty cols 100 rows 30; exec \"$ORIFUDE_LIFECYCLE_BINARY\"")
            .arg("/dev/null")
            .env("ORIFUDE_LIFECYCLE_BINARY", binary)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("XDG_DATA_HOME", root.join("xdg-data"))
            .env("XDG_CONFIG_HOME", root.join("xdg-config"))
            .env("XDG_CACHE_HOME", root.join("xdg-cache"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let screen = Arc::new(Screen::default());
        let readers = [
            child.stdout.take().map(|stream| screen.record(stream)),
            child.stderr.take().map(|stream| screen.record(stream)),
        ];
        let result = drive(&mut child, &screen, steps);
        if result.is_err() {
            let _kill_result = child.kill();
        }
        let status = wait_for_exit(&mut child);
        for reader in readers.into_iter().flatten() {
            let _joined = reader.join();
        }
        let failure = match (result, status) {
            (Ok(()), Ok(status)) if status.success() => return Ok(()),
            (Ok(()), Ok(status)) => format!("exited with {status}"),
            (Err(error), _) | (Ok(()), Err(error)) => error.to_string(),
        };
        Err(format!(
            "{action} failed: {failure}; terminal output: {}",
            String::from_utf8_lossy(&screen.snapshot())
        )
        .into())
    }

    fn drive(child: &mut Child, screen: &Screen, steps: &[Step]) -> Result<(), Box<dyn Error>> {
        let mut input = child.stdin.take().ok_or("the terminal has no input")?;
        // Input sent before raw mode is consumed by the terminal line discipline.
        screen.wait_for(b"\x1b[?1049h", 0)?;
        for step in steps {
            let after = screen.len();
            input.write_all(step.input)?;
            input.flush()?;
            screen.wait_for(step.wait_for, after)?;
        }
        input.write_all(b"\x1bqy")?;
        input.flush()?;
        Ok(())
    }

    fn wait_for_exit(child: &mut Child) -> Result<std::process::ExitStatus, Box<dyn Error>> {
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() >= PROCESS_TIMEOUT {
                let _kill_result = child.kill();
                let _wait_result = child.wait();
                return Err(format!("the game exceeded {PROCESS_TIMEOUT:?}").into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Terminal output collected from the game's pseudoterminal.
    #[derive(Default)]
    struct Screen {
        bytes: Mutex<Vec<u8>>,
        changed: Condvar,
    }

    impl Screen {
        fn record(
            self: &Arc<Self>,
            mut stream: impl Read + Send + 'static,
        ) -> thread::JoinHandle<()> {
            let screen = Arc::clone(self);
            thread::spawn(move || {
                let mut buffer = [0_u8; 4096];
                while let Ok(read @ 1..) = stream.read(&mut buffer) {
                    if let Ok(mut bytes) = screen.bytes.lock() {
                        bytes.extend_from_slice(&buffer[..read]);
                    }
                    screen.changed.notify_all();
                }
            })
        }

        fn len(&self) -> usize {
            self.bytes.lock().map_or(0, |bytes| bytes.len())
        }

        fn snapshot(&self) -> Vec<u8> {
            self.bytes
                .lock()
                .map(|bytes| bytes.clone())
                .unwrap_or_default()
        }

        fn wait_for(&self, needle: &[u8], after: usize) -> Result<(), Box<dyn Error>> {
            let bytes = self
                .bytes
                .lock()
                .map_err(|_| "terminal output is unavailable")?;
            let (bytes, timeout) = self
                .changed
                .wait_timeout_while(bytes, PROCESS_TIMEOUT, |bytes| {
                    !bytes
                        .get(after..)
                        .is_some_and(|new| new.windows(needle.len()).any(|part| part == needle))
                })
                .map_err(|_| "terminal output is unavailable")?;
            drop(bytes);
            if timeout.timed_out() {
                return Err(format!(
                    "the terminal never showed {:?}",
                    String::from_utf8_lossy(needle)
                )
                .into());
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("error: this direct-binary lifecycle check currently requires Linux XDG isolation");
    std::process::exit(1);
}
