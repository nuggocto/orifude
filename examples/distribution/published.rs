use super::{
    archive, packages,
    support::{self, MAX_BYTES, REPOSITORY, Result, gh, require},
};
use serde_json::Value;
use std::{fs, path::Path};

pub fn validate_assets(release: &Value, expected: &[String]) -> Result<()> {
    require(
        release["immutable"] == true && release["draft"] == false,
        "public checks require an immutable published release",
    )?;
    let assets = release["assets"]
        .as_array()
        .ok_or("missing release assets")?;
    require(
        assets.len() == expected.len(),
        "unexpected public asset count",
    )?;
    let mut names = Vec::with_capacity(expected.len());
    for asset in assets {
        require(
            asset["state"] == "uploaded"
                && asset["size"]
                    .as_u64()
                    .is_some_and(|size| size > 0 && size <= MAX_BYTES),
            "public asset is incomplete or exceeds the download limit",
        )?;
        names.push(asset["name"].as_str().ok_or("missing public asset name")?);
    }
    names.sort_unstable();
    require(
        names == expected,
        "public asset names differ from the release contract",
    )
}

fn download(directory: &Path) -> Result<()> {
    let tag = format!("v{}", archive::version()?);
    let release: Value = serde_json::from_str(&gh(&[
        "api",
        &format!("repos/{REPOSITORY}/releases/tags/{tag}"),
    ])?)?;
    let names = archive::public_assets()?;
    validate_assets(&release, &names)?;
    gh(&["release", "verify", &tag, "--repo", REPOSITORY])?;
    gh(&[
        "release",
        "download",
        &tag,
        "--repo",
        REPOSITORY,
        "--dir",
        &directory.to_string_lossy(),
    ])?;
    for name in &names {
        let path = directory.join(name);
        support::read(&path, MAX_BYTES)?;
        gh(&[
            "release",
            "verify-asset",
            &tag,
            &path.to_string_lossy(),
            "--repo",
            REPOSITORY,
        ])?;
    }
    // Compare downloaded files before generating the unpublished package metadata.
    for (name, bytes) in archive::generated(&archive::manifest(directory)?)? {
        if names.contains(&name) {
            require(
                support::read(&directory.join(&name), MAX_BYTES)? == bytes,
                "published checksums or installer differ from the verified archives",
            )?;
        } else {
            fs::write(directory.join(name), bytes)?;
        }
    }
    archive::check(directory)?;
    println!("public_assets=verified tag={tag}");
    Ok(())
}

fn player(directory: &Path, target: &str, binary: &Path) -> Result<()> {
    let binary = binary.canonicalize()?;
    let files = archive::files(&directory.join(archive::name(target)?), target)?;
    require(
        support::read(&binary, MAX_BYTES)? == files[archive::binary_name(target)],
        "installed executable differs from the attested release",
    )?;
    require(
        support::run(support::command(&binary).arg("--version"))?
            == format!("orifude {}", archive::version()?),
        "installed version is incorrect",
    )?;
    archive::installed_journey(&binary)
}

pub fn verify(target: &str, channel: &str) -> Result<()> {
    archive::checked_target(target)?;
    require(
        matches!(
            (channel, target),
            ("archives" | "installer", _)
                | ("homebrew", "x86_64-apple-darwin" | "aarch64-apple-darwin")
                | ("scoop", "x86_64-pc-windows-msvc")
                | ("aur", "x86_64-unknown-linux-musl")
        ),
        "unsupported public installation channel",
    )?;
    let temporary = tempfile::tempdir()?;
    let directory = temporary.path().join("downloads");
    fs::create_dir(&directory)?;
    download(&directory)?;
    match channel {
        "archives" => archive::journey(&directory, target)?,
        "installer" => installer(&directory, target, temporary.path())?,
        "homebrew" => homebrew(&directory, target)?,
        "scoop" => scoop(&directory, target, temporary.path())?,
        "aur" => aur(&directory, target, temporary.path())?,
        _ => return Err("unsupported public installation channel".into()),
    }
    println!("public_installation=pass channel={channel} target={target}");
    Ok(())
}

fn installer(directory: &Path, target: &str, root: &Path) -> Result<()> {
    let destination = root.join("bin with spaces");
    fs::create_dir(&destination)?;
    let mut command = if target.contains("windows") {
        let mut command = support::command("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(directory.join("install.ps1"))
            .arg("-NoPath")
            .arg("-BinDir");
        command
    } else {
        let mut command = support::command("sh");
        command.arg(directory.join("install.sh")).arg("--bin-dir");
        command
    };
    command.arg(&destination);
    support::run(&mut command)?;
    player(
        directory,
        target,
        &destination.join(archive::binary_name(target)),
    )
}

fn homebrew(directory: &Path, target: &str) -> Result<()> {
    let formula = "nuggocto/tap/orifude";
    let repository = support::run(support::command("brew").arg("--repository"))?;
    let tap = Path::new(&repository).join("Library/Taps/nuggocto/homebrew-tap");
    require(!tap.exists(), "public Homebrew QA requires an unused tap")?;
    require(
        support::run(support::command("brew").args(["list", "--formula"]))?
            .lines()
            .all(|name| name != "orifude"),
        "Orifude is already installed through Homebrew",
    )?;
    let brew = |args: &[&str]| {
        support::run(
            support::command("brew")
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .args(args),
        )
    };
    brew(&["tap", "nuggocto/tap"])?;
    let result = (|| -> Result<()> {
        require(
            support::text(&tap.join("Formula/orifude.rb"))?
                == support::text(&directory.join("orifude.rb"))?,
            "public Homebrew formula differs from verified metadata",
        )?;
        brew(&["install", formula])?;
        brew(&["test", formula])?;
        let prefix = brew(&["--prefix", formula])?;
        player(directory, target, &Path::new(&prefix).join("bin/orifude"))
    })();
    let uninstall = brew(&["uninstall", "--force", formula]);
    let untap = brew(&["untap", "nuggocto/tap"]);
    result?;
    uninstall?;
    untap?;
    Ok(())
}

fn scoop(directory: &Path, target: &str, root: &Path) -> Result<()> {
    let scoop_root = root.join("scoop");
    for name in ["shims", "buckets"] {
        fs::create_dir_all(scoop_root.join(name))?;
    }
    let checkout = scoop_root.join("apps/scoop/current");
    support::run(
        support::command("git")
            .args(["clone", "https://github.com/ScoopInstaller/Scoop.git"])
            .arg(&checkout),
    )?;
    support::run(
        support::command("git")
            .args(["checkout", "--detach", packages::SCOOP_COMMIT])
            .current_dir(checkout),
    )?;
    let script = root.join("public-scoop.ps1");
    fs::write(&script, SCOOP_CHECK)?;
    let expected = archive::digest(&support::read(&directory.join("orifude.json"), MAX_BYTES)?);
    let invoke = |mode: &str| {
        support::run(
            support::command("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&script)
                .arg(&scoop_root)
                .arg(&expected)
                .arg(mode),
        )
    };
    let result = (|| -> Result<()> {
        invoke("install")?;
        player(
            directory,
            target,
            &scoop_root.join("apps/orifude/current/orifude.exe"),
        )
    })();
    let cleanup = invoke("uninstall");
    result?;
    cleanup?;
    Ok(())
}

fn aur(directory: &Path, target: &str, root: &Path) -> Result<()> {
    let output = root.join("installed");
    fs::create_dir(&output)?;
    let transcript = support::run(support::command("docker").args([
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "--cap-drop",
        "NET_RAW",
        "--mount",
        &format!(
            "type=bind,src={},dst=/expected,readonly",
            directory.display()
        ),
        "--mount",
        &format!("type=bind,src={},dst=/installed", output.display()),
        packages::ARCH_IMAGE,
        "bash",
        "-c",
        ARCH_CHECK,
    ]))?;
    require(
        transcript.ends_with("public_aur_install_remove=pass"),
        "public AUR journey did not finish",
    )?;
    player(directory, target, &output.join("orifude"))
}

const SCOOP_CHECK: &str = r#"param($Root, $Expected, $Mode)
$ErrorActionPreference = 'Stop'
$env:SCOOP = $Root
$env:SCOOP_GLOBAL = Join-Path $Root 'global'
$env:XDG_CONFIG_HOME = Join-Path $Root 'config'
$env:PATH = (Join-Path $Root 'shims') + ';' + $env:PATH
$Scoop = Join-Path $Root 'apps/scoop/current/bin/scoop.ps1'
if ($Mode -eq 'install') {
    & $Scoop config aria2-enabled false
    & $Scoop config last_update ([DateTime]::Now.ToString('o'))
    & $Scoop bucket add nuggocto https://github.com/nuggocto/scoop-bucket
    if ($LASTEXITCODE -ne 0) { throw 'Public Scoop bucket setup failed.' }
    $Manifest = Join-Path $Root 'buckets/nuggocto/bucket/orifude.json'
    # Git may check JSON out with CRLF on Windows; compare its canonical LF text.
    $Text = [IO.File]::ReadAllText($Manifest).Replace("`r`n", "`n")
    $Algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $Hash = [BitConverter]::ToString($Algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))).Replace('-', '').ToLowerInvariant()
    } finally { $Algorithm.Dispose() }
    if ($Hash -cne $Expected) {
        throw 'Public Scoop manifest differs from verified metadata.'
    }
    & $Scoop install nuggocto/orifude
    if ($LASTEXITCODE -ne 0) { throw 'Public Scoop installation failed.' }
    $Version = (Get-Content -Raw -LiteralPath $Manifest | ConvertFrom-Json).version
    $Actual = & (Join-Path $Root 'shims/orifude.exe') --version
    if ($LASTEXITCODE -ne 0 -or $Actual -cne "orifude $Version") { throw 'Public Scoop shim failed.' }
} else {
    & $Scoop uninstall orifude
    if ($LASTEXITCODE -ne 0 -or (Test-Path (Join-Path $Root 'shims/orifude.exe'))) {
        throw 'Public Scoop removal failed.'
    }
}
"#;

const ARCH_CHECK: &str = r"set -eu
pacman -Sy --noconfirm --needed base-devel git >/dev/null
useradd -m builder
runuser -u builder -- git clone https://aur.archlinux.org/orifude-bin.git /home/builder/package
cd /home/builder/package
cmp PKGBUILD /expected/PKGBUILD
runuser -u builder -- makepkg --printsrcinfo > /tmp/expected-srcinfo
cmp .SRCINFO /tmp/expected-srcinfo
runuser -u builder -- makepkg --verifysource --noconfirm
runuser -u builder -- makepkg --noconfirm
pacman -U --noconfirm ./orifude-bin-*-x86_64.pkg.tar.zst
orifude --version
cp /usr/bin/orifude /installed/orifude
pacman -R --noconfirm orifude-bin
test ! -e /usr/bin/orifude
printf 'public_aur_install_remove=pass\n'
";
