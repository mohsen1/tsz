//! Tests for cross-file class declaration merging in JS/checkJs mode.
//!
//! When a class declaration in one JS file has the same name as a variable/function
//! in another JS file (script mode, global scope), the merged symbol must still
//! resolve to a valid class constructor type. This test ensures that cross-arena
//! delegation does not incorrectly return UNKNOWN for the class, which would cause
//! false TS18046 ("is of type 'unknown'") errors.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Check a JS file (file2) in a multi-file project where file1 also exists.
/// Both files share a global scope (script mode, no modules).
/// Returns the error codes emitted for file2.
fn check_two_js_files_codes(file1_source: &str, file2_source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };

    let mut parser1 = ParserState::new("file1.js".to_string(), file1_source.to_string());
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let mut parser2 = ParserState::new("file2.js".to_string(), file2_source.to_string());
    let root2 = parser2.parse_source_file();
    let mut binder2 = BinderState::new();
    binder2.bind_source_file(parser2.get_arena(), root2);

    let arena1 = Arc::new(parser1.get_arena().clone());
    let arena2 = Arc::new(parser2.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena1), Arc::clone(&arena2)]);

    let binder1 = Arc::new(binder1);
    let binder2 = Arc::new(binder2);
    let all_binders = Arc::new(vec![Arc::clone(&binder1), Arc::clone(&binder2)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena2.as_ref(),
        binder2.as_ref(),
        &types,
        "file2.js".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root2);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn cross_file_class_with_var_constructor_no_ts18046() {
    // Regression test: when a constructor function `var Foo = function(){}` exists
    // in file1.js and `class Foo {}` exists in file2.js, `Foo.prop = 0` in file2.js
    // must NOT emit TS18046 ("'Foo' is of type 'unknown'").
    //
    // Root cause: the merged symbol's primary arena pointed to file1.js. Cross-arena
    // delegation sent the resolution to file1.js's checker, which couldn't find a
    // class declaration node and returned TypeId::UNKNOWN.
    let codes = check_two_js_files_codes(
        // file1.js
        r#"var SomeClass = function () {
    this.otherProp = 0;
};
new SomeClass();"#,
        // file2.js
        r#"class SomeClass { }
SomeClass.prop = 0"#,
    );

    assert!(
        !codes.contains(&18046),
        "Expected no TS18046 for cross-file class+constructor merge, got codes: {codes:?}"
    );
}

#[test]
fn cross_file_class_with_var_no_ts18046() {
    // Same pattern but with a plain variable instead of a constructor function.
    let codes = check_two_js_files_codes(
        // file1.js
        "var SomeClass = 42;",
        // file2.js
        r#"class SomeClass { }
SomeClass.prop = 0"#,
    );

    assert!(
        !codes.contains(&18046),
        "Expected no TS18046 for cross-file class+var merge, got codes: {codes:?}"
    );
}

#[test]
fn single_file_class_property_access_no_ts18046() {
    // Sanity check: a single-file class with property access works fine.
    let codes = check_two_js_files_codes(
        // file1.js (empty)
        "",
        // file2.js
        r#"class SomeClass { }
SomeClass.prop = 0"#,
    );

    assert!(
        !codes.contains(&18046),
        "Expected no TS18046 for single-file class property access, got codes: {codes:?}"
    );
}
