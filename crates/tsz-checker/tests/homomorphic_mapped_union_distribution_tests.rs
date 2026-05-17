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

// ---------------------------------------------------------------------------
// Named union alias: distribution must work identically to literal unions
// ---------------------------------------------------------------------------

/// When a distributive conditional wraps a homomorphic mapped type and the arg
/// is a NAMED union alias (`type Nodes = A | B`), the distribution over union
/// members must be identical to a literal union (`A | B`).
///
/// Structural rule: `F<type Alias = A | B>` must equal `F<A | B>` for any
/// distributive conditional generic `F`.
#[test]
fn distributive_conditional_named_alias_distributes_like_literal_union() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type RKSimple<U> = U extends U ? { [P in keyof U]: U[P] } : never

type RS2 = RKSimple<Nodes>
declare const e: RS2

const _a: { type: "A"; name: string } = { type: "A", name: "hello" }
const _b: RS2 = _a
"#,
    );
}

/// Extract on `RKSimple<Nodes>` must distribute correctly — the result must
/// have member-specific properties of the extracted branch.
#[test]
fn extract_on_distributive_conditional_with_named_union_alias() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type RKSimple<U> = U extends U ? { [P in keyof U]: U[P] } : never

type RS2 = RKSimple<Nodes>
type ExtRS2 = Extract<RS2, { type: "A" }>

declare const e: ExtRS2
const name: string = e.name
"#,
    );
}

/// Renamed type parameter: distribution must not depend on the parameter name.
#[test]
fn distributive_conditional_renamed_param_named_alias_distributes() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type RKSimple<X> = X extends X ? { [K in keyof X]: X[K] } : never

type RS2 = RKSimple<Nodes>
type ExtRS2 = Extract<RS2, { type: "A" }>

declare const e: ExtRS2
const name: string = e.name
"#,
    );
}

/// Wrapper alias: named alias wrapped in another alias must also distribute.
#[test]
fn distributive_conditional_nested_alias_distributes() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB
type MoreNodes = Nodes

type RKSimple<U> = U extends U ? { [P in keyof U]: U[P] } : never

type RS3 = RKSimple<MoreNodes>
type ExtRS3 = Extract<RS3, { type: "A" }>

declare const e: ExtRS3
const name: string = e.name
"#,
    );
}

// ---------------------------------------------------------------------------
// ReplaceKeys pattern: complex mapped type inside distributive conditional
// ---------------------------------------------------------------------------

/// Issue #6813: `ReplaceKeys<Nodes, ...>` produces a union; `Extract` on that
/// union should correctly identify the matching member, not return `never`.
///
/// Structural rule: when a distributive conditional type `U extends U ? MappedOf<U> : never`
/// is instantiated with a named union alias `type Nodes = A | B`, Extract on
/// the result must distribute over the per-member mapped types.
#[test]
fn replace_keys_named_union_alias_extract_is_not_never() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T extends string, Y> = U extends U ? {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
} : never

type Replaced = ReplaceKeys<Nodes, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

/// Literal union variant of the same pattern: must also work and match named alias.
#[test]
fn replace_keys_literal_union_extract_is_not_never() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }

type ReplaceKeys<U, T extends string, Y> = U extends U ? {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
} : never

type Replaced = ReplaceKeys<NodeA | NodeB, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

/// Renamed type parameters: same fix must work regardless of parameter names.
#[test]
fn replace_keys_renamed_params_named_union_extract_is_not_never() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<X, Q extends string, Z> = X extends X ? {
  [J in keyof X]: J extends Q ? (J extends keyof Z ? Z[J] : never) : X[J]
} : never

type Replaced = ReplaceKeys<Nodes, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

/// Issue #6813 exact repro: homomorphic `ReplaceKeys` (no explicit distributive
/// conditional) applied to a named union alias, then Extract on the result.
/// tsc distributes homomorphic mapped types over unions, so the result should be
/// a union, and Extract should correctly identify the matching member.
#[test]
fn replace_keys_homomorphic_no_explicit_conditional_named_alias_extract() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T extends string, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type Replaced = ReplaceKeys<Nodes, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

#[test]
fn replace_keys_homomorphic_no_explicit_conditional_literal_union_extract() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }

type ReplaceKeys<U, T extends string, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type Replaced = ReplaceKeys<NodeA | NodeB, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

/// Interface form: same as above but with `interface` declarations instead of types.
#[test]
fn replace_keys_homomorphic_interfaces_named_alias_extract() {
    no_errors(
        r#"
interface NodeA { type: "A"; name: string }
interface NodeB { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type Replaced = ReplaceKeys<Nodes, "name", { name: number }>
type ExtractedA = Extract<Replaced, { type: "A" }>

declare const a: ExtractedA
const aName: number = a.name
"#,
    );
}

/// Bounded type parameter variant: when T extends keyof U and U is a union alias,
/// distribution over union members is applied per-member so member-specific keys
/// satisfy the constraint within each branch.
#[test]
fn replace_keys_bounded_params_per_member_named_alias_extract() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }

type RK<U, T extends keyof U, Y> = U extends U ? {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
} : never

type Replaced = RK<NodeA | NodeB, "type", { type: "X" }>
type ExtractedA = Extract<Replaced, { type: "X" }>

declare const a: ExtractedA
const t: "X" = a.type
"#,
    );
}

/// Exact repro from issue #6813: interfaces, named union alias `Nodes`, and
/// Extract on the result of a homomorphic ReplaceKeys mapped type.
#[test]
fn issue_6813_exact_repro_extract_from_replace_keys_named_union() {
    no_errors(
        r#"
interface NodeA { type: "A"; name: string }
interface NodeB { type: "B"; id: number }

type Nodes = NodeA | NodeB;

type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T
    ? K extends keyof Y
      ? Y[K]
      : never
    : U[K]
};

type Replaced = ReplaceKeys<Nodes, "name", { name: number }>;
type ExtractedA = Extract<Replaced, { type: "A" }>;

declare const a: ExtractedA;
const aName: number = a.name;
"#,
    );
}

// ---------------------------------------------------------------------------
// Stdlib utility types with named union aliases
// ---------------------------------------------------------------------------

/// `Partial<Nodes>` must distribute over the named union alias.
/// `Extract<Partial<Nodes>, {type:"A"}>` must give the partial NodeA shape.
#[test]
fn extract_on_builtin_partial_named_union_alias() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type P = Partial<Nodes>
type ExtA = Extract<P, { type?: "A" }>

declare const e: ExtA
const _type: "A" | undefined = e.type
const _name: string | undefined = e.name
"#,
    );
}

/// `Readonly<Nodes>` must distribute over the named union alias.
#[test]
fn extract_on_builtin_readonly_named_union_alias() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type R = Readonly<Nodes>
type ExtA = Extract<R, { type: "A" }>

declare const e: ExtA
const _name: string = e.name
"#,
    );
}

/// `Exclude<Nodes, ...>` with named union alias.
#[test]
fn exclude_on_named_union_alias_gives_other_member() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ExclA = Exclude<Nodes, { type: "A" }>

declare const e: ExclA
const _id: number = e.id
"#,
    );
}

/// Multi-level alias: the APPLICATION result is itself a named type alias (not inline).
/// `type RS2 = ReplaceKeys<Nodes, ...>; type ExtA = Extract<RS2, ...>` must work.
#[test]
fn extract_on_named_application_alias_not_inline() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type RS2 = ReplaceKeys<Nodes, "name", { name: number }>
type ExtA = Extract<RS2, { type: "A" }>

declare const a: ExtA
const aName: number = a.name
"#,
    );
}

/// Double alias: result alias referenced through another alias.
#[test]
fn extract_on_double_aliased_application_result() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type RS2 = ReplaceKeys<Nodes, "name", { name: number }>
type RS2Alias = RS2
type ExtA = Extract<RS2Alias, { type: "A" }>

declare const a: ExtA
const aName: number = a.name
"#,
    );
}

/// Partial result: Extract on Partial<ReplaceKeys<Nodes>>.
#[test]
fn extract_on_partial_of_replace_keys_named_alias() {
    no_errors(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T ? (K extends keyof Y ? Y[K] : never) : U[K]
}

type RS2 = ReplaceKeys<Nodes, "name", { name: number }>
type PR = Partial<RS2>
type ExtA = Extract<PR, { type?: "A" }>

declare const a: ExtA
const t: "A" | undefined = a.type
"#,
    );
}

/// Negative case: extract of a non-existent branch gives never.
#[test]
fn extract_on_distributive_conditional_named_alias_nonexistent_branch_is_never() {
    // `ExtBad` resolves to `never` — no TS2339 should fire on `never`-typed access.
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
type NodeA = { type: "A"; name: string }
type NodeB = { type: "B"; id: number }
type Nodes = NodeA | NodeB

type RKSimple<U> = U extends U ? { [P in keyof U]: U[P] } : never

type RS2 = RKSimple<Nodes>
type ExtBad = Extract<RS2, { type: "C" }>

declare const e: ExtBad
const x: string = e as any
"#,
    );
    let has_2339 = diags.iter().any(|d| d.code == 2339);
    assert!(
        !has_2339,
        "should not emit TS2339 on `never` access, got: {diags:#?}"
    );
}
