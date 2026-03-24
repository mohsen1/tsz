//! TypeScript compatibility layer for assignability rules.

use crate::caches::db::QueryDatabase;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::{NoopResolver, SubtypeChecker, TypeResolver};
use crate::types::{IntrinsicKind, LiteralValue, PropertyInfo, TypeData, TypeId};
use crate::visitor::{
    TypeVisitor, intrinsic_kind, is_empty_object_type_through_type_constraints, lazy_def_id,
};
use crate::{AnyPropagationRules, AssignabilityChecker, TypeDatabase};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

// =============================================================================
// Visitor Pattern Implementations
// =============================================================================

/// Visitor to extract object shape ID from types.
pub(crate) struct ShapeExtractor<'a, R: TypeResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, R: TypeResolver> ShapeExtractor<'a, R> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Self {
            db,
            resolver,
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ShapeExtraction,
            ),
        }
    }

    /// Extract shape from a type, returning None if not an object type.
    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<u32> {
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return None, // Cycle or limits exceeded
        }
        let result = self.visit_type(self.db, type_id);
        self.guard.leave(type_id);
        result
    }
}

/// Visitor to check if a type is string-like (string, string literal, or template literal).
pub(crate) struct StringLikeVisitor<'a> {
    pub(crate) db: &'a dyn TypeDatabase,
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

    fn visit_type_parameter(&mut self, info: &crate::types::TypeParamInfo) -> Self::Output {
        info.constraint.is_some_and(|c| self.visit_type(self.db, c))
    }

    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
        // Can't resolve refs without a resolver, conservatively return false
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

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        Some(shape_id)
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        Some(shape_id)
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        let def_id = crate::def::DefId(def_id);
        if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.db) {
            return self.extract(resolved);
        }
        None
    }

    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        let symbol_ref = crate::types::SymbolRef(symbol_ref);
        // Prefer DefId resolution if available
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol_ref) {
            return self.visit_lazy(def_id.0);
        }
        if let Some(resolved) = self.resolver.resolve_symbol_ref(symbol_ref, self.db) {
            return self.extract(resolved);
        }
        None
    }

    // TSZ-4: Handle Intersection types for nominal checking
    // For private brands, we need to find object shapes within the intersection
    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let member_list = self.db.type_list(crate::types::TypeListId(list_id));
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
/// This allows the solver's `CompatChecker` to call back into the checker
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

/// A no-op implementation of `AssignabilityOverrideProvider` for when no checker context is available.
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
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    query_db: Option<&'a dyn QueryDatabase>,
    pub(crate) subtype: SubtypeChecker<'a, R>,
    /// The "Lawyer" layer - handles nuanced rules for `any` propagation.
    lawyer: AnyPropagationRules,
    strict_function_types: bool,
    strict_null_checks: bool,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
    /// When true, enables additional strict subtype checking rules for lib.d.ts
    strict_subtype_checking: bool,
    /// When true, skip weak type checks (TS2559) during assignability.
    /// This matches tsc's `isTypeAssignableTo` behavior which does not
    /// include the weak type check. The weak type check is only applied
    /// at specific diagnostic sites in tsc.
    skip_weak_type_checks: bool,
    cache: FxHashMap<(TypeId, TypeId), bool>,
}

impl<'a> CompatChecker<'a, NoopResolver> {
    /// Create a new compatibility checker without a resolver.
    /// Note: Callers should configure `strict_function_types` explicitly via `set_strict_function_types()`
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
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
            skip_weak_type_checks: false,
            cache: FxHashMap::default(),
        }
    }
}

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    fn normalize_assignability_operand(&mut self, mut type_id: TypeId) -> TypeId {
        // Keep normalization bounded to avoid infinite resolver/evaluator cycles.
        for _ in 0..8 {
            let next = match self.interner.lookup(type_id) {
                Some(TypeData::Lazy(def_id)) => self
                    .subtype
                    .resolver
                    .resolve_lazy(def_id, self.interner)
                    .unwrap_or(type_id),
                Some(TypeData::Mapped(_) | TypeData::Application(_) | TypeData::KeyOf(_)) => {
                    self.subtype.evaluate_type(type_id)
                }
                _ => type_id,
            };

            if next == type_id {
                break;
            }
            type_id = next;
        }
        type_id
    }

    pub(crate) fn normalize_assignability_operands(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (TypeId, TypeId) {
        (
            self.normalize_assignability_operand(source),
            self.normalize_assignability_operand(target),
        )
    }

    const fn uses_generic_failure_surface(reason: &SubtypeFailureReason) -> bool {
        matches!(
            reason,
            SubtypeFailureReason::TypeMismatch { .. }
                | SubtypeFailureReason::NoCommonProperties { .. }
                | SubtypeFailureReason::NoUnionMemberMatches { .. }
                | SubtypeFailureReason::NoIntersectionMemberMatches { .. }
        )
    }

    fn remap_failure_surface(
        reason: SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeFailureReason {
        match reason {
            SubtypeFailureReason::MissingProperty { property_name, .. } => {
                SubtypeFailureReason::MissingProperty {
                    property_name,
                    source_type: source,
                    target_type: target,
                }
            }
            SubtypeFailureReason::MissingProperties { property_names, .. } => {
                SubtypeFailureReason::MissingProperties {
                    property_names,
                    source_type: source,
                    target_type: target,
                }
            }
            SubtypeFailureReason::NoCommonProperties { .. } => {
                SubtypeFailureReason::NoCommonProperties {
                    source_type: source,
                    target_type: target,
                }
            }
            SubtypeFailureReason::NoUnionMemberMatches {
                target_union_members,
                ..
            } => SubtypeFailureReason::NoUnionMemberMatches {
                source_type: source,
                target_union_members,
            },
            SubtypeFailureReason::NoIntersectionMemberMatches { .. } => {
                SubtypeFailureReason::NoIntersectionMemberMatches {
                    source_type: source,
                    target_type: target,
                }
            }
            SubtypeFailureReason::TypeMismatch { .. } => SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            },
            other => other,
        }
    }

    /// Detect whether a type is the global `Object` interface from lib.d.ts.
    ///
    /// Checks via resolver boxed type lookup, Lazy DefId matching, and structural
    /// detection (an `ObjectShape` with `constructor`, `toString`, `valueOf`,
    /// `hasOwnProperty`, and `isPrototypeOf` properties).
    fn is_global_object_interface_target(&self, target: TypeId) -> bool {
        if self
            .subtype
            .resolver
            .is_boxed_type_id(target, IntrinsicKind::Object)
            || self
                .subtype
                .resolver
                .get_boxed_type(IntrinsicKind::Object)
                .is_some_and(|boxed| boxed == target)
        {
            return true;
        }
        if lazy_def_id(self.interner, target).is_some_and(|def_id| {
            self.subtype
                .resolver
                .is_boxed_def_id(def_id, IntrinsicKind::Object)
        }) {
            return true;
        }
        match self.interner.lookup(target) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                // Object interface has exactly 7 properties (constructor, toString,
                // toLocaleString, valueOf, hasOwnProperty, isPrototypeOf,
                // propertyIsEnumerable). Use tight cap to avoid matching derived
                // types like Boolean (8 props) or Number (~10 props).
                if shape.properties.len() > 7 {
                    return false;
                }
                let constructor = self.interner.intern_string("constructor");
                let has_own = self.interner.intern_string("hasOwnProperty");
                let is_proto = self.interner.intern_string("isPrototypeOf");
                let prop_is_enum = self.interner.intern_string("propertyIsEnumerable");
                shape.properties.iter().any(|p| p.name == constructor)
                    && shape.properties.iter().any(|p| p.name == has_own)
                    && shape.properties.iter().any(|p| p.name == is_proto)
                    && shape.properties.iter().any(|p| p.name == prop_is_enum)
            }
            _ => false,
        }
    }

    /// Check if the source has any property whose type conflicts with the Object
    /// interface's property of the same name.
    ///
    /// For example, `{ toString: number }` conflicts because Object requires
    /// `toString: () => string`. But `{ x: number }` doesn't conflict because
    /// `x` is not a property of Object.
    fn has_conflicting_properties_with_object(
        &mut self,
        source: TypeId,
        object_target: TypeId,
    ) -> bool {
        let source_shape_id = match self.interner.lookup(source) {
            Some(TypeData::Object(s) | TypeData::ObjectWithIndex(s)) => s,
            _ => return false,
        };
        let target_shape_id = match self.interner.lookup(object_target) {
            Some(TypeData::Object(s) | TypeData::ObjectWithIndex(s)) => s,
            _ => return false,
        };

        let source_props: Vec<_> = self
            .interner
            .object_shape(source_shape_id)
            .properties
            .clone();
        let target_props: Vec<_> = self
            .interner
            .object_shape(target_shape_id)
            .properties
            .clone();

        for source_prop in &source_props {
            if let Some(target_prop) = target_props.iter().find(|p| p.name == source_prop.name) {
                // Source has a property with the same name as an Object property.
                // Check if the types are compatible.
                self.configure_subtype(self.strict_function_types);
                if !self
                    .subtype
                    .is_subtype_of(source_prop.type_id, target_prop.type_id)
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_function_target_member(&self, member: TypeId) -> bool {
        let is_function_object_shape = match self.interner.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                // Function interface has ~15 properties (own + inherited Object).
                // Cap at 20 to avoid false positives on large interfaces.
                if shape.properties.len() > 20 {
                    false
                } else {
                    let apply = self.interner.intern_string("apply");
                    let call = self.interner.intern_string("call");
                    let bind = self.interner.intern_string("bind");
                    shape.properties.iter().any(|prop| prop.name == apply)
                        && shape.properties.iter().any(|prop| prop.name == call)
                        && shape.properties.iter().any(|prop| prop.name == bind)
                }
            }
            _ => false,
        };

        intrinsic_kind(self.interner, member) == Some(IntrinsicKind::Function)
            || is_function_object_shape
            || self
                .subtype
                .resolver
                .get_boxed_type(IntrinsicKind::Function)
                .is_some_and(|boxed| boxed == member)
            || lazy_def_id(self.interner, member).is_some_and(|def_id| {
                self.subtype
                    .resolver
                    .is_boxed_def_id(def_id, IntrinsicKind::Function)
            })
    }

    /// Create a new compatibility checker with a resolver.
    /// Note: Callers should configure `strict_function_types` explicitly via `set_strict_function_types()`
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
            skip_weak_type_checks: false,
            cache: FxHashMap::default(),
        }
    }

    /// Set the query database for Salsa-backed memoization.
    /// Propagates to the internal `SubtypeChecker`.
    pub fn set_query_db(&mut self, db: &'a dyn QueryDatabase) {
        self.query_db = Some(db);
        self.subtype.query_db = Some(db);
    }

    /// Set the inheritance graph for nominal class subtype checking.
    /// Propagates to the internal `SubtypeChecker`.
    pub const fn set_inheritance_graph(
        &mut self,
        graph: Option<&'a crate::classes::inheritance::InheritanceGraph>,
    ) {
        self.subtype.inheritance_graph = graph;
    }

    /// Configure strict function parameter checking.
    /// See <https://github.com/microsoft/TypeScript/issues/18654>.
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
    /// See <https://github.com/microsoft/TypeScript/issues/13195>.
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

    pub fn set_assume_related_on_cycle(&mut self, assume: bool) {
        if self.subtype.assume_related_on_cycle != assume {
            self.subtype.assume_related_on_cycle = assume;
            self.cache.clear();
        }
    }

    /// Enable generic erasure for function subtype checks.
    ///
    /// When true, non-generic functions can match generic targets by erasing
    /// target type parameters to their constraints. This matches tsc's
    /// `eraseGenerics` behavior used in the comparable relation and base type
    /// structural checks (TS2415/TS2417).
    pub fn set_erase_generics(&mut self, erase: bool) {
        if self.subtype.erase_generics != erase {
            self.subtype.erase_generics = erase;
            self.cache.clear();
        }
    }

    /// Skip weak type checks (TS2559) during assignability.
    ///
    /// In tsc, `isTypeAssignableTo` does not include the weak type check.
    /// The weak type check is only applied at specific diagnostic sites.
    /// This flag matches tsc's `isTypeAssignableTo` behavior.
    pub fn set_skip_weak_type_checks(&mut self, skip: bool) {
        if self.skip_weak_type_checks != skip {
            self.skip_weak_type_checks = skip;
            self.cache.clear();
        }
    }

    /// Apply compiler options from a bitmask flags value.
    ///
    /// The flags correspond to `RelationCacheKey` bits:
    /// - bit 0: `strict_null_checks`
    /// - bit 1: `strict_function_types`
    /// - bit 2: `exact_optional_property_types`
    /// - bit 3: `no_unchecked_indexed_access`
    /// - bit 4: `disable_method_bivariance` (`strict_subtype_checking`)
    /// - bit 5: `allow_void_return`
    /// - bit 6: `allow_bivariant_rest`
    /// - bit 7: `allow_bivariant_param_count`
    ///
    /// This is used by `QueryCache::is_assignable_to_with_flags` to ensure
    /// cached results respect the compiler configuration.
    pub fn apply_flags(&mut self, flags: u16) {
        // Apply flags to CompatChecker's own fields
        let strict_null_checks = (flags & (1 << 0)) != 0;
        let strict_function_types = (flags & (1 << 1)) != 0;
        let exact_optional_property_types = (flags & (1 << 2)) != 0;
        let no_unchecked_indexed_access = (flags & (1 << 3)) != 0;
        let disable_method_bivariance = (flags & (1 << 4)) != 0;

        self.set_strict_null_checks(strict_null_checks);
        self.set_strict_function_types(strict_function_types);
        self.set_exact_optional_property_types(exact_optional_property_types);
        self.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        self.set_strict_subtype_checking(disable_method_bivariance);

        // Also apply flags to the internal SubtypeChecker
        // We do this directly since apply_flags() uses a builder pattern
        self.subtype.strict_null_checks = strict_null_checks;
        self.subtype.strict_function_types = strict_function_types;
        self.subtype.exact_optional_property_types = exact_optional_property_types;
        self.subtype.no_unchecked_indexed_access = no_unchecked_indexed_access;
        self.subtype.disable_method_bivariance = disable_method_bivariance;
        self.subtype.allow_void_return = (flags & (1 << 5)) != 0;
        self.subtype.allow_bivariant_rest = (flags & (1 << 6)) != 0;
        self.subtype.allow_bivariant_param_count = (flags & (1 << 7)) != 0;
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
    pub const fn lawyer(&self) -> &AnyPropagationRules {
        &self.lawyer
    }

    /// Apply configuration from `JudgeConfig`.
    ///
    /// This is used to configure the `CompatChecker` with settings from
    /// the `CompilerOptions` (passed through `JudgeConfig`).
    pub fn apply_config(&mut self, config: &crate::judge::JudgeConfig) {
        self.strict_function_types = config.strict_function_types;
        self.strict_null_checks = config.strict_null_checks;
        self.exact_optional_property_types = config.exact_optional_property_types;
        self.no_unchecked_indexed_access = config.no_unchecked_indexed_access;

        // In tsc, `any` is always assignable to and from all types regardless of
        // strictFunctionTypes. The strictFunctionTypes flag only affects contravariance
        // of function parameters. Sound mode is the opt-in for stricter `any` behavior.
        self.lawyer.allow_any_suppression = !config.sound_mode;

        // Clear cache as configuration changed
        self.cache.clear();
    }

    /// Check if `source` is assignable to `target` using TS compatibility rules.
    pub fn is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Fast identity check — avoids hash map lookup and is_assignable_impl entirely.
        if source == target {
            return true;
        }
        // Without strictNullChecks, null and undefined are assignable to and from any type.
        // This check is at the top-level only (not in subtype member iteration) to avoid
        // incorrectly accepting types within union member comparisons.
        if !self.strict_null_checks && target.is_nullish() {
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
        use super::freshness::is_fresh_object_type;
        use crate::visitor::{ObjectTypeKind, classify_object_type};

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

        let (has_string_index, has_number_index) = self.check_index_signatures(target);

        // If target has string index signature, skip excess property check entirely
        if has_string_index {
            return true;
        }

        // Collect all target properties (including base types if intersection)
        let target_properties = self.collect_target_properties(target);

        // TypeScript forgives excess properties when the target type is completely empty
        // (like `{}`, an empty interface, or an empty class) because it accepts any non-primitive.
        if target_properties.is_empty() && !has_number_index {
            return true;
        }

        // Check each source property
        for prop_info in &source_shape.properties {
            if !target_properties.contains(&prop_info.name) {
                // If target has a numeric index signature, numeric-named properties are allowed
                if has_number_index {
                    let name_str = self.interner.resolve_atom(prop_info.name);
                    if name_str.parse::<f64>().is_ok() {
                        continue;
                    }
                }
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
        use super::freshness::is_fresh_object_type;
        use crate::visitor::{ObjectTypeKind, classify_object_type};

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
            Some(TypeData::Lazy(def_id)) => {
                // Try to resolve the Lazy type
                self.subtype.resolver.resolve_lazy(def_id, self.interner)?
            }
            Some(TypeData::Mapped(_) | TypeData::Application(_)) => {
                // Evaluate mapped and application types
                self.subtype.evaluate_type(target)
            }
            _ => target,
        };

        let (has_string_index, has_number_index) = self.check_index_signatures(resolved_target);

        // If target has string index signature, skip excess property check entirely
        if has_string_index {
            return None;
        }

        // Collect all target properties (including base types if intersection)
        let target_properties = self.collect_target_properties(resolved_target);

        // TypeScript forgives excess properties when the target type is completely empty
        if target_properties.is_empty() && !has_number_index {
            return None;
        }

        // Check each source property
        for prop_info in &source_shape.properties {
            if !target_properties.contains(&prop_info.name) {
                // If target has a numeric index signature, numeric-named properties are allowed
                if has_number_index {
                    let name_str = self.interner.resolve_atom(prop_info.name);
                    if name_str.parse::<f64>().is_ok() {
                        continue;
                    }
                }
                // Excess property found!
                return Some(prop_info.name);
            }
        }

        None
    }

    /// Collect all property names from a type into a set (handles intersections and unions).
    ///
    /// For both intersections and unions: property exists if it's in ANY member.
    /// This matches tsc's `isKnownProperty` semantics for excess property checking.
    ///
    /// Check if a type or any of its composite members has a string or numeric index signature.
    /// Returns `(has_string_index, has_number_index)`.
    fn check_index_signatures(&mut self, type_id: TypeId) -> (bool, bool) {
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return (true, true);
        }

        // The `object` type (like `{}`) conceptually accepts any properties —
        // when it appears in a union, excess property checking should be suppressed.
        if type_id == TypeId::OBJECT {
            return (true, false);
        }

        // The global `Object` interface (capital O from lib.d.ts) also accepts any
        // properties, just like `object`/`{}`. When it appears as a union member
        // (e.g., `Object | string`), excess property checking should be suppressed.
        if self.is_global_object_interface_target(type_id) {
            return (true, false);
        }

        let type_id = match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id)) => self
                .subtype
                .resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id),
            Some(TypeData::Mapped(_) | TypeData::Application(_)) => {
                self.subtype.evaluate_type(type_id)
            }
            _ => type_id,
        };

        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return (true, true);
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                (shape.string_index.is_some(), shape.number_index.is_some())
            }
            Some(TypeData::Intersection(members_id)) | Some(TypeData::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                let mut has_str = false;
                let mut has_num = false;
                for &member in members.iter() {
                    let (s, n) = self.check_index_signatures(member);
                    has_str |= s;
                    has_num |= n;
                }
                (has_str, has_num)
            }
            Some(TypeData::Conditional(cond_id)) => {
                // For unresolved conditional types, check both branches for index
                // signatures. If either branch has one, it's considered present.
                let cond = self.interner.get_conditional(cond_id);
                let (ts, tn) = self.check_index_signatures(cond.true_type);
                let (fs, fn_) = self.check_index_signatures(cond.false_type);
                (ts || fs, tn || fn_)
            }
            _ => (false, false),
        }
    }

    fn collect_target_properties(&mut self, type_id: TypeId) -> rustc_hash::FxHashSet<Atom> {
        // Handle Mapped, Application, Lazy, and Conditional types by evaluating/resolving
        // them to concrete types before property collection.
        let type_id = match self.interner.lookup(type_id) {
            Some(TypeData::Mapped(_) | TypeData::Application(_)) => {
                self.subtype.evaluate_type(type_id)
            }
            Some(TypeData::Lazy(def_id)) => self
                .subtype
                .resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id),
            _ => type_id,
        };

        let mut properties = rustc_hash::FxHashSet::default();

        match self.interner.lookup(type_id) {
            Some(TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                // Property exists if it's in ANY member of intersection
                for &member in members.iter() {
                    let member_props = self.collect_target_properties(member);
                    properties.extend(member_props);
                }
            }
            Some(TypeData::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                // For excess property checking, a property is "known" if it exists
                // in ANY member of the union (same as tsc's isKnownProperty).
                // The source only needs to be assignable to one constituent.
                for &member in members.iter() {
                    let member_props = self.collect_target_properties(member);
                    properties.extend(member_props);
                }
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop_info in &shape.properties {
                    properties.insert(prop_info.name);
                }
            }
            Some(TypeData::Conditional(cond_id)) => {
                // For unresolved conditional types (e.g. T extends U ? X : Y where T
                // is a type parameter), a property is "known" if it exists in either
                // branch. This matches tsc's isKnownProperty behavior — excess property
                // checking should not reject properties that may be valid once the
                // conditional resolves.
                let cond = self.interner.get_conditional(cond_id);
                let true_props = self.collect_target_properties(cond.true_type);
                let false_props = self.collect_target_properties(cond.false_type);
                properties.extend(true_props);
                properties.extend(false_props);
            }
            Some(TypeData::Mapped(mapped_id)) => {
                if let Some(mapped_props) =
                    crate::type_queries::collect_finite_mapped_property_names(
                        self.interner,
                        mapped_id,
                    )
                {
                    properties.extend(mapped_props);
                }
            }
            _ => {}
        }

        properties
    }

    /// Internal implementation of assignability check.
    /// Extracted to share logic between `is_assignable` and `is_assignable_strict`.
    fn is_assignable_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
        strict_function_types: bool,
    ) -> bool {
        let (source, target) = self.normalize_assignability_operands(source, target);

        // Fast path checks
        if let Some(result) = self.check_assignable_fast_path(source, target) {
            return result;
        }

        // Enum nominal typing check (Lawyer layer implementation)
        // This provides enum member distinction even without checker context
        if let Some(result) = self.enum_assignability_override(source, target) {
            return result;
        }

        // Weak type checks (TS2559)
        // Skipped when skip_weak_type_checks is set, matching tsc's
        // isTypeAssignableTo which does not include weak type detection.
        if !self.skip_weak_type_checks {
            if self.violates_weak_union(source, target) {
                return false;
            }
            if self.violates_weak_type(source, target) {
                return false;
            }
        }

        // Excess property checking (TS2353) - Lawyer layer
        if !self.check_excess_properties(source, target) {
            return false;
        }

        // Empty object target or top-like union `{}` | null | undefined
        if let Some((allow_null, allow_undefined)) = self.empty_object_with_nullish_target(target) {
            return self.is_assignable_to_empty_object_or_nullish(
                source,
                allow_null,
                allow_undefined,
            );
        }

        // Empty object target
        if self.is_empty_object_target(target) {
            return self.is_assignable_to_empty_object(source);
        }

        // Check mapped-to-mapped structural comparison before full subtype check.
        if let (Some(TypeData::Mapped(s_mapped_id)), Some(TypeData::Mapped(t_mapped_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let result = self.check_mapped_to_mapped_assignability(s_mapped_id, t_mapped_id);
            if let Some(assignable) = result {
                return assignable;
            }
        }

        // Object interface check
        if !source.is_nullable() {
            let object_target = if self.is_global_object_interface_target(target) {
                Some(target)
            } else if let Some(TypeData::Union(members_id)) = self.interner.lookup(target) {
                let members = self.interner.type_list(members_id);
                members
                    .iter()
                    .find(|&&m| self.is_global_object_interface_target(m))
                    .copied()
            } else {
                None
            };
            if let Some(obj_target) = object_target
                && !self.has_conflicting_properties_with_object(source, obj_target)
            {
                return true;
            }
        }

        // Function interface
        if self.is_function_target_member(target)
            && crate::type_queries::is_callable_type(self.interner, source)
        {
            return true;
        }

        // Default to structural subtype checking
        self.configure_subtype(strict_function_types);
        self.subtype.is_subtype_of(source, target)
    }

    /// Check if two mapped types are assignable via structural template comparison.
    ///
    /// When both source and target are mapped types with the same constraint
    /// (e.g., both iterate over `keyof T`), compare their templates directly.
    /// This handles cases like `Readonly<T>` assignable to `Partial<T>` where
    /// the mapped types can't be concretely expanded because T is generic.
    ///
    /// Returns `Some(true/false)` if determination was made, `None` to fall through.
    fn check_mapped_to_mapped_assignability(
        &mut self,
        s_mapped_id: crate::types::MappedTypeId,
        t_mapped_id: crate::types::MappedTypeId,
    ) -> Option<bool> {
        use super::subtype::rules::generics::flatten_mapped_chain;
        use crate::types::MappedModifier;
        use crate::visitor::mapped_type_id;

        // Try the flattened chain approach first: this handles nested homomorphic
        // mapped types like Partial<Readonly<T>> vs Readonly<Partial<T>>.
        if let (Some(s_flat), Some(t_flat)) = (
            flatten_mapped_chain(self.interner, s_mapped_id),
            flatten_mapped_chain(self.interner, t_mapped_id),
        ) {
            let sources_match = if s_flat.source == t_flat.source {
                true
            } else {
                self.configure_subtype(self.strict_function_types);
                self.subtype.is_subtype_of(s_flat.source, t_flat.source)
            };

            if sources_match {
                // Source has optional but target doesn't → reject
                if s_flat.has_optional && !t_flat.has_optional {
                    return Some(false);
                }
                return Some(true);
            }
        }

        // Fallback: single-level mapped type comparison
        let s_mapped = self.interner.get_mapped(s_mapped_id);
        let t_mapped = self.interner.get_mapped(t_mapped_id);

        // Both must have the same constraint (e.g., both `keyof T`).
        // First try identity, then evaluate to normalize (e.g., keyof(Readonly<T>) → keyof(T)).
        let constraints_match = if s_mapped.constraint == t_mapped.constraint {
            true
        } else {
            let s_eval = self.subtype.evaluate_type(s_mapped.constraint);
            let t_eval = self.subtype.evaluate_type(t_mapped.constraint);
            s_eval == t_eval
        };

        if !constraints_match {
            return None;
        }

        let source_template = s_mapped.template;
        let mut target_template = t_mapped.template;

        // If the target adds optional (`?`), the target template effectively
        // becomes `template | undefined` since optional properties accept undefined.
        let target_adds_optional = t_mapped.optional_modifier == Some(MappedModifier::Add);
        let source_adds_optional = s_mapped.optional_modifier == Some(MappedModifier::Add);

        if target_adds_optional && !source_adds_optional {
            target_template = self.interner.union2(target_template, TypeId::UNDEFINED);
        }

        // If the target removes optional (Required) but source doesn't,
        // fall through to full structural check.
        let target_removes_optional = t_mapped.optional_modifier == Some(MappedModifier::Remove);
        if target_removes_optional && !source_adds_optional && s_mapped.optional_modifier.is_none()
        {
            return None;
        }

        // If both templates are themselves mapped types, recurse
        if let (Some(s_inner), Some(t_inner)) = (
            mapped_type_id(self.interner, source_template),
            mapped_type_id(self.interner, target_template),
        ) {
            return self.check_mapped_to_mapped_assignability(s_inner, t_inner);
        }

        // Compare templates using the subtype checker
        self.configure_subtype(self.strict_function_types);
        Some(self.subtype.is_subtype_of(source_template, target_template))
    }

    /// Check fast-path assignability conditions.
    /// Returns Some(result) if fast path applies, None if need to do full check.
    fn check_assignable_fast_path(&self, source: TypeId, target: TypeId) -> Option<bool> {
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target)
            && let Some(resolved_target) = self.subtype.resolver.resolve_lazy(def_id, self.interner)
            && resolved_target != target
        {
            return self.check_assignable_fast_path(source, resolved_target);
        }

        // Same type
        if source == target {
            return Some(true);
        }

        // Any at the top-level is assignable to/from everything
        // UNLESS strict any propagation is enabled (disables suppression)
        if source == TypeId::ANY || target == TypeId::ANY {
            // North Star Fix: any should not silence structural mismatches.
            // We only allow any to match any here, and fall through to structural
            // checking for mixed pairs.
            if source == target {
                return Some(true);
            }
            // tsc: any is NOT assignable to never (the bottom type).
            // `isSimpleTypeRelatedTo`: `if (s & TypeFlags.Any) return !(t & TypeFlags.Never);`
            if source == TypeId::ANY && target == TypeId::NEVER {
                return Some(false);
            }
            // If legacy suppression is allowed, we still return true here.
            if self.lawyer.allow_any_suppression {
                return Some(true);
            }
            // Fall through to structural checking for unsound pairs
            return None;
        }

        // Null/undefined in non-strict null check mode.
        // Exception: nullish values are NOT assignable to type parameters even
        // without strictNullChecks. In tsc, type parameters are opaque —
        // `null` cannot be assigned to `T` because `T` could be instantiated
        // as `never` or any non-nullable type. The structural subtype check
        // at core.rs:830-889 correctly rejects concrete <: TypeParam, so we
        // must not short-circuit here.
        if !self.strict_null_checks
            && source.is_nullish()
            && !matches!(
                self.interner.lookup(target),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            )
        {
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

        // Error types are assignable to/from everything (like `any`).
        // In tsc, errorType silences further errors to prevent cascading diagnostics.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return Some(true);
        }

        // unknown is not assignable to non-top types
        if source == TypeId::UNKNOWN {
            return Some(false);
        }

        // Compatibility: unions containing `Function` should accept callable sources.
        // Example: `setTimeout(() => {}, 0)` where first arg is `string | Function`.
        if let Some(TypeData::Union(members_id)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members_id);
            if members
                .iter()
                .any(|&member| self.is_function_target_member(member))
                && crate::type_queries::is_callable_type(self.interner, source)
            {
                return Some(true);
            }
        }

        None // Need full check
    }

    pub fn is_assignable_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target)
            && let Some(resolved_target) = self.subtype.resolver.resolve_lazy(def_id, self.interner)
            && resolved_target != target
        {
            return self.is_assignable_strict(source, resolved_target);
        }

        // Always use strict function types
        if source == target {
            return true;
        }
        // Without strictNullChecks, null/undefined are assignable to all types
        // EXCEPT type parameters (which are opaque and could be any type).
        if !self.strict_null_checks
            && source.is_nullish()
            && !matches!(
                self.interner.lookup(target),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            )
        {
            return true;
        }
        // Without strictNullChecks, null and undefined are assignable to and from any type.
        // This check is at the top-level only (not in subtype member iteration).
        if !self.strict_null_checks && target.is_nullish() {
            return true;
        }
        if target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }
        // Error types are assignable to/from everything (like `any` in tsc)
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return true;
        }
        if source == TypeId::UNKNOWN {
            return false;
        }
        if let Some(TypeData::Union(members_id)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members_id);
            if members
                .iter()
                .any(|&member| self.is_function_target_member(member))
                && crate::type_queries::is_callable_type(self.interner, source)
            {
                return true;
            }
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
        if !self.strict_null_checks
            && source.is_nullish()
            && !matches!(
                self.interner.lookup(target),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            )
        {
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

        // Error types are assignable to/from everything (like `any` in tsc)
        // No failure to explain — suppress cascading diagnostics
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return None;
        }

        // Weak type violations
        let violates = self.violates_weak_union(source, target);
        if violates {
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

        // Private brand incompatibility: remember the result but don't short-circuit.
        // Let the structural explain path run first — it may find real missing properties
        // (not just brands) that produce TS2741 instead of generic TS2322.
        // Only use the brand result as a fallback if the structural path returns None.
        let brand_fails = matches!(
            self.private_brand_assignability_override(source, target),
            Some(false)
        );

        // Empty object target or top-like union `{}` | null | undefined
        if let Some((allow_null, allow_undefined)) = self.empty_object_with_nullish_target(target)
            && self.is_assignable_to_empty_object_or_nullish(source, allow_null, allow_undefined)
        {
            return None;
        }

        // Empty object target
        if self.is_empty_object_target(target) && self.is_assignable_to_empty_object(source) {
            return None;
        }

        self.configure_subtype(self.strict_function_types);
        let mut structural_result = self.subtype.explain_failure(source, target);

        if structural_result
            .as_ref()
            .is_none_or(Self::uses_generic_failure_surface)
        {
            let (normalized_source, normalized_target) =
                self.normalize_assignability_operands(source, target);
            if normalized_source != source || normalized_target != target {
                let normalized_result = self
                    .subtype
                    .explain_failure(normalized_source, normalized_target);
                if let Some(normalized_reason) = normalized_result
                    && !Self::uses_generic_failure_surface(&normalized_reason)
                {
                    structural_result = Some(Self::remap_failure_surface(
                        normalized_reason,
                        source,
                        target,
                    ));
                }
            }
        }

        // If the structural path found a useful reason, use it.
        // Otherwise, fall back to the brand mismatch result.
        match (&structural_result, brand_fails) {
            // Structural path found something — prefer it over brand mismatch
            (Some(_), _) => structural_result,
            // No structural result but brand fails — use TypeMismatch
            (None, true) => Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            }),
            // No structural result, no brand issue
            (None, false) => None,
        }
    }

    const fn configure_subtype(&mut self, strict_function_types: bool) {
        self.subtype.strict_function_types = strict_function_types;
        self.subtype.allow_void_return = true;
        self.subtype.allow_bivariant_rest = true;
        self.subtype.exact_optional_property_types = self.exact_optional_property_types;
        self.subtype.strict_null_checks = self.strict_null_checks;
        self.subtype.no_unchecked_indexed_access = self.no_unchecked_indexed_access;
        // Propagate weak type enforcement into nested structural comparisons.
        // This ensures TS2559 is detected not just at the top-level assignment,
        // but also when comparing nested property types (e.g., { a: { y: string } }
        // assigned to { a: { x?: number } }).
        self.subtype.enforce_weak_types = true;
        // Any propagation is controlled by the Lawyer's allow_any_suppression flag
        // Standard TypeScript allows any to propagate through arrays/objects regardless
        // of strictFunctionTypes - it only affects function parameter variance
        self.subtype.any_propagation = self.lawyer.any_propagation_mode();
        // In strict mode, disable method bivariance for soundness
        self.subtype.disable_method_bivariance = self.strict_subtype_checking;
    }

    fn violates_weak_type(&self, source: TypeId, target: TypeId) -> bool {
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);

        let Some(target_shape_id) = extractor.extract(target) else {
            return false;
        };

        let target_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(target_shape_id));

        // ObjectWithIndex with index signatures is not a weak type
        if let Some(TypeData::ObjectWithIndex(_)) = self.interner.lookup(target)
            && (target_shape.string_index.is_some() || target_shape.number_index.is_some())
        {
            return false;
        }

        let target_props = target_shape.properties.as_slice();
        if target_props.is_empty() || target_props.iter().any(|prop| !prop.optional) {
            return false;
        }

        // Target is a weak type (all optional properties). Check source.
        // Array/Tuple types are objects (not primitives) but the ShapeExtractor
        // can't extract their shape. In tsc, arrays have properties like `length`,
        // `push`, etc. that are checked against the weak type's properties.
        // When the source is an array/tuple type, check if the weak target has
        // any property that arrays also have. If not, it's a weak type violation.
        //
        // IMPORTANT: Only apply this when the target is a standalone weak type
        // (Object/ObjectWithIndex), NOT when it's part of an intersection.
        // Intersections like `{ a?: string } & number[]` should not trigger
        // weak type violations because the intersection includes array properties.
        if self.is_array_or_tuple_type(source) {
            // Only trigger the array weak-type check when the target is a
            // standalone object shape, not an intersection or other compound type.
            let target_is_standalone_object = matches!(
                self.interner.lookup(target),
                Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_))
            );
            if target_is_standalone_object {
                return !self.target_has_array_like_property(target_props);
            }
            // For intersection/other compound targets, skip the array check
            // and fall through to the standard weak type check.
        }

        self.violates_weak_type_with_target_props(source, target_props)
    }

    fn violates_weak_union(&self, source: TypeId, target: TypeId) -> bool {
        // Don't resolve the target - check it directly for union type
        // (resolve_weak_type_ref was converting unions to objects, which is wrong)
        let target_key = match self.interner.lookup(target) {
            Some(TypeData::Union(members)) => members,
            _ => {
                return false;
            }
        };

        let members = self.interner.type_list(target_key);
        if members.is_empty() {
            return false;
        }

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let mut has_weak_member = false;

        for member in members.iter() {
            let resolved_member = self.resolve_weak_type_ref(*member);
            // Weak-union checks only apply when ALL union members are object-like.
            // If any member is primitive/non-object (e.g. `string | Function`),
            // TypeScript does not apply TS2559-style weak-type rejection.
            let Some(member_shape_id) = extractor.extract(resolved_member) else {
                return false;
            };

            let member_shape = self
                .interner
                .object_shape(crate::types::ObjectShapeId(member_shape_id));

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
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.violates_weak_type_with_target_props(*member, target_props));
        }

        // The global Object type is exempt from weak type checks.
        // People treat Object as equivalent to {}, even though it declares
        // properties (constructor, toString, etc.). See TypeScript PR #16047.
        if self.is_global_object_interface_target(source) {
            return false;
        }

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let source_shape_id = match extractor.extract(source) {
            Some(id) => id,
            None => {
                // No extractable object shape. tsc considers primitives assignable
                // to weak types: `bigint extends {t?: string}` is valid because
                // the weak type check is about objects with wrong properties.
                // Primitives have no own properties to conflict.
                return false;
            }
        };

        let source_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(source_shape_id));
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
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.source_lacks_union_common_property(*member, target_members));
        }

        // The global Object type is exempt from weak type checks (same as violates_weak_type).
        if self.is_global_object_interface_target(source) {
            return false;
        }

        // Handle TypeParameter explicitly
        if let Some(TypeData::TypeParameter(param)) = self.interner.lookup(source) {
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
            None => {
                // Array/Tuple types are objects but not extractable. They rarely
                // share property names with arbitrary union members, so treat as
                // lacking common properties (matching tsc's getPropertiesOfType
                // behavior for arrays in weak type detection).
                if self.is_array_or_tuple_type(source) {
                    return true;
                }
                return false;
            }
        };

        let source_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(source_shape_id));
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
                .object_shape(crate::types::ObjectShapeId(member_shape_id));
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
        crate::utils::has_common_property_name(source_props, target_props)
    }

    /// Check if a type is an Array or Tuple type.
    /// These are object types but the `ShapeExtractor` can't extract their shape.
    fn is_array_or_tuple_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Array(_)) | Some(TypeData::Tuple(_))
        )
    }

    /// Check if any property in the target weak type has a name commonly found
    /// on Array types (e.g. `length`). This prevents false weak-type violations
    /// for cases like `{ length?: number } | number[]`.
    fn target_has_array_like_property(&self, target_props: &[PropertyInfo]) -> bool {
        // Known property names that exist on Array.prototype / Array instances.
        // We only need to check the most commonly used ones that could appear
        // as optional properties on weak types intended to accept arrays.
        target_props.iter().any(|prop| {
            let name = self.interner.resolve_atom(prop.name);
            matches!(
                name.as_str(),
                "length"
                    | "push"
                    | "pop"
                    | "shift"
                    | "unshift"
                    | "concat"
                    | "join"
                    | "reverse"
                    | "slice"
                    | "sort"
                    | "splice"
                    | "indexOf"
                    | "lastIndexOf"
                    | "every"
                    | "some"
                    | "forEach"
                    | "map"
                    | "filter"
                    | "reduce"
                    | "reduceRight"
                    | "find"
                    | "findIndex"
                    | "fill"
                    | "copyWithin"
                    | "entries"
                    | "keys"
                    | "values"
                    | "includes"
                    | "flatMap"
                    | "flat"
                    | "at"
                    | "toString"
                    | "toLocaleString"
            )
        })
    }

    fn resolve_weak_type_ref(&self, type_id: TypeId) -> TypeId {
        self.subtype.resolve_lazy_type(type_id)
    }

    /// Check if a type is an empty object target.
    /// Uses the visitor pattern from `solver::visitor`.
    fn is_empty_object_target(&self, target: TypeId) -> bool {
        is_empty_object_type_through_type_constraints(self.interner, target)
    }

    fn empty_object_with_nullish_target(&self, target: TypeId) -> Option<(bool, bool)> {
        let TypeData::Union(members) = self.interner.lookup(target)? else {
            return None;
        };
        let members = self.interner.type_list(members);
        let mut saw_empty_object = false;
        let mut allow_null = false;
        let mut allow_undefined = false;
        for &member in members.iter() {
            if self.is_empty_object_target(member) {
                saw_empty_object = true;
                continue;
            }
            match member {
                TypeId::NULL => allow_null = true,
                TypeId::UNDEFINED => allow_undefined = true,
                _ => return None,
            }
        }
        (saw_empty_object && allow_null && allow_undefined).then_some((allow_null, allow_undefined))
    }

    fn is_assignable_to_empty_object_or_nullish(
        &self,
        source: TypeId,
        allow_null: bool,
        allow_undefined: bool,
    ) -> bool {
        if allow_null && allow_undefined {
            return true;
        }
        match source {
            TypeId::NULL => return allow_null,
            TypeId::UNDEFINED => return allow_undefined,
            _ => {}
        }
        self.is_assignable_to_empty_object(source)
    }

    fn is_assignable_to_empty_object(&self, source: TypeId) -> bool {
        if source == TypeId::ANY || source == TypeId::NEVER {
            return true;
        }
        // Error types are assignable to everything (like `any` in tsc)
        if source == TypeId::ERROR {
            return true;
        }
        if !self.strict_null_checks && source.is_nullish() {
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
            TypeData::Union(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .all(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeData::IndexAccess(object_type, _) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, source);
                if evaluated != source {
                    return self.is_assignable_to_empty_object(evaluated);
                }

                !crate::type_queries::is_type_parameter_like(self.interner, object_type)
            }
            TypeData::TypeParameter(param) => match param.constraint {
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
        // Also allow bivariant parameter count: when checking overload compatibility,
        // an implementation with fewer params is still compatible (extra args are ignored in JS).
        let prev = self.subtype.allow_bivariant_param_count;
        self.subtype.allow_bivariant_param_count = true;
        let result = self.is_assignable_impl(source, target, false);
        self.subtype.allow_bivariant_param_count = prev;
        result
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.subtype.evaluate_type(type_id)
    }
}

#[cfg(test)]
#[path = "../../tests/compat_tests.rs"]
mod tests;
