//! Call-argument missing-property promotion when the parameter type carries a
//! *bound* signature type parameter (issue #17145).
//!
//! Structural rule: when an incompatible value is passed as a call argument to a
//! parameter whose type is a fully-formed structural object/interface, tsc
//! promotes a sole/leading missing-required-property failure to the specific
//! head diagnostic — TS2741 (one property), TS2739 (several), TS2740 (many) —
//! exactly as it does for the equivalent direct assignment. This holds even when
//! the target additionally declares an index signature and a *generic* method
//! (`m<S>(x: S): S`): the method's own type parameter is bound by its signature,
//! so the target is concrete and the promotion still applies.
//!
//! tsz previously routed the call-argument diagnostic to the
//! type-parameter-display-preserving emitter (a bare TS2345 head with no
//! property list) whenever the parameter type *contained* a type parameter
//! anywhere — including a generic method's own bound parameter. The broad
//! `contains_type_parameters` walk descends into signature bodies, and an index
//! signature on the interface happens to leave that bound `S` as a raw
//! `TypeParameter` node rather than a canonical `BoundParameter`, so the walk
//! reported the concrete target as "contains a type parameter" and the missing
//! -property promotion was silently dropped. The routing now asks
//! `contains_free_type_parameters` (free/unbound parameters only, skipping
//! generic signature bodies), which is the property the decision actually means.
//!
//! Tests vary the binder names (interface / method / property / type-parameter
//! spellings) so a fix keyed to a particular spelling would not satisfy them,
//! and assert on the promoted diagnostic *code* plus that the missing member is
//! named, rather than on exact type rendering (owned by the printer). The
//! construct-signature and property-provided cases guard against over-promotion.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when a primary diagnostic of `code` names `prop` as missing.
fn has_missing_property_head(diags: &[Diagnostic], code: u32, prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    diags
        .iter()
        .any(|d| d.code == code && d.message_text.contains(&needle))
}

/// True when a primary diagnostic of `code` is a "missing the following
/// properties from type ...: a, b" summary naming every property.
fn has_missing_properties_head(diags: &[Diagnostic], code: u32, props: &[&str]) -> bool {
    diags.iter().any(|d| {
        d.code == code
            && d.message_text
                .contains("missing the following properties from type")
            && props.iter().all(|p| d.message_text.contains(p))
    })
}

/// Canonical repro: the parameter type declares both a number index signature
/// and a generic method. A `{}` argument is missing the required method, and the
/// call-argument diagnostic must promote to TS2741 naming that method — matching
/// the direct-assignment path (asserted alongside).
#[test]
fn index_signature_and_generic_method_call_argument_promotes_ts2741() {
    let diags = diagnostics(
        r#"
interface Big {
    m<S>(x: S): S;
    readonly [n: number]: string;
}
function f(x: Big): void {}
f({});
const b: Big = {};
"#,
    );
    assert!(
        has_missing_property_head(&diags, 2741, "m"),
        "call argument `{{}}` to a param with an index signature + generic method \
         should promote to TS2741 naming the missing method `m`; got {diags:?}"
    );
    // Direct assignment already promoted; the call path must now agree. Exactly
    // two TS2741 (the call and the assignment), no bare TS2345 fallback.
    assert_eq!(
        diags.iter().filter(|d| d.code == 2741).count(),
        2,
        "both the call argument and the direct assignment should report TS2741; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2345),
        "the call argument must not fall back to a bare TS2345 head; got {diags:?}"
    );
}

/// Same structural rule, different interface / method / type-parameter
/// spellings, and a generic interface applied to a concrete argument. A
/// spelling-keyed fix would miss this.
#[test]
fn index_signature_and_generic_method_promotion_is_name_agnostic() {
    let diags = diagnostics(
        r#"
interface Zzz<Q> {
    readonly [k: number]: string;
    method<PARAM>(v: PARAM): PARAM;
}
function accept(w: Zzz<number>): void {}
accept({});
"#,
    );
    assert!(
        has_missing_property_head(&diags, 2741, "method"),
        "renamed binders should still promote the call-argument missing-property \
         head (TS2741 naming `method`); got {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2345),
        "renamed case must not fall back to a bare TS2345; got {diags:?}"
    );
}

/// Several missing required properties use the TS2739 summary head, still at the
/// call-argument site, with the index signature + generic method present.
#[test]
fn index_signature_and_generic_method_multiple_missing_promotes_ts2739() {
    let diags = diagnostics(
        r#"
interface Multi {
    m<S>(x: S): S;
    other: number;
    readonly [n: number]: string;
}
function f(x: Multi): void {}
f({});
"#,
    );
    assert!(
        has_missing_properties_head(&diags, 2739, &["m", "other"]),
        "multiple missing required members should promote to a TS2739 summary \
         naming `m` and `other`; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2345),
        "multi-missing case must not fall back to a bare TS2345; got {diags:?}"
    );
}

/// A generic method alone (no index signature) already promoted; guard that the
/// routing change did not regress it.
#[test]
fn generic_method_without_index_signature_still_promotes() {
    let diags = diagnostics(
        r#"
interface OnlyMethod {
    m<S>(x: S): S;
    other: number;
}
function f(x: OnlyMethod): void {}
f({});
"#,
    );
    assert!(
        has_missing_properties_head(&diags, 2739, &["m", "other"]),
        "generic-method-only target should still promote to TS2739; got {diags:?}"
    );
}

/// Over-promotion guard: a generic *construct* signature plus an index signature
/// declares no missing *property* (the construct signature is not a member
/// name), so tsc keeps the bare TS2345 head. The routing change must not
/// synthesize a spurious missing-property head here.
#[test]
fn generic_construct_signature_keeps_bare_head() {
    let diags = diagnostics(
        r#"
interface Ctor {
    new <S>(x: S): S;
    readonly [n: number]: string;
}
function f(x: Ctor): void {}
f({});
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2345),
        "a construct-signature target with no missing property keeps the bare \
         TS2345 head; got {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == 2741 || d.code == 2739 || d.code == 2740),
        "must not synthesize a missing-property head when no named property is \
         missing; got {diags:?}"
    );
}

/// Providing the required method makes the argument assignable: no diagnostic at
/// the call. Guards the fix against a false positive.
#[test]
fn providing_the_required_method_is_accepted() {
    let diags = diagnostics(
        r#"
interface Big {
    m<S>(x: S): S;
    readonly [n: number]: string;
}
function f(x: Big): void {}
f({ m: (y) => y });
"#,
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == 2345 || d.code == 2741 || d.code == 2739 || d.code == 2740),
        "supplying the required generic method should type-check with no call \
         diagnostic; got {diags:?}"
    );
}
