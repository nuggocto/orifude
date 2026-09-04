mod journey;

use crate::domain::paper::{
    BrushRule, CellId, Column, Coordinate, Fold, FoldCount, FoldDirection, PaperAction, Row,
    StrokeCount,
};
use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use crate::domain::replay::{Replay, ReplayMetadata};
use crate::domain::score::Par;
use crate::generator::{GenerationError, Generator, GeneratorConfig};
use crate::packs::PuzzleContent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltInPaper {
    puzzle: Puzzle,
    title: Box<str>,
    description: Box<str>,
    cues: Box<[Box<str>]>,
    solution: Box<[PaperAction]>,
}

impl BuiltInPaper {
    pub(crate) const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn cues(&self) -> &[Box<str>] {
        &self.cues
    }

    pub(crate) fn solution(&self) -> &[PaperAction] {
        &self.solution
    }
}

impl BuiltInPaper {
    fn from_content(content: &PuzzleContent) -> Self {
        let solution = content
            .solution()
            .expect("official journey papers must carry a validated solution");
        Self {
            puzzle: content.puzzle().clone(),
            title: content.title().into(),
            description: content.description().unwrap_or_default().into(),
            cues: content.tutorial_cues().to_vec().into_boxed_slice(),
            solution: solution.actions().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchGift {
    Leaf,
    PairedLeaves,
    Berries,
    PaperBoat,
    Bird,
    LongBranch,
    BerrySprig,
    Canopy,
}

impl BranchGift {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Leaf => "a first leaf",
            Self::PairedLeaves => "a pair of leaves",
            Self::Berries => "a berry cluster",
            Self::PaperBoat => "a folded boat",
            Self::Bird => "a small bird",
            Self::LongBranch => "a longer branch",
            Self::BerrySprig => "a berry sprig",
            Self::Canopy => "the full canopy",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JourneyGroup {
    pub(crate) title: &'static str,
    pub(crate) mechanic: &'static str,
    pub(crate) first_paper: usize,
    pub(crate) paper_count: usize,
    pub(crate) gift: BranchGift,
}

pub(crate) fn journey_groups() -> &'static [JourneyGroup] {
    journey::groups()
}

pub(crate) fn journey_group(paper_index: usize) -> Option<(usize, &'static JourneyGroup)> {
    journey_groups().iter().enumerate().find(|(_, group)| {
        paper_index >= group.first_paper
            && paper_index < group.first_paper.saturating_add(group.paper_count)
    })
}

pub(crate) fn lesson() -> BuiltInPaper {
    folded_paper(
        BuiltInText {
            pack_id: "orifude-lesson",
            puzzle_id: "one-fold",
            title: "One fold, one mark",
            description: "Fold the left half across, then let one dot pass through both layers.",
            cues: &[
                "Fold ready. + marks the moving side; Enter folds.",
                "Dot brush ready. Move @ to row 2, col 3; Enter inks.",
                "Target matched. Enter opens and checks the paper.",
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
    journey::papers().to_vec()
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
        title: content.title.into(),
        description: content.description.into(),
        cues: content
            .cues
            .iter()
            .map(|cue| Box::<str>::from(*cue))
            .collect(),
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
    fn journey_groups_cover_the_catalog_exactly() {
        let papers = journey();
        let groups = journey_groups();

        assert_eq!(papers.len(), 40);
        assert_eq!(groups.len(), 8);
        for (index, group) in groups.iter().enumerate() {
            assert_eq!(group.first_paper, index * 5);
            assert_eq!(group.paper_count, 5);
        }
        assert_eq!(
            groups.last().unwrap().first_paper + groups.last().unwrap().paper_count,
            papers.len()
        );
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
