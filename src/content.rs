use crate::domain::paper::{
    BrushRule, CellId, Column, Coordinate, Fold, FoldCount, FoldDirection, PaperAction, Row,
    StrokeCount,
};
use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use crate::domain::replay::{Replay, ReplayMetadata};
use crate::domain::score::Par;
use crate::generator::{GenerationError, Generator, GeneratorConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltInPaper {
    puzzle: Puzzle,
    title: &'static str,
    description: &'static str,
    cues: &'static [&'static str],
    solution: Box<[PaperAction]>,
}

impl BuiltInPaper {
    pub(crate) const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    pub(crate) const fn title(&self) -> &'static str {
        self.title
    }

    pub(crate) const fn description(&self) -> &'static str {
        self.description
    }

    pub(crate) const fn cues(&self) -> &'static [&'static str] {
        self.cues
    }

    pub(crate) fn solution(&self) -> &[PaperAction] {
        &self.solution
    }
}

pub(crate) fn lesson() -> BuiltInPaper {
    folded_paper(
        BuiltInText {
            pack_id: "orifude-lesson",
            puzzle_id: "one-fold",
            title: "One fold, one mark",
            description: "Fold the left half across, then let one dot pass through both layers.",
            cues: &[
                "Tab previews the fold. + marks the moving side; Enter confirms.",
                "Move @ to row 2, column 3. Tab selects the dot; Enter marks the stack.",
                "No action is selected. Enter opens the paper and compares it with the target.",
            ],
        },
        vec![Fold::new(FoldDirection::Right, 2)],
        vec![
            PaperAction::Fold(Fold::new(FoldDirection::Right, 2)),
            PaperAction::Dot(coordinate(1, 2)),
        ],
        (1, 1),
    )
}

pub(crate) fn journey() -> Vec<BuiltInPaper> {
    vec![
        folded_paper(
            BuiltInText {
                pack_id: "orifude-journey",
                puzzle_id: "first-drop",
                title: "First drop",
                description: "Place one quiet dot on the waiting sheet.",
                cues: &["Move to the marked cell, choose the brush, and confirm."],
            },
            Vec::new(),
            vec![PaperAction::Dot(coordinate(1, 1))],
            (0, 1),
        ),
        folded_paper(
            BuiltInText {
                pack_id: "orifude-journey",
                puzzle_id: "folded-pair",
                title: "Folded pair",
                description: "One fold lets a single dot reach two cells.",
                cues: &["Fold right at crease 2 before applying the dot."],
            },
            vec![Fold::new(FoldDirection::Right, 2)],
            vec![
                PaperAction::Fold(Fold::new(FoldDirection::Right, 2)),
                PaperAction::Dot(coordinate(1, 2)),
            ],
            (1, 1),
        ),
        folded_paper(
            BuiltInText {
                pack_id: "orifude-journey",
                puzzle_id: "four-leaves",
                title: "Four leaves",
                description: "Two folds gather four cells beneath one brush.",
                cues: &["Fold down, fold right, then mark the four-layer stack."],
            },
            vec![
                Fold::new(FoldDirection::Down, 2),
                Fold::new(FoldDirection::Right, 2),
            ],
            vec![
                PaperAction::Fold(Fold::new(FoldDirection::Down, 2)),
                PaperAction::Fold(Fold::new(FoldDirection::Right, 2)),
                PaperAction::Dot(coordinate(2, 2)),
            ],
            (2, 1),
        ),
    ]
}

pub(crate) fn generator(pack_id: &str) -> Result<Generator, GenerationError> {
    let folds = vec![
        Fold::new(FoldDirection::Left, 2),
        Fold::new(FoldDirection::Right, 2),
        Fold::new(FoldDirection::Up, 2),
        Fold::new(FoldDirection::Down, 2),
    ];
    let config = GeneratorConfig::new(pack_id, 4, 4)?
        .with_rules(folds, vec![BrushRule::Dot])?
        .with_budgets(2, 1)
        .with_source_action_range(2, 3)
        .with_attempt_limit(64);
    Generator::new(config)
}

#[derive(Clone, Copy)]
struct BuiltInText {
    pack_id: &'static str,
    puzzle_id: &'static str,
    title: &'static str,
    description: &'static str,
    cues: &'static [&'static str],
}

fn folded_paper(
    content: BuiltInText,
    allowed_folds: Vec<Fold>,
    solution: Vec<PaperAction>,
    budgets: (u8, u8),
) -> BuiltInPaper {
    let (fold_budget, stroke_budget) = budgets;
    let identity = PuzzleIdentity::new(content.pack_id, content.puzzle_id)
        .expect("built-in paper identities must follow the portable grammar");
    let template = Puzzle::new(
        PuzzleSpec::new(identity.clone(), 4, 4)
            .with_allowed_folds(allowed_folds.clone())
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(fold_budget, stroke_budget),
    )
    .expect("built-in paper rules must validate");
    let mut attempt = template.start();
    for &action in &solution {
        attempt
            .apply(action)
            .expect("a built-in solution must use its declared rules");
    }
    let target = attempt.ink().cell_ids().collect::<Vec<CellId>>();
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity, 4, 4)
            .with_target_cells(target)
            .with_allowed_folds(allowed_folds)
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(fold_budget, stroke_budget)
            .with_par(Par::new(
                FoldCount::new(fold_budget).expect("built-in fold par fits"),
                StrokeCount::new(stroke_budget).expect("built-in stroke par fits"),
            )),
    )
    .expect("built-in paper must validate");
    let replay = Replay::new(ReplayMetadata::current(&puzzle), solution.clone())
        .expect("built-in solutions fit the replay bound");
    assert!(
        replay
            .execute(&puzzle)
            .expect("built-in replay must execute")
            .result()
            .is_success(),
        "built-in replay must solve its exact paper"
    );
    BuiltInPaper {
        puzzle,
        title: content.title,
        description: content.description,
        cues: content.cues,
        solution: solution.into_boxed_slice(),
    }
}

fn coordinate(row: u8, column: u8) -> Coordinate {
    Coordinate::new(
        Row::new(row).expect("built-in row fits"),
        Column::new(column).expect("built-in column fits"),
    )
}

#[cfg(test)]
mod tests {
    use crate::generator::{GenerationOutcome, GenerationSeed};
    use crate::solver::NeverCancel;

    use super::*;

    #[test]
    fn built_in_papers_are_solved_by_their_recorded_actions() {
        let mut papers = journey();
        papers.push(lesson());

        for paper in papers {
            let replay = Replay::new(
                ReplayMetadata::current(paper.puzzle()),
                paper.solution().to_vec(),
            )
            .expect("recorded replay");
            let attempt = replay.execute(paper.puzzle()).expect("replay executes");
            assert!(attempt.result().is_success());
        }
    }

    #[test]
    fn player_generation_policy_produces_a_replay_verified_paper() {
        let generator = generator("orifude-endless").expect("generation policy");
        let outcome = generator.generate(GenerationSeed::current(7), &NeverCancel);
        let GenerationOutcome::Generated { puzzle, .. } = outcome else {
            panic!("fixed player seed should generate a paper");
        };

        assert!(
            puzzle
                .solution()
                .replay()
                .execute(puzzle.puzzle())
                .expect("generated replay executes")
                .result()
                .is_success()
        );
    }
}
