use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use orifude::domain::paper::{BrushRule, Dimensions, PaperAction};
use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use orifude::domain::replay::Replay;
use orifude::storage::{AppPaths, Settings, Storage};

const SAMPLES: usize = 500;
const SAVED_PUZZLES: usize = 1024;

fn main() -> Result<(), Box<dyn Error>> {
    if cfg!(debug_assertions) {
        return Err("storage measurements must run with --release".into());
    }
    let mut arguments = std::env::args_os().skip(1);
    if let Some(mode) = arguments.next() {
        let root = arguments.next().ok_or("expected --prepare ROOT")?;
        if mode != "--prepare" || arguments.next().is_some() {
            return Err("expected --prepare ROOT".into());
        }
        return prepare_player(Path::new(&root));
    }
    let root = tempfile::tempdir_in("target")?;
    let paths = app_paths(root.path());
    let (puzzle, replay) = solved_replay("berry")?;
    let mut storage = Storage::open(paths)?;
    report("fresh", measure(&mut storage, &puzzle, &replay)?);
    seed_history(&mut storage)?;
    report("populated", measure(&mut storage, &puzzle, &replay)?);
    let page = storage.progress_page(1024)?;
    assert_eq!(page.entries.len(), 1);
    assert!(!page.has_more);
    drop(storage);
    root.close()?;
    Ok(())
}

fn report(workload: &str, mut samples: Vec<Duration>) {
    for (index, sample) in samples.iter().enumerate() {
        println!("sample_us,{workload},{index},{}", sample.as_micros());
    }
    samples.sort_unstable();
    println!("workload={workload}");
    println!("samples={SAMPLES}");
    println!("p50_us={}", percentile(&samples, 50).as_micros());
    println!("p95_us={}", percentile(&samples, 95).as_micros());
    println!("p99_us={}", percentile(&samples, 99).as_micros());
    println!(
        "max_us={}",
        samples.last().copied().unwrap_or_default().as_micros()
    );
}

fn measure(
    storage: &mut Storage,
    puzzle: &Puzzle,
    replay: &Replay,
) -> Result<Vec<Duration>, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let started = Instant::now();
        storage.record_completion(puzzle, replay, i64::try_from(sample)?, 0, false)?;
        samples.push(started.elapsed());
    }
    Ok(samples)
}

fn solved_replay(id: &str) -> Result<(Puzzle, Replay), Box<dyn Error>> {
    let identity = PuzzleIdentity::new("measure", id)?;
    let dimensions = Dimensions::new(4, 4)?;
    let coordinate = dimensions.coordinate(0, 0)?;
    let target = dimensions.cell_id(coordinate)?;
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity, 4, 4)
            .with_target_cells(vec![target])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(0, 1),
    )?;
    let mut attempt = puzzle.start();
    attempt.apply(PaperAction::Dot(coordinate))?;
    let replay = Replay::from_attempt(&attempt);
    Ok((puzzle, replay))
}

fn app_paths(root: &Path) -> AppPaths {
    AppPaths::injected(
        root.join("data/orifude"),
        root.join("config/orifude"),
        root.join("cache/orifude"),
    )
}

fn seed_history(storage: &mut Storage) -> Result<(), Box<dyn Error>> {
    for index in 0..SAVED_PUZZLES {
        let (puzzle, replay) = solved_replay(&format!("paper-{index}"))?;
        storage.record_completion(&puzzle, &replay, i64::try_from(index)?, 0, false)?;
    }
    println!("saved_puzzles={SAVED_PUZZLES}");
    Ok(())
}

fn prepare_player(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir(root)?;
    let mut storage = Storage::open(app_paths(root))?;
    seed_history(&mut storage)?;
    storage.save_settings(Settings {
        lesson_complete: true,
        reduced_motion: true,
        ..Settings::default()
    })?;
    let source = root.join("source");
    fs::create_dir_all(source.join("puzzles"))?;
    fs::write(
        source.join("pack.toml"),
        r#"format_version = 1
id = "measure-board"
title = "Wide paper"
authors = ["Orifude"]
license = "Apache-2.0"
puzzles = ["wide-paper"]
"#,
    )?;
    let mut target = vec!["............"; 12];
    target[0] = ".....##.....";
    fs::write(
        source.join("puzzles/wide-paper.toml"),
        format!(
            r#"format_version = 1
id = "wide-paper"
title = "Wide paper"
width = 12
height = 12
target = {target:?}
folds = [{{ direction = "right", crease = 6 }}]
brushes = [{{ kind = "dot" }}]
fold_budget = 1
stroke_budget = 1
"#
        ),
    )?;
    storage.install_pack(&source, 0)?;
    Ok(())
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let index = samples
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len().saturating_sub(1));
    samples.get(index).copied().unwrap_or_default()
}
