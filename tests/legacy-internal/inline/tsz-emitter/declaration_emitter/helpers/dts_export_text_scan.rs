//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/dts_export_text_scan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2f5ae11ab41858a32d36e9e938b6a8bfe9ce62e4cf69c29e3bb8893ed18f4ba0 33 export_prefix_matches_leading_keyword_only
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
// TSZ_INLINE_TEST_END 2f5ae11ab41858a32d36e9e938b6a8bfe9ce62e4cf69c29e3bb8893ed18f4ba0

// TSZ_INLINE_TEST_BEGIN 80cea40a2ddc5c3fda92bfdba17f0e1426981c7fe01d31acd042328bfe931130 44 export_star_detects_wildcard_reexport
    #[test]
    fn export_star_detects_wildcard_reexport() {
        assert!(dts_text_has_export_star(
            "export * from \"./a\";\nexport * from \"./b\";"
        ));
        assert!(!dts_text_has_export_star("export { a } from \"./a\";"));
        assert!(!dts_text_has_export_star("declare const x: number;"));
    }
// TSZ_INLINE_TEST_END 80cea40a2ddc5c3fda92bfdba17f0e1426981c7fe01d31acd042328bfe931130
