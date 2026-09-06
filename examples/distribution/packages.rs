use super::{
    archive,
    fixture::Http,
    support::{self, Result, require},
};
use std::{fs, path::Path};
pub const ARCH_IMAGE: &str =
    "archlinux:base@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a";
pub const SCOOP_COMMIT: &str = "b588a06e41d920d2123ec70aee682bae14935939";

pub fn verify(directory: &Path, target: &str) -> Result<()> {
    archive::check(directory)?;
    archive::smoke(directory, target)?;
    if target.contains("linux") {
        return arch(directory);
    }
    let server = Http::start(directory)?;
    if target.contains("darwin") {
        homebrew(directory, &server.base)
    } else {
        let root = tempfile::tempdir()?;
        scoop(directory, root.path(), &server.base)
    }
}
fn homebrew(directory: &Path, base: &str) -> Result<()> {
    let name = "orifude-fixture/candidate";
    let formula_name = format!("{name}/orifude");
    let repository = support::run(support::command("brew").arg("--repository"))?;
    let tap = Path::new(&repository).join("Library/Taps/orifude-fixture/homebrew-candidate");
    require(!tap.exists(), "fixture tap already exists")?;
    require(
        support::run(support::command("brew").args(["list", "--formula"]))?
            .lines()
            .all(|line| line != "orifude"),
        "Orifude is already installed through Homebrew",
    )?;
    support::run(support::command("brew").args(["tap-new", "--no-git", name]))?;
    let result = (|| -> Result<()> {
        let template =
            support::text(&directory.join("orifude.rb"))?.replace(&archive::base_url()?, base);
        let formula = tap.join("Formula/orifude.rb");
        fs::write(&formula, &template)?;
        support::run(support::command("brew").args(["install", "--formula", &formula_name]))?;
        support::run(support::command("brew").args(["test", &formula_name]))?;
        fs::write(
            &formula,
            template.replace(
                "  license \"Apache-2.0\"",
                "  revision 1\n  license \"Apache-2.0\"",
            ),
        )?;
        support::run(support::command("brew").args(["upgrade", &formula_name]))?;
        let revision = format!("{}_1", archive::version()?);
        require(
            support::run(support::command("brew").args(["list", "--versions", &formula_name]))?
                .split_whitespace()
                .any(|value| value == revision),
            "Homebrew did not install the package revision",
        )?;
        support::run(support::command("brew").args(["test", &formula_name]))?;
        Ok(())
    })();
    let uninstall =
        support::run(support::command("brew").args(["uninstall", "--force", &formula_name]));
    let untap = support::run(support::command("brew").args(["untap", name]));
    result?;
    uninstall?;
    untap?;
    println!("homebrew_install_upgrade_test_uninstall=pass");
    Ok(())
}
fn scoop(directory: &Path, root: &Path, base: &str) -> Result<()> {
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
            .args(["checkout", "--detach", SCOOP_COMMIT])
            .current_dir(&checkout),
    )?;
    let bucket = root.join("bucket-source");
    fs::create_dir(&bucket)?;
    let mut manifest: serde_json::Value =
        serde_json::from_str(&support::text(&directory.join("orifude.json"))?)?;
    manifest["architecture"]["64bit"]["url"] =
        format!("{base}/{}", archive::name("x86_64-pc-windows-msvc")?).into();
    fs::write(
        bucket.join("orifude.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    support::run(
        support::command("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(&bucket),
    )?;
    support::run(
        support::command("git")
            .args(["add", "."])
            .current_dir(&bucket),
    )?;
    support::run(
        support::command("git")
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "Create package fixture",
            ])
            .current_dir(&bucket),
    )?;
    let script = root.join("check.ps1");
    fs::write(&script, SCOOP_CHECK)?;
    let out = support::run(
        support::command("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script)
            .arg(&scoop_root)
            .arg(&bucket)
            .arg(archive::version()?),
    )?;
    require(
        out.ends_with("scoop_journey=complete"),
        "Scoop stopped before completing the installation journey",
    )?;
    println!("scoop_install_upgrade_uninstall=pass");
    Ok(())
}
fn arch(directory: &Path) -> Result<()> {
    if std::env::consts::ARCH != "x86_64" {
        println!(
            "arch_arm_package=covered_by_arch_assembly; arm_payload=covered_by_native_journey"
        );
        return Ok(());
    }
    let output = support::run(support::command("docker").args([
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "--cap-drop",
        "NET_RAW",
        "--mount",
        &format!(
            "type=bind,src={},dst=/candidate,readonly",
            directory.display()
        ),
        ARCH_IMAGE,
        "bash",
        "-c",
        ARCH_CHECK,
    ]))?;
    require(
        output.ends_with("arch_install_upgrade_uninstall=pass"),
        "Arch stopped before completing the installation journey",
    )?;
    println!("arch_install_upgrade_uninstall=pass");
    Ok(())
}
const SCOOP_CHECK: &str = r#"param($Root, $Bucket, $Version)
$ErrorActionPreference = 'Stop'
$env:SCOOP = $Root
$env:SCOOP_GLOBAL = Join-Path $Root 'global'
$env:XDG_CONFIG_HOME = Join-Path $Root 'config'
$env:PATH = (Join-Path $Root 'shims') + ';' + $env:PATH
$Scoop = Join-Path $Root 'apps/scoop/current/bin/scoop.ps1'
& $Scoop config aria2-enabled false
& $Scoop config last_update ([DateTime]::Now.ToString('o'))
$BucketUrl = ([Uri]$Bucket).AbsoluteUri
& $Scoop bucket add fixture $BucketUrl
if ($LASTEXITCODE -ne 0) { throw 'Scoop bucket setup failed.' }
& $Scoop install fixture/orifude
if ($LASTEXITCODE -ne 0) { throw 'Scoop installation failed.' }
$Binary = Join-Path $Root 'shims/orifude.exe'
$ActualVersion = & $Binary --version
if ($LASTEXITCODE -ne 0 -or $ActualVersion -cne "orifude $Version") { throw 'Scoop shim failed.' }
$ManifestPath = Join-Path $Root 'buckets/fixture/orifude.json'
$Manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$Manifest.version = $Manifest.version + '.1'
$Manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ManifestPath -Encoding UTF8
& $Scoop update orifude
if ($LASTEXITCODE -ne 0) { throw 'Scoop update failed.' }
$Installed = Get-Content -Raw -LiteralPath (Join-Path $Root 'apps/orifude/current/manifest.json') | ConvertFrom-Json
if ($Installed.version -ne $Manifest.version) { throw 'Scoop did not install the package revision.' }
$ActualVersion = & $Binary --version
if ($LASTEXITCODE -ne 0 -or $ActualVersion -cne "orifude $Version") { throw 'Updated Scoop shim failed.' }
& $Scoop uninstall orifude
if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $Binary)) { throw 'Scoop uninstall failed.' }
Write-Output 'scoop_journey=complete'
"#;
const ARCH_CHECK: &str = r"set -eu
pacman -Sy --noconfirm --needed base-devel >/dev/null
useradd -m builder
mkdir /work
cp /candidate/PKGBUILD /candidate/*.tar.gz /work/
chown -R builder:builder /work
cd /work
runuser -u builder -- makepkg --printsrcinfo > .SRCINFO
runuser -u builder -- makepkg --verifysource --noconfirm
runuser -u builder -- makepkg --noconfirm
pacman -U --noconfirm ./orifude-bin-*-x86_64.pkg.tar.zst
orifude --version
orifude --help >/dev/null
sed -i 's/^pkgrel=1$/pkgrel=2/' PKGBUILD
runuser -u builder -- makepkg --noconfirm
pacman -U --noconfirm ./orifude-bin-*-2-x86_64.pkg.tar.zst
orifude --version
pacman -R --noconfirm orifude-bin
if test -e /usr/bin/orifude; then exit 1; fi
cp /etc/makepkg.conf /work/arm.conf
printf '\nCARCH=aarch64\n' >> /work/arm.conf
runuser -u builder -- makepkg --config /work/arm.conf --noconfirm
printf 'arch_install_upgrade_uninstall=pass\n'
";
