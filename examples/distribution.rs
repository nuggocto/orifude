//! Developer-only archive, installation, and publication checks.
#[path = "distribution/archive.rs"]
mod archive;
#[path = "distribution/fixture.rs"]
mod fixture;
#[path = "distribution/install.rs"]
mod install;
#[path = "distribution/packages.rs"]
mod packages;
#[path = "distribution/publication.rs"]
mod publication;
#[path = "distribution/support.rs"]
mod support;
#[cfg(test)]
#[path = "distribution/tests.rs"]
mod tests;

use std::{path::Path, process::ExitCode};
use support::{Result, require};

fn execute() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["targets"] => println!("{}", serde_json::to_string(&archive::targets()?)?),
        ["build", target, directory] => archive::build(target, Path::new(directory))?,
        ["pack", target, binary, directory] => {
            archive::pack(target, Path::new(binary), Path::new(directory))?;
        }
        ["assemble", directory] => archive::assemble(Path::new(directory))?,
        ["check", directory] => archive::check(Path::new(directory))?,
        ["check", directory, "--target", target] => {
            archive::check(Path::new(directory))?;
            archive::smoke(Path::new(directory), target)?;
        }
        ["smoke", target, directory] => archive::smoke(Path::new(directory), target)?,
        ["installer-check", directory, target] => {
            install::verify(&Path::new(directory).canonicalize()?, target)?;
        }
        ["artifact-check", directory, target] => archive::journey(Path::new(directory), target)?,
        ["package-check", directory, target] => {
            packages::verify(&Path::new(directory).canonicalize()?, target)?;
        }
        ["publish", tag, commit, run] => publication::publish(tag, commit, run, false)?,
        ["publish", tag, commit, run, "--publish"] => publication::publish(tag, commit, run, true)?,
        ["channel", directory, channel] => {
            publication::channel(Path::new(directory), channel, false)?;
        }
        ["channel", directory, channel, "--push"] => {
            publication::channel(Path::new(directory), channel, true)?;
        }
        _ => require(
            false,
            "usage: distribution targets | build TARGET DIR | pack TARGET BINARY DIR | assemble DIR | check DIR [--target TARGET] | smoke TARGET DIR | installer-check DIR TARGET | artifact-check DIR TARGET | package-check DIR TARGET | publish TAG COMMIT RUN [--publish] | channel DIR homebrew|scoop|aur [--push]",
        )?,
    }
    Ok(())
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
