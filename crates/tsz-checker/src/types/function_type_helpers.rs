//! Function type resolution helpers: JSDoc type predicates, enclosing type
//! parameter resolution, arguments object detection, contextual rest
//! parameter evaluation, and async/return completeness checks.

use crate::computation::complex::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};
use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::construct_signatures as signature_construction;
use crate::query_boundaries::function_returns as return_type_construction;
use crate::query_boundaries::signature_building as signature_building_boundary;
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
    /// Intersection of the `TNext`s the body's `yield*` delegations declared.
    pub(crate) delegated_gen_next_type: Option<TypeId>,
}

/// What checking an unannotated generator's body inferred for its signature:
/// the aggregated yield type, and the `TNext` its `yield*` delegations imply.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InferredGeneratorYield {
    pub(crate) yield_type: Option<TypeId>,
    pub(crate) delegated_next_type: Option<TypeId>,
}

impl InferredGeneratorYield {
    /// Nothing inferred — the function is not an unannotated generator.
    pub(crate) const NONE: Self = Self {
        yield_type: None,
        delegated_next_type: None,
    };
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

pub(crate) struct ExpressionBodyReturnCheckCtx {
    pub(crate) idx: NodeIndex,
    pub(crate) body: NodeIndex,
    pub(crate) is_closure: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) is_async_for_context: bool,
    pub(crate) contextual_void_return_exception: bool,
    pub(crate) expected_expression_return_type: Option<TypeId>,
    pub(crate) jsdoc_return_context: Option<TypeId>,
}

struct DirectExpressionBodyReturnMismatchCtx {
    idx: NodeIndex,
    body: NodeIndex,
    expected_return_type: TypeId,
    actual_return: TypeId,
    actual_return_node: NodeIndex,
    actual_return_uses_jsdoc_cast: bool,
    is_closure: bool,
    is_async_for_context: bool,
    return_annotation: DirectReturnAnnotation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectReturnAnnotation {
    ContextualOnly,
    Declared,
}

impl DirectReturnAnnotation {
    const fn from_parts(has_type_annotation: bool, has_jsdoc_return: bool) -> Self {
        if has_type_annotation || has_jsdoc_return {
            Self::Declared
        } else {
            Self::ContextualOnly
        }
    }

    const fn is_declared(self) -> bool {
        matches!(self, Self::Declared)
    }
}

impl<'a> CheckerState<'a> {
    /// Whether `idx` is an object-literal method shorthand (e.g. `{ m() {} }`).
    /// The class-vs-object distinction for method declarations is decided by the
    /// parent node kind: a method's parent is the class or object-literal node
    /// directly. Object-literal methods are contextually typed by the enclosing
    /// object literal, so for implicit-any tracking they behave like arrow /
    /// function-expression property initializers even though they are not
    /// `is_closure` nodes.
    pub(crate) fn is_object_literal_method(&self, idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::METHOD_DECLARATION)
            && self
                .ctx
                .arena
                .get_extended(idx)
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .is_some_and(|parent| parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
    }

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
            // tsc's `getIntersectedSignatures` (the >1-signature branch of
            // `getContextualCallSignature`) returns undefined outright unless
            // `noImplicitAny` is on — an overloaded, non-generic callable target
            // contextually types nothing at all under a non-strict program, even
            // when the probe above found no per-position type through the
            // ordinary extractor. Synthesizing a merged mono `FunctionShape` here
            // regardless of `noImplicitAny` would reintroduce that same
            // cross-signature union the extractor now declines, just through this
            // fallback instead. A single-signature callable is unaffected: tsc
            // returns it unconditionally, so the merge below is a no-op there.
            let overloaded_without_no_implicit_any = !self.ctx.compiler_options.no_implicit_any
                && crate::query_boundaries::common::callable_shape_id(
                    self.ctx.types,
                    evaluated_type,
                )
                .is_some_and(|shape_id| {
                    self.ctx
                        .types
                        .callable_shape(shape_id)
                        .call_signatures
                        .len()
                        > 1
                });
            let evaluated_type = if !overloaded_without_no_implicit_any
                && helper_probe.get_this_type().is_none()
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
                .map(|shape| {
                    signature_construction::function_type_from_shape(self.ctx.types, shape)
                })
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

    /// Drain the collected `yield` contributions of the just-checked generator
    /// body and collapse them to the unwidened inferred yield type (`never`
    /// when the body has no yields). Shared by the yield-type aggregation here
    /// and the TS7055 implicit-any check in `function_declaration_checks.rs` so
    /// the two sites cannot drift on how contributions collapse.
    pub(crate) fn take_generator_yield_union(
        &mut self,
    ) -> (Vec<crate::context::GeneratorYieldContribution>, TypeId) {
        let contributions = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let inferred = if contributions.is_empty() {
            TypeId::NEVER
        } else {
            let types: Vec<TypeId> = contributions.iter().map(|c| c.type_id).collect();
            crate::query_boundaries::function_returns::function_return_union(self.ctx.types, types)
        };
        (contributions, inferred)
    }

    /// Intersect the `TNext` every `yield*` in the just-checked body
    /// contributed. `tsc`'s `checkAndAggregateYieldOperandTypes` pushes one
    /// `getIterationTypeOfIterable(IterationTypeKind.Next, ...)` per delegation
    /// and combines them with `getIntersectionType`, which is what a caller
    /// sending a value through the outer generator must satisfy: the value has
    /// to be acceptable to *every* delegate it can reach. `None` when no
    /// delegation declared a `TNext`, leaving the slot at its `unknown` default.
    fn delegated_generator_next_type(
        &self,
        contributions: &[crate::context::GeneratorYieldContribution],
    ) -> Option<TypeId> {
        let next_types: Vec<TypeId> = contributions
            .iter()
            .filter_map(|c| c.delegated_next_type)
            .collect();
        if next_types.is_empty() {
            return None;
        }
        Some(
            crate::query_boundaries::function_returns::function_return_intersection(
                self.ctx.types,
                next_types,
            ),
        )
    }

    pub(crate) fn check_generator_body_return(
        &mut self,
        ctx: GeneratorBodyReturnCheckCtx<'_>,
    ) -> InferredGeneratorYield {
        if !ctx.is_generator {
            return InferredGeneratorYield::NONE;
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
            return InferredGeneratorYield::NONE;
        }

        let (yield_contributions, inferred_yield) = self.take_generator_yield_union();
        let delegated_next_type = self.delegated_generator_next_type(&yield_contributions);
        // tsc's `getWidenedLiteralLikeTypeForContextualIterationTypeIfNeeded` +
        // `getWidenedType(getUnionType(...))`: widen only a union collapsed to a
        // single *fresh* literal (or enum member) that the contextual yield
        // type does not itself admit. A non-literal contextual yield type does
        // NOT suppress widening — with a `(a: T) => IterableIterator<T |
        // undefined, void>` contextual signature, `yield 1` still widens to
        // `number` (generatorTypeCheck63). Per-contribution freshness policy
        // lives in `yield_contribution_is_widenable`. The blanket
        // `widen_literal_type` is safe here (unlike the return path, which
        // needs `widen_return_contribution_preserving_const`): only unit
        // literals reach the widener, never object-literal shapes with
        // per-property `as const`.
        let is_widenable_literal =
            crate::query_boundaries::common::is_literal_type(self.ctx.types, inferred_yield)
                || self.is_enum_member_type_for_widening(inferred_yield);
        // Term order matters: `is_widenable_literal` is the cheap, selective
        // gate (an empty contribution set collapses to `never` and fails it);
        // the contribution scan runs only for a collapsed unit literal; and the
        // contextual gate runs last because it resolves lazy types in the
        // environment — evaluating it for every generator perturbs in-flight
        // inference state on speculative re-checks.
        let widened = if is_widenable_literal
            && yield_contributions.iter().all(|c| c.widenable)
            && !ctx.early_yield_type.is_some_and(|ctx_yield| {
                self.contextual_type_allows_literal(ctx_yield, inferred_yield)
            }) {
            let widened_literal = self.widen_literal_type(inferred_yield);
            self.widen_enum_member_type(widened_literal)
        } else {
            inferred_yield
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

        InferredGeneratorYield {
            yield_type: Some(final_yield),
            delegated_next_type,
        }
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

    pub(crate) fn check_expression_body_return_type(&mut self, ctx: ExpressionBodyReturnCheckCtx) {
        let Some(raw_expected_return_type) = ctx.expected_expression_return_type else {
            return;
        };
        let Some(body_node) = self.ctx.arena.get(ctx.body) else {
            return;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return;
        }

        let expected_return_type = if crate::query_boundaries::common::is_index_access_type(
            self.ctx.types,
            raw_expected_return_type,
        ) {
            let evaluated = self.evaluate_type_with_env(raw_expected_return_type);
            if evaluated != TypeId::ERROR {
                evaluated
            } else {
                raw_expected_return_type
            }
        } else {
            raw_expected_return_type
        };
        if expected_return_type == TypeId::ANY || self.type_contains_error(expected_return_type) {
            return;
        }

        let mut actual_return_node = ctx.body;
        let mut actual_return_uses_jsdoc_cast = false;
        let actual_return = (if let Some(ty) =
            self.jsdoc_type_annotation_for_node_direct(actual_return_node)
        {
            actual_return_uses_jsdoc_cast = true;
            Some(ty)
        } else {
            let mut found = None;
            while let Some(parent_idx) = self
                .ctx
                .arena
                .get_extended(actual_return_node)
                .map(|ext| ext.parent)
            {
                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                    break;
                };
                if parent_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                    break;
                }
                actual_return_node = parent_idx;
                if let Some(ty) = self.jsdoc_type_annotation_for_node_direct(actual_return_node) {
                    actual_return_uses_jsdoc_cast = true;
                    found = Some(ty);
                    break;
                }
            }
            found
        })
        .unwrap_or_else(|| {
            // A contextually sensitive body (e.g. a nested arrow/closure) can still
            // receive useful parameter context from an `expected_return_type` that
            // mentions free type parameters from an enclosing generic signature.
            // tsc applies the expected return type to such a body so the inner
            // closure's own parameters acquire their declared shapes (e.g. `set`
            // gets `(p: T) => void` rather than `any`). Only genuine `infer`
            // placeholders — which carry no usable shape yet — must block the
            // contextual body. Free type parameters do not, so gate on
            // `contains_infer_types` rather than the broader inference-hole check
            // (which also trips on any `contains_type_parameters`).
            let can_apply_contextual_body =
                !crate::query_boundaries::state::type_environment::contains_infer_types(
                    self.ctx.types,
                    expected_return_type,
                );
            let literal_sensitive_return = crate::query_boundaries::common::literal_value(
                self.ctx.types,
                expected_return_type,
            )
            .is_some()
                || crate::query_boundaries::common::enum_def_id(
                    self.ctx.types,
                    expected_return_type,
                )
                .is_some()
                || (crate::query_boundaries::common::is_symbol_or_unique_symbol(
                    self.ctx.types,
                    expected_return_type,
                ) && expected_return_type != TypeId::SYMBOL)
                || expected_return_type == TypeId::NEVER
                || crate::query_boundaries::common::union_list_id(
                    self.ctx.types,
                    expected_return_type,
                )
                .is_some_and(|list_id| {
                    self.ctx.types.type_list(list_id).iter().any(|&member| {
                        crate::query_boundaries::common::is_literal_type(self.ctx.types, member)
                            || crate::query_boundaries::common::enum_def_id(self.ctx.types, member)
                                .is_some()
                    })
                });
            let concrete_return_context = expected_return_type != TypeId::ANY
                && expected_return_type != TypeId::UNKNOWN
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    expected_return_type,
                );
            let keep_contextual_body = ctx.has_type_annotation
                || ctx.jsdoc_return_context.is_some()
                || literal_sensitive_return
                || (can_apply_contextual_body
                    && (is_contextually_sensitive(self, ctx.body)
                        || (concrete_return_context
                            && expression_needs_contextual_return_type(self, ctx.body))));
            let body_request = if keep_contextual_body {
                TypingRequest::with_contextual_type(expected_return_type)
            } else {
                TypingRequest::NONE
            };
            let prev_preserve_literals = self.ctx.preserve_literal_types;
            if keep_contextual_body {
                self.ctx.preserve_literal_types = true;
            }
            if body_request.is_empty() {
                self.invalidate_expression_for_contextual_retry(ctx.body);
            }
            let t = self.get_type_of_node_with_request(ctx.body, &body_request);
            self.ctx.preserve_literal_types = prev_preserve_literals;
            t
        });

        let actual_return = if ctx.is_async_for_context {
            self.unwrap_async_return_type_for_body(actual_return)
        } else {
            actual_return
        };
        let body_is_simple_expression = self.ctx.arena.get(ctx.body).is_some_and(|body_node| {
            let effective_kind = if body_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                self.ctx
                    .arena
                    .get_parenthesized(body_node)
                    .and_then(|paren| self.ctx.arena.get(paren.expression))
                    .map(|inner| inner.kind)
                    .unwrap_or(body_node.kind)
            } else {
                body_node.kind
            };
            effective_kind != syntax_kind_ext::BLOCK
                && effective_kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && effective_kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        });
        let suppress_contextual_return_check = !ctx.has_type_annotation
            && ctx.jsdoc_return_context.is_none()
            && (self.type_has_unresolved_inference_holes(expected_return_type)
                || (crate::query_boundaries::common::is_callable_type(
                    self.ctx.types,
                    actual_return,
                ) && !crate::query_boundaries::common::is_callable_type(
                    self.ctx.types,
                    expected_return_type,
                ))
                || body_is_simple_expression);
        let use_generic_return_mismatch =
            !ctx.has_type_annotation
                && ctx.jsdoc_return_context.is_none()
                && self.ctx.arena.get(ctx.body).is_some_and(|body_node| {
                    body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                })
                && self.type_has_unresolved_inference_holes(expected_return_type);
        if ctx.contextual_void_return_exception || suppress_contextual_return_check {
            return;
        }
        if use_generic_return_mismatch {
            self.check_generic_expression_body_return_mismatch(
                ctx.body,
                expected_return_type,
                actual_return,
                ctx.is_async_for_context,
            );
            return;
        }

        self.check_direct_expression_body_return_mismatch(DirectExpressionBodyReturnMismatchCtx {
            idx: ctx.idx,
            body: ctx.body,
            expected_return_type,
            actual_return,
            actual_return_node,
            actual_return_uses_jsdoc_cast,
            is_closure: ctx.is_closure,
            is_async_for_context: ctx.is_async_for_context,
            return_annotation: DirectReturnAnnotation::from_parts(
                ctx.has_type_annotation,
                ctx.jsdoc_return_context.is_some(),
            ),
        });
    }

    fn check_generic_expression_body_return_mismatch(
        &mut self,
        body: NodeIndex,
        expected_return_type: TypeId,
        actual_return: TypeId,
        is_async_for_context: bool,
    ) {
        let conditional_branch_mismatch = self
            .ctx
            .arena
            .get(body)
            .and_then(|body_node| self.ctx.arena.get_conditional_expr(body_node))
            .is_some_and(|cond| {
                let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                let return_req = TypingRequest::with_contextual_type(expected_return_type);
                let mut when_true = self.get_type_of_node_with_request(cond.when_true, &return_req);
                let mut when_false =
                    self.get_type_of_node_with_request(cond.when_false, &return_req);
                snap.rollback(&mut self.ctx.diagnostic_state());
                if is_async_for_context {
                    when_true = self.unwrap_promise_type(when_true).unwrap_or(when_true);
                    when_false = self.unwrap_promise_type(when_false).unwrap_or(when_false);
                }
                !self
                    .return_relation_outcome(when_true, expected_return_type)
                    .related
                    || !self
                        .return_relation_outcome(when_false, expected_return_type)
                        .related
            });
        if conditional_branch_mismatch
            && !self
                .is_nested_same_wrapper_application_assignment(actual_return, expected_return_type)
            && let Some(loc) = self.get_source_location(body)
        {
            let src_str = self.format_type(actual_return);
            let tgt_str = self.format_type(expected_return_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            self.ctx
                .diagnostics
                .push(crate::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                ));
        }
    }

    fn check_direct_expression_body_return_mismatch(
        &mut self,
        ctx: DirectExpressionBodyReturnMismatchCtx,
    ) {
        let body_is_conditional = self
            .ctx
            .arena
            .get(ctx.body)
            .is_some_and(|n| n.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION);
        let is_rhs_assignment = ctx.is_closure && self.is_rhs_of_assignment(ctx.idx);
        let inner_body = if ctx.actual_return_uses_jsdoc_cast {
            ctx.actual_return_node
        } else {
            self.ctx.arena.skip_parenthesized_and_assertions(ctx.body)
        };
        let assignability_ok = if body_is_conditional || is_rhs_assignment {
            self.check_assignable_or_report_at(
                ctx.actual_return,
                ctx.expected_return_type,
                ctx.body,
                ctx.body,
            )
        } else {
            self.check_assignable_or_report_at_exact_anchor(
                ctx.actual_return,
                ctx.expected_return_type,
                inner_body,
                inner_body,
            )
        };
        if !assignability_ok {
            for diag in self.ctx.diagnostics.iter().rev() {
                if diag.code
                    == tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    && let Some(body_node) = self.ctx.arena.get(ctx.body)
                    && diag.start >= body_node.pos
                    && diag.start < body_node.end
                {
                    self.ctx.callback_return_type_errors.push(diag.clone());
                    break;
                }
            }
        }
        if assignability_ok
            && let Some(body_node) = self.ctx.arena.get(ctx.body)
            && body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
        {
            self.check_conditional_return_branches_against_type(
                ctx.body,
                ctx.expected_return_type,
                ctx.is_async_for_context,
            );
        }
        // A concise (expression) body that is directly a fresh object/array
        // literal must still run the contextual excess-property check against the
        // declared return type. The block-body path performs this in
        // `check_return_statement`; the conditional concise path handles it in
        // `check_conditional_return_branches_against_type` above. The remaining
        // direct concise path (`(): { x: number } => ({ x: 1, y: 2 })`) had no EPC
        // pass, so excess properties on fresh literals were silently accepted.
        //
        // Only run it when structural assignability already passed: a failed
        // relation has already emitted TS2353/TS2322 through the failure reason,
        // and EPC is gated on a declared return type so a purely contextual return
        // type (an arrow assigned to an interface method) is left untouched, just
        // as in tsc.
        if assignability_ok
            && ctx.return_annotation.is_declared()
            && !ctx.actual_return_uses_jsdoc_cast
        {
            self.check_concise_body_excess_properties(ctx.body, ctx.expected_return_type);
        }
    }

    /// Run the contextual excess-property check on a concise (expression-body)
    /// return whose declared return type is `expected`, recursing through array
    /// literals so array-of-object and nested concise returns are covered. Fresh
    /// object literals are routed through `check_object_literal_excess_properties`
    /// (which itself recurses into nested object property values), matching what
    /// the block-body return path does for `return <literal>`.
    ///
    /// Only parentheses are skipped, never `as`/`satisfies`/type assertions: an
    /// asserted body (`({ x: 1, y: 2 } as { x: number })`) performs its own
    /// check, and tsc does not additionally run EPC on it against the return
    /// type, so stopping at the assertion node keeps parity.
    fn check_concise_body_excess_properties(&mut self, node: NodeIndex, expected: TypeId) {
        if expected == TypeId::ANY
            || expected == TypeId::UNKNOWN
            || self.type_contains_error(expected)
        {
            return;
        }
        let node = self.ctx.arena.skip_parenthesized(node);
        let Some(kind) = self.ctx.arena.get(node).map(|n| n.kind) else {
            return;
        };
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let source = self.get_type_of_node(node);
            self.check_object_literal_excess_properties(source, expected, node);
        } else if kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let element_nodes: Vec<NodeIndex> = match self
                .ctx
                .arena
                .get(node)
                .and_then(|n| self.ctx.arena.get_literal_expr(n))
            {
                Some(array) => array.elements.nodes.clone(),
                None => return,
            };
            let tuple_elements =
                crate::query_boundaries::common::tuple_elements(self.ctx.types, expected);
            // A plain (non-tuple) array target shares one contextual element type
            // across every element; only resolve it when there is no tuple shape.
            let array_element = match tuple_elements {
                Some(_) => None,
                None => {
                    crate::query_boundaries::common::array_element_type(self.ctx.types, expected)
                }
            };
            for (index, &element) in element_nodes.iter().enumerate() {
                if element.is_none() {
                    continue;
                }
                let expected_element = match &tuple_elements {
                    Some(elements) => elements.get(index).map(|te| te.type_id),
                    None => array_element,
                };
                if let Some(expected_element) = expected_element {
                    self.check_concise_body_excess_properties(element, expected_element);
                }
            }
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
        // No prototype special case: TypeScript 7 dropped JS constructor-function
        // inference, so `M.prototype` does not name a synthesized instance type.
        // `M.prototype.m = function () { ... }` takes the ordinary
        // assignment-receiver `this` (the type of `M.prototype`) like any other
        // `obj.m = function () { ... }`. The CommonJS `exports.A = ...` /
        // `module.exports.A = ...` bases are handled by
        // `assignment_rhs_base_this_type` (Mechanism 2 of #16964), keeping this
        // return-inference path in step with the diagnostic path.
        let base_expr = self.ctx.arena.get_access_expr(left_node)?.expression;
        self.assignment_rhs_base_this_type(base_expr)
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
            params.push(signature_building_boundary::param_info(
                None,
                signature_building_boundary::param_array_type(self.ctx.types, TypeId::ANY),
                true,
                true,
            ));
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
            return_type_construction::function_return_lazy_type(self.ctx.types, def_id)
        });
        let Some(base) = lazy_base else {
            return TypeId::ANY;
        };

        let yield_t = ctx.final_generator_yield_type.unwrap_or(TypeId::ANY);
        let body_return_t = self.unannotated_generator_body_return_type(ctx);
        let return_t = body_return_t
            .or(ctx.early_gen_return_type)
            .unwrap_or(TypeId::VOID);
        // A contextual `Generator<Y, R, N>` still wins: it is the declared shape
        // the generator is being checked against, and `tsc` reads plain yields'
        // next contributions out of exactly that contextual type. The delegated
        // aggregate speaks only where tsz previously had nothing to say and fell
        // through to `unknown` — an unannotated, uncontextualized generator whose
        // body delegates.
        let next_t = ctx
            .early_gen_next_type
            .or(ctx.delegated_gen_next_type)
            .unwrap_or(TypeId::UNKNOWN);

        let application = return_type_construction::function_return_application(
            self.ctx.types,
            base,
            vec![yield_t, return_t, next_t],
        );
        // Warm the solver's application-eval cache for this exact (def, args)
        // pair while the checker's env-aware resolver is live. Without this,
        // a later raw-solver re-evaluation of the same Application — e.g. a
        // generic call's constraint/finalize passes over an argument that IS
        // this call's own return type — can run through a resolver context
        // that cannot re-derive `AsyncGenerator`/`Generator`'s type params
        // from the bare `Lazy(DefId)` base on its own, and falls back to the
        // interface's unsubstituted structural shape (dropping our `yield_t`/
        // `return_t`/`next_t` args): the printer then shows a bare
        // `AsyncGenerator` and a spurious TS2345 fires even though a
        // concrete assignment against the identical type args succeeds.
        // Reachable only via `AsyncGenerator`/`Generator` return-type
        // inference — an explicit annotation lowers through the ordinary
        // type-node path and never hits this gap. See #16119.
        let _ = self.evaluate_type_with_env(application);
        application
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

    fn wrap_unannotated_async_return_type(&mut self, return_type: TypeId) -> TypeId {
        let mut awaited_type = self.compute_awaited_type(return_type, 0);
        let had_awaitable_layer = awaited_type != return_type;
        if !had_awaitable_layer
            && !crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, awaited_type)
        {
            awaited_type = self.widen_literal_type(awaited_type);
        }
        if let Some(promise_type) = self.get_promise_type(awaited_type) {
            return promise_type;
        }
        return_type_construction::function_return_application(
            self.ctx.types,
            TypeId::PROMISE_BASE,
            vec![awaited_type],
        )
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
        use tsz_solver::TypePredicateTarget;

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

        Some(signature_building_boundary::type_predicate(
            is_asserts,
            target,
            type_id,
            parameter_index,
        ))
    }

    /// Resolve a non-predicate JSDoc `@return {TypeExpr}` to a TypeId.
    ///
    /// This handles cases like `@return {false}`, `@return {void}`, `@return {number}`, etc.
    /// Returns `None` if no `@return` tag is found or the type expression can't be resolved.
    /// Type predicate returns (like `@return {x is string}`) are excluded.
    ///
    /// `comment_start` anchors a bare (non-`typeof`) `import("./mod").Member`
    /// type expression's TS2694 at the member-name token inside the comment,
    /// matching tsc — mirroring `resolve_jsdoc_param_type_with_pos`'s
    /// identical `@param` precise-anchor path (#17193). Passing `None` keeps
    /// the coarse `jsdoc_typedef_anchor_pos` fallback `resolve_jsdoc_reference`
    /// uses for every other shape.
    pub(crate) fn resolve_jsdoc_return_type(
        &mut self,
        jsdoc: &str,
        comment_start: Option<u32>,
    ) -> Option<TypeId> {
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
            if let Some(ty) = self.resolve_jsdoc_return_type_import_member(type_expr, comment_start)
            {
                return Some(ty);
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

        for info in type_params {
            let name = self.ctx.types.resolve_atom_ref(info.name).to_string();
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = signature_building_boundary::type_param(self.ctx.types, *info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
        }

        updates
    }

    /// Check if a function body references the `arguments` object.
    /// Walks the AST recursively but stops at nested function boundaries.
    /// Used by JS files to determine if a function needs an implicit rest parameter.
    pub(crate) fn body_has_arguments_reference(&self, body: NodeIndex) -> bool {
        // Guard this uncached structural walk against a node graph that is not a
        // finite tree. The walk assumes every child link strictly descends, but
        // a body node can end up reachable from itself (observed while checking
        // `async` generic methods in the config-broken canary apps: immich-server,
        // cal-com, infisical). Without a guard the recursion never terminates and
        // overflows the worker stack (SIGABRT) — the parser bounds genuine
        // nesting via `MAX_PARSER_RECURSION_DEPTH`, so unbounded depth here means
        // a cycle, not deep input. A well-formed tree never revisits a node, so
        // the visited set changes no result on valid input while terminating on
        // cyclic input — the same `FxHashSet<NodeIndex>` cycle guard used for
        // node-index walks elsewhere in this crate.
        let mut visited: rustc_hash::FxHashSet<NodeIndex> = rustc_hash::FxHashSet::default();
        self.body_has_arguments_reference_guarded(body, &mut visited)
    }

    fn body_has_arguments_reference_guarded(
        &self,
        body: NodeIndex,
        visited: &mut rustc_hash::FxHashSet<NodeIndex>,
    ) -> bool {
        if !visited.insert(body) {
            // Already on the current walk: a cyclic AST link. Stop instead of
            // recursing forever. Any real `arguments` reference reachable from a
            // well-formed position was already visited on first descent.
            return false;
        }

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
                if self.body_has_arguments_reference_guarded(stmt, visited) {
                    return true;
                }
            }
        } else if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
            if self.body_has_arguments_reference_guarded(expr_stmt.expression, visited) {
                return true;
            }
        } else if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
            for &decl in &var_stmt.declarations.nodes {
                if self.body_has_arguments_reference_guarded(decl, visited) {
                    return true;
                }
            }
        } else if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if self.body_has_arguments_reference_guarded(var_decl.initializer, visited) {
                return true;
            }
        } else if let Some(ret) = self.ctx.arena.get_return_statement(node) {
            if self.body_has_arguments_reference_guarded(ret.expression, visited) {
                return true;
            }
        } else if let Some(call) = self.ctx.arena.get_call_expr(node) {
            if self.body_has_arguments_reference_guarded(call.expression, visited) {
                return true;
            }
            if let Some(ref args) = call.arguments {
                for &arg in &args.nodes {
                    if self.body_has_arguments_reference_guarded(arg, visited) {
                        return true;
                    }
                }
            }
        } else if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if self.body_has_arguments_reference_guarded(bin.left, visited)
                || self.body_has_arguments_reference_guarded(bin.right, visited)
            {
                return true;
            }
        } else if let Some(access) = self.ctx.arena.get_access_expr(node) {
            if self.body_has_arguments_reference_guarded(access.expression, visited) {
                return true;
            }
            // Element access: also check the index expression (e.g. obj[arguments]).
            // Property names like `holder.arguments` are not references to the
            // function's implicit `arguments` object.
            if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && self.body_has_arguments_reference_guarded(access.name_or_argument, visited)
            {
                return true;
            }
        } else if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
            if self.body_has_arguments_reference_guarded(if_stmt.expression, visited)
                || self.body_has_arguments_reference_guarded(if_stmt.then_statement, visited)
                || self.body_has_arguments_reference_guarded(if_stmt.else_statement, visited)
            {
                return true;
            }
        } else if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            if self.body_has_arguments_reference_guarded(loop_stmt.initializer, visited)
                || self.body_has_arguments_reference_guarded(loop_stmt.condition, visited)
                || self.body_has_arguments_reference_guarded(loop_stmt.incrementor, visited)
                || self.body_has_arguments_reference_guarded(loop_stmt.statement, visited)
            {
                return true;
            }
        } else if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
            if self.body_has_arguments_reference_guarded(for_in_of.expression, visited)
                || self.body_has_arguments_reference_guarded(for_in_of.statement, visited)
            {
                return true;
            }
        } else if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
            if self.body_has_arguments_reference_guarded(paren.expression, visited) {
                return true;
            }
        } else if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
            if self.body_has_arguments_reference_guarded(unary.operand, visited) {
                return true;
            }
        } else if let Some(unary_ex) = self.ctx.arena.get_unary_expr_ex(node) {
            if self.body_has_arguments_reference_guarded(unary_ex.expression, visited) {
                return true;
            }
        } else if let Some(spread) = self.ctx.arena.get_spread(node) {
            if self.body_has_arguments_reference_guarded(spread.expression, visited) {
                return true;
            }
        } else if let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            && (self.body_has_arguments_reference_guarded(cond.condition, visited)
                || self.body_has_arguments_reference_guarded(cond.when_true, visited)
                || self.body_has_arguments_reference_guarded(cond.when_false, visited))
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

        let mut enclosing_param_indices: Vec<(Vec<NodeIndex>, Option<Vec<TypeId>>)> = Vec::new();
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
                // A method signature built while its class is being checked must
                // close over the class binders already installed by
                // `push_effective_class_type_parameters`. Re-resolving the same
                // declarations here can observe a different transient recovery
                // state (for example, `None` versus `Some(ERROR)` constraints)
                // and mint a second `TypeId` for the same binder.
                let exact_class_type_parameter_ids = if matches!(
                    parent.kind,
                    k if k == syntax_kind_ext::CLASS_DECLARATION
                        || k == syntax_kind_ext::CLASS_EXPRESSION
                ) {
                    self.ctx
                        .enclosing_class
                        .as_ref()
                        .filter(|info| {
                            info.class_idx == parent_idx
                                && info.class_type_parameter_ids.len() == indices.len()
                        })
                        .map(|info| info.class_type_parameter_ids.clone())
                } else {
                    None
                };
                enclosing_param_indices.push((indices, exact_class_type_parameter_ids));
            }

            current = parent_idx;
        }

        if enclosing_param_indices.is_empty() {
            return Vec::new();
        }

        let mut updates = Vec::new();
        let mut added_params: Vec<(NodeIndex, bool)> = Vec::new();

        // Pass 1: Add all type parameters to scope WITHOUT constraints
        for (param_indices, exact_class_type_parameter_ids) in
            enclosing_param_indices.into_iter().rev()
        {
            for (param_position, param_idx) in param_indices.into_iter().enumerate() {
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
                    .map_or_else(
                        || "T".to_string(),
                        |id_data| id_data.escaped_text.to_string(),
                    );

                if let Some(type_id) = exact_class_type_parameter_ids
                    .as_ref()
                    .and_then(|ids| ids.get(param_position))
                    .copied()
                {
                    let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                    updates.push((name, previous, false));
                    continue;
                }

                let atom = self.ctx.types.intern_string(&name);

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info =
                    signature_building_boundary::user_type_param_info(atom, None, None, is_const);
                let needs_identity_scope =
                    self.type_parameter_decl_needs_identity_scope(&name, data.name);
                // Mint through the declaration-scoped cache (not a structural
                // `factory.type_param` intern) so the enclosing parameter
                // resolves to the SAME `TypeId` here as under
                // `push_type_parameters`. A structural mint gives a member
                // annotation a different identity for the same declared
                // parameter than the one the `implements`-clause type
                // arguments resolve to, breaking the alias-application
                // identity fast path (false TS2416, #13044).
                let type_id = self
                    .intern_type_param_for_decl_stamped_with_identity(
                        data.name,
                        info,
                        needs_identity_scope,
                    )
                    .0;

                let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                updates.push((name, previous, false));
                added_params.push((param_idx, needs_identity_scope));
            }
        }

        // Pass 2: Resolve constraints AND defaults now that all type
        // parameters are in scope. The refined intern must carry the
        // declaration's DEFAULT as well: the canonical `push_type_parameters`
        // mint for the same declaration includes it, and the decl-scoped
        // cache is keyed on the full `TypeParamInfo`, so omitting the default
        // here gives the enclosing-scope entry a DIFFERENT `TypeId` from the
        // one the enclosing function's member types reference. The dangling-
        // parameter fill (`resolve_unbound_property_member_defaults`) then
        // treats the enclosing parameter as unbound and collapses e.g.
        // `Box<R>.get`'s `R` to its declared default inside nested-function
        // bodies (`f<R = unknown>(box: Box<R>) { () => box.get() }`).
        for (param_idx, needs_identity_scope) in added_params {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            if data.constraint == NodeIndex::NONE && data.default == NodeIndex::NONE {
                continue;
            }

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(
                    || "T".to_string(),
                    |id_data| id_data.escaped_text.to_string(),
                );
            let atom = self.ctx.types.intern_string(&name);

            let constraint = (data.constraint != NodeIndex::NONE)
                .then(|| self.get_type_from_type_node(data.constraint))
                .filter(|&constraint_type| constraint_type != TypeId::ERROR);
            let default = (data.default != NodeIndex::NONE)
                .then(|| self.get_type_from_type_node(data.default))
                .filter(|&default_type| default_type != TypeId::ERROR);

            let is_const = self
                .ctx
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
            let info = signature_building_boundary::user_type_param_info(
                atom, constraint, default, is_const,
            );
            let refined_type_id = self
                .intern_type_param_for_decl_stamped_with_identity(
                    data.name,
                    info,
                    needs_identity_scope,
                )
                .0;
            self.ctx.type_parameter_scope.insert(name, refined_type_id);
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

        signature_construction::function_type_with_params_replaced(
            self.ctx.types,
            &shape,
            new_params,
        )
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
        // For async functions, suppress return-completeness diagnostics only
        // when the annotation resolves to the actual global Promise. A local
        // or qualified type named Promise still follows normal return checks.
        if is_async
            && check_return_type == effective_return_type
            && has_type_annotation
            && self.return_type_annotation_is_exactly_promise(type_annotation)
        {
            check_return_type = TypeId::VOID;
        }
        let requires_return = self.requires_return_value(check_return_type);
        let has_return = self.body_has_return_with_value(body);
        let falls_through = self.function_body_falls_through(body);
        // Return type used by both TS7030 sources below (fall-off-the-end and
        // per-bare-return). Computed once, and only when `noImplicitReturns` is
        // on, since both consumers are gated on it. Uses `function_is_generator`
        // (the ctx-sourced flag the TS7030 path has always used), distinct from
        // the arena-sourced `is_generator` behind `check_return_type` above.
        let ts7030_check_type = if self.ctx.no_implicit_returns() {
            self.return_type_for_implicit_return_check(
                annotated_return_type.unwrap_or(return_type),
                is_async,
                function_is_generator,
            )
        } else {
            check_return_type
        };
        if has_type_annotation
            && requires_return
            && falls_through
            && check_return_type != TypeId::VOID
            && (!has_return || self.ctx.strict_null_checks())
        {
            if !has_return {
                self.error_at_node(
                    type_annotation,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else {
                // TS2366 (has explicit return, falls through). The branch gate
                // above guarantees strictNullChecks here: in non-strict mode
                // `undefined` is assignable to every type, so tsc's guard
                // `strictNullChecks && !isTypeAssignableTo(undefinedType, type)`
                // short-circuits to false (checker.ts checkAllCodePaths... :39580).
                // Excluding the has-return non-strict case from the gate lets
                // control fall through to the TS7030 noImplicitReturns check.
                self.error_at_node(
                    type_annotation,
                    diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
            }
        } else if self.ctx.no_implicit_returns() && has_return && falls_through {
            // TS7030: noImplicitReturns - not all code paths return a value
            // TSC skips TS7030 for functions returning void, any, or unions containing void/any
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

        // TS7030 per bare `return;`. Skip an unannotated generator whose
        // `TReturn` can't be extracted (mirrors `can_check_generator_completion`
        // in `function_declaration_checks.rs`).
        if !function_is_generator
            || self
                .generator_return_type_for_implicit_return_check(
                    annotated_return_type.unwrap_or(return_type),
                )
                .is_some()
        {
            self.report_no_implicit_return_bare_returns(
                body,
                ts7030_check_type,
                has_type_annotation,
                function_is_generator,
            );
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
            let cached = cache.borrow().get(&class_idx).copied();
            return cached.or_else(|| {
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

#[path = "function_type_helpers_async_promise.rs"]
mod function_type_helpers_async_promise;
