use tsz_common::diagnostics::data::diagnostic_messages;

/// Returns true when `tsc 6.0.3` appends the migration-URL note.
///
/// `tsc` only chains "Visit https://aka.ms/ts6" for the deprecated entries
/// whose `createDeprecatedDiagnostic` call receives the TS5111 related message.
/// Most TS 6 deprecations, including `allowSyntheticDefaultImports=false`,
/// `alwaysStrict=false`, and `moduleResolution=classic`, do not get the suffix.
pub(super) fn option_has_migration_url(option_key: &str, option_value: Option<&str>) -> bool {
    matches!(
        (option_key, option_value),
        ("baseUrl", None) | ("moduleResolution", Some("node10"))
    )
}

/// Appends the TS5111 migration-URL sentence to a TS5107/TS5101 base message,
/// matching the `flattenDiagnosticMessageText("\n  ")` output produced by tsc.
/// Use [`maybe_with_migration_url`] when the option may or may not have a URL.
pub(super) fn with_migration_url(base: String) -> String {
    format!(
        "{}\n  {}",
        base,
        diagnostic_messages::VISIT_HTTPS_AKA_MS_TS6_FOR_MIGRATION_INFORMATION
    )
}

/// Appends the migration URL only when `tsc 6.0.3` would do so.
pub(super) fn maybe_with_migration_url(
    base: String,
    option_key: &str,
    option_value: Option<&str>,
) -> String {
    if option_has_migration_url(option_key, option_value) {
        with_migration_url(base)
    } else {
        base
    }
}
