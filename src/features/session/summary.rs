//! Summary generation and bounded agent prompt formatting.

use std::path::Path;

use crate::features::session::{
    ports::{Git, Snapshot},
    summary_engine::{CommitLine, GitQueries},
};

struct Queries<'a>(&'a dyn Git);

impl GitQueries for Queries<'_> {
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Option<bool> {
        self.0.is_ancestor(ancestor, descendant)
    }

    fn log_range(&self, from: &str, to: &str) -> Option<Vec<CommitLine>> {
        self.0.log_range(from, to)
    }

    fn has_merges_in_range(&self, from: &str, to: &str) -> Option<bool> {
        self.0.has_merges_in_range(from, to)
    }

    fn log_head(&self, head: &str, count: usize) -> Option<Vec<CommitLine>> {
        self.0.log_head(head, count)
    }
}

pub(crate) fn generate(git: &dyn Git, before: &Snapshot, after: &Snapshot) -> String {
    crate::features::session::summary_engine::summary(before, after, &Queries(git))
}

pub(crate) fn prompt(tool_id: &str, summary_text: &str, kept_path: &Path) -> String {
    const MAX_PROMPT_BYTES: usize = 65536;
    let text = format!("{tool_id} session summary:\n\n{}", summary_text.trim());
    if text.len() <= MAX_PROMPT_BYTES {
        return text;
    }
    let mut cut = MAX_PROMPT_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut capped = text[..cut].to_string();
    capped.push_str(&format!(
        "\n\n(output truncated; full summary kept at {})",
        kept_path.display()
    ));
    capped
}
