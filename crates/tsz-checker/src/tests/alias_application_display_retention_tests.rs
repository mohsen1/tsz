//! Regression tests for tsc's `aliasSymbol` retention policy on generic
//! type-alias applications in assignability diagnostics (issue #15368).
//!
//! The structural rule: instantiation that *resolves the type away* — a
//! conditional whose branch is taken, a resolved indexed access or `keyof`,
//! or an alias-forwarding chain bottoming out at one of those — drops the
//! alias symbol, so the diagnostic renders the evaluated type. Constructors
//! that *survive* instantiation (mapped, union, object) keep the alias and
//! render `Name<Args>`; a nullable union target that keeps its alias is not
//! stripped to its non-nullish member.
//!
//! Owners: `tsz_solver::diagnostics::format::application_reduction` (shared
//! display reduction), `type_queries::application_base_reducing_alias_body_kind`,
//! and the checker's `render_missing_property` primitive-source target display.

use crate::test_utils::check_source_diagnostics;

/// The single TS2322 message produced by `source`.
fn ts2322_message(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2322[0].message_text.clone()
}

// ── Reducing bodies drop the alias ──

#[test]
fn indexed_access_bodied_alias_application_renders_member_type() {
    let message = ts2322_message(
        r#"
type PickInner<Q extends { inner: unknown }> = Q['inner'];
const wrong: PickInner<{ inner: { deep: boolean } }> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ deep: boolean; }'."
    );
}

#[test]
fn keyof_bodied_alias_application_renders_keys_in_declaration_order() {
    let message = ts2322_message(
        r#"
type KeysOf<Rec> = keyof Rec;
const wrong: KeysOf<{ zebra: 1; apple: 2 }> = 5;
"#,
    );
    // Property declaration order (`zebra` before `apple`), not the interner's
    // canonical sort.
    assert_eq!(
        message,
        "Type '5' is not assignable to type '\"zebra\" | \"apple\"'."
    );
}

#[test]
fn alias_forwarding_to_conditional_alias_renders_resolved_branch() {
    let message = ts2322_message(
        r#"
type Choose<V> = V extends string ? { picked: V } : { fallback: V };
type Forwarded<W> = Choose<W>;
const wrong: Forwarded<string> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ picked: string; }'."
    );
}

#[test]
fn converging_recursive_conditional_alias_renders_reduced_primitive() {
    let message = ts2322_message(
        r#"
type Unwrap<E> = E extends readonly (infer Inner)[] ? Unwrap<Inner> : E;
const wrong: Unwrap<string[][]> = 5;
"#,
    );
    assert_eq!(message, "Type 'number' is not assignable to type 'string'.");
}

#[test]
fn converging_recursive_conditional_alias_renders_reduced_object() {
    let message = ts2322_message(
        r#"
type Peel<E> = E extends readonly (infer Inner)[] ? Peel<Inner> : E;
const wrong: Peel<{ leaf: 1 }[][]> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ leaf: 1; }'."
    );
}

// ── Surviving constructors keep the alias ──

#[test]
fn mapped_bodied_alias_application_keeps_alias_name() {
    let message = ts2322_message(
        r#"
type Identityish<S> = { [K in keyof S]: S[K] };
const wrong: Identityish<{ m: string }> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type 'Identityish<{ m: string; }>'."
    );
}

#[test]
fn union_bodied_alias_application_keeps_alias_and_is_not_nullish_stripped() {
    let message = ts2322_message(
        r#"
type OrMissing<S> = S | undefined;
const wrong: OrMissing<{ u: string }> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type 'OrMissing<{ u: string; }>'."
    );
}

#[test]
fn non_generic_union_alias_target_keeps_alias_name() {
    let message = ts2322_message(
        r#"
type MaybeBox = { u: string } | undefined;
const wrong: MaybeBox = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type 'MaybeBox'."
    );
}

#[test]
fn anonymous_nullable_union_target_still_strips_to_non_nullish_member() {
    let message = ts2322_message(
        r#"
const wrong: { u: string } | undefined = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ u: string; }'."
    );
}

// ── Negative / fallback cases ──

#[test]
fn still_generic_reducing_application_keeps_alias_spelling() {
    // A free type parameter defers the reduction; tsc keeps `Name<Args>`.
    let diags = check_source_diagnostics(
        r#"
type Choose<V> = V extends string ? { picked: V } : { fallback: V };
function take<W>(w: W) {
    const wrong: string = null as unknown as Choose<W>;
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.starts_with("Type 'Choose<W>'"),
        "a still-generic conditional application keeps its alias spelling, got: {}",
        ts2322[0].message_text
    );
}

#[test]
fn non_converging_recursive_tuple_alias_keeps_alias_annotation() {
    // The recursive tuple alias never converges; expanding it would render a
    // truncated cycle, so the annotation surface is preserved.
    let diags = check_source_diagnostics(
        r#"
type Nest<T> = [42, Nest<{ x: T }>];
declare const n: Nest<number>;
const wrong: string = n;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.contains("Nest<"),
        "a non-converging recursive alias keeps its alias surface, got: {}",
        ts2322[0].message_text
    );
}

#[test]
fn distributive_conditional_over_inline_union_renders_distributed_branches() {
    // Self-contained `Omit` equivalent (the unit harness carries no lib).
    let message = ts2322_message(
        r#"
type Excl<A, B> = A extends B ? never : A;
type DropKey<T, K extends string> = { [P in Excl<keyof T, K>]: T[P] };
type NoC<T> = T extends unknown ? DropKey<T, 'c'> : never;
declare const val: NoC<{ kind: 'a'; x: number; c: boolean } | { kind: 'b'; y: string; c: boolean }>;
const wrong: { kind: 'a'; x: number; c: boolean } = val;
"#,
    );
    assert_eq!(
        message,
        "Type 'DropKey<{ kind: \"a\"; x: number; c: boolean; }, \"c\"> | DropKey<{ kind: \"b\"; y: string; c: boolean; }, \"c\">' is not assignable to type '{ kind: \"a\"; x: number; c: boolean; }'."
    );
}

#[test]
fn keyof_alias_with_numeric_keys_keeps_alias_name() {
    // Numeric property names resolve to number-literal keys, which the
    // declaration-order reconstruction does not cover; the alias surface is
    // preserved rather than rendering a mis-ordered union.
    let diags = check_source_diagnostics(
        r#"
type KeysOf<Rec> = keyof Rec;
const wrong: KeysOf<{ 1: true; 0: false }> = 'nope';
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.contains("KeysOf<")
            || ts2322[0].message_text.contains("0 | 1")
            || ts2322[0].message_text.contains("1 | 0"),
        "numeric keyof falls back without asserting a specific order, got: {}",
        ts2322[0].message_text
    );
}
