//! Generic type subtype checking.
//!
//! This module handles subtyping for TypeScript's generic and reference types:
//! - Ref types (nominal references to type aliases, classes, interfaces)
//! - TypeQuery (typeof expressions)
//! - Type applications (Generic<T, U>)
//! - Mapped types ({ [K in keyof T]: T[K] })
//! - Type expansion and instantiation

use crate::binder::SymbolId;
use crate::solver::types::*;
use crate::solver::visitor::{
    application_id, index_access_parts, keyof_inner_type, literal_value, object_shape_id,
    object_with_index_shape_id, ref_symbol, type_param_info, union_list_id,
};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Helper for resolving two Ref/TypeQuery symbols and checking subtype.
    ///
    /// Handles the common pattern of:
    /// - Both resolved: check s_type <: t_type
    /// - Only source resolved: check s_type <: target
    /// - Only target resolved: check source <: t_type
    /// - Neither resolved: False
    pub(crate) fn check_resolved_pair_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_resolved: Option<TypeId>,
        t_resolved: Option<TypeId>,
    ) -> SubtypeResult {
        match (s_resolved, t_resolved) {
            (Some(s_type), Some(t_type)) => self.check_subtype(s_type, t_type),
            (Some(s_type), None) => self.check_subtype(s_type, target),
            (None, Some(t_type)) => self.check_subtype(source, t_type),
            (None, None) => SubtypeResult::False,
        }
    }

    /// Check Ref to Ref subtype with optional identity shortcut.
    ///
    /// For class-to-class checks, uses InheritanceGraph for O(1) nominal subtyping
    /// before falling back to structural checking. This is critical for:
    /// - Performance: Avoids expensive member-by-member comparison
    /// - Correctness: Properly handles private/protected members (nominal, not structural)
    /// - Recursive types: Breaks cycles in class inheritance (e.g., `class Box { next: Box }`)
    ///
    /// # DefId-Level Cycle Detection
    ///
    /// This function implements cycle detection at the SymbolRef level (analogous to DefId)
    /// to catch recursive types before resolution. This prevents infinite expansion of
    /// types like:
    /// - `type List<T> = { head: T; tail: List<T> }`
    /// - `interface Recursive { self: Recursive }`
    ///
    /// When we detect that we're comparing the same (source_sym, target_sym) pair that
    /// we're already checking, we return `Provisional` (assumed true) which implements
    /// coinductive semantics.
    pub(crate) fn check_ref_ref_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_sym: &SymbolRef,
        t_sym: &SymbolRef,
    ) -> SubtypeResult {
        // Identity check: same symbol is always a subtype of itself
        if s_sym == t_sym {
            return SubtypeResult::True;
        }

        // =======================================================================
        // DefId-level cycle detection (before resolution!)
        //
        // This catches cycles in recursive type aliases and interfaces at the
        // symbol level, preventing infinite expansion. We check this BEFORE
        // resolving the symbols to their structural forms.
        // =======================================================================
        let ref_pair = (*s_sym, *t_sym);
        if self.seen_refs.contains(&ref_pair) {
            // We're in a cycle at the symbol level - return provisional true
            // This implements coinductive semantics for recursive types
            return SubtypeResult::Provisional;
        }

        // Also check the reversed pair for bivariant cross-recursion
        let reversed_ref_pair = (*t_sym, *s_sym);
        if self.seen_refs.contains(&reversed_ref_pair) {
            return SubtypeResult::Provisional;
        }

        // Mark this pair as being checked
        self.seen_refs.insert(ref_pair);

        // O(1) nominal class subtype checking using InheritanceGraph
        // This short-circuits expensive structural checks for class inheritance
        if let (Some(graph), Some(is_class)) = (self.inheritance_graph, self.is_class_symbol) {
            // Check if both symbols are classes (not interfaces or type aliases)
            let s_is_class = is_class(*s_sym);
            let t_is_class = is_class(*t_sym);

            if s_is_class && t_is_class {
                // Both are classes - use nominal inheritance check
                // Convert SymbolRef to SymbolId for InheritanceGraph
                let s_sid = SymbolId(s_sym.0);
                let t_sid = SymbolId(t_sym.0);

                if graph.is_derived_from(s_sid, t_sid) {
                    // O(1) bitset check: source is a subclass of target
                    self.seen_refs.remove(&ref_pair);
                    return SubtypeResult::True;
                }

                // Not a subclass - fall through to structural check below
                // This handles the case where a class is structurally compatible
                // even though it doesn't inherit from the target
            }
        }

        let s_resolved = self.resolver.resolve_ref(*s_sym, self.interner);
        let t_resolved = self.resolver.resolve_ref(*t_sym, self.interner);
        let result = self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved);

        // Remove from seen set after checking
        self.seen_refs.remove(&ref_pair);

        result
    }

    /// Check TypeQuery to TypeQuery subtype with optional identity shortcut.
    pub(crate) fn check_typequery_typequery_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_sym: &SymbolRef,
        t_sym: &SymbolRef,
    ) -> SubtypeResult {
        if s_sym == t_sym {
            return SubtypeResult::True;
        }

        let s_resolved = self.resolver.resolve_ref(*s_sym, self.interner);
        let t_resolved = self.resolver.resolve_ref(*t_sym, self.interner);
        self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved)
    }

    /// Check Ref to structural type subtype.
    ///
    /// When the source type is a nominal reference (Ref), we must resolve it to
    /// its structural type and then check subtyping against the target.
    pub(crate) fn check_ref_subtype(
        &mut self,
        _source: TypeId,
        target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(s_resolved) => self.check_subtype(s_resolved, target),
            None => SubtypeResult::False,
        }
    }

    /// Check structural type to Ref subtype.
    ///
    /// When the target type is a nominal reference (Ref), we must resolve it to
    /// its structural type and then check if the source is a subtype of that.
    pub(crate) fn check_to_ref_subtype(
        &mut self,
        source: TypeId,
        _target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(t_resolved) => self.check_subtype(source, t_resolved),
            None => SubtypeResult::False,
        }
    }

    /// Check TypeQuery (typeof) to structural type subtype.
    pub(crate) fn check_typequery_subtype(
        &mut self,
        _source: TypeId,
        target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(s_resolved) => self.check_subtype(s_resolved, target),
            None => SubtypeResult::False,
        }
    }

    /// Check structural type to TypeQuery (typeof) subtype.
    pub(crate) fn check_to_typequery_subtype(
        &mut self,
        source: TypeId,
        _target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(t_resolved) => self.check_subtype(source, t_resolved),
            None => SubtypeResult::False,
        }
    }

    /// Check if a generic type application is a subtype of another application.
    ///
    /// Generic type applications (e.g., `Map<string, number>`, `Array<string>`)
    /// are first checked with covariant args (fast path for the common case).
    /// If that fails, we try expanding both applications to their structural forms
    /// and comparing those, which handles contravariant/invariant positions correctly.
    pub(crate) fn check_application_to_application_subtype(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> SubtypeResult {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);

        // Fast path: same base, same args count, covariant args check
        if s_app.args.len() == t_app.args.len()
            && self.check_subtype(s_app.base, t_app.base).is_true()
        {
            let mut all_covariant = true;
            for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                if !self.check_subtype(*s_arg, *t_arg).is_true() {
                    all_covariant = false;
                    break;
                }
            }
            if all_covariant {
                return SubtypeResult::True;
            }
        }

        // Slow path: try expanding both applications to structural form and
        // comparing. This handles cases with contravariant or invariant type
        // parameters, where the covariant fast path incorrectly rejects.
        let s_expanded = self.try_expand_application(s_app_id);
        let t_expanded = self.try_expand_application(t_app_id);
        match (s_expanded, t_expanded) {
            (Some(s_struct), Some(t_struct)) => self.check_subtype(s_struct, t_struct),
            (Some(s_struct), None) => {
                // Re-intern the target application for comparison
                let t_app = self.interner.type_application(t_app_id);
                let target = self.interner.application(t_app.base, t_app.args.clone());
                self.check_subtype(s_struct, target)
            }
            (None, Some(t_struct)) => {
                let s_app = self.interner.type_application(s_app_id);
                let source = self.interner.application(s_app.base, s_app.args.clone());
                self.check_subtype(source, t_struct)
            }
            (None, None) => SubtypeResult::False,
        }
    }

    /// Check Application expansion to target (one-sided Application case).
    ///
    /// When the target is an Application type that can be expanded (e.g., conditional
    /// types, mapped types), we first expand it and then check subtyping.
    pub(crate) fn check_application_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => SubtypeResult::False,
        }
    }

    /// Check source to Application expansion (one-sided Application case).
    ///
    /// When the source is an Application type that can be expanded (e.g., conditional
    /// types, mapped types), we first expand it and then check subtyping.
    pub(crate) fn check_source_to_application_expansion(
        &mut self,
        source: TypeId,
        _target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(source, expanded),
            None => SubtypeResult::False,
        }
    }

    /// Check Mapped expansion to target (one-sided Mapped case).
    ///
    /// When the target is a Mapped type that can be expanded (e.g., `{ [K in keyof T]: T[K] }`),
    /// we first expand it and then check subtyping.
    pub(crate) fn check_mapped_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => SubtypeResult::False,
        }
    }

    /// Check source to Mapped expansion (one-sided Mapped case).
    ///
    /// When the source is a Mapped type that can be expanded, we first expand it
    /// and then check subtyping.
    pub(crate) fn check_source_to_mapped_expansion(
        &mut self,
        source: TypeId,
        _target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(source, expanded),
            None => SubtypeResult::False,
        }
    }

    /// Try to expand an Application type to its structural form.
    /// Returns None if the application cannot be expanded (missing type params or body).
    pub(crate) fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::solver::{TypeSubstitution, instantiate_type};

        let app = self.interner.type_application(app_id);

        // If the base is a Ref, try to resolve and instantiate
        if let Some(symbol) = ref_symbol(self.interner, app.base) {
            // Get type parameters for this symbol
            let type_params = self.resolver.get_type_params(symbol)?;

            // Resolve the base type to get the body
            let resolved = self.resolver.resolve_ref(symbol, self.interner)?;

            // Skip expansion if the resolved type is just this Application
            // (prevents infinite recursion on self-referential types)
            if let Some(resolved_app_id) = application_id(self.interner, resolved)
                && resolved_app_id == app_id
            {
                return None;
            }

            // Create substitution and instantiate
            let substitution = TypeSubstitution::from_args(self.interner, &type_params, &app.args);
            let instantiated = instantiate_type(self.interner, resolved, &substitution);

            // Return the instantiated type for recursive checking
            Some(instantiated)
        } else {
            // Base is not a Ref - can't expand
            None
        }
    }

    /// Try to expand a Mapped type to its structural form.
    /// Returns None if the mapped type cannot be expanded (unresolvable constraint).
    pub(crate) fn try_expand_mapped(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::solver::{
            LiteralValue, MappedModifier, PropertyInfo, TypeSubstitution, instantiate_type,
        };

        let mapped = self.interner.mapped_type(mapped_id);

        // Get concrete keys from the constraint
        let keys = self.try_evaluate_mapped_constraint(mapped.constraint)?;
        if keys.is_empty() {
            return None;
        }

        let (source_object, is_homomorphic) =
            match index_access_parts(self.interner, mapped.template) {
                Some((obj, idx)) => {
                    let is_homomorphic = type_param_info(self.interner, idx)
                        .map(|param| param.name == mapped.type_param.name)
                        .unwrap_or(false);
                    let source_object = if is_homomorphic { Some(obj) } else { None };
                    (source_object, is_homomorphic)
                }
                None => (None, false),
            };

        // Helper to get original property modifiers
        let get_original_modifiers = |key_name: crate::interner::Atom| -> (bool, bool) {
            if let Some(source_obj) = source_object {
                let shape_id = object_shape_id(self.interner, source_obj)
                    .or_else(|| object_with_index_shape_id(self.interner, source_obj));
                if let Some(shape_id) = shape_id {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                }
            }
            (false, false)
        };

        // Build properties by instantiating template for each key
        let mut properties = Vec::new();
        for key_name in keys {
            let key_literal = self
                .interner
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            let instantiated_type = instantiate_type(self.interner, mapped.template, &subst);
            let property_type = self.evaluate_type(instantiated_type);

            // Determine modifiers based on mapped type configuration
            let (original_optional, original_readonly) = get_original_modifiers(key_name);
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        original_optional
                    } else {
                        false
                    }
                }
            };
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        original_readonly
                    } else {
                        false
                    }
                }
            };

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
            });
        }

        Some(self.interner.object(properties))
    }

    /// Try to evaluate a mapped type constraint to get concrete string keys.
    /// Returns None if the constraint can't be resolved to concrete keys.
    pub(crate) fn try_evaluate_mapped_constraint(
        &self,
        constraint: TypeId,
    ) -> Option<Vec<crate::interner::Atom>> {
        use crate::solver::LiteralValue;

        if let Some(operand) = keyof_inner_type(self.interner, constraint) {
            // Try to resolve the operand to get concrete keys
            return self.try_get_keyof_keys(operand);
        }

        if let Some(LiteralValue::String(name)) = literal_value(self.interner, constraint) {
            return Some(vec![name]);
        }

        if let Some(list_id) = union_list_id(self.interner, constraint) {
            let members = self.interner.type_list(list_id);
            let mut keys = Vec::new();
            for &member in members.iter() {
                if let Some(LiteralValue::String(name)) = literal_value(self.interner, member) {
                    keys.push(name);
                }
            }
            return if keys.is_empty() { None } else { Some(keys) };
        }

        None
    }

    /// Try to get keys from keyof an operand type.
    pub(crate) fn try_get_keyof_keys(&self, operand: TypeId) -> Option<Vec<crate::interner::Atom>> {
        let shape_id = object_shape_id(self.interner, operand)
            .or_else(|| object_with_index_shape_id(self.interner, operand));
        if let Some(shape_id) = shape_id {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty() {
                return None;
            }
            return Some(shape.properties.iter().map(|p| p.name).collect());
        }

        if let Some(symbol) = ref_symbol(self.interner, operand) {
            // Try to resolve the ref and get keys from the resolved type
            let resolved = self.resolver.resolve_ref(symbol, self.interner)?;
            if resolved == operand {
                return None; // Avoid infinite recursion
            }
            return self.try_get_keyof_keys(resolved);
        }

        None
    }
}
