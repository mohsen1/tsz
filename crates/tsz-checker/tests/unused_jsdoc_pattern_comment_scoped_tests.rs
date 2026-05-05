//! Regression coverage for #2835: the `noUnusedLocals` JSDoc-reference
//! probe used to scan the entire source text for tag patterns like
//! `@type {Hidden}` or `@import { AlsoHidden }`. Plain string literals
//! that happened to contain those tokens silently suppressed TS6196
//! because the helper could not distinguish a real comment from
//! arbitrary code or strings.
//!
//! These tests pin the comment-scoped behavior: a JSDoc-looking match
//! inside a string literal (or single-line comment, or template
//! literal) must not count, while a real block-comment reference still
//! does.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_with_no_unused_locals(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        no_unused_locals: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn jsdoc_type_tag_in_string_literal_does_not_suppress_ts6196() {
    let source = r#"
export {};

type Hidden = { value: number };

const marker = "@type {Hidden}";

marker;
"#;
    let codes = check_with_no_unused_locals(source);
    assert!(
        codes.contains(&6196),
        "Expected TS6196 for `Hidden`; substring inside a string literal must not suppress it. Got: {codes:?}"
    );
}

#[test]
fn jsdoc_import_tag_split_across_string_literals_does_not_suppress_ts6196() {
    let source = r#"
export {};

type AlsoHidden = { value: string };

const importMarker = "anything with @import and { AlsoHidden } in a string";

importMarker;
"#;
    let codes = check_with_no_unused_locals(source);
    assert!(
        codes.contains(&6196),
        "Expected TS6196 for `AlsoHidden`; @import + brace match must require both inside an actual comment. Got: {codes:?}"
    );
}

#[test]
fn jsdoc_type_tag_in_line_comment_does_not_suppress_ts6196() {
    // Line comments cannot host JSDoc tags per tsc's grammar; substring
    // mentions in `// @type {X}` must not opt the symbol back in.
    let source = r#"
export {};

type Hidden = { value: number };

// example: @type {Hidden}
"#;
    let codes = check_with_no_unused_locals(source);
    assert!(
        codes.contains(&6196),
        "Expected TS6196 for `Hidden`; line comment substring must not suppress it. Got: {codes:?}"
    );
}

#[test]
fn jsdoc_type_tag_in_block_comment_still_suppresses_ts6196() {
    // Sanity: a real `/** ... */` reference still counts as a usage.
    let source = r#"
export {};

type Used = { value: number };

/** @type {Used} */
let _x: any;
"#;
    let codes = check_with_no_unused_locals(source);
    assert!(
        !codes.contains(&6196),
        "Expected NO TS6196 for `Used`; a real block-comment @type reference must still suppress it. Got: {codes:?}"
    );
}

#[test]
fn unrelated_unused_type_still_reports_ts6196() {
    // The other two suppressions must NOT mask a genuinely unused alias.
    let source = r#"
export {};

type StillUnused = { value: boolean };
"#;
    let codes = check_with_no_unused_locals(source);
    assert!(
        codes.contains(&6196),
        "Expected TS6196 for `StillUnused`. Got: {codes:?}"
    );
}
