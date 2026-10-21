//! Session result precedence.

/// What to do with the result after the TUI exits.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Deliver,
    NoChanges,
    NoAgent,
    Failed(i32),
    Killed,
}

/// Preserves the original precedence: signal, nonzero exit, no changes,
/// missing agent, then delivery.
pub(crate) fn decide(code: Option<i32>, summary_text: &str, has_agent: bool) -> Outcome {
    let Some(code) = code else {
        return Outcome::Killed;
    };
    if code != 0 {
        return Outcome::Failed(code);
    }
    if summary_text.trim().is_empty() {
        return Outcome::NoChanges;
    }
    if has_agent {
        Outcome::Deliver
    } else {
        Outcome::NoAgent
    }
}
