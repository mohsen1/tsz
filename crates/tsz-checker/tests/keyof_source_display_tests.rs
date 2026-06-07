//! Issue #10799 (display family): a `keyof <operand>` assignment *source* must
//! render as tsc does in `TS2322`/`TS2345` diagnostics.
//!
//! Structural rule: tsc attaches no `aliasSymbol` to a `keyof` over an anonymous
//! operand (an inline object type literal or a `typeof value`), so the source is
//! shown as its reduced key set. Against a *literal-sensitive* target (a literal,
//! enum, template-literal, or unit-symbol type) the literal key members survive
//! (`"a" | "b"`); against any other target the key set is widened to its
//! primitive base (`string`), exactly as tsc does. A `keyof <named operand>`
//! (interface / class / type alias) keeps its `keyof Name` spelling.
//!
//! Before the fix tsz leaked an unreduced `keyof { … }`, a prematurely widened
//! `string`, or the bare alias name depending on how the source type interned.
//!
//! Binder / alias / type-parameter names are varied across cases so the
//! rendering is proven structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn single(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

fn assert_source(source: &str, code: u32, expected_source: &str) {
    let diags = check_strict(source);
    let diag = single(&diags, code);
    let needle = if code == 2345 {
        format!("Argument of type '{expected_source}' is not assignable")
    } else {
        format!("Type '{expected_source}' is not assignable")
    };
    assert!(
        diag.message_text.contains(&needle),
        "expected source display '{expected_source}', got: {}",
        diag.message_text
    );
}

/// `keyof <object type literal>` alias reduces to its literal key union against a
/// literal-sensitive target — not a widened `string`.
#[test]
fn keyof_object_literal_alias_source_renders_literal_key_union() {
    assert_source(
        "type Keys_A = keyof { alpha: 1; beta: 2 };\n\
         declare const witness_a: Keys_A;\n\
         const sink_a: 0 = witness_a;\n",
        2322,
        "\"alpha\" | \"beta\"",
    );
}

/// The same reduction in argument position (`TS2345`).
#[test]
fn keyof_object_literal_alias_argument_renders_literal_key_union() {
    assert_source(
        "type Keys_B = keyof { one: true; two: false };\n\
         declare const witness_b: Keys_B;\n\
         declare function consume_b(value: 0): void;\n\
         consume_b(witness_b);\n",
        2345,
        "\"one\" | \"two\"",
    );
}

/// A `keyof Named` over a named interface keeps its `keyof Name` spelling.
#[test]
fn keyof_named_interface_source_keeps_keyof_spelling() {
    assert_source(
        "interface Shape_C { width: 1; height: 2 }\n\
         type Keys_C = keyof Shape_C;\n\
         declare const witness_c: Keys_C;\n\
         const sink_c: 0 = witness_c;\n",
        2322,
        "keyof Shape_C",
    );
}

/// A `keyof` over an object type with a string index signature reduces to its
/// primitive key set (`string | number`) — there is no finite literal context.
#[test]
fn keyof_index_signature_alias_source_renders_primitive_key_set() {
    assert_source(
        "type Keys_D = keyof { [entry: string]: number };\n\
         declare const witness_d: Keys_D;\n\
         const sink_d: 0 = witness_d;\n",
        2322,
        "string | number",
    );
}

/// Negative control: against a *non*-literal-sensitive target (an object type)
/// tsc widens the anonymous key set to `string`, and so does tsz.
#[test]
fn keyof_object_literal_alias_source_widens_for_object_target() {
    assert_source(
        "type Keys_E = keyof { first: 1; second: 2 };\n\
         declare const witness_e: Keys_E;\n\
         const sink_e: { slot: 1 } = witness_e;\n",
        2322,
        "string",
    );
}

/// Negative control: a directly-written literal union alias is *not* a `keyof`
/// reduction, so tsc keeps its alias name. The keyof rewrite must not touch it.
#[test]
fn direct_literal_union_alias_source_keeps_alias_name() {
    assert_source(
        "type Choice_F = \"left\" | \"right\";\n\
         declare const witness_f: Choice_F;\n\
         const sink_f: 0 = witness_f;\n",
        2322,
        "Choice_F",
    );
}

/// Negative control: a deferred generic `keyof T` (free type parameter) keeps
/// its spelling because it has not reduced to a concrete key set.
#[test]
fn deferred_generic_keyof_source_keeps_spelling() {
    let diags = check_strict(
        "function pick_g<T extends { tag: 0 }>(holder: T): 0 {\n\
         return null as any as keyof T;\n\
         }\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text.contains("keyof T"),
        "a deferred generic keyof must keep its spelling; got: {}",
        diag.message_text
    );
}
