//! Call/construct signature building (parameter extraction, instantiation, return types).

use crate::state::{CheckerState, ParamTypeResolutionMode};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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
    ) -> tsz_solver::CallSignature {
        // Push enclosing type parameters so that overload signatures can reference
        // type parameters from outer function/class/interface scopes.
        let enclosing_updates = self.push_enclosing_type_parameters(func_idx);

        let (type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&func.parameters);
        let (return_type, type_predicate) =
            self.return_type_and_predicate(func.type_annotation, &params);

        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_updates);

        tsz_solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: false,
        }
    }

    /// Build a `CallSignature` from a method declaration.
    pub(crate) fn call_signature_from_method(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        method_idx: NodeIndex,
    ) -> tsz_solver::CallSignature {
        self.call_signature_from_method_with_this(method, None, method_idx)
    }

    /// Build a `CallSignature` from a method declaration with an explicit `this` type.
    /// This is used for static methods where `this` refers to the constructor type.
    pub(crate) fn call_signature_from_method_with_this(
        &mut self,
        method: &tsz_parser::parser::node::MethodDeclData,
        explicit_this_type: Option<TypeId>,
        method_idx: NodeIndex,
    ) -> tsz_solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let (mut return_type, mut type_predicate) =
            if method.type_annotation.is_none() && method.body.is_some() {
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
                self.return_type_and_predicate(method.type_annotation, &params)
            };

        // Check JSDoc @returns for type predicates on class methods.
        // Mirrors the logic in get_type_of_function (function_type.rs) for standalone
        // functions. In JS files, method return type predicates like
        // `@return {this is Entry}` are specified via JSDoc instead of syntax.
        if type_predicate.is_none() {
            let method_jsdoc = self.find_jsdoc_for_function(method_idx);
            if let Some(pred) = self.extract_jsdoc_return_type_predicate(&method_jsdoc, &params) {
                return_type = if pred.asserts {
                    TypeId::VOID
                } else {
                    TypeId::BOOLEAN
                };
                type_predicate = Some(pred);
            }
        }

        // Wrap unannotated generator/async method return types (matching get_type_of_function).
        let has_annotation = method.type_annotation.is_some();
        let is_generator = method.asterisk_token;
        let is_async = self.has_async_modifier(&method.modifiers);

        if !has_annotation && is_generator {
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
        } else if !has_annotation && is_async {
            if let Some(inner) = self.unwrap_promise_type(return_type) {
                return_type = inner;
            }
            let promise_base = {
                let lib_binders = self.get_lib_binders();
                self.ctx
                    .binder
                    .get_global_type_with_libs("Promise", &lib_binders)
                    .map(|sym_id| self.ctx.create_lazy_type_ref(sym_id))
                    .unwrap_or(TypeId::PROMISE_BASE)
            };
            return_type = self
                .ctx
                .types
                .factory()
                .application(promise_base, vec![return_type]);
        }

        self.pop_type_parameters(type_param_updates);

        tsz_solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: true,
        }
    }

    /// Build a `CallSignature` from a constructor declaration.
    pub(crate) fn call_signature_from_constructor(
        &mut self,
        ctor: &tsz_parser::parser::node::ConstructorData,
        ctor_idx: NodeIndex,
        instance_type: TypeId,
        class_type_params: &[tsz_solver::TypeParamInfo],
    ) -> tsz_solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&ctor.type_parameters);
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

        tsz_solver::CallSignature {
            type_params: all_type_params,
            params,
            this_type,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }
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
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use tsz_solver::ParamInfo;

        let substitution = TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
        let params: Vec<ParamInfo> = sig
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(self.ctx.types, param.type_id, &substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();

        let this_type = sig
            .this_type
            .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution));
        let return_type = instantiate_type(self.ctx.types, sig.return_type, &substitution);
        let type_predicate =
            sig.type_predicate
                .as_ref()
                .map(|predicate| tsz_solver::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target,
                    type_id: predicate
                        .type_id
                        .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution)),
                    parameter_index: predicate.parameter_index,
                });

        tsz_solver::CallSignature {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: sig.is_method,
        }
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
        use tsz_solver::ParamInfo;

        let mut params = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Resolve parameter type based on mode
            let type_id = if param.type_annotation.is_some() {
                match mode {
                    ParamTypeResolutionMode::InTypeLiteral => {
                        self.get_type_from_type_node_in_type_literal(param.type_annotation)
                    }
                    ParamTypeResolutionMode::FromTypeNode => {
                        self.get_type_from_type_node(param.type_annotation)
                    }
                }
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

            params.push(ParamInfo {
                name,
                type_id,
                optional,
                rest,
            });
        }

        (params, this_type)
    }

    // =========================================================================
    // Return Type and Type Predicate
    // =========================================================================

    /// Extract return type and type predicate from a type annotation (declaration context).
    pub(crate) fn return_type_and_predicate(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        self.return_type_and_predicate_impl(type_annotation, params, false)
    }

    /// Extract return type and type predicate from a type literal annotation.
    pub(crate) fn return_type_and_predicate_in_type_literal(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        self.return_type_and_predicate_impl(type_annotation, params, true)
    }

    /// Shared implementation for return type + type predicate extraction.
    /// When `in_type_literal` is true, uses `get_type_from_type_node_in_type_literal`;
    /// otherwise uses `get_type_from_type_node`.
    fn return_type_and_predicate_impl(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
        in_type_literal: bool,
    ) -> (TypeId, Option<tsz_solver::TypePredicate>) {
        use tsz_solver::{TypePredicate, TypePredicateTarget};

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
        }

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
            parameter_index,
        };

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
}
