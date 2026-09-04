use std::collections::BTreeSet;
use std::path::PathBuf;

use orifude::packs::validate_directory;
use orifude::solver::{NeverCancel, SolveOutcome, Solver, SolverLimits};

fn catalog(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("puzzles")
        .join(name)
}

#[test]
fn official_journey_is_valid_and_independently_solvable() {
    let pack = validate_directory(&catalog("journey")).expect("official journey validates");
    assert_eq!(pack.metadata().id(), "orifude-journey");
    assert_eq!(pack.puzzles().len(), 40);

    let mut identities = BTreeSet::new();
    let mut titles = BTreeSet::new();
    for (index, content) in pack.puzzles().iter().enumerate() {
        let puzzle = content.puzzle();
        assert!(identities.insert(puzzle.identity().puzzle_id()));
        assert!(titles.insert(content.title()));

        let recorded = content.solution().expect("official solution witness");
        if index == 0 {
            assert_eq!(
                content.tutorial_cues().len(),
                recorded.actions().len() + 1,
                "the first journey paper should teach each input"
            );
        } else {
            assert!(
                content.tutorial_cues().is_empty(),
                "{} should leave its solution for the player to discover",
                puzzle.identity().puzzle_id()
            );
        }
        assert!(
            content
                .description()
                .is_some_and(|description| !description.is_empty()),
            "{} should carry a short description",
            puzzle.identity().puzzle_id()
        );
        let attempt = recorded
            .execute(puzzle)
            .expect("recorded solution executes");
        assert!(attempt.result().is_success());

        match Solver::solve(puzzle, SolverLimits::default(), &NeverCancel) {
            SolveOutcome::Solved(solution) => {
                let attempt = solution
                    .replay()
                    .execute(puzzle)
                    .expect("solver replay executes");
                assert!(attempt.result().is_success());
            }
            outcome => panic!(
                "official puzzle {} was not solved: {outcome:?}",
                puzzle.identity().puzzle_id()
            ),
        }
    }
}

#[test]
fn the_first_crease_arrives_after_two_flat_papers() {
    let pack = validate_directory(&catalog("journey")).expect("official journey validates");
    let opening = &pack.puzzles()[..5];

    assert!(
        opening[..2]
            .iter()
            .all(|paper| paper.puzzle().fold_budget().get() == 0)
    );
    assert!(
        opening[2..]
            .iter()
            .all(|paper| paper.puzzle().fold_budget().get() > 0)
    );
}

#[test]
fn example_community_pack_uses_the_public_format() {
    let pack = validate_directory(&catalog("example-pack")).expect("example pack validates");

    assert_eq!(pack.metadata().id(), "paper-garden");
    assert_eq!(pack.puzzles().len(), 3);
    assert!(
        pack.puzzles()
            .iter()
            .all(|content| content.solution().is_some_and(|solution| solution
                .execute(content.puzzle())
                .is_ok_and(|attempt| attempt.result().is_success())))
    );
    for content in pack.puzzles() {
        assert_eq!(
            content.tutorial_cues().len(),
            content
                .solution()
                .expect("example solution")
                .actions()
                .len()
                + 1,
            "{} should advance one cue per action and end with the open instruction",
            content.puzzle().identity().puzzle_id()
        );
    }
}
