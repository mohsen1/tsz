use super::*;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::subtype::{NoopResolver, TypeResolver};
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
    // true ? string : number -> string
    let true_type = interner.literal_boolean(true);
    let result =
        compute_conditional_expression_type(&interner, true_type, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_falsy_condition() {
    let interner = TypeInterner::new();
    // false ? string : number -> number
    let false_type = interner.literal_boolean(false);
    let result =
        compute_conditional_expression_type(&interner, false_type, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(result, TypeId::NUMBER);
}

// =========================================================================
// Template Expression Tests
// =========================================================================

#[test]
fn test_template_always_string() {
    let interner = TypeInterner::new();
    // `foo${bar}` -> string
    let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_template_empty() {
    let interner = TypeInterner::new();
    // `` -> string
    let result = compute_template_expression_type(&interner, &[]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_template_error_propagation() {
    let interner = TypeInterner::new();
    // `foo${ERROR}` -> ERROR
    let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::ERROR]);
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_template_never_propagation() {
    let interner = TypeInterner::new();
    // `foo${never}` -> never
    let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::NEVER]);
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
