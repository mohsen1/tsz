//! Shared text predicates for scanning third-party `.d.ts` files on disk.
//!
//! These helpers exist for one narrow situation: deciding export-ness of a
//! declaration that lives in an external dependency's emitted `.d.ts`, for
//! which tsz has no in-memory AST or symbol graph (the file is read straight
//! from disk via `std::fs::read_to_string`). For declarations that tsz parsed
//! itself, the idiomatic check is the AST modifier list
//! (`stmt_has_export_modifier`) — never these text predicates.
//!
//! Centralizing the `export `-prefix scan removes the copy-pasted predicate
//! that previously lived inline at each disk-reading call site, so the brittle
//! string contract is stated once.

/// A `.d.ts` line begins an exported declaration when, after trimming leading
/// whitespace, it starts with the `export ` keyword (the trailing space rules
/// out the `exports` identifier). Trailing whitespace and a trailing `;` do not
/// affect the prefix, so callers may pass either a raw or pre-trimmed line.
pub(crate) fn dts_line_has_export_prefix(line: &str) -> bool {
    line.trim_start().starts_with("export ")
}

/// Whether a `.d.ts` source re-exports an entire module via an
/// `export * from` clause anywhere in the file. Used to decide whether a
/// package root forwards its members through a wildcard re-export.
pub(crate) fn dts_text_has_export_star(text: &str) -> bool {
    text.contains("export * from")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_prefix_matches_leading_keyword_only() {
        assert!(dts_line_has_export_prefix("export const x = 1;"));
        assert!(dts_line_has_export_prefix("    export function f(): void;"));
        assert!(dts_line_has_export_prefix("export { a, b } from \"./m\";"));
        // `exports` (CJS identifier) must not be mistaken for the keyword.
        assert!(!dts_line_has_export_prefix("exports.foo = 1;"));
        assert!(!dts_line_has_export_prefix("const exported = 1;"));
        assert!(!dts_line_has_export_prefix("declare const x: number;"));
    }

    #[test]
    fn export_star_detects_wildcard_reexport() {
        assert!(dts_text_has_export_star(
            "export * from \"./a\";\nexport * from \"./b\";"
        ));
        assert!(!dts_text_has_export_star("export { a } from \"./a\";"));
        assert!(!dts_text_has_export_star("declare const x: number;"));
    }
}
