//! Union-target best-member selection for generic-reference sources.
//!
//! When a relation fails against a union target and the source is an
//! instantiation of a generic reference, tsc's `getBestMatchingType`
//! (`findMatchingTypeReferenceOrTypeAliasReference`) elaborates beneath a
//! member frame against the first union member naming the source's own
//! generic base — across forwarding-alias spellings on either side — and
//! that member relation drills its differing type **argument**
//! (`Type 'string | number' is not assignable to type 'number'.` ->
//! `Type 'string' is not assignable to type 'number'.`), never the
//! structural property walk.
//!
//! Owner: solver — `explain_union_target_failure`'s same-base member step
//! plus `explain_same_generic_type_arguments`' provenance-recovered
//! application identity.
//!
//! Oracle: typescript@7.0.2 via `scripts/conformance/oracle.sh` (`--strict`).
//! Where the pinned oracle's `--stableTypeOrdering` sorts the union head
//! differently from tsz's current written-order display, the fences pin the
//! structural rule (frame member = first same-base member of tsz's member
//! list); the union member-order family (#17661 residual 3 / board item (b))
//! owns flipping the list itself, which flips head and frame together.

use tsz_checker::test_utils::{check_with_options, strict_checker_options};
use tsz_common::diagnostics::Diagnostic;

/// Assert the single diagnostic of `code` renders exactly this elaboration
/// chain: the primary message at depth 0 followed by its related-information
/// `(depth + 1, text)` pairs.
fn assert_exact_chain(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut chain = vec![(0u8, matching[0].message_text.clone())];
    chain.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    let rendered: Vec<(u8, &str)> = chain.iter().map(|(d, m)| (*d, m.as_str())).collect();
    assert_eq!(rendered, expected, "chain mismatch for:\n{source}");
}

const BUILDER: &str = r#"
interface RawBuilder<Output> {
  readonly expressionType: Output | undefined
  readonly isRawBuilder: true
}
"#;

// --- Direct pair: evaluated/declared instantiations drill the argument -----

// KNOWN HEAD RESIDUAL: the pinned oracle renders the alias (`'StrRow'`) in
// the head's target slot; tsz's same-generic mismatch head evaluates a
// non-generic alias of an instantiation to its body application
// (`'RawBuilder<string>'`) — pre-existing on main for the
// both-sides-application shape (`declare const a: RawBuilder<number>` vs
// `SR`), so it is pinned as-is here. The drill shape (argument relation, no
// property walk) is the rule under test.
#[test]
fn direct_pair_drills_type_argument_not_property() {
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<string | number>
type StrRow = RawBuilder<string>
const t: StrRow = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'RawBuilder<string>'.",
            ),
            (
                1,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (2, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn generic_alias_direct_pair_keeps_alias_head_and_argument_drill() {
    // A generic alias instantiation records display-alias provenance, so the
    // head keeps `Row<string>` (oracle-exact) while the drill still compares
    // the underlying `RawBuilder` type argument.
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<string | number>
type Row<Payload> = RawBuilder<Payload>
const t: Row<string> = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'Row<string>'.",
            ),
            (
                1,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (2, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

// --- Union target: member frame against the first same-base member ---------

#[test]
fn alias_arm_union_target_elaborates_first_same_base_member() {
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<string | number>
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
const t: StrRow | NumRow = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow | NumRow'.",
            ),
            (
                1,
                "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn generic_alias_arm_union_target_hops_forwarding_alias_base() {
    // Oracle-exact (written and sorted member order agree here): the frame
    // member keeps the alias spelling `Row<string>`, the drill compares the
    // underlying `RawBuilder` type argument.
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<string | number>
type Row<Payload> = RawBuilder<Payload>
const t: Row<string> | Row<number> = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'Row<string> | Row<number>'.",
            ),
            (
                1,
                "Type 'RawBuilder<string | number>' is not assignable to type 'Row<string>'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn renamed_binder_arms_keep_member_frame_and_argument_drill() {
    assert_exact_chain(
        r#"
interface CrateRow<Payload> {
  readonly slot: Payload | undefined
  readonly sealed: true
}
declare const crated: CrateRow<string | number>
type FirstCrate = CrateRow<string>
type SecondCrate = CrateRow<number>
const t: FirstCrate | SecondCrate = crated
"#,
        2322,
        &[
            (
                0,
                "Type 'CrateRow<string | number>' is not assignable to type 'FirstCrate | SecondCrate'.",
            ),
            (
                1,
                "Type 'CrateRow<string | number>' is not assignable to type 'FirstCrate'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn same_base_member_outranks_earlier_different_base_member() {
    // Oracle-exact: the different-base `AaaBuilder<boolean>` arm precedes the
    // same-base `StrRow` arm in both written and sorted order, yet tsc's
    // same-reference step still selects `StrRow`; a single failing argument
    // renders one drill line (no union-source leaf).
    assert_exact_chain(
        &format!(
            "{BUILDER}
interface AaaBuilder<Output> {{
  readonly expressionType: Output | undefined
  readonly isAaa: true
}}
declare const r: RawBuilder<boolean>
type StrRow = RawBuilder<string>
const t: AaaBuilder<boolean> | StrRow = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<boolean>' is not assignable to type 'AaaBuilder<boolean> | StrRow'.",
            ),
            (
                1,
                "Type 'RawBuilder<boolean>' is not assignable to type 'StrRow'.",
            ),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn non_union_argument_renders_single_drill_line() {
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<boolean>
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
const t: StrRow | NumRow = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<boolean>' is not assignable to type 'StrRow | NumRow'.",
            ),
            (
                1,
                "Type 'RawBuilder<boolean>' is not assignable to type 'StrRow'.",
            ),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn nullish_arm_keeps_member_frame_for_multi_real_member_union() {
    assert_exact_chain(
        &format!(
            "{BUILDER}
declare const r: RawBuilder<string | number>
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
const t: StrRow | NumRow | undefined = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow | NumRow | undefined'.",
            ),
            (
                1,
                "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

// --- Nested same-generic drill: one indent level per layer ------------------

#[test]
fn nested_same_generic_argument_drill_keeps_per_layer_nesting() {
    // Depth guard for the union-source drill pull-up: a doubly-wrapped
    // same-generic pair drills one indent level per layer — the inner pair
    // line is a CHILD of the outer, never its sibling.
    assert_exact_chain(
        r#"
interface Crate<Held> { readonly held: Held }
declare const doubled: Crate<Crate<string>>
const want: Crate<Crate<number>> = doubled
"#,
        2322,
        &[
            (
                0,
                "Type 'Crate<Crate<string>>' is not assignable to type 'Crate<Crate<number>>'.",
            ),
            (
                1,
                "Type 'Crate<string>' is not assignable to type 'Crate<number>'.",
            ),
            (2, "Type 'string' is not assignable to type 'number'."),
        ],
    );
}

#[test]
fn nested_same_generic_union_argument_composes_both_drill_shapes() {
    // The inner layer's failing argument is itself a union: the nested
    // same-generic drill (no depth pull-up) composes with the union-source
    // drill (pulled up one level) into one contiguous chain.
    assert_exact_chain(
        r#"
interface Crate<Held> { readonly held: Held }
declare const mixedDeep: Crate<Crate<string | number>>
const wantDeep: Crate<Crate<number>> = mixedDeep
"#,
        2322,
        &[
            (
                0,
                "Type 'Crate<Crate<string | number>>' is not assignable to type 'Crate<Crate<number>>'.",
            ),
            (
                1,
                "Type 'Crate<string | number>' is not assignable to type 'Crate<number>'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'number'.",
            ),
            (3, "Type 'string' is not assignable to type 'number'."),
        ],
    );
}

// --- Negative controls -----------------------------------------------------

#[test]
fn no_same_base_member_keeps_missing_property_fold() {
    // No union member shares the source's base: the same-base step declines
    // and the pre-existing missing-property fold owns the elaboration.
    assert_exact_chain(
        &format!(
            "{BUILDER}
interface Other<Output> {{
  readonly expressionType: Output | undefined
  readonly isOther: true
}}
declare const r: RawBuilder<string | number>
const t: Other<string> | Other<number> = r
"
        ),
        2322,
        &[
            (
                0,
                "Type 'RawBuilder<string | number>' is not assignable to type 'Other<string> | Other<number>'.",
            ),
            (
                1,
                "Property 'isOther' is missing in type 'RawBuilder<string | number>' but required in type 'Other<number>'.",
            ),
            (1, "'isOther' is declared here."),
        ],
    );
}
