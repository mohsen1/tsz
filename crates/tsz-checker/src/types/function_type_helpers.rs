//! Function type resolution helpers: JSDoc type predicates, enclosing type
//! parameter resolution, arguments object detection, contextual rest
//! parameter evaluation, and async/return completeness checks.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{FunctionShape, ParamInfo, TypeId, TypeParamInfo};

/// Context for TS2366/TS2355/TS7030 function return completeness checks.
pub(crate) struct FunctionReturnCheckCtx {
    /// Whether this is a function declaration (checked separately).
    pub(crate) is_function_declaration: bool,
    /// The function body node.
    pub(crate) body: NodeIndex,
    /// The function node itself.
    pub(crate) func_idx: NodeIndex,
    /// The annotated return type, if any.
    pub(crate) annotated_return_type: Option<TypeId>,
    /// The inferred or annotated return type.
    pub(crate) return_type: TypeId,
    /// Whether an explicit return type annotation is present.
    pub(crate) has_type_annotation: bool,
    /// The type annotation node (used as error anchor).
    pub(crate) type_annotation: NodeIndex,
    /// Whether this function is a generator.
    pub(crate) function_is_generator: bool,
    /// Optional name node for TS7030 (implicit return) anchoring.
    pub(crate) name_node: Option<NodeIndex>,
    /// The overall expression/declaration index used for diagnostics.
    pub(crate) idx: NodeIndex,
}

pub(crate) struct FunctionFinalReturnTypeCtx {
    pub(crate) has_type_annotation: bool,
    pub(crate) function_is_async: bool,
    pub(crate) function_is_generator: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) final_generator_yield_type: Option<TypeId>,
    pub(crate) early_gen_return_type: Option<TypeId>,
    pub(crate) early_gen_next_type: Option<TypeId>,
}

pub(crate) struct GeneratorBodyReturnCheckCtx<'b> {
    pub(crate) is_generator: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) type_annotation: NodeIndex,
    pub(crate) idx: NodeIndex,
    pub(crate) function_is_async: bool,
    pub(crate) early_yield_type: Option<TypeId>,
    pub(crate) name_node: Option<NodeIndex>,
    pub(crate) name_for_error: Option<&'b str>,
}

pub(crate) struct FunctionBodyReturnTypeCtx {
    pub(crate) idx: NodeIndex,
    pub(crate) is_generator: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) type_annotation: NodeIndex,
    pub(crate) is_async_for_context: bool,
    pub(crate) has_contextual_return: bool,
    pub(crate) contextual_void_return_exception: bool,
    pub(crate) return_context_for_circularity: Option<TypeId>,
    pub(crate) jsdoc_return_context: Option<TypeId>,
    pub(crate) early_gen_return_type: Option<TypeId>,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn function_contextual_type_context(
        &mut self,
        idx: NodeIndex,
        contextual_type: Option<TypeId>,
        is_function_declaration: bool,
        is_closure: bool,
    ) -> (
        Option<TypeId>,
        Option<Vec<TypeParamInfo>>,
        Option<FunctionShape>,
        bool,
    ) {
        if let Some(ctx_type) = contextual_type {
            use crate::query_boundaries::type_checking_utilities::{
                EvaluationNeeded, classify_for_evaluation, lazy_def_id, type_application,
            };

            let preserve_raw_mixed_context =
                crate::query_boundaries::common::union_members(self.ctx.types, ctx_type)
                    .is_some_and(|members| {
                        let has_callable = members.iter().any(|&member| {
                            crate::query_boundaries::common::is_callable_type(
                                self.ctx.types,
                                member,
                            )
                        });
                        let has_non_callable = members.iter().any(|&member| {
                            !crate::query_boundaries::common::is_callable_type(
                                self.ctx.types,
                                member,
                            )
                        });
                        has_callable && has_non_callable
                    });
            let preserve_raw_signature_context =
                preserve_raw_mixed_context || self.raw_contextual_signature_available(ctx_type);

            let evaluated_type = if preserve_raw_signature_context {
                ctx_type
            } else if type_application(self.ctx.types, ctx_type).is_some() {
                self.evaluate_application_type(ctx_type)
            } else if let Some(def_id) = lazy_def_id(self.ctx.types, ctx_type) {
                self.resolve_and_insert_def_type(def_id)
                    .unwrap_or_else(|| self.judge_evaluate(ctx_type))
            } else if matches!(
                classify_for_evaluation(self.ctx.types, ctx_type),
                EvaluationNeeded::IndexAccess { .. } | EvaluationNeeded::KeyOf(..)
            ) {
                self.judge_evaluate(ctx_type)
            } else {
                self.evaluate_contextual_type(ctx_type)
            };
            // Preserve original when evaluation degrades to UNKNOWN (unresolved conditionals).
            let evaluated_type = if evaluated_type == TypeId::UNKNOWN {
                ctx_type
            } else {
                evaluated_type
            };

            let evaluated_type = self.evaluate_contextual_rest_param_applications(evaluated_type);
            let contextual_signature_shape =
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    evaluated_type,
                );
            let evaluated_type = if preserve_raw_signature_context {
                evaluated_type
            } else {
                self.normalize_contextual_signature_with_env(evaluated_type)
            };
            let helper_probe = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                evaluated_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            let evaluated_type = if helper_probe.get_this_type().is_none()
                && helper_probe.get_return_type().is_none()
                && helper_probe.get_parameter_type(0).is_none()
                && helper_probe.get_rest_parameter_type(0).is_none()
                && !crate::query_boundaries::common::is_union_type(self.ctx.types, evaluated_type)
                && !crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    evaluated_type,
                ) {
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    evaluated_type,
                )
                .map(|shape| self.ctx.types.factory().function(shape))
                .unwrap_or(evaluated_type)
            } else {
                evaluated_type
            };

            return (
                Some(evaluated_type),
                self.contextual_type_params_from_expected(evaluated_type),
                contextual_signature_shape,
                false,
            );
        }

        if self.is_js_file() && (is_function_declaration || is_closure) {
            // In JS/checkJs, JSDoc `@type {FunctionType}` can live either on a
            // function declaration or on an enclosing variable statement for a
            // function expression (`const f = function() {}`), so support both.
            if let Some(evaluated_type) = self.jsdoc_callable_type_annotation_for_function(idx) {
                return (
                    Some(evaluated_type),
                    self.contextual_type_params_from_expected(evaluated_type),
                    None,
                    true,
                );
            }

            if is_closure
                && let Some(evaluated_type) = self.jsdoc_callable_type_annotation_for_node(idx)
            {
                return (
                    Some(evaluated_type),
                    self.contextual_type_params_from_expected(evaluated_type),
                    None,
                    true,
                );
            }
        }

        (None, None, None, false)
    }

    pub(crate) fn check_generator_body_return(
        &mut self,
        ctx: GeneratorBodyReturnCheckCtx<'_>,
    ) -> Option<TypeId> {
        if !ctx.is_generator {
            return None;
        }

        if ctx.has_type_annotation {
            let declared_type = ctx.annotated_return_type.unwrap_or(ctx.return_type);
            let yield_t = self.ctx.current_yield_type();
            let error_node = if ctx.type_annotation != NodeIndex::NONE {
                ctx.type_annotation
            } else {
                ctx.idx
            };
            self.check_generator_return_type_assignability(
                ctx.function_is_async,
                yield_t,
                declared_type,
                error_node,
            );
            return None;
        }

        let yield_types = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let inferred_yield = if yield_types.is_empty() {
            TypeId::NEVER
        } else {
            self.ctx.types.factory().union(yield_types)
        };
        let widened = if ctx.early_yield_type.is_some() {
            inferred_yield
        } else {
            self.widen_literal_type(inferred_yield)
        };
        let final_yield = if !self.ctx.strict_null_checks()
            && crate::query_boundaries::common::is_only_null_or_undefined(self.ctx.types, widened)
        {
            TypeId::ANY
        } else {
            widened
        };

        if final_yield == TypeId::ANY
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
            && !self.ctx.generator_had_ts7057
            && ctx.early_yield_type.is_none()
        {
            use crate::diagnostics::diagnostic_codes;
            if let Some(name) = ctx.name_for_error {
                self.error_at_node_msg(
                    ctx.name_node.unwrap_or(ctx.idx),
                    diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_YIELD_TYPE,
                    &[name, "any"],
                );
            } else {
                self.error_at_node_msg(
                    ctx.idx,
                    diagnostic_codes::GENERATOR_IMPLICITLY_HAS_YIELD_TYPE_CONSIDER_SUPPLYING_A_RETURN_TYPE_ANNOTATION,
                    &["any"],
                );
            }
        }

        Some(final_yield)
    }

    pub(crate) fn function_body_return_type(&mut self, ctx: FunctionBodyReturnTypeCtx) -> TypeId {
        let body_return_type = if ctx.is_generator && ctx.has_type_annotation {
            self.annotated_generator_body_return_type(&ctx)
        } else if ctx.is_async_for_context && ctx.has_type_annotation {
            let original_type = ctx.annotated_return_type.unwrap_or(ctx.return_type);
            self.unwrap_promise_type(original_type)
                .unwrap_or(ctx.return_type)
        } else if ctx.is_async_for_context
            && ctx.has_contextual_return
            && ctx
                .return_context_for_circularity
                .is_some_and(|t| t != TypeId::VOID && t != TypeId::ANY && t != TypeId::UNKNOWN)
        {
            ctx.return_context_for_circularity
                .expect("is_some_and guard ensures Some")
        } else if ctx.is_async_for_context
            && ctx.has_contextual_return
            && ctx.return_context_for_circularity == Some(TypeId::VOID)
        {
            TypeId::ANY
        } else if ctx.is_async_for_context {
            self.unwrap_async_return_type_for_body(ctx.return_type)
        } else if ctx.contextual_void_return_exception {
            TypeId::ANY
        } else if ctx.is_generator
            && !ctx.has_type_annotation
            && ctx.has_contextual_return
            && let Some(early_t) = ctx.early_gen_return_type
            && early_t != TypeId::ANY
            && early_t != TypeId::UNKNOWN
        {
            early_t
        } else if ctx.has_type_annotation
            || ctx.has_contextual_return
            || ctx.jsdoc_return_context.is_some()
        {
            self.sync_function_body_return_type(&ctx)
        } else {
            TypeId::ANY
        };

        self.substitute_direct_this_body_return_type(&ctx, body_return_type)
    }

    fn annotated_generator_body_return_type(&mut self, ctx: &FunctionBodyReturnTypeCtx) -> TypeId {
        let original_type = ctx.annotated_return_type.unwrap_or(ctx.return_type);
        if original_type == TypeId::VOID || ctx.return_type == TypeId::VOID {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                ctx.type_annotation,
                "A generator cannot have a 'void' type annotation.",
                diagnostic_codes::A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION,
            );
            return TypeId::ANY;
        }

        self.get_generator_return_type_argument(original_type)
            .unwrap_or(ctx.return_type)
    }

    fn sync_function_body_return_type(&mut self, ctx: &FunctionBodyReturnTypeCtx) -> TypeId {
        let has_direct_callable_jsdoc = !ctx.is_async_for_context
            && !ctx.is_generator
            && ctx.has_contextual_return
            && !ctx.has_type_annotation
            && ctx.jsdoc_return_context.is_none()
            && self
                .jsdoc_callable_type_annotation_for_node_direct(ctx.idx)
                .is_some();
        let sync_ctx = has_direct_callable_jsdoc
            .then_some(ctx.return_context_for_circularity)
            .flatten()
            .filter(|&t| t != TypeId::ANY && t != TypeId::UNKNOWN);
        sync_ctx.unwrap_or_else(|| ctx.annotated_return_type.unwrap_or(ctx.return_type))
    }

    fn substitute_direct_this_body_return_type(
        &mut self,
        ctx: &FunctionBodyReturnTypeCtx,
        body_return_type: TypeId,
    ) -> TypeId {
        if !(ctx.has_type_annotation || ctx.jsdoc_return_context.is_some())
            || !crate::query_boundaries::common::is_this_type(self.ctx.types, body_return_type)
        {
            return body_return_type;
        }

        if let Some(concrete_this) = self.current_this_type() {
            crate::query_boundaries::common::substitute_this_type(
                self.ctx.types,
                body_return_type,
                concrete_this,
            )
        } else {
            body_return_type
        }
    }

    pub(crate) fn implicit_function_this_type(
        &mut self,
        idx: NodeIndex,
        is_arrow_function: bool,
        outer_this_type: Option<TypeId>,
        explicit_this_type: Option<TypeId>,
        contextual_this_type: Option<TypeId>,
        js_constructor_instance_type: Option<TypeId>,
        js_prototype_owner_instance_type: Option<TypeId>,
    ) -> Option<TypeId> {
        let implicit_this = if is_arrow_function {
            outer_this_type
        } else {
            explicit_this_type
                .or(contextual_this_type)
                .or(js_constructor_instance_type)
                .or(js_prototype_owner_instance_type)
                .or_else(|| self.assignment_receiver_this_type(idx))
        };

        implicit_this.map(|this_type| self.resolve_lazy_type(this_type))
    }

    fn assignment_receiver_this_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let mut current = idx;
        for _ in 0..3 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
                    break;
                };
                if binary.right == current && self.is_assignment_operator(binary.operator_token) {
                    return self.this_type_from_assignment_left(binary.left);
                }
                break;
            }
            if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent;
                continue;
            }
            break;
        }
        None
    }

    fn this_type_from_assignment_left(&mut self, left: NodeIndex) -> Option<TypeId> {
        let left_node = self.ctx.arena.get(left)?;
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && left_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(left_node)?;
        if let Some(instance_type) = self.prototype_assignment_instance_type(access.expression) {
            return Some(instance_type);
        }
        let receiver = self.get_type_of_node(access.expression);
        (receiver != TypeId::ERROR).then_some(receiver)
    }

    fn prototype_assignment_instance_type(&mut self, expr: NodeIndex) -> Option<TypeId> {
        let proto_node = self.ctx.arena.get(expr)?;
        if proto_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && proto_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let proto_access = self.ctx.arena.get_access_expr(proto_node)?;
        let proto_name_node = self.ctx.arena.get(proto_access.name_or_argument)?;
        let proto_ident = self.ctx.arena.get_identifier(proto_name_node)?;
        if proto_ident.escaped_text != "prototype" {
            return None;
        }
        let constructor_type = self.get_type_of_node(proto_access.expression);
        self.synthesize_js_constructor_instance_type(proto_access.expression, constructor_type, &[])
    }

    pub(crate) fn append_js_arguments_rest_param(
        &mut self,
        body: NodeIndex,
        params: &mut Vec<ParamInfo>,
    ) {
        // In JS files, functions that reference `arguments` accept any number
        // of extra arguments. Pre-walk the body as a fallback for call sites
        // that compute function types before body checking updates the flag.
        let uses_arguments =
            self.ctx.js_body_uses_arguments || self.body_has_arguments_reference(body);
        if self.is_js_file() && uses_arguments && !params.last().is_some_and(|p| p.rest) {
            params.push(ParamInfo {
                name: None,
                type_id: self.ctx.types.factory().array(TypeId::ANY),
                optional: true,
                rest: true,
            });
        }
    }

    pub(crate) fn final_function_return_type(&mut self, ctx: FunctionFinalReturnTypeCtx) -> TypeId {
        let mut final_return_type = if !ctx.has_type_annotation && ctx.function_is_generator {
            self.unannotated_generator_return_type(&ctx)
        } else {
            ctx.annotated_return_type.unwrap_or(ctx.return_type)
        };

        if !ctx.has_type_annotation && ctx.function_is_async && !ctx.function_is_generator {
            final_return_type = self.wrap_unannotated_async_return_type(final_return_type);
        }

        final_return_type
    }

    fn unannotated_generator_return_type(&mut self, ctx: &FunctionFinalReturnTypeCtx) -> TypeId {
        let gen_name = if ctx.function_is_async {
            "AsyncGenerator"
        } else {
            "Generator"
        };
        let _resolved = self.resolve_lib_type_by_name(gen_name);
        let lazy_base = self.ctx.binder.file_locals.get(gen_name).map(|sym_id| {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx.types.factory().lazy(def_id)
        });
        let Some(base) = lazy_base else {
            return TypeId::ANY;
        };

        let yield_t = ctx.final_generator_yield_type.unwrap_or(TypeId::ANY);
        let body_return_t = self.unannotated_generator_body_return_type(ctx);
        let return_t = body_return_t
            .or(ctx.early_gen_return_type)
            .unwrap_or(TypeId::VOID);
        let next_t = ctx.early_gen_next_type.unwrap_or(TypeId::UNKNOWN);

        self.ctx
            .types
            .factory()
            .application(base, vec![yield_t, return_t, next_t])
    }

    fn unannotated_generator_body_return_type(
        &mut self,
        ctx: &FunctionFinalReturnTypeCtx,
    ) -> Option<TypeId> {
        let return_type = ctx.return_type;
        if return_type == TypeId::UNKNOWN
            || return_type == TypeId::VOID
            || return_type == TypeId::UNDEFINED
            || (return_type == TypeId::ANY && ctx.early_gen_return_type.is_some())
        {
            return None;
        }

        let contextual_pins_return = ctx
            .early_gen_return_type
            .is_some_and(|t| t != TypeId::VOID && t != TypeId::ANY && t != TypeId::UNKNOWN);
        let preserve = contextual_pins_return
            || crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, return_type);
        let widened = if preserve {
            return_type
        } else {
            self.widen_literal_type(return_type)
        };
        Some(widened)
    }

    fn wrap_unannotated_async_return_type(&mut self, mut return_type: TypeId) -> TypeId {
        let had_promise_wrapper = if let Some(inner) = self.unwrap_promise_type(return_type) {
            return_type = inner;
            true
        } else {
            false
        };
        if !had_promise_wrapper
            && !crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, return_type)
        {
            return_type = self.widen_literal_type(return_type);
        }
        let promise_base = self
            .ctx
            .lib_promise_type_ref()
            .unwrap_or(TypeId::PROMISE_BASE);
        self.ctx
            .types
            .factory()
            .application(promise_base, vec![return_type])
    }

    pub(crate) fn prewarm_inferred_predicate_operand_types(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        let mut stack = Vec::new();
        if body_node.kind == syntax_kind_ext::BLOCK {
            let Some(block) = self.ctx.arena.get_block(body_node) else {
                return;
            };
            let Some(&stmt_idx) = block.statements.nodes.last() else {
                return;
            };
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                return;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                return;
            }
            let Some(ret) = self.ctx.arena.get_return_statement(stmt_node) else {
                return;
            };
            if ret.expression.is_some() {
                stack.push(ret.expression);
            }
        } else {
            stack.push(body_idx);
        }

        while let Some(expr_idx) = stack.pop() {
            let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            match expr_node.kind {
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                        continue;
                    };
                    if binary.operator_token == SyntaxKind::InstanceOfKeyword as u16 {
                        self.get_type_of_node(binary.right);
                    } else if matches!(
                        binary.operator_token,
                        k if k == SyntaxKind::AmpersandAmpersandToken as u16
                            || k == SyntaxKind::BarBarToken as u16
                    ) {
                        stack.push(binary.left);
                        stack.push(binary.right);
                    }
                }
                syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    if let Some(unary) = self.ctx.arena.get_unary_expr(expr_node) {
                        stack.push(unary.operand);
                    }
                }
                syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(expr_node) {
                        stack.push(assertion.expression);
                    }
                }
                _ => {}
            }
        }
    }

    /// Extract a type predicate from JSDoc `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Parse JSDoc `@return` for type predicates and build `TypePredicate` with parameter index.
    pub(crate) fn extract_jsdoc_return_type_predicate(
        &mut self,
        func_jsdoc: &Option<String>,
        params: &[tsz_solver::ParamInfo],
    ) -> Option<tsz_solver::TypePredicate> {
        use tsz_solver::{TypePredicate, TypePredicateTarget};

        let jsdoc = func_jsdoc.as_ref()?;
        let (is_asserts, param_name, type_str) = Self::jsdoc_returns_type_predicate(jsdoc)?;

        // Build the target
        let target = if param_name == "this" {
            TypePredicateTarget::This
        } else {
            let atom = self.ctx.types.intern_string(&param_name);
            TypePredicateTarget::Identifier(atom)
        };

        // Resolve the type (if present)
        let type_id = type_str.and_then(|ts| self.resolve_jsdoc_type_str(&ts));

        // Find parameter index for identifier targets
        let mut parameter_index = None;
        if let TypePredicateTarget::Identifier(name) = &target {
            parameter_index = params.iter().position(|p| p.name == Some(*name));
        }

        Some(TypePredicate {
            asserts: is_asserts,
            target,
            type_id,
            parameter_index,
        })
    }

    /// Resolve a non-predicate JSDoc `@return {TypeExpr}` to a TypeId.
    ///
    /// This handles cases like `@return {false}`, `@return {void}`, `@return {number}`, etc.
    /// Returns `None` if no `@return` tag is found or the type expression can't be resolved.
    /// Type predicate returns (like `@return {x is string}`) are excluded.
    pub(crate) fn resolve_jsdoc_return_type(&mut self, jsdoc: &str) -> Option<TypeId> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let rest = rest.trim_start();
            if !rest.starts_with('{') {
                continue;
            }
            let after_open = &rest[1..];
            let end = after_open.find('}')?;
            let type_expr = after_open[..end].trim();
            if type_expr.is_empty() {
                return None;
            }
            // Skip type predicates — handled separately
            if Self::jsdoc_returns_type_predicate_from_type_expr(type_expr).is_some() {
                return None;
            }
            return self.resolve_jsdoc_reference(type_expr);
        }
        None
    }

    pub(crate) fn contextual_type_params_from_expected(
        &self,
        expected: TypeId,
    ) -> Option<Vec<TypeParamInfo>> {
        crate::query_boundaries::common::extract_contextual_type_params(self.ctx.types, expected)
    }

    pub(crate) fn push_contextual_type_parameter_infos(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> Vec<(String, Option<TypeId>, bool)> {
        let mut updates = Vec::with_capacity(type_params.len());
        let factory = self.ctx.types.factory();

        for info in type_params {
            let name = self.ctx.types.resolve_atom_ref(info.name).to_string();
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = factory.type_param(*info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
        }

        updates
    }

    /// Check if a function body references the `arguments` object.
    /// Walks the AST recursively but stops at nested function boundaries.
    /// Used by JS files to determine if a function needs an implicit rest parameter.
    pub(crate) fn body_has_arguments_reference(&self, body: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body) else {
            return false;
        };

        // Check if this node is an identifier named "arguments"
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "arguments";
        }

        // Stop at nested function/method/class boundaries
        let k = node.kind;
        if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION
            || k == syntax_kind_ext::METHOD_DECLARATION
            || k == syntax_kind_ext::CLASS_DECLARATION
            || k == syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }

        // Walk children based on node kind
        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt in &block.statements.nodes {
                if self.body_has_arguments_reference(stmt) {
                    return true;
                }
            }
        } else if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
            if self.body_has_arguments_reference(expr_stmt.expression) {
                return true;
            }
        } else if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
            for &decl in &var_stmt.declarations.nodes {
                if self.body_has_arguments_reference(decl) {
                    return true;
                }
            }
        } else if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if self.body_has_arguments_reference(var_decl.initializer) {
                return true;
            }
        } else if let Some(ret) = self.ctx.arena.get_return_statement(node) {
            if self.body_has_arguments_reference(ret.expression) {
                return true;
            }
        } else if let Some(call) = self.ctx.arena.get_call_expr(node) {
            if self.body_has_arguments_reference(call.expression) {
                return true;
            }
            if let Some(ref args) = call.arguments {
                for &arg in &args.nodes {
                    if self.body_has_arguments_reference(arg) {
                        return true;
                    }
                }
            }
        } else if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if self.body_has_arguments_reference(bin.left)
                || self.body_has_arguments_reference(bin.right)
            {
                return true;
            }
        } else if let Some(access) = self.ctx.arena.get_access_expr(node) {
            if self.body_has_arguments_reference(access.expression) {
                return true;
            }
            // Element access: also check the index expression (e.g. obj[arguments]).
            // Property names like `holder.arguments` are not references to the
            // function's implicit `arguments` object.
            if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && self.body_has_arguments_reference(access.name_or_argument)
            {
                return true;
            }
        } else if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
            if self.body_has_arguments_reference(if_stmt.expression)
                || self.body_has_arguments_reference(if_stmt.then_statement)
                || self.body_has_arguments_reference(if_stmt.else_statement)
            {
                return true;
            }
        } else if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            if self.body_has_arguments_reference(loop_stmt.initializer)
                || self.body_has_arguments_reference(loop_stmt.condition)
                || self.body_has_arguments_reference(loop_stmt.incrementor)
                || self.body_has_arguments_reference(loop_stmt.statement)
            {
                return true;
            }
        } else if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
            if self.body_has_arguments_reference(for_in_of.expression)
                || self.body_has_arguments_reference(for_in_of.statement)
            {
                return true;
            }
        } else if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
            if self.body_has_arguments_reference(paren.expression) {
                return true;
            }
        } else if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
            if self.body_has_arguments_reference(unary.operand) {
                return true;
            }
        } else if let Some(unary_ex) = self.ctx.arena.get_unary_expr_ex(node) {
            if self.body_has_arguments_reference(unary_ex.expression) {
                return true;
            }
        } else if let Some(spread) = self.ctx.arena.get_spread(node) {
            if self.body_has_arguments_reference(spread.expression) {
                return true;
            }
        } else if let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            && (self.body_has_arguments_reference(cond.condition)
                || self.body_has_arguments_reference(cond.when_true)
                || self.body_has_arguments_reference(cond.when_false))
        {
            return true;
        }

        false
    }

    /// Push type parameters from all enclosing generic functions/classes/interfaces.
    pub(crate) fn push_enclosing_type_parameters(
        &mut self,
        func_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut enclosing_param_indices: Vec<Vec<NodeIndex>> = Vec::new();
        let mut current = func_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            let type_param_nodes: Option<Vec<NodeIndex>> = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent)
                        .and_then(|f| f.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent)
                        .and_then(|c| c.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent)
                    .and_then(|i| i.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent)
                    .and_then(|m| m.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                _ => None,
            };

            if let Some(indices) = type_param_nodes {
                enclosing_param_indices.push(indices);
            }

            current = parent_idx;
        }

        if enclosing_param_indices.is_empty() {
            return Vec::new();
        }

        let mut updates = Vec::new();
        let mut added_params: Vec<NodeIndex> = Vec::new();
        let factory = self.ctx.types.factory();

        // Pass 1: Add all type parameters to scope WITHOUT constraints
        for param_indices in enclosing_param_indices.into_iter().rev() {
            for param_idx in param_indices {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };

                let name = self
                    .ctx
                    .arena
                    .get(data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
                let atom = self.ctx.types.intern_string(&name);

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const,
                };
                let type_id = factory.type_param(info);

                let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                updates.push((name, previous, false));
                added_params.push(param_idx);
            }
        }

        // Pass 2: Resolve constraints now that all type parameters are in scope
        for param_idx in added_params {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            if data.constraint == NodeIndex::NONE {
                continue;
            }

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);

            let constraint_type = self.get_type_from_type_node(data.constraint);
            let constraint = (constraint_type != TypeId::ERROR).then_some(constraint_type);

            let is_const = self
                .ctx
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const,
            };
            let constrained_type_id = factory.type_param(info);
            self.ctx
                .type_parameter_scope
                .insert(name, constrained_type_id);
        }

        updates
    }

    /// Evaluate indirection (Application, typeof, lazy) in rest parameters of
    /// contextual function types so that downstream contextual-typing code can
    /// split the tuple across the callback's own parameters.
    ///
    /// Why: `(...args: typeof t2) => void` where `t2: [number, boolean, ...string[]]`
    /// needs to expose the tuple shape to `(a, b, c) => {}` param matching. When
    /// the outer context is preserved raw (#688), this helper is the only place
    /// that resolves the rest param — so it must handle more than just Application.
    pub(crate) fn evaluate_contextual_rest_param_applications(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common::{
            function_shape_for_type, is_generic_application, is_type_query_type, lazy_def_id,
        };

        let Some(shape) = function_shape_for_type(self.ctx.types, type_id) else {
            return type_id;
        };

        let Some(last_param) = shape.params.last() else {
            return type_id;
        };

        if !last_param.rest {
            return type_id;
        }

        let rest_tid = last_param.type_id;
        let needs_resolution = is_generic_application(self.ctx.types, rest_tid)
            || is_type_query_type(self.ctx.types, rest_tid)
            || lazy_def_id(self.ctx.types, rest_tid).is_some();
        if !needs_resolution {
            return type_id;
        }

        let evaluated_rest = self.evaluate_type_with_env(rest_tid);
        if evaluated_rest == rest_tid {
            return type_id;
        }

        // Create a new function shape with the evaluated rest param type
        let mut new_params = shape.params.clone();
        new_params
            .last_mut()
            .expect("new_params cloned from non-empty shape.params")
            .type_id = evaluated_rest;

        let new_shape = tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: new_params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        };

        self.ctx.types.function(new_shape)
    }

    /// TS2705/TS2468: Check that the Promise constructor is available for async functions.
    /// Emits TS2468 (program-level) and TS2705 when Promise is missing from loaded libs.
    pub(crate) fn check_async_promise_constructor_availability(
        &mut self,
        is_async: bool,
        is_generator: bool,
        is_function_declaration: bool,
        has_type_annotation: bool,
        async_node_idx: NodeIndex,
        func_idx: NodeIndex,
    ) {
        if !is_async || is_generator {
            return;
        }
        let should_check_promise_constructor = !is_function_declaration || has_type_annotation;
        let missing_promise = self.ctx.promise_constructor_diagnostics_required();
        if !(should_check_promise_constructor && missing_promise) {
            return;
        }

        // Find the `async` keyword position for error anchoring.
        // For async arrow functions (no name node), the node `pos` starts at
        // the first parameter, not the `async` keyword. We scan backward
        // in the source to locate the keyword.
        let async_keyword_span = if async_node_idx.is_none() {
            // Arrow function — scan backward from node start to find `async`
            self.ctx.arena.get(func_idx).and_then(|n| {
                let sf = self.ctx.arena.source_files.first()?;
                let text = sf.text.as_bytes();
                let node_pos = n.pos as usize;
                // Scan backward over whitespace to find end of `async`
                let mut end = node_pos;
                while end > 0 && text.get(end - 1).copied() == Some(b' ') {
                    end -= 1;
                }
                // Check that the 5 chars before `end` are "async"
                if end >= 5 && &text[end - 5..end] == b"async" {
                    Some((end as u32 - 5, 5u32))
                } else {
                    None
                }
            })
        } else {
            None
        };

        // TS2468: Cannot find global value 'Promise'.
        // tsc emits this as a program-level diagnostic (no file location).
        if !is_function_declaration {
            let message =
                format_message(diagnostic_messages::CANNOT_FIND_GLOBAL_VALUE, &["Promise"]);
            self.error_program_level(message, diagnostic_codes::CANNOT_FIND_GLOBAL_VALUE);
        }

        // TS2705: anchored at the `async` keyword
        if let Some((start, length)) = async_keyword_span {
            self.error_at_position(
                start,
                length,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        } else {
            let diagnostic_node = if async_node_idx.is_none() {
                func_idx
            } else {
                async_node_idx
            };
            self.error_at_node(
                diagnostic_node,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        }
    }

    /// TS2705/TS1055/TS1064: Check that an async function's return type annotation is Promise.
    /// Emits TS1055 (ES5) or TS1064 (ES6+) when the declared return type is not Promise<T>.
    pub(crate) fn check_async_return_type_is_promise(
        &mut self,
        has_type_annotation: bool,
        is_async: bool,
        is_generator: bool,
        return_type: TypeId,
        type_annotation: NodeIndex,
    ) {
        if !has_type_annotation || !is_async || is_generator {
            return;
        }
        use tsz_scanner::SyntaxKind;
        let should_emit = if self.is_global_promise_type(return_type) {
            // Return type is exactly the global Promise<T> - OK
            false
        } else if self.is_promise_type_through_alias(return_type) {
            // Return type is a type alias application that resolves to Promise
            // (e.g., `type MyPromise<T> = Promise<T>` with `declare var MyPromise: typeof Promise`).
            // The merged symbol prevents is_global_promise_type from recognizing it.
            false
        } else if self.return_type_annotation_is_exactly_promise(type_annotation) {
            // The declared annotation resolves to the lib Promise symbol. Some
            // evaluated Promise<T> forms lose the lazy base identity and arrive
            // as an Application over an object shape; tsc still accepts them.
            false
        } else if self.is_non_promise_application_type(return_type) {
            // Return type is an Application with a non-Promise base (e.g., MyPromise<T>).
            // TSC requires exactly Promise<T>, not subclasses.
            true
        } else if return_type != TypeId::ERROR {
            // Return type evaluated to a non-Application form (e.g., Object).
            // Fall back to strict syntactic check: only suppress TS1064 if the
            // annotation literally says `Promise<...>`. TSC uses `isReferenceToType`
            // which requires exactly the global Promise — not subclasses like
            // `MyPromise`, not qualified names like `X.MyPromise`, not type aliases.
            !self.return_type_annotation_is_exactly_promise(type_annotation)
        } else {
            // Return type is ERROR - use syntactic fallback
            // Check if the type annotation is a primitive keyword (never valid for async function)
            let type_node_result = self.ctx.arena.get(type_annotation);
            match type_node_result {
                Some(type_node) => {
                    // Primitives are definitely not valid async function return types
                    matches!(
                        type_node.kind as u32,
                        k if k == SyntaxKind::StringKeyword as u32
                            || k == SyntaxKind::NumberKeyword as u32
                            || k == SyntaxKind::BooleanKeyword as u32
                            || k == SyntaxKind::VoidKeyword as u32
                            || k == SyntaxKind::UndefinedKeyword as u32
                            || k == SyntaxKind::NullKeyword as u32
                            || k == SyntaxKind::NeverKeyword as u32
                            || k == SyntaxKind::ObjectKeyword as u32
                    )
                }
                None => false,
            }
        };
        if !should_emit {
            return;
        }
        use crate::context::ScriptTarget;
        // For ES5/ES3 targets, emit TS1055 instead of TS2705
        let is_es5_or_lower = matches!(
            self.ctx.compiler_options.target,
            ScriptTarget::ES3 | ScriptTarget::ES5
        );
        if is_es5_or_lower {
            let type_name = self.format_type(return_type);
            self.error_at_node(
                type_annotation,
                &format_message(
                    diagnostic_messages::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                    &[&type_name],
                ),
                diagnostic_codes::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
            );
        } else {
            // TS1064: For ES6+ targets, the return type must be Promise<T>
            let type_name = self
                .promise_like_return_type_argument(return_type)
                .map_or_else(
                    || self.format_type(return_type),
                    |inner| self.format_type(inner),
                );
            self.error_at_node(
                type_annotation,
                &format_message(
                    diagnostic_messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                    &[&type_name],
                ),
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        }
    }

    /// TS1064 for async functions in JS files with `@type {function(): ReturnType}`.
    ///
    /// When a variable in a JS file has `/** @type {function(): string} */` and the
    /// initializer is an async function, tsc emits TS1064 because `string` is not
    /// `Promise<string>`. The main `check_async_return_type_is_promise` only fires
    /// when there's an AST-level return type annotation, so this method handles the
    /// JSDoc-only case.
    pub(crate) fn check_async_return_type_from_jsdoc_type(
        &mut self,
        func_idx: NodeIndex,
        func_jsdoc: &Option<String>,
    ) {
        let Some(jsdoc) = func_jsdoc else {
            return;
        };
        let Some(ret_type_str) = Self::jsdoc_type_tag_function_return_type(jsdoc) else {
            return;
        };
        let trimmed = ret_type_str.trim();
        if Self::jsdoc_return_type_is_exact_promise_reference(trimmed)
            && self.jsdoc_promise_name_resolves_to_global(func_idx)
        {
            return;
        }

        let inner_type_name = trimmed;
        let sf = self.source_file_data_for_node(func_idx);
        let span = sf.and_then(|sf| {
            let source_text: &str = &sf.text;
            let comments = &sf.comments;
            let func_node = self.ctx.arena.get(func_idx)?;
            for comment in comments.iter().rev() {
                if comment.end <= func_node.pos {
                    if tsz_common::comments::is_jsdoc_comment(comment, source_text) {
                        return Self::jsdoc_type_tag_function_return_type_span_in_source(
                            source_text,
                            comment.pos,
                        );
                    }
                    break;
                }
            }
            self.try_jsdoc_with_ancestor_walk(func_idx, comments, source_text)
                .and_then(|_jsdoc_text| {
                    let mut current = func_idx;
                    for _ in 0..4 {
                        if let Some(ext) = self.ctx.arena.get_extended(current) {
                            let parent = ext.parent;
                            if parent.is_none() {
                                break;
                            }
                            if let Some(parent_node) = self.ctx.arena.get(parent) {
                                for comment in comments.iter().rev() {
                                    if (comment.end <= parent_node.pos
                                        || (comment.pos <= parent_node.pos
                                            && comment.end <= parent_node.end))
                                        && tsz_common::comments::is_jsdoc_comment(
                                            comment,
                                            source_text,
                                        ) {
                                            return Self::jsdoc_type_tag_function_return_type_span_in_source(
                                                source_text,
                                                comment.pos,
                                            );
                                        }
                                }
                                current = parent;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    None
                })
        });
        let msg = format_message(
            diagnostic_messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            &[inner_type_name],
        );
        if let Some((start, length)) = span {
            self.error_at_position(
                start,
                length,
                &msg,
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        } else {
            self.error_at_node(
                func_idx,
                &msg,
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        }
    }

    fn jsdoc_promise_name_resolves_to_global(&self, func_idx: NodeIndex) -> bool {
        if self.jsdoc_has_prior_promise_typedef(func_idx) {
            return false;
        }

        for sym_id in self
            .ctx
            .binder
            .current_scope
            .get("Promise")
            .into_iter()
            .chain(self.ctx.binder.file_locals.get("Promise"))
        {
            if !self.ctx.sym_id_is_lib_promise(sym_id)
                && !self.ctx.sym_id_is_current_cloned_lib_promise(sym_id)
            {
                return false;
            }
        }
        true
    }

    fn jsdoc_has_prior_promise_typedef(&self, func_idx: NodeIndex) -> bool {
        let Some(sf) = self.source_file_data_for_node(func_idx) else {
            return false;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        sf.comments.iter().any(|comment| {
            comment.end <= func_node.pos
                && tsz_common::comments::is_jsdoc_comment(comment, &sf.text)
                && Self::jsdoc_comment_declares_promise_typedef(
                    &sf.text[comment.pos as usize..comment.end as usize],
                )
        })
    }

    fn jsdoc_comment_declares_promise_typedef(comment: &str) -> bool {
        comment.contains("@typedef")
            && comment.split_whitespace().any(|token| {
                token.trim_matches(|c: char| {
                    matches!(c, '*' | '/' | '{' | '}' | '(' | ')' | '[' | ']' | ',')
                }) == "Promise"
            })
    }

    fn jsdoc_return_type_is_exact_promise_reference(trimmed: &str) -> bool {
        let Some(rest) = trimmed.strip_prefix("Promise") else {
            return false;
        };
        let rest = rest.trim_start();
        rest.is_empty() || rest.starts_with('<') || rest.starts_with(".<")
    }

    /// Check if a type is a type alias application that resolves to Promise.
    ///
    /// For example, `type PromiseAlias<T> = Promise<T>; async function f(): PromiseAlias<void>`
    /// -- the return type `PromiseAlias<void>` is an Application whose base is a type alias.
    /// This method resolves the alias body and checks if it references the global Promise type.
    ///
    /// This handles tsc's `isReferenceToType` semantics for TS1064, where type aliases
    /// that ultimately resolve to Promise<T> are accepted as valid async return types.
    /// It also handles merged symbols (e.g., `type MyPromise<T> = Promise<T>` combined
    /// with `declare var MyPromise: typeof Promise`) by finding the type alias declaration
    /// among the symbol's declarations.
    pub(crate) fn is_promise_type_through_alias(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::checkers::promise as query;
        use tsz_binder::symbol_flags;

        // Must be an Application type
        let query::PromiseTypeKind::Application { base, .. } =
            query::classify_promise_type(self.ctx.types, type_id)
        else {
            return false;
        };

        // Check if the base is a Lazy(DefId) pointing to a type alias
        let def_id = match query::classify_promise_type(self.ctx.types, base) {
            query::PromiseTypeKind::Lazy(def_id) => def_id,
            _ => return false,
        };

        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only handle type aliases (not classes/interfaces)
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        // Get the alias body type using type_reference_symbol_type_with_params which
        // correctly handles merged symbols (e.g., `type MyPromise<T> = Promise<T>`
        // merged with `declare var MyPromise: typeof Promise`). It finds the type
        // alias declaration in the symbol's declarations list.
        let (body_type, _params) = self.type_reference_symbol_type_with_params(sym_id);
        if self.is_global_promise_type(body_type) {
            return true;
        }

        // The body might itself be an Application (e.g., `Promise<T>`)
        // Check if the Application base refers to the global Promise type
        if let query::PromiseTypeKind::Application {
            base: body_base, ..
        } = query::classify_promise_type(self.ctx.types, body_type)
        {
            // Check if the body's base is Promise
            return self.is_global_promise_type(body_base);
        }

        false
    }

    /// TS2366/TS2355/TS7030: Check that all code paths return a value when required.
    /// For function expressions and arrow functions with return type annotations.
    pub(crate) fn check_function_return_completeness(&mut self, ctx: FunctionReturnCheckCtx) {
        let FunctionReturnCheckCtx {
            is_function_declaration,
            body,
            func_idx,
            annotated_return_type,
            return_type,
            has_type_annotation,
            type_annotation,
            function_is_generator,
            name_node,
            idx,
        } = ctx;
        if is_function_declaration || body.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        // Class methods and constructors have their return completeness checked
        // by ambient_signature_checks.rs during the class checking phase, where
        // enclosing_class is properly set. Skip them here to avoid false
        // positives during the type building phase when enclosing_class is not
        // yet available (needed for `this.method()` never-returning call detection).
        if node.kind == syntax_kind_ext::METHOD_DECLARATION
            || node.kind == syntax_kind_ext::CONSTRUCTOR
        {
            return;
        }
        // Determine if this is an async function or generator
        let (is_async, is_generator) = if let Some(func) = self.ctx.arena.get_function(node) {
            (func.is_async, func.asterisk_token)
        } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
            (
                self.has_async_modifier(&method.modifiers),
                method.asterisk_token,
            )
        } else {
            (false, false)
        };
        let effective_return_type = annotated_return_type.unwrap_or(return_type);
        let mut check_return_type = self.return_type_for_implicit_return_check(
            effective_return_type,
            is_async,
            is_generator,
        );
        // For async functions, if we couldn't unwrap Promise<T> (e.g. lib files not loaded),
        // fall back to the annotation syntax. If it looks like Promise<...>, suppress TS2355.
        if is_async
            && check_return_type == effective_return_type
            && has_type_annotation
            && self.return_type_annotation_looks_like_promise(type_annotation)
        {
            check_return_type = TypeId::VOID;
        }
        let requires_return = self.requires_return_value(check_return_type);
        let has_return = self.body_has_return_with_value(body);
        let falls_through = self.function_body_falls_through(body);
        if has_type_annotation
            && requires_return
            && falls_through
            && check_return_type != TypeId::VOID
        {
            if !has_return {
                self.error_at_node(
                    type_annotation,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else {
                // TS2366: always emit when return type doesn't include undefined
                self.error_at_node(
                    type_annotation,
                    diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
            }
        } else if self.ctx.no_implicit_returns() && has_return && falls_through {
            // TS7030: noImplicitReturns - not all code paths return a value
            // TSC skips TS7030 for functions returning void, any, or unions containing void/any
            let ts7030_check_type = self.return_type_for_implicit_return_check(
                annotated_return_type.unwrap_or(return_type),
                is_async,
                function_is_generator,
            );
            if !self.should_skip_no_implicit_return_check(
                ts7030_check_type,
                has_type_annotation,
                function_is_generator,
            ) {
                // TSC points TS7030 to: return type annotation > function name > node itself
                let error_node = if has_type_annotation {
                    type_annotation
                } else if let Some(nn) = name_node {
                    nn
                } else {
                    idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }
        }
    }

    /// Check if a return context type is or references a const type parameter.
    /// Used to propagate const context into callback bodies during generic inference.
    pub(crate) fn return_context_has_const_type_param(&self, ret_ctx: TypeId) -> bool {
        // Direct check: is the return context itself a const type parameter?
        if let Some(tp_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, ret_ctx)
            && tp_info.is_const
        {
            return true;
        }

        // General check: does the return context reference any const type parameter?
        let referenced =
            crate::query_boundaries::common::collect_referenced_types(self.ctx.types, ret_ctx);
        referenced.into_iter().any(|ty| {
            crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
                .is_some_and(|info| info.is_const)
        })
    }

    pub(crate) fn class_property_arrow_lexical_this_type(
        &mut self,
        arrow_idx: NodeIndex,
    ) -> Option<TypeId> {
        let (property_idx, class_idx) = self.class_property_arrow_owner(arrow_idx)?;
        let property_node = self.ctx.arena.get(property_idx)?;
        let prop = self.ctx.arena.get_property_decl(property_node)?;
        let class_node = self.ctx.arena.get(class_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;
        let is_static = self.has_static_modifier(&prop.modifiers);

        // When the arrow function is itself currently being typed, the arrow
        // node is on `node_resolution_stack`. Triggering a fresh class instance
        // (or constructor) type build here would recursively re-enter
        // `get_class_instance_type_inner`, which in turn calls
        // `get_type_of_node(prop.initializer)` for this same arrow; that
        // re-entry hits the circular-reference guard and poisons the cached
        // class shape. Use the already-cached class type or the enclosing-class
        // snapshot instead.
        if self.ctx.node_resolution_stack.contains(&arrow_idx) {
            let cache = if is_static {
                &self.ctx.class_constructor_type_cache
            } else {
                &self.ctx.class_instance_type_cache
            };
            return cache.get(&class_idx).copied().or_else(|| {
                if is_static {
                    return None;
                }
                self.ctx
                    .enclosing_class
                    .as_ref()
                    .filter(|info| info.class_idx == class_idx)
                    .and_then(|info| info.cached_instance_this_type)
            });
        }

        Some(if is_static {
            self.get_class_constructor_type(class_idx, class_data)
        } else {
            self.get_class_instance_type(class_idx, class_data)
        })
    }

    fn class_property_arrow_owner(&self, arrow_idx: NodeIndex) -> Option<(NodeIndex, NodeIndex)> {
        let mut current = arrow_idx;
        for _ in 0..16 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;

            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let class_idx = self.ctx.arena.get_extended(parent)?.parent;
                let class_node = self.ctx.arena.get(class_idx)?;
                if class_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && class_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    return None;
                }
                return Some((parent, class_idx));
            }

            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent_node.kind == syntax_kind_ext::CONSTRUCTOR
            {
                return None;
            }

            current = parent;
        }

        None
    }
}
