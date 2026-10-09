//! The roadmap as data, in `public/roadmap.json`: Now (the next release,
//! with each item's status), Next and Later. Trunk copies the file into
//! every build, so main's copy is published at `/asbuilt/dev/roadmap.json`,
//! and every build fetches that one: whichever version a visitor views,
//! the roadmap is the current one. The copy compiled in here is what a
//! build shows until that fetch succeeds, or if it fails. Moving an item is
//! an edit to the file, made in the commit that does the work.
//!
//! When a release ships, delete its done items (the changelog keeps
//! them), move `milestone` and `next_milestone` on a version, promote
//! from Next, and give `now_theme` and `next_theme` the few words each
//! release is about.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::version::snapshot_url;

/// The roadmap this build was compiled with.
pub const BUILT: &str = include_str!("../public/roadmap.json");

/// Where main's roadmap is published, which every release build fetches.
pub fn current_url() -> String {
    format!("{}roadmap.json", snapshot_url("dev"))
}

/// Whether the section says it is showing main's roadmap: on a release
/// build that fetched it, so its Now column is not read as what that
/// release shipped. The dev build is main's.
pub fn says_main(fetched: bool, dev: bool) -> bool {
    fetched && !dev
}

/// Unknown keys, statuses and columns are tolerated rather than refused,
/// so an older snapshot can still read a roadmap a newer main publishes;
/// the host tests hold this repo's own file to exactly these fields and
/// values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roadmap {
    /// The release the Now column is building toward.
    pub milestone: String,
    /// The release after it, which the Next column is planned for.
    pub next_milestone: String,
    /// What each release is about, in a few words, by version: Now and
    /// Next show their milestone's. Keyed by version, so a theme cannot
    /// outlive its release when the milestones move.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub themes: BTreeMap<String, String>,
    pub items: Vec<Item>,
}

impl Roadmap {
    /// The roadmap compiled into this build.
    pub fn built() -> Self {
        serde_json::from_str(BUILT).expect("public/roadmap.json parses; the host tests hold it")
    }

    /// The items in one column, in roadmap order.
    pub fn column(&self, horizon: Horizon) -> impl Iterator<Item = &Item> {
        self.items
            .iter()
            .filter(move |item| item.horizon == horizon)
    }

    /// What the release a column is building toward is about: the theme
    /// of Now's or Next's milestone, none for Later, a blank theme, or a
    /// milestone without one.
    pub fn theme(&self, horizon: Horizon) -> Option<&str> {
        let version = match horizon {
            Horizon::Now => &self.milestone,
            Horizon::Next => &self.next_milestone,
            Horizon::Later | Horizon::Unknown => return None,
        };
        self.themes
            .get(version)
            .map(String::as_str)
            .filter(|theme| !theme.trim().is_empty())
    }

    /// Done and total for the Now column.
    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .column(Horizon::Now)
            .filter(|item| item.status == Status::Done)
            .count();
        (done, self.column(Horizon::Now).count())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Horizon {
    Now,
    Next,
    Later,
    /// A column a newer main added; its items are not shown here.
    #[serde(other)]
    Unknown,
}

impl Horizon {
    pub const ALL: [Horizon; 3] = [Horizon::Now, Horizon::Next, Horizon::Later];

    pub fn label(self) -> &'static str {
        match self {
            Horizon::Now => "Now",
            Horizon::Next => "Next",
            Horizon::Later => "Later",
            Horizon::Unknown => "",
        }
    }

    /// The column's id, and the prefix of its items' ids.
    pub fn id(self) -> &'static str {
        match self {
            Horizon::Now => "roadmap-now",
            Horizon::Next => "roadmap-next",
            Horizon::Later => "roadmap-later",
            Horizon::Unknown => "roadmap-unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[default]
    Planned,
    InProgress,
    Done,
    /// A status a newer main added; the item shows without one.
    #[serde(other)]
    Unknown,
}

impl Status {
    /// The badge text, or `None` for a status this build does not know.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Status::Planned => Some("Planned"),
            Status::InProgress => Some("In progress"),
            Status::Done => Some("Done"),
            Status::Unknown => None,
        }
    }

    fn is_planned(&self) -> bool {
        *self == Status::Planned
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Stable, so a test or a link can find the item: `<column id>-<slug>`.
    pub id: String,
    pub title: String,
    pub blurb: String,
    pub horizon: Horizon,
    /// Only Now items are anything but planned, which is the default.
    #[serde(default, skip_serializing_if = "Status::is_planned")]
    pub status: Status,
    /// The GitHub issue that tracks it, once there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<u32>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_file_holds_exactly_the_roadmap_s_fields() {
        // An unknown or misspelled key parses (and is dropped), so the file
        // must survive a round trip through the types unchanged.
        let file: serde_json::Value = serde_json::from_str(BUILT).unwrap();
        assert_eq!(
            serde_json::to_value(Roadmap::built()).unwrap(),
            file,
            "public/roadmap.json has a key or value the types drop: a misspelling, \
             an unknown status or column, or a planned status or a null issue \
             written out (leave those two out)"
        );
    }

    #[test]
    fn every_item_id_is_unique() {
        let roadmap = Roadmap::built();
        let ids: BTreeSet<&str> = roadmap.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids.len(), roadmap.items.len());
    }

    #[test]
    fn every_item_id_starts_with_its_column_id() {
        let strays: Vec<String> = Roadmap::built()
            .items
            .into_iter()
            .filter(|item| !item.id.starts_with(&format!("{}-", item.horizon.id())))
            .map(|item| item.id)
            .collect();
        assert_eq!(strays, Vec::<String>::new());
    }

    #[test]
    fn only_now_items_are_started_or_done() {
        let strays: Vec<String> = Roadmap::built()
            .items
            .into_iter()
            .filter(|item| item.horizon != Horizon::Now && item.status != Status::Planned)
            .map(|item| item.id)
            .collect();
        assert_eq!(strays, Vec::<String>::new());
    }

    fn item(id: &str, horizon: Horizon, status: Status) -> Item {
        Item {
            id: id.into(),
            title: String::new(),
            blurb: String::new(),
            horizon,
            status,
            issue: None,
        }
    }

    #[test]
    fn progress_counts_done_items_in_now_and_ignores_the_other_columns() {
        let roadmap = Roadmap {
            milestone: "0.1.0".into(),
            next_milestone: "0.2.0".into(),
            themes: BTreeMap::new(),
            items: vec![
                item("roadmap-now-a", Horizon::Now, Status::Done),
                item("roadmap-now-b", Horizon::Now, Status::InProgress),
                item("roadmap-now-c", Horizon::Now, Status::Planned),
                item("roadmap-next-d", Horizon::Next, Status::Done),
            ],
        };
        assert_eq!(roadmap.progress(), (1, 3));
    }

    #[test]
    fn every_column_has_an_item() {
        let roadmap = Roadmap::built();
        for horizon in Horizon::ALL {
            assert!(
                roadmap.column(horizon).next().is_some(),
                "{horizon:?} is empty"
            );
        }
    }

    #[test]
    fn a_roadmap_with_a_key_this_build_does_not_know_still_reads() {
        let newer = r#"{"milestone": "9.0.0", "next_milestone": "9.1.0", "owner": "x",
            "items": [{"id": "roadmap-now-a", "title": "A", "blurb": "", "horizon": "now", "eta": "soon"}]}"#;
        assert_eq!(
            serde_json::from_str::<Roadmap>(newer)
                .map(|r| r.milestone)
                .ok(),
            Some("9.0.0".to_string())
        );
    }

    #[test]
    fn main_s_roadmap_is_fetched_from_the_dev_build() {
        assert_eq!(current_url(), "/asbuilt/dev/roadmap.json");
    }

    #[test]
    fn an_item_with_a_status_this_build_does_not_know_still_reads() {
        let newer = r#"{"milestone": "9.0.0", "next_milestone": "9.1.0", "items": [
            {"id": "roadmap-now-a", "title": "A", "blurb": "", "horizon": "now", "status": "blocked"}]}"#;
        assert_eq!(
            serde_json::from_str::<Roadmap>(newer)
                .map(|r| r.items[0].status)
                .ok(),
            Some(Status::Unknown)
        );
    }

    #[test]
    fn an_item_in_a_column_this_build_does_not_know_is_in_none_it_shows() {
        let newer = r#"{"milestone": "9.0.0", "next_milestone": "9.1.0", "items": [
            {"id": "roadmap-someday-a", "title": "A", "blurb": "", "horizon": "someday"}]}"#;
        let roadmap: Roadmap = serde_json::from_str(newer).unwrap();
        assert_eq!(
            Horizon::ALL.map(|horizon| roadmap.column(horizon).count()),
            [0, 0, 0]
        );
    }

    #[test]
    fn a_release_build_that_fetched_main_s_roadmap_says_so() {
        assert!(says_main(true, false));
    }

    #[test]
    fn a_release_build_showing_its_own_roadmap_says_nothing() {
        assert!(!says_main(false, false));
    }

    #[test]
    fn the_dev_build_never_says_it_is_main_s() {
        assert!(!says_main(true, true));
    }

    fn themed(themes: &[(&str, &str)]) -> Roadmap {
        Roadmap {
            milestone: "0.4.0".into(),
            next_milestone: "0.5.0".into(),
            themes: themes
                .iter()
                .map(|(v, t)| (v.to_string(), t.to_string()))
                .collect(),
            ..Roadmap::built()
        }
    }

    #[test]
    fn now_shows_its_milestone_s_theme() {
        assert_eq!(
            themed(&[("0.4.0", "Architecture review")]).theme(Horizon::Now),
            Some("Architecture review")
        );
    }

    #[test]
    fn next_shows_its_milestone_s_theme() {
        assert_eq!(
            themed(&[("0.5.0", "Reach beyond Rust users")]).theme(Horizon::Next),
            Some("Reach beyond Rust users")
        );
    }

    #[test]
    fn a_theme_for_a_release_no_column_is_building_toward_is_not_shown() {
        // The milestones moved on; 0.3.0's theme does not follow them.
        assert_eq!(
            themed(&[("0.3.0", "Drop-in and readable")]).theme(Horizon::Now),
            None
        );
    }

    #[test]
    fn a_blank_theme_is_none() {
        assert_eq!(themed(&[("0.4.0", "  ")]).theme(Horizon::Now), None);
    }

    #[test]
    fn later_has_no_theme() {
        assert_eq!(themed(&[("0.4.0", "x")]).theme(Horizon::Later), None);
    }

    #[test]
    fn the_built_roadmap_s_now_and_next_have_themes() {
        let roadmap = Roadmap::built();
        assert_eq!(
            [Horizon::Now, Horizon::Next].map(|h| roadmap.theme(h).is_some()),
            [true, true]
        );
    }
}
