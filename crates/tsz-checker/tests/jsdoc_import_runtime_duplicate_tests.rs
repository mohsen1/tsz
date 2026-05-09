//! Issue #3508: a runtime ES `import` and a JSDoc `@import` for the same
//! local name in the same JS file must report TS2300 "Duplicate identifier"
//! at *each* occurrence — not TS18042 on the runtime import.
//!
//! Before the fix, the JSDoc `@import` redeclared the local symbol with
//! `is_type_only = true`, which the checker then read back when validating
//! the runtime import and incorrectly classified it as a type-only JS
//! import. The fix:
//!
//! 1. Binder: do not redeclare a name in `bind_jsdoc_import_tags` when an
//!    alias for that name already exists in the file scope. The runtime
//!    alias remains the value-bearing binding for the file.
//! 2. Checker: extend the JSDoc duplicate-import pass to detect a JSDoc
//!    `@import` whose local name collides with a runtime ES import in the
//!    same file, and emit TS2300 at both positions.

use tsz_checker::test_utils::check_js_source_diagnostics;

#[test]
fn runtime_named_import_and_jsdoc_import_emit_ts2300_at_both_positions() {
    let source = r#"import { Foo } from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let ts2300_count = codes.iter().filter(|&&c| c == 2300).count();
    assert_eq!(
        ts2300_count, 2,
        "expected exactly two TS2300 (one at runtime import, one at JSDoc \
         @import); got codes={codes:?}",
    );
    assert!(
        !codes.contains(&18042),
        "TS18042 must not fire when a JSDoc @import duplicates a runtime \
         import: codes={codes:?}",
    );
}

#[test]
fn runtime_default_import_and_jsdoc_import_emit_ts2300_at_both_positions() {
    let source = r#"import Foo from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let ts2300_count = codes.iter().filter(|&&c| c == 2300).count();
    assert_eq!(
        ts2300_count, 2,
        "expected exactly two TS2300 for default-import + JSDoc collision; \
         got codes={codes:?}",
    );
    assert!(
        !codes.contains(&18042),
        "TS18042 must not fire on the runtime default import: codes={codes:?}",
    );
}

#[test]
fn jsdoc_import_alone_does_not_emit_ts2300() {
    // Sanity: a JSDoc `@import` with no runtime import for the same name
    // must not emit TS2300.
    let source = r#"/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2300),
        "no duplicate, so TS2300 must not fire: codes={codes:?}",
    );
}

#[test]
fn duplicate_jsdoc_imports_still_emit_ts2300() {
    // Sanity: two JSDoc `@import` tags for the same name must still emit
    // TS2300 at both positions (regression guard for the existing
    // JSDoc-vs-JSDoc detection).
    let source = r#"/** @import { Foo } from "./types.js" */
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let ts2300_count = codes.iter().filter(|&&c| c == 2300).count();
    assert_eq!(
        ts2300_count, 2,
        "two JSDoc @imports of the same name must each emit TS2300: \
         codes={codes:?}",
    );
}

#[test]
fn renamed_runtime_import_and_jsdoc_import_emit_ts2300_at_local_name() {
    // `import { Bar as Foo }` introduces local name `Foo`; a JSDoc
    // `@import { Foo }` collides on that local name.
    let source = r#"import { Bar as Foo } from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let ts2300_count = codes.iter().filter(|&&c| c == 2300).count();
    assert_eq!(
        ts2300_count, 2,
        "renamed runtime import collides with JSDoc @import on local name; \
         got codes={codes:?}",
    );
    assert!(
        !codes.contains(&18042),
        "TS18042 must not fire: codes={codes:?}",
    );
}

#[test]
fn distinct_runtime_and_jsdoc_imports_do_not_collide() {
    // Sanity: different local names for runtime and JSDoc imports must
    // not produce a spurious TS2300.
    let source = r#"import { Bar } from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diags = check_js_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2300),
        "distinct names must not collide: codes={codes:?}",
    );
}
