//! Property access resolution helpers.
//!
//! Contains mapped-type resolution, primitive/array/function/application property
//! resolution, and related utility methods extracted from `operations_property.rs`
//! to keep file sizes manageable.

use super::*;
use crate::apparent_primitive_member_kind;
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{
    FunctionShape, MappedType, MappedTypeId, ParamInfo, PropertyInfo, PropertyLookup, TupleElement,
    TypeApplicationId,
};

impl<'a> PropertyAccessEvaluator<'a> {
    /// Lazily resolve a single property from a mapped type without fully expanding it.
    /// This avoids OOM by only computing the property type that was requested.
    ///
    /// Returns `Some(result)` if we could resolve the property lazily,
    /// `None` if we need to fall back to eager expansion.
    pub(super) fn resolve_mapped_property_lazy(
        &self,
        mapped_id: MappedTypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        use crate::types::MappedModifier;

        let mapped = self.interner().mapped_type(mapped_id);

        // SPECIAL CASE: Mapped types over array-like sources
        // When a mapped type like Boxified<T> = { [P in keyof T]: Box<T[P]> } is applied
        // to an array type, array methods (pop, push, concat, etc.) should NOT be mapped
        // through the template. They should be resolved from the resulting array type.
        //
        // For example: Boxified<T> where T extends any[]
        // - Numeric properties (0, 1, 2) → Box<T[number]>
        // - Array methods (pop, push) → resolved from Array<Box<T[number]>>
        if let Some(result) =
            self.resolve_array_mapped_type_method(&mapped, mapped_id, prop_name, prop_atom)
        {
            return Some(result);
        }

        // Step 1: Check if this property name is valid in the constraint
        // We need to check if the literal string prop_name is in the constraint
        let constraint = mapped.constraint;

        // Try to determine if prop_name is a valid key
        let is_valid_key = self.is_key_in_mapped_constraint(constraint, prop_name);

        if !is_valid_key {
            // Property not in constraint - check if there's a string index signature
            if self.mapped_has_string_index(&mapped) {
                // Has string index - property access is valid
            } else {
                return Some(PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 2: Create a substitution for just this property
        let key_literal = self.interner().literal_string_atom(prop_atom);

        // Handle name remapping if present (e.g., `as` clause in mapped types)
        if let Some(name_type) = mapped.name_type {
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);
            let remapped = instantiate_type(self.interner(), name_type, &subst);
            let remapped = self
                .db
                .evaluate_type_with_options(remapped, self.no_unchecked_indexed_access);
            if remapped == TypeId::NEVER {
                // Key is filtered out by `as never`
                return Some(PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 3: Instantiate the template with this single key
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_literal);
        let property_type = instantiate_type(self.interner(), mapped.template, &subst);
        let property_type = self
            .db
            .evaluate_type_with_options(property_type, self.no_unchecked_indexed_access);

        // Step 4: Apply optional modifier
        let final_type = match mapped.optional_modifier {
            Some(MappedModifier::Add) => self.interner().union2(property_type, TypeId::UNDEFINED),
            Some(MappedModifier::Remove) | None => property_type,
        };

        Some(PropertyAccessResult::Success {
            type_id: final_type,
            write_type: None,
            from_index_signature: false,
        })
    }

    /// Handle array method access on mapped types applied to array-like sources.
    ///
    /// When a mapped type like `{ [P in keyof T]: F<T[P]> }` is applied to an array type,
    /// TypeScript preserves array methods (pop, push, concat, etc.) from the resulting
    /// array type rather than mapping them through the template.
    ///
    /// Returns `Some(result)` if this is an array method on a mapped array type,
    /// `None` otherwise.
    fn resolve_array_mapped_type_method(
        &self,
        mapped: &MappedType,
        _mapped_id: MappedTypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        // Only handle non-numeric property names (array methods)
        // Numeric properties should go through normal template mapping
        if prop_name.parse::<usize>().is_ok() {
            return None;
        }

        // Check if constraint is `keyof T` where T might be array-like
        let source_type = self.get_homomorphic_source(mapped)?;

        // Check if source type is array-like (array, tuple, or type param with array constraint)
        if !self.is_array_like_type(source_type) {
            return None;
        }

        // For array methods, we need to:
        // 1. Compute the mapped element type: F<T[number]>
        // 2. Create Array<mapped_element>
        // 3. Resolve the property on that array type

        // Get the element type mapping: instantiate template with `number` as the key
        let number_type = TypeId::NUMBER;
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, number_type);
        let mapped_element = instantiate_type(self.interner(), mapped.template, &subst);
        let mapped_element = self
            .db
            .evaluate_type_with_options(mapped_element, self.no_unchecked_indexed_access);

        // Create the resulting array type
        let array_type = self.interner().array(mapped_element);

        // Resolve the property on the array type
        let result = self.resolve_array_property(array_type, prop_name, prop_atom);

        // If property not found on array, return None to fall through to normal handling
        if result.is_not_found() {
            return None;
        }

        Some(result)
    }

    /// Get the homomorphic source type for a mapped type.
    ///
    /// For a mapped type like `{ [P in keyof T]: ... }`, returns `T`.
    /// Returns `None` if the mapped type is not homomorphic.
    fn get_homomorphic_source(&self, mapped: &MappedType) -> Option<TypeId> {
        use crate::types::TypeData;

        // Check if constraint is `keyof T`
        if let Some(TypeData::KeyOf(source)) = self.interner().lookup(mapped.constraint) {
            return Some(source);
        }

        None
    }

    /// Check if a type is array-like (array, tuple, or type parameter constrained to array).
    fn is_array_like_type(&self, type_id: TypeId) -> bool {
        use crate::types::TypeData;

        match self.interner().lookup(type_id) {
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
            Some(TypeData::TypeParameter(info)) => {
                // Check if the type parameter has an array-like constraint
                if let Some(constraint) = info.constraint {
                    self.is_array_like_type(constraint)
                } else {
                    false
                }
            }
            Some(TypeData::ReadonlyType(inner)) => self.is_array_like_type(inner),
            // Also check for union types where all members are array-like
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                !members.is_empty() && members.iter().all(|&m| self.is_array_like_type(m))
            }
            Some(TypeData::Intersection(members)) => {
                // For intersection, at least one member should be array-like
                let members = self.interner().type_list(members);
                members.iter().any(|&m| self.is_array_like_type(m))
            }
            _ => false,
        }
    }

    /// Check if a property name is valid in a mapped type's constraint.
    fn is_key_in_mapped_constraint(&self, constraint: TypeId, prop_name: &str) -> bool {
        use crate::types::{LiteralValue, TypeData};

        // Evaluate the constraint to try to reduce it
        let evaluated = self
            .db
            .evaluate_type_with_options(constraint, self.no_unchecked_indexed_access);

        let Some(key) = self.interner().lookup(evaluated) else {
            return false;
        };

        match key {
            // Single string literal - exact match
            TypeData::Literal(LiteralValue::String(s)) => {
                self.interner().resolve_atom(s) == prop_name
            }

            // Union of literals - check if prop_name is in the union
            TypeData::Union(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        // string index covers all string properties
                        return true;
                    }
                    // Recursively check each union member
                    if self.is_key_in_mapped_constraint(member, prop_name) {
                        return true;
                    }
                }
                false
            }

            // Intersection - key must be valid in ALL members
            TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                // For intersection of key types, a key is valid if it's in the intersection
                // This is conservative - we check if it might be valid
                members
                    .iter()
                    .any(|&m| self.is_key_in_mapped_constraint(m, prop_name))
            }

            TypeData::Intrinsic(crate::types::IntrinsicKind::String)
            | TypeData::KeyOf(_)
            | TypeData::TypeParameter(_)
            | TypeData::Conditional(_)
            | TypeData::Application(_)
            | TypeData::Infer(_) => true,

            // Other types - be conservative and reject
            _ => false,
        }
    }

    /// Check if a mapped type has a string index signature (constraint includes `string`).
    fn mapped_has_string_index(&self, mapped: &MappedType) -> bool {
        use crate::types::{IntrinsicKind, TypeData};

        let constraint = mapped.constraint;

        // Evaluate keyof if needed
        let evaluated = if let Some(TypeData::KeyOf(operand)) = self.interner().lookup(constraint) {
            let keyof_type = self.interner().keyof(operand);
            self.db
                .evaluate_type_with_options(keyof_type, self.no_unchecked_indexed_access)
        } else {
            constraint
        };

        if evaluated == TypeId::STRING {
            return true;
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(evaluated) {
            let members = self.interner().type_list(members);
            for &member in members.iter() {
                if member == TypeId::STRING {
                    return true;
                }
                if let Some(TypeData::Intrinsic(IntrinsicKind::String)) =
                    self.interner().lookup(member)
                {
                    return true;
                }
            }
        }

        if let Some(TypeData::Intrinsic(IntrinsicKind::String)) = self.interner().lookup(evaluated)
        {
            return true;
        }

        false
    }

    pub(crate) fn lookup_object_property<'props>(
        &self,
        shape_id: ObjectShapeId,
        props: &'props [PropertyInfo],
        prop_atom: Atom,
    ) -> Option<&'props PropertyInfo> {
        match self.interner().object_property_index(shape_id, prop_atom) {
            PropertyLookup::Found(idx) => props.get(idx),
            PropertyLookup::NotFound => None,
            // Properties are sorted by Atom, use binary search for O(log N)
            PropertyLookup::Uncached => props
                .binary_search_by_key(&prop_atom, |p| p.name)
                .ok()
                .map(|idx| &props[idx]),
        }
    }

    fn any_args_function(&self, return_type: TypeId) -> TypeId {
        let rest_array = self.interner().array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner().function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn method_result(&self, return_type: TypeId) -> PropertyAccessResult {
        PropertyAccessResult::Success {
            type_id: self.any_args_function(return_type),
            write_type: None,
            from_index_signature: false,
        }
    }

    /// Resolve property access on a generic Application type (e.g., `D<string>`) nominally.
    ///
    /// This preserves nominal identity for classes/interfaces instead of structurally
    /// expanding them. The key difference:
    /// - Type aliases: expand structurally (transparent)
    /// - Classes/Interfaces: preserve nominal identity (opaque)
    ///
    /// For `D<string>.a`:
    /// 1. Get Application's base (D) and args ([string])
    /// 2. Resolve base to get its body (Object with properties)
    /// 3. Find property 'a' in the body
    /// 4. Instantiate property type T with arg string -> string
    /// 5. Return instantiated property type (NOT the full structurally expanded type)
    pub(super) fn resolve_application_property(
        &self,
        app_id: TypeApplicationId,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> PropertyAccessResult {
        let app = self.interner().type_application(app_id);
        let prop_atom = prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));

        // Get the base type (should be a Ref to class/interface/alias)
        let base_key = match self.interner().lookup(app.base) {
            Some(k) => k,
            None => {
                return PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                };
            }
        };

        // Handle Object types (e.g., test array interface setup)
        if let TypeData::Object(shape_id) = base_key {
            let shape = self.interner().object_shape(shape_id);

            // Try to find the property in the Object's properties
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, prop_atom) {
                // Get type params from the array base type (stored during test setup)
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    // No type params available, return the property type as-is
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        write_type: None,
                        from_index_signature: false,
                    };
                }

                // Create substitution: map type params to application args
                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                // Instantiate the property type with substitution
                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                // Handle `this` types
                let app_type = self.interner().application(app.base, app.args.clone());
                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    write_type: None,
                    from_index_signature: false,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // Handle ObjectWithIndex types
        if let TypeData::ObjectWithIndex(shape_id) = base_key {
            let shape = self.interner().object_shape(ObjectShapeId(shape_id.0));

            // Try to find the property in the ObjectWithIndex's properties
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, prop_atom) {
                // Get type params
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        write_type: None,
                        from_index_signature: false,
                    };
                }

                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                let app_type = self.interner().application(app.base, app.args.clone());
                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    write_type: None,
                    from_index_signature: false,
                };
            }

            // Check index signatures if property not found
            if let Some(ref idx) = shape.string_index {
                return PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(idx.value_type),
                    write_type: None,
                    from_index_signature: true,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // Handle Callable types (e.g., Array constructor with instance methods as properties)
        if let TypeData::Callable(shape_id) = base_key {
            let shape = self.interner().callable_shape(shape_id);

            // Try to find the property in the Callable's properties
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, prop_atom) {
                // For Callable properties, we need to substitute type parameters
                // The Array Callable has properties that reference the type parameter T
                // We need to substitute T with the element_type from app.args[0]

                // Create substitution: map the Callable's type parameters to the application's arguments
                // For Array, this means T -> element_type
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    // No type params available, return the property type as-is
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        write_type: None,
                        from_index_signature: false,
                    };
                }

                // Task 2.2: Lazy Member Instantiation
                // Instantiate ONLY the property type, not the entire Callable
                // This avoids recursion into other 37+ Array methods
                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                // Use instantiate_type_infer to handle infer vars and avoid depth issues
                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                // Task 2.3: Handle `this` Types
                // Array methods may return `this` or `this[]` which need to be
                // substituted with the actual Application type (e.g., `T[]`)
                let app_type = self.interner().application(app.base, app.args.clone());

                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    write_type: None,
                    from_index_signature: false,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // We only handle Lazy types (def_id references)
        let TypeData::Lazy(def_id) = base_key else {
            // For non-Lazy bases (e.g., TypeParameter), fall back to structural evaluation
            let evaluated = self.db.evaluate_type_with_options(
                self.interner().application(app.base, app.args.clone()),
                self.no_unchecked_indexed_access,
            );
            return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
        };

        // Resolve the def_id to get the SymbolId, then get the body type
        let sym_id = match self.db.def_to_symbol_id(def_id) {
            Some(id) => id,
            None => {
                // Can't convert def_id to symbol_id - fall back to structural evaluation
                let evaluated = self.db.evaluate_type_with_options(
                    self.interner().application(app.base, app.args.clone()),
                    self.no_unchecked_indexed_access,
                );
                return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
            }
        };

        let symbol_ref = crate::SymbolRef(sym_id.0);

        // Resolve the symbol to get its body type
        let body_type = if let Some(inner_def_id) = self.db.symbol_to_def_id(symbol_ref) {
            self.db.resolve_lazy(inner_def_id, self.interner())
        } else {
            self.db.resolve_symbol_ref(symbol_ref, self.interner())
        };

        let Some(body_type) = body_type else {
            // Resolution failed - fall back to structural evaluation
            let evaluated = self.db.evaluate_type_with_options(
                self.interner().application(app.base, app.args.clone()),
                self.no_unchecked_indexed_access,
            );
            return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
        };

        // Get type parameters for this symbol (try SymbolRef first, then DefId)
        let type_params = match self.db.get_type_params(symbol_ref) {
            Some(params) if !params.is_empty() => params,
            _ => match self.db.get_lazy_type_params(def_id) {
                Some(params) if !params.is_empty() => params,
                _ => {
                    // No type params - resolve on the body directly
                    return self.resolve_property_access_inner(
                        body_type,
                        prop_name,
                        Some(prop_atom),
                    );
                }
            },
        };

        // The body should be an Object type with properties
        let body_key = match self.interner().lookup(body_type) {
            Some(k) => k,
            None => {
                return PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                };
            }
        };

        // Handle Object types (classes/interfaces)
        match body_key {
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);

                // Try to find the property in the shape
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    // Found! Now instantiate the property type with the type arguments
                    let substitution =
                        TypeSubstitution::from_args(self.interner(), &type_params, &app.args);

                    // Instantiate both read and write types
                    let instantiated_read_type =
                        instantiate_type(self.interner(), prop.type_id, &substitution);
                    let instantiated_write_type =
                        instantiate_type(self.interner(), prop.write_type, &substitution);

                    let read_type = self.optional_property_type(&PropertyInfo {
                        name: prop.name,
                        type_id: instantiated_read_type,
                        write_type: instantiated_write_type,
                        readonly: prop.readonly,
                        optional: prop.optional,
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                    let write = (instantiated_write_type != instantiated_read_type)
                        .then_some(instantiated_write_type);
                    return PropertyAccessResult::Success {
                        type_id: read_type,
                        write_type: write,
                        from_index_signature: false,
                    };
                }

                // Property not found in explicit properties - check index signatures
                if let Some(ref idx) = shape.string_index {
                    // Found string index signature - instantiate the value type
                    let substitution =
                        TypeSubstitution::from_args(self.interner(), &type_params, &app.args);
                    let instantiated_value =
                        instantiate_type(self.interner(), idx.value_type, &substitution);

                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(instantiated_value),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                // Check numeric index signature for numeric property names
                use crate::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner());
                if resolver.is_numeric_index_name(prop_name)
                    && let Some(ref idx) = shape.number_index
                {
                    let substitution =
                        TypeSubstitution::from_args(self.interner(), &type_params, &app.args);
                    let instantiated_value =
                        instantiate_type(self.interner(), idx.value_type, &substitution);

                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(instantiated_value),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                // Property not found
                PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                }
            }
            // For non-Object body types (e.g., type aliases to unions), fall back to evaluation
            _ => {
                let evaluated = self.db.evaluate_type_with_options(
                    self.interner().application(app.base, app.args.clone()),
                    self.no_unchecked_indexed_access,
                );
                self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom))
            }
        }
    }

    pub(crate) fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner().union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn resolve_apparent_property(
        &self,
        kind: IntrinsicKind,
        owner_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match apparent_primitive_member_kind(self.interner(), kind, prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => PropertyAccessResult::Success {
                type_id,
                write_type: None,
                from_index_signature: false,
            },
            Some(ApparentMemberKind::Method(return_type)) => self.method_result(return_type),
            None => PropertyAccessResult::PropertyNotFound {
                type_id: owner_type,
                property_name: prop_atom,
            },
        }
    }

    pub(crate) fn resolve_object_member(
        &self,
        prop_name: &str,
        _prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        match apparent_object_member_kind(prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => Some(PropertyAccessResult::Success {
                type_id,
                write_type: None,
                from_index_signature: false,
            }),
            Some(ApparentMemberKind::Method(return_type)) => Some(self.method_result(return_type)),
            None => None,
        }
    }

    /// Resolve properties on string type.
    pub(crate) fn resolve_string_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::String, TypeId::STRING, prop_name, prop_atom)
    }

    /// Resolve properties on number type.
    pub(crate) fn resolve_number_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Number, TypeId::NUMBER, prop_name, prop_atom)
    }

    /// Resolve properties on boolean type.
    pub(crate) fn resolve_boolean_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_primitive_property(
            IntrinsicKind::Boolean,
            TypeId::BOOLEAN,
            prop_name,
            prop_atom,
        )
    }

    /// Resolve properties on bigint type.
    pub(crate) fn resolve_bigint_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Bigint, TypeId::BIGINT, prop_name, prop_atom)
    }

    /// Helper to resolve properties on primitive types.
    /// Extracted to reduce duplication across string/number/boolean/bigint property resolvers.
    fn resolve_primitive_property(
        &self,
        kind: IntrinsicKind,
        type_id: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        // STEP 1: Try to get the boxed interface type from the resolver (e.g. Number for number)
        // This allows us to use lib.d.ts definitions instead of just hardcoded lists
        if let Some(boxed_type) = self.db.get_boxed_type(kind) {
            // Resolve the property on the boxed interface type
            // This handles inheritance (e.g., String extends Object) automatically
            // and allows user-defined augmentations to lib.d.ts to work
            let result = self.resolve_property_access_inner(boxed_type, prop_name, Some(prop_atom));

            // If the property was found (or we got a definitive answer like IsUnknown), return it.
            // Only fall back if the property was NOT found on the boxed type.
            // This ensures that if the environment defines the interface but is incomplete
            // (e.g., during bootstrapping or partial lib loading), we still find the intrinsic methods.
            if !result.is_not_found() {
                return result;
            }
        }

        // STEP 2: Fallback to hardcoded apparent members (bootstrapping/no-lib behavior)
        self.resolve_apparent_property(kind, type_id, prop_name, prop_atom)
    }

    /// Resolve properties on symbol primitive type.
    pub(crate) fn resolve_symbol_primitive_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        if prop_name == "toString" || prop_name == "valueOf" {
            return PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                write_type: None,
                from_index_signature: false,
            };
        }

        self.resolve_apparent_property(IntrinsicKind::Symbol, TypeId::SYMBOL, prop_name, prop_atom)
    }

    /// Resolve properties on array type.
    ///
    /// Uses the Array<T> interface from lib.d.ts to resolve array methods.
    /// Falls back to numeric index signature for numeric property names.
    pub(crate) fn resolve_array_property(
        &self,
        array_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        let element_type = self.array_element_type(array_type);

        // Try to use the Array<T> interface from lib.d.ts
        let array_base = self.db.get_array_base_type();

        if let Some(array_base) = array_base {
            // Create TypeApplication: Array<element_type>
            // This triggers resolve_application_property which handles substitution correctly
            let app_type = self.interner().application(array_base, vec![element_type]);

            // Resolve property on the application type
            let result = self.resolve_property_access_inner(app_type, prop_name, Some(prop_atom));

            // If we found the property, simplify Application types back to arrays and return it
            if !result.is_not_found() {
                return self.simplify_array_application_in_result(result, array_base);
            }
        }

        // Handle numeric index access (e.g., arr[0], arr["0"])
        use crate::index_signatures::IndexSignatureResolver;
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name) {
            let element_or_undefined = self.element_type_with_undefined(element_type);
            return PropertyAccessResult::Success {
                type_id: element_or_undefined,
                write_type: None,
                from_index_signature: true,
            };
        }

        // Fall back to Object prototype properties (constructor, valueOf, hasOwnProperty, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return result;
        }

        // Property not found
        PropertyAccessResult::PropertyNotFound {
            type_id: array_type,
            property_name: prop_atom,
        }
    }

    /// Simplifies Array<T> Application types back to T[] array types in property access results.
    ///
    /// This is needed because when resolving properties on arrays like `.sort()`, `.map()`, etc.,
    /// the type system returns Application types like `Array<T>` which should be simplified to `T[]`
    /// to avoid exposing the full array interface structure in error messages.
    fn simplify_array_application_in_result(
        &self,
        result: PropertyAccessResult,
        array_base: TypeId,
    ) -> PropertyAccessResult {
        match result {
            PropertyAccessResult::Success {
                type_id,
                write_type,
                from_index_signature,
            } => {
                let simplified_type = self.simplify_array_application(type_id, array_base);
                let simplified_write =
                    write_type.map(|wt| self.simplify_array_application(wt, array_base));
                PropertyAccessResult::Success {
                    type_id: simplified_type,
                    write_type: simplified_write,
                    from_index_signature,
                }
            }
            other => other,
        }
    }

    /// Recursively simplifies Array<T> Application types to T[] array types.
    fn simplify_array_application(&self, type_id: TypeId, array_base: TypeId) -> TypeId {
        match self.interner().lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner().type_application(app_id);
                // Check if this is Array<T>
                if app.base == array_base && app.args.len() == 1 {
                    // Simplify Array<T> to T[]
                    return self.interner().array(app.args[0]);
                }
                // Not an array application, return as-is
                type_id
            }
            Some(TypeData::Callable(callable_id)) => {
                // Simplify function return types
                let shape = self.interner().callable_shape(callable_id);
                let mut simplified_call_sigs = Vec::new();
                let mut simplified_construct_sigs = Vec::new();
                let mut changed = false;

                // Simplify call signatures
                for sig in &shape.call_signatures {
                    let simplified_return =
                        self.simplify_array_application(sig.return_type, array_base);
                    if simplified_return != sig.return_type {
                        changed = true;
                        let mut new_sig = sig.clone();
                        new_sig.return_type = simplified_return;
                        simplified_call_sigs.push(new_sig);
                    } else {
                        simplified_call_sigs.push(sig.clone());
                    }
                }

                // Simplify construct signatures
                for sig in &shape.construct_signatures {
                    let simplified_return =
                        self.simplify_array_application(sig.return_type, array_base);
                    if simplified_return != sig.return_type {
                        changed = true;
                        let mut new_sig = sig.clone();
                        new_sig.return_type = simplified_return;
                        simplified_construct_sigs.push(new_sig);
                    } else {
                        simplified_construct_sigs.push(sig.clone());
                    }
                }

                if changed {
                    let mut new_shape = (*shape).clone();
                    new_shape.call_signatures = simplified_call_sigs;
                    new_shape.construct_signatures = simplified_construct_sigs;
                    self.interner().callable(new_shape)
                } else {
                    type_id
                }
            }
            Some(TypeData::Union(list_id)) => {
                // Simplify union members
                let members = self.interner().type_list(list_id);
                let simplified_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.simplify_array_application(m, array_base))
                    .collect();

                // Check if any member changed
                if simplified_members
                    .iter()
                    .zip(members.iter())
                    .any(|(s, o)| s != o)
                {
                    self.interner().union(simplified_members)
                } else {
                    type_id
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                // Simplify intersection members
                let members = self.interner().type_list(list_id);
                let simplified_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.simplify_array_application(m, array_base))
                    .collect();

                // Check if any member changed
                if simplified_members
                    .iter()
                    .zip(members.iter())
                    .any(|(s, o)| s != o)
                {
                    self.interner().intersection(simplified_members)
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    pub(crate) fn array_element_type(&self, array_type: TypeId) -> TypeId {
        match self.interner().lookup(array_type) {
            Some(TypeData::Array(elem)) => elem,
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                self.tuple_element_union(&elements)
            }
            _ => TypeId::ERROR, // Return ERROR instead of ANY for non-array/tuple types
        }
    }

    fn tuple_element_union(&self, elements: &[TupleElement]) -> TypeId {
        let mut members = Vec::new();
        for elem in elements {
            let mut ty = if elem.rest {
                self.array_element_type(elem.type_id)
            } else {
                elem.type_id
            };
            if elem.optional {
                ty = self.element_type_with_undefined(ty);
            }
            members.push(ty);
        }
        self.interner().union(members)
    }

    fn element_type_with_undefined(&self, element_type: TypeId) -> TypeId {
        self.interner().union2(element_type, TypeId::UNDEFINED)
    }

    pub(super) fn resolve_function_property(
        &self,
        func_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match prop_name {
            "apply" | "call" | "bind" => self.method_result(TypeId::ANY),
            "toString" => self.method_result(TypeId::STRING),
            "name" => PropertyAccessResult::Success {
                type_id: TypeId::STRING,
                write_type: None,
                from_index_signature: false,
            },
            "length" => PropertyAccessResult::Success {
                type_id: TypeId::NUMBER,
                write_type: None,
                from_index_signature: false,
            },
            "prototype" | "arguments" => PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                write_type: None,
                from_index_signature: false,
            },
            "caller" => PropertyAccessResult::Success {
                type_id: self.any_args_function(TypeId::ANY),
                write_type: None,
                from_index_signature: false,
            },
            _ => {
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                PropertyAccessResult::PropertyNotFound {
                    type_id: func_type,
                    property_name: prop_atom,
                }
            }
        }
    }
}
