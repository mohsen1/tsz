//! Structural type matching for inference.
//!
//! This module implements the structural type-walking algorithm that collects
//! inference candidates by recursing into type shapes (objects, functions,
//! tuples, unions, intersections, template literals, etc.).
//!
//! It is the core of `infer_from_types`: given a source type and a target type
//! containing type parameters, it walks both structures in parallel and records
//! lower/upper bound candidates for each inference variable.

use crate::def::DefId;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_generic, instantiate_type};
use crate::relations::variance::compute_type_param_variances_with_resolver;
use crate::types::{
    CallSignature, CallableShapeId, FunctionShape, FunctionShapeId, InferencePriority,
    IntrinsicKind, LiteralValue, MappedTypeId, ObjectShapeId, ParamInfo, TemplateLiteralId,
    TemplateSpan, TupleElement, TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId,
    Variance,
};
use tsz_common::interner::Atom;

use super::infer::{InferenceContext, InferenceError, InferenceVar};

impl<'a> InferenceContext<'a> {
    /// Perform structural type inference from a source type to a target type.
    ///
    /// This is the core algorithm for inferring type parameters from function arguments.
    /// It walks the structure of both types, collecting constraints for type parameters.
    ///
    /// # Arguments
    /// * `source` - The type from the value argument (e.g., `string` from `identity("hello")`)
    /// * `target` - The type from the parameter (e.g., `T` from `function identity<T>(x: T)`)
    /// * `priority` - The inference priority (e.g., `NakedTypeVariable` for direct arguments)
    ///
    /// # Type Inference Algorithm
    ///
    /// TypeScript uses structural type inference with the following rules:
    ///
    /// 1. **Direct Parameter Match**: If target is a type parameter `T` we're inferring,
    ///    add source as a lower bound candidate for `T`.
    ///
    /// 2. **Structural Recursion**: For complex types, recurse into the structure:
    ///    - Objects: Match properties recursively
    ///    - Arrays: Match element types
    ///    - Functions: Match parameters (contravariant) and return types (covariant)
    ///
    /// 3. **Variance Handling**:
    ///    - Covariant positions (properties, arrays, return types): `infer(source, target)`
    ///    - Contravariant positions (function parameters): `infer(target, source)` (swapped!)
    ///
    /// # Example
    /// ```text
    /// let mut ctx = InferenceContext::new(&interner);
    /// let t_var = ctx.fresh_type_param(interner.intern_string("T"), false);
    ///
    /// // Inference: identity("hello") should infer T = string
    /// ctx.infer_from_types(string_type, t_type, InferencePriority::NakedTypeVariable)?;
    /// ```
    pub fn infer_from_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        // Resolve the types to their actual TypeDatas
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        // Block inference if target is NoInfer<T> (TypeScript 5.4+)
        // NoInfer prevents inference from flowing through this type position
        if let Some(TypeData::NoInfer(_)) = target_key {
            return Ok(()); // Stop inference - don't descend into NoInfer
        }

        // Unwrap NoInfer from source if present (rare but possible)
        let source_key = if let Some(TypeData::NoInfer(inner)) = source_key {
            self.interner.lookup(inner)
        } else {
            source_key
        };

        // Case 1: Target is a TypeParameter we're inferring (Lower Bound: source <: T)
        if let Some(TypeData::TypeParameter(ref param_info)) = target_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // Add source as a lower bound candidate for this type parameter
            self.add_candidate(var, source, priority);
            return Ok(());
        }

        // Case 2: Source is a TypeParameter we're inferring (Upper Bound: T <: target)
        // CRITICAL: This handles contravariance! When function parameters are swapped,
        // the TypeParameter moves to source position and becomes an upper bound.
        if let Some(TypeData::TypeParameter(ref param_info)) = source_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // When in contra_mode (function parameter inference), add as a contra-candidate
            // instead of an upper bound. This matches tsc's behavior where inference from
            // function parameter types (contravariant position) produces contra-candidates
            // that are resolved via intersection/most-specific, NOT hard upper bounds that
            // must each individually be satisfied. Without this, decomposing a union target
            // (e.g., matching {kind: T} against {kind:'a'} | {kind:'b'}) produces separate
            // upper bounds 'a' and 'b', causing false TS2345 errors.
            if self.in_contra_mode {
                self.add_contra_candidate(var, target, priority);
            } else {
                // T <: target, so target is an UPPER bound
                self.add_upper_bound(var, target);
            }
            return Ok(());
        }

        // Resolve Lazy types before structural dispatch. Lazy(DefId) types are
        // opaque references that the inference engine can't match structurally.
        // Resolve them to their underlying types so inference can see the structure.
        if let Some(TypeData::Lazy(def_id)) = source_key
            && let Some(resolved) = self.resolve_lazy_for_inference(def_id, source)
            && resolved != source
        {
            return self.infer_from_types(resolved, target, priority);
        }
        if let Some(TypeData::Lazy(def_id)) = target_key
            && let Some(resolved) = self.resolve_lazy_for_inference(def_id, target)
            && resolved != target
        {
            return self.infer_from_types(source, resolved, priority);
        }

        // Case 3: Structural recursion - match based on type structure
        match (source_key, target_key) {
            // Object types: recurse into properties
            (
                Some(TypeData::Object(source_shape) | TypeData::ObjectWithIndex(source_shape)),
                Some(TypeData::Object(target_shape) | TypeData::ObjectWithIndex(target_shape)),
            ) => {
                self.infer_objects(source_shape, target_shape, priority)?;
            }

            // Function types: handle variance (parameters are contravariant, return is covariant)
            (Some(TypeData::Function(source_func)), Some(TypeData::Function(target_func))) => {
                self.infer_functions(source_func, target_func, priority)?;
            }

            // Callable types: infer across signatures and properties
            (Some(TypeData::Callable(source_call)), Some(TypeData::Callable(target_call))) => {
                self.infer_callables(source_call, target_call, priority)?;
            }

            // Cross-type Function ↔ Callable inference: when a Function needs to be
            // inferred against a Callable's call signature (or vice versa), bridge them
            // by matching the function shape against the callable's last call signature.
            (Some(TypeData::Function(source_func)), Some(TypeData::Callable(target_call))) => {
                let target = self.interner.callable_shape(target_call);
                if let Some(target_sig) = target.call_signatures.last() {
                    self.infer_function_vs_signature(source_func, target_sig, priority)?;
                }
            }
            (Some(TypeData::Callable(source_call)), Some(TypeData::Function(target_func))) => {
                let source = self.interner.callable_shape(source_call);
                if let Some(source_sig) = source.call_signatures.last() {
                    self.infer_signature_vs_function(source_sig, target_func, priority)?;
                }
            }

            // Array types: recurse into element types
            (Some(TypeData::Array(source_elem)), Some(TypeData::Array(target_elem))) => {
                self.infer_from_types(source_elem, target_elem, priority)?;
            }

            // Tuple types: recurse into elements
            (Some(TypeData::Tuple(source_elems)), Some(TypeData::Tuple(target_elems))) => {
                self.infer_tuples(source_elems, target_elems, priority)?;
            }

            // Union types: try to infer against each member
            (Some(TypeData::Union(source_members)), Some(TypeData::Union(target_members))) => {
                self.infer_unions(source_members, target_members, priority)?;
            }

            // Intersection types: both source and target are intersections
            (
                Some(TypeData::Intersection(source_members)),
                Some(TypeData::Intersection(target_members)),
            ) => {
                self.infer_intersections(source_members, target_members, priority)?;
            }

            // Target is a union/intersection but source is not: decompose the target
            // and infer against each member. This handles cases like:
            //   source: {store: string}  target: {dispatch: number} & OwnProps
            // We try each intersection member so that type parameters within the
            // target (like OwnProps, or union branches) can be inferred from the source.
            (_, Some(TypeData::Union(target_members) | TypeData::Intersection(target_members))) => {
                let target_list = self.interner.type_list(target_members);
                for &target_member in target_list.iter() {
                    let _ = self.infer_from_types(source, target_member, priority);
                }
            }

            // Source is an intersection but target is not: try inferring from
            // each source member against the target.
            (Some(TypeData::Intersection(source_members)), _) => {
                let source_list = self.interner.type_list(source_members);
                for &source_member in source_list.iter() {
                    let _ = self.infer_from_types(source_member, target, priority);
                }
            }

            // TypeApplication: recurse into instantiated type
            (Some(TypeData::Application(source_app)), Some(TypeData::Application(target_app))) => {
                self.infer_applications(source, source_app, target_app, priority)?;
            }

            // Index access types: infer both object and index types
            (
                Some(TypeData::IndexAccess(source_obj, source_idx)),
                Some(TypeData::IndexAccess(target_obj, target_idx)),
            ) => {
                self.infer_from_types(source_obj, target_obj, priority)?;
                self.infer_from_types(source_idx, target_idx, priority)?;
            }

            // Preserve structure through keyof when inferring mapped/apparent relations.
            // Without this, `<T>(x: { [K in keyof T]: T[K] })` matched against a
            // concrete mapped type over `keyof U` falls back to `T = unknown`,
            // which makes later assignability too permissive.
            // KeyOf and ReadonlyType: unwrap structural wrappers and infer inner types
            (Some(TypeData::KeyOf(source_inner)), Some(TypeData::KeyOf(target_inner)))
            | (
                Some(TypeData::ReadonlyType(source_inner)),
                Some(TypeData::ReadonlyType(target_inner)),
            ) => {
                self.infer_from_types(source_inner, target_inner, priority)?;
            }

            // Mapped-to-mapped inference: walk both the key space and the template
            // so generic source parameters can retain the target's apparent-member
            // constraint. We intentionally do NOT infer from the templates here:
            // inferring `T` from `T[K]` against `Obj[K]` would incorrectly add
            // `T <: Obj`, collapsing array-constrained sources to plain objects.
            (Some(TypeData::Mapped(source_mapped)), Some(TypeData::Mapped(target_mapped))) => {
                let source_mapped = self.interner.mapped_type(source_mapped);
                let target_mapped = self.interner.mapped_type(target_mapped);

                self.infer_from_types(
                    source_mapped.constraint,
                    target_mapped.constraint,
                    priority,
                )?;
            }

            // Unwrap ReadonlyType when only target is readonly (mutable source is compatible)
            (_, Some(TypeData::ReadonlyType(target_inner))) => {
                self.infer_from_types(source, target_inner, priority)?;
            }

            // Task #40: Template literal deconstruction for infer patterns
            // Handles: source extends `prefix${infer T}suffix` ? true : false
            (Some(source_key), Some(TypeData::TemplateLiteral(target_id))) => {
                self.infer_from_template_literal(source, Some(&source_key), target_id, priority)?;
            }

            // Mapped type inference: infer from object properties against mapped type
            // Handles: source { a: string, b: number } against target { [P in K]: T }
            // Infers K from property names and T from property value types
            (
                Some(TypeData::Object(source_shape) | TypeData::ObjectWithIndex(source_shape)),
                Some(TypeData::Mapped(mapped_id)),
            ) => {
                self.infer_from_mapped_type(source_shape, mapped_id, priority)?;
            }

            // Tuple against mapped type: reverse-mapped inference from tuple elements.
            // Handles: source [Wrap<string>, Wrap<number>] against
            //   target { [K in keyof Tuple]: Wrap<Tuple[K]> }
            // Infers Tuple from the tuple elements by substituting numeric keys
            // into the template and inferring each element type.
            (Some(TypeData::Tuple(source_elems)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type_tuple(source_elems, mapped_id, priority)?;
            }

            // Array against mapped type: infer element type against mapped template.
            // Handles: source Wrap<string>[] against
            //   target { [K in keyof Arr]: Wrap<Arr[K]> }
            (Some(TypeData::Array(source_elem)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type_array(source_elem, mapped_id, priority)?;
            }

            // TypeApplication target: expand type alias and recurse.
            // This handles cases like `Spec<T[P]>` where Spec is a mapped type alias.
            // Without expansion, inference against recursive type alias applications
            // silently fails (e.g., `{ [P in keyof T]: Func<T[P]> | Spec<T[P]> }`).
            (_, Some(TypeData::Application(target_app_id))) => {
                if let Some(expanded) = self.try_expand_application(target_app_id) {
                    self.infer_from_types(source, expanded, priority)?;
                }
            }

            // If we can't match structurally, that's okay - it might mean the types are incompatible
            // The Checker will handle this with proper error reporting
            _ => {
                // No structural match possible
                // This is not an error - the Checker will verify assignability separately
            }
        }

        Ok(())
    }

    /// Infer from object types by matching properties
    fn infer_objects(
        &mut self,
        source_shape: ObjectShapeId,
        target_shape: ObjectShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_shape = self.interner.object_shape(source_shape);
        let target_shape = self.interner.object_shape(target_shape);

        // For each property in the target, try to find a matching property in the source
        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                // Use partially inferable type to prevent implicit `any` from
                // flowing contravariantly into inference. Matches tsc behavior.
                let inferable_type = self.get_partially_inferable_type(source_prop.type_id);
                self.infer_from_types(inferable_type, target_prop.type_id, priority)?;
            }
        }

        // Also check index signatures for inference
        // If target has a string index signature, infer from source's string index
        if let Some(target_string_idx) = &target_shape.string_index {
            if let Some(source_string_idx) = &source_shape.string_index {
                self.infer_from_types(
                    source_string_idx.value_type,
                    target_string_idx.value_type,
                    priority,
                )?;
            } else {
                // Source has no explicit string index. Collect contributions from:
                // 1. Number index (JS converts numeric keys to strings, so a number
                //    index implies string-indexability)
                // 2. Named properties (implicit index signature)
                //
                // This matches tsc's behavior where `typeof E1` (numeric enum with
                // number index + named members) infers T from all value types when
                // matched against `{ [x: string]: T }`.
                let mut implicit_parts: Vec<TypeId> = Vec::new();

                // Contribution from number index: in JS, numeric keys are converted
                // to strings, so a source number index contributes to string index
                // inference. E.g., enum namespace `typeof E1` has `[n: number]: string`
                // for reverse mappings.
                if let Some(s_number_idx) = &source_shape.number_index {
                    implicit_parts.push(s_number_idx.value_type);
                }

                // Contribution from named properties (implicit index signature).
                // For anonymous object types (no symbol), all property values
                // contribute. For enum namespace types (ENUM_NAMESPACE flag), named
                // properties also contribute — tsc treats enum namespaces as
                // having implicit string index signatures derived from their members.
                //
                // Named class/interface instance types are excluded — they must
                // declare an explicit index signature.
                if !source_shape.properties.is_empty() {
                    let has_implicit_index = source_shape.symbol.is_none()
                        || source_shape
                            .flags
                            .contains(crate::types::ObjectFlags::ENUM_NAMESPACE);
                    if has_implicit_index {
                        for p in &source_shape.properties {
                            // For optional properties, strip `undefined` from optionality.
                            // tsc: `{ a: string, b?: number }` infers T as `string | number`
                            // (not `string | number | undefined`).
                            let prop_type = if p.optional {
                                crate::narrowing::utils::remove_undefined(self.interner, p.type_id)
                            } else {
                                p.type_id
                            };
                            implicit_parts.push(prop_type);
                        }
                    }
                }

                if !implicit_parts.is_empty() {
                    let implicit_index_type = if implicit_parts.len() == 1 {
                        implicit_parts[0]
                    } else {
                        self.interner.union(implicit_parts)
                    };
                    self.infer_from_types(
                        implicit_index_type,
                        target_string_idx.value_type,
                        priority,
                    )?;
                }
            }
        }

        // If target has a number index signature, infer from source's number index
        if let Some(target_number_idx) = &target_shape.number_index {
            if let Some(source_number_idx) = &source_shape.number_index {
                self.infer_from_types(
                    source_number_idx.value_type,
                    target_number_idx.value_type,
                    priority,
                )?;
            } else if !source_shape.properties.is_empty() {
                // Implicit number index: collect types of numeric-named properties.
                // Same rule as string index: allow anonymous types and enum namespaces.
                let has_implicit_index = source_shape.symbol.is_none()
                    || source_shape
                        .flags
                        .contains(crate::types::ObjectFlags::ENUM_NAMESPACE);
                if has_implicit_index {
                    let numeric_types: Vec<TypeId> = source_shape
                        .properties
                        .iter()
                        .filter(|p| crate::utils::is_numeric_property_name(self.interner, p.name))
                        .map(|p| p.type_id)
                        .collect();
                    if !numeric_types.is_empty() {
                        let implicit_index_type = if numeric_types.len() == 1 {
                            numeric_types[0]
                        } else {
                            self.interner.union(numeric_types)
                        };
                        self.infer_from_types(
                            implicit_index_type,
                            target_number_idx.value_type,
                            priority,
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Infer type arguments from an object type matched against a mapped type.
    ///
    /// When source is `{ a: string, b: number }` and target is `{ [P in K]: T }`:
    /// - Infer K from the union of source property name literals ("a" | "b")
    /// - Infer T from each source property value type against the mapped template
    fn infer_from_mapped_type(
        &mut self,
        source_shape: ObjectShapeId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);
        let source = self.interner.object_shape(source_shape);

        if !source.properties.is_empty() {
            // Infer the constraint type (K) from the union of source property names
            // e.g., for { foo: string, bar: number }, K = "foo" | "bar"
            let name_literals: Vec<TypeId> = source
                .properties
                .iter()
                .map(|p| self.interner.literal_string_atom(p.name))
                .collect();
            let names_union = if name_literals.len() == 1 {
                name_literals[0]
            } else {
                self.interner.union(name_literals)
            };
            self.infer_from_types(names_union, mapped.constraint, priority)?;

            // Infer the template type (T) from each source property value type.
            // Use MappedType priority so candidates are combined via union (not
            // common supertype). This matches tsc's PriorityImpliesCombination
            // which includes MappedTypeConstraint: when multiple properties each
            // contribute a different type for T, the result should be their union
            // (e.g., Box<number> | Box<string> | Box<boolean>), not a single "best" type.
            let template_priority = InferencePriority::MappedType;
            for prop in &source.properties {
                let key_literal = self.interner.literal_string_atom(prop.name);
                let mut subst = TypeSubstitution::new();
                subst.insert(mapped.type_param.name, key_literal);
                let instantiated_template =
                    instantiate_type(self.interner, mapped.template, &subst);
                // Use the partially inferable version of the source property type.
                // This replaces implicit `any` parameters in function types with
                // `unknown`, preventing them from flowing contravariantly into
                // inference candidates. Matches tsc's getPartiallyInferableType.
                let inferable_prop_type = self.get_partially_inferable_type(prop.type_id);
                self.infer_from_types(
                    inferable_prop_type,
                    instantiated_template,
                    template_priority,
                )?;
            }
        } else if let Some(ref string_index) = source.string_index {
            // Source has no named properties but has a string index signature
            // (e.g., `{ [index: string]: number }`). Infer K from `string`
            // and V from the index signature value type.
            self.infer_from_types(TypeId::STRING, mapped.constraint, priority)?;
            self.infer_from_types(
                string_index.value_type,
                mapped.template,
                InferencePriority::MappedType,
            )?;
        } else if let Some(ref number_index) = source.number_index {
            // Source has a number index signature (e.g., `{ [index: number]: V }`).
            self.infer_from_types(TypeId::NUMBER, mapped.constraint, priority)?;
            self.infer_from_types(
                number_index.value_type,
                mapped.template,
                InferencePriority::MappedType,
            )?;
        }

        Ok(())
    }

    /// Infer type parameters from a tuple source against a mapped type target.
    ///
    /// For a mapped type `{ [K in keyof T]: Template<T[K]> }` and a source tuple
    /// `[Wrap<string>, Wrap<number>]`, this:
    /// 1. Substitutes K with "0", "1", etc. in the template
    /// 2. Infers each element type against the instantiated template
    ///
    /// This matches tsc's `inferFromMappedType` which handles both object and
    /// tuple sources against mapped types.
    fn infer_from_mapped_type_tuple(
        &mut self,
        source_elems: TupleListId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);
        let source_elems = self.interner.tuple_list(source_elems);
        if source_elems.is_empty() {
            return Ok(());
        }

        let iter_param_name = mapped.type_param.name;

        // Infer the constraint type from numeric key literals
        // e.g., for [a, b, c], K = "0" | "1" | "2"
        let name_literals: Vec<TypeId> = (0..source_elems.len())
            .map(|i| {
                let key_str = i.to_string();
                let key_atom = self.interner.intern_string(&key_str);
                self.interner.literal_string_atom(key_atom)
            })
            .collect();
        let names_union = if name_literals.len() == 1 {
            name_literals[0]
        } else {
            self.interner.union(name_literals.clone())
        };
        self.infer_from_types(names_union, mapped.constraint, priority)?;

        // Infer the template type from each tuple element
        let template_priority = InferencePriority::MappedType;
        for (i, elem) in source_elems.iter().enumerate() {
            let key_literal = name_literals[i];
            let mut subst = TypeSubstitution::new();
            subst.insert(iter_param_name, key_literal);
            let instantiated_template = instantiate_type(self.interner, mapped.template, &subst);
            let inferable_elem_type = self.get_partially_inferable_type(elem.type_id);
            self.infer_from_types(
                inferable_elem_type,
                instantiated_template,
                template_priority,
            )?;
        }

        Ok(())
    }

    /// Infer type parameters from an array source against a mapped type target.
    ///
    /// For a mapped type `{ [K in keyof T]: Template<T[K]> }` and a source array
    /// `Wrap<string>[]`, this infers from the array element type against the template
    /// using `number` as the key type.
    fn infer_from_mapped_type_array(
        &mut self,
        source_elem: TypeId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);

        // Infer the constraint from `number` (array index type)
        self.infer_from_types(TypeId::NUMBER, mapped.constraint, priority)?;

        // Infer the template from the element type
        self.infer_from_types(source_elem, mapped.template, InferencePriority::MappedType)?;

        Ok(())
    }

    /// Resolve a `Lazy(DefId)` type for inference purposes.
    ///
    /// Returns the resolved type if available, or `None` if the resolver isn't present
    /// or the `DefId` can't be resolved.
    fn resolve_lazy_for_inference(&self, def_id: DefId, _original: TypeId) -> Option<TypeId> {
        let resolver = self.resolver?;
        resolver.resolve_lazy(def_id, self.interner)
    }

    /// Try to expand a `TypeApplication` target into its instantiated body.
    ///
    /// For type aliases like `type Spec<T> = { [P in keyof T]: ... }`, this expands
    /// `Spec<SomeArg>` into the substituted mapped type body, enabling structural
    /// inference to proceed. Without this, `(Object, Application)` falls through
    /// the match and inference candidates are lost.
    ///
    /// Returns `None` if:
    /// - No resolver is available
    /// - The base isn't a resolvable DefId
    /// - Type parameters or body can't be resolved
    /// - Application expansion depth limit is exceeded (prevents infinite recursion
    ///   for recursive type aliases)
    fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        let resolver = self.resolver?;
        let app = self.interner.type_application(app_id);

        // Extract DefId from the base type (must be Lazy(DefId))
        let def_id = match self.interner.lookup(app.base)? {
            TypeData::Lazy(def_id) => def_id,
            _ => return None,
        };

        // Depth guard: prevent infinite recursion for recursive type aliases
        // (e.g., type Spec<T> = { [P in keyof T]: Spec<T[P]> })
        let depth = self.app_expansion_depth;
        if depth >= Self::MAX_APP_EXPANSION_DEPTH {
            return None;
        }
        self.app_expansion_depth += 1;

        // Resolve the type alias body and its type parameters
        let type_params = resolver.get_lazy_type_params(def_id)?;
        let body = resolver.resolve_lazy(def_id, self.interner)?;

        // Instantiate the body with the application's type arguments
        let instantiated = instantiate_generic(self.interner, body, &type_params, &app.args);

        // Restore depth after expansion
        self.app_expansion_depth = depth;

        Some(instantiated)
    }

    /// Compute the variances of each type parameter for a type application's base type.
    ///
    /// Given a base type (e.g., the `Func1` in `Func1<T>`), this resolves the DefId
    /// and computes how each type parameter is used (covariantly, contravariantly, etc.).
    /// Returns `None` if no resolver is available or the base isn't a resolvable definition.
    fn compute_application_variances(&self, base: TypeId) -> Option<std::sync::Arc<[Variance]>> {
        let resolver = self.resolver?;
        let def_id = match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => def_id,
            _ => return None,
        };
        compute_type_param_variances_with_resolver(self.interner, resolver, def_id)
    }

    /// Infer from function types, handling variance correctly
    fn infer_functions(
        &mut self,
        source_func: FunctionShapeId,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_sig = self.interner.function_shape(source_func);
        let target_sig = self.interner.function_shape(target_func);

        tracing::trace!(
            source_params = source_sig.params.len(),
            target_params = target_sig.params.len(),
            "infer_functions called"
        );

        // Parameters are contravariant: swap source and target.
        // Set in_contra_mode so that type parameters found in the source position
        // (after direction swap) are recorded as contra-candidates rather than hard
        // upper bounds. This matches tsc's handling of function parameter inference.
        let was_contra = self.in_contra_mode;
        self.in_contra_mode = true;
        let mut source_params = source_sig.params.iter().peekable();
        let mut target_params = target_sig.params.iter().peekable();

        loop {
            let source_rest = source_params.peek().is_some_and(|p| p.rest);
            let target_rest = target_params.peek().is_some_and(|p| p.rest);

            tracing::trace!(
                source_rest,
                target_rest,
                "Checking rest params in loop iteration"
            );

            // If both have rest params, infer the rest element types
            if source_rest && target_rest {
                let source_param = source_params
                    .next()
                    .expect("source_rest flag guarantees next element");
                let target_param = target_params
                    .next()
                    .expect("target_rest flag guarantees next element");
                self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                break;
            }

            // If source has rest param, infer all remaining target params into it
            if source_rest {
                let source_param = source_params
                    .next()
                    .expect("source_rest flag guarantees next element");
                for target_param in target_params.by_ref() {
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                break;
            }

            // If target has rest param, infer all remaining source params into it
            if target_rest {
                let target_param = target_params
                    .next()
                    .expect("target_rest flag guarantees next element");

                // CRITICAL: Check if target rest param is a type parameter (like A extends any[])
                // If so, we need to infer it as a TUPLE of all remaining source params,
                // not as individual param types.
                //
                // Example: wrap<A extends any[], R>(fn: (...args: A) => R)
                //          with add(a: number, b: number): number
                //          should infer A = [number, number], not A = number
                let target_is_type_param = matches!(
                    self.interner.lookup(target_param.type_id),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                );

                tracing::trace!(
                    target_is_type_param,
                    target_param_type = ?target_param.type_id,
                    "Rest parameter inference - target is type param check"
                );

                if target_is_type_param {
                    // Collect all remaining source params into a tuple
                    let mut tuple_elements = Vec::new();
                    for source_param in source_params.by_ref() {
                        tuple_elements.push(TupleElement {
                            type_id: source_param.type_id,
                            name: source_param.name,
                            optional: source_param.optional,
                            rest: source_param.rest,
                        });
                    }

                    tracing::trace!(
                        num_elements = tuple_elements.len(),
                        "Collected source params into tuple"
                    );

                    // Infer the tuple type against the type parameter
                    // Note: Parameters are contravariant, so target comes first
                    if !tuple_elements.is_empty() {
                        let tuple_type = self.interner.tuple(tuple_elements);
                        tracing::trace!(
                            tuple_type = ?tuple_type,
                            target_param = ?target_param.type_id,
                            "Inferring tuple against type parameter"
                        );
                        self.infer_from_types(target_param.type_id, tuple_type, priority)?;
                    }
                } else {
                    // Target rest param is not a type parameter (e.g., number[] or Array<string>)
                    // Infer each source param individually against the rest element type
                    for source_param in source_params.by_ref() {
                        self.infer_from_types(
                            target_param.type_id,
                            source_param.type_id,
                            priority,
                        )?;
                    }
                }
                break;
            }

            // Neither has rest param, do normal pairwise comparison
            match (source_params.next(), target_params.next()) {
                (Some(source_param), Some(target_param)) => {
                    // Note the swapped arguments! This is the key to handling contravariance.
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                _ => break, // Mismatch in arity - stop here
            }
        }

        // Restore contra mode before covariant inference (return type, type predicates).
        self.in_contra_mode = was_contra;

        // Return type is covariant: normal order
        self.infer_from_types(source_sig.return_type, target_sig.return_type, priority)?;

        // This type is contravariant
        if let (Some(source_this), Some(target_this)) = (source_sig.this_type, target_sig.this_type)
        {
            let was_contra2 = self.in_contra_mode;
            self.in_contra_mode = true;
            self.infer_from_types(target_this, source_this, priority)?;
            self.in_contra_mode = was_contra2;
        }

        // Type predicates are covariant
        if let (Some(source_pred), Some(target_pred)) =
            (&source_sig.type_predicate, &target_sig.type_predicate)
        {
            // Compare targets by index if possible
            let targets_match = match (source_pred.parameter_index, target_pred.parameter_index) {
                (Some(s_idx), Some(t_idx)) => s_idx == t_idx,
                _ => source_pred.target == target_pred.target,
            };

            tracing::trace!(
                targets_match,
                ?source_pred.parameter_index,
                ?target_pred.parameter_index,
                "Inferring from type predicates"
            );

            if targets_match
                && source_pred.asserts == target_pred.asserts
                && let (Some(source_ty), Some(target_ty)) =
                    (source_pred.type_id, target_pred.type_id)
            {
                self.infer_from_types(source_ty, target_ty, priority)?;
            }
        }

        Ok(())
    }

    /// Infer from tuple types
    fn infer_tuples(
        &mut self,
        source_elems: TupleListId,
        target_elems: TupleListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.tuple_list(source_elems);
        let target_list = self.interner.tuple_list(target_elems);

        for (source_elem, target_elem) in source_list.iter().zip(target_list.iter()) {
            self.infer_from_types(source_elem.type_id, target_elem.type_id, priority)?;
        }

        Ok(())
    }

    /// Infer from callable types, handling signatures and properties
    fn infer_callables(
        &mut self,
        source_id: CallableShapeId,
        target_id: CallableShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source = self.interner.callable_shape(source_id);
        let target = self.interner.callable_shape(target_id);

        // For each call signature in the target, try to find a compatible one in the source
        for target_sig in &target.call_signatures {
            for source_sig in &source.call_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
                    }
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // For each construct signature
        for target_sig in &target.construct_signatures {
            for source_sig in &source.construct_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
                    }
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // Properties
        for target_prop in &target.properties {
            if let Some(source_prop) = source
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                self.infer_from_types(source_prop.type_id, target_prop.type_id, priority)?;
            }
        }

        // String index
        if let (Some(target_idx), Some(source_idx)) = (&target.string_index, &source.string_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        // Number index
        if let (Some(target_idx), Some(source_idx)) = (&target.number_index, &source.number_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        Ok(())
    }

    /// Infer from a Function shape against a Callable's call signature.
    /// Bridges Function ↔ Callable for cross-type inference.
    fn infer_function_vs_signature(
        &mut self,
        source_func: FunctionShapeId,
        target_sig: &crate::types::CallSignature,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source = self.interner.function_shape(source_func);
        // Parameters are contravariant
        for (s_param, t_param) in source.params.iter().zip(target_sig.params.iter()) {
            self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
        }
        // Return type is covariant
        self.infer_from_types(source.return_type, target_sig.return_type, priority)?;
        Ok(())
    }

    /// Infer from a Callable's call signature against a Function shape.
    /// Bridges Callable → Function for cross-type inference.
    fn infer_signature_vs_function(
        &mut self,
        source_sig: &crate::types::CallSignature,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let target = self.interner.function_shape(target_func);
        // Parameters are contravariant
        for (s_param, t_param) in source_sig.params.iter().zip(target.params.iter()) {
            self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
        }
        // Return type is covariant
        self.infer_from_types(source_sig.return_type, target.return_type, priority)?;
        Ok(())
    }

    /// Infer from union types
    ///
    /// Implements TSC's union-to-union inference strategy:
    /// 1. Partition target members into parameterized (contains inference vars) and fixed.
    /// 2. Further split parameterized into naked type params vs structured (e.g., `Foo<V>`).
    /// 3. Filter out source members that match fixed targets.
    /// 4. For remaining source members, prefer structural matches over naked type params.
    fn infer_unions(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // Resolve Lazy types in target members and flatten any unions they resolve to.
        // This is critical for type aliases used in union targets: e.g., `T | Primitive`
        // where `Primitive = number | string | boolean | Date` must be flattened so that
        // fixed member matching can properly skip source members like `number` or `string`.
        let resolved_targets = self.resolve_and_flatten_union_members(&target_list);

        // Similarly resolve source members so they can match resolved fixed targets.
        let resolved_sources = self.resolve_and_flatten_union_members(&source_list);

        let (parameterized, fixed): (Vec<TypeId>, Vec<TypeId>) = resolved_targets
            .iter()
            .copied()
            .partition(|&t| self.target_contains_inference_param(t));

        if parameterized.is_empty() {
            // No inference targets — nothing to infer
            return Ok(());
        }

        // Further split parameterized into naked type params vs structured
        let (_naked_params, structured_params): (Vec<TypeId>, Vec<TypeId>) = parameterized
            .iter()
            .partition(|&&t| matches!(self.interner.lookup(t), Some(TypeData::TypeParameter(_))));

        for &source_ty in resolved_sources.iter() {
            // Skip source members that match fixed targets
            if fixed.contains(&source_ty) {
                continue;
            }

            if !structured_params.is_empty() {
                // Check if this source member structurally matches any structured target
                let has_structural_match = structured_params
                    .iter()
                    .any(|&t| self.types_share_outer_structure(source_ty, t));

                if has_structural_match {
                    // Infer only against structurally matching targets, NOT naked type params.
                    // This prevents e.g. `Foo<U>` from being inferred against naked `V`
                    // when `Foo<V>` is available as a structural match.
                    for &target_ty in &structured_params {
                        if self.types_share_outer_structure(source_ty, target_ty) {
                            self.infer_from_types(source_ty, target_ty, priority)?;
                        }
                    }
                    continue;
                }
            }

            // No structural match found — infer against all parameterized targets
            // (including naked type params)
            for &target_ty in &parameterized {
                self.infer_from_types(source_ty, target_ty, priority)?;
            }
        }

        Ok(())
    }

    /// Resolve Lazy types in union members and flatten any unions they resolve to.
    ///
    /// When a union contains `Lazy(DefId)` members (e.g., type alias references like
    /// `Primitive` in `T | Primitive`), this resolves them and flattens the result.
    /// For example, if `Primitive = number | string | boolean | Date`, then:
    ///   `[T, Lazy(Primitive)]` → `[T, number, string, boolean, Date]`
    ///
    /// This is necessary for correct inference matching: without flattening,
    /// source members like `number` can't be matched against the opaque `Lazy(Primitive)`
    /// and incorrectly get inferred against type parameter `T`.
    fn resolve_and_flatten_union_members(&self, members: &[TypeId]) -> Vec<TypeId> {
        let mut result = Vec::with_capacity(members.len());
        for &member in members {
            if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(member)
                && let Some(resolved) = self.resolve_lazy_for_inference(def_id, member)
                && resolved != member
            {
                if let Some(TypeData::Union(inner_members)) = self.interner.lookup(resolved) {
                    // Flatten: the lazy resolved to a union, add its members
                    let inner = self.interner.type_list(inner_members);
                    result.extend(inner.iter().copied());
                    continue;
                }
                // Resolved to a non-union type, use the resolved form
                result.push(resolved);
                continue;
            }
            result.push(member);
        }
        result
    }

    /// Check if two types share the same outer structure (same kind / same generic base).
    ///
    /// Used to match source union members to the best target union member during
    /// inference. For example, `Foo<U>` and `Foo<V>` share outer structure (both
    /// are applications of `Foo`), but `U` and `Foo<V>` do not.
    fn types_share_outer_structure(&self, source: TypeId, target: TypeId) -> bool {
        let (Some(s_key), Some(t_key)) =
            (self.interner.lookup(source), self.interner.lookup(target))
        else {
            return false;
        };
        match (s_key, t_key) {
            // Both are applications of the same base type
            (TypeData::Application(s_app_id), TypeData::Application(t_app_id)) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                s_app.base == t_app.base
            }
            // Both share the same structural kind
            (TypeData::Object(_), TypeData::Object(_))
            | (TypeData::Callable(_), TypeData::Callable(_))
            | (TypeData::Function(_), TypeData::Function(_))
            | (TypeData::Tuple(_), TypeData::Tuple(_))
            | (TypeData::Array(_), TypeData::Array(_)) => true,
            _ => false,
        }
    }

    /// Check if a target type directly is or contains an inference type parameter.
    ///
    /// This must be recursive: for `V | Foo<V>`, both `V` (direct type param)
    /// and `Foo<V>` (application containing a type param) are parameterized.
    /// Without recursion, `Foo<V>` would be classified as "fixed", causing
    /// source members like `Foo<U>` to be inferred against the naked `V`
    /// instead of structurally matching `Foo<V>`.
    fn target_contains_inference_param(&self, target: TypeId) -> bool {
        self.target_contains_inference_param_inner(target, &mut std::collections::HashSet::new())
    }

    fn target_contains_inference_param_inner(
        &self,
        target: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(target) {
            return false;
        }
        let Some(key) = self.interner.lookup(target) else {
            return false;
        };
        match key {
            TypeData::TypeParameter(ref info) => self.find_type_param(info.name).is_some(),
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let base = app.base;
                let args = app.args.clone();
                self.target_contains_inference_param_inner(base, visited)
                    || args
                        .iter()
                        .any(|&arg| self.target_contains_inference_param_inner(arg, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let list = self.interner.type_list(members);
                list.iter()
                    .any(|&m| self.target_contains_inference_param_inner(m, visited))
            }
            _ => false,
        }
    }

    /// Infer from intersection types
    fn infer_intersections(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // For intersections, we can pick any member that matches
        for source_ty in source_list.iter() {
            for target_ty in target_list.iter() {
                // Don't fail if one member doesn't match
                let _ = self.infer_from_types(*source_ty, *target_ty, priority);
            }
        }

        Ok(())
    }

    /// Infer from `TypeApplication` (generic type instantiations)
    fn infer_applications(
        &mut self,
        source: TypeId,
        source_app: TypeApplicationId,
        target_app: TypeApplicationId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_info = self.interner.type_application(source_app);
        let target_info = self.interner.type_application(target_app);

        // When both applications share the same base type, infer directly from
        // type arguments, respecting the variance of each type parameter position.
        // This matches tsc's inferFromTypeArguments: contravariant type parameters
        // (e.g., T in `type Func<T> = (x: T) => void`) swap source/target direction
        // so that inference candidates are correctly categorized.
        if source_info.base == target_info.base {
            // Try to compute variances for the base type's type parameters.
            // This requires a resolver and a Lazy(DefId) base type.
            let variances = self.compute_application_variances(source_info.base);
            for (i, (source_arg, target_arg)) in source_info
                .args
                .iter()
                .zip(target_info.args.iter())
                .enumerate()
            {
                let variance = variances
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .unwrap_or(Variance::COVARIANT);
                if variance.is_contravariant() {
                    // Contravariant position: swap source and target so that
                    // candidates are recorded as contra-candidates (via in_contra_mode)
                    // or equivalently, infer in the reverse direction.
                    let was_contra = self.in_contra_mode;
                    self.in_contra_mode = !was_contra;
                    self.infer_from_types(*source_arg, *target_arg, priority)?;
                    self.in_contra_mode = was_contra;
                } else {
                    self.infer_from_types(*source_arg, *target_arg, priority)?;
                }
            }
            return Ok(());
        }

        // When the bases differ (e.g., source is MyPromise<boolean, any> and target
        // is DoNothingAlias<T, U> where DoNothingAlias extends MyPromise), expand
        // the target application to its structural form and infer structurally.
        // This mirrors tsc's behavior of walking the base type chain when reference
        // type targets don't share the same origin as the source.
        if let Some(expanded_target) = self.try_expand_application(target_app) {
            return self.infer_from_types(source, expanded_target, priority);
        }

        Ok(())
    }

    // =========================================================================
    // Task #40: Template Literal Deconstruction
    // =========================================================================

    /// Infer from template literal patterns with `infer` placeholders.
    ///
    /// This implements the "Reverse String Matcher" for extracting type information
    /// from string literals that match template patterns like `user_${infer ID}`.
    ///
    /// # Example
    ///
    /// ```typescript
    /// type GetID<T> = T extends `user_${infer ID}` ? ID : never;
    /// // GetID<"user_123"> should infer ID = "123"
    /// ```
    ///
    /// # Algorithm
    ///
    /// The matching is **non-greedy** for all segments except the last:
    /// 1. Scan through template spans sequentially
    /// 2. For text spans: match literal text at current position
    /// 3. For infer type spans: capture text until next literal anchor (non-greedy)
    /// 4. For the last span: capture all remaining text (greedy)
    ///
    /// # Arguments
    ///
    /// * `source` - The source type being checked (e.g., `"user_123"`)
    /// * `source_key` - The `TypeData` of the source (cached for efficiency)
    /// * `target_template` - The template literal pattern to match against
    /// * `priority` - Inference priority for the extracted candidates
    fn infer_from_template_literal(
        &mut self,
        source: TypeId,
        source_key: Option<&TypeData>,
        target_template: TemplateLiteralId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let spans = self.interner.template_list(target_template);

        // Special case: if source is `any` or the intrinsic `string` type, all infer vars get that type
        if source == TypeId::ANY
            || matches!(source_key, Some(TypeData::Intrinsic(IntrinsicKind::String)))
        {
            for span in spans.iter() {
                if let TemplateSpan::Type(type_id) = span
                    && let Some(TypeData::Infer(param_info)) = self.interner.lookup(*type_id)
                    && let Some(var) = self.find_type_param(param_info.name)
                {
                    // Source is `any` or `string`, so infer that for all variables
                    self.add_candidate(var, source, priority);
                }
            }
            return Ok(());
        }

        // If source is a union, try to match each member against the template
        if let Some(TypeData::Union(source_members)) = source_key {
            let members = self.interner.type_list(*source_members);
            for &member in members.iter() {
                let member_key = self.interner.lookup(member);
                self.infer_from_template_literal(
                    member,
                    member_key.as_ref(),
                    target_template,
                    priority,
                )?;
            }
            return Ok(());
        }

        // For literal string types, perform the actual pattern matching
        if let Some(source_str) = self.extract_string_literal(source)
            && let Some(captures) = self.match_template_pattern(&source_str, &spans)
        {
            // Convert captured strings to literal types and add as candidates
            for (infer_var, captured_string) in captures {
                let literal_type = self.interner.literal_string(&captured_string);
                self.add_candidate(infer_var, literal_type, priority);
            }
        }

        Ok(())
    }

    /// Extract a string literal value from a `TypeId`.
    ///
    /// Returns None if the type is not a literal string.
    fn extract_string_literal(&self, type_id: TypeId) -> Option<String> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(LiteralValue::String(s))) => Some(self.interner.resolve_atom(s)),
            _ => None,
        }
    }

    /// Match a source string against a template pattern, extracting infer variable bindings.
    ///
    /// # Arguments
    ///
    /// * `source` - The source string to match (e.g., `"user_123"`)
    /// * `spans` - The template spans (e.g., `[Text("user_"), Type(ID), Text("_")]`)
    ///
    /// # Returns
    ///
    /// * `Some(bindings)` - Mapping from inference variables to captured strings
    /// * `None` - The source doesn't match the pattern
    fn match_template_pattern(
        &self,
        source: &str,
        spans: &[TemplateSpan],
    ) -> Option<Vec<(InferenceVar, String)>> {
        let mut bindings = Vec::new();
        let mut pos = 0;

        for (i, span) in spans.iter().enumerate() {
            let is_last = i == spans.len() - 1;

            match span {
                TemplateSpan::Text(text_atom) => {
                    // Match literal text at current position
                    let text = self.interner.resolve_atom(*text_atom).to_string();
                    if !source.get(pos..)?.starts_with(&text) {
                        return None; // Text doesn't match
                    }
                    pos += text.len();
                }

                TemplateSpan::Type(type_id) => {
                    // Check if this is an infer variable
                    if let Some(TypeData::Infer(param_info)) = self.interner.lookup(*type_id)
                        && let Some(var) = self.find_type_param(param_info.name)
                    {
                        if is_last {
                            // Last span: capture all remaining text (greedy)
                            let captured = source[pos..].to_string();
                            bindings.push((var, captured));
                            pos = source.len();
                        } else {
                            // Non-last span: capture until next literal anchor (non-greedy)
                            // Find the next text span to use as an anchor
                            if let Some(anchor_text) = self.find_next_text_anchor(spans, i) {
                                let anchor = self.interner.resolve_atom(anchor_text).to_string();
                                // Find the first occurrence of the anchor (non-greedy)
                                let capture_end = source[pos..].find(&anchor)? + pos;
                                let captured = source[pos..capture_end].to_string();
                                bindings.push((var, captured));
                                pos = capture_end;
                            } else {
                                // No text anchor found (e.g., `${infer A}${infer B}`)
                                // Capture empty string for non-greedy match and continue
                                bindings.push((var, String::new()));
                                // pos remains unchanged - next infer var starts here
                            }
                        }
                    }
                }
            }
        }

        // Must have consumed the entire source string
        (pos == source.len()).then_some(bindings)
    }

    /// Find the next text span after a given index to use as a matching anchor.
    fn find_next_text_anchor(&self, spans: &[TemplateSpan], start_idx: usize) -> Option<Atom> {
        spans.iter().skip(start_idx + 1).find_map(|span| {
            if let TemplateSpan::Text(text) = span {
                Some(*text)
            } else {
                None
            }
        })
    }

    /// Get the "partially inferable" version of a type for property inference.
    ///
    /// Matches tsc's `getPartiallyInferableType`: for function types whose
    /// parameters have type `any` (from implicit typing in method shorthands),
    /// replace those `any` parameters with `unknown`. This prevents implicit
    /// `any` from flowing contravariantly into inference candidates, which
    /// would incorrectly produce `T = any` instead of `T = unknown` when
    /// inference has no other information.
    ///
    /// This is critical for reverse-mapped type inference where callback
    /// parameters depend on the type being inferred. Without this, patterns
    /// like `{ contains(k) { ... } }` matched against `{ [K in keyof T]: Box<T[K]> }`
    /// would infer `T[K] = any` instead of `T[K] = unknown`.
    fn get_partially_inferable_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                // Only transform if the function has any `any`-typed parameters.
                // This indicates the parameters are implicitly typed (from method
                // shorthand or untyped callback params). Explicitly typed `any`
                // params would have the same effect but are rare enough that the
                // slightly conservative behavior is acceptable.
                let has_any_param = shape.params.iter().any(|p| p.type_id == TypeId::ANY);
                if !has_any_param {
                    return type_id;
                }
                let new_params: Vec<ParamInfo> = shape
                    .params
                    .iter()
                    .map(|p| {
                        if p.type_id == TypeId::ANY {
                            ParamInfo {
                                type_id: TypeId::UNKNOWN,
                                ..*p
                            }
                        } else {
                            p.clone()
                        }
                    })
                    .collect();
                let new_shape = FunctionShape {
                    params: new_params,
                    ..(*shape).clone()
                };
                self.interner.function(new_shape)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                let has_any_param = shape
                    .call_signatures
                    .iter()
                    .any(|sig| sig.params.iter().any(|p| p.type_id == TypeId::ANY));
                if !has_any_param {
                    return type_id;
                }
                let new_sigs: Vec<_> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| {
                        let new_params: Vec<ParamInfo> = sig
                            .params
                            .iter()
                            .map(|p| {
                                if p.type_id == TypeId::ANY {
                                    ParamInfo {
                                        type_id: TypeId::UNKNOWN,
                                        ..*p
                                    }
                                } else {
                                    p.clone()
                                }
                            })
                            .collect();
                        CallSignature {
                            params: new_params,
                            ..sig.clone()
                        }
                    })
                    .collect();
                let mut new_shape = (*shape).clone();
                new_shape.call_signatures = new_sigs;
                self.interner.callable(new_shape)
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                // For object types, transform any function-typed properties
                // to their partially inferable versions. This handles cases
                // like `{ contains(k) {...} }` where the method is a property
                // of an object literal.
                let shape = self.interner.object_shape(shape_id);
                let has_function_with_any = shape.properties.iter().any(|p| {
                    matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Function(fid)) if {
                            let fs = self.interner.function_shape(fid);
                            fs.params.iter().any(|param| param.type_id == TypeId::ANY)
                        }
                    )
                });
                if !has_function_with_any {
                    return type_id;
                }
                let new_props: Vec<_> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        let new_type = self.get_partially_inferable_type(p.type_id);
                        if new_type != p.type_id {
                            let mut new_prop = p.clone();
                            new_prop.type_id = new_type;
                            new_prop
                        } else {
                            p.clone()
                        }
                    })
                    .collect();
                let mut new_shape = (*shape).clone();
                new_shape.properties = new_props;
                // Use object_with_index for both Object and ObjectWithIndex
                // since this is a temporary type only used during inference.
                self.interner.object_with_index(new_shape)
            }
            _ => type_id,
        }
    }
}
