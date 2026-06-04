use crate::caches::db::QueryDatabase;

use crate::construction::TypeDatabase;

use crate::diagnostics::SubtypeFailureReason;

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type_cached};

use crate::operations::AssignabilityChecker;

use crate::relations::lawyer::AnyPropagationRules;

use crate::relations::subtype::{NoopResolver, SubtypeChecker, TypeResolver};

use crate::types::{
    IntrinsicKind, LiteralValue, MappedModifier, MappedType, PropertyInfo, SymbolRef, TypeData,
    TypeId,
};

use crate::visitor::{
    TypeVisitor, application_id, array_element_type, index_access_parts, intrinsic_kind,
    is_empty_object_type_through_type_constraints, is_error_type, keyof_inner_type, lazy_def_id,
    mapped_type_id, tuple_list_id, type_param_info, union_list_id,
};

use rustc_hash::FxHashMap;

use tsz_common::interner::Atom;



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
        // Fast path: intrinsic types (primitives, any, never, void, etc.)
        // never have an object shape — `visit_intrinsic` always returns
        // `None`. Short-circuit before paying the recursion-guard
        // enter/leave `FxHashSet` round-trip. `is_intrinsic()` is a free
        // TypeId-range check. Mirrors the same pattern as #1988 (visitor
        // predicates) and #1996 (`RecursiveTypeCollector`).
        if type_id.is_intrinsic() {
            return None;
        }
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

    // `NoInfer<T>` and `ReadonlyType<T>` are transparent wrappers — their shape
    // is the shape of the inner type. Looking through them is required for
    // weak-type detection (TS2559) when the target is e.g. `NoInfer<T> & {...}`.
    fn visit_no_infer(&mut self, inner: TypeId) -> Self::Output {
        self.extract(inner)
    }

    fn visit_readonly_type(&mut self, inner: TypeId) -> Self::Output {
        self.extract(inner)
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
    allow_bivariant_rest: bool,
    /// When true, enables additional strict subtype checking rules for lib.d.ts
    strict_subtype_checking: bool,
    /// When true, disables TypeScript's method parameter bivariance exception.
    disable_method_bivariance: bool,
    /// When true, skip weak type checks (TS2559) during assignability.
    /// This matches tsc's `isTypeAssignableTo` behavior which does not
    /// include the weak type check. The weak type check is only applied
    /// at specific diagnostic sites in tsc.
    skip_weak_type_checks: bool,
    cache: FxHashMap<(TypeId, TypeId), bool>,
}

/// Operation-local cache statistics for [`CompatChecker`].
///
/// Owner: one compatibility-checking request family. The relation memo is
/// cleared when policy-affecting checker options change and is dropped with the
/// checker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompatCheckerCacheStatistics {
    /// Entries in the compatibility relation memo keyed by source and target `TypeId`.
    pub relation_entries: usize,
    estimated_size_bytes: usize,
}

impl CompatCheckerCacheStatistics {
    /// Estimated heap bytes owned by compatibility memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
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
            allow_bivariant_rest: true,
            strict_subtype_checking: false,
            disable_method_bivariance: false,
            skip_weak_type_checks: false,
            cache: FxHashMap::default(),
        }
    }
}
