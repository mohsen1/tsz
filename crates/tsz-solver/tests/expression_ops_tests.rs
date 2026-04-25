use super::*;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::relations::subtype::{NoopResolver, TypeResolver};
use rustc_hash::FxHashMap;

struct EnumParentResolver {
    parent_map: FxHashMap<DefId, DefId>,
    lazy_map: FxHashMap<DefId, TypeId>,
}

impl EnumParentResolver {
    fn new() -> Self {
        Self {
            parent_map: FxHashMap::default(),
            lazy_map: FxHashMap::default(),
        }
    }
}

impl TypeResolver for EnumParentResolver {
    fn resolve_ref(
        &self,
        _symbol: crate::types::SymbolRef,
        _interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.lazy_map.get(&def_id).copied()
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        self.parent_map.get(&member_def_id).copied()
    }
}

// =========================================================================
// Conditional Expression Tests
// =========================================================================

#[test]
fn test_conditional_both_same() {
    let interner = TypeInterner::new();
    // string ? string : string -> string
    let result = compute_conditional_expression_type(
        &interner,
        TypeId::BOOLEAN,
        TypeId::STRING,
        TypeId::STRING,
    );
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_different_branches() {
    let interner = TypeInterner::new();
    // boolean ? string : number -> string | number
    let result = compute_conditional_expression_type(
        &interner,
        TypeId::BOOLEAN,
        TypeId::STRING,
        TypeId::NUMBER,
    );
    // Result should be a union type (not equal to either branch)
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_error_propagation() {
    let interner = TypeInterner::new();
    // ERROR ? string : number -> ERROR
    let result = compute_conditional_expression_type(
        &interner,
        TypeId::ERROR,
        TypeId::STRING,
        TypeId::NUMBER,
    );
    assert_eq!(result, TypeId::ERROR);

    // boolean ? ERROR : number -> ERROR
    let result = compute_conditional_expression_type(
        &interner,
        TypeId::BOOLEAN,
        TypeId::ERROR,
        TypeId::NUMBER,
    );
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_conditional_any_condition() {
    let interner = TypeInterner::new();
    // any ? string : number -> string | number
    let result =
        compute_conditional_expression_type(&interner, TypeId::ANY, TypeId::STRING, TypeId::NUMBER);
    // Result should be a union type
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_never_condition() {
    let interner = TypeInterner::new();
    // never ? string : number -> never
    let result = compute_conditional_expression_type(
        &interner,
        TypeId::NEVER,
        TypeId::STRING,
        TypeId::NUMBER,
    );
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_truthy_condition() {
    let interner = TypeInterner::new();
    // true ? string : number -> string | number
    // tsc always computes the union of both branches (with subtype reduction),
    // even when the condition is a known literal boolean.
    let true_type = interner.literal_boolean(true);
    let result =
        compute_conditional_expression_type(&interner, true_type, TypeId::STRING, TypeId::NUMBER);
    // Result should be a union type (not equal to either branch alone)
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_falsy_condition() {
    let interner = TypeInterner::new();
    // false ? string : number -> string | number
    // tsc always computes the union of both branches (with subtype reduction),
    // even when the condition is a known literal boolean.
    let false_type = interner.literal_boolean(false);
    let result =
        compute_conditional_expression_type(&interner, false_type, TypeId::STRING, TypeId::NUMBER);
    // Result should be a union type (not equal to either branch alone)
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_fresh_object_literals_get_complementary_optional_properties() {
    let interner = TypeInterner::new();
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let left = interner.object_with_flags(
        vec![PropertyInfo::new(a, TypeId::NUMBER)],
        ObjectFlags::FRESH_LITERAL,
    );
    let right = interner.object_with_flags(
        vec![PropertyInfo::new(b, TypeId::NUMBER)],
        ObjectFlags::FRESH_LITERAL,
    );

    let result = compute_conditional_expression_type(&interner, TypeId::BOOLEAN, left, right);
    let members = crate::type_queries::get_union_members(&interner, result).unwrap();
    assert_eq!(members.len(), 2);

    for member in members.iter().copied() {
        let shape = match interner.lookup(member) {
            Some(TypeData::Object(id)) | Some(TypeData::ObjectWithIndex(id)) => {
                interner.object_shape(id)
            }
            other => panic!("expected object member, got {other:?}"),
        };
        let has_a = shape.properties.iter().any(|p| p.name == a);
        let has_b = shape.properties.iter().any(|p| p.name == b);
        assert!(has_a && has_b, "member should contain both properties");
    }
}

// =========================================================================
// Template Expression Tests
// =========================================================================

#[test]
fn test_template_always_string() {
    let interner = TypeInterner::new();
    // `foo${bar}` -> string
    let result =
        compute_template_expression_type(&interner, &[], &[TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_template_empty() {
    let interner = TypeInterner::new();
    // `` -> string
    let result = compute_template_expression_type(&interner, &[], &[]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_template_error_propagation() {
    let interner = TypeInterner::new();
    // `foo${ERROR}` -> ERROR
    let result = compute_template_expression_type(&interner, &[], &[TypeId::STRING, TypeId::ERROR]);
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_template_never_propagation() {
    let interner = TypeInterner::new();
    // `foo${never}` -> never
    let result = compute_template_expression_type(&interner, &[], &[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

// =========================================================================
// Best Common Type Tests
// =========================================================================

#[test]
fn test_bct_empty() {
    let interner = TypeInterner::new();
    // BCT of empty set -> never
    let result = compute_best_common_type::<NoopResolver>(&interner, &[], None);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_bct_single() {
    let interner = TypeInterner::new();
    // BCT of [string] -> string
    let result = compute_best_common_type::<NoopResolver>(&interner, &[TypeId::STRING], None);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_all_same() {
    let interner = TypeInterner::new();
    // BCT of [string, string, string] -> string
    let result = compute_best_common_type::<NoopResolver>(
        &interner,
        &[TypeId::STRING, TypeId::STRING, TypeId::STRING],
        None,
    );
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_different() {
    let interner = TypeInterner::new();
    // BCT of [string, number] -> string | number
    let result = compute_best_common_type::<NoopResolver>(
        &interner,
        &[TypeId::STRING, TypeId::NUMBER],
        None,
    );
    // Result should be a union type (not equal to either input)
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_error_propagation() {
    let interner = TypeInterner::new();
    // BCT of [string, ERROR, number] -> ERROR
    let result = compute_best_common_type::<NoopResolver>(
        &interner,
        &[TypeId::STRING, TypeId::ERROR, TypeId::NUMBER],
        None,
    );
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_bct_any_absorbs_all() {
    let interner = TypeInterner::new();
    let result = compute_best_common_type::<NoopResolver>(
        &interner,
        &[TypeId::STRING, TypeId::ANY, TypeId::NUMBER],
        None,
    );
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_bct_enum_members_widen_to_parent_enum() {
    let interner = TypeInterner::new();
    let parent_def = DefId(100);
    let member_a_def = DefId(101);
    let member_b_def = DefId(102);

    let parent_enum_type = interner.intern(TypeData::Enum(parent_def, TypeId::NUMBER));
    let member_a = interner.intern(TypeData::Enum(member_a_def, TypeId::NUMBER));
    let member_b = interner.intern(TypeData::Enum(member_b_def, TypeId::NUMBER));

    let mut resolver = EnumParentResolver::new();
    resolver.parent_map.insert(member_a_def, parent_def);
    resolver.parent_map.insert(member_b_def, parent_def);
    resolver.lazy_map.insert(parent_def, parent_enum_type);

    let result = compute_best_common_type(&interner, &[member_a, member_b], Some(&resolver));
    assert_eq!(result, parent_enum_type);
}

#[test]
fn test_bct_preserves_undefined_in_mixed_nullable_candidates() {
    let interner = TypeInterner::new();
    let result = compute_best_common_type::<NoopResolver>(
        &interner,
        &[TypeId::NUMBER, TypeId::UNDEFINED],
        None,
    );
    let members = crate::type_queries::get_union_members(&interner, result)
        .expect("expected number | undefined union");
    assert_eq!(members.len(), 2);
    assert!(members.contains(&TypeId::NUMBER));
    assert!(members.contains(&TypeId::UNDEFINED));
}

#[test]
fn test_bct_preserves_null_in_mixed_nullable_candidates() {
    let interner = TypeInterner::new();
    let result =
        compute_best_common_type::<NoopResolver>(&interner, &[TypeId::STRING, TypeId::NULL], None);
    let members = crate::type_queries::get_union_members(&interner, result)
        .expect("expected string | null union");
    assert_eq!(members.len(), 2);
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NULL));
}

// =========================================================================
// Subtype Reduction in BCT Fallback
// =========================================================================

#[test]
fn test_bct_removes_structural_subtypes_in_fallback_union() {
    // When 3 types exist and no single type is supertype of all, BCT falls back
    // to a union. But before creating the union, it should remove subtypes.
    //
    // Given: base = { x: number }, derived = { x: number, y: string }, unrelated = { z: boolean }
    // derived <: base, but unrelated is not related to either.
    // No single type is supertype of all → falls back to union.
    // Expected: base | unrelated (derived is removed as subtype of base).
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);

    let result =
        compute_best_common_type::<NoopResolver>(&interner, &[base, derived, unrelated], None);

    // Result should be a union of base and unrelated (derived removed as subtype of base)
    let members =
        crate::type_queries::get_union_members(&interner, result).expect("expected a union type");
    assert_eq!(
        members.len(),
        2,
        "expected 2 members after subtype reduction, got {}: {:?}",
        members.len(),
        members
    );
    assert!(members.contains(&base), "expected base in union");
    assert!(members.contains(&unrelated), "expected unrelated in union");
    assert!(
        !members.contains(&derived),
        "derived should be removed (it's a subtype of base)"
    );
}

// =========================================================================
// Subtype-Reduction Cache Wiring Tests
// =========================================================================

#[test]
fn test_bct_cached_matches_uncached_for_subtype_reduction() {
    // The cached path must produce the same result as the uncached path —
    // this guards against the cache silently changing observable behavior.
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);

    let uncached = crate::expression_ops::compute_best_common_type::<NoopResolver>(
        &interner,
        &[base, derived, unrelated],
        None,
    );
    let cached = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated],
        None,
    );
    assert_eq!(uncached, cached);
}

#[test]
fn test_bct_cache_records_miss_then_hit() {
    // Two back-to-back BCT calls with the same input list must produce one
    // cache miss followed by one cache hit. Mirrors the wiring contract
    // verified for the instantiation cache by `cache_hit_after_first_*`.
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);

    let stats0 = db.statistics();

    let r1 = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated],
        None,
    );
    let r2 = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated],
        None,
    );

    assert_eq!(r1, r2, "cached BCT result must equal recomputed result");

    let stats1 = db.statistics();
    assert!(
        stats1.subtype_reduction_cache_misses > stats0.subtype_reduction_cache_misses,
        "first call must record a miss"
    );
    assert!(
        stats1.subtype_reduction_cache_hits > stats0.subtype_reduction_cache_hits,
        "second call must record a hit (got hits={})",
        stats1.subtype_reduction_cache_hits
    );
    assert!(
        stats1.subtype_reduction_cache_entries >= 1,
        "cache must contain >= 1 entry"
    );
}

#[test]
fn test_bct_cache_distinguishes_input_lists() {
    // Different input candidate lists that BOTH reach the
    // `remove_subtypes_for_bct` fallback must produce distinct cache
    // slots (a hash collision here would corrupt downstream BCT results).
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");
    let name_w = interner.intern_string("w");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated_a = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);
    let unrelated_b = interner.object(vec![PropertyInfo::new(name_w, TypeId::STRING)]);

    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated_a],
        None,
    );
    let entries_after_first = db.statistics().subtype_reduction_cache_entries;
    assert!(
        entries_after_first >= 1,
        "first call must populate the cache (got {entries_after_first} entries)"
    );

    // A list that ALSO falls through to remove_subtypes_for_bct (no unit
    // types, no winning supertype, no constructor-only short-circuit).
    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated_b],
        None,
    );
    let entries_after_second = db.statistics().subtype_reduction_cache_entries;

    assert!(
        entries_after_second > entries_after_first,
        "distinct input lists must occupy distinct cache slots ({entries_after_first} -> {entries_after_second})"
    );
}

#[test]
fn test_bct_cache_input_order_independence() {
    // The cache key sorts the input slice, so two BCT calls whose input
    // lists are permutations of each other share a cache slot. (BCT itself
    // is set-valued, so the same answer is correct.)
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);

    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[base, derived, unrelated],
        None,
    );
    let stats_after_first = db.statistics();

    // Reorder the inputs — the sorted-key cache must hit.
    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[unrelated, base, derived],
        None,
    );
    let stats_after_second = db.statistics();

    assert!(
        stats_after_second.subtype_reduction_cache_hits
            > stats_after_first.subtype_reduction_cache_hits,
        "permuted-input call must hit the same cache slot"
    );
}

#[test]
fn test_bct_cache_no_query_db_disables_cache() {
    // Calling with `query_db = None` must compute the correct result
    // without populating any cache entry. This preserves the existing
    // backwards-compatible call path used by ad-hoc tests.
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let unrelated = interner.object(vec![PropertyInfo::new(name_z, TypeId::BOOLEAN)]);

    let stats0 = db.statistics();

    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        None,
        &[base, derived, unrelated],
        None,
    );

    let stats1 = db.statistics();
    assert_eq!(
        stats1.subtype_reduction_cache_entries, stats0.subtype_reduction_cache_entries,
        "calls with query_db=None must NOT populate the cache"
    );
}

#[test]
fn test_bct_cache_resolver_present_distinct_from_absent() {
    // Same input TypeIds, but `resolver = Some(_)` vs `None` must occupy
    // distinct cache slots — a no-resolver answer cached and served back
    // when class-hierarchy resolution is enabled would be wrong.
    use crate::caches::query_cache::QueryCache;
    use crate::types::PropertyInfo;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let a = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(name_y, TypeId::STRING)]);

    // No-resolver path.
    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[a, b],
        None,
    );
    let entries_no_res = db.statistics().subtype_reduction_cache_entries;

    // Same TypeIds but with a (no-op) resolver — must take a different slot.
    let resolver = NoopResolver;
    let _ = crate::expression_ops::compute_best_common_type_cached::<NoopResolver>(
        &interner,
        Some(&db),
        &[a, b],
        Some(&resolver),
    );
    let entries_with_res = db.statistics().subtype_reduction_cache_entries;

    assert!(
        entries_with_res > entries_no_res,
        "resolver-present must be a distinct cache slot ({entries_no_res} -> {entries_with_res})"
    );
}

// =========================================================================
// Template Literal Expression Tests
// =========================================================================

#[test]
fn test_template_expression_default_is_string() {
    let interner = TypeInterner::new();
    // Template expressions without context produce string type
    let result =
        compute_template_expression_type(&interner, &[], &[TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_template_expression_error_propagation() {
    let interner = TypeInterner::new();
    // ERROR in any part propagates
    let result = compute_template_expression_type(&interner, &[], &[TypeId::ERROR, TypeId::STRING]);
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_template_expression_never_propagation() {
    let interner = TypeInterner::new();
    // NEVER in any part propagates
    let result = compute_template_expression_type(&interner, &[], &[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_template_expression_contextual_produces_template_type() {
    // When called with contextual info, should produce a template literal type
    let interner = TypeInterner::new();
    let result = compute_template_expression_type_contextual(
        &interner,
        &["hello ".to_string(), "!".to_string()],
        &[TypeId::STRING],
    );
    // Should NOT be plain string — should be a template literal type `hello ${string}!`
    assert_ne!(result, TypeId::STRING);
    // Check it's a TemplateLiteral type
    assert!(
        matches!(interner.lookup(result), Some(TypeData::TemplateLiteral(_))),
        "Expected TemplateLiteral type, got: {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_template_expression_contextual_all_literals_produces_string_literal() {
    // When all parts are concrete literals, template_literal() returns a string literal
    let interner = TypeInterner::new();
    let lit_42 = interner.literal_number(42.0);
    let result = compute_template_expression_type_contextual(
        &interner,
        &["value: ".to_string(), String::new()],
        &[lit_42],
    );
    // The solver's template_literal() expands concrete literals to a string literal
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Literal(_))),
        "Expected string literal type, got: {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_template_expression_contextual_error_still_propagates() {
    let interner = TypeInterner::new();
    let result = compute_template_expression_type_contextual(
        &interner,
        &["a".to_string(), "b".to_string()],
        &[TypeId::ERROR],
    );
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_is_template_literal_contextual_type_basic() {
    let interner = TypeInterner::new();

    // Template literal type → true
    use crate::types::TemplateSpan;
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    assert!(is_template_literal_contextual_type(&interner, tl));

    // String literal → true
    let sl = interner.literal_string("hello");
    assert!(is_template_literal_contextual_type(&interner, sl));

    // Plain string → false
    assert!(!is_template_literal_contextual_type(
        &interner,
        TypeId::STRING
    ));

    // Number → false
    assert!(!is_template_literal_contextual_type(
        &interner,
        TypeId::NUMBER
    ));
}

#[test]
fn test_is_template_literal_contextual_type_union() {
    let interner = TypeInterner::new();

    // Union containing a template literal → true
    use crate::types::TemplateSpan;
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("x-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);
    let union = interner.union(vec![tl, TypeId::NUMBER]);
    assert!(is_template_literal_contextual_type(&interner, union));

    // Union of plain types → false
    let plain_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(!is_template_literal_contextual_type(&interner, plain_union));
}
