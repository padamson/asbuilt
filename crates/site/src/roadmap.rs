//! The roadmap as data: Now (the next release, with each item's status),
//! Next and Later. The section renders straight from `ITEMS`, so moving an
//! item is an edit here, made in the commit that does the work.
//!
//! When a release ships, delete its done items (the changelog keeps
//! them), move `MILESTONE` and `NEXT_MILESTONE` on a version and promote
//! from Next.

/// The release the Now column is building toward.
pub const MILESTONE: &str = "0.2.0";

/// The release after it, which the Next column is planned for.
pub const NEXT_MILESTONE: &str = "0.3.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horizon {
    Now,
    Next,
    Later,
}

impl Horizon {
    pub const ALL: [Horizon; 3] = [Horizon::Now, Horizon::Next, Horizon::Later];

    pub fn label(self) -> &'static str {
        match self {
            Horizon::Now => "Now",
            Horizon::Next => "Next",
            Horizon::Later => "Later",
        }
    }

    /// The column's id, and the prefix of its items' ids.
    pub fn id(self) -> &'static str {
        match self {
            Horizon::Now => "roadmap-now",
            Horizon::Next => "roadmap-next",
            Horizon::Later => "roadmap-later",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Planned,
    /// For the item being worked on; no item is at times.
    #[allow(dead_code)]
    InProgress,
    Done,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Planned => "Planned",
            Status::InProgress => "In progress",
            Status::Done => "Done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// Stable, so a test or a link can find the item: `<column id>-<slug>`.
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    pub horizon: Horizon,
    /// Only Now items are anything but planned.
    pub status: Status,
    /// The GitHub issue that tracks it, once there is one.
    pub issue: Option<u32>,
}

const fn now(id: &'static str, title: &'static str, blurb: &'static str, status: Status) -> Item {
    Item {
        id,
        title,
        blurb,
        horizon: Horizon::Now,
        status,
        issue: None,
    }
}

const fn later(
    horizon: Horizon,
    id: &'static str,
    title: &'static str,
    blurb: &'static str,
) -> Item {
    Item {
        id,
        title,
        blurb,
        horizon,
        status: Status::Planned,
        issue: None,
    }
}

pub const ITEMS: &[Item] = &[
    now(
        "roadmap-now-titles",
        "Page titles name the project",
        "Every docs page's title leads with home_title: asbuilt · Architecture · asbuilt-core.",
        Status::Done,
    ),
    now(
        "roadmap-now-labels",
        "Crate labels say what a crate builds",
        "A lib with a bin beside it is a library and binary crate; examples-only packages are example crates.",
        Status::Done,
    ),
    now(
        "roadmap-now-readable",
        "Diagrams at a readable size",
        "Every view at a legible scale in a scrolling frame, with Fit, 1:1, Wide and Fullscreen, so a 5000pt view is not shrunk to 3px text.",
        Status::Done,
    ),
    now(
        "roadmap-now-explore",
        "Zoom, pan and click-through",
        "Wheel and pinch zoom, drag to pan, a node links to its page and names itself on hover.",
        Status::Planned,
    ),
    now(
        "roadmap-now-edges",
        "Open a merged edge",
        "Click an edge LikeC4 labels [...] to list the module relations it stands for, each linked.",
        Status::Planned,
    ),
    now(
        "roadmap-now-tags",
        "Color bins, tests and examples",
        "bin, tests and examples are element kinds of their own, so [theme] colors them and a crate's binary stands apart from its modules.",
        Status::Done,
    ),
    now(
        "roadmap-now-likec4",
        "LikeC4 1.59.4 and a parity check",
        "Bump the pin, and run the LikeC4 tests weekly against the latest release so a break is an issue, not a surprise.",
        Status::Done,
    ),
    now(
        "roadmap-now-fixes",
        "Small fixes",
        "Link curated views from every page, reject a misspelled config entry, and ship the pre-commit hook as a definition consumers reference by release.",
        Status::Done,
    ),
    later(
        Horizon::Next,
        "roadmap-next-ci",
        "Drop-in CI",
        "A GitHub Action, cargo-binstall metadata, and aarch64 Linux and musl builds.",
    ),
    later(
        Horizon::Next,
        "roadmap-next-drift",
        "Drift you can read",
        "check names what changed (a relation added, a module removed) and speaks JSON with --format json.",
    ),
    later(
        Horizon::Next,
        "roadmap-next-kinds",
        "Relation kinds you can see",
        "A line and arrowhead per kind, so implements and uses differ at a glance, and a legend.",
    ),
    later(
        Horizon::Later,
        "roadmap-later-deps",
        "Dependencies as externals",
        "Opt-in externals from Cargo.toml, and crate-to-crate summaries on each container page.",
    ),
    later(
        Horizon::Later,
        "roadmap-later-cfg",
        "Features and cfg",
        "Know which modules and relations a feature or platform turns on.",
    ),
    later(
        Horizon::Later,
        "roadmap-later-viewer",
        "LikeC4's interactive viewer",
        "An option to embed LikeC4's own web component: element details, a relationship browser, search.",
    ),
    later(
        Horizon::Later,
        "roadmap-later-frontends",
        "More front-ends",
        "A second language, and a deployment front-end to fill the model's empty deployment section.",
    ),
];

/// The items in one column, in roadmap order.
pub fn column(items: &[Item], horizon: Horizon) -> impl Iterator<Item = &Item> {
    items.iter().filter(move |item| item.horizon == horizon)
}

/// Done and total for the Now column.
pub fn progress(items: &[Item]) -> (usize, usize) {
    let done = column(items, Horizon::Now)
        .filter(|item| item.status == Status::Done)
        .count();
    (done, column(items, Horizon::Now).count())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_item_id_is_unique() {
        let ids: BTreeSet<&str> = ITEMS.iter().map(|item| item.id).collect();
        assert_eq!(ids.len(), ITEMS.len());
    }

    #[test]
    fn every_item_id_starts_with_its_column_id() {
        let strays: Vec<&str> = ITEMS
            .iter()
            .filter(|item| !item.id.starts_with(&format!("{}-", item.horizon.id())))
            .map(|item| item.id)
            .collect();
        assert_eq!(strays, Vec::<&str>::new());
    }

    #[test]
    fn only_now_items_are_started_or_done() {
        let strays: Vec<&str> = ITEMS
            .iter()
            .filter(|item| item.horizon != Horizon::Now && item.status != Status::Planned)
            .map(|item| item.id)
            .collect();
        assert_eq!(strays, Vec::<&str>::new());
    }

    #[test]
    fn progress_counts_done_items_in_now_and_ignores_the_other_columns() {
        let items = [
            now("roadmap-now-a", "", "", Status::Done),
            now("roadmap-now-b", "", "", Status::InProgress),
            now("roadmap-now-c", "", "", Status::Planned),
            later(Horizon::Next, "roadmap-next-d", "", ""),
        ];
        assert_eq!(progress(&items), (1, 3));
    }

    #[test]
    fn every_column_has_an_item() {
        for horizon in Horizon::ALL {
            assert!(
                column(ITEMS, horizon).next().is_some(),
                "{horizon:?} is empty"
            );
        }
    }
}
