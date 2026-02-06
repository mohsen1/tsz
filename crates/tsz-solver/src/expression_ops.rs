//! Expression type computation operations.
//!
//! This module implements AST-agnostic type computation for expressions,
//! migrated from the Checker as part of the Solver-First architecture refactor.
//!
//! These functions operate purely on TypeIds and maintain no AST dependencies.

use crate::TypeDatabase;
use crate::TypeResolver;
use crate::is_subtype_of;
use crate::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, TypeId, TypeKey};

/// Computes the result type of a conditional expression: `condition ? true_branch : false_branch`.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `condition` - Type of the condition expression
/// * `true_type` - Type of the true branch (when_true)
/// * `false_type` - Type of the false branch (when_false)
///
/// # Returns
/// * If condition is definitely truthy: returns `true_type`
/// * If condition is definitely falsy: returns `false_type`
/// * Otherwise: returns union of `true_type` and `false_type`
pub fn compute_conditional_expression_type(
    interner: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    // Handle error propagation
    if condition == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if true_type == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if false_type == TypeId::ERROR {
        return TypeId::ERROR;
    }

    // Handle special type constants
    if condition == TypeId::ANY {
        // any ? A : B -> A | B
        return interner.union2(true_type, false_type);
    }
    if condition == TypeId::NEVER {
        // never ? A : B -> never (unreachable)
        return TypeId::NEVER;
    }

    // Check if condition is definitely truthy or falsy
    if is_definitely_truthy(interner, condition) {
        return true_type;
    }
    if is_definitely_falsy(interner, condition) {
        return false_type;
    }

    // If both branches are the same type, no need for union
    if true_type == false_type {
        return true_type;
    }

    // Default: return union of both branches
    interner.union2(true_type, false_type)
}

/// Computes the type of a template literal expression.
///
/// Template literals always produce string type in TypeScript.
///
/// # Arguments
/// * `_interner` - The type database/interner (unused in Phase 1)
/// * `parts` - Slice of type IDs for each template part
///
/// # Returns
/// * `TypeId::STRING` - Template literals always produce strings
pub fn compute_template_expression_type(_interner: &dyn TypeDatabase, parts: &[TypeId]) -> TypeId {
    // Check for error propagation
    for &part in parts {
        if part == TypeId::ERROR {
            return TypeId::ERROR;
        }
        if part == TypeId::NEVER {
            return TypeId::NEVER;
        }
    }

    // For Phase 1, template literals always produce string type
    // The Checker handles type-checking each part's expression
    TypeId::STRING
}

/// Computes the best common type (BCT) of a set of types.
///
/// This is used for array literal type inference and other contexts
/// where a single type must be inferred from multiple candidates.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `types` - Slice of type IDs to find the best common type of
/// * `resolver` - Optional TypeResolver for nominal hierarchy lookups (class inheritance)
///
/// # Returns
/// * Empty slice: Returns `TypeId::NEVER`
/// * Single type: Returns that type
/// * All same type: Returns that type
/// * Otherwise: Returns union of all types (or common base class if available)
///
/// # Note
/// When `resolver` is provided, this implements the full TypeScript BCT algorithm:
/// - Find the first candidate that is a supertype of all others
/// - Handle literal widening (via TypeChecker's pre-widening)
/// - Handle base class relationships (Dog + Cat -> Animal)
pub fn compute_best_common_type<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    // Handle empty cases
    if types.is_empty() {
        return TypeId::NEVER;
    }

    // Propagate errors
    for &ty in types {
        if ty == TypeId::ERROR {
            return TypeId::ERROR;
        }
    }

    // Single type: return it directly
    if types.len() == 1 {
        return types[0];
    }

    // If all types are the same, no need for union
    let first = types[0];
    if types.iter().all(|&ty| ty == first) {
        return first;
    }

    // Step 1: Apply literal widening for array literals
    // When we have multiple literal types of the same primitive kind, widen to the primitive
    // Example: [1, 2] -> number[], ["a", "b"] -> string[]
    let widened = widen_literals(interner, types);

    // Step 1.5: Enum member widening
    // If all candidates are enum members from the same parent enum,
    // infer the parent enum type directly instead of a large union of members.
    // This matches TypeScript's behavior for expressions like [E.A, E.B] -> E[].
    if let Some(res) = resolver
        && let Some(common_enum_type) = common_parent_enum_type(interner, &widened, res)
    {
        return common_enum_type;
    }

    // OPTIMIZATION: Unit-type fast-path
    // If ALL types are unit types (tuples of literals/enums, or literals themselves),
    // no single type can be a supertype of the others (unit types are disjoint).
    // Skip the O(N²) subtype loop and go directly to union creation.
    // This turns O(N²) into O(N) for cases like enumLiteralsSubtypeReduction.ts
    // which has 500 distinct enum-tuple return types.
    if widened.len() > 2 {
        let all_unit = widened.iter().all(|&ty| interner.is_unit_type(ty));
        if all_unit {
            // All unit types -> no common supertype exists, create union
            return interner.union(widened.to_vec());
        }
    }

    // Step 2: Find the best common type from the candidate types
    // TypeScript rule: The best common type must be one of the input types
    // For example: [Dog, Cat] -> Dog | Cat (NOT Animal, even if both extend Animal)
    //              [Dog, Animal] -> Animal (Animal is in the set and is a supertype)
    //
    // OPTIMIZATION: Create ONE SubtypeChecker and reuse it for all comparisons.
    // Previously, check_subtype() created a new SubtypeChecker (with 3 FxHashSets) for
    // every single comparison. With N candidates and N types, that's O(N²) allocations.
    // For enumLiteralsSubtypeReduction.ts (512 return types), this was 262,144 allocations!
    //
    // We handle the two cases (with/without resolver) separately because SubtypeChecker<R>
    // and SubtypeChecker<NoopResolver> are different types.
    if let Some(res) = resolver {
        let mut checker = SubtypeChecker::with_resolver(interner, res);
        for &candidate in &widened {
            let is_supertype = widened.iter().all(|&ty| {
                // CRITICAL: Reset the recursion guard counters for each top-level check.
                // Otherwise, total_checks accumulates across the loop and eventually
                // causes spurious DepthExceeded failures (treated as false).
                checker.total_checks = 0;
                checker.depth = 0;
                checker.is_subtype_of(ty, candidate)
            });
            if is_supertype {
                return candidate;
            }
        }
    } else {
        let mut checker = SubtypeChecker::new(interner);
        for &candidate in &widened {
            let is_supertype = widened.iter().all(|&ty| {
                checker.total_checks = 0;
                checker.depth = 0;
                checker.is_subtype_of(ty, candidate)
            });
            if is_supertype {
                return candidate;
            }
        }
    }

    // Step 3: Try to find a common base type for primitives/literals
    // For example, [string, "hello"] -> string
    if let Some(base) = find_common_base_type(interner, &widened) {
        // All types share a common base type
        if all_types_are_narrower_than_base(interner, &widened, base) {
            return base;
        }
    }

    // Step 4: Default to union of all types
    interner.union(widened.to_vec())
}

/// Widen literal types to their primitive base types when appropriate.
///
/// This implements Rule #10 (Literal Widening) for BCT:
/// - Fresh literals in arrays are widened to their primitive types
/// - Example: [1, 2] -> [number, number]
/// - Example: ["a", "b"] -> [string, string]
/// - Example: [1, "a"] -> [number, string] (mixed types)
///
/// The widening happens for each literal individually, even in mixed arrays.
/// Non-literal types are preserved as-is.
fn widen_literals(interner: &dyn TypeDatabase, types: &[TypeId]) -> Vec<TypeId> {
    // Widen each literal individually, regardless of what else is in the list.
    // This matches TypeScript's behavior where [1, "a"] infers as (number | string)[]
    types
        .iter()
        .map(|&ty| {
            if let Some(key) = interner.lookup(ty) {
                if let crate::types::TypeKey::Literal(ref lit) = key {
                    return match lit {
                        crate::types::LiteralValue::String(_) => TypeId::STRING,
                        crate::types::LiteralValue::Number(_) => TypeId::NUMBER,
                        crate::types::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
                        crate::types::LiteralValue::BigInt(_) => TypeId::BIGINT,
                    };
                }
            }
            ty // Non-literal types are preserved
        })
        .collect()
}

/// Find a common base type for a set of types.
/// For example, [string, "hello"] -> Some(string)
fn find_common_base_type(interner: &dyn TypeDatabase, types: &[TypeId]) -> Option<TypeId> {
    if types.is_empty() {
        return None;
    }

    // Get the base type of the first type
    let first_base = get_base_type(interner, types[0])?;

    // Check if all other types have the same base type
    for &ty in types.iter().skip(1) {
        let base = get_base_type(interner, ty)?;
        if base != first_base {
            return None;
        }
    }

    Some(first_base)
}

/// Get the base type of a type (for literals, this is the primitive type).
fn get_base_type(interner: &dyn TypeDatabase, ty: TypeId) -> Option<TypeId> {
    match interner.lookup(ty) {
        Some(crate::types::TypeKey::Literal(ref lit)) => {
            let base = match lit {
                crate::types::LiteralValue::String(_) => TypeId::STRING,
                crate::types::LiteralValue::Number(_) => TypeId::NUMBER,
                crate::types::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
                crate::types::LiteralValue::BigInt(_) => TypeId::BIGINT,
            };
            Some(base)
        }
        _ => Some(ty),
    }
}

/// Check if all types are narrower than (subtypes of) the given base type.
fn all_types_are_narrower_than_base(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    base: TypeId,
) -> bool {
    types.iter().all(|&ty| is_subtype_of(interner, ty, base))
}

/// Return the common parent enum type if all candidates are members of the same enum.
fn common_parent_enum_type<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: &R,
) -> Option<TypeId> {
    let mut parent_def = None;

    for &ty in types {
        let TypeKey::Enum(def_id, _) = interner.lookup(ty)? else {
            return None;
        };

        let current_parent = resolver.get_enum_parent_def_id(def_id).unwrap_or(def_id);
        if let Some(existing) = parent_def {
            if existing != current_parent {
                return None;
            }
        } else {
            parent_def = Some(current_parent);
        }
    }

    let parent_def = parent_def?;
    resolver
        .resolve_lazy(parent_def, interner)
        .or_else(|| Some(interner.intern(TypeKey::Lazy(parent_def))))
}

// =============================================================================
// Helpers
// =============================================================================

/// Checks if a type is definitely truthy.
fn is_definitely_truthy(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::Boolean(true))) => true,
        Some(TypeKey::Literal(LiteralValue::String(s))) if !s.is_none() => true,
        Some(TypeKey::Literal(LiteralValue::Number(_ordered_float))) => true, // TODO: Check if zero
        Some(TypeKey::Object(_)) => true,
        Some(TypeKey::Function(_)) => true,
        _ => false,
    }
}

/// Checks if a type is definitely falsy.
fn is_definitely_falsy(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::Boolean(false))) => true,
        Some(TypeKey::Literal(LiteralValue::String(s))) if s.is_none() => true,
        Some(TypeKey::Literal(LiteralValue::Number(_ordered_float))) => true, // TODO: Check if zero
        Some(TypeKey::Intrinsic(IntrinsicKind::Null)) => true,
        Some(TypeKey::Intrinsic(IntrinsicKind::Undefined)) => true,
        Some(TypeKey::Intrinsic(IntrinsicKind::Void)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
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
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::ANY,
            TypeId::STRING,
            TypeId::NUMBER,
        );
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
        let result = compute_conditional_expression_type(
            &interner,
            true_type,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_conditional_falsy_condition() {
        let interner = TypeInterner::new();
        // false ? string : number -> number
        let false_type = interner.literal_boolean(false);
        let result = compute_conditional_expression_type(
            &interner,
            false_type,
            TypeId::STRING,
            TypeId::NUMBER,
        );
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

        let parent_enum_type = interner.intern(TypeKey::Enum(parent_def, TypeId::NUMBER));
        let member_a = interner.intern(TypeKey::Enum(member_a_def, TypeId::NUMBER));
        let member_b = interner.intern(TypeKey::Enum(member_b_def, TypeId::NUMBER));

        let mut resolver = EnumParentResolver::new();
        resolver.parent_map.insert(member_a_def, parent_def);
        resolver.parent_map.insert(member_b_def, parent_def);
        resolver.lazy_map.insert(parent_def, parent_enum_type);

        let result = compute_best_common_type(&interner, &[member_a, member_b], Some(&resolver));
        assert_eq!(result, parent_enum_type);
    }
}
