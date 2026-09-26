use serde_json::{Value, json};

use crate::text::Message;

#[derive(Debug, Default)]
pub(crate) struct WireInput {
    pub(crate) hook_event_name: String,
    pub(crate) tool_name: String,
    pub(crate) file_path: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) stop_hook_active: bool,
    pub(crate) background_tasks: usize,
}

pub(crate) fn read(text: &str) -> Result<WireInput, Message> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| Message::new(format!("hook input is not JSON: {error}")))?;
    let field = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_owned);
    let input = |name: &str| {
        value
            .get("tool_input")
            .and_then(|tool| tool.get(name))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Ok(WireInput {
        hook_event_name: field("hook_event_name").unwrap_or_default(),
        tool_name: field("tool_name").unwrap_or_default(),
        file_path: input("file_path").or_else(|| input("notebook_path")),
        command: input("command"),
        stop_hook_active: value
            .get("stop_hook_active")
            .and_then(Value::as_bool)
            .unwrap_or_default(),
        background_tasks: value
            .get("background_tasks")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    })
}

pub(crate) fn deny(reason: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

pub(crate) fn block(reason: &str) -> String {
    json!({ "decision": "block", "reason": reason }).to_string()
}

pub(crate) fn context(event: &str, text: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": text,
        }
    })
    .to_string()
}

pub(crate) fn notice(text: &str) -> String {
    json!({ "systemMessage": text }).to_string()
}
