//! Parity guards for the session-level computed-variance cache.
//!
//! `compute_type_param_variances_with_resolver_cached` memoizes the
//! declared-variance mask of a generic `DefId` in the session `variance_cache`
//! so repeated references to the same generic do not rebuild a fresh
//! `VarianceComputer` and re-walk the lazy type graph. The cache key is the
//! `DefId` alone and the stored mask is the same value the uncached walk
//! returns, so consulting/populating it must never change a diagnostic.
//!
//! These tests are deliberately **name-agnostic**: the same structural program
//! is checked with several different type-parameter spellings. If the cache
//! were keyed on (or otherwise sensitive to) the user-chosen parameter name,
//! the renamed variants would diverge. They also exercise the **warm** path —
//! many references to the same generic within one program — which is exactly
//! the path the session cache short-circuits.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut v: Vec<u32> = diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|code| *code != 2318) // ignore "Cannot find global type" noise
        .collect();
    v.sort_unstable();
    v
}

/// Build a covariant-container program where the generic is referenced many
/// times (warm cache), parameterized by the type-parameter spelling so the
/// same structural program can be checked under several names.
fn covariant_container_program(param: &str) -> String {
    format!(
        r#"
interface Box<{param}> {{ value: {param}; read(): {param}; }}
interface Foo {{ x: number; }}
interface Bar {{ x: number; y: number; }}

declare var a1: Box<Foo>;
declare var b1: Box<Bar>;
declare var a2: Box<Foo>;
declare var b2: Box<Bar>;
declare var a3: Box<Foo>;
declare var b3: Box<Bar>;

a1 = b1; // ok: Bar <: Foo (covariant value)
b1 = a1; // ERROR: Foo missing y
a2 = b2; // ok
b2 = a2; // ERROR
a3 = b3; // ok
b3 = a3; // ERROR
"#
    )
}

#[test]
fn covariant_container_warm_cache_is_name_agnostic() {
    // The cache key is the DefId, not the parameter spelling. Every renamed
    // variant must produce identical diagnostics, and the warm references must
    // not change the answer relative to a single reference.
    let baseline = codes(&covariant_container_program("T"));
    assert!(
        baseline.iter().filter(|c| **c == 2322).count() == 3,
        "expected exactly three TS2322 (the three reverse assignments), got {baseline:?}"
    );
    for param in ["K", "P", "Element", "TValue", "_T0"] {
        let renamed = codes(&covariant_container_program(param));
        assert_eq!(
            baseline, renamed,
            "variance diagnostics must be identical regardless of the type-parameter spelling \
             (`T` vs `{param}`); a divergence means the session variance cache is name-sensitive"
        );
    }
}

/// Bivariant method-parameter program: `T` used solely as a direct method
/// parameter must stay bivariant so both assignment directions succeed. Warm
/// (repeated) references must keep this stable.
fn bivariant_method_program(param: &str) -> String {
    format!(
        r#"
interface Sink<{param}> {{ m(x: {param}): void; }}
interface Foo {{ x: number; }}
interface Bar {{ x: number; y: number; }}

declare var a1: Sink<Foo>;
declare var b1: Sink<Bar>;
declare var a2: Sink<Foo>;
declare var b2: Sink<Bar>;

a1 = b1;
b1 = a1;
a2 = b2;
b2 = a2;
"#
    )
}

#[test]
fn bivariant_method_warm_cache_is_name_agnostic() {
    let baseline = codes(&bivariant_method_program("T"));
    assert!(
        !baseline.contains(&2322),
        "pure method-param generic must remain bivariant (no TS2322), got {baseline:?}"
    );
    for param in ["U", "X", "Item", "TArg"] {
        let renamed = codes(&bivariant_method_program(param));
        assert_eq!(
            baseline, renamed,
            "bivariant-method diagnostics must be identical for spelling `{param}`"
        );
    }
}

/// Alias-wrapped generic: a type alias whose body references another generic.
/// Variance must follow the alias to the wrapped generic. Exercises the
/// resolver-aware path (local alias body) that the query database's own
/// resolver may not see, proving the cached helper still computes via the
/// supplied resolver.
fn alias_wrapped_program(outer: &str, inner: &str) -> String {
    format!(
        r#"
interface Cell<{inner}> {{ get(): {inner}; }}
type Holder<{outer}> = {{ cell: Cell<{outer}>; }};
interface Foo {{ x: number; }}
interface Bar {{ x: number; y: number; }}

declare var a1: Holder<Foo>;
declare var b1: Holder<Bar>;
declare var a2: Holder<Foo>;
declare var b2: Holder<Bar>;

a1 = b1; // ok: covariant through Cell
b1 = a1; // ERROR
a2 = b2; // ok
b2 = a2; // ERROR
"#
    )
}

#[test]
fn alias_wrapped_generic_warm_cache_is_name_agnostic() {
    let baseline = codes(&alias_wrapped_program("T", "U"));
    assert!(
        baseline.iter().filter(|c| **c == 2322).count() == 2,
        "expected two TS2322 (the two reverse assignments) for the alias-wrapped covariant \
         container, got {baseline:?}"
    );
    for (outer, inner) in [("A", "B"), ("Outer", "Inner"), ("P", "Q")] {
        let renamed = codes(&alias_wrapped_program(outer, inner));
        assert_eq!(
            baseline, renamed,
            "alias-wrapped variance diagnostics must be identical for spellings `{outer}`/`{inner}`"
        );
    }
}

/// Negative/fallback guard: a contravariant position (the generic appears only
/// in a function-parameter position) must reverse the assignability direction.
/// This proves the cache does not silently widen everything to covariant.
fn contravariant_program(param: &str) -> String {
    format!(
        r#"
interface Consumer<{param}> {{ accept(x: {param}): void; use: (x: {param}) => void; }}
interface Foo {{ x: number; }}
interface Bar {{ x: number; y: number; }}

declare var a: Consumer<Foo>;
declare var b: Consumer<Bar>;
a = b; // ERROR under contravariance: Bar-consumer not assignable to Foo-consumer
b = a; // ok
"#
    )
}

#[test]
fn contravariant_consumer_is_name_agnostic() {
    let baseline = codes(&contravariant_program("T"));
    for param in ["S", "In", "TInput"] {
        let renamed = codes(&contravariant_program(param));
        assert_eq!(
            baseline, renamed,
            "contravariant-consumer diagnostics must be identical for spelling `{param}`"
        );
    }
}
