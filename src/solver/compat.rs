//! TypeScript compatibility layer for assignability rules.

use crate::interner::Atom;
use crate::solver::db::QueryDatabase;
use crate::solver::diagnostics::SubtypeFailureReason;
use crate::solver::subtype::{NoopResolver, SubtypeChecker, TypeResolver};
use crate::solver::types::{IntrinsicKind, LiteralValue, PropertyInfo, TypeId, TypeKey};
use crate::solver::visitor::{TypeVisitor, is_empty_object_type_db};
use crate::solver::{AnyPropagationRules, AssignabilityChecker, TypeDatabase};
use rustc_hash::FxHashMap;

#[cfg(test)]
use crate::solver::TypeInterner;

// =============================================================================
// Visitor Pattern Implementations
// =============================================================================

/// Visitor to extract object shape ID from types.
struct ShapeExtractor<'a, R: TypeResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
    visiting: rustc_hash::FxHashSet<TypeId>,
}

impl<'a, R: TypeResolver> ShapeExtractor<'a, R> {
    fn new(db: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Self {
            db,
            resolver,
            visiting: rustc_hash::FxHashSet::default(),
        }
    }

    /// Extract shape from a type, returning None if not an object type.
    fn extract(&mut self, type_id: TypeId) -> Option<u32> {
        if !self.visiting.insert(type_id) {
            return None; // Cycle detected
        }
        let result = self.visit_type(self.db, type_id);
        self.visiting.remove(&type_id);
        result
    }
}

/// Visitor to check if a type is string-like (string, string literal, or template literal).
struct StringLikeVisitor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for StringLikeVisitor<'a> {
    type Output = bool;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        kind == IntrinsicKind::String
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        matches!(value, LiteralValue::String(_))
    }

    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        true
    }

    fn visit_type_parameter(&mut self, info: &crate::solver::types::TypeParamInfo) -> Self::Output {
        info.constraint.is_some_and(|c| self.visit_type(self.db, c))
    }

    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        let _symbol_ref = crate::solver::types::SymbolRef(symbol_ref);
        // Resolve the ref and check the resolved type
        // This is a simplified check - in practice we'd need the resolver
        false
    }

    fn visit_lazy(&mut self, _def_id: u32) -> Self::Output {
        // We can't resolve Lazy without a resolver, so conservatively return false
        false
    }

    fn default_output() -> Self::Output {
        false
    }
}

impl<'a, R: TypeResolver> TypeVisitor for ShapeExtractor<'a, R> {
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

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        let def_id = crate::solver::def::DefId(def_id);
        if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.db) {
            return self.extract(resolved);
        }
        None
    }

    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        let symbol_ref = crate::solver::types::SymbolRef(symbol_ref);
        // Phase 3.4: Prefer DefId resolution if available
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol_ref) {
            return self.visit_lazy(def_id.0);
        }
        #[allow(deprecated)]
        if let Some(resolved) = self.resolver.resolve_ref(symbol_ref, self.db) {
            return self.extract(resolved);
        }
        None
    }

    // TSZ-4: Handle Intersection types for nominal checking
    // For private brands, we need to find object shapes within the intersection
    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let member_list = self.db.type_list(crate::solver::types::TypeListId(list_id));
        // For nominal checking, iterate and return the first valid object shape found
        // This ensures we check the private/protected members of constituent types
        for member in member_list.iter() {
            if let Some(shape) = self.visit_type(self.db, *member) {
                return Some(shape);
            }
        }
        None
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

    /// Set the inheritance graph for nominal class subtype checking.
    /// Propagates to the internal SubtypeChecker.
    pub fn set_inheritance_graph(
        &mut self,
        graph: Option<&crate::solver::inheritance::InheritanceGraph>,
    ) {
        // Need to transmute the lifetime because the SubtypeChecker expects &'a but we only have &.
        // This is safe because the InheritanceGraph is owned by CheckerContext which outlives the CompatChecker.
        self.subtype.inheritance_graph = graph.map(|g| unsafe {
            std::mem::transmute::<
                &crate::solver::inheritance::InheritanceGraph,
                &'a crate::solver::inheritance::InheritanceGraph,
            >(g)
        });
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

    /// Apply configuration from JudgeConfig.
    ///
    /// This is used to configure the CompatChecker with settings from
    /// the CompilerOptions (passed through JudgeConfig).
    pub fn apply_config(&mut self, config: &crate::solver::judge::JudgeConfig) {
        self.strict_function_types = config.strict_function_types;
        self.strict_null_checks = config.strict_null_checks;
        self.exact_optional_property_types = config.exact_optional_property_types;
        self.no_unchecked_indexed_access = config.no_unchecked_indexed_access;
        // Clear cache as configuration changed
        self.cache.clear();
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

    /// Check for excess properties in object literal assignment (TS2353).
    ///
    /// This implements the "Lawyer" layer rule where fresh object literals
    /// cannot have properties that don't exist in the target type, unless the
    /// target has an index signature.
    ///
    /// # Arguments
    /// * `source` - The source type (should be a fresh object literal)
    /// * `target` - The target type
    ///
    /// # Returns
    /// `true` if no excess properties found, `false` if TS2353 should be reported
    fn check_excess_properties(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::solver::freshness::is_fresh_object_type;
        use crate::solver::visitor::{ObjectTypeKind, classify_object_type};

        // Only check fresh object literals
        if !is_fresh_object_type(self.interner, source) {
            return true;
        }

        // Get source shape
        let source_shape_id = match classify_object_type(self.interner, source) {
            ObjectTypeKind::Object(shape_id) | ObjectTypeKind::ObjectWithIndex(shape_id) => {
                shape_id
            }
            ObjectTypeKind::NotObject => return true,
        };

        let source_shape = self.interner.object_shape(source_shape_id);

        // Get target shape
        let target_shape_id = match classify_object_type(self.interner, target) {
            ObjectTypeKind::Object(shape_id) | ObjectTypeKind::ObjectWithIndex(shape_id) => {
                shape_id
            }
            ObjectTypeKind::NotObject => return true, // Not an object type, can't check
        };

        let target_shape = self.interner.object_shape(target_shape_id);

        // If target has string index signature, skip excess property check
        if target_shape.string_index.is_some() {
            return true;
        }

        // Collect all target properties (including base types if intersection)
        let target_properties = self.collect_target_properties(target);

        // Check each source property
        for prop_info in &source_shape.properties {
            if !target_properties.contains(&prop_info.name) {
                // Excess property found!
                return false;
            }
        }

        true
    }

    /// Find the first excess property in object literal assignment.
    ///
    /// Returns `Some(property_name)` if an excess property is found, `None` otherwise.
    /// This is used by `explain_failure` to generate TS2353 diagnostics.
    fn find_excess_property(&mut self, source: TypeId, target: TypeId) -> Option<Atom> {
        use crate::solver::freshness::is_fresh_object_type;
        use crate::solver::visitor::{ObjectTypeKind, classify_object_type};

        // Only check fresh object literals
        if !is_fresh_object_type(self.interner, source) {
            return None;
        }

        // Get source shape
        let source_shape_id = match classify_object_type(self.interner, source) {
            ObjectTypeKind::Object(shape_id) | ObjectTypeKind::ObjectWithIndex(shape_id) => {
                shape_id
            }
            ObjectTypeKind::NotObject => return None,
        };

        let source_shape = self.interner.object_shape(source_shape_id);

        // Get target shape - resolve Lazy, Mapped, and Application types
        let target_key = self.interner.lookup(target);
        let resolved_target = match target_key {
            Some(TypeKey::Lazy(def_id)) => {
                // Try to resolve the Lazy type
                if let Some(resolved) = self.subtype.resolver.resolve_lazy(def_id, self.interner) {
                    resolved
                } else {
                    return None;
                }
            }
            Some(TypeKey::Mapped(_)) | Some(TypeKey::Application(_)) => {
                // Evaluate mapped and application types
                self.subtype.evaluate_type(target)
            }
            _ => target,
        };

        let target_shape_id = match classify_object_type(self.interner, resolved_target) {
            ObjectTypeKind::Object(shape_id) | ObjectTypeKind::ObjectWithIndex(shape_id) => {
                shape_id
            }
            ObjectTypeKind::NotObject => return None,
        };

        let target_shape = self.interner.object_shape(target_shape_id);

        // If target has string index signature, skip excess property check
        if target_shape.string_index.is_some() {
            return None;
        }

        // Collect all target properties (including base types if intersection)
        let target_properties = self.collect_target_properties(resolved_target);

        // Check each source property
        for prop_info in &source_shape.properties {
            if !target_properties.contains(&prop_info.name) {
                // Excess property found!
                return Some(prop_info.name);
            }
        }

        None
    }

    /// Collect all property names from a type into a set (handles intersections and unions).
    ///
    /// For intersections: property exists if it's in ANY member
    /// For unions: property exists if it's in ALL members
    fn collect_target_properties(&mut self, type_id: TypeId) -> rustc_hash::FxHashSet<Atom> {
        // Handle Mapped and Application types by evaluating them to concrete types
        // We resolve before matching so the existing logic handles the result.
        let type_id = match self.interner.lookup(type_id) {
            Some(TypeKey::Mapped(_)) | Some(TypeKey::Application(_)) => {
                self.subtype.evaluate_type(type_id)
            }
            _ => type_id,
        };

        let mut properties = rustc_hash::FxHashSet::default();

        match self.interner.lookup(type_id) {
            Some(TypeKey::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                // Property exists if it's in ANY member of intersection
                for &member in members.iter() {
                    let member_props = self.collect_target_properties(member);
                    properties.extend(member_props);
                }
            }
            Some(TypeKey::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                if members.is_empty() {
                    return properties;
                }
                // For unions, property exists if it's in ALL members
                // Start with first member's properties
                let mut all_props = self.collect_target_properties(members[0]);
                // Intersect with remaining members
                for &member in members.iter().skip(1) {
                    let member_props = self.collect_target_properties(member);
                    all_props = all_props.intersection(&member_props).cloned().collect();
                }
                properties = all_props;
            }
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop_info in &shape.properties {
                    properties.insert(prop_info.name);
                }
            }
            _ => {}
        }

        properties
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

        // Enum nominal typing check (Lawyer layer implementation)
        // This provides enum member distinction even without checker context
        if let Some(result) = self.enum_assignability_override(source, target) {
            return result;
        }

        // Weak type checks
        if self.violates_weak_union(source, target) {
            return false;
        }
        if self.violates_weak_type(source, target) {
            return false;
        }

        // Excess property checking (TS2353) - Lawyer layer
        if !self.check_excess_properties(source, target) {
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

        // Excess property checking (TS2353)
        if let Some(excess_prop) = self.find_excess_property(source, target) {
            return Some(SubtypeFailureReason::ExcessProperty {
                property_name: excess_prop,
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
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);

        let target_shape_id = match extractor.extract(target) {
            Some(id) => id,
            None => return false,
        };

        let target_shape = self
            .interner
            .object_shape(crate::solver::types::ObjectShapeId(target_shape_id));

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

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let mut has_weak_member = false;

        for member in members.iter() {
            let resolved_member = self.resolve_weak_type_ref(*member);
            let member_shape_id = match extractor.extract(resolved_member) {
                Some(id) => id,
                None => continue,
            };

            let member_shape = self
                .interner
                .object_shape(crate::solver::types::ObjectShapeId(member_shape_id));

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

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let source_shape_id = match extractor.extract(source) {
            Some(id) => id,
            None => return false,
        };

        let source_shape = self
            .interner
            .object_shape(crate::solver::types::ObjectShapeId(source_shape_id));
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
                Some(constraint) => {
                    self.source_lacks_union_common_property(constraint, target_members)
                }
                None => false,
            };
        }

        // Use visitor for Object types
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let source_shape_id = match extractor.extract(source) {
            Some(id) => id,
            None => return false,
        };

        let source_shape = self
            .interner
            .object_shape(crate::solver::types::ObjectShapeId(source_shape_id));
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

            let member_shape = self
                .interner
                .object_shape(crate::solver::types::ObjectShapeId(member_shape_id));
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
    ///
    /// Uses recursive structure to preserve Union/Intersection semantics:
    /// - Union (A | B): OR logic - must satisfy at least one branch
    /// - Intersection (A & B): AND logic - must satisfy all branches
    pub fn private_brand_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        use crate::solver::types::Visibility;

        // 1. Handle Target Union (OR logic)
        // S -> (A | B) : Valid if S -> A OR S -> B
        if let Some(TypeKey::Union(members)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members);
            // If source matches ANY target member, it's valid
            for &member in members.iter() {
                match self.private_brand_assignability_override(source, member) {
                    Some(true) | None => return None, // Pass (or structural fallback)
                    Some(false) => {}                 // Keep checking other members
                }
            }
            return Some(false); // Failed against all members
        }

        // 2. Handle Source Union (AND logic)
        // (A | B) -> T : Valid if A -> T AND B -> T
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if let Some(false) = self.private_brand_assignability_override(member, target) {
                    return Some(false); // Fail if any member fails
                }
            }
            return None; // All passed or fell back
        }

        // 3. Handle Target Intersection (AND logic)
        // S -> (A & B) : Valid if S -> A AND S -> B
        if let Some(TypeKey::Intersection(members)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if let Some(false) = self.private_brand_assignability_override(source, member) {
                    return Some(false); // Fail if any member fails
                }
            }
            return None; // All passed or fell back
        }

        // 4. Handle Source Intersection (OR logic)
        // (A & B) -> T : Valid if A -> T OR B -> T
        if let Some(TypeKey::Intersection(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                match self.private_brand_assignability_override(member, target) {
                    Some(true) | None => return None, // Pass (or structural fallback)
                    Some(false) => {}                 // Keep checking other members
                }
            }
            return Some(false); // Failed against all members
        }

        // 5. Handle Lazy types (recursive resolution)
        if let Some(TypeKey::Lazy(def_id)) = self.interner.lookup(source) {
            if let Some(resolved) = self.subtype.resolver.resolve_lazy(def_id, self.interner) {
                return self.private_brand_assignability_override(resolved, target);
            }
        }

        if let Some(TypeKey::Lazy(def_id)) = self.interner.lookup(target) {
            if let Some(resolved) = self.subtype.resolver.resolve_lazy(def_id, self.interner) {
                return self.private_brand_assignability_override(source, resolved);
            }
        }

        // 6. Base case: Extract and compare object shapes
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);

        // Get source shape
        let source_shape_id = extractor.extract(source)?;
        let source_shape = self
            .interner
            .object_shape(crate::solver::types::ObjectShapeId(source_shape_id));

        // Get target shape
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let target_shape_id = extractor.extract(target)?;
        let target_shape = self
            .interner
            .object_shape(crate::solver::types::ObjectShapeId(target_shape_id));

        let mut has_private_brands = false;

        // Check Target requirements (Nominality)
        // If Target has a private/protected property, Source MUST match its origin exactly.
        for target_prop in &target_shape.properties {
            if target_prop.visibility == Visibility::Private
                || target_prop.visibility == Visibility::Protected
            {
                has_private_brands = true;
                let source_prop = source_shape
                    .properties
                    .iter()
                    .find(|p| p.name == target_prop.name);

                match source_prop {
                    Some(sp) => {
                        // CRITICAL: The parent_id must match exactly.
                        if sp.parent_id != target_prop.parent_id {
                            return Some(false);
                        }
                    }
                    None => {
                        return Some(false);
                    }
                }
            }
        }

        // Check Source restrictions (Visibility leakage)
        // If Source has a private/protected property, it cannot be assigned to a Target
        // that expects it to be Public.
        for source_prop in &source_shape.properties {
            if source_prop.visibility == Visibility::Private
                || source_prop.visibility == Visibility::Protected
            {
                has_private_brands = true;
                if let Some(target_prop) = target_shape
                    .properties
                    .iter()
                    .find(|p| p.name == source_prop.name)
                {
                    if target_prop.visibility == Visibility::Public {
                        return Some(false);
                    }
                }
            }
        }

        if has_private_brands { Some(true) } else { None }
    }

    /// Enum member assignability override.
    /// Implements nominal typing for enum members: EnumA.X is NOT assignable to EnumB even if values match.
    ///
    /// TypeScript enum rules:
    /// 1. Different enums with different DefIds are NOT assignable (nominal typing)
    /// 2. Numeric enums are bidirectionally assignable to number (Rule #7 - Open Numeric Enums)
    /// 3. String enums are strictly nominal (string literals NOT assignable to string enums)
    /// 4. Same enum members with different values are NOT assignable (EnumA.X != EnumA.Y)
    /// 5. Unions containing enums: Source union assigned to target enum checks all members
    pub fn enum_assignability_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        use crate::solver::type_queries;
        use crate::solver::visitor;

        // Special case: Source union -> Target enum
        // When assigning a union to an enum, ALL enum members in the union must match the target enum.
        // This handles cases like: (EnumA | EnumB) assigned to EnumC
        if let Some((t_def, _)) = visitor::enum_components(self.interner, target) {
            if type_queries::is_union_type(self.interner, source) {
                let union_members = type_queries::get_union_members(self.interner, source)?;

                // Check if any union member is an enum with a different DefId
                for &member in union_members.iter() {
                    if let Some((member_def, _)) = visitor::enum_components(self.interner, member) {
                        if member_def != t_def {
                            // Found an enum in the source union with a different DefId than target
                            // This makes the union NOT assignable to the target enum
                            return Some(false);
                        }
                    }
                }
                // All enums in the union match the target enum DefId.
                // Fall through to structural check to verify non-enum union members.
            }
        }

        let source_enum = visitor::enum_components(self.interner, source);
        let target_enum = visitor::enum_components(self.interner, target);

        match (source_enum, target_enum) {
            // Case 1: Both are enums (or enum members)
            (Some((s_def, _)), Some((t_def, _))) => {
                if s_def != t_def {
                    // Nominal mismatch: EnumA.X is not assignable to EnumB
                    return Some(false);
                }
                // Same enum: Check if they're the exact same member
                // If source == target, they're the same member (assignable)
                // If source != target, they're different members (not assignable)
                Some(source == target)
            }

            // Case 2: Target is an enum, source is a primitive
            (None, Some((t_def, _))) => {
                // Check if target is a numeric enum
                if self.subtype.resolver.is_numeric_enum(t_def) {
                    // Numeric enums allow number assignability (Rule #7)
                    // Return None to let SubtypeChecker handle it structurally
                    None
                } else {
                    // String enums do NOT allow raw string assignability
                    // If source is string or string literal, reject
                    if self.is_string_like(source) {
                        return Some(false);
                    }
                    None
                }
            }

            // Case 3: Source is an enum, target is a primitive
            // Enums are always structurally assignable to their base primitives
            // (e.g., EnumA.X -> number). Let structural check handle it.
            (Some(_), None) => None,

            // Case 4: Neither is an enum
            (None, None) => None,
        }
    }

    /// Check if a type is string-like (string, string literal, or template literal).
    /// Used to reject primitive-to-string-enum assignments.
    fn is_string_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING {
            return true;
        }
        // Use visitor to check for string literals, template literals, etc.
        let mut visitor = StringLikeVisitor { db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }
}

#[cfg(test)]
#[path = "tests/compat_tests.rs"]
mod tests;
