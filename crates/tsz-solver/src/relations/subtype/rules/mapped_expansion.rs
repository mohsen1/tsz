//! Concrete mapped-type expansion helpers for subtype relations.

use super::super::{SubtypeChecker, TypeResolver};
use crate::types::{MappedModifier, MappedTypeId, SymbolRef, TypeData, TypeId, Visibility};
use crate::visitor::{
    index_access_parts, keyof_inner_type, lazy_def_id, literal_value, object_shape_id,
    object_with_index_shape_id, type_param_info, union_list_id,
};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Try to expand a Mapped type to its structural form.
    /// Returns None if the mapped type cannot be expanded (unresolvable constraint).
    pub(crate) fn try_expand_mapped(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
        use crate::{LiteralValue, PropertyInfo};

        let mapped = self.interner.get_mapped(mapped_id);

        // Do not expand when the constraint is an intersection that contains
        // `keyof TypeParam` AND the template is an indexed access into that same
        // type parameter (i.e. `{ [K in keyof T & keyof C]: T[K] }`).
        // The key-set derived from T's upper-bound constraint is not definitive:
        // a concrete T could have fewer keys (if it is a proper subtype of its
        // constraint). Expanding here would prematurely collapse the mapped type
        // to the constraint shape (e.g. `Stuff`) and produce wrong error messages.
        // The homomorphic-mapped-to-target path handles the correct assignability
        // check without full expansion.
        if self.mapped_intersection_constraint_has_generic_keyof(mapped_id) {
            return None;
        }

        // A constraint that *definitively* resolves to an empty key space denotes a
        // homomorphic mapped type with no members, which reduces to the empty object
        // type `{}` (and `{}` accepts any object source). This covers `keyof` of any
        // type with no enumerable string/number/symbol keys: a bare function or
        // constructor type, `{}`, etc., all of which evaluate to `never`. It is
        // distinct from a constraint we merely cannot resolve yet (a still-generic
        // `keyof T` for a type parameter), which stays deferred (`KeyOf`, not `never`)
        // and must remain unexpandable.
        if self.evaluate_type(mapped.constraint) == TypeId::NEVER {
            return Some(self.interner.object(Vec::new()));
        }

        // Get concrete keys from the constraint.
        let keys = self.try_evaluate_mapped_constraint(mapped.constraint)?;
        if keys.is_empty() {
            return None;
        }

        let (source_object, is_homomorphic) =
            match index_access_parts(self.interner, mapped.template) {
                Some((obj, idx)) => {
                    let is_homomorphic = type_param_info(self.interner, idx)
                        .is_some_and(|param| param.name == mapped.type_param.name);
                    let source_object = is_homomorphic.then_some(obj);
                    (source_object, is_homomorphic)
                }
                None => (None, false),
            };

        // Helper to get original property modifiers.
        let get_original_modifiers = |key_name: tsz_common::interner::Atom| -> (bool, bool) {
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

        // Build properties by instantiating template for each key.
        //
        // If the mapped type has an `as` clause, the expanded property names must
        // come from the remapped key, not from the original constraint key. When
        // the remapped name is still generic (for example `${K}${T}`), the mapped
        // type is not concretely expandable and must stay deferred.
        let mut properties = Vec::new();
        for key_name in keys {
            // Convert atom to the correct TypeId for substitution.
            // `__unique_<id>` atoms must become UniqueSymbol types so that the template
            // `(p: K) => void` instantiates to `(p: typeof A) => void` rather than
            // `(p: "__unique_<id>") => void` for symbol-keyed mapped types.
            let key_name_str = self.interner.resolve_atom(key_name);
            let key_literal = if let Some(sym_str) = key_name_str.strip_prefix("__unique_")
                && let Ok(id) = sym_str.parse::<u32>()
            {
                self.interner.unique_symbol(SymbolRef(id))
            } else {
                self.interner.literal_string_atom(key_name)
            };

            let subst = TypeSubstitution::single(mapped.type_param.name, key_literal);

            let remapped_names: Vec<tsz_common::interner::Atom> = if let Some(name_type) =
                mapped.name_type
            {
                let remapped =
                    self.evaluate_type(instantiate_type(self.interner, name_type, &subst));
                if remapped == TypeId::NEVER {
                    continue;
                }
                if let Some(LiteralValue::String(name)) = literal_value(self.interner, remapped) {
                    vec![name]
                } else {
                    let list_id = union_list_id(self.interner, remapped)?;
                    let members = self.interner.type_list(list_id);
                    let mut names = Vec::with_capacity(members.len());
                    for &member in members.iter() {
                        let Some(LiteralValue::String(name)) = literal_value(self.interner, member)
                        else {
                            return None;
                        };
                        names.push(name);
                    }
                    if names.is_empty() {
                        return None;
                    }
                    names
                }
            } else {
                vec![key_name]
            };

            let instantiated_type = instantiate_type(self.interner, mapped.template, &subst);
            let property_type = self.evaluate_type(instantiated_type);

            // Determine modifiers based on mapped type configuration.
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

            for remapped_name in remapped_names {
                properties.push(PropertyInfo {
                    name: remapped_name,
                    type_id: property_type,
                    write_type: property_type,
                    optional,
                    readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                });
            }
        }

        Some(self.interner.object(properties))
    }

    /// Try to evaluate a mapped type constraint to get concrete string/symbol keys.
    /// Returns None if the constraint can't be resolved to concrete keys.
    pub(crate) fn try_evaluate_mapped_constraint(
        &mut self,
        constraint: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        use crate::LiteralValue;

        // Evaluate the constraint using the resolver-aware evaluator to handle types
        // like `T['type']` that evaluate to concrete unions `typeof A | typeof B`.
        let evaluated = self.evaluate_type(constraint);
        if evaluated != constraint {
            return self.try_evaluate_mapped_constraint(evaluated);
        }

        if let Some(operand) = keyof_inner_type(self.interner, constraint) {
            // Try to resolve the operand to get concrete keys.
            return self.try_get_keyof_keys(operand);
        }

        if let Some(LiteralValue::String(name)) = literal_value(self.interner, constraint) {
            return Some(vec![name]);
        }

        // Single unique symbol constraint (e.g., `[K in typeof A]: ...`).
        if let Some(TypeData::UniqueSymbol(sym)) = self.interner.lookup(constraint) {
            let atom = self.interner.intern_string(&format!("__unique_{}", sym.0));
            return Some(vec![atom]);
        }

        if let Some(list_id) = union_list_id(self.interner, constraint) {
            let members = self.interner.type_list(list_id);
            let mut keys = Vec::new();
            for &member in members.iter() {
                if let Some(LiteralValue::String(name)) = literal_value(self.interner, member) {
                    keys.push(name);
                } else if let Some(TypeData::UniqueSymbol(sym)) = self.interner.lookup(member) {
                    // Symbol-keyed constraints: `typeof A | typeof B` use `"__unique_<id>"` atoms.
                    let atom = self.interner.intern_string(&format!("__unique_{}", sym.0));
                    keys.push(atom);
                }
            }
            return if keys.is_empty() { None } else { Some(keys) };
        }

        None
    }

    /// Try to get keys from keyof an operand type.
    pub(crate) fn try_get_keyof_keys(
        &mut self,
        operand: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        self.try_get_keyof_keys_depth(operand, 0)
    }

    fn try_get_keyof_keys_depth(
        &mut self,
        operand: TypeId,
        depth: u32,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        if depth > 5 {
            return None;
        }
        let shape_id = object_shape_id(self.interner, operand)
            .or_else(|| object_with_index_shape_id(self.interner, operand));
        if let Some(shape_id) = shape_id {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty() {
                return None;
            }
            return Some(shape.properties.iter().map(|p| p.name).collect());
        }

        if let Some(def_id) = lazy_def_id(self.interner, operand) {
            let resolved = self
                .resolver
                .resolve_lazy(def_id, self.interner)
                .map(|resolved| self.bind_polymorphic_this(operand, resolved))?;
            if resolved == operand {
                return None; // Avoid infinite recursion.
            }
            return self.try_get_keyof_keys_depth(resolved, depth + 1);
        }

        // When the operand is a TypeParameter with a constraint, resolve through
        // the constraint. E.g., for `keyof T` where `T extends { content: C }`,
        // the keys are determined by the constraint `{ content: C }`.
        if let Some(tp) = type_param_info(self.interner, operand)
            && let Some(constraint) = tp.constraint
        {
            // Evaluate the constraint first (e.g., Application(IData, [C]) -> { content: C }).
            let evaluated = self.evaluate_type(constraint);
            if evaluated != operand {
                return self.try_get_keyof_keys_depth(evaluated, depth + 1);
            }
        }

        // When the operand is a ThisType (polymorphic `this`), resolve it to
        // the concrete class/interface instance type via the resolver and recurse.
        // E.g., `keyof this` inside PersonModel -> keyof PersonModel.
        if matches!(self.interner.lookup(operand), Some(TypeData::ThisType))
            && let Some(concrete_this) = self.resolver.resolve_this_type(self.interner)
            && concrete_this != operand
        {
            return self.try_get_keyof_keys_depth(concrete_this, depth + 1);
        }

        None
    }

    /// Guard for `try_expand_mapped`: true only for the homomorphic pattern
    /// `{ [K in keyof T & keyof C]: T[K] }` with T still a type parameter.
    ///
    /// Three conditions must all hold:
    /// 1. Constraint is an intersection with at least one `keyof T` member where T
    ///    is a TypeParameter/Infer.
    /// 2. Template is an indexed access whose object is the **same TypeId** as the
    ///    T identified in (1) — guards against `{ [K in keyof T & ...]: U[K] }` where
    ///    the template indexes a *different* generic.
    /// 3. The index in the template is a `TypeParameter` whose name matches the
    ///    mapped type's iteration variable — guards against `{ [K in keyof T & ...]: T[string] }`.
    ///
    /// Expansion is blocked because T's concrete key-set is unknown; expanding
    /// would collapse the type to the limit shape and produce wrong errors.
    fn mapped_intersection_constraint_has_generic_keyof(&self, mapped_id: MappedTypeId) -> bool {
        let mapped = self.interner.get_mapped(mapped_id);
        let Some(TypeData::Intersection(members)) = self.interner.lookup(mapped.constraint) else {
            return false;
        };
        // (1) Find the TypeId of T from the first `keyof T` member in the intersection.
        let Some(keyof_param) = self.interner.type_list(members).iter().find_map(|&m| {
            if let Some(TypeData::KeyOf(s)) = self.interner.lookup(m)
                && matches!(
                    self.interner.lookup(s),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                )
            {
                return Some(s);
            }
            None
        }) else {
            return false;
        };
        let Some((obj, idx)) = index_access_parts(self.interner, mapped.template) else {
            return false;
        };
        // (2) Template object must be the exact same TypeId as the keyof source.
        if obj != keyof_param {
            return false;
        }
        // (3) Template index must be the mapped iteration variable K (matched by name).
        matches!(
            self.interner.lookup(idx),
            Some(TypeData::TypeParameter(p)) if p.name == mapped.type_param.name
        )
    }
}
