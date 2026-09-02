use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use orifude::domain::paper::{BrushRule, Dimensions, PaperAction};
use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use orifude::domain::replay::Replay;
use orifude::storage::{AppPaths, Storage};

const SAMPLES: usize = 500;

fn main() -> Result<(), Box<dyn Error>> {
    let root = measurement_root()?;
    fs::create_dir(&root)?;
    let paths = AppPaths::injected(root.join("data"), root.join("config"), root.join("cache"));
    let (puzzle, replay) = solved_replay()?;
    let result = measure(&paths, &puzzle, &replay);
    let cleanup = fs::remove_dir_all(&root);
    let mut samples = result?;
    cleanup?;
    samples.sort_unstable();
    println!("samples={SAMPLES}");
    println!("p50_us={}", percentile(&samples, 50).as_micros());
    println!("p95_us={}", percentile(&samples, 95).as_micros());
    println!("p99_us={}", percentile(&samples, 99).as_micros());
    println!(
        "max_us={}",
        samples.last().copied().unwrap_or_default().as_micros()
    );
    Ok(())
}

fn measure(
    paths: &AppPaths,
    puzzle: &Puzzle,
    replay: &Replay,
) -> Result<Vec<Duration>, Box<dyn Error>> {
    let mut storage = Storage::open(paths.clone())?;
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let started = Instant::now();
        storage.record_completion(puzzle, replay, i64::try_from(sample)?, 0, false)?;
        samples.push(started.elapsed());
    }
    Ok(samples)
}

fn solved_replay() -> Result<(Puzzle, Replay), Box<dyn Error>> {
    let identity = PuzzleIdentity::new("measure", "berry")?;
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

fn measurement_root() -> Result<PathBuf, std::io::Error> {
    Ok(std::env::current_dir()?
        .join("target")
        .join(format!("orifude-storage-measure-{}", std::process::id())))
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
