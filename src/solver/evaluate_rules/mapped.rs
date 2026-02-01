//! Mapped type evaluation.
//!
//! Handles TypeScript's mapped types: `{ [K in keyof T]: T[K] }`
//! Including homomorphic mapped types that preserve modifiers.

use crate::interner::Atom;
use crate::solver::instantiate::{TypeSubstitution, instantiate_type};
use crate::solver::subtype::TypeResolver;
use crate::solver::types::*;

use super::super::evaluate::TypeEvaluator;

pub(crate) struct MappedKeys {
    pub string_literals: Vec<Atom>,
    pub has_string: bool,
    pub has_number: bool,
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Helper for key remapping in mapped types.
    /// Returns Ok(Some(remapped)) if remapping succeeded,
    /// Ok(None) if the key should be filtered (remapped to never),
    /// Err(()) if we can't process and should return the original mapped type.
    fn remap_key_type_for_mapped(
        &mut self,
        mapped: &MappedType,
        key_type: TypeId,
    ) -> Result<Option<TypeId>, ()> {
        let Some(name_type) = mapped.name_type else {
            return Ok(Some(key_type));
        };

        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_type);
        let remapped = instantiate_type(self.interner(), name_type, &subst);
        let remapped = self.evaluate(remapped);
        if remapped == TypeId::NEVER {
            return Ok(None);
        }
        Ok(Some(remapped))
    }

    /// Helper to get property modifiers for a given key in a source object.
    fn get_property_modifiers_for_key(
        &self,
        source_object: Option<TypeId>,
        key_name: Atom,
    ) -> (bool, bool) {
        if let Some(source_obj) = source_object {
            if let Some(TypeKey::Object(shape_id)) = self.interner().lookup(source_obj) {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if prop.name == key_name {
                        return (prop.optional, prop.readonly);
                    }
                }
            } else if let Some(TypeKey::ObjectWithIndex(shape_id)) =
                self.interner().lookup(source_obj)
            {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if prop.name == key_name {
                        return (prop.optional, prop.readonly);
                    }
                }
            }
        }
        // Default modifiers when we can't determine
        (false, false)
    }

    /// Helper to compute modifiers for a mapped type property.
    fn get_mapped_modifiers(
        &self,
        mapped: &MappedType,
        is_homomorphic: bool,
        source_object: Option<TypeId>,
        key_name: Atom,
    ) -> (bool, bool) {
        let source_mods = self.get_property_modifiers_for_key(source_object, key_name);

        let optional = match mapped.optional_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => {
                // For homomorphic types with no explicit modifier, preserve original
                if is_homomorphic { source_mods.0 } else { false }
            }
        };

        let readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => {
                // For homomorphic types with no explicit modifier, preserve original
                if is_homomorphic { source_mods.1 } else { false }
            }
        };

        (optional, readonly)
    }

    /// Evaluate a mapped type: { [K in Keys]: Template }
    ///
    /// Algorithm:
    /// 1. Extract the constraint (Keys) - this defines what keys to iterate over
    /// 2. For each key K in the constraint:
    ///    - Substitute K into the template type
    ///    - Apply readonly/optional modifiers
    /// 3. Construct a new object type with the resulting properties
    pub fn evaluate_mapped(&mut self, mapped: &MappedType) -> TypeId {
        // Check if depth was already exceeded
        if self.is_depth_exceeded() {
            return TypeId::ERROR;
        }

        // Get the constraint - this tells us what keys to iterate over
        let constraint = mapped.constraint;

        // SPECIAL CASE: Don't expand mapped types over type parameters.
        // When the constraint is `keyof T` where T is a type parameter, we should
        // keep the mapped type deferred. Even though we might be able to evaluate
        // `keyof T` to concrete keys (via T's constraint), the template instantiation
        // would fail because T[key] can't be resolved for a type parameter.
        //
        // This is critical for patterns like:
        //   function f<T extends any[]>(a: Boxified<T>) { a.pop(); }
        // where Boxified<T> = { [P in keyof T]: Box<T[P]> }
        //
        // If we expand this, T["pop"] becomes ERROR. We need to keep it deferred
        // and handle property access on the deferred mapped type specially.
        if self.is_mapped_type_over_type_parameter(mapped) {
            return self.interner().mapped(mapped.clone());
        }

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_keyof_or_constraint(constraint);

        // If we can't determine concrete keys, keep it as a mapped type (deferred)
        let key_set = match self.extract_mapped_keys(keys) {
            Some(keys) => keys,
            None => return self.interner().mapped(mapped.clone()),
        };

        // Limit number of keys to prevent OOM with large mapped types.
        // WASM environments have limited memory, but 100 is too restrictive for
        // real-world code (large SDKs, generated API types often have 150-250 keys).
        // 250 covers ~99% of real-world use cases while remaining safe for WASM.
        #[cfg(target_arch = "wasm32")]
        const MAX_MAPPED_KEYS: usize = 250;
        #[cfg(not(target_arch = "wasm32"))]
        const MAX_MAPPED_KEYS: usize = 500;
        if key_set.string_literals.len() > MAX_MAPPED_KEYS {
            self.set_depth_exceeded(true);
            return TypeId::ERROR;
        }

        // Check if this is a homomorphic mapped type (template is T[K] indexed access)
        // In this case, we should preserve the original property modifiers
        let is_homomorphic = self.is_homomorphic_mapped_type(mapped);

        // Extract source object type if this is homomorphic
        // For { [K in keyof T]: T[K] }, the constraint is keyof T and template is T[K]
        let source_object = if is_homomorphic {
            self.extract_source_from_homomorphic(mapped)
        } else {
            None
        };

        // Build the resulting object properties
        let mut properties = Vec::new();

        for key_name in key_set.string_literals {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Create substitution: type_param.name -> literal key type
            // First intern the Atom as a literal string type
            let key_literal = self
                .interner()
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));
            let remapped = match self.remap_key_type_for_mapped(mapped, key_literal) {
                Ok(Some(remapped)) => remapped,
                Ok(None) => continue,
                Err(()) => return self.interner().mapped(mapped.clone()),
            };
            let remapped_name = match self.interner().lookup(remapped) {
                Some(TypeKey::Literal(LiteralValue::String(name))) => name,
                _ => return self.interner().mapped(mapped.clone()),
            };

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            // Substitute into the template
            let property_type =
                self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

            // Check if evaluation hit depth limit
            if property_type == TypeId::ERROR && self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Get modifiers for this specific key (preserves homomorphic behavior)
            let (optional, readonly) =
                self.get_mapped_modifiers(mapped, is_homomorphic, source_object, key_name);

            properties.push(PropertyInfo {
                name: remapped_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
            });
        }

        let string_index = if key_set.has_string {
            match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::STRING {
                        return self.interner().mapped(mapped.clone());
                    }
                    let key_type = TypeId::STRING;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

                    // Get modifiers for string index
                    let empty_atom = self.interner().intern_string("");
                    let (idx_optional, idx_readonly) = self.get_mapped_modifiers(
                        mapped,
                        is_homomorphic,
                        source_object,
                        empty_atom,
                    );
                    if idx_optional {
                        value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(mapped.clone()),
            }
        } else {
            None
        };

        let number_index = if key_set.has_number {
            match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::NUMBER {
                        return self.interner().mapped(mapped.clone());
                    }
                    let key_type = TypeId::NUMBER;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

                    // Get modifiers for number index
                    let empty_atom = self.interner().intern_string("");
                    let (idx_optional, idx_readonly) = self.get_mapped_modifiers(
                        mapped,
                        is_homomorphic,
                        source_object,
                        empty_atom,
                    );
                    if idx_optional {
                        value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(mapped.clone()),
            }
        } else {
            None
        };

        if string_index.is_some() || number_index.is_some() {
            self.interner().object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
            })
        } else {
            self.interner().object(properties)
        }
    }

    /// Check if a mapped type's constraint is `keyof T` where T is a type parameter.
    ///
    /// When this is true, we should not expand the mapped type because the template
    /// instantiation would fail (T[key] can't be resolved for a type parameter).
    fn is_mapped_type_over_type_parameter(&self, mapped: &MappedType) -> bool {
        // Check if the constraint is `keyof T`
        let Some(TypeKey::KeyOf(source)) = self.interner().lookup(mapped.constraint) else {
            return false;
        };

        // Check if the source is a type parameter
        matches!(
            self.interner().lookup(source),
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
        )
    }

    /// Evaluate a keyof or constraint type for mapped type iteration.
    fn evaluate_keyof_or_constraint(&mut self, constraint: TypeId) -> TypeId {
        if let Some(TypeKey::Conditional(cond_id)) = self.interner().lookup(constraint) {
            let cond = self.interner().conditional_type(cond_id);
            return self.evaluate_conditional(cond.as_ref());
        }

        // If constraint is already a union of literals, return it
        if let Some(TypeKey::Union(_)) = self.interner().lookup(constraint) {
            return constraint;
        }

        // If constraint is a literal, return it
        if let Some(TypeKey::Literal(LiteralValue::String(_))) = self.interner().lookup(constraint)
        {
            return constraint;
        }

        // If constraint is KeyOf, evaluate it
        if let Some(TypeKey::KeyOf(operand)) = self.interner().lookup(constraint) {
            return self.evaluate_keyof(operand);
        }

        // Otherwise return as-is
        constraint
    }

    /// Extract mapped keys from a type (for mapped type iteration).
    fn extract_mapped_keys(&self, type_id: TypeId) -> Option<MappedKeys> {
        let key = self.interner().lookup(type_id)?;

        let mut keys = MappedKeys {
            string_literals: Vec::new(),
            has_string: false,
            has_number: false,
        };

        match key {
            TypeKey::Literal(LiteralValue::String(s)) => {
                keys.string_literals.push(s);
                Some(keys)
            }
            TypeKey::Union(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        keys.has_string = true;
                        continue;
                    }
                    if member == TypeId::NUMBER {
                        keys.has_number = true;
                        continue;
                    }
                    if member == TypeId::SYMBOL {
                        // We don't model symbol index signatures yet; ignore symbol keys.
                        continue;
                    }
                    if let Some(TypeKey::Literal(LiteralValue::String(s))) =
                        self.interner().lookup(member)
                    {
                        keys.string_literals.push(s);
                    } else {
                        // Non-literal in union - can't fully evaluate
                        return None;
                    }
                }
                if !keys.has_string && !keys.has_number && keys.string_literals.is_empty() {
                    // Only symbol keys (or nothing) - defer until we support symbol indices.
                    return None;
                }
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::String) => {
                keys.has_string = true;
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::Number) => {
                keys.has_number = true;
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::Never) => {
                // Mapped over `never` yields an empty object.
                Some(keys)
            }
            // Can't extract literals from other types
            _ => None,
        }
    }

    /// Check if a mapped type is homomorphic (template is T[K] indexed access).
    /// Homomorphic mapped types preserve modifiers from the source type.
    fn is_homomorphic_mapped_type(&self, mapped: &MappedType) -> bool {
        // Check if template is an IndexAccess type
        match self.interner().lookup(mapped.template) {
            Some(TypeKey::IndexAccess(_obj, idx)) => {
                // Check if the index is our type parameter
                match self.interner().lookup(idx) {
                    Some(TypeKey::TypeParameter(param)) => param.name == mapped.type_param.name,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Extract the source object type from a homomorphic mapped type.
    /// For { [K in keyof T]: T[K] }, extract T.
    fn extract_source_from_homomorphic(&self, mapped: &MappedType) -> Option<TypeId> {
        match self.interner().lookup(mapped.template) {
            Some(TypeKey::IndexAccess(obj, _idx)) => {
                // The object part of T[K] is the source type
                Some(obj)
            }
            _ => None,
        }
    }
}
