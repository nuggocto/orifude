use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::content::{BranchGift, BuiltInPaper, JourneyGroup};

const GROUPS: [JourneyGroup; 8] = [
    JourneyGroup {
        title: "Ink on paper",
        mechanic: "Place two marks on flat paper, then let one crease carry ink farther.",
        first_paper: 0,
        paper_count: 5,
        gift: BranchGift::Leaf,
    },
    JourneyGroup {
        title: "Across the crease",
        mechanic: "Choose the moving side and let one mark reach two layers.",
        first_paper: 5,
        paper_count: 5,
        gift: BranchGift::PairedLeaves,
    },
    JourneyGroup {
        title: "Gathered layers",
        mechanic: "Use two axes and read stacks gathered into a quarter.",
        first_paper: 10,
        paper_count: 5,
        gift: BranchGift::Berries,
    },
    JourneyGroup {
        title: "Uneven edges",
        mechanic: "Use off-center creases and folds that leave empty positions.",
        first_paper: 15,
        paper_count: 5,
        gift: BranchGift::PaperBoat,
    },
    JourneyGroup {
        title: "Fold order",
        mechanic: "Make later creases possible by choosing the earlier fold first.",
        first_paper: 20,
        paper_count: 5,
        gift: BranchGift::Bird,
    },
    JourneyGroup {
        title: "The long brush",
        mechanic: "Draw horizontal and vertical lines across occupied stacks.",
        first_paper: 25,
        paper_count: 5,
        gift: BranchGift::LongBranch,
    },
    JourneyGroup {
        title: "Mixed brushwork",
        mechanic: "Combine dots, lines, folds, and a limited stroke budget.",
        first_paper: 30,
        paper_count: 5,
        gift: BranchGift::BerrySprig,
    },
    JourneyGroup {
        title: "Under the canopy",
        mechanic: "Read a larger target and combine every earlier rule.",
        first_paper: 35,
        paper_count: 5,
        gift: BranchGift::Canopy,
    },
];

static PAPERS: OnceLock<Box<[BuiltInPaper]>> = OnceLock::new();

pub(super) const fn groups() -> &'static [JourneyGroup] {
    &GROUPS
}

pub(super) fn papers() -> &'static [BuiltInPaper] {
    PAPERS.get_or_init(load).as_ref()
}

fn load() -> Box<[BuiltInPaper]> {
    let pack = crate::packs::validate_files(files())
        .expect("embedded journey files must pass the production pack validator");
    assert_eq!(pack.metadata().id(), "orifude-journey");
    assert_eq!(pack.puzzles().len(), 40);
    pack.puzzles()
        .iter()
        .map(BuiltInPaper::from_content)
        .collect()
}

macro_rules! journey_files {
    ($files:ident; $($id:literal),+ $(,)?) => {
        $(
            assert!(
                $files
                    .insert(
                        concat!("puzzles/", $id, ".toml").to_owned(),
                        include_bytes!(concat!(
                            "../../puzzles/journey/puzzles/",
                            $id,
                            ".toml"
                        ))
                        .to_vec(),
                    )
                    .is_none(),
                "embedded journey paths must be unique",
            );
        )+
    };
}

fn files() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    assert!(
        files
            .insert(
                "pack.toml".to_owned(),
                include_bytes!("../../puzzles/journey/pack.toml").to_vec(),
            )
            .is_none()
    );
    journey_files!(files;
        "first-drop", "corner-seed", "two-drops", "open-window", "small-sprig",
        "folded-pair", "low-reflection", "high-reflection", "left-reflection", "two-windows",
        "four-leaves", "upper-left", "crossed-corner", "stacked-rain", "quarter-turn",
        "short-crease", "far-crease", "shallow-rain", "lifted-edge", "uneven-corner",
        "second-crease", "returning-edge", "falling-twice", "rising-twice", "folded-crossroads",
        "horizontal-thread", "vertical-thread", "folded-stripe", "folded-bar", "cross-weave",
        "line-and-drop", "bar-and-drop", "quarter-line", "uneven-ink", "woven-corner",
        "quiet-canopy", "offset-canopy", "three-bands", "wide-branches", "full-canopy",
    );
    files
}
