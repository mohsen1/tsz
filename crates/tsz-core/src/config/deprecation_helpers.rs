use tsz_common::diagnostics::data::diagnostic_messages;

/// Returns true when `tsc 6.0.3` chains the TS5111 migration-URL note
/// (Visit <https://aka.ms/ts6> for migration information.) onto the TS5107/TS5101
/// message for the given deprecated `(option_key, display_value)` pair.
///
/// `tsc` decides this per deprecation, not per option name: it passes the URL as the
/// `related` chain argument in only two of the 6.0-wave deprecations. This mirrors
/// `checkDeprecations("6.0", "7.0", ...)` inside `verifyDeprecatedCompilerOptions`
/// (TypeScript 6.0.3 `commandLineParser`/`program`). The full wave and its URL state:
///
/// | deprecation                       | TS code | migration URL |
/// |-----------------------------------|---------|---------------|
/// | `moduleResolution=node10`         | TS5107  | yes           |
/// | `baseUrl`                         | TS5101  | yes           |
/// | `moduleResolution=classic`        | TS5107  | no            |
/// | `target=ES5`                      | TS5107  | no            |
/// | `module=None`/`AMD`/`UMD`/`System`| TS5107  | no            |
/// | `alwaysStrict=false`              | TS5107  | no            |
/// | `esModuleInterop=false`           | TS5107  | no            |
/// | `allowSyntheticDefaultImports=false` | TS5107 | no          |
/// | `outFile`                         | TS5101  | no            |
/// | `downlevelIteration`              | TS5101  | no            |
///
/// `display_value` is the rendered option value for TS5107 (e.g. `"node10"`,
/// `"classic"`) and `None` for the key-only TS5101 deprecations.
pub(super) fn option_has_migration_url(option_key: &str, display_value: Option<&str>) -> bool {
    matches!(
        (option_key, display_value),
        ("moduleResolution", Some("node10")) | ("baseUrl", None)
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

/// Appends the migration URL only when `tsc 6.0.3` would do so for the
/// `(option_key, display_value)` deprecation.
pub(super) fn maybe_with_migration_url(
    base: String,
    option_key: &str,
    display_value: Option<&str>,
) -> String {
    if option_has_migration_url(option_key, display_value) {
        with_migration_url(base)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact TS6-wave table from `tsc 6.0.3`. Each row is
    /// `(option_key, display_value, expects_url)` and mirrors a
    /// `createDeprecatedDiagnostic(...)` call in `checkDeprecations("6.0", "7.0")`.
    const TS6_DEPRECATION_URL_MATRIX: &[(&str, Option<&str>, bool)] = &[
        // The only two deprecations tsc chains the migration URL onto.
        ("moduleResolution", Some("node10"), true),
        ("baseUrl", None, true),
        // Everything else in the 6.0 wave is deprecated without the URL.
        ("moduleResolution", Some("classic"), false),
        ("target", Some("ES5"), false),
        ("module", Some("None"), false),
        ("module", Some("AMD"), false),
        ("module", Some("UMD"), false),
        ("module", Some("System"), false),
        ("alwaysStrict", Some("false"), false),
        ("esModuleInterop", Some("false"), false),
        ("allowSyntheticDefaultImports", Some("false"), false),
        ("outFile", None, false),
        ("downlevelIteration", None, false),
    ];

    #[test]
    fn migration_url_matches_tsc_6_0_3_table() {
        for &(key, value, expected) in TS6_DEPRECATION_URL_MATRIX {
            assert_eq!(
                option_has_migration_url(key, value),
                expected,
                "migration URL decision for ({key}, {value:?}) must match tsc 6.0.3"
            );
        }
    }

    #[test]
    fn maybe_with_migration_url_appends_only_when_expected() {
        let base = "Option 'x' is deprecated.".to_string();
        assert!(
            maybe_with_migration_url(base.clone(), "moduleResolution", Some("node10"))
                .contains("aka.ms/ts6")
        );
        assert!(!maybe_with_migration_url(base, "target", Some("ES5")).contains("aka.ms/ts6"));
    }
}
