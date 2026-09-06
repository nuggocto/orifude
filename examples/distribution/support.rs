use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const REPOSITORY: &str = "nuggocto/orifude";
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
pub fn read_stream(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take(limit + 1).read_to_end(&mut data)?;
    require(
        data.len() as u64 <= limit,
        "file exceeds release byte limit",
    )?;
    Ok(data)
}
pub fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    require(
        metadata.is_file() && metadata.len() <= limit,
        "not a bounded regular file",
    )?;
    read_stream(File::open(path)?, limit)
}
pub fn text(path: &Path) -> Result<String> {
    Ok(String::from_utf8(read(path, 1024 * 1024)?)?)
}
pub fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    {
        require(path.is_file(), "executable is missing")?;
    }
    Ok(())
}
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.current_dir(root()).stdin(Stdio::null());
    // A child of pwsh must let Windows PowerShell rebuild its own module paths.
    if command
        .get_program()
        .to_string_lossy()
        .eq_ignore_ascii_case("powershell.exe")
    {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().eq_ignore_ascii_case("PSMODULEPATH") {
                command.env_remove(key);
            }
        }
    }
    command
}

pub struct OwnedChild(pub Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Capture to private files so a verbose child cannot deadlock on a full pipe.
// Polling also bounds disk output and kills/reaps the owned child on failure.
pub fn output(command: &mut Command, seconds: u64) -> Result<(ExitStatus, String)> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    let mut child = OwnedChild(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        require(
            stdout.metadata()?.len() <= MAX_BYTES && stderr.metadata()?.len() <= MAX_BYTES,
            "child output exceeds release byte limit",
        )?;
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        require(Instant::now() < deadline, "release command timed out")?;
        thread::sleep(Duration::from_millis(25));
    };
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let out = String::from_utf8(read_stream(stdout, MAX_BYTES)?)?;
    let err = String::from_utf8_lossy(&read_stream(stderr, MAX_BYTES)?).into_owned();
    if !err.is_empty() {
        eprint!("{err}");
    }
    if !status.success() {
        eprint!("{out}");
    }
    Ok((status, out.trim().to_owned()))
}
pub fn run(command: &mut Command) -> Result<String> {
    let (status, out) = output(command, 1200)?;
    require(
        status.success(),
        &format!(
            "{} failed ({status})",
            command.get_program().to_string_lossy()
        ),
    )?;
    Ok(out)
}
pub fn gh(args: &[&str]) -> Result<String> {
    let (status, out) = output(command("gh").args(args), 120)?;
    require(status.success(), "GitHub CLI command failed")?;
    Ok(out)
}
