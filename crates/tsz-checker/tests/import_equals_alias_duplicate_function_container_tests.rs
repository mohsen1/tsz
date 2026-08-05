//! Two `import x = ...` aliases sharing a name inside one function-like body
//! or class static block report TS2300 at each of them.
//!
//! `tsc`'s binder keeps two cursors while binding: `container` and
//! `blockScopeContainer`. An alias is not block-scoped, so a position-invalid
//! `import x = ...` is recorded in the enclosing *container* — and a function
//! body, method body, constructor body, accessor body, function-expression
//! body, arrow body and `static { }` block are all containers. A second alias
//! of the same name in that container is a redeclaration, so `declareSymbol`
//! reports TS2300 for both, alongside the TS1232 each one already earns for
//! being position-invalid.
//!
//! #16429/#16435 taught `check_import_alias_duplicates` to recurse *through*
//! transparent block-like statements, but the scan is still only ever invoked
//! with a source file's or a namespace body's statement list — it has no call
//! site at a function body or static block at all. So two aliases directly
//! inside `function f() { ... }` were grouped by nothing and reported TS1232
//! alone. `check_import_alias_duplicates_in_nested_containers` closes that by
//! grouping each alias under its nearest declaration container rather than
//! adding a call site per container kind: arrow and function-expression bodies
//! hang off *expressions*, so no statement-list-driven pass reaches them.
//!
//! All expectations were measured against `typescript@7.0.2` with
//! `--noEmit --strict --pretty false --target es2015 --module commonjs`,
//! matching the harness options below.
//!
//! ## Deliberately not asserted here
//!
//! TS2307 counts. Inside a function body `tsc` resolves no module specifier at
//! all for these aliases, and `tsz` keeps a distinct symbol per colliding alias
//! declaration (the #16410 item 2 / #16411 alias-merge residual, which predates
//! this fix). These rows assert the TS2300 and TS1232 counts, which is exactly
//! the behaviour this fix owns.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    let mut codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

/// Assert the oracle-pinned TS2300 and TS1232 counts for one row.
fn assert_counts(source: &str, duplicates: usize, position_invalid: usize, what: &str) {
    let codes = check(source);
    assert_eq!(
        count(&codes, 2300),
        duplicates,
        "{what}: expected {duplicates} TS2300, got {codes:?}"
    );
    assert_eq!(
        count(&codes, 1232),
        position_invalid,
        "{what}: expected {position_invalid} TS1232, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive: every function-like container, plus the class static block.
// ---------------------------------------------------------------------------

#[test]
fn function_declaration_body_aliases_collide() {
    assert_counts(
        r#"function f() { import x = require("nonexistent-a"); import x = require("nonexistent-b"); }
"#,
        2,
        2,
        "function declaration body",
    );
}

#[test]
fn arrow_function_body_aliases_collide() {
    assert_counts(
        r#"const g = () => { import x = require("nonexistent-a"); import x = require("nonexistent-b"); };
"#,
        2,
        2,
        "arrow function body",
    );
}

#[test]
fn function_expression_body_aliases_collide() {
    assert_counts(
        r#"const h = function () { import x = require("nonexistent-a"); import x = require("nonexistent-b"); };
"#,
        2,
        2,
        "function expression body",
    );
}

#[test]
fn method_body_aliases_collide() {
    assert_counts(
        r#"class K { m() { import x = require("nonexistent-a"); import x = require("nonexistent-b"); } }
"#,
        2,
        2,
        "method body",
    );
}

#[test]
fn constructor_body_aliases_collide() {
    assert_counts(
        r#"class K { constructor() { import x = require("nonexistent-a"); import x = require("nonexistent-b"); } }
"#,
        2,
        2,
        "constructor body",
    );
}

#[test]
fn accessor_body_aliases_collide() {
    assert_counts(
        r#"class K { get p(): number { import x = require("nonexistent-a"); import x = require("nonexistent-b"); return 1; } }
"#,
        2,
        2,
        "get accessor body",
    );
}

#[test]
fn static_block_aliases_collide() {
    assert_counts(
        r#"class K { static { import x = require("nonexistent-a"); import x = require("nonexistent-b"); } }
"#,
        2,
        2,
        "class static block",
    );
}

/// The transparent-block recursion #16435 added has to keep working *inside* a
/// function body: the two aliases sit in sibling blocks of one container.
#[test]
fn sibling_blocks_inside_a_function_body_collide() {
    assert_counts(
        r#"function f() { { import x = require("nonexistent-a"); } { import x = require("nonexistent-b"); } }
"#,
        2,
        2,
        "sibling blocks inside a function body",
    );
}

#[test]
fn three_aliases_in_a_function_body_all_flagged() {
    assert_counts(
        r#"function f() { import x = require("nonexistent-a"); import x = require("nonexistent-b"); import x = require("nonexistent-c"); }
"#,
        3,
        3,
        "three colliding aliases in one function body",
    );
}

/// A function body nested in a namespace is still its own container — the
/// namespace-body scan must not be what reports this, and must not double it.
#[test]
fn function_body_inside_a_namespace_collides_once_each() {
    assert_counts(
        r#"namespace M { function f() { import x = require("nonexistent-a"); import x = require("nonexistent-b"); } }
"#,
        2,
        2,
        "function body inside a namespace",
    );
}

/// Binder names must not drive the decision.
#[test]
fn renamed_binders_collide_the_same_way() {
    assert_counts(
        r#"function outer() { import zeta = require("nonexistent-a"); import zeta = require("nonexistent-b"); }
"#,
        2,
        2,
        "renamed colliding binders",
    );
}

// ---------------------------------------------------------------------------
// Negative: container boundaries must still separate.
// ---------------------------------------------------------------------------

#[test]
fn sibling_functions_do_not_collide() {
    assert_counts(
        r#"function f1() { import x = require("nonexistent-a"); }
function f2() { import x = require("nonexistent-b"); }
"#,
        0,
        2,
        "same name in two sibling function bodies",
    );
}

#[test]
fn a_function_body_does_not_collide_with_the_file_top_level() {
    assert_counts(
        r#"import x = require("nonexistent-a");
function f() { import x = require("nonexistent-b"); }
"#,
        0,
        1,
        "function-body alias shadowing a top-level alias",
    );
}

#[test]
fn a_nested_function_body_does_not_collide_with_its_enclosing_function() {
    assert_counts(
        r#"function outer() { import x = require("nonexistent-a"); function inner() { import x = require("nonexistent-b"); } }
"#,
        0,
        2,
        "inner function body shadowing its enclosing function body",
    );
}

#[test]
fn differently_named_aliases_in_one_function_body_do_not_collide() {
    assert_counts(
        r#"function f() { import alpha = require("nonexistent-a"); import beta = require("nonexistent-b"); }
"#,
        0,
        2,
        "two differently named aliases in one function body",
    );
}

#[test]
fn a_single_alias_in_a_function_body_does_not_collide() {
    assert_counts(
        r#"function f() { import x = require("nonexistent-a"); }
"#,
        0,
        1,
        "a lone position-invalid alias in a function body",
    );
}
