impl<'a> InferenceContext<'a> {
    fn target_contains_inference_param_inner(
        &self,
        target: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> bool {
        if target.is_intrinsic() {
            return false;
        }
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
        target: TypeId,
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
        let shared_base_def = if source_info.base == target_info.base {
            self.application_base_def_id(source_info.base)
        } else {
            self.shared_application_base_def_id(source_info.base, target_info.base)
        };
        if source_info.base == target_info.base || shared_base_def.is_some() {
            // Try to compute variances for the base type's type parameters.
            // This requires a resolver and a Lazy(DefId) base type.
            let variances = shared_base_def
                .and_then(|def_id| {
                    let resolver = self.resolver?;
                    compute_type_param_variances_with_resolver(self.interner, resolver, def_id)
                })
                .or_else(|| self.compute_application_variances(source_info.base));
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

        // When the bases differ, expand both applications when possible before
        // falling back to one-sided expansion. This mirrors tsc's reference
        // inference path, where structurally related generic references can still
        // contribute candidates even when their declarations differ.
        let expanded_source = self.try_expand_application(source_app);
        let expanded_target = self.try_expand_application(target_app);
        match (expanded_source, expanded_target) {
            (Some(expanded_source), Some(expanded_target)) => {
                return self.infer_from_types(expanded_source, expanded_target, priority);
            }
            (Some(expanded_source), None) => {
                return self.infer_from_types(expanded_source, target, priority);
            }
            (None, Some(expanded_target)) => {
                return self.infer_from_types(source, expanded_target, priority);
            }
            (None, None) => {}
        }

        Ok(())
    }

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
                    && let Some(TypeData::Infer(param_info) | TypeData::TypeParameter(param_info)) =
                        self.interner.lookup(*type_id)
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

        if let Some(TypeData::TemplateLiteral(source_template)) = source_key {
            let source_spans = self.interner.template_list(*source_template);
            if source_spans.len() == spans.len() {
                let source_spans: Vec<TemplateSpan> = source_spans.iter().cloned().collect();
                let target_spans: Vec<TemplateSpan> = spans.iter().cloned().collect();
                let mut matched = true;
                for (source_span, target_span) in source_spans.iter().zip(target_spans.iter()) {
                    match (source_span, target_span) {
                        (TemplateSpan::Text(source_text), TemplateSpan::Text(target_text))
                            if source_text == target_text => {}
                        (TemplateSpan::Type(source_type), TemplateSpan::Type(target_type)) => {
                            // Promote so a captured `${number}` segment yields
                            // a string subtype rather than the bare `number`.
                            let promoted = crate::type_queries::extended::string_like_type_for_type(
                                self.interner,
                                *source_type,
                            );
                            self.infer_from_types(promoted, *target_type, priority)?;
                        }
                        _ => {
                            matched = false;
                            break;
                        }
                    }
                }
                if matched {
                    return Ok(());
                }
            }
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
        // BOOLEAN_TRUE/FALSE are intrinsic IDs that resolve to Literal(Boolean),
        // never Literal(String). Other intrinsics resolve to Intrinsic. Skip lookup.
        if type_id.is_intrinsic() {
            return None;
        }
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
        let mut bindings = Vec::with_capacity(spans.len());
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
                    // Match both `infer T` (conditional) and generic `T` (type parameter).
                    // Intrinsics are never Infer or TypeParameter.
                    if !type_id.is_intrinsic()
                        && let Some(
                            TypeData::Infer(param_info) | TypeData::TypeParameter(param_info),
                        ) = self.interner.lookup(*type_id)
                        && let Some(var) = self.find_type_param(param_info.name)
                    {
                        if is_last {
                            // Last span: capture all remaining text (greedy)
                            let captured = source[pos..].to_string();
                            bindings.push((var, captured));
                            pos = source.len();
                        } else if let Some(alternatives) =
                            find_next_anchor_alternatives(self.interner, spans, i, |type_id| {
                                if type_id.is_intrinsic() {
                                    return false;
                                }
                                matches!(
                                    self.interner.lookup(type_id),
                                    Some(
                                        TypeData::Infer(param_info)
                                            | TypeData::TypeParameter(param_info)
                                    ) if self.find_type_param(param_info.name).is_some()
                                )
                            })
                        {
                            let capture_end = find_leftmost_occurrence(source, pos, &alternatives)?;
                            let captured = source[pos..capture_end].to_string();
                            bindings.push((var, captured));
                            pos = capture_end;
                        } else {
                            bindings.push((var, String::new()));
                        }
                    } else {
                        let next_pos =
                            match_template_segment_prefix(self.interner, source, pos, *type_id)?;
                        pos = next_pos;
                    }
                }
            }
        }

        // Must have consumed the entire source string
        (pos == source.len()).then_some(bindings)
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
        // Intrinsics are never Function/Object/Tuple — return as-is.
        if type_id.is_intrinsic() {
            return type_id;
        }
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
                            *p
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
                                    *p
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
