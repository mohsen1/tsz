//! Array-literal tuple context from a union contextual type.
//!
//! `tsc`'s `checkArrayLiteral` computes
//! `inTupleContext = !!contextualType && someType(contextualType,
//! isTupleLikeType)`: **one** tuple constituent is enough to make the literal a
//! tuple, and the remaining constituents are irrelevant to that decision. tsz
//! used to require *every* constituent to be a tuple, so any union pairing a
//! tuple with another shape widened the literal to an array and reported a
//! spurious `TS2322` on an element list that only ever satisfied the tuple arm.
//!
//! Witnessed by `superjson`'s `plainer.ts` (issue #15731), whose
//! `MinimisedTree<T> = Tree<T> | Record<string, Tree<T>> | undefined` pairs
//! tuple arms with a `Record` arm.
//!
//! Every expectation here is pinned against real `tsc` 7.0.2 output under
//! `--strict`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_with_options;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

fn assert_clean(source: &str, label: &str) {
    let diagnostics = check_strict(source);
    assert!(
        diagnostics.is_empty(),
        "{label}: expected no diagnostics, got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// The `superjson` witness, reduced: a generic recursive tuple alias unioned
/// with a `Record` arm, consumed through a property slot.
#[test]
fn tuple_alias_unioned_with_record_types_property_literal_as_tuple() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
type Tree<T> = [T] | [T, Rec<Tree<T>>];
type MinimisedTree<T> = Tree<T> | Rec<Tree<T>> | undefined;

interface Result {
    transformedValue: unknown;
    annotations?: MinimisedTree<string>;
}

declare const annotation: string;
const result: Result = { transformedValue: 1, annotations: [annotation] };
"#,
        "tuple alias | Record, property slot",
    );
}

/// Same alias shape in return position, with the two-element tuple arm.
#[test]
fn tuple_alias_unioned_with_record_types_return_literal_as_tuple() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
type Tree<T> = [T] | [T, Rec<Tree<T>>];
type MinimisedTree<T> = Tree<T> | Rec<Tree<T>> | undefined;

declare const annotation: string;
declare const children: Rec<Tree<string>>;

function build(withChildren: boolean): MinimisedTree<string> {
    if (withChildren) {
        return [annotation, children];
    }
    return [annotation];
}
"#,
        "tuple alias | Record, return position",
    );
}

/// The non-generic form of `ReferentialEqualityAnnotations`, which is where
/// `plainer.ts` lines 179/181 reported `never[]`.
#[test]
fn record_first_union_still_types_return_literal_as_tuple() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
type ReferentialEqualityAnnotations =
    | Rec<string[]>
    | [string[]]
    | [string[], Rec<string[]>];

declare const rootPaths: string[];
declare const rest: Rec<string[]>;

function annotate(dedupe: boolean): ReferentialEqualityAnnotations | undefined {
    if (dedupe) {
        return [rootPaths];
    }
    return [rootPaths, rest];
}
"#,
        "Record-first union, return position",
    );
}

/// A tuple arm paired with a plain array arm: `tsc` accepts, because the tuple
/// arm alone satisfies `someType(…, isTupleLikeType)` and the literal never has
/// to be checked against `number[]`.
#[test]
fn tuple_unioned_with_array_types_literal_as_tuple() {
    assert_clean(
        r#"
declare const s: string;
const value: [string] | number[] = [s];
const readonlyValue: [string] | readonly number[] = [s];
"#,
        "tuple | array",
    );
}

/// Non-array constituents do not veto the tuple arm either.
#[test]
fn tuple_unioned_with_non_array_constituents_types_literal_as_tuple() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
declare const s: string;
const withPrimitive: [string] | number = [s];
const withObject: [string] | { k: number } = [s];
const withNull: [string] | null = [s];
const withRecord: [string] | Rec<number> = [s];
"#,
        "tuple | non-array constituents",
    );
}

/// Renamed binders and an aliased indirection: the rule is structural, not
/// keyed to any particular alias or property name.
#[test]
fn renamed_binders_and_alias_indirection_behave_identically() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
type Zqx<Payload> = [Payload] | [Payload, Rec<Zqx<Payload>>];
type Wrapper<Payload> = Zqx<Payload> | Rec<Zqx<Payload>> | undefined;
type Aliased = Wrapper<number>;

declare const leaf: number;
const direct: Wrapper<number> = [leaf];
const aliased: Aliased = [leaf];
"#,
        "renamed binders through an alias",
    );
}

/// Nested literals: the inner literal sees the same union context through the
/// outer array's element type.
#[test]
fn nested_array_literal_in_union_element_context_stays_a_tuple() {
    assert_clean(
        r#"
type Rec<T> = { [key: string]: T };
declare const s: string;
const nested: ([string] | Rec<number>)[] = [[s]];
"#,
        "nested literal, union element context",
    );
}

/// Negative control, and the reason plain arrays must not count as tuple-like
/// on their own: with **no** tuple constituent the union stays ambiguous, and
/// `tsc` still reports `TS7006` for the conflicting callback arms.
#[test]
fn record_and_array_callback_union_without_a_tuple_arm_still_reports_ts7006() {
    let diagnostics = check_strict(
        r#"
type Rec<T> = { [key: string]: T };
const handlers: Rec<(arg: string) => void> | ((arg: number) => void)[] = [
    (arg) => {},
];
"#,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![7006],
        "ambiguous Record|Array callback union must keep TS7006, got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Negative control: tuple context does not make a wrong element list pass.
/// `tsc` reports `TS2322` here — "Type '[string, string]' is not assignable to
/// type 'Rec<string> | [string]'" — and the tuple in that message is
/// itself the evidence that the literal really did become a tuple.
#[test]
fn tuple_context_still_reports_a_genuinely_incompatible_literal() {
    let diagnostics = check_strict(
        r#"
type Rec<T> = { [key: string]: T };
declare const s: string;
const bad: [string] | Rec<string> = [s, s];
"#,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![2322],
        "over-long literal must still report TS2322, got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Negative control: an element of the wrong type still fails against the
/// tuple arm.
#[test]
fn tuple_context_still_reports_a_wrong_element_type() {
    let diagnostics = check_strict(
        r#"
type Rec<T> = { [key: string]: T };
declare const n: number;
const bad: [string] | Rec<string> = [n];
"#,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![2322],
        "wrong element type must still report TS2322, got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
