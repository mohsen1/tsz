//! Structural type matching for inference.
//!
//! This module implements the structural type-walking algorithm that collects
//! inference candidates by recursing into type shapes (objects, functions,
//! tuples, unions, intersections, template literals, etc.).
//!
//! It is the core of `infer_from_types`: given a source type and a target type
//! containing type parameters, it walks both structures in parallel and records
//! lower/upper bound candidates for each inference variable.

use crate::types::{
    CallableShapeId, FunctionShapeId, InferencePriority, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId,
    TypeData, TypeId, TypeListId,
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
    /// ```ignore
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
            // T <: target, so target is an UPPER bound
            self.add_upper_bound(var, target);
            return Ok(());
        }

        // Case 3: Structural recursion - match based on type structure
        match (source_key, target_key) {
            // Object types: recurse into properties
            (Some(TypeData::Object(source_shape)), Some(TypeData::Object(target_shape))) => {
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

            // Intersection types
            (
                Some(TypeData::Intersection(source_members)),
                Some(TypeData::Intersection(target_members)),
            ) => {
                self.infer_intersections(source_members, target_members, priority)?;
            }

            // TypeApplication: recurse into instantiated type
            (Some(TypeData::Application(source_app)), Some(TypeData::Application(target_app))) => {
                self.infer_applications(source_app, target_app, priority)?;
            }

            // Index access types: infer both object and index types
            (
                Some(TypeData::IndexAccess(source_obj, source_idx)),
                Some(TypeData::IndexAccess(target_obj, target_idx)),
            ) => {
                self.infer_from_types(source_obj, target_obj, priority)?;
                self.infer_from_types(source_idx, target_idx, priority)?;
            }

            // ReadonlyType: unwrap if both are readonly (e.g. readonly [T] vs readonly [number])
            (
                Some(TypeData::ReadonlyType(source_inner)),
                Some(TypeData::ReadonlyType(target_inner)),
            ) => {
                self.infer_from_types(source_inner, target_inner, priority)?;
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
            (Some(TypeData::Object(source_shape)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type(source_shape, mapped_id, priority)?;
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
                self.infer_from_types(source_prop.type_id, target_prop.type_id, priority)?;
            }
        }

        // Also check index signatures for inference
        // If target has a string index signature, infer from source's string index
        if let (Some(target_string_idx), Some(source_string_idx)) =
            (&target_shape.string_index, &source_shape.string_index)
        {
            self.infer_from_types(
                source_string_idx.value_type,
                target_string_idx.value_type,
                priority,
            )?;
        }

        // If target has a number index signature, infer from source's number index
        if let (Some(target_number_idx), Some(source_number_idx)) =
            (&target_shape.number_index, &source_shape.number_index)
        {
            self.infer_from_types(
                source_number_idx.value_type,
                target_number_idx.value_type,
                priority,
            )?;
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

        if source.properties.is_empty() {
            return Ok(());
        }

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

        // Infer the template type (T) from each source property value type
        for prop in &source.properties {
            self.infer_from_types(prop.type_id, mapped.template, priority)?;
        }

        Ok(())
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

        // Parameters are contravariant: swap source and target
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
                let source_param = source_params.next().unwrap();
                let target_param = target_params.next().unwrap();
                self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                break;
            }

            // If source has rest param, infer all remaining target params into it
            if source_rest {
                let source_param = source_params.next().unwrap();
                for target_param in target_params.by_ref() {
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                break;
            }

            // If target has rest param, infer all remaining source params into it
            if target_rest {
                let target_param = target_params.next().unwrap();

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

        // Return type is covariant: normal order
        self.infer_from_types(source_sig.return_type, target_sig.return_type, priority)?;

        // This type is contravariant
        if let (Some(source_this), Some(target_this)) = (source_sig.this_type, target_sig.this_type)
        {
            self.infer_from_types(target_this, source_this, priority)?;
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

    /// Infer from union types
    fn infer_unions(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // TypeScript inference filtering: when the target union contains both
        // type parameters and fixed types (e.g., `T | undefined`), strip source
        // members that match fixed target members before inferring against the
        // parameterized members. This prevents `undefined` in `number | undefined`
        // from being inferred as a candidate for `T` in `T | undefined`.
        let (parameterized, fixed): (Vec<TypeId>, Vec<TypeId>) = target_list
            .iter()
            .partition(|&&t| self.target_contains_inference_param(t));

        if !parameterized.is_empty() && !fixed.is_empty() {
            // Filter source: only infer members not already covered by fixed targets
            for &source_ty in source_list.iter() {
                let matches_fixed = fixed.contains(&source_ty);
                if !matches_fixed {
                    for &target_ty in &parameterized {
                        self.infer_from_types(source_ty, target_ty, priority)?;
                    }
                }
            }
        } else {
            // No filtering needed — fall back to exhaustive inference
            for source_ty in source_list.iter() {
                for target_ty in target_list.iter() {
                    self.infer_from_types(*source_ty, *target_ty, priority)?;
                }
            }
        }

        Ok(())
    }

    /// Check if a target type directly is or contains an inference type parameter.
    fn target_contains_inference_param(&self, target: TypeId) -> bool {
        let Some(key) = self.interner.lookup(target) else {
            return false;
        };
        match key {
            TypeData::TypeParameter(ref info) => self.find_type_param(info.name).is_some(),
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
        source_app: TypeApplicationId,
        target_app: TypeApplicationId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_info = self.interner.type_application(source_app);
        let target_info = self.interner.type_application(target_app);

        // The base types must match for inference to work
        if source_info.base != target_info.base {
            return Ok(());
        }

        // Recurse into the type arguments
        for (source_arg, target_arg) in source_info.args.iter().zip(target_info.args.iter()) {
            self.infer_from_types(*source_arg, *target_arg, priority)?;
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
}
