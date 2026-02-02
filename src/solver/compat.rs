//! TypeScript compatibility layer for assignability rules.

use crate::solver::db::QueryDatabase;
use crate::solver::diagnostics::SubtypeFailureReason;
use crate::solver::subtype::{NoopResolver, SubtypeChecker, TypeResolver};
use crate::solver::types::{PropertyInfo, TypeId, TypeKey};
use crate::solver::visitor::{is_empty_object_type_db, TypeVisitor};
use crate::solver::{AnyPropagationRules, AssignabilityChecker, TypeDatabase};
use rustc_hash::FxHashMap;

#[cfg(test)]
use crate::solver::TypeInterner;

// =============================================================================
// Visitor Pattern Implementations
// =============================================================================

/// Visitor to extract object shape ID from types.
struct ShapeExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ShapeExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    /// Extract shape from a type, returning None if not an object type.
    fn extract(&mut self, type_id: TypeId) -> Option<u32> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ShapeExtractor<'a> {
    type Output = Option<u32>;

    fn visit_intrinsic(&mut self, _kind: crate::solver::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::solver::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        Some(shape_id)
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        Some(shape_id)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Trait for providing checker-specific assignability overrides.
///
/// This allows the solver's CompatChecker to call back into the checker
/// for special cases that require binder/symbol information (enums,
/// abstract constructors, constructor accessibility).
pub trait AssignabilityOverrideProvider {
    /// Override for enum assignability rules.
    /// Returns Some(true/false) if the override applies, None to fall through to structural checking.
    fn enum_assignability_override(&self, source: TypeId, target: TypeId) -> Option<bool>;

    /// Override for abstract constructor assignability rules.
    /// Returns Some(false) if abstract class cannot be assigned to concrete constructor, None otherwise.
    fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool>;

    /// Override for constructor accessibility rules (private/protected).
    /// Returns Some(false) if accessibility mismatch prevents assignment, None otherwise.
    fn constructor_accessibility_override(&self, source: TypeId, target: TypeId) -> Option<bool>;
}

/// A no-op implementation of AssignabilityOverrideProvider for when no checker context is available.
pub struct NoopOverrideProvider;

impl AssignabilityOverrideProvider for NoopOverrideProvider {
    fn enum_assignability_override(&self, _source: TypeId, _target: TypeId) -> Option<bool> {
        None
    }

    fn abstract_constructor_assignability_override(
        &self,
        _source: TypeId,
        _target: TypeId,
    ) -> Option<bool> {
        None
    }

    fn constructor_accessibility_override(&self, _source: TypeId, _target: TypeId) -> Option<bool> {
        None
    }
}

/// Compatibility checker that applies TypeScript's unsound rules
/// before delegating to the structural subtype engine.
///
/// This layer integrates with the "Lawyer" layer to apply nuanced rules
/// for `any` propagation.
pub struct CompatChecker<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    query_db: Option<&'a dyn QueryDatabase>,
    subtype: SubtypeChecker<'a, R>,
    /// The "Lawyer" layer - handles nuanced rules for `any` propagation.
    lawyer: AnyPropagationRules,
    strict_function_types: bool,
    strict_null_checks: bool,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
    /// When true, enables additional strict subtype checking rules for lib.d.ts
    strict_subtype_checking: bool,
    cache: FxHashMap<(TypeId, TypeId), bool>,
}

impl<'a> CompatChecker<'a, NoopResolver> {
    /// Create a new compatibility checker without a resolver.
    /// Note: Callers should configure strict_function_types explicitly via set_strict_function_types()
    pub fn new(interner: &'a dyn TypeDatabase) -> CompatChecker<'a, NoopResolver> {
        CompatChecker {
            interner,
            query_db: None,
            subtype: SubtypeChecker::new(interner),
            lawyer: AnyPropagationRules::new(),
            // Default to false (legacy TypeScript behavior) for compatibility
            // Callers should set this explicitly based on compiler options
            strict_function_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            exact_optional_property_types: false,
            strict_subtype_checking: false,
            cache: FxHashMap::default(),
        }
    }
}

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    /// Create a new compatibility checker with a resolver.
    /// Note: Callers should configure strict_function_types explicitly via set_strict_function_types()
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        CompatChecker {
            interner,
            query_db: None,
            subtype: SubtypeChecker::with_resolver(interner, resolver),
            lawyer: AnyPropagationRules::new(),
            // Default to false (legacy TypeScript behavior) for compatibility
            // Callers should set this explicitly based on compiler options
            strict_function_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            exact_optional_property_types: false,
            strict_subtype_checking: false,
            cache: FxHashMap::default(),
        }
    }

    /// Set the query database for Salsa-backed memoization.
    /// Propagates to the internal SubtypeChecker.
    pub fn set_query_db(&mut self, db: &'a dyn QueryDatabase) {
        self.query_db = Some(db);
        self.subtype.query_db = Some(db);
    }

    /// Configure strict function parameter checking.
    /// See https://github.com/microsoft/TypeScript/issues/18654.
    pub fn set_strict_function_types(&mut self, strict: bool) {
        if self.strict_function_types != strict {
            self.strict_function_types = strict;
            self.cache.clear();
        }
    }

    /// Configure strict null checks (legacy null/undefined assignability).
    pub fn set_strict_null_checks(&mut self, strict: bool) {
        if self.strict_null_checks != strict {
            self.strict_null_checks = strict;
            self.cache.clear();
        }
    }

    /// Configure unchecked indexed access (include `undefined` in `T[K]`).
    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        if self.no_unchecked_indexed_access != enabled {
            self.no_unchecked_indexed_access = enabled;
            self.cache.clear();
        }
    }

    /// Configure exact optional property types.
    /// See https://github.com/microsoft/TypeScript/issues/13195.
    pub fn set_exact_optional_property_types(&mut self, exact: bool) {
        if self.exact_optional_property_types != exact {
            self.exact_optional_property_types = exact;
            self.cache.clear();
        }
    }

    /// Configure strict mode for `any` propagation.

    /// Configure strict subtype checking mode for lib.d.ts type checking.
    ///
    /// When enabled, applies additional strictness rules that reject borderline
    /// cases allowed by TypeScript's legacy behavior. This includes disabling
    /// method bivariance for soundness.
    pub fn set_strict_subtype_checking(&mut self, strict: bool) {
        if self.strict_subtype_checking != strict {
            self.strict_subtype_checking = strict;
            self.cache.clear();
        }
    }
    ///
    /// When strict mode is enabled, `any` does NOT silence structural mismatches.
    /// This means the type checker will still report errors even when `any` is involved,
    /// if there's a real structural mismatch.
    pub fn set_strict_any_propagation(&mut self, strict: bool) {
        self.lawyer.set_allow_any_suppression(!strict);
        self.cache.clear();
    }

    /// Get a reference to the lawyer layer for `any` propagation rules.
    pub fn lawyer(&self) -> &AnyPropagationRules {
        &self.lawyer
    }

    /// Get a mutable reference to the lawyer layer for `any` propagation rules.
    pub fn lawyer_mut(&mut self) -> &mut AnyPropagationRules {
        self.cache.clear();
        &mut self.lawyer
    }

    /// Check if `source` is assignable to `target` using TS compatibility rules.
    pub fn is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Without strictNullChecks, null and undefined are assignable to and from any type.
        // This check is at the top-level only (not in subtype member iteration) to avoid
        // incorrectly accepting types within union member comparisons.
        if !self.strict_null_checks && (target == TypeId::NULL || target == TypeId::UNDEFINED) {
            return true;
        }

        let key = (source, target);
        if let Some(&cached) = self.cache.get(&key) {
            return cached;
        }

        let result = self.is_assignable_impl(source, target, self.strict_function_types);

        self.cache.insert(key, result);
        result
    }

    /// Internal implementation of assignability check.
    /// Extracted to share logic between is_assignable and is_assignable_strict.
    fn is_assignable_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
        strict_function_types: bool,
    ) -> bool {
        // Fast path checks
        if let Some(result) = self.check_assignable_fast_path(source, target, false) {
            return result;
        }

        // Weak type checks
        if self.violates_weak_union(source, target) {
            return false;
        }
        if self.violates_weak_type(source, target) {
            return false;
        }

        // Empty object target
        if self.is_empty_object_target(target) {
            return self.is_assignable_to_empty_object(source);
        }

        // Default to structural subtype checking
        self.configure_subtype(strict_function_types);
        self.subtype.is_subtype_of(source, target)
    }

    /// Check fast-path assignability conditions.
    /// Returns Some(result) if fast path applies, None if need to do full check.
    fn check_assignable_fast_path(
        &self,
        source: TypeId,
        target: TypeId,
        skip_error_check: bool,
    ) -> Option<bool> {
        // Same type
        if source == target {
            return Some(true);
        }

        // Any at the top-level is assignable to/from everything
        if source == TypeId::ANY || target == TypeId::ANY {
            return Some(true);
        }

        // Null/undefined in non-strict null check mode
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return Some(true);
        }

        // unknown is top
        if target == TypeId::UNKNOWN {
            return Some(true);
        }

        // never is bottom
        if source == TypeId::NEVER {
            return Some(true);
        }

        // Error types are NOT assignable to other types (except themselves)
        // This prevents "error poisoning" where unresolved types mask real errors
        if !skip_error_check && (source == TypeId::ERROR || target == TypeId::ERROR) {
            return Some(source == target);
        }

        // unknown is not assignable to non-top types
        if source == TypeId::UNKNOWN {
            return Some(false);
        }

        None // Need full check
    }

    pub fn is_assignable_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        // Always use strict function types
        if source == target {
            return true;
        }
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return true;
        }
        // Without strictNullChecks, null and undefined are assignable to and from any type.
        // This check is at the top-level only (not in subtype member iteration).
        if !self.strict_null_checks && (target == TypeId::NULL || target == TypeId::UNDEFINED) {
            return true;
        }
        if target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }
        if source == TypeId::ERROR || target == TypeId::ERROR {
            // Error types are only assignable to themselves
            return source == target;
        }
        if source == TypeId::UNKNOWN {
            return false;
        }
        if self.is_empty_object_target(target) {
            return self.is_assignable_to_empty_object(source);
        }

        let prev = self.subtype.strict_function_types;
        self.configure_subtype(true);
        let result = self.subtype.is_subtype_of(source, target);
        self.subtype.strict_function_types = prev;
        result
    }

    /// Explain why `source` is not assignable to `target` using TS compatibility rules.
    pub fn explain_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Fast path: if assignable, no failure to explain
        if source == target {
            return None;
        }
        if target == TypeId::UNKNOWN {
            return None;
        }
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return None;
        }
        // Without strictNullChecks, null and undefined are assignable to and from any type.
        if !self.strict_null_checks && (target == TypeId::NULL || target == TypeId::UNDEFINED) {
            return None;
        }
        if source == TypeId::NEVER {
            return None;
        }
        if source == TypeId::UNKNOWN {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        // Error types should produce ErrorType failure reason
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return Some(SubtypeFailureReason::ErrorType {
                source_type: source,
                target_type: target,
            });
        }

        // Weak type violations
        if self.violates_weak_union(source, target) {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }
        if self.violates_weak_type(source, target) {
            return Some(SubtypeFailureReason::NoCommonProperties {
                source_type: source,
                target_type: target,
            });
        }

        // Empty object target
        if self.is_empty_object_target(target) && self.is_assignable_to_empty_object(source) {
            return None;
        }

        self.configure_subtype(self.strict_function_types);
        self.subtype.explain_failure(source, target)
    }

    fn configure_subtype(&mut self, strict_function_types: bool) {
        self.subtype.strict_function_types = strict_function_types;
        self.subtype.allow_void_return = true;
        self.subtype.allow_bivariant_rest = true;
        self.subtype.exact_optional_property_types = self.exact_optional_property_types;
        self.subtype.strict_null_checks = self.strict_null_checks;
        self.subtype.no_unchecked_indexed_access = self.no_unchecked_indexed_access;
        self.subtype.any_propagation = self.lawyer.any_propagation_mode();
        // In strict mode, disable method bivariance for soundness
        self.subtype.disable_method_bivariance = self.strict_subtype_checking;
    }

    fn violates_weak_type(&self, source: TypeId, target: TypeId) -> bool {
        let mut extractor = ShapeExtractor::new(self.interner);

        let target_shape_id = match extractor.extract(target) {
            Some(id) => id,
            None => return false,
        };

        let target_shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(target_shape_id));

        // ObjectWithIndex with index signatures is not a weak type
        if let Some(TypeKey::ObjectWithIndex(_)) = self.interner.lookup(target) {
            if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                return false;
            }
        }

        let target_props = target_shape.properties.as_slice();
        if target_props.is_empty() || target_props.iter().any(|prop| !prop.optional) {
            return false;
        }

        self.violates_weak_type_with_target_props(source, target_props)
    }

    fn violates_weak_union(&self, source: TypeId, target: TypeId) -> bool {
        let target = self.resolve_weak_type_ref(target);
        let target_key = match self.interner.lookup(target) {
            Some(TypeKey::Union(members)) => members,
            _ => return false,
        };

        let members = self.interner.type_list(target_key);
        if members.is_empty() {
            return false;
        }

        let mut extractor = ShapeExtractor::new(self.interner);
        let mut has_weak_member = false;

        for member in members.iter() {
            let resolved_member = self.resolve_weak_type_ref(*member);
            let member_shape_id = match extractor.extract(resolved_member) {
                Some(id) => id,
                None => continue,
            };

            let member_shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(member_shape_id));

            if member_shape.properties.is_empty()
                || member_shape.string_index.is_some()
                || member_shape.number_index.is_some()
            {
                return false;
            }

            if member_shape.properties.iter().all(|prop| prop.optional) {
                has_weak_member = true;
            }
        }

        if !has_weak_member {
            return false;
        }

        self.source_lacks_union_common_property(source, members.as_ref())
    }

    pub fn is_weak_union_violation(&self, source: TypeId, target: TypeId) -> bool {
        self.violates_weak_union(source, target)
    }

    fn violates_weak_type_with_target_props(
        &self,
        source: TypeId,
        target_props: &[PropertyInfo],
    ) -> bool {
        // Handle Union types explicitly before visitor
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.violates_weak_type_with_target_props(*member, target_props));
        }

        let mut extractor = ShapeExtractor::new(self.interner);
        let source_shape_id = match extractor.extract(source) {
            Some(id) => id,
            None => return false,
        };

        let source_shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(source_shape_id));
        let source_props = source_shape.properties.as_slice();

        // Empty objects are assignable to weak types (all optional properties).
        // Only trigger weak type violation if source has properties that don't overlap.
        !source_props.is_empty() && !self.has_common_property(source_props, target_props)
    }

    fn source_lacks_union_common_property(
        &self,
        source: TypeId,
        target_members: &[TypeId],
    ) -> bool {
        let source = self.resolve_weak_type_ref(source);

        // Handle Union explicitly
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.source_lacks_union_common_property(*member, target_members));
        }

        // Handle TypeParameter explicitly
        if let Some(TypeKey::TypeParameter(param)) = self.interner.lookup(source) {
            return match param.constraint {
                Some(constraint) => self.source_lacks_union_common_property(constraint, target_members),
                None => false,
            };
        }

        // Use visitor for Object types
        let mut extractor = ShapeExtractor::new(self.interner);
        let source_shape_id = match extractor.extract(source) {
            Some(id) => id,
            None => return false,
        };

        let source_shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(source_shape_id));
        if source_shape.string_index.is_some() || source_shape.number_index.is_some() {
            return false;
        }
        let source_props = source_shape.properties.as_slice();
        if source_props.is_empty() {
            return false;
        }

        let mut has_common = false;
        for member in target_members {
            let resolved_member = self.resolve_weak_type_ref(*member);
            let member_shape_id = match extractor.extract(resolved_member) {
                Some(id) => id,
                None => continue,
            };

            let member_shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(member_shape_id));
            if member_shape.string_index.is_some() || member_shape.number_index.is_some() {
                return false;
            }
            if self.has_common_property(source_props, member_shape.properties.as_slice()) {
                has_common = true;
                break;
            }
        }

        !has_common
    }

    fn has_common_property(
        &self,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
    ) -> bool {
        let mut source_idx = 0;
        let mut target_idx = 0;

        while source_idx < source_props.len() && target_idx < target_props.len() {
            let source_name = source_props[source_idx].name;
            let target_name = target_props[target_idx].name;
            if source_name == target_name {
                return true;
            }
            if source_name < target_name {
                source_idx += 1;
            } else {
                target_idx += 1;
            }
        }

        false
    }

    fn resolve_weak_type_ref(&self, type_id: TypeId) -> TypeId {
        self.subtype.resolve_ref_type(type_id)
    }

    /// Check if a type is an empty object target.
    /// Uses the visitor pattern from solver::visitor.
    fn is_empty_object_target(&self, target: TypeId) -> bool {
        is_empty_object_type_db(self.interner, target)
    }

    fn is_assignable_to_empty_object(&self, source: TypeId) -> bool {
        if source == TypeId::ANY || source == TypeId::NEVER {
            return true;
        }
        // ERROR types are NOT assignable to empty object (only reflexive)
        if source == TypeId::ERROR {
            return false;
        }
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return true;
        }
        if source == TypeId::UNKNOWN
            || source == TypeId::NULL
            || source == TypeId::UNDEFINED
            || source == TypeId::VOID
        {
            return false;
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .all(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeKey::TypeParameter(param) => match param.constraint {
                Some(constraint) => self.is_assignable_to_empty_object(constraint),
                None => false,
            },
            _ => true,
        }
    }
}

impl<'a, R: TypeResolver> AssignabilityChecker for CompatChecker<'a, R> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable(source, target)
    }

    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_strict(source, target)
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        // Bypass the cache and perform a one-off check with non-strict function variance.
        self.is_assignable_impl(source, target, false)
    }
}

// =============================================================================
// Assignability Override Functions (moved from checker/state.rs)
// =============================================================================

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    /// Check if `source` is assignable to `target` using TS compatibility rules,
    /// with checker-provided overrides for enums, abstract constructors, and accessibility.
    ///
    /// This is the main entry point for assignability checking when checker context is available.
    pub fn is_assignable_with_overrides<P: AssignabilityOverrideProvider>(
        &mut self,
        source: TypeId,
        target: TypeId,
        overrides: &P,
    ) -> bool {
        // Check override provider for enum assignability
        if let Some(result) = overrides.enum_assignability_override(source, target) {
            return result;
        }

        // Check override provider for abstract constructor assignability
        if let Some(result) = overrides.abstract_constructor_assignability_override(source, target)
        {
            return result;
        }

        // Check override provider for constructor accessibility
        if let Some(result) = overrides.constructor_accessibility_override(source, target) {
            return result;
        }

        // Check private brand assignability (can be done with TypeDatabase alone)
        if let Some(result) = self.private_brand_assignability_override(source, target) {
            return result;
        }

        // Fall through to regular assignability check
        self.is_assignable(source, target)
    }

    /// Private brand assignability override.
    /// If both source and target types have private brands, they must match exactly.
    /// This implements nominal typing for classes with private fields.
    pub fn private_brand_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let source_brand = self.get_private_brand(source);
        let target_brand = self.get_private_brand(target);

        match (source_brand, target_brand) {
            (Some(brand1), Some(brand2)) => {
                // Both types have private brands - they must match exactly
                // Different private brands = different class declarations = not assignable
                Some(brand1 == brand2)
            }
            (None, Some(_)) => {
                // Target has a private brand but source doesn't
                // Source cannot satisfy target's private requirements
                Some(false)
            }
            (Some(_), None) => {
                // Source has a private brand but target doesn't (e.g., interface)
                // Fall through to structural check - a class can implement an interface
                None
            }
            (None, None) => None, // Neither has private brand, fall through to normal check
        }
    }

    /// Extract the private brand property name from a type if it has one.
    /// Returns `Some(brand_name)` if the type has a private brand, `None` otherwise.
    fn get_private_brand(&self, type_id: TypeId) -> Option<String> {
        // Handle Callable explicitly
        if let Some(TypeKey::Callable(callable_id)) = self.interner.lookup(type_id) {
            let callable = self.interner.callable_shape(callable_id);
            for prop in callable.properties.iter() {
                let name = self.interner.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            return None;
        }

        // Use visitor for Object types
        let mut extractor = ShapeExtractor::new(self.interner);
        let shape_id = extractor.extract(type_id)?;
        let shape = self.interner.object_shape(crate::solver::types::ObjectShapeId(shape_id));

        for prop in shape.properties.iter() {
            let name = self.interner.resolve_atom(prop.name);
            if name.starts_with("__private_brand_") {
                return Some(name);
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "tests/compat_tests.rs"]
mod tests;
