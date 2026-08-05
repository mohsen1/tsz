//! A position-invalid `export default [ expr ]` reports its placement
//! diagnostic and nothing else — the exported expression is never typed, so an
//! unresolved name inside it draws no name-resolution diagnostic.
//!
//! tsc's `checkExportAssignment` opens with `checkGrammarModuleElementContext`
//! and `return`s the moment the context is invalid, before it types the
//! expression. The exception is a body tsc revisits through its *deferred*
//! queue (`checkFunctionExpressionOrObjectLiteralMethodDeferred`): a function
//! expression, an arrow function, or an object-literal method. Inside one of
//! those the expression is typed and the unresolved name still reports.
//!
//! The object-literal accessor is the row that pins the rule to the deferred
//! set rather than to "is inside some function": tsc does not defer accessors,
//! so an object-literal getter suppresses exactly like a class method does
//! while the object-literal method one line away does not.
//!
//! Harness note: the default unit harness does not report an unresolved name in
//! an *expression* position at all — a plain `const zz = undefinedName;` comes
//! back clean under it, while the same name in a heritage clause does report.
//! The suppressing rows below therefore assert the placement diagnostic, which
//! the harness does see, and the deferred-container rows are pinned but
//! `#[ignore]`d; both halves are verified end to end through the CLI in the PR
//! body. `default_exported_class_declaration_in_a_block_still_resolves_its_heritage`
//! is the live over-reach control: it goes through a path the harness *can*
//! observe and would fail if the suppression reached the declaration forms.
//!
//! Verified against the pinned tsc 7.0.2 through
//! `scripts/conformance/oracle.sh` (which carries the
//! `--singleThreaded --stableTypeOrdering true` flags the conformance cache
//! generator uses), cross-checked against the default scheduler. All rows here
//! are flag-insensitive.

use crate::test_utils::check_source_diagnostics;

const DEFAULT_EXPORT_TOP_LEVEL: u32 = 1258;
const EXPORT_ASSIGNMENT_TOP_LEVEL: u32 = 1231;
const MODIFIERS_CANNOT_APPEAR_HERE: u32 = 1184;
const DEFAULT_EXPORT_IN_NAMESPACE: u32 = 1319;

/// `Cannot find name 'x'.` and its did-you-mean variant. The default unit
/// harness loads no lib, so which of the two fires depends on whether a
/// suggestion candidate exists; the rule under test is about whether the name
/// is resolved at all, so both codes count as "the expression was typed".
const CANNOT_FIND_NAME: [u32; 2] = [2304, 2552];

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn resolved_the_expression(source: &str) -> bool {
    let found = codes(source);
    CANNOT_FIND_NAME.iter().any(|code| found.contains(code))
}

// --- Suppressing containers: everything tsc checks in the eager walk. ---

#[test]
fn bare_block_reports_placement_only() {
    let source = "{ export default undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn function_declaration_body_reports_placement_only() {
    let source = "function f() { export default undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn class_method_body_reports_placement_only() {
    let source = "class C { m() { export default undefinedName; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn class_constructor_body_reports_placement_only() {
    let source = "class C { constructor() { export default undefinedName; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn class_getter_body_reports_placement_only() {
    let source = "class C { get x() { export default undefinedName; return 1; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn class_static_block_reports_placement_only() {
    let source = "class D { static { export default undefinedName; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn if_block_reports_placement_only() {
    let source = "if (true) { export default undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn loop_body_reports_placement_only() {
    let source = "for (;;) { export default undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn nested_block_inside_a_function_declaration_reports_placement_only() {
    let source = "function f() { { export default undefinedName; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// The nearest function-like container decides, not the outermost one: a
/// function *declaration* nested inside an arrow still suppresses.
#[test]
fn function_declaration_inside_an_arrow_reports_placement_only() {
    let source = "const g = () => { function inner() { export default undefinedName; } };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// tsc does not defer object-literal accessors, so this suppresses even though
/// the object-literal *method* on the next test does not.
#[test]
fn object_literal_getter_reports_placement_only() {
    let source = "const o = { get x() { export default undefinedName; return 1; } };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

// --- Deferred containers: tsc types the expression, and so must tsz. ---

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn arrow_function_body_still_resolves_the_expression() {
    let source = "const g = () => { export default undefinedName; };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn function_expression_body_still_resolves_the_expression() {
    let source = "const g = function () { export default undefinedName; };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn named_function_expression_body_still_resolves_the_expression() {
    let source = "const g = function fn() { export default undefinedName; };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn object_literal_method_body_still_resolves_the_expression() {
    let source = "const o = { m() { export default undefinedName; } };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn immediately_invoked_arrow_still_resolves_the_expression() {
    let source = "(() => { export default undefinedName; })();\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn nested_block_inside_an_arrow_still_resolves_the_expression() {
    let source = "const g = () => { { export default undefinedName; } };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn arrow_nested_inside_a_class_method_still_resolves_the_expression() {
    let source = "class C { m() { const q = () => { export default undefinedName; }; } }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn class_property_initializer_arrow_still_resolves_the_expression() {
    let source = "class C { p = () => { export default undefinedName; }; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn generator_function_expression_still_resolves_the_expression() {
    let source = "const g = function* () { export default undefinedName; };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

// --- Controls: rows the suppression must not reach. ---

/// A valid top-level `export default` is the whole point of the gate being
/// container-scoped: it still resolves.
#[test]
fn top_level_default_export_draws_no_placement_diagnostic() {
    let source = "export default undefinedName;\n";
    let found = codes(source);
    assert!(
        !found.contains(&DEFAULT_EXPORT_TOP_LEVEL),
        "codes: {found:?}"
    );
}

/// `export default class`/`function` in a statement position is the declaration
/// carrying an illegal modifier (TS1184), not an export assignment, so tsc
/// checks it normally and the heritage/body names still resolve.
#[test]
fn default_exported_class_declaration_in_a_block_still_resolves_its_heritage() {
    let source = "{ export default class extends undefinedBase {} }\n";
    let found = codes(source);
    assert!(found.contains(&MODIFIERS_CANNOT_APPEAR_HERE));
    assert!(!found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn default_exported_function_declaration_in_a_block_still_resolves_its_body() {
    let source = "{ export default function h() { return undefinedName; } }\n";
    let found = codes(source);
    assert!(found.contains(&MODIFIERS_CANNOT_APPEAR_HERE));
    assert!(!found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

/// `export =` is a different production with its own placement diagnostic and
/// no default symbol; it was already correct and must stay untouched.
#[test]
fn export_assignment_in_a_block_reports_placement_only() {
    let source = "{ export = undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&EXPORT_ASSIGNMENT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn export_assignment_in_a_function_body_reports_placement_only() {
    let source = "function f() { export = undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&EXPORT_ASSIGNMENT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// A namespace body is a *valid* module-element context, so the suppression
/// here comes from the pre-existing TS1319 arm, not from this rule.
#[test]
fn namespace_body_keeps_its_own_ts1319_arm() {
    let source = "namespace N { export default undefinedName; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_IN_NAMESPACE));
    assert!(!found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// Suppression is about the *expression walk*, not about the placement
/// diagnostic: a resolvable name and a literal were already clean and stay so.
#[test]
fn resolvable_name_in_a_block_reports_placement_only() {
    let source = "const known = 1;\n{ export default known; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
fn literal_expression_in_a_block_reports_placement_only() {
    let source = "{ export default 42; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// A renamed binder must not change the verdict — the rule is structural.
#[test]
fn renamed_binder_in_a_block_reports_placement_only() {
    let source = "{ const zzz = 1; export default zzZ; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn renamed_binder_in_an_arrow_still_resolves_the_expression() {
    let source = "const g = () => { const zzz = 1; export default zzZ; };\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}

/// A non-identifier expression takes the same walk, so the suppression has to
/// cover names nested inside it too.
#[test]
fn object_literal_expression_in_a_block_reports_placement_only() {
    let source = "{ export default { a: undefinedName }; }\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

/// A file that is genuinely a module (it has a top-level `export {}`) behaves
/// the same: the rule is about the declaration's container, not the file kind.
#[test]
fn module_file_bare_block_reports_placement_only() {
    let source = "{ export default undefinedName; }\nexport {};\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!resolved_the_expression(source), "codes: {found:?}");
}

#[test]
#[ignore = "the default unit harness does not report an unresolved name in an \
           expression position at all — a plain `const zz = undefinedName;` is \
           clean under it — so the deferred-container half of this rule is pinned \
           here but verified through the CLI against `scripts/conformance/oracle.sh`; \
           see the matrix in the PR body"]
fn module_file_arrow_body_still_resolves_the_expression() {
    let source = "const g = () => { export default undefinedName; };\nexport {};\n";
    let found = codes(source);
    assert!(found.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(resolved_the_expression(source), "codes: {found:?}");
}
