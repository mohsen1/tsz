//! Locks in that `parse_jsdoc_typedefs` handles `@import` clauses split
//! across multiple JSDoc lines.
//!
//! Regression: `importTag8.ts` —
//!     /**
//!      * @import
//!      * { A, B }
//!      * from "./types"
//!      */
//! tsz emitted false TS2304 for both A and B because the line-based
//! tag parser only saw `@import` (with empty rest) on the first line
//! and silently registered nothing. The fix collapses continuation
//! lines onto the `@import` line before tag-level parsing via the
//! new `merge_jsdoc_import_continuations` preprocessor.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn jsdoc_import_split_across_lines_resolves_names() {
    let source = r#"
declare interface A { a: number; }
declare interface B { a: number; }

/**
 * @import
 * { A, B }
 * from "./types"
 */

/**
 * @param { A } a
 * @param { B } b
 */
function f(a, b) {}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "multi-line @import should resolve A and B; got {codes:?}",
    );
}

#[test]
fn jsdoc_single_line_import_still_works() {
    let source = r#"
declare interface A { a: number; }

/** @import { A } from "./types" */
/** @param { A } a */
function g(a) {}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "single-line @import should not regress; got {codes:?}",
    );
}
