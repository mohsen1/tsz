//! Tests for homomorphic mapped type distribution over union types (issue #6564).
//!
//! When `{ [P in keyof T]: ... }` is instantiated with T = A | B, TypeScript
//! distributes to `{ [P in keyof A]: ... } | { [P in keyof B]: ... }`.
//! Without distribution, `keyof (A | B)` produces only common keys, losing
//! member-specific properties.
use tsz_checker::test_utils::check_source_diagnostics;

fn no_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no diagnostics, got: {relevant:#?}"
    );
}

// ---------------------------------------------------------------------------
// Basic distribution: member-specific properties must survive
// ---------------------------------------------------------------------------

/// `MyPartial<A | B>` must produce `MyPartial<A> | MyPartial<B>` so that
/// member-specific properties like `name` (only in `NodeA`) and `id` (only in
/// `NodeB`) remain accessible.  Without distribution, both are lost because
/// `keyof (NodeA | NodeB)` collapses to only the common key `type`.
#[test]
fn homomorphic_partial_union_preserves_member_specific_properties() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }

type MyPartial<T> = { [K in keyof T]?: T[K] }

// Both `name` (NodeA-only) and `id` (NodeB-only) must exist after distribution.
const a: MyPartial<NodeA | NodeB> = { type: "A", name: "hello" };
const b: MyPartial<NodeA | NodeB> = { type: "B", id: 42 };
"#,
    );
}

/// Object literal with only the common property is also valid (both branches have `type`).
#[test]
fn homomorphic_partial_union_common_key_still_valid() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }

type MyPartial<T> = { [K in keyof T]?: T[K] }
const c: MyPartial<NodeA | NodeB> = { type: "A" };
"#,
    );
}

/// `Readonly<T>` distributed over a union of different shapes.
#[test]
fn homomorphic_readonly_distributes_over_union() {
    no_errors(
        r#"
type MyReadonly<T> = { readonly [K in keyof T]: T[K] }

type A = { x: number }
type B = { y: string }

// Both x (A-only) and y (B-only) must exist
const _a: MyReadonly<A | B> = { x: 1 };
const _b: MyReadonly<A | B> = { y: "hi" };
"#,
    );
}

// ---------------------------------------------------------------------------
// Three-member union
// ---------------------------------------------------------------------------

/// Distribution over a three-member union: each member's unique properties
/// must appear in the distributed result.
#[test]
fn homomorphic_mapped_three_member_union_distributes() {
    no_errors(
        r#"
type Cat  = { kind: "cat";  purr: boolean }
type Dog  = { kind: "dog";  bark: boolean }
type Fish = { kind: "fish"; swim: boolean }

type MyPartial<T> = { [K in keyof T]?: T[K] }

// purr, bark, swim are member-specific — all must remain accessible
const c: MyPartial<Cat | Dog | Fish> = { kind: "cat", purr: true };
const d: MyPartial<Cat | Dog | Fish> = { kind: "dog", bark: false };
const f: MyPartial<Cat | Dog | Fish> = { kind: "fish", swim: true };
"#,
    );
}

// ---------------------------------------------------------------------------
// Mapped type with constant template (non-identity)
// ---------------------------------------------------------------------------

/// When the template is a constant (not `T[K]`), distribution should still
/// happen: each union member gets its own key set.
#[test]
fn homomorphic_constant_template_distributes_over_union() {
    no_errors(
        r#"
// { [K in keyof T]: boolean } — template doesn't reference T[K]
type Flagged<T> = { [K in keyof T]: boolean }

type A = { x: number; y: string }
type B = { z: boolean }

// After distribution: { x: boolean; y: boolean } | { z: boolean }
const _a: Flagged<A | B> = { x: true, y: false };  // NodeA branch
const _b: Flagged<A | B> = { z: true };             // NodeB branch
"#,
    );
}

// ---------------------------------------------------------------------------
// Verify that the distribution result is a union (assignability both ways)
// ---------------------------------------------------------------------------

/// A value valid for the `NodeA` branch must be assignable to the distributed type.
#[test]
fn homomorphic_distributed_union_accepts_branch_a_value() {
    no_errors(
        r#"
type A = { a: number; shared: string }
type B = { b: boolean; shared: string }

type Identity<T> = { [K in keyof T]: T[K] }

// After distribution: { a: number; shared: string } | { b: boolean; shared: string }
declare const val_a: { a: number; shared: string };
const _: Identity<A | B> = val_a;
"#,
    );
}

/// A value valid for the `NodeB` branch must also be assignable.
#[test]
fn homomorphic_distributed_union_accepts_branch_b_value() {
    no_errors(
        r#"
type A = { a: number; shared: string }
type B = { b: boolean; shared: string }

type Identity<T> = { [K in keyof T]: T[K] }

declare const val_b: { b: boolean; shared: string };
const _: Identity<A | B> = val_b;
"#,
    );
}
