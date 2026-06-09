//! JSDoc `import(...)` resolution-mode attribute parsing for `CheckerState`.
//!
//! Split out of `jsdoc/parsing.rs` to keep that file within its size budget.
//! These helpers let JSDoc `import("m", { with: { "resolution-mode": ... } }).X`
//! type queries parse and resolve under the requested ESM/CJS condition, the
//! inline counterpart to the `@import ... with { ... }` tag form.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Skip an optional `, { ... }` import-attributes argument that may follow
    /// the module specifier inside an `import("m"<here>)` type expression,
    /// returning the slice starting at the closing `)`. When no such argument
    /// is present the input is returned unchanged.
    pub(super) fn skip_jsdoc_import_attributes_argument(after_quote: &str) -> &str {
        let Some(after_comma) = after_quote.strip_prefix(',') else {
            return after_quote;
        };
        let after_comma = after_comma.trim_start();
        if !after_comma.starts_with('{') {
            return after_quote;
        }
        let bytes = after_comma.as_bytes();
        let mut depth = 0usize;
        for (idx, &b) in bytes.iter().enumerate() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return after_comma[idx + 1..].trim_start();
                    }
                }
                _ => {}
            }
        }
        // Unbalanced braces: leave the original input untouched so the caller's
        // `)` expectation fails cleanly rather than silently mis-parsing.
        after_quote
    }

    /// Read the `resolution-mode` import attribute out of an
    /// `import("m", { with: { "resolution-mode": "import" } }).Member` type
    /// expression. Returns the resolver override, or `None` when absent.
    pub(super) fn jsdoc_import_type_resolution_mode(
        type_expr: &str,
    ) -> Option<crate::context::ResolutionModeOverride> {
        use crate::context::ResolutionModeOverride;

        let attr_idx = type_expr.find("resolution-mode")?;
        let after = type_expr[attr_idx + "resolution-mode".len()..]
            .trim_start()
            .trim_start_matches(['"', '\'']);
        let after = after.trim_start().strip_prefix(':')?.trim_start();
        let quote = after.chars().next().filter(|&c| c == '"' || c == '\'')?;
        let value = after[quote.len_utf8()..].split(quote).next()?;
        match value {
            "import" => Some(ResolutionModeOverride::Import),
            "require" => Some(ResolutionModeOverride::Require),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::CheckerState;

    #[test]
    fn import_type_tolerates_resolution_mode_attributes_argument() {
        // The inline `import("m", { ... }).Member` type carries an attributes
        // argument; `parse_jsdoc_import_type` must skip it and still recover the
        // specifier and member name.
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(
                r#"import("foo", { with: { "resolution-mode": "import" } }).Member"#
            ),
            Some(("foo".to_string(), Some("Member".to_string())))
        );
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(
                r#"import("foo", { with: { "resolution-mode": "require" } })"#
            ),
            Some(("foo".to_string(), None))
        );
    }

    #[test]
    fn import_type_reads_resolution_mode_override() {
        use crate::context::ResolutionModeOverride;
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(
                r#"import("foo", { with: { "resolution-mode": "import" } }).Member"#
            ),
            Some(ResolutionModeOverride::Import)
        );
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(
                r#"import("foo", { with: { "resolution-mode": "require" } }).Member"#
            ),
            Some(ResolutionModeOverride::Require)
        );
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(r#"import("foo").Member"#),
            None
        );
    }
}
