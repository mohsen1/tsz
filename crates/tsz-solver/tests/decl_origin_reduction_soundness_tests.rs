//! Adversarial soundness gate for the #14345 WAVE-1 decl-origin-through-reduction
//! consult (branch `green/wave1-cowalk`).
//!
//! The consult (`check_subtype`, `cache.rs`) accepts two reduced-body
//! `TypeParameter` leaves when their carried `DeclScoped { file, node }` origins
//! form a registered alpha-rename pair. The prior co-walk was UNSOUND; this
//! module proves the origin-keyed consult does NOT register/accept a FALSE
//! `T ≡ U` correspondence while still accepting the genuine same-decl `B ≡ A`
//! alpha-rename.
//!
//! These tests exercise the REAL relation path (`SubtypeChecker::check_subtype`
//! with a populated `type_param_equivalences` table), not just the isolated
//! `matches_origins` helper. They run under the flag-ON process env
//! (`TSZ_TYPEPARAM_DECL_IDENTITY=1 TSZ_DECL_ORIGIN_REDUCTION=1`); when the flag
//! is OFF the origin branch is inert and only the id-keyed match applies, which
//! these tests also tolerate (the false pair still must not relate, and the
//! genuine pair must still relate BY ID — which it does when the leaf ids equal
//! the registered ids).

use crate::construction::TypeInterner;
use crate::relations::subtype::{SubtypeChecker, TypeParamEquivalence};
use crate::types::{TypeData, TypeId, TypeParamInfo, TypeParamOrigin};
use tsz_common::interner::Atom;

/// Whether the decl-origin consult is active in THIS process (both gate flags
/// set). The env is read once per process by the checker's `OnceLock`; we mirror
/// the same predicate so the assertions can branch on the real gate state.
fn origin_consult_active() -> bool {
    std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").as_deref() == Ok("1")
        && std::env::var("TSZ_DECL_ORIGIN_REDUCTION").as_deref() == Ok("1")
}

fn decl(file: u32, node: u32) -> TypeParamOrigin {
    TypeParamOrigin::DeclScoped {
        file: Atom(file),
        node,
    }
}

/// Build a `TypeParameter` leaf whose surface name is `name` and whose origin is
/// `origin`. Two leaves with the SAME name but DIFFERENT origin intern to
/// DISTINCT ids (the decl-identity stamp differentiates the bare info), which is
/// exactly the name-collision the WAVE-1 fix must survive.
fn param_leaf(interner: &TypeInterner, name: &str, origin: TypeParamOrigin) -> TypeId {
    interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin,
    }))
}

// ---------------------------------------------------------------------------
// (1) OVER-RELATE ORACLE
// ---------------------------------------------------------------------------

/// The core over-relate oracle: two structurally-identical-at-leaf but
/// DECL-DISTINCT params (`T` from one decl site, `U` from another; and a THIRD
/// pair `V`/`W`) must NOT be bridged by the origin consult unless their origin
/// pair was actually registered by the alpha-rename.
///
/// Setup: the alpha-rename registered `(T_origin, U_origin)` — the two signatures'
/// aligned params. The reduced bodies then present a DIFFERENT leaf pair whose
/// origins were NEVER registered (`V`/`W`, the es5-Array-map-element-T vs
/// result-U-via-HKT witnesses — distinct declaration sites). The consult must
/// reject them (`SubtypeResult != True` from the equivalence branch).
#[test]
fn over_relate_oracle_rejects_unregistered_origin_pair() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Registered alpha-rename pair: signature params T (file 57, node 20) and
    // U (file 5, node 20). These are the ONLY origins the co-walk registered.
    let t_leaf = param_leaf(&interner, "T", decl(57, 20));
    let u_leaf = param_leaf(&interner, "U", decl(5, 20));
    checker.type_param_equivalences.push(TypeParamEquivalence {
        source: t_leaf,
        target: u_leaf,
        origins: Some((decl(57, 20), decl(5, 20))),
    });

    // KNOWN-FALSE leaf pair: V (file 88, node 3) and W (file 91, node 7) —
    // distinct declaration sites that were NEVER registered. In the
    // over-relate witness these are the Array element-T of `map` vs the result-U
    // reached via HKT: structurally two bare params, semantically unrelated.
    let v_leaf = param_leaf(&interner, "T", decl(88, 3));
    let w_leaf = param_leaf(&interner, "U", decl(91, 7));

    // The equivalence consult must NOT relate V and W: their origin pair is not
    // registered, and their ids are not registered either. `check_subtype` may
    // still return False via ordinary structural rules — what matters is it does
    // NOT return True, i.e. the origin consult did not mint a false T≡U.
    let related = checker.check_subtype(v_leaf, w_leaf).is_true();
    assert!(
        !related,
        "OVER-RELATE: unregistered decl-distinct params V/W must not relate \
         via the origin consult (registered pair was T/U only)"
    );
}

/// Sharper over-relate: one leaf DOES carry a registered origin, but the OTHER
/// leaf carries a THIRD, unregistered origin. A half-match must be rejected — the
/// origin consult requires BOTH sides to match the registered pair.
#[test]
fn over_relate_oracle_rejects_half_registered_origin_pair() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_leaf = param_leaf(&interner, "T", decl(57, 20));
    let u_leaf = param_leaf(&interner, "U", decl(5, 20));
    checker.type_param_equivalences.push(TypeParamEquivalence {
        source: t_leaf,
        target: u_leaf,
        origins: Some((decl(57, 20), decl(5, 20))),
    });

    // Source leaf carries the registered T origin; target leaf carries a THIRD
    // origin (file 5, node 21 — same file as U but a DIFFERENT declaration node).
    let src = param_leaf(&interner, "T", decl(57, 20));
    let bad_target = param_leaf(&interner, "U", decl(5, 21));

    let related = checker.check_subtype(src, bad_target).is_true();
    assert!(
        !related,
        "OVER-RELATE: a half-registered origin pair (registered T vs unregistered \
         node-21 param) must not relate"
    );
}

/// `Carrier<{v:T}>` vs `Carrier<{v:U}>` at the leaf: when T and U are distinct
/// declarations and NOTHING was registered, the params must not relate. This is
/// the empty-registry control — the origin consult has no entry to fire on, so a
/// leaf pair with distinct origins stays unrelated.
#[test]
fn over_relate_oracle_empty_registry_keeps_distinct_params_apart() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_leaf = param_leaf(&interner, "T", decl(57, 20));
    let u_leaf = param_leaf(&interner, "U", decl(5, 20));

    // No equivalence registered at all.
    let related = checker.check_subtype(t_leaf, u_leaf).is_true();
    assert!(
        !related,
        "OVER-RELATE: with an empty equivalence registry, decl-distinct params \
         T and U must not relate"
    );
}

/// Even with a registered pair present, a leaf pair where BOTH leaves share the
/// SAME origin as ONE registered side (i.e. both are `T`) must not be coerced
/// into relating a `T`-vs-something-else via the origin table. Here we probe two
/// `T` leaves against the registered `(T,U)` — the origin match requires the
/// pair `(T,U)`, and `(T,T)` is not registered, so it must not fire.
#[test]
fn over_relate_oracle_same_origin_both_sides_not_registered_as_pair() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_leaf = param_leaf(&interner, "T", decl(57, 20));
    let u_leaf = param_leaf(&interner, "U", decl(5, 20));
    checker.type_param_equivalences.push(TypeParamEquivalence {
        source: t_leaf,
        target: u_leaf,
        origins: Some((decl(57, 20), decl(5, 20))),
    });

    // Two DISTINCT leaves both carrying the T origin but a third leaf carrying a
    // never-registered origin. `(T_origin, X_origin)` is not a registered pair.
    let another_t = param_leaf(&interner, "T", decl(57, 20));
    let unregistered = param_leaf(&interner, "Z", decl(200, 200));

    // Note: `another_t` and `t_leaf` are the SAME interned id (same info), so
    // check them against a truly-unregistered origin.
    let related = checker.check_subtype(another_t, unregistered).is_true();
    assert!(
        !related,
        "OVER-RELATE: T paired with an unregistered origin Z must not relate"
    );
}

// ---------------------------------------------------------------------------
// (2) POSITIVE CONTROL — genuine same-decl B ≡ A alpha-rename accepted.
// ---------------------------------------------------------------------------

/// The genuine WAVE-1 positive case: two reduced-body leaves whose `(file, node)`
/// origins equal the registered signature-param pair MUST relate through the
/// origin consult — but ONLY when the flag is on. When the flag is off, the
/// origin branch is inert; the id-keyed match still relates them because we
/// register the exact leaf ids too (mirroring the real registration which pushes
/// both the ids and the origins).
#[test]
fn positive_control_genuine_same_decl_pair_relates() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Registered alpha-rename: A (file 57, node 20) ≡ B (file 5, node 20).
    let a_leaf = param_leaf(&interner, "A", decl(57, 20));
    let b_leaf = param_leaf(&interner, "B", decl(5, 20));
    checker.type_param_equivalences.push(TypeParamEquivalence {
        source: a_leaf,
        target: b_leaf,
        origins: Some((decl(57, 20), decl(5, 20))),
    });

    // Reduced-body leaves that carry the SAME declaration origins but are freshly
    // re-interned with a THIRD surface name (the name-keyed re-mint). Because the
    // origin is preserved and the surface name changed, these leaves intern to
    // ids DISTINCT from the registered ids — so ONLY the origin consult can
    // bridge them.
    let a_remint = param_leaf(&interner, "renamedA", decl(57, 20));
    let b_remint = param_leaf(&interner, "renamedB", decl(5, 20));
    assert_ne!(
        a_remint, a_leaf,
        "the re-minted A leaf must have a distinct id (name changed, origin kept)"
    );
    assert_ne!(
        b_remint, b_leaf,
        "the re-minted B leaf must have a distinct id (name changed, origin kept)"
    );

    let related = checker.check_subtype(a_remint, b_remint).is_true();
    if origin_consult_active() {
        assert!(
            related,
            "POSITIVE CONTROL: genuine same-decl (A,B) reduced-body leaves must \
             relate through the origin consult when the flag is on"
        );
    } else {
        // Flag off: the origin branch is inert. The re-minted leaves have ids
        // different from the registered ids, so the equivalence consult does not
        // fire; ordinary structural rules apply. This asserts the flag-OFF
        // inertness — the origin path contributed nothing.
        assert!(
            !related,
            "flag-OFF: the origin consult must be inert (re-minted leaves have \
             ids not in the registered pair, so they do not relate via origins)"
        );
    }
}

/// Attribution guard: with the flag ON but NO equivalence registered, the same
/// re-minted `(A,B)` leaves must NOT relate. This proves the positive control's
/// relation is attributable to the ORIGIN CONSULT (a registered origin pair) and
/// not to some unrelated structural fallback that would relate two bare params.
#[test]
fn attribution_guard_no_registration_means_no_relation_even_flag_on() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Same leaves as the positive control, but nothing registered.
    let a_remint = param_leaf(&interner, "renamedA", decl(57, 20));
    let b_remint = param_leaf(&interner, "renamedB", decl(5, 20));

    let related = checker.check_subtype(a_remint, b_remint).is_true();
    assert!(
        !related,
        "ATTRIBUTION: without a registered equivalence, two decl-distinct bare \
         params must not relate — so the positive control's relation is due to \
         the origin consult, not a structural fallback"
    );
}

/// Order-insensitivity of the genuine pair: (B,A) queried against a registered
/// (A,B) must also relate under the flag (alpha-rename is symmetric).
#[test]
fn positive_control_genuine_pair_relates_both_orders() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_leaf = param_leaf(&interner, "A", decl(57, 20));
    let b_leaf = param_leaf(&interner, "B", decl(5, 20));
    checker.type_param_equivalences.push(TypeParamEquivalence {
        source: a_leaf,
        target: b_leaf,
        origins: Some((decl(57, 20), decl(5, 20))),
    });

    let a_remint = param_leaf(&interner, "renamedA", decl(57, 20));
    let b_remint = param_leaf(&interner, "renamedB", decl(5, 20));

    let related_reversed = checker.check_subtype(b_remint, a_remint).is_true();
    if origin_consult_active() {
        assert!(
            related_reversed,
            "POSITIVE CONTROL: (B,A) must relate as well — the origin consult is \
             order-insensitive"
        );
    } else {
        assert!(
            !related_reversed,
            "flag-OFF: reversed re-minted pair must not relate via origins"
        );
    }
}

// ---------------------------------------------------------------------------
// Direct discriminator probe (mirrors the checker's own unit tests but at the
// consult granularity): a registered origin pair matched against a genuine leaf
// pair fires; against a false pair it does not. This proves the discriminator
// itself is exact regardless of the process flag state.
// ---------------------------------------------------------------------------

#[test]
fn discriminator_matches_genuine_and_rejects_false() {
    let eq = TypeParamEquivalence {
        source: TypeId(100),
        target: TypeId(200),
        origins: Some((decl(57, 20), decl(5, 20))),
    };

    // Genuine same-origin pair (either order) matches.
    assert!(eq.matches_origins(decl(57, 20), decl(5, 20)));
    assert!(eq.matches_origins(decl(5, 20), decl(57, 20)));

    // False pairs: any differing (file, node) on either side rejects.
    assert!(!eq.matches_origins(decl(57, 21), decl(5, 20)));
    assert!(!eq.matches_origins(decl(57, 20), decl(5, 21)));
    assert!(!eq.matches_origins(decl(58, 20), decl(5, 20)));
    assert!(!eq.matches_origins(decl(1, 1), decl(2, 2)));

    // User (unstamped) leaves never match — no declaration site.
    assert!(!eq.matches_origins(TypeParamOrigin::User, decl(5, 20)));
    assert!(!eq.matches_origins(decl(57, 20), TypeParamOrigin::User));
    assert!(!eq.matches_origins(TypeParamOrigin::User, TypeParamOrigin::User));

    // An id-only equivalence (origins None) never matches on origins.
    let id_only = TypeParamEquivalence::ids(TypeId(100), TypeId(200));
    assert!(!id_only.matches_origins(decl(57, 20), decl(5, 20)));
}
