use tsz_common::diagnostics::data::diagnostic_messages;

/// Returns true when `tsc 6.0.3` appends the migration-URL note for `option_key`.
///
/// `tsc` only chains "Visit https://aka.ms/ts6" when the deprecated option has an
/// active migration target documented in the TS 6 migration guide (module-resolution
/// overhaul, target upgrade). Options that are deprecated without a documented
/// migration path (e.g. `allowSyntheticDefaultImports=false`, `esModuleInterop=false`)
/// do not get the URL suffix.
pub(super) fn option_has_migration_url(option_key: &str) -> bool {
    matches!(
        option_key,
        "moduleResolution" | "module" | "target" | "downlevelIteration"
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

/// Appends the migration URL only when `tsc 6.0.3` would do so for `option_key`.
pub(super) fn maybe_with_migration_url(base: String, option_key: &str) -> String {
    if option_has_migration_url(option_key) {
        with_migration_url(base)
    } else {
        base
    }
}
