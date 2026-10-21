//! Validated context required to route an action.

use super::context_json;

#[derive(Debug)]
pub(crate) struct ActionContext {
    pub(crate) cwd: String,
    pub(crate) workspace_id: Option<String>,
    pub(crate) caller_pane: Option<String>,
    pub(crate) caller_agent: Option<String>,
    pub(crate) caller_tab: Option<String>,
}

impl ActionContext {
    pub(crate) fn from_environment() -> Result<Self, String> {
        let json = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "HERDR_PLUGIN_CONTEXT_JSON is not set; invoke this action from herdr".to_string()
            })?;
        let parsed = context_json::parse(&json)?;
        Ok(Self {
            cwd: parsed.dir,
            workspace_id: parsed
                .workspace_id
                .or_else(|| env_non_blank("HERDR_WORKSPACE_ID")),
            caller_pane: parsed
                .focused_pane_id
                .or_else(|| env_non_blank("HERDR_PANE_ID")),
            caller_agent: parsed.focused_pane_agent,
            caller_tab: parsed.tab_id,
        })
    }
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
