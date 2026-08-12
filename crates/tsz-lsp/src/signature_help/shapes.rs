use super::*;
use tsz_checker::state::CheckerState;

impl<'a> SignatureHelpProvider<'a> {
    pub(super) fn get_signatures_from_type(
        &self,
        type_id: TypeId,
        checker: &CheckerState,
        call_kind: CallKind,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Vec<SignatureCandidate> {
        if let Some(shape_id) = visitor::function_shape_id(self.interner, type_id) {
            let shape = self.interner.function_shape(shape_id);
            return self.signature_candidates_for_shape(
                &shape,
                checker,
                false,
                callee_name,
                has_explicit_type_args,
                explicit_type_arg_texts,
            );
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            let mut sigs = Vec::new();
            let include_call = call_kind == CallKind::Call || call_kind == CallKind::TaggedTemplate;
            let include_construct = call_kind == CallKind::New;
            sigs.reserve(
                usize::from(include_call) * shape.call_signatures.len()
                    + usize::from(include_construct) * shape.construct_signatures.len(),
            );

            if include_call {
                // Add call signatures
                for sig in &shape.call_signatures {
                    // Convert CallSignature to FunctionShape for formatting
                    let func_shape = FunctionShape {
                        type_params: sig.type_params.clone(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: false,
                    };
                    sigs.extend(self.signature_candidates_for_shape(
                        &func_shape,
                        checker,
                        false,
                        callee_name,
                        has_explicit_type_args,
                        explicit_type_arg_texts,
                    ));
                }
            }
            if include_construct {
                // Add construct signatures
                for sig in &shape.construct_signatures {
                    let func_shape = FunctionShape {
                        type_params: sig.type_params.clone(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: true,
                        is_method: false,
                    };
                    sigs.extend(self.signature_candidates_for_shape(
                        &func_shape,
                        checker,
                        true,
                        callee_name,
                        has_explicit_type_args,
                        explicit_type_arg_texts,
                    ));
                }
            }
            return sigs;
        }

        // Union of functions
        if let Some(members) = visitor::union_list_id(self.interner, type_id) {
            let members = self.interner.type_list(members);
            let mut sigs = Vec::with_capacity(members.len());
            for &member in members.iter() {
                sigs.extend(self.get_signatures_from_type(
                    member,
                    checker,
                    call_kind,
                    callee_name,
                    has_explicit_type_args,
                    explicit_type_arg_texts,
                ));
            }
            return sigs;
        }

        vec![]
    }

    /// Returns the return type when `type_id` is the synthetic apparent method type created by
    /// `make_apparent_method_type`: a single nameless rest parameter of type `any[]`.
    /// Distinguishes the no-lib fallback shape from real function types with a rest parameter.
    pub(super) fn synthetic_apparent_method_return_type(&self, type_id: TypeId) -> Option<TypeId> {
        let shape_id = visitor::function_shape_id(self.interner, type_id)?;
        let shape = self.interner.function_shape(shape_id);
        if shape.params.len() != 1 {
            return None;
        }
        let p = &shape.params[0];
        if p.rest && p.name.is_none() && p.type_id == self.interner.array(TypeId::ANY) {
            Some(shape.return_type)
        } else {
            None
        }
    }

    /// If `callee_expr` is a property-access on a primitive intrinsic type and the
    /// resolved callee type is the synthetic `...args: any[]` fallback, return
    /// signatures built from the known intrinsic parameter table.
    pub(super) fn try_build_intrinsic_signatures(
        &self,
        callee_expr: NodeIndex,
        callee_type: TypeId,
        checker: &mut CheckerState,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Option<Vec<SignatureCandidate>> {
        let return_type = self.synthetic_apparent_method_return_type(callee_type)?;
        let callee_node = self.arena.get(callee_expr)?;
        let access = self.arena.get_access_expr(callee_node)?;
        let method_name = self.arena.get_identifier_text(access.name_or_argument)?;
        let raw_obj_type = checker.get_type_of_node(access.expression);
        let obj_type = checker.resolve_lazy_type(raw_obj_type);
        let kind = apparent_intrinsic_kind(self.interner, obj_type)?;
        let param_specs: &[IntrinsicParamSpec] = match kind {
            tsz_solver::IntrinsicKind::String => string_intrinsic_method_params(method_name),
            tsz_solver::IntrinsicKind::Number => number_intrinsic_method_params(method_name),
            tsz_solver::IntrinsicKind::Boolean => boolean_intrinsic_method_params(method_name),
            tsz_solver::IntrinsicKind::Bigint => bigint_intrinsic_method_params(method_name),
            _ => None,
        }?;

        let params: Vec<ParamInfo> = param_specs
            .iter()
            .map(|spec| {
                let base_ty = Self::intrinsic_param_type_hint_to_type_id(spec.ty);
                let type_id = if spec.rest {
                    self.interner.array(base_ty)
                } else {
                    base_ty
                };
                ParamInfo {
                    name: Some(self.interner.intern_string(spec.name)),
                    type_id,
                    optional: spec.optional,
                    rest: spec.rest,
                    arity_only_optional: false,
                }
            })
            .collect();

        let shape = FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        Some(self.signature_candidates_for_shape(
            &shape,
            checker,
            false,
            callee_name,
            has_explicit_type_args,
            explicit_type_arg_texts,
        ))
    }

    pub(super) const fn intrinsic_param_type_hint_to_type_id(
        hint: IntrinsicParamTypeHint,
    ) -> TypeId {
        match hint {
            IntrinsicParamTypeHint::String => TypeId::STRING,
            IntrinsicParamTypeHint::Number => TypeId::NUMBER,
        }
    }

    pub(super) fn signature_candidates_for_shape(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Vec<SignatureCandidate> {
        self.expand_rest_tuple_union_variants(shape)
            .into_iter()
            .map(|variant| {
                self.signature_candidate(
                    &variant,
                    checker,
                    is_constructor,
                    callee_name,
                    has_explicit_type_args,
                    explicit_type_arg_texts,
                )
            })
            .collect()
    }

    pub(super) fn expand_rest_tuple_union_variants(
        &self,
        shape: &FunctionShape,
    ) -> Vec<FunctionShape> {
        let Some(rest_index) = shape.params.iter().position(|param| param.rest) else {
            return vec![shape.clone()];
        };
        let rest_param = shape.params[rest_index];
        let Some(list_id) = visitor::union_list_id(self.interner, rest_param.type_id) else {
            return vec![shape.clone()];
        };

        let members = self.interner.type_list(list_id);
        let mut variants = Vec::with_capacity(members.len());
        for &member in members.iter() {
            if visitor::tuple_list_id(self.interner, member).is_none() {
                continue;
            }
            let mut variant = shape.clone();
            variant.params[rest_index].type_id = member;
            variants.push(variant);
        }

        if variants.is_empty() {
            vec![shape.clone()]
        } else {
            variants
        }
    }

    /// Format a `FunctionShape` into `SignatureInformation`
    pub(super) fn format_signature(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
    ) -> SignatureInformation {
        let mut parameters = Vec::new();

        // Build type parameters string for generics.
        // When no explicit type arguments are provided, hide the type parameter list.
        let type_params_str = if !shape.type_params.is_empty() && has_explicit_type_args {
            let tp_parts: Vec<String> = shape
                .type_params
                .iter()
                .map(|tp| {
                    let name = checker.ctx.types.resolve_atom(tp.name);
                    if let Some(constraint) = tp.constraint {
                        format!("{} extends {}", name, checker.format_type(constraint))
                    } else {
                        name
                    }
                })
                .collect();
            format!("<{}>", tp_parts.join(", "))
        } else {
            String::new()
        };

        // Build parameters
        let mut param_labels = Vec::new();
        // Note: we do NOT include `this` parameter in user-visible params
        // because tsserver also excludes it from the signature help display.

        for param in &shape.params {
            // When a rest parameter has a tuple type, expand the tuple elements
            // as individual parameters. e.g. `...args: [...names: string[], allCaps: boolean]`
            // becomes `...names: string[], allCaps: boolean`.
            if param.rest
                && let Some(list_id) = visitor::tuple_list_id(self.interner, param.type_id)
            {
                let elements = self.interner.tuple_list(list_id);
                let param_base_name = param.name.map_or_else(
                    || "arg".to_string(),
                    |atom| checker.ctx.types.resolve_atom(atom),
                );
                for (i, elem) in elements.iter().enumerate() {
                    let elem_name = elem.name.map_or_else(
                        || format!("{param_base_name}_{i}"),
                        |atom| checker.ctx.types.resolve_atom(atom),
                    );
                    let type_str = if elem.type_id == TypeId::UNKNOWN {
                        "any".to_string()
                    } else {
                        checker.format_type(elem.type_id)
                    };
                    let is_optional = elem.optional && !elem.rest;
                    let optional = if is_optional && !self.is_js_like_file() {
                        "?"
                    } else {
                        ""
                    };
                    let rest = if elem.rest { "..." } else { "" };

                    let param_label = format!("{rest}{elem_name}{optional}: {type_str}");
                    parameters.push(ParameterInformation {
                        name: elem_name.clone(),
                        label: param_label.clone(),
                        documentation: None,
                        is_optional,
                        is_rest: elem.rest,
                    });
                    param_labels.push(param_label);
                }
                continue;
            }

            let mut name = param.name.map_or_else(
                || "arg".to_string(),
                |atom| checker.ctx.types.resolve_atom(atom),
            );
            if param.rest && name == "arg" {
                name = "args".to_string();
            }
            let mut type_str = if param.type_id == TypeId::UNKNOWN {
                "any".to_string()
            } else {
                checker.format_type(param.type_id)
            };
            // Rest parameters with bare 'any' type should display as 'any[]'
            if param.rest && type_str == "any" {
                type_str = "any[]".to_string();
            }
            let is_optional = param.optional && !param.rest;
            let optional = if is_optional && !self.is_js_like_file() {
                "?"
            } else {
                ""
            };
            let rest = if param.rest { "..." } else { "" };

            let param_label = format!("{rest}{name}{optional}: {type_str}");
            parameters.push(ParameterInformation {
                name: name.clone(),
                label: param_label.clone(),
                documentation: None,
                is_optional,
                is_rest: param.rest,
            });

            param_labels.push(param_label);
        }

        // Inferred tuple wrappers may degrade `(...a: [])` to `(...a: any[])`.
        // Align with tsserver display by rendering this synthetic single-rest
        // parameter shape as a zero-parameter callable.
        if parameters.len() == 1 {
            let only = &parameters[0];
            if only.is_rest && only.name == "a" && only.label == "...a: any[]" {
                parameters.clear();
                param_labels.clear();
            }
        }

        // Build prefix and suffix
        // For return type display:
        // - Type predicate → "paramName is Type" or "this is Type"
        // - UNKNOWN → "any" (matches TypeScript's display for untyped returns)
        // - Constructor with OBJECT/UNKNOWN → class name (TypeScript shows class name)
        let return_type_str = if is_constructor {
            // For construct signatures with an explicit return type, use it.
            // Otherwise fall back to callee name (class constructors).
            if shape.return_type != TypeId::UNKNOWN {
                checker.format_type(shape.return_type)
            } else {
                callee_name.to_string()
            }
        } else if let Some(ref predicate) = shape.type_predicate {
            // Format type predicate: "x is Type" or "asserts x is Type"
            let target_name = match &predicate.target {
                TypePredicateTarget::This => "this".to_string(),
                TypePredicateTarget::Identifier(atom) => checker.ctx.types.resolve_atom(*atom),
            };
            let type_part = predicate
                .type_id
                .map(|tid| checker.format_type(tid))
                .unwrap_or_default();
            if predicate.asserts {
                if type_part.is_empty() {
                    format!("asserts {target_name}")
                } else {
                    format!("asserts {target_name} is {type_part}")
                }
            } else if type_part.is_empty() {
                target_name
            } else {
                format!("{target_name} is {type_part}")
            }
        } else if shape.return_type == TypeId::UNKNOWN {
            // Functions without return type annotation display as 'any' in TypeScript
            "any".to_string()
        } else {
            checker.format_type(shape.return_type)
        };
        let prefix = format!("{callee_name}{type_params_str}(");
        let suffix = format!("): {return_type_str}");

        // Build full label: prefix + params joined by ", " + suffix
        let label = format!("{}{}{}", prefix, param_labels.join(", "), suffix,);
        let is_variadic = parameters.iter().any(|param| param.is_rest);

        SignatureInformation {
            label,
            prefix,
            suffix,
            documentation: None,
            parameters,
            is_variadic,
            is_constructor,
            tags: Vec::new(),
        }
    }

    pub(super) fn signature_candidate(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> SignatureCandidate {
        let type_params = shape
            .type_params
            .iter()
            .map(|tp| {
                let name = checker.ctx.types.resolve_atom(tp.name);
                if let Some(constraint) = tp.constraint {
                    format!("{name} extends {}", checker.format_type(constraint))
                } else {
                    name
                }
            })
            .collect::<Vec<_>>();
        let type_param_substitutions = if !shape.type_params.is_empty() {
            if has_explicit_type_args && !explicit_type_arg_texts.is_empty() {
                // Use the actual explicit type argument text for substitution
                shape
                    .type_params
                    .iter()
                    .enumerate()
                    .map(|(i, tp)| {
                        let name = checker.ctx.types.resolve_atom(tp.name);
                        let substitution = if i < explicit_type_arg_texts.len() {
                            explicit_type_arg_texts[i].clone()
                        } else if let Some(default) = tp.default {
                            checker.format_type(default)
                        } else if let Some(constraint) = tp.constraint {
                            checker.format_type(constraint)
                        } else {
                            "unknown".to_string()
                        };
                        (name, substitution)
                    })
                    .collect()
            } else if !has_explicit_type_args {
                // No explicit type args: use defaults/constraints/unknown
                shape
                    .type_params
                    .iter()
                    .map(|tp| {
                        let name = checker.ctx.types.resolve_atom(tp.name);
                        let substitution = if let Some(default) = tp.default {
                            checker.format_type(default)
                        } else if let Some(constraint) = tp.constraint {
                            checker.format_type(constraint)
                        } else {
                            "unknown".to_string()
                        };
                        (name, substitution)
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        // When explicit type args are provided and we have substitutions,
        // hide the <T, U> prefix since the types are instantiated in params.
        let show_type_params = has_explicit_type_args
            && (explicit_type_arg_texts.is_empty() || type_param_substitutions.is_empty());
        let info = self.format_signature(
            shape,
            checker,
            is_constructor,
            callee_name,
            show_type_params,
        );
        let required_params = info
            .parameters
            .iter()
            .filter(|param| !param.is_optional && !param.is_rest)
            .count();
        let total_params = info.parameters.len();
        let has_rest = info.parameters.iter().any(|param| param.is_rest);
        let param_names = info
            .parameters
            .iter()
            .map(|param| Some(param.name.clone()))
            .collect();
        SignatureCandidate {
            info,
            required_params,
            total_params,
            has_rest,
            param_names,
            type_params,
            type_param_substitutions,
        }
    }

    pub(super) fn apply_source_signature_type_overrides(
        &self,
        signatures: &mut [SignatureCandidate],
        symbol_id: tsz_binder::SymbolId,
    ) {
        if signatures.len() != 1 {
            return;
        }

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        if symbol.declarations.len() != 1 {
            return;
        }

        let Some((param_type_texts, return_type_text)) =
            self.source_signature_type_texts(symbol.declarations[0])
        else {
            return;
        };
        let Some(signature) = signatures.first_mut() else {
            return;
        };
        if param_type_texts.len() != signature.info.parameters.len() {
            return;
        }

        for (param, type_text) in signature.info.parameters.iter_mut().zip(param_type_texts) {
            if let Some(type_text) = type_text {
                let optional = if param.is_optional && !self.is_js_like_file() {
                    "?"
                } else {
                    ""
                };
                let rest = if param.is_rest { "..." } else { "" };
                param.label = format!("{rest}{}{optional}: {type_text}", param.name);
            }
        }

        if let Some(return_type_text) = return_type_text {
            signature.info.suffix = format!("): {return_type_text}");
        }

        let param_labels: Vec<String> = signature
            .info
            .parameters
            .iter()
            .map(|param| param.label.clone())
            .collect();
        signature.info.label = format!(
            "{}{}{}",
            signature.info.prefix,
            param_labels.join(", "),
            signature.info.suffix
        );
    }

    pub(super) fn expand_source_rest_tuple_union_signatures(
        &self,
        signatures: &mut Vec<SignatureCandidate>,
        symbol_id: tsz_binder::SymbolId,
    ) {
        if signatures.is_empty() {
            return;
        }

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        if symbol.declarations.len() != 1 {
            return;
        }
        let Some((param_type_texts, _)) = self.source_signature_type_texts(symbol.declarations[0])
        else {
            return;
        };
        let Some((rest_param_index, rest_tuple_union_text)) = param_type_texts
            .iter()
            .enumerate()
            .find_map(|(idx, maybe_text)| {
                let text = maybe_text.as_ref()?;
                (Self::tuple_union_variants(text).len() > 1).then_some((idx, text.clone()))
            })
        else {
            return;
        };
        let tuple_variants = Self::tuple_union_variants(&rest_tuple_union_text);
        if tuple_variants.len() <= 1 {
            return;
        }

        let Some(base) = signatures
            .iter()
            .find(|sig| sig.info.parameters.len() >= rest_param_index)
            .cloned()
            .or_else(|| signatures.first().cloned())
        else {
            return;
        };
        let base_rest_name = self
            .arena
            .get(symbol.declarations[0])
            .and_then(|decl_node| self.arena.get_function(decl_node))
            .and_then(|fn_data| fn_data.parameters.nodes.get(rest_param_index).copied())
            .and_then(|param_idx| self.arena.get(param_idx))
            .and_then(|param_node| self.arena.get_parameter(param_node))
            .and_then(|param| self.arena.get_identifier_text(param.name))
            .map(|name| name.to_string())
            .or_else(|| {
                signatures
                    .iter()
                    .flat_map(|sig| sig.info.parameters.iter())
                    .find(|param| param.is_rest)
                    .map(|param| param.name.clone())
            })
            .unwrap_or_else(|| "args".to_string());

        let mut expanded = Vec::with_capacity(tuple_variants.len());
        for tuple_variant in tuple_variants {
            let Some(expanded_rest_params) =
                Self::tuple_variant_parameters(&tuple_variant, &base_rest_name)
            else {
                continue;
            };
            let mut info = base.info.clone();
            let prefix_param_count = rest_param_index.min(base.info.parameters.len());
            let mut params = Vec::with_capacity(prefix_param_count + expanded_rest_params.len());
            params.extend_from_slice(&base.info.parameters[..prefix_param_count]);
            params.extend(expanded_rest_params);
            info.parameters = params;
            let labels: Vec<&str> = info
                .parameters
                .iter()
                .map(|param| param.label.as_str())
                .collect();
            info.label = format!("{}{}{}", info.prefix, labels.join(", "), info.suffix);

            let required_params = info
                .parameters
                .iter()
                .filter(|param| !param.is_optional && !param.is_rest)
                .count();
            let total_params = info.parameters.len();
            let has_rest = info.parameters.iter().any(|param| param.is_rest);
            info.is_variadic = has_rest;
            let param_names = info
                .parameters
                .iter()
                .map(|param| Some(param.name.clone()))
                .collect();

            expanded.push(SignatureCandidate {
                info,
                required_params,
                total_params,
                has_rest,
                param_names,
                type_params: base.type_params.clone(),
                type_param_substitutions: base.type_param_substitutions.clone(),
            });
        }

        if !expanded.is_empty() {
            *signatures = expanded;
        }
    }
}
