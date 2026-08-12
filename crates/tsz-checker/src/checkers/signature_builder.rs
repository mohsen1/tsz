//! Call/construct signature building (parameter extraction, instantiation, return types).

use crate::query_boundaries::signature_building as signature_query;
use crate::state::{CheckerState, ParamTypeResolutionMode};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, TypeId};

// =============================================================================
// Signature Building Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Call Signature Building
    // =========================================================================

    /// Build a `CallSignature` from a function declaration/expression.
    /// `func_idx` is the node index of the function declaration, used to resolve
    /// enclosing type parameters from outer generic scopes (e.g., inner function
    /// overloads referencing outer function type parameters).
    pub(crate) fn call_signature_from_function(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        func_idx: tsz_parser::parser::NodeIndex,
    ) -> CallSignature {
        // Push enclosing type parameters so that overload signatures can reference
        // type parameters from outer function/class/interface scopes.
        let enclosing_updates = self.push_enclosing_type_parameters(func_idx);

        self.exclude_params_for_type_param_constraints(&func.parameters);
        let (type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);
        self.clear_excluded_params_for_type_param_constraints();
        let (params, this_type) = self.extract_params_from_parameter_list(&func.parameters);
        let (return_type, type_predicate) =
            self.return_type_and_predicate(func.type_annotation, &params, &func.parameters.nodes);
        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_updates);

        signature_query::call_signature(
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            false,
        )
    }

    pub(crate) fn jsdoc_overload_call_signatures_for_function(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        func_idx: NodeIndex,
    ) -> Vec<CallSignature> {
        use tsz_common::comments::get_jsdoc_content;

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return Vec::new();
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return Vec::new();
        };
        let source_text = sf.text.to_string();
        let overload_docs: Vec<(String, u32)> = self
            .leading_jsdoc_comments_for_node(func_idx)
            .into_iter()
            .filter_map(|comment| {
                let jsdoc = get_jsdoc_content(&comment, &source_text);
                Self::jsdoc_contains_tag(&jsdoc, "overload").then_some((jsdoc, comment.pos))
            })
            .collect();
        if overload_docs.is_empty() {
            return Vec::new();
        }

        let base_signature = self.call_signature_from_function(func, func_idx);
        overload_docs
            .into_iter()
            .map(|(jsdoc, comment_pos)| {
                let (type_params, updates) =
                    self.push_jsdoc_template_type_parameters_for_comment(comment_pos, &jsdoc);
                let mut signature = base_signature.clone();
                signature.type_params = type_params;
                let jsdoc_params = Self::extract_jsdoc_param_names(&jsdoc);
                signature.params.truncate(jsdoc_params.len());

                for (i, (param_name, _)) in jsdoc_params.iter().enumerate() {
                    let Some(param) = signature.params.get_mut(i) else {
                        break;
                    };

                    let jsdoc_optional = Self::extract_jsdoc_param_type_string(&jsdoc, param_name)
                        .is_some_and(|type_expr| type_expr.trim().ends_with('='))
                        || Self::is_jsdoc_param_optional_by_brackets(&jsdoc, param_name);

                    if let Some(jsdoc_type) = self.resolve_jsdoc_param_type_with_pos(
                        &jsdoc,
                        param_name,
                        Some(comment_pos),
                    ) {
                        param.type_id = jsdoc_type;
                    }
                    param.optional = jsdoc_optional;
                    param.rest = Self::jsdoc_param_is_rest(&jsdoc, param_name);
                }

                let jsdoc_for_predicate = Some(jsdoc.clone());
                if let Some(predicate) = self
                    .extract_jsdoc_return_type_predicate(&jsdoc_for_predicate, &signature.params)
                {
                    signature.return_type = if predicate.asserts {
                        TypeId::VOID
                    } else {
                        TypeId::BOOLEAN
                    };
                    signature.type_predicate = Some(predicate);
                } else {
                    signature.return_type = self
                        .resolve_jsdoc_return_type(&jsdoc, Some(comment_pos))
                        .unwrap_or(TypeId::ANY);
                    signature.type_predicate = None;
                }

                self.pop_type_parameters(updates);
                signature
            })
            .collect()
    }

    /// Build a `CallSignature` from a method declaration.
    pub(crate) fn call_signature_from_method(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        method_idx: NodeIndex,
    ) -> tsz_solver::CallSignature {
        self.call_signature_from_method_with_this(method, None, method_idx)
    }

    /// Build only a method's declared parameter surface.
    ///
    /// Early class-construction queries sometimes need a later method's
    /// parameters before its body is checked. Inferring that body's return type
    /// would recurse into work that class publication intentionally deferred.
    pub(crate) fn call_signature_parameter_surface_from_method(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        method_idx: NodeIndex,
    ) -> tsz_solver::CallSignature {
        self.call_signature_from_method_internal(method, None, method_idx, true)
    }

    /// Build a `CallSignature` from a method declaration with an explicit `this` type.
    /// This is used for static methods where `this` refers to the constructor type.
    pub(crate) fn call_signature_from_method_with_this(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        explicit_this_type: Option<TypeId>,
        method_idx: NodeIndex,
    ) -> tsz_solver::CallSignature {
        self.call_signature_from_method_internal(method, explicit_this_type, method_idx, false)
    }

    fn call_signature_from_method_internal(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        explicit_this_type: Option<TypeId>,
        method_idx: NodeIndex,
        parameter_surface_only: bool,
    ) -> tsz_solver::CallSignature {
        let enclosing_updates = self.push_enclosing_type_parameters(method_idx);
        self.exclude_params_for_type_param_constraints(&method.parameters);
        let (mut type_params, type_param_updates) =
            self.push_type_parameters(&method.type_parameters);
        self.clear_excluded_params_for_type_param_constraints();
        let method_jsdoc = if self.is_js_file() {
            self.find_jsdoc_for_function(method_idx)
        } else {
            None
        };
        let jsdoc_type_param_updates = if type_params.is_empty() {
            let (jsdoc_type_params, updates) = method_jsdoc.as_ref().map_or_else(
                || (Vec::new(), Vec::new()),
                |jsdoc| self.push_jsdoc_template_type_parameters_for_owner(method_idx, jsdoc),
            );
            type_params = jsdoc_type_params;
            updates
        } else {
            Vec::new()
        };
        let (mut params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        if let Some(jsdoc) = method_jsdoc.as_ref() {
            let comment_start = self.get_jsdoc_comment_pos_for_function(method_idx);
            let jsdoc_param_names: Vec<String> = Self::extract_jsdoc_param_names(jsdoc)
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            for (i, param_idx) in method.parameters.nodes.iter().enumerate() {
                if i >= params.len() {
                    break;
                }
                let Some(param_node) = self.ctx.arena.get(*param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if param.type_annotation.is_some() {
                    continue;
                }
                let pname = self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, i);
                let jsdoc_optional = Self::extract_jsdoc_param_type_string(jsdoc, &pname)
                    .is_some_and(|type_expr| type_expr.trim().ends_with('='))
                    || Self::is_jsdoc_param_optional_by_brackets(jsdoc, &pname);
                if let Some(jsdoc_type) =
                    self.resolve_jsdoc_param_type_with_pos(jsdoc, &pname, comment_start)
                {
                    params[i].type_id = jsdoc_type;
                    params[i].optional =
                        param.question_token || param.initializer.is_some() || jsdoc_optional;
                }
            }
        }
        let (mut return_type, mut type_predicate) = if parameter_surface_only {
            (TypeId::ANY, None)
        } else if method.type_annotation.is_none() && method.body.is_some() {
            // Infer return type from body when there's no annotation
            // Push the this type for proper resolution
            let pushed_this = if let Some(this_ty) = explicit_this_type {
                self.ctx.this_type_stack.push(this_ty);
                true
            } else {
                false
            };
            let inferred = self.infer_return_type_from_body(method_idx, method.body, None);
            if pushed_this {
                self.ctx.this_type_stack.pop();
            }
            (inferred, None)
        } else {
            self.return_type_and_predicate(
                method.type_annotation,
                &params,
                &method.parameters.nodes,
            )
        };

        // Check JSDoc @returns for type predicates on class methods.
        // Mirrors the logic in get_type_of_function (function_type.rs) for standalone
        // functions. In JS files, method return type predicates like
        // `@return {this is Entry}` are specified via JSDoc instead of syntax.
        if !parameter_surface_only && type_predicate.is_none() {
            if let Some(pred) = self.extract_jsdoc_return_type_predicate(&method_jsdoc, &params) {
                return_type = if pred.asserts {
                    TypeId::VOID
                } else {
                    TypeId::BOOLEAN
                };
                type_predicate = Some(pred);
            } else if let Some(ref jsdoc) = method_jsdoc {
                // Also check for non-predicate JSDoc return types like `@return {false}`.
                // Without this, body inference widens literal return types (e.g., `false`
                // → `boolean`), which breaks union predicate narrowing that requires
                // non-predicate members to return only `false` or `never`.
                let comment_start = self.get_jsdoc_comment_pos_for_function(method_idx);
                if let Some(jsdoc_ret_type) = self.resolve_jsdoc_return_type(jsdoc, comment_start) {
                    return_type = jsdoc_ret_type;
                }
            }
        }

        if !parameter_surface_only
            && type_predicate.is_none()
            && method.type_annotation.is_none()
            && matches!(return_type, TypeId::BOOLEAN | TypeId::UNKNOWN)
            && method.body.is_some()
        {
            self.prewarm_inferred_predicate_operand_types(method.body);
            let analyzer = self.flow_analyzer();
            if let Some(pred) = analyzer.try_infer_type_predicate_from_body(
                method.body,
                &method.parameters.nodes,
                &params,
            ) {
                type_predicate = Some(pred);
            }
        }

        if !parameter_surface_only && method.type_annotation.is_none() {
            return_type = self.maybe_evaluate_inferred_return_contribution(return_type, None);
        }

        // Wrap unannotated generator/async method return types (matching get_type_of_function).
        let has_annotation = method.type_annotation.is_some();
        let is_generator = method.asterisk_token;
        let is_async = self.has_async_modifier(&method.modifiers);

        if !parameter_surface_only && !has_annotation && is_generator {
            let gen_name = if is_async {
                "AsyncGenerator"
            } else {
                "Generator"
            };
            let _resolved = self.resolve_lib_type_by_name(gen_name);
            let lazy_base = self.ctx.binder.file_locals.get(gen_name).map(|sym_id| {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                self.ctx.types.factory().lazy(def_id)
            });
            if let Some(base) = lazy_base {
                return_type = self
                    .ctx
                    .types
                    .factory()
                    .application(base, vec![TypeId::ANY, TypeId::VOID, TypeId::UNKNOWN]);
            }
        } else if !parameter_surface_only && !has_annotation && is_async {
            if let Some(inner) = self.unwrap_promise_type(return_type) {
                return_type = inner;
            }
            let promise_base = self
                .ctx
                .lib_promise_type_ref()
                .unwrap_or(TypeId::PROMISE_BASE);
            return_type = self
                .ctx
                .types
                .factory()
                .application(promise_base, vec![return_type]);
        }

        self.pop_type_parameters(jsdoc_type_param_updates);
        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_updates);

        signature_query::call_signature(
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            true,
        )
    }

    /// Build a `CallSignature` from a constructor declaration.
    pub(crate) fn call_signature_from_constructor(
        &mut self,
        ctor: &tsz_parser::parser::node::ConstructorData,
        ctor_idx: NodeIndex,
        instance_type: TypeId,
        class_type_params: &[tsz_solver::TypeParamInfo],
    ) -> tsz_solver::CallSignature {
        self.exclude_params_for_type_param_constraints(&ctor.parameters);
        let (type_params, type_param_updates) = self.push_type_parameters(&ctor.type_parameters);
        self.clear_excluded_params_for_type_param_constraints();
        let enclosing_class_template_types = self.enclosing_jsdoc_class_template_types(ctor_idx);
        let (mut params, this_type) = self.extract_params_from_parameter_list(&ctor.parameters);

        // In JS files, resolve JSDoc @param types for constructor parameters.
        // extract_params_from_parameter_list defaults untyped params to ANY,
        // but JSDoc @param {T} annotations should provide the real type.
        if self.is_js_file()
            && let Some(jsdoc) = self.find_jsdoc_for_function(ctor_idx)
        {
            for (i, param_idx) in ctor.parameters.nodes.iter().enumerate() {
                if i >= params.len() {
                    break;
                }
                if params[i].type_id != TypeId::ANY {
                    continue;
                }
                if let Some(param_node) = self.ctx.arena.get(*param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    && param.type_annotation.is_none()
                {
                    let pname = self.parameter_name_for_error(param.name);
                    let jsdoc_optional = Self::extract_jsdoc_param_type_string(&jsdoc, &pname)
                        .is_some_and(|type_expr| type_expr.trim().ends_with('='))
                        || Self::is_jsdoc_param_optional_by_brackets(&jsdoc, &pname);
                    if let Some(comment_start) = self.get_jsdoc_comment_pos_for_function(ctor_idx)
                        && let Some(jsdoc_type) = self
                            .resolve_jsdoc_param_type_with_pos(&jsdoc, &pname, Some(comment_start))
                            .or_else(|| {
                                Self::extract_jsdoc_param_type_string(&jsdoc, &pname).and_then(
                                    |type_expr| {
                                        let normalized = type_expr
                                            .trim()
                                            .trim_end_matches('=')
                                            .trim_start_matches("...")
                                            .trim();
                                        enclosing_class_template_types.get(normalized).copied()
                                    },
                                )
                            })
                    {
                        params[i].type_id = jsdoc_type;
                        params[i].optional =
                            param.question_token || param.initializer.is_some() || jsdoc_optional;
                    }
                }
            }
        }

        self.pop_type_parameters(type_param_updates);

        let mut all_type_params = Vec::with_capacity(class_type_params.len() + type_params.len());
        all_type_params.extend_from_slice(class_type_params);
        all_type_params.extend(type_params);

        signature_query::call_signature(
            all_type_params,
            params,
            this_type,
            instance_type,
            None,
            true,
        )
    }

    // =========================================================================
    // Signature Instantiation
    // =========================================================================

    /// Instantiate a signature (call or constructor) with type arguments.
    /// Substitutes type parameters with the provided type arguments throughout
    /// the signature's params, this type, return type, and type predicate.
    pub(crate) fn instantiate_signature(
        &self,
        sig: &tsz_solver::CallSignature,
        type_args: &[TypeId],
    ) -> tsz_solver::CallSignature {
        signature_query::instantiate_signature(self.ctx.types, sig, type_args)
    }

    /// Partially instantiate a signature with fewer type arguments than type
    /// parameters.  The explicitly supplied type arguments are substituted
    /// throughout the signature body, while the remaining type parameters are
    /// preserved (with their constraints/defaults updated to reflect the
    /// supplied args).  This allows the solver to infer the remaining
    /// parameters from call-site arguments before falling back to defaults.
    pub(crate) fn partially_instantiate_signature(
        &self,
        sig: &tsz_solver::CallSignature,
        supplied_args: &[TypeId],
    ) -> tsz_solver::CallSignature {
        signature_query::partially_instantiate_signature(self.ctx.types, sig, supplied_args)
    }

    // =========================================================================
    // Parameter Extraction
    // =========================================================================

    /// Helper to extract parameters from a `SignatureData`.
    pub(crate) fn extract_params_from_signature(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        // SignatureData belongs to type-position declarations such as interface
        // and type-literal members. Its parameter annotations must be resolved
        // through the binder-aware type-node path, not expression checking.
        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::FromTypeNode,
        )
    }

    /// Helper to extract parameters from a parameter list.
    pub(crate) fn extract_params_from_parameter_list(
        &mut self,
        params_list: &tsz_parser::parser::NodeList,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::FromTypeNode,
        )
    }

    /// Unified implementation for extracting parameters from a parameter list.
    /// The `mode` parameter controls which type resolution method is used.
    pub(crate) fn extract_params_from_parameter_list_impl(
        &mut self,
        params_list: &tsz_parser::parser::NodeList,
        mode: ParamTypeResolutionMode,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        let mut params = Vec::with_capacity(params_list.nodes.len());
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let type_id = if param.type_annotation.is_some() {
                // Later parameter annotations can reference earlier value
                // parameters via `typeof`.
                self.push_typeof_param_scope(&params);
                let type_id = match mode {
                    ParamTypeResolutionMode::InTypeLiteral => {
                        self.get_type_from_type_node_in_type_literal(param.type_annotation)
                    }
                    ParamTypeResolutionMode::FromTypeNode => {
                        self.get_type_from_type_node(param.type_annotation)
                    }
                };
                self.pop_typeof_param_scope(&params);
                type_id
            } else {
                TypeId::ANY
            };

            // Check for ThisKeyword parameter
            let name_node = self.ctx.arena.get(param.name);
            if let Some(name_node) = name_node
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                if this_type.is_none() {
                    this_type = Some(type_id);
                }
                continue;
            }

            // Extract parameter name
            let name: Option<Atom> = if let Some(name_node) = name_node {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    Some(self.ctx.types.intern_string(&name_data.escaped_text))
                } else {
                    None
                }
            } else {
                None
            };

            // In JS files, parameters without type annotations are implicitly optional
            let optional = param.question_token
                || param.initializer.is_some()
                || (self.is_js_file() && param.type_annotation.is_none());
            let rest = param.dot_dot_dot_token;

            // Check for "this" parameter by name
            if let Some(name_atom) = name
                && name_atom == this_atom
            {
                if this_type.is_none() {
                    this_type = Some(type_id);
                }
                continue;
            }

            // For `?`-optional params, tsc includes `| undefined` in the
            // signature type unconditionally (for display). Default-value
            // params keep the base type.
            let sig_type_id = if param.question_token
                && type_id != TypeId::ANY
                && type_id != TypeId::UNKNOWN
                && type_id != TypeId::ERROR
                && !crate::query_boundaries::common::type_contains_undefined(
                    self.ctx.types,
                    type_id,
                ) {
                self.ctx.types.factory().union2(type_id, TypeId::UNDEFINED)
            } else {
                type_id
            };

            params.push(signature_query::param_info(
                name,
                sig_type_id,
                optional,
                rest,
            ));
        }

        (params, this_type)
    }

    // =========================================================================
    // Return Type and Type Predicate
    // =========================================================================

    /// Extract return type and type predicate from a type annotation (declaration context).
    ///
    /// `raw_params` are the parameter declaration nodes backing `params`, used
    /// only to resolve a type predicate's subject identifier against
    /// destructuring binding patterns (see
    /// [`Self::type_predicate_name_matches_binding_element`]).
    pub(crate) fn return_type_and_predicate(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
        raw_params: &[NodeIndex],
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        self.return_type_and_predicate_impl(type_annotation, params, raw_params, false)
    }

    /// Extract return type and type predicate from a type literal annotation.
    pub(crate) fn return_type_and_predicate_in_type_literal(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
        raw_params: &[NodeIndex],
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        self.return_type_and_predicate_impl(type_annotation, params, raw_params, true)
    }

    /// Shared implementation for return type + type predicate extraction.
    /// When `in_type_literal` is true, uses `get_type_from_type_node_in_type_literal`;
    /// otherwise uses `get_type_from_type_node`.
    fn return_type_and_predicate_impl(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
        raw_params: &[NodeIndex],
        in_type_literal: bool,
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        use tsz_solver::TypePredicateTarget;

        if type_annotation.is_none() {
            return (TypeId::ANY, None);
        }

        let resolve_type = |this: &mut Self, node: NodeIndex| {
            if in_type_literal {
                this.get_type_from_type_node_in_type_literal(node)
            } else {
                this.get_type_from_type_node(node)
            }
        };

        let Some(predicate_node_idx) = self.find_type_predicate_node(type_annotation) else {
            return (resolve_type(self, type_annotation), None);
        };

        let Some(node) = self.ctx.arena.get(predicate_node_idx) else {
            return (TypeId::BOOLEAN, None);
        };
        let Some(data) = self.ctx.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node.is_none() {
            None
        } else {
            Some(resolve_type(self, data.type_node))
        };

        let mut parameter_index = None;
        if let TypePredicateTarget::Identifier(name) = &target {
            parameter_index = params.iter().position(|p| p.name == Some(*name));
            if parameter_index.is_none() {
                if self.ctx.has_parse_errors {
                    return (return_type, None);
                }
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                let name_text = self.ctx.types.resolve_atom(*name);
                // Report on the asserted identifier, not the whole predicate node.
                // For an `asserts X is T` / `asserts X` predicate the TYPE_PREDICATE
                // node begins at the `asserts` modifier, so `node.pos` points at the
                // keyword while tsc points at the named identifier. The
                // `parameter_name` node always spans exactly that identifier (for a
                // plain `X is T` predicate it coincides with `node.pos`, so this is a
                // no-op there). Mirrors the rest-parameter branch below.
                let error_node = self.ctx.arena.get(data.parameter_name).unwrap_or(node);
                // `name` never matched a top-level parameter above. tsc still
                // resolves it against the whole parameter list, including
                // destructuring binding patterns, and reports TS1230 when it
                // names an element the pattern introduces — a predicate's
                // subject must be a plain parameter, never a destructured
                // binding (renamed, nested, array, or rest).
                if self.type_predicate_name_matches_binding_element(raw_params, &name_text) {
                    self.ctx.error(
                        error_node.pos,
                        error_node.end.saturating_sub(error_node.pos),
                        diagnostic_messages::A_TYPE_PREDICATE_CANNOT_REFERENCE_ELEMENT_IN_A_BINDING_PATTERN
                            .replace("{0}", &name_text),
                        diagnostic_codes::A_TYPE_PREDICATE_CANNOT_REFERENCE_ELEMENT_IN_A_BINDING_PATTERN,
                    );
                    return (return_type, None);
                }
                self.ctx.error(
                    error_node.pos,
                    error_node.end.saturating_sub(error_node.pos),
                    diagnostic_messages::CANNOT_FIND_PARAMETER.replace("{0}", &name_text),
                    diagnostic_codes::CANNOT_FIND_PARAMETER,
                );
                return (return_type, None);
            }
            if let Some(index) = parameter_index
                && params.get(index).is_some_and(|param| param.rest)
            {
                if self.ctx.has_parse_errors {
                    return (return_type, None);
                }
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                let error_node = self.ctx.arena.get(data.parameter_name).unwrap_or(node);
                self.ctx.error(
                    error_node.pos,
                    error_node.end.saturating_sub(error_node.pos),
                    diagnostic_messages::A_TYPE_PREDICATE_CANNOT_REFERENCE_A_REST_PARAMETER
                        .to_string(),
                    diagnostic_codes::A_TYPE_PREDICATE_CANNOT_REFERENCE_A_REST_PARAMETER,
                );
                return (return_type, None);
            }
        }

        let predicate = signature_query::type_predicate(
            data.asserts_modifier,
            target,
            type_id,
            parameter_index,
        );

        (return_type, Some(predicate))
    }

    /// Recursively find a type predicate node within a type node (e.g., inside parentheses or intersections).
    fn find_type_predicate_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::TYPE_PREDICATE => Some(node_idx),
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                let wrapped = self.ctx.arena.get_wrapped_type(node)?;
                self.find_type_predicate_node(wrapped.type_node)
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                let composite = self.ctx.arena.get_composite_type(node)?;
                for &member in &composite.types.nodes {
                    if let Some(found) = self.find_type_predicate_node(member) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// True when `name_text` is bound by an element of one of `raw_params`'
    /// destructuring binding patterns (any nesting depth, object or array,
    /// including renamed and rest elements) — checked only after `name_text`
    /// has already failed to match a top-level parameter name.
    fn type_predicate_name_matches_binding_element(
        &self,
        raw_params: &[NodeIndex],
        name_text: &str,
    ) -> bool {
        raw_params.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                .is_some_and(|param| self.binding_pattern_contains_name(param.name, name_text))
        })
    }

    /// Recursively search a binding pattern for an identifier bound to
    /// `name_text`. `node_idx` may be a plain identifier (no match, not a
    /// pattern), an object/array binding pattern, or `NONE`.
    fn binding_pattern_contains_name(&self, node_idx: NodeIndex, name_text: &str) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if !node.is_binding_pattern() {
            return false;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(node) else {
            return false;
        };
        pattern.elements.nodes.iter().any(|&el_idx| {
            let Some(el_name) = self
                .ctx
                .arena
                .get(el_idx)
                .and_then(|el_node| self.ctx.arena.get_binding_element(el_node))
                .map(|el| el.name)
            else {
                return false;
            };
            let is_direct_match = self
                .ctx
                .arena
                .get(el_name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text.as_str() == name_text);
            is_direct_match || self.binding_pattern_contains_name(el_name, name_text)
        })
    }
}

/// Parameter-declaration node indices backing a `SignatureData`'s optional
/// parameter list, or `&[]` when absent. Lets call sites that only have a
/// `sig.parameters: Option<NodeList>` pass raw nodes to
/// [`CheckerState::return_type_and_predicate`] /
/// [`CheckerState::return_type_and_predicate_in_type_literal`] without
/// repeating the `Option` unwrap at every site.
pub(crate) fn signature_param_nodes(list: &Option<tsz_parser::parser::NodeList>) -> &[NodeIndex] {
    list.as_ref().map_or(&[], |l| l.nodes.as_slice())
}
