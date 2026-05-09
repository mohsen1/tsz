//! Regression tests for [issue #3508]: a runtime ES `import { Foo }` and a
//! JSDoc `@import { Foo }` declaring the same local name in the same JS file
//! must produce a TS2300 "Duplicate identifier" at *each* occurrence — not a
//! TS18042 ("type-only import in JS file") on the runtime side.
//!
//! The structural rule under test is:
//!
//! > When a runtime ES `import { X }` (or default-import / namespace-import)
//! > and a JSDoc `@import { X }` introduce the same local name in the same
//! > JS file, tsc emits TS2300 at each occurrence; tsz must do the same and
//! > must not let the JSDoc tag corrupt the runtime alias's type-only-ness.
//!
//! [issue #3508]: https://github.com/mohsen1/tsz/issues/3508

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_js_and_get_diagnostics(file_name: &str, source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.allow_js = true;
    options.check_js = true;
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count_code(diagnostics: &[(u32, String)], code: u32) -> usize {
    diagnostics.iter().filter(|(c, _)| *c == code).count()
}

#[test]
fn runtime_named_import_and_jsdoc_import_emit_ts2300_at_both_positions() {
    let source = r#"import { Foo } from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        2,
        "expected TS2300 at runtime and JSDoc positions; got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 18042),
        0,
        "TS18042 must not fire when a runtime alias already exists for the name; got: {diagnostics:?}"
    );
}

#[test]
fn runtime_default_import_and_jsdoc_import_emit_ts2300_at_both_positions() {
    let source = r#"import Foo from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        2,
        "expected TS2300 at default-import and JSDoc positions; got: {diagnostics:?}"
    );
    assert_eq!(count_code(&diagnostics, 18042), 0);
}

#[test]
fn renamed_runtime_import_collides_on_local_name_with_jsdoc_import() {
    // `import { Bar as Foo }` introduces `Foo` locally; JSDoc `@import { Foo }`
    // also introduces `Foo`. The duplicate is on the local name, not the
    // exported one. The structural rule is independent of the user-chosen
    // exported name: this test uses `Bar`/`Foo`; renaming to `Baz`/`Qux`
    // would still produce two TS2300s on the local `Qux`.
    let source = r#"import { Bar as Foo } from "./types.js";
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        2,
        "expected TS2300 at the local name `Foo` and at the JSDoc occurrence; got: {diagnostics:?}"
    );
}

#[test]
fn duplicate_jsdoc_imports_still_emit_ts2300() {
    // Regression guard for the existing JSDoc/JSDoc detection path: two JSDoc
    // `@import { Foo }` tags in the same file still produce TS2300 at each.
    let source = r#"/** @import { Foo } from "./types.js" */
/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        2,
        "expected TS2300 at both JSDoc positions; got: {diagnostics:?}"
    );
}

#[test]
fn lone_jsdoc_import_does_not_emit_ts2300() {
    // Sanity: a single JSDoc `@import` alone is not a duplicate.
    let source = r#"/** @import { Foo } from "./types.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        0,
        "no TS2300 expected for a single JSDoc @import; got: {diagnostics:?}"
    );
}

#[test]
fn distinct_runtime_and_jsdoc_import_names_do_not_collide() {
    // Sanity: different local names introduced by the runtime import and
    // the JSDoc `@import` must NOT trigger TS2300.
    let source = r#"import { Foo } from "./types.js";
/** @import { Bar } from "./other.js" */
export const value = 1;
"#;
    let diagnostics = compile_js_and_get_diagnostics("a.js", source);
    assert_eq!(
        count_code(&diagnostics, 2300),
        0,
        "no TS2300 expected for distinct names; got: {diagnostics:?}"
    );
}
