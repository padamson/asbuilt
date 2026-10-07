//! The drift check: the committed model against a fresh survey, as a
//! unified diff when they differ. Line endings are normalized first so
//! a consumer with `autocrlf` gets a clean check.

use similar::TextDiff;

/// What the comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Current,
    /// A unified diff from the committed text to the fresh one.
    Drift(String),
}

pub(crate) fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Compare the committed text with the fresh survey; `label` is the
/// path shown in the diff header.
pub fn compare(committed: &str, fresh: &str, label: &str) -> Outcome {
    let committed = normalize(committed);
    let fresh = normalize(fresh);
    if committed == fresh {
        return Outcome::Current;
    }
    let diff = TextDiff::from_lines(&committed, &fresh)
        .unified_diff()
        .header(&format!("a/{label}"), &format!("b/{label}"))
        .to_string();
    Outcome::Drift(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_is_current() {
        assert_eq!(compare("a\nb\n", "a\nb\n", "m.c4"), Outcome::Current);
    }

    #[test]
    fn crlf_in_the_committed_text_is_not_drift() {
        assert_eq!(compare("a\r\nb\r\n", "a\nb\n", "m.c4"), Outcome::Current);
    }

    #[test]
    fn a_changed_line_is_drift_with_both_versions_and_the_label_in_the_header() {
        match compare("a\nb\nc\n", "a\nB\nc\n", "docs/model.c4") {
            Outcome::Drift(diff) => {
                assert!(
                    diff.starts_with("--- a/docs/model.c4\n+++ b/docs/model.c4\n"),
                    "{diff}"
                );
                assert!(diff.contains("\n-b\n"), "{diff}");
                assert!(diff.contains("\n+B\n"), "{diff}");
            }
            other => panic!("expected Drift, got {other:?}"),
        }
    }

    #[test]
    fn an_added_line_is_drift() {
        assert!(matches!(compare("a\n", "a\nb\n", "m"), Outcome::Drift(_)));
    }
}
