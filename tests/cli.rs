use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_orifude"))
        .args(arguments)
        .output()
        .expect("the Orifude binary should start")
}

#[test]
fn starting_without_arguments_reports_the_current_product_state() {
    let output = run(&[]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
        concat!(
            "Orifude is a quiet, offline puzzle game for the terminal.\n",
            "The playable game is not available in this build yet.\n",
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_only_advertises_available_behavior() {
    for argument in ["-h", "--help"] {
        let output = run(&[argument]);

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
            concat!(
                "Orifude is a quiet, offline puzzle game for the terminal.\n\n",
                "Usage: orifude [OPTIONS]\n\n",
                "Options:\n",
                "  -h, --help     Print help\n",
                "  -V, --version  Print version\n",
            )
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn version_comes_from_the_package_metadata() {
    for argument in ["-V", "--version"] {
        let output = run(&[argument]);

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
            format!("orifude {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn unsupported_arguments_have_a_stable_usage_status() {
    for arguments in [&["play"][..], &["--help", "extra"][..]] {
        let output = run(arguments);

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).expect("stderr should be UTF-8"),
            concat!(
                "error: unsupported command-line arguments\n\n",
                "Usage: orifude [OPTIONS]\n",
                "For more information, try '--help'.\n",
            )
        );
    }
}

#[test]
fn unsupported_arguments_are_not_reflected_to_the_terminal() {
    let output = run(&["--bad\u{1b}[31m"]);
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");

    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr.contains('\u{1b}'));
    assert!(!stderr.contains("--bad"));
}
