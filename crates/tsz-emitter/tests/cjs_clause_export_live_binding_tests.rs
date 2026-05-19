//! Regression tests for CJS live-export binding substitution on clause-exported
//! locals (`export { x }` form).
//!
//! Previously, `collect_cjs_deferred_export_names` skipped any `export { … }`
//! clause that mixed renamed and unrenamed specifiers (e.g.
//! `export { foo, baz as quux }`). This made `cjs_deferred_export_names` empty
//! for all clause-exported locals, which in turn disabled:
//!
//! - Inline `exports.X = X;` emission after declarations.
//! - Live-export substitution on simple assignments (`foo = 3`).
//! - Compound assignment substitution (`buzz += 3`).
//! - Prefix-unary substitution (`++bizz`).
//! - Postfix-unary substitution (`bizz++`).
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/const_enums.rs`
//! (`collect_cjs_deferred_export_names`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn cjs_es5() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn cjs_es2015() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Inline export assignment after declaration
// ---------------------------------------------------------------------------

/// Mixed clause (`export { x, y as z }`) must emit `exports.x = x;` and
/// `exports.z = y;` inline after each declaration — the same as a pure
/// unrenamed or pure renamed clause does.
#[test]
fn mixed_clause_emits_inline_exports_after_declarations() {
    let source = "let foo = 1;\nlet baz = 2;\nexport { foo, baz as quux };\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.foo = foo;"),
        "Must emit inline `exports.foo = foo;` for unrenamed specifier in mixed clause.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.quux = baz;"),
        "Must emit inline `exports.quux = baz;` for renamed specifier in mixed clause.\nOutput:\n{output}"
    );
}

/// Pure unrenamed clause still emits inline exports (regression guard).
#[test]
fn pure_unrenamed_clause_still_emits_inline_exports() {
    let source = "let x = 1;\nlet y = 2;\nexport { x, y };\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.x = x;"),
        "Pure unrenamed clause must still emit `exports.x = x;`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.y = y;"),
        "Pure unrenamed clause must still emit `exports.y = y;`.\nOutput:\n{output}"
    );
}

/// Pure renamed clause still emits inline exports (regression guard).
#[test]
fn pure_renamed_clause_still_emits_inline_exports() {
    let source = "let a = 1;\nlet b = 2;\nexport { a as alpha, b as beta };\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.alpha = a;"),
        "Pure renamed clause must still emit `exports.alpha = a;`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.beta = b;"),
        "Pure renamed clause must still emit `exports.beta = b;`.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Simple assignment substitution
// ---------------------------------------------------------------------------

/// Assignment to a clause-exported local must chain `exports.X = local = val`.
#[test]
fn simple_assignment_chains_through_clause_export() {
    let source = "let x = 0;\nexport { x };\nx = 42;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.x = x = 42"),
        "Simple assignment to clause-exported local must update exports.\nOutput:\n{output}"
    );
}

/// Same with a different binding name — guards against hardcoding.
#[test]
fn simple_assignment_chains_through_clause_export_alternate_name() {
    let source = "let counter = 0;\nexport { counter };\ncounter = 99;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.counter = counter = 99"),
        "Simple assignment must update exports regardless of binding name.\nOutput:\n{output}"
    );
}

/// Assignment to a local exported under an alias updates the alias key.
#[test]
fn simple_assignment_updates_renamed_export_alias() {
    let source = "let val = 0;\nexport { val as value };\nval = 7;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.value = val = 7"),
        "Assignment to a clause-aliased local must update the alias export key.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Compound assignment substitution
// ---------------------------------------------------------------------------

/// `x += n` on a clause-exported local must become `exports.x = x += n`.
#[test]
fn compound_add_assignment_updates_clause_export() {
    let source = "let x = 0;\nexport { x };\nx += 5;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.x = x += 5"),
        "`+=` on a clause-exported local must update the export.\nOutput:\n{output}"
    );
}

/// `y -= n` variant — proves it's not hardcoded to `+=`.
#[test]
fn compound_sub_assignment_updates_clause_export() {
    let source = "let score = 10;\nexport { score };\nscore -= 3;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.score = score -= 3"),
        "`-=` on a clause-exported local must update the export.\nOutput:\n{output}"
    );
}

/// Compound assignment on an aliased clause export.
#[test]
fn compound_assignment_updates_renamed_clause_export() {
    let source = "let n = 0;\nexport { n as count };\nn *= 2;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.count = n *= 2"),
        "`*=` on a clause-aliased local must update the alias export key.\nOutput:\n{output}"
    );
}

/// Mixed clause: compound assignment on unrenamed specifier still updates exports.
#[test]
fn compound_assignment_in_mixed_clause_updates_export() {
    let source =
        "let foo = 1;\nlet baz = 2;\nexport { foo, baz as quux };\nfoo += 10;\nbaz -= 1;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.foo = foo += 10"),
        "`+=` must update export for unrenamed specifier in mixed clause.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.quux = baz -= 1"),
        "`-=` must update export for renamed specifier in mixed clause.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Prefix-unary substitution
// ---------------------------------------------------------------------------

/// `++x` on a clause-exported local must become `exports.x = ++x`.
#[test]
fn prefix_increment_updates_clause_export() {
    let source = "let x = 0;\nexport { x };\n++x;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.x = ++x"),
        "`++x` on a clause-exported local must update the export.\nOutput:\n{output}"
    );
}

/// `--y` variant — proves the rule applies to both prefix operators.
#[test]
fn prefix_decrement_updates_clause_export() {
    let source = "let hits = 5;\nexport { hits };\n--hits;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.hits = --hits"),
        "`--hits` on a clause-exported local must update the export.\nOutput:\n{output}"
    );
}

/// Prefix-unary on a renamed clause export updates the alias key.
#[test]
fn prefix_increment_updates_renamed_clause_export() {
    let source = "let n = 0;\nexport { n as index };\n++n;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.index = ++n"),
        "`++n` must update the alias export key `exports.index`.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Postfix-unary substitution (statement context)
// ---------------------------------------------------------------------------

/// `x++` as a statement on a clause-exported local must become
/// `exports.x = (x++, x)`.
#[test]
fn postfix_increment_stmt_updates_clause_export() {
    let source = "let x = 0;\nexport { x };\nx++;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.x = (x++, x)"),
        "`x++` statement on a clause-exported local must update exports.\nOutput:\n{output}"
    );
}

/// `y--` statement variant.
#[test]
fn postfix_decrement_stmt_updates_clause_export() {
    let source = "let count = 3;\nexport { count };\ncount--;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.count = (count--, count)"),
        "`count--` statement on a clause-exported local must update exports.\nOutput:\n{output}"
    );
}

/// Postfix-unary statement on a renamed clause export updates the alias key.
#[test]
fn postfix_increment_stmt_updates_renamed_clause_export() {
    let source = "let n = 0;\nexport { n as idx };\nn++;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.idx = (n++, n)"),
        "`n++` statement must update alias export key `exports.idx`.\nOutput:\n{output}"
    );
}

/// Postfix-unary statement on an inline export that is also clause-aliased must
/// assign aliases from the updated export value, not from the stale postfix
/// result.
#[test]
fn postfix_increment_stmt_updates_inline_export_alias() {
    let source = "export let x = 0;\nexport { x as y };\nx++;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.y = (exports.x++, exports.x)"),
        "`x++` statement must update aliased export from the post-increment value.\nOutput:\n{output}"
    );
}

/// Same rule for postfix decrement on inline exports with a clause alias.
#[test]
fn postfix_decrement_stmt_updates_inline_export_alias() {
    let source = "export let count = 3;\nexport { count as total };\ncount--;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.total = (exports.count--, exports.count)"),
        "`count--` statement must update aliased export from the post-decrement value.\nOutput:\n{output}"
    );
}

/// Expression context still returns the pre-update value while refreshing the
/// alias from the updated inline export.
#[test]
fn postfix_increment_expr_returns_previous_value_and_updates_inline_export_alias() {
    let source = "export let x = 0;\nexport { x as y };\nlet before = x++;\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("exports.y = (_a = exports.x++, exports.x), _a"),
        "`x++` expression must return the previous value while updating the alias.\nOutput:\n{output}"
    );
}

/// Exported function bodies are printed with CommonJS temporarily suppressed,
/// but clause-exported locals still need live-binding mutation rewrites there.
#[test]
fn exported_function_body_updates_later_clause_export() {
    let source = "let x = 1;\nexport function foo(y: number) {\n    if (y <= x++) return y <= x++;\n    ++x;\n    x--;\n}\nexport { x };\n";
    let output = parse_lower_emit(source, cjs_es2015());
    assert!(
        output.contains("if (y <= (exports.x = (_a = x++, x), _a))"),
        "Postfix expression inside exported function must update clause export and return previous value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.x = ++x;"),
        "Prefix statement inside exported function must update clause export.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.x = (x--, x);"),
        "Postfix statement inside exported function must update clause export from updated local.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Full mix: the original bug repro
// ---------------------------------------------------------------------------

/// The complete original failing shape: mixed clause with all mutation forms.
#[test]
fn mixed_clause_all_mutation_forms() {
    let source = r#"
let foo = 1;
let baz = 2;
let buzz = 3;
let bizz = 4;
export { foo, baz, baz as quux, buzz, bizz };
foo = 3;
buzz += 3;
bizz++;
++bizz;
"#;
    let output = parse_lower_emit(source, cjs_es5());
    assert!(
        output.contains("exports.foo = foo = 3"),
        "Simple assignment must update `exports.foo`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.buzz = buzz += 3"),
        "Compound `+=` must update `exports.buzz`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.bizz = (bizz++, bizz)"),
        "Postfix `bizz++` must update `exports.bizz`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.bizz = ++bizz"),
        "Prefix `++bizz` must update `exports.bizz`.\nOutput:\n{output}"
    );
}
