#[cfg(target_os = "linux")]
mod linux {
    use std::error::Error;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use orifude::storage::{AppPaths, Storage};

    const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);

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
        record_progress(&root.app_paths(), &example_pack)?;

        install_binary(&candidate, &installed)?;
        expect_success(
            &run_binary(&installed, root.path(), ["--version"])?,
            "candidate startup",
        )?;
        expect_pack_list(&installed, root.path())?;
        expect_progress(&root.app_paths(), &example_pack)?;

        install_binary(&baseline, &installed)?;
        expect_pack_list(&installed, root.path())?;
        expect_progress(&root.app_paths(), &example_pack)?;

        fs::remove_file(&installed)?;
        expect_progress(&root.app_paths(), &example_pack)?;

        install_binary(&candidate, &installed)?;
        expect_stdout(
            run_binary(&installed, root.path(), ["pack", "remove", "paper-garden"])?,
            "Removed pack paper-garden. Saved progress was kept.\n",
            "candidate pack removal",
        )?;
        expect_progress(&root.app_paths(), &example_pack)?;
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
        expect_progress(&root.app_paths(), &example_pack)?;

        root.cleanup()?;

        println!("direct_binary_lifecycle=pass");
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

    fn record_progress(paths: &AppPaths, example_pack: &Path) -> Result<(), Box<dyn Error>> {
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

    fn expect_progress(paths: &AppPaths, example_pack: &Path) -> Result<(), Box<dyn Error>> {
        let storage = Storage::open(paths.clone())?;
        let progress = storage
            .progress("paper-garden", "first-seed")?
            .ok_or("saved progress is missing")?;
        if progress.attempt_count != 1 || progress.best_replay_id <= 0 {
            return Err("saved progress changed during the lifecycle".into());
        }
        let expected_pack = orifude::packs::validate_directory(example_pack)?;
        let expected_paper = expected_pack
            .puzzles()
            .first()
            .ok_or("the example pack has no paper")?;
        let expected_replay = expected_paper
            .solution()
            .ok_or("the example paper has no recorded solution")?;
        let saved = storage
            .best_replay("paper-garden", "first-seed")?
            .ok_or("the saved best replay is missing")?;
        if saved.puzzle() != expected_paper.puzzle() || saved.replay() != expected_replay {
            return Err("the saved best replay changed during the lifecycle".into());
        }
        Ok(())
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
