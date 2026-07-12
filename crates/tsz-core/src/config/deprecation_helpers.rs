use serde_json::{Map, Value};
use tsz_common::diagnostics::data::{diagnostic_codes, diagnostic_messages};
use tsz_common::diagnostics::format_message;

use super::normalize_option;

/// A compiler option or option value that TypeScript 7 rejects after parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RemovedOptionNotice {
    key: &'static str,
    value: Option<&'static str>,
}

impl RemovedOptionNotice {
    pub(super) const fn key(self) -> &'static str {
        self.key
    }

    pub(super) const fn code(self) -> u32 {
        if self.value.is_some() {
            diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2
        } else {
            diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
        }
    }

    pub(super) const fn is_value(self) -> bool {
        self.value.is_some()
    }

    pub(super) fn message(self) -> String {
        let base = if let Some(value) = self.value {
            format_message(
                diagnostic_messages::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2,
                &[self.key, value],
            )
        } else {
            format_message(
                diagnostic_messages::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                &[self.key],
            )
        };

        if self.key == "baseUrl" {
            append_related_message(
                base,
                format_message(
                    diagnostic_messages::USE_INSTEAD,
                    &[r#""paths": {"*": ["./*"]}"#],
                ),
            )
        } else {
            base
        }
    }
}

/// Returns TS7 removal notices in tsc's declaration order.
///
/// Type-invalid and null values are intentionally skipped here: tsc reports
/// TS5024 for the former and treats the latter as an unset option.
pub(super) fn removed_option_notices_from_json(
    options: &Map<String, Value>,
) -> Vec<RemovedOptionNotice> {
    let mut notices = Vec::new();

    push_removed_bool_value(&mut notices, options, "alwaysStrict");
    push_removed_string_value(&mut notices, options, "target");
    push_removed_string_value(&mut notices, options, "moduleResolution");
    push_removed_key(&mut notices, options, "baseUrl");
    push_removed_bool_value(&mut notices, options, "esModuleInterop");
    push_removed_bool_value(&mut notices, options, "allowSyntheticDefaultImports");
    push_removed_key(&mut notices, options, "outFile");
    push_removed_string_value(&mut notices, options, "module");
    push_removed_key(&mut notices, options, "downlevelIteration");

    notices
}

fn push_removed_key(
    notices: &mut Vec<RemovedOptionNotice>,
    options: &Map<String, Value>,
    key: &'static str,
) {
    let type_is_valid = matches!(
        (key, options.get(key)),
        ("baseUrl" | "outFile", Some(Value::String(_)))
            | ("downlevelIteration", Some(Value::Bool(_)))
    );
    if type_is_valid {
        notices.push(key_notice(key));
    }
}

fn push_removed_bool_value(
    notices: &mut Vec<RemovedOptionNotice>,
    options: &Map<String, Value>,
    key: &'static str,
) {
    if options.get(key) == Some(&Value::Bool(false)) {
        notices.push(value_notice(key, "false"));
    }
}

fn push_removed_string_value(
    notices: &mut Vec<RemovedOptionNotice>,
    options: &Map<String, Value>,
    key: &'static str,
) {
    if let Some(Value::String(value)) = options.get(key)
        && let Some(display) = removed_string_value(key, value)
    {
        notices.push(value_notice(key, display));
    }
}

fn removed_string_value(key: &str, value: &str) -> Option<&'static str> {
    let normalized = normalize_option(value);
    match (key, normalized.as_str()) {
        ("target", "es5") => Some("ES5"),
        ("moduleResolution", "node" | "node10") => Some("node10"),
        ("moduleResolution", "classic") => Some("Classic"),
        ("module", "amd") => Some("AMD"),
        ("module", "system") => Some("System"),
        ("module", "umd") => Some("UMD"),
        _ => None,
    }
}

const fn key_notice(key: &'static str) -> RemovedOptionNotice {
    RemovedOptionNotice { key, value: None }
}

const fn value_notice(key: &'static str, value: &'static str) -> RemovedOptionNotice {
    RemovedOptionNotice {
        key,
        value: Some(value),
    }
}

fn append_related_message(base: String, related: String) -> String {
    format!("{base}\n  {related}")
}
