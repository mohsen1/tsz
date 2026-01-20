//! TypeScript compatibility layer for assignability rules.

use crate::solver::subtype::{NoopResolver, SubtypeChecker, SubtypeFailureReason, TypeResolver};
use crate::solver::types::{PropertyInfo, TypeId, TypeKey};
use crate::solver::{AnyPropagationRules, AssignabilityChecker, TypeDatabase};
use rustc_hash::FxHashMap;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Compatibility checker that applies TypeScript's unsound rules
/// before delegating to the structural subtype engine.
///
/// This layer integrates with the "Lawyer" layer to apply nuanced rules
/// for `any` propagation.
pub struct CompatChecker<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
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
    pub fn new(interner: &'a dyn TypeDatabase) -> CompatChecker<'a, NoopResolver> {
        CompatChecker {
            interner,
            subtype: SubtypeChecker::new(interner),
            lawyer: AnyPropagationRules::new(),
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
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        CompatChecker {
            interner,
            subtype: SubtypeChecker::with_resolver(interner, resolver),
            lawyer: AnyPropagationRules::new(),
            strict_function_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            exact_optional_property_types: false,
            strict_subtype_checking: false,
            cache: FxHashMap::default(),
        }
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
        let key = (source, target);
        if let Some(&cached) = self.cache.get(&key) {
            return cached;
        }

        let result = if source == target {
            true
        } else if let Some(any_result) =
            self.lawyer
                .check_any_propagation(source, target, self.interner)
        {
            // The Lawyer layer decided the outcome based on `any` propagation rules.
            // In default mode, `any` is the JS escape hatch (top + bottom).
            // In strict mode, `any` may delegate to structural checking.
            // See https://github.com/microsoft/TypeScript/issues/10715.
            any_result
        } else if !self.strict_null_checks
            && (source == TypeId::NULL || source == TypeId::UNDEFINED)
        {
            true
        } else if target == TypeId::UNKNOWN {
            // `unknown` is top but not assignable to non-top types. See https://github.com/microsoft/TypeScript/issues/10715.
            true
        } else if source == TypeId::NEVER {
            // `never` is bottom - assignable to everything
            true
        } else if source == TypeId::ERROR || target == TypeId::ERROR {
            // Error types should NOT silently pass assignability checks.
            // This prevents "error poisoning" where a TS2304 (cannot find name) masks
            // downstream TS2322 (type not assignable) errors.
            // Delegate to subtype checker which returns false for ERROR.
            self.configure_subtype(self.strict_function_types);
            self.subtype.is_subtype_of(source, target)
        } else if source == TypeId::UNKNOWN {
            false
        } else if self.violates_weak_union(source, target) {
            false
        } else if self.violates_weak_type(source, target) {
            false
        } else if self.is_empty_object_target(target) {
            // `{}` accepts any non-nullish value (including primitives). See https://github.com/microsoft/TypeScript/issues/60582.
            self.is_assignable_to_empty_object(source)
        } else {
            self.configure_subtype(self.strict_function_types);
            self.subtype.is_subtype_of(source, target)
        };

        self.cache.insert(key, result);
        result
    }

    pub fn is_assignable_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        // Use the lawyer layer for `any` propagation rules
        if let Some(any_result) = self
            .lawyer
            .check_any_propagation(source, target, self.interner)
        {
            return any_result;
        }
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return true;
        }
        if target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }
        if source == TypeId::ERROR || target == TypeId::ERROR {
            // Error types should NOT silently pass assignability checks.
            // Delegate to subtype checker which returns false for ERROR.
            let prev = self.subtype.strict_function_types;
            self.configure_subtype(true);
            let result = self.subtype.is_subtype_of(source, target);
            self.subtype.strict_function_types = prev;
            return result;
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
        if source == target {
            return None;
        }
        // Use the lawyer layer for `any` propagation rules
        if self
            .lawyer
            .check_any_propagation(source, target, self.interner)
            .is_some()
        {
            return None;
        }
        if target == TypeId::UNKNOWN {
            return None;
        }
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return None;
        }
        if source == TypeId::NEVER {
            return None;
        }
        // Error types should NOT return None - let subtype checker explain the failure
        // This prevents "error poisoning" where errors mask downstream type mismatches
        if source == TypeId::ERROR || target == TypeId::ERROR {
            self.configure_subtype(self.strict_function_types);
            return self.subtype.explain_failure(source, target);
        }
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
        if self.is_empty_object_target(target) {
            if self.is_assignable_to_empty_object(source) {
                return None;
            }
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
        // In strict mode, disable method bivariance for soundness
        self.subtype.disable_method_bivariance = self.strict_subtype_checking;
    }

    fn violates_weak_type(&self, source: TypeId, target: TypeId) -> bool {
        let target_key = match self.interner.lookup(target) {
            Some(key) => key,
            None => return false,
        };

        let target_shape = match &target_key {
            TypeKey::Object(shape_id) => self.interner.object_shape(*shape_id),
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    return false;
                }
                shape
            }
            _ => return false,
        };

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

        let mut has_weak_member = false;
        for member in members.iter() {
            let resolved_member = self.resolve_weak_type_ref(*member);
            let Some(member_key) = self.interner.lookup(resolved_member) else {
                continue;
            };
            let shape = match member_key {
                TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                    self.interner.object_shape(shape_id)
                }
                _ => continue,
            };

            if shape.properties.is_empty()
                || shape.string_index.is_some()
                || shape.number_index.is_some()
            {
                return false;
            }

            if shape.properties.iter().all(|prop| prop.optional) {
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
        let source_key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &source_key {
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                // Empty objects are assignable to weak types (all optional properties).
                // Only trigger weak type violation if source has properties that don't overlap.
                let source_props = shape.properties.as_slice();
                !source_props.is_empty() && !self.has_common_property(source_props, target_props)
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                // Empty objects are assignable to weak types (all optional properties).
                // Only trigger weak type violation if source has properties that don't overlap.
                let source_props = shape.properties.as_slice();
                !source_props.is_empty() && !self.has_common_property(source_props, target_props)
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                members
                    .iter()
                    .any(|member| self.violates_weak_type_with_target_props(*member, target_props))
            }
            _ => false,
        }
    }

    fn source_lacks_union_common_property(
        &self,
        source: TypeId,
        target_members: &[TypeId],
    ) -> bool {
        let source = self.resolve_weak_type_ref(source);
        let source_key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &source_key {
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                members
                    .iter()
                    .any(|member| self.source_lacks_union_common_property(*member, target_members))
            }
            TypeKey::TypeParameter(param) => match param.constraint {
                Some(constraint) => {
                    self.source_lacks_union_common_property(constraint, target_members)
                }
                None => false,
            },
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    return false;
                }
                let source_props = shape.properties.as_slice();
                if source_props.is_empty() {
                    return false;
                }

                let mut has_common = false;
                for member in target_members {
                    let resolved_member = self.resolve_weak_type_ref(*member);
                    let Some(member_key) = self.interner.lookup(resolved_member) else {
                        continue;
                    };
                    let shape = match member_key {
                        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                            self.interner.object_shape(shape_id)
                        }
                        _ => continue,
                    };
                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        return false;
                    }
                    if self.has_common_property(source_props, shape.properties.as_slice()) {
                        has_common = true;
                        break;
                    }
                }

                !has_common
            }
            _ => false,
        }
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

    fn is_empty_object_target(&self, target: TypeId) -> bool {
        match self.interner.lookup(target) {
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            _ => false,
        }
    }

    fn is_assignable_to_empty_object(&self, source: TypeId) -> bool {
        if source == TypeId::ANY || source == TypeId::NEVER {
            return true;
        }
        // ERROR types should NOT silently pass - they represent unresolved types
        // and should propagate errors rather than being silently compatible
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

        match &key {
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                members
                    .iter()
                    .all(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(*members);
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
}

#[cfg(test)]
#[path = "compat_tests.rs"]
mod tests;
