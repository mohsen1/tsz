//! Generic type subtype checking.
//!
//! This module handles subtyping for TypeScript's generic and reference types:
//! - Ref types (nominal references to type aliases, classes, interfaces)
//! - TypeQuery (typeof expressions)
//! - Type applications (Generic<T, U>)
//! - Mapped types ({ [K in keyof T]: T[K] })
//! - Type expansion and instantiation

use crate::solver::types::*;

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
    pub(crate) fn check_ref_ref_subtype(
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
    /// must have:
    /// 1. The same base type (e.g., both are `Map`)
    /// 2. The same number of type arguments
    /// 3. Covariant type arguments (each source arg must be a subtype of target arg)
    pub(crate) fn check_application_to_application_subtype(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> SubtypeResult {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);
        if s_app.args.len() != t_app.args.len() {
            return SubtypeResult::False;
        }
        if !self.check_subtype(s_app.base, t_app.base).is_true() {
            return SubtypeResult::False;
        }
        for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
            if !self.check_subtype(*s_arg, *t_arg).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
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

        // Look up the base type key
        let base_key = self.interner.lookup(app.base)?;

        // If the base is a Ref, try to resolve and instantiate
        if let TypeKey::Ref(symbol) = base_key {
            // Get type parameters for this symbol
            let type_params = self.resolver.get_type_params(symbol)?;

            // Resolve the base type to get the body
            let resolved = self.resolver.resolve_ref(symbol, self.interner)?;

            // Skip expansion if the resolved type is just this Application
            // (prevents infinite recursion on self-referential types)
            let resolved_key = self.interner.lookup(resolved);
            if let Some(TypeKey::Application(resolved_app_id)) = resolved_key
                && resolved_app_id == app_id
            {
                return None;
            }

            // Create substitution and instantiate
            let substitution = TypeSubstitution::from_args(&type_params, &app.args);
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
            LiteralValue, MappedModifier, PropertyInfo, TypeSubstitution, evaluate_type,
            instantiate_type,
        };

        let mapped = self.interner.mapped_type(mapped_id);

        // Get concrete keys from the constraint
        let keys = self.try_evaluate_mapped_constraint(mapped.constraint)?;
        if keys.is_empty() {
            return None;
        }

        // Check if this is a homomorphic mapped type (template is T[K])
        let is_homomorphic = match self.interner.lookup(mapped.template) {
            Some(TypeKey::IndexAccess(_obj, idx)) => match self.interner.lookup(idx) {
                Some(TypeKey::TypeParameter(param)) => param.name == mapped.type_param.name,
                _ => false,
            },
            _ => false,
        };

        // Extract source object type for homomorphic mapped types
        let source_object = if is_homomorphic {
            match self.interner.lookup(mapped.template) {
                Some(TypeKey::IndexAccess(obj, _idx)) => Some(obj),
                _ => None,
            }
        } else {
            None
        };

        // Helper to get original property modifiers
        let get_original_modifiers = |key_name: crate::interner::Atom| -> (bool, bool) {
            if let Some(source_obj) = source_object {
                if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(source_obj) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                } else if let Some(TypeKey::ObjectWithIndex(shape_id)) =
                    self.interner.lookup(source_obj)
                {
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
            let property_type = evaluate_type(self.interner, instantiated_type);

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

        let key = self.interner.lookup(constraint)?;

        match key {
            TypeKey::KeyOf(operand) => {
                // Try to resolve the operand to get concrete keys
                self.try_get_keyof_keys(operand)
            }
            TypeKey::Literal(LiteralValue::String(name)) => Some(vec![name]),
            TypeKey::Union(list_id) => {
                let members = self.interner.type_list(list_id);
                let mut keys = Vec::new();
                for &member in members.iter() {
                    if let Some(TypeKey::Literal(LiteralValue::String(name))) =
                        self.interner.lookup(member)
                    {
                        keys.push(name);
                    }
                }
                if keys.is_empty() { None } else { Some(keys) }
            }
            _ => None,
        }
    }

    /// Try to get keys from keyof an operand type.
    pub(crate) fn try_get_keyof_keys(&self, operand: TypeId) -> Option<Vec<crate::interner::Atom>> {
        let key = self.interner.lookup(operand)?;

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return None;
                }
                Some(shape.properties.iter().map(|p| p.name).collect())
            }
            TypeKey::Ref(symbol) => {
                // Try to resolve the ref and get keys from the resolved type
                let resolved = self.resolver.resolve_ref(symbol, self.interner)?;
                if resolved == operand {
                    return None; // Avoid infinite recursion
                }
                self.try_get_keyof_keys(resolved)
            }
            _ => None,
        }
    }
}
