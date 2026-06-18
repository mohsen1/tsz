//! Regression coverage for the explain-pass `collect_properties` memo
//! activation (issues #13242 / #13243, workstream B).
//!
//! Structural rule: when an assignment from an **intersection source** to an
//! object/callable target fails, the explain pass collects the source
//! intersection's merged property closure to report the missing property. That
//! collection must go through the context-free `collect_properties` memo
//! (#13865, exposed as `collect_properties_cached`) — the same memo the boolean
//! relation already threads `query_db` into at every other intersection
//! collection site (`overlap`/`helpers`/`core`). Before this fix the explain
//! pass was the remaining bare caller, re-walking the full recursive-schema
//! closure from scratch on every failing diagnostic.
//!
//! The activation is a pure cache surface: it must return a byte-identical
//! failure reason whether or not a `QueryCache` is supplied. These tests pin
//! that invariant by explaining the same failing relation twice — once with a
//! `QueryCache` (`query_db = Some`, the cached path) and once with the bare
//! interner (`query_db = None`, the legacy path) — and asserting the reason is
//! structurally identical, naming the genuinely-missing property.

use crate::caches::query_cache::QueryCache;
use crate::diagnostics::SubtypeFailureReason;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TypeId};

/// `{ a: number } & { b: string }` assigned to `{ a: number; b: string; c: boolean }`
/// fails because the source closure lacks `c`. The explain pass walks the
/// intersection-source arm (`collect_properties_cached(resolved_source, …)`).
#[test]
fn explain_intersection_source_missing_property_is_identical_cached_and_uncached() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(b, TypeId::STRING)]);
    let source = interner.intersection2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo::new(a, TypeId::NUMBER),
        PropertyInfo::new(b, TypeId::STRING),
        PropertyInfo::new(c, TypeId::BOOLEAN),
    ]);

    // Sanity: the relation genuinely fails (source has no `c`).
    let mut probe = SubtypeChecker::new(&interner);
    assert!(
        !probe.is_assignable_to(source, target),
        "intersection source missing `c` must not be assignable to the wider object",
    );

    // Bare interner: `query_db = None` (the legacy bare-collect path).
    let uncached = {
        let mut checker = SubtypeChecker::new(&interner);
        checker.explain_failure(source, target)
    };

    // With a `QueryCache`: `query_db = Some` (the memo-activated path the fix
    // wires up). Explain twice on the same instance so the second call would
    // hit the populated `collect_properties` memo — the result must not change.
    let db = QueryCache::new(&interner);
    let (cached_first, cached_second) = {
        let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
        let first = checker.explain_failure(source, target);
        let second = checker.explain_failure(source, target);
        (first, second)
    };

    for reason in [&uncached, &cached_first, &cached_second] {
        let names_missing_c = matches!(
            reason,
            Some(SubtypeFailureReason::MissingProperty { property_name, .. }) if *property_name == c,
        ) || matches!(
            reason,
            Some(SubtypeFailureReason::MissingProperties { property_names, .. })
                if property_names.contains(&c),
        );
        assert!(
            names_missing_c,
            "explain pass must report `c` as the missing property, got {reason:?}",
        );
    }

    assert_eq!(
        uncached, cached_first,
        "memo activation must be byte-identical to the bare-collect path",
    );
    assert_eq!(
        cached_first, cached_second,
        "a populated collect-properties memo must serve the same reason",
    );
}

/// Same shape with two missing properties (`c` and `d`) to exercise the
/// `MissingProperties` (TS2739) branch of the same intersection-source arm and
/// confirm the cached/uncached parity holds for the multi-property reason too.
#[test]
fn explain_intersection_source_multiple_missing_properties_parity() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let c = interner.intern_string("c");
    let d = interner.intern_string("d");

    let obj_a = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
    // A second member so the source is a genuine intersection (drives the
    // `intersection_list_id(source).is_some()` arm rather than a plain object).
    let obj_marker = interner.object(vec![PropertyInfo::new(
        interner.intern_string("marker"),
        TypeId::BOOLEAN,
    )]);
    let source = interner.intersection2(obj_a, obj_marker);

    let target = interner.object(vec![
        PropertyInfo::new(a, TypeId::NUMBER),
        PropertyInfo::new(c, TypeId::STRING),
        PropertyInfo::new(d, TypeId::BOOLEAN),
    ]);

    let uncached = {
        let mut checker = SubtypeChecker::new(&interner);
        checker.explain_failure(source, target)
    };
    let db = QueryCache::new(&interner);
    let cached = {
        let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
        checker.explain_failure(source, target)
    };

    assert!(
        uncached.is_some(),
        "a failing relation must produce a reason"
    );
    assert_eq!(
        uncached, cached,
        "intersection-source multi-missing-property explain must be identical \
         with and without the collect-properties memo",
    );
}
