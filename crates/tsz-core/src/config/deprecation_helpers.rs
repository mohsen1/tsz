use tsz_common::diagnostics::data::diagnostic_messages;

/// Appends the TS5111 migration-URL sentence to a TS5107/TS5101 base message,
/// matching the `flattenDiagnosticMessageText("\n  ")` output produced by tsc.
pub(super) fn with_migration_url(base: String) -> String {
    format!(
        "{}\n  {}",
        base,
        diagnostic_messages::VISIT_HTTPS_AKA_MS_TS6_FOR_MIGRATION_INFORMATION
    )
}
