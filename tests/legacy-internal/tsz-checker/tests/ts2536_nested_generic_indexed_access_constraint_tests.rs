//! Regression coverage for TS2536 on *arbitrarily-deep* generic indexed accesses
//! into a concrete object base: `T[K1][K2][K3]…` where each `Ki` is a type
//! parameter constrained to the appropriate level's key space.
//!
//! tsc validates the outer key against the apparent type of the deferred base
//! (`getApparentType` / `getConstraintOfIndexedAccessType`): every reachable type
//! parameter is reduced to its constraint and the access is evaluated, so the
//! base carries a concrete key space even while staying syntactically deferred.
//! The single-level recovery only fired when the access *base* was itself a type
//! parameter (`K[idx]`); a nested access such as `T[K1][K2]` has an indexed-access
//! base, so prior to this fix its key space stayed deferred and a literal outer
//! key — e.g. `K3 extends keyof T[keyof T][keyof T[keyof T]]`, which reduces to a
//! concrete literal — could not be validated, yielding a spurious TS2536. This is
//! the depth-≥3 generalization of the depth-2 #13720 recovery, surfaced by the
//! valibot/kysely cross-arena families (#13212).
//!
//! Binder names vary across cases so no fixture/identifier string drives the
//! decision; a genuinely-missing key still emits TS2536 (the negative cases).

use crate::test_utils::check_source_codes;

/// Depth-3 access whose K-constraints are rooted at `keyof T` over a multi-key
/// (union-valued) base — the original false positive. tsc is clean.
#[test]
fn depth3_keyof_t_rooted_union_base_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Registry = { alpha: { inner: { leaf: 1 } }; beta: { inner: { leaf: 2 } } };
type Get<
  A extends keyof Registry,
  B extends keyof Registry[keyof Registry],
  C extends keyof Registry[keyof Registry][keyof Registry[keyof Registry]],
> = Registry[A][B][C];
type Probe = Get<"alpha", "inner", "leaf">;
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for `Registry[A][B][C]` rooted at `keyof Registry`: {codes:?}"
    );
}

/// Depth-3 access whose K-constraints are rooted at the `K1` chain
/// (`keyof T[K1]`) over a multi-key base. tsc is clean.
#[test]
fn depth3_k_chain_rooted_union_base_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Shape = { left: { mid: { tip: 1 } }; right: { mid: { tip: 2 } } };
type Pick3<
  P extends keyof Shape,
  Q extends keyof Shape[P],
  R extends keyof Shape[P][Q],
> = Shape[P][Q][R];
type Out = Pick3<"left", "mid", "tip">;
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for `Shape[P][Q][R]` rooted at the `P` chain: {codes:?}"
    );
}

/// Depth-4 access — the reduction must follow the full chain, not a fixed depth.
#[test]
fn depth4_nested_indexed_access_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Tree = { root: { branch: { twig: { leaf: 1 } } } };
type Reach<
  W extends keyof Tree,
  X extends keyof Tree[W],
  Y extends keyof Tree[W][X],
  Z extends keyof Tree[W][X][Y],
> = Tree[W][X][Y][Z];
type Leaf = Reach<"root", "branch", "twig", "leaf">;
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for depth-4 `Tree[W][X][Y][Z]`: {codes:?}"
    );
}

/// Multiple valid leaf keys at depth 3: indexing by a different-but-valid key
/// stays clean.
#[test]
fn depth3_multi_key_leaf_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Catalog = { one: { slot: { keep: 1; drop: 2 } }; two: { slot: { keep: 3; drop: 4 } } };
type Read<
  H extends keyof Catalog,
  I extends keyof Catalog[keyof Catalog],
  J extends keyof Catalog[keyof Catalog][keyof Catalog[keyof Catalog]],
> = Catalog[H][I][J];
type Kept = Read<"one", "slot", "drop">;
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire when the leaf key is a valid member: {codes:?}"
    );
}

/// Negative: a leaf key outside the apparent key space must still emit TS2536,
/// matching tsc. The reduction validates against the real apparent type, so it
/// cannot mask a genuinely-missing key.
#[test]
fn depth3_bogus_leaf_key_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Store = { up: { mid: { only: 1 } }; down: { mid: { only: 2 } } };
type Bad<
  M extends keyof Store,
  N extends keyof Store[keyof Store],
  O extends "absent",
> = Store[M][N][O];
"#,
    );
    assert!(
        codes.contains(&2536),
        "TS2536 must still fire for a leaf key absent from the apparent type: {codes:?}"
    );
}

/// Negative: a concrete literal leaf key that is not a member of the depth-3
/// apparent type must still emit TS2536.
#[test]
fn depth3_concrete_missing_literal_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Graph = { n0: { edge: { weight: 1 } } };
type Weight<S extends keyof Graph, T extends keyof Graph[S]> = Graph[S][T]["missing"];
"#,
    );
    assert!(
        codes.contains(&2536),
        "TS2536 must still fire for a concrete missing literal at depth 3: {codes:?}"
    );
}
