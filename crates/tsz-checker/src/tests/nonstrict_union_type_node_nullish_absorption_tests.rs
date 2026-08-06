//! Tests for #16580: with `strictNullChecks` off, tsc's `getUnionType`
//! (`addTypeToUnion`) drops a `null`/`undefined` constituent out of *every*
//! union it constructs whenever a non-nullish sibling survives — not only the
//! array-literal element path #16578 already fixed
//! (`array_literal.rs::nonstrict_array_element_union_absorbs_nullish_scalars`).
//! A written union type node — a return-type annotation, a type-alias body, a
//! parameter or variable annotation — resolves through one common choke
//! point, `TypeNodeChecker::get_type_from_union_type`, so the reduction lives
//! there (`nonstrict_union_type_node_absorbs_nullish_scalars`).
//!
//! Every row is pinned against `typescript@7.0.2`,
//! `--strict false --strictNullChecks false --target es2015`.
//!
//! `type Age = number | undefined` reducing to bare `number` also means the
//! alias has nothing left to print once the nullish member is gone — tsc
//! renders the surviving primitive, not the alias name, and so must tsz: this
//! is a naming *consequence* of the shape reduction, not a printer defect to
//! chase separately.

use crate::test_utils::{
    check_with_options_code_messages, non_strict_checker_options, strict_checker_options,
};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// Probe the reduced type by forcing it into a `TS2322` that prints it.
fn assert_reduces_to(source: &str, rendered: &str, target: &str) {
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            format!("Type '{rendered}' is not assignable to type '{target}'.")
        )],
        "expected the union type node to reduce to `{rendered}`: {messages:?}"
    );
}

#[test]
fn return_type_annotation_drops_null_with_non_nullish_sibling() {
    assert_reduces_to(
        "declare function f(): number | null;\nvar probe: string = f();",
        "number",
        "string",
    );
}

#[test]
fn type_alias_drops_undefined_with_non_nullish_sibling() {
    // The alias itself has nothing left to print once `undefined` is gone —
    // tsc renders `number`, not the alias name `Age`.
    assert_reduces_to(
        "type Age = number | undefined;\ndeclare var age: Age;\nvar probe: string = age;",
        "number",
        "string",
    );
}

#[test]
fn three_member_type_alias_drops_both_nullish_members() {
    assert_reduces_to(
        "type Zqq = string | null | undefined;\ndeclare var w: Zqq;\nvar probe: number = w;",
        "string",
        "number",
    );
}

#[test]
fn variable_annotation_union_drops_null_with_non_nullish_sibling() {
    assert_reduces_to(
        "declare var a: number | null;\nvar probe: string = a;",
        "number",
        "string",
    );
}

#[test]
fn parameter_type_annotation_drops_undefined_with_non_nullish_sibling() {
    assert_reduces_to(
        "declare function take(x: string | undefined): void;\n\
         function wrap(x: string | undefined) {\n\
           take(x);\n\
           var probe: number = x;\n\
         }",
        "string",
        "number",
    );
}

#[test]
fn multiple_non_nullish_survivors_keep_a_smaller_union() {
    // Only the nullish member is absorbed; the two non-nullish siblings stay
    // a union, not a single surviving type. Written directly (not through a
    // type alias) to isolate the reduction from the pre-existing, unrelated
    // display quirk where a multi-member all-primitive *alias* union keeps
    // its alias name instead of expanding — reproducible on `main` with no
    // nullish member involved at all (`type V = string | number | boolean`),
    // so it is out of this fix's scope.
    assert_reduces_to(
        "declare var u: string | number | null;\nvar probe: boolean = u;",
        "string | number",
        "boolean",
    );
}

#[test]
fn renamed_binder_type_alias_still_reduces() {
    // Anti-hardcoding control: an unrelated alias/binding name must not
    // change the outcome.
    assert_reduces_to(
        "type Zorbatron = boolean | null;\ndeclare var qqxk: Zorbatron;\nvar probe: string = qqxk;",
        "boolean",
        "string",
    );
}

#[test]
fn all_nullish_union_type_node_is_untouched() {
    // Negative control (matches #16580's `a6`): an all-nullish union has no
    // non-nullish sibling to absorb into and must stay assignable to itself,
    // not collapse away entirely.
    let messages =
        nonstrict_messages("declare var a: null | undefined;\nvar probe: null | undefined = a;");
    assert!(
        messages.is_empty(),
        "an all-nullish union type node must stay clean: {messages:?}"
    );
}

#[test]
fn strict_mode_keeps_the_nullish_member() {
    // Positive control: the reduction is strictNullChecks-off only. Under
    // strict mode the union keeps its written members.
    let messages =
        strict_messages("declare function f(): number | null;\nvar probe: string = f();");
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'number | null' is not assignable to type 'string'.".to_string()
        )],
        "strict mode must keep `null` in the union: {messages:?}"
    );
}
