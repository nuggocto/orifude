use super::{
    archive,
    fixture::Https,
    support::{self, MAX_BYTES, Result, require},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Installer {
    root: PathBuf,
    script: PathBuf,
    destination: PathBuf,
    certificate: PathBuf,
    windows: bool,
}
impl Installer {
    fn environment(&self, command: &mut Command) {
        command
            .env("CURL_CA_BUNDLE", &self.certificate)
            .env("ORIFUDE_FIXTURE_CA", &self.certificate)
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("LOCALAPPDATA", self.root.join("data"))
            .env("APPDATA", self.root.join("config"))
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", &self.root)
            .env("TEMP", &self.root)
            .env("TMP", &self.root)
            .env(
                "ORIFUDE_DOWNLOAD_SENTINEL",
                self.root.join("executed-partial-installer"),
            );
        if self.windows {
            // Model the client policy without changing user or machine settings.
            command.env("PSExecutionPolicyPreference", "Restricted");
        }
    }
    fn command(&self) -> Command {
        let mut command = if self.windows {
            let mut command = support::command("powershell.exe");
            command
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&self.script)
                .arg("-NoPath")
                .arg("-BinDir");
            command
        } else {
            let mut command = support::command("sh");
            command.arg(&self.script).arg("--bin-dir");
            command
        };
        command.arg(&self.destination);
        self.environment(&mut command);
        command
    }
    fn execute(&self, success: bool) -> Result<()> {
        expect(&mut self.command(), success)
    }
    fn check_execution_policy(&self) -> Result<()> {
        if !self.windows {
            return Ok(());
        }
        let mut policy = support::command("powershell.exe");
        policy.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-ExecutionPolicy",
        ]);
        self.environment(&mut policy);
        require(
            support::run(&mut policy)? == "Restricted",
            "installer fixture did not inherit the restricted client policy",
        )
    }
    fn partial_download(&self, server: &Https) -> Result<()> {
        let partial = self.root.join(if self.windows {
            "partial.ps1"
        } else {
            "partial.sh"
        });
        let name = partial
            .file_name()
            .ok_or("missing fixture filename")?
            .to_string_lossy();
        server.serve(
            &name,
            if self.windows {
                b"Set-Content -LiteralPath $env:ORIFUDE_DOWNLOAD_SENTINEL -Value executed\r\n"
            } else {
                b"touch \"$ORIFUDE_DOWNLOAD_SENTINEL\"\n"
            },
            100,
        )?;
        let url = format!("{}/{name}", server.base);
        let mut command = if self.windows {
            let wrapper = self.root.join("download.ps1");
            fs::write(
                &wrapper,
                "param($Destination, $Url)\n$ErrorActionPreference = 'Stop'\ncurl.exe --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --cacert $env:ORIFUDE_FIXTURE_CA --max-time 10 --output $Destination $Url\nif ($LASTEXITCODE -ne 0) { throw 'Installer download failed.' }\n& $Destination\n",
            )?;
            let mut command = support::command("powershell.exe");
            command
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(wrapper)
                .arg(&partial)
                .arg(url);
            command
        } else {
            let mut command = support::command("sh");
            command.args(["-c", "curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --max-time 10 --output \"$1\" \"$2\" && sh \"$1\"", "download"]).arg(&partial).arg(url);
            command
        };
        self.environment(&mut command);
        expect(&mut command, false)?;
        require(
            !support::read(&partial, 4096)?.is_empty(),
            "partial-download fixture did not transfer the executable prefix",
        )?;
        require(
            !self.root.join("executed-partial-installer").exists(),
            "a partial installer was executed",
        )
    }
}
fn expect(command: &mut Command, success: bool) -> Result<()> {
    let (status, _) = support::output(command, 180)?;
    require(
        status.success() == success,
        "installer returned an unexpected exit status",
    )
}
fn preserved(existing: &Path) -> Result<()> {
    require(
        support::read(existing, MAX_BYTES)? == b"previous executable must survive failure",
        "failed installation replaced the previous executable",
    )
}

pub fn verify(directory: &Path, target: &str) -> Result<()> {
    archive::check(directory)?;
    archive::checked_target(target)?;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let destination = root.join("bin with spaces");
    fs::create_dir(&destination)?;
    let windows = target.contains("windows");
    let server = Https::start(root)?;
    let installer = Installer {
        root: root.to_owned(),
        script: root.join(if windows { "install.ps1" } else { "install.sh" }),
        destination,
        certificate: server.certificate.clone(),
        windows,
    };
    installer.check_execution_policy()?;
    let script_name = if windows { "install.ps1" } else { "install.sh" };
    let mut script =
        support::text(&directory.join(script_name))?.replace(&archive::base_url()?, &server.base);
    if windows {
        script = script.replace(
            "--connect-timeout",
            "--cacert $env:ORIFUDE_FIXTURE_CA --connect-timeout",
        );
    }
    fs::write(&installer.script, &script)?;
    let archive_name = archive::name(target)?;
    let original = support::read(&directory.join(&archive_name), MAX_BYTES)?;
    let existing = installer.destination.join(archive::binary_name(target));
    fs::write(&existing, b"previous executable must survive failure")?;
    let saved = root.join("data/orifude/orifude.sqlite3");
    fs::create_dir_all(saved.parent().ok_or("missing data parent")?)?;
    fs::write(&saved, b"existing player database")?;
    installer.partial_download(&server)?;
    preserved(&existing)?;
    #[cfg(unix)]
    unix_failures(&installer, &existing)?;
    server.serve(&archive_name, b"tampered archive", 0)?;
    installer.execute(false)?;
    preserved(&existing)?;
    server.missing(&archive_name)?;
    installer.execute(false)?;
    preserved(&existing)?;
    server.serve(
        &archive_name,
        &original[..original.len().min(128)],
        original.len().saturating_sub(128),
    )?;
    installer.execute(false)?;
    preserved(&existing)?;
    server.serve(&archive_name, &original, 0)?;
    fs::write(
        &installer.script,
        script.replace(&archive::digest(&original), &"0".repeat(64)),
    )?;
    installer.execute(false)?;
    preserved(&existing)?;
    fs::write(&installer.script, &script)?;
    fs::remove_file(&existing)?;
    fs::create_dir(&existing)?;
    installer.execute(false)?;
    fs::remove_dir(&existing)?;
    installer.execute(true)?;
    require(
        support::run(support::command(&existing).arg("--version"))?
            == format!("orifude {}", archive::version()?),
        "installed version differs",
    )?;
    let before = support::read(&existing, MAX_BYTES)?;
    installer.execute(true)?;
    require(
        support::read(&existing, MAX_BYTES)? == before,
        "reinstall changed verified bytes",
    )?;
    fs::remove_file(&existing)?;
    installer.execute(true)?;
    new_destinations(&installer, target, &before)?;
    require(
        support::read(&saved, MAX_BYTES)? == b"existing player database",
        "installation changed unrelated saved data",
    )?;
    for parent in [root, &installer.destination] {
        for entry in fs::read_dir(parent)? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            require(
                !name.starts_with(".orifude-install") && !name.starts_with("orifude-install"),
                "installer left temporary state",
            )?;
        }
    }
    println!("installer_check=pass target={target}");
    Ok(())
}

fn new_destinations(installer: &Installer, target: &str, before: &[u8]) -> Result<()> {
    let root = &installer.root;
    let windows = installer.windows;
    let existing = installer.destination.join(archive::binary_name(target));
    // A first install must create its directory, including the default location.
    fs::remove_file(&existing)?;
    fs::remove_dir(&installer.destination)?;
    installer.execute(true)?;
    let default_destination = if windows {
        root.join("data/Programs/Orifude")
    } else {
        root.join("home/.local/bin")
    };
    let mut default_command = if windows {
        let mut command = support::command("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&installer.script)
            .arg("-NoPath");
        command
    } else {
        let mut command = support::command("sh");
        command.arg(&installer.script);
        command
    };
    installer.environment(&mut default_command);
    expect(&mut default_command, true)?;
    require(
        support::read(
            &default_destination.join(archive::binary_name(target)),
            MAX_BYTES,
        )? == before,
        "default installation differs from the verified executable",
    )?;
    if windows {
        windows_path(installer)?;
    }
    Ok(())
}

fn windows_path(installer: &Installer) -> Result<()> {
    let wrapper = installer.root.join("path-check.ps1");
    fs::write(
        &wrapper,
        r#"
param($Installer, $BinDir)
$ErrorActionPreference = 'Stop'
$Key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$Present = $Key.GetValueNames() -contains 'Path'
$Original = $Key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$Kind = if ($Present) { $Key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
try {
    $FixturePath = '%SystemRoot%\System32;C:\unrelated path'
    $Key.SetValue('Path', $FixturePath, [Microsoft.Win32.RegistryValueKind]::ExpandString)
    & $Installer -BinDir $BinDir -NoPath
    if ($Key.GetValue('Path', '', 1) -cne $FixturePath) { throw 'NoPath changed saved PATH.' }
    & $Installer -BinDir $BinDir
    $Expected = "$BinDir;$FixturePath"
    if ($Key.GetValue('Path', '', 1) -cne $Expected) { throw 'User PATH lost existing entries.' }
    if ($Key.GetValueKind('Path') -ne 'ExpandString') { throw 'User PATH lost its registry type.' }
    & $Installer -BinDir $BinDir
    if ($Key.GetValue('Path', '', 1) -cne $Expected) { throw 'Reinstall duplicated user PATH.' }
    $Key.SetValue('Path', "$($BinDir.ToUpperInvariant())\;$FixturePath", [Microsoft.Win32.RegistryValueKind]::String)
    & $Installer -BinDir $BinDir
    if ($Key.GetValue('Path', '', 1) -cne "$($BinDir.ToUpperInvariant())\;$FixturePath") {
        throw 'An equivalent PATH entry was duplicated.'
    }
    if ($Key.GetValueKind('Path') -ne 'String') { throw 'String PATH changed type.' }
} finally {
    if ($Present) { $Key.SetValue('Path', $Original, $Kind) } else { $Key.DeleteValue('Path', $false) }
    $Key.Dispose()
}
"#,
    )?;
    let mut command = support::command("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(wrapper)
        .arg(&installer.script)
        .arg(&installer.destination);
    installer.environment(&mut command);
    expect(&mut command, true)
}
#[cfg(unix)]
fn unix_failures(installer: &Installer, existing: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    require(
        support::run(support::command("id").arg("-u"))? != "0",
        "installer permission checks require an unprivileged account",
    )?;
    let unsupported = installer.root.join("unsupported");
    fs::create_dir(&unsupported)?;
    let uname = unsupported.join("uname");
    fs::write(&uname, "#!/bin/sh\nprintf 'Unsupported\\n'\n")?;
    support::executable(&uname)?;
    let mut paths = vec![unsupported];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    expect(
        installer
            .command()
            .env("PATH", std::env::join_paths(paths)?),
        false,
    )?;
    preserved(existing)?;
    fs::set_permissions(&installer.destination, fs::Permissions::from_mode(0o500))?;
    let result = installer.execute(false).and_then(|()| preserved(existing));
    fs::set_permissions(&installer.destination, fs::Permissions::from_mode(0o700))?;
    result
}
