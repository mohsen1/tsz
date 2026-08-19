//! An enum type or enum member is displayed bare in diagnostics; the
//! namespace-qualified spelling appears only for a generalized relation
//! source (#17661 residual b).
//!
//! tsc renders types through `typeToString`, which names an enum by its own
//! symbol regardless of the namespace containing it: `namespace P { export
//! enum Q { R, S } }` prints `Q` for the enum and `Q.R` for a member — the
//! member is qualified by its *parent enum*, never by the enclosing
//! namespace, and never by the alias or annotation the reference was written
//! with (`type MA = Q.R` is a shared singleton with no `aliasSymbol`).
//!
//! The one exception is `reportRelationError`'s generalized source: when the
//! failing source is enum-ish and the target cannot hold a top-level
//! singleton, tsc widens the source (`getBaseTypeOfLiteralType`: member ->
//! parent enum) and renders it through `getTypeNameForErrorDisplay`, i.e.
//! with `TypeFormatFlags.UseFullyQualifiedType` — `P.Q`. A singleton-capable
//! target (a literal, an enum, an enum member, or a union holding one)
//! generalizes nothing and keeps the bare spelling.
//!
//! Every expectation in this file is pinned against `tsc` 7.0.2 on the exact
//! source text below.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

fn assert_messages(source: &str, expected: &[(u32, &str)], context: &str) {
    let diagnostics = check_source_diagnostics(source);
    let actual = diagnostic_code_message_refs(&diagnostics);
    assert_eq!(
        actual, expected,
        "{context}: diagnostics did not match the tsc 7.0.2 oracle",
    );
}

// ---------------------------------------------------------------------------
// Generalized source: fully qualified (`P.Q`).
// ---------------------------------------------------------------------------

#[test]
fn namespaced_enum_source_against_non_singleton_target_is_fully_qualified() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q;
const g: string = w;
"#,
        &[(2322, "Type 'P.Q' is not assignable to type 'string'.")],
        "whole enum generalizes against `string` (tsc `UseFullyQualifiedType`)",
    );
}

#[test]
fn namespaced_enum_member_source_generalizes_to_the_qualified_parent_enum() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q.S;
const g: string = w;
"#,
        &[(2322, "Type 'P.Q' is not assignable to type 'string'.")],
        "member widens to its parent enum, then fully qualifies",
    );
}

#[test]
fn deeply_nested_namespace_enum_source_qualifies_every_segment() {
    assert_messages(
        r#"
namespace Outer { export namespace Inner { export enum Deep { D1, D2 } } }
declare const w: Outer.Inner.Deep;
const g: string = w;
"#,
        &[(
            2322,
            "Type 'Outer.Inner.Deep' is not assignable to type 'string'.",
        )],
        "two enclosing namespaces both appear in the generalized source",
    );
}

#[test]
fn renamed_binders_keep_the_generalized_qualification() {
    assert_messages(
        r#"
namespace Zone { export enum Signal { Go, Halt } }
declare const reading: Zone.Signal;
const label: string = reading;
"#,
        &[(
            2322,
            "Type 'Zone.Signal' is not assignable to type 'string'.",
        )],
        "renamed namespace/enum/member/variable binders (no identifier dependence)",
    );
}

#[test]
fn string_enum_source_qualifies_the_same_way() {
    assert_messages(
        r#"
namespace P { export enum Chan { Email = "email", Sms = "sms" } }
declare const w: P.Chan;
const g: number = w;
"#,
        &[(2322, "Type 'P.Chan' is not assignable to type 'number'.")],
        "string enum, non-singleton target",
    );
}

#[test]
fn nested_property_leaf_generalizes_and_qualifies() {
    // The elaboration chain lives in `related_information`; only the head line
    // is a top-level diagnostic.
    let diagnostics = check_source_diagnostics(
        r#"
namespace P { export enum Q { R, S } }
declare const w: { m: P.Q };
const g: { m: string } = w;
"#,
    );
    assert_eq!(
        diagnostic_code_message_refs(&diagnostics),
        [(
            2322,
            "Type '{ m: Q; }' is not assignable to type '{ m: string; }'."
        )],
        "the enclosing object shape renders its enum property bare",
    );
    let chain: Vec<&str> = diagnostics
        .first()
        .expect("one TS2322")
        .related_information
        .iter()
        .map(|related| related.message_text.as_str())
        .collect();
    assert_eq!(
        chain,
        [
            "Types of property 'm' are incompatible.",
            "Type 'P.Q' is not assignable to type 'string'.",
        ],
        "the property leaf generalizes and fully qualifies (tsc runs \
         `reportRelationError` on every relation line)",
    );
}

#[test]
fn generalized_argument_source_qualifies() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare function takeStr(x: string): void;
declare const w: P.Q.S;
takeStr(w);
"#,
        &[(
            2345,
            "Argument of type 'P.Q' is not assignable to parameter of type 'string'.",
        )],
        "TS2345 argument surface generalizes like the TS2322 assignment surface",
    );
}

// ---------------------------------------------------------------------------
// Singleton-capable target: no generalization, bare spelling.
// ---------------------------------------------------------------------------

#[test]
fn namespaced_enum_against_its_own_member_target_stays_bare() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q;
const g: P.Q.R = w;
"#,
        &[(2322, "Type 'Q' is not assignable to type 'Q.R'.")],
        "an enum-member target is singleton-capable, so nothing generalizes",
    );
}

#[test]
fn namespaced_member_against_a_sibling_member_target_stays_bare() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q.S;
const g: P.Q.R = w;
"#,
        &[(2322, "Type 'Q.S' is not assignable to type 'Q.R'.")],
        "member-vs-member keeps both operands bare and parent-qualified",
    );
}

#[test]
fn cross_enum_pair_stays_bare_on_both_sides() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
namespace P2 { export enum Q2 { R2, S2 } }
declare const w: P.Q;
const g: P2.Q2 = w;
"#,
        &[(2322, "Type 'Q' is not assignable to type 'Q2'.")],
        "an enum target is singleton-capable; distinct names need no qualification",
    );
}

#[test]
fn namespaced_enum_against_a_literal_target_stays_bare() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q;
const g: "lit" = w;
"#,
        &[(2322, "Type 'Q' is not assignable to type '\"lit\"'.")],
        "a string-literal target is singleton-capable",
    );
}

#[test]
fn namespaced_enum_against_a_literal_union_target_stays_bare() {
    assert_messages(
        r#"
namespace P { export enum Q { R, S } }
declare const w: P.Q;
const g: "a" | "b" = w;
"#,
        &[(2322, "Type 'Q' is not assignable to type '\"a\" | \"b\"'.")],
        "a union holding literals is singleton-capable",
    );
}

// ---------------------------------------------------------------------------
// Top-level enums: never namespace-qualified, and unaffected by the split.
// ---------------------------------------------------------------------------

#[test]
fn top_level_enum_renders_bare_against_both_target_kinds() {
    assert_messages(
        r#"
enum Mode { A, B }
declare const m: Mode;
const s: string = m;
const t: Mode.A = m;
"#,
        &[
            (2322, "Type 'Mode' is not assignable to type 'string'."),
            (2322, "Type 'Mode' is not assignable to type 'Mode.A'."),
        ],
        "a top-level enum has no namespace to qualify with, on either path",
    );
}

#[test]
fn top_level_enum_member_keeps_its_parent_enum_qualifier() {
    assert_messages(
        r#"
enum Mode { A, B }
declare const a: Mode.A;
const t: Mode.B = a;
"#,
        &[(2322, "Type 'Mode.A' is not assignable to type 'Mode.B'.")],
        "member display is parent-enum-qualified, not namespace-qualified",
    );
}

#[test]
fn single_member_enum_keeps_the_tsc_identity_display() {
    assert_messages(
        r#"
enum One { Only }
declare const o: One.Only;
const s: string = o;
"#,
        &[(2322, "Type 'One' is not assignable to type 'string'.")],
        "a single-member enum's member type IS the enum type (bare name, no `One.Only`)",
    );
}
