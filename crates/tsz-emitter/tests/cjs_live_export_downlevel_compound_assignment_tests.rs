//! Regression tests: CommonJS live-export mirror on **down-leveled** compound
//! assignments (`**=`, `&&=`, `||=`, `??=`).
//!
//! When an exported local is written with a compound-assignment operator that
//! the target down-levels, the lowered expansion must still thread the write
//! through the CommonJS live named export, exactly as the non-lowered path does:
//!
//! - clause export `export { b }` → `exports.b = b = <value>`
//! - clause rename `export { b as foo }` → `exports.foo = b = <value>`
//! - inline export `export let b` → `exports.b = <value>`
//!
//! Previously the exponentiation lowering (`emit_exponentiation_expression`) and
//! the logical-assignment lowering (`emit_logical_assignment_expression`) emitted
//! the write-target LHS directly, bypassing the live-export gateway, so the
//! exported binding was never updated at runtime. `tsc` keeps the mirror:
//!
//! ```js
//! // export { b }; b ??= 5;  (--target es2015 --module commonjs)
//! b !== null && b !== void 0 ? b : (exports.b = b = 5);
//! // b **= 2;
//! exports.b = b = Math.pow(b, 2);
//! ```
//!
//! Owner: emitter — `crates/tsz-emitter/src/emitter/expressions/binary_downlevel.rs`.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn cjs(target: ScriptTarget) -> PrintOptions {
    PrintOptions {
        target,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Clause export (`export { b }`) — down-leveled logical assignment
// ---------------------------------------------------------------------------

#[test]
fn clause_export_nullish_assignment_es2015_mirrors_export() {
    let source = "let b = 1;\nb ??= 5;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("b !== null && b !== void 0 ? b : (exports.b = b = 5);"),
        "Down-leveled `??=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

#[test]
fn clause_export_or_assignment_es2015_mirrors_export() {
    let source = "let b = 1;\nb ||= 2;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("b || (exports.b = b = 2);"),
        "Down-leveled `||=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

#[test]
fn clause_export_and_assignment_es2015_mirrors_export() {
    let source = "let b = 1;\nb &&= 3;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("b && (exports.b = b = 3);"),
        "Down-leveled `&&=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

#[test]
fn clause_export_exponentiation_assignment_es2015_mirrors_export() {
    let source = "let b = 1;\nb **= 2;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("exports.b = b = Math.pow(b, 2);"),
        "Down-leveled `**=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

#[test]
fn clause_export_nullish_assignment_es5_mirrors_export() {
    let source = "let b = 1;\nb ??= 5;\nb **= 2;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES5));
    assert!(
        output.contains("b !== null && b !== void 0 ? b : (exports.b = b = 5);"),
        "ES5 down-leveled `??=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.b = b = Math.pow(b, 2);"),
        "ES5 down-leveled `**=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

#[test]
fn clause_export_nullish_assignment_es2020_uses_native_nullish_and_mirrors() {
    // ES2020 supports `??` natively but not `??=`, so the logical assignment
    // still lowers — to the native `??` short-circuit form.
    let source = "let b = 1;\nb ??= 5;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2020));
    assert!(
        output.contains("b ?? (exports.b = b = 5);"),
        "ES2020 down-leveled `??=` on a clause export must mirror `exports.b`.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Clause rename / multiple aliases
// ---------------------------------------------------------------------------

#[test]
fn renamed_clause_export_downlevel_compound_mirrors_alias() {
    let source = "let b = 1;\nb ??= 5;\nb **= 2;\nexport { b as foo };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("b !== null && b !== void 0 ? b : (exports.foo = b = 5);"),
        "Down-leveled `??=` must mirror the renamed export `exports.foo`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.foo = b = Math.pow(b, 2);"),
        "Down-leveled `**=` must mirror the renamed export `exports.foo`.\nOutput:\n{output}"
    );
}

#[test]
fn multi_alias_clause_export_downlevel_compound_mirrors_all() {
    let source = "let b = 1;\nb ??= 5;\nb **= 2;\nexport { b, b as foo };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains("b !== null && b !== void 0 ? b : (exports.foo = exports.b = b = 5);"),
        "Down-leveled `??=` must mirror every alias in a multi-name clause.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.foo = exports.b = b = Math.pow(b, 2);"),
        "Down-leveled `**=` must mirror every alias in a multi-name clause.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Inline export (`export let b`) with an extra clause alias
// ---------------------------------------------------------------------------

#[test]
fn inline_export_with_alias_downlevel_compound_mirrors_both() {
    let source = "export let b = 1;\nb ??= 5;\nb **= 2;\nexport { b as foo };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        output.contains(
            "exports.b !== null && exports.b !== void 0 ? exports.b : (exports.foo = exports.b = 5);"
        ),
        "Down-leveled `??=` on an inline export must rewrite reads and mirror the alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.foo = exports.b = Math.pow(exports.b, 2);"),
        "Down-leveled `**=` on an inline export must rewrite reads and mirror the alias.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls
// ---------------------------------------------------------------------------

#[test]
fn non_exported_local_downlevel_compound_has_no_export_mirror() {
    // A plain (non-exported) local must NOT gain an `exports.` mirror.
    let source = "let local = 1;\nlocal ??= 5;\nlocal **= 2;\nexport {};\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ES2015));
    assert!(
        !output.contains("exports.local"),
        "A non-exported local must not gain an export mirror.\nOutput:\n{output}"
    );
    assert!(
        output.contains("local !== null && local !== void 0 ? local : (local = 5);"),
        "Non-exported `??=` keeps the plain lowered form.\nOutput:\n{output}"
    );
    assert!(
        output.contains("local = Math.pow(local, 2);"),
        "Non-exported `**=` keeps the plain lowered form.\nOutput:\n{output}"
    );
}

#[test]
fn native_compound_assignment_clause_export_still_mirrors() {
    // Regression guard for the non-lowered path: at a target that supports
    // `??=`/`**=` natively, the clause export mirror still emits (unchanged).
    let source = "let b = 1;\nb ??= 5;\nb **= 2;\nexport { b };\n";
    let output = parse_lower_emit(source, cjs(ScriptTarget::ESNext));
    assert!(
        output.contains("exports.b = b ??= 5;"),
        "Native `??=` on a clause export must still mirror `exports.b`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.b = b **= 2;"),
        "Native `**=` on a clause export must still mirror `exports.b`.\nOutput:\n{output}"
    );
}
