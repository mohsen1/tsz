//! Regression tests for #16580: with `strictNullChecks` off, tsc's
//! `getUnionType(members, UnionReduction.Subtype)` absorbs a scalar
//! `null`/`undefined` member out of *every* union it constructs whenever a
//! non-nullish sibling is present, not just the array-literal element union
//! #16578 already fixed. Fixed at the general syntactic-union-type-node
//! resolvers (`types/type_node.rs`, `types/computation/type_operators.rs`,
//! `types/type_literal_checker.rs`) via the shared solver primitive
//! `nonstrict_union_members_absorb_nullish_scalars`.
//!
//! Every row is pinned against a real `typescript@7.0.2` oracle
//! (`--strict false --strictNullChecks false`).

use crate::test_utils::{
    check_with_options_code_messages, non_strict_checker_options, strict_checker_options,
};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// Declared return type: `number | null` reduces to bare `number`.
#[test]
fn nonstrict_return_type_union_absorbs_null() {
    let source = "\
declare function f(): number | null;
var probe: string = f();
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )]
    );
}

/// Type alias whose body is exactly `number | undefined` fully reduces to
/// `number`, and prints as the reduced primitive rather than the alias name
/// (there is nothing left of the alias's union shape to point the origin at).
#[test]
fn nonstrict_type_alias_full_reduction_prints_reduced_type_not_alias_name() {
    let source = "\
type Widget = number | undefined;
declare var w: Widget;
var probe: string = w;
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )]
    );
}

/// A 3-member alias (`string | null | undefined`) drops both nullish
/// scalars, landing on the lone survivor `string` — renamed-binder control
/// distinct from the previous test's names.
#[test]
fn nonstrict_type_alias_three_member_union_drops_both_nullish_scalars() {
    let source = "\
type Named = string | null | undefined;
declare var value: Named;
var probe: number = value;
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'string' is not assignable to type 'number'.".to_string()
        )]
    );
}

/// Multiple non-nullish siblings survive the reduction as a (smaller) union,
/// not a single type — the reduction only drops the nullish scalar members.
#[test]
fn nonstrict_union_keeps_remaining_non_nullish_members_as_a_union() {
    let source = "\
declare var q: string | number | null;
var probe: boolean = q;
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'string | number' is not assignable to type 'boolean'.".to_string()
        )]
    );
}

/// Negative control: an all-nullish union (`null | undefined`) must NOT be
/// touched by this reduction — it stays nullable and both members are
/// assignable to anything in non-strict mode, so this is clean.
#[test]
fn nonstrict_all_nullish_union_stays_untouched_and_clean() {
    let source = "\
declare var allNullish: null | undefined;
var probe: string = allNullish;
";
    assert_eq!(nonstrict_messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: under `strictNullChecks`, the reduction must not fire —
/// `number | null` stays a union and the assignment is still an error citing
/// the full union.
#[test]
fn strict_mode_return_type_union_keeps_null_member() {
    let source = "\
declare function f(): number | null;
var probe: string = f();
";
    assert_eq!(
        strict_messages(source),
        vec![(
            2322,
            "Type 'number | null' is not assignable to type 'string'.".to_string()
        )]
    );
}

/// A union member written inside an inline type literal reduces the same
/// way as a top-level annotation.
#[test]
fn nonstrict_type_literal_member_union_absorbs_null() {
    let source = "\
declare var obj: { x: number | null };
var probe: string = obj.x;
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )]
    );
}

/// Positive control unaffected by this change: a union with no nullish
/// member reduces exactly as before.
#[test]
fn nonstrict_union_without_nullish_member_is_unaffected() {
    let source = "\
declare function f(): string | number;
var probe: boolean = f();
";
    assert_eq!(
        nonstrict_messages(source),
        vec![(
            2322,
            "Type 'string | number' is not assignable to type 'boolean'.".to_string()
        )]
    );
}
