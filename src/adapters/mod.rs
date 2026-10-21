//! Operating-system and Herdr integration boundaries.
//!
//! Feature workflows use these modules instead of reaching external systems
//! directly. The implementations currently delegate to the corresponding
//! legacy modules; task 6.1 will switch module ownership once all call sites
//! have moved to feature ports.

pub(crate) mod git;
pub(crate) mod herdr;
pub(crate) mod process;
pub(crate) mod state;
pub(crate) mod terminal;
