//! Type assignability and excess property checking.
//! Subtype, identity, and redeclaration compatibility live in `subtype_identity_checker`.

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, AssignabilityQueryInputs, are_types_overlapping_with_env,
    check_application_variance_assignability, classify_for_assignability_eval,
    contains_free_infer_types, get_allowed_keys, get_keyof_type, get_string_literal_value,
    get_union_members, is_assignable_bivariant_with_resolver, is_assignable_with_overrides,
    is_relation_cacheable, keyof_object_properties, map_compound_members,
};
use crate::state::{CheckerOverrideProvider, CheckerState};
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::NarrowingContext;
use tsz_solver::RelationCacheKey;
use tsz_solver::TypeId;
use tsz_solver::visitor::{collect_lazy_def_ids, collect_type_queries};

// =============================================================================
// Assignability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    pub(crate) fn callable_has_own_generic_signatures(&self, type_id: TypeId) -> bool {
        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id) {
            return !shape.type_params.is_empty();
        }
        if let Some(shape) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id) {
            return shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
                || shape
                    .construct_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty());
        }
        false
    }

    /// Check if an argument node is a callback (arrow function or function expression)
    /// with unannotated parameters that rely on contextual typing.
    pub(crate) fn arg_is_callback_with_unannotated_params(&self, arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        // Check if it's an arrow function or function expression
        let is_callback = node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;

        if !is_callback {
            // Check for parenthesized expressions wrapping a callback
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    return self.arg_is_callback_with_unannotated_params(paren.expression);
                }
            }
            return false;
        }

        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        // Check if any parameter lacks a type annotation (relies on contextual typing)
        func.parameters.nodes.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|pn| self.ctx.arena.get_parameter(pn))
                .is_some_and(|p| p.type_annotation.is_none())
        })
    }

    fn normalize_nested_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        // Depth guard: prevents stack overflow from mutually recursive types
        // (e.g., Foo<T> ↔ Bar<T>) where each fresh visited set misses cross-function cycles.
        thread_local! { static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
        let depth = DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        if depth >= 10 {
            DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return type_id;
        }
        let mut visited = FxHashSet::default();
        let result = self.normalize_nested_type_for_assignability_inner(type_id, &mut visited);
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        result
    }

    fn normalize_nested_type_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if !visited.insert(type_id) {
            return type_id;
        }

        let resolved = self.resolve_type_query_type(type_id);
        let evaluated =
            if tsz_solver::type_queries::get_type_application(self.ctx.types, resolved).is_some() {
                self.evaluate_type_for_assignability(resolved)
            } else {
                self.evaluate_type_with_env(resolved)
            };
        let type_id = if evaluated == TypeId::UNKNOWN && resolved != TypeId::UNKNOWN {
            resolved
        } else if evaluated != resolved {
            evaluated
        } else {
            resolved
        };

        if let Some(inner) = tsz_solver::type_queries::get_readonly_inner(self.ctx.types, type_id) {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.readonly_type(normalized)
            } else {
                type_id
            }
        } else if let Some(inner) =
            tsz_solver::type_queries::get_noinfer_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.no_infer(normalized)
            } else {
                type_id
            }
        } else if let Some(elem) =
            tsz_solver::type_queries::get_array_element_type(self.ctx.types, type_id)
        {
            if tsz_solver::type_queries::is_array_type(self.ctx.types, type_id) {
                let normalized = self.normalize_nested_type_for_assignability_inner(elem, visited);
                if normalized != elem {
                    self.ctx.types.array(normalized)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(elements) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, type_id)
        {
            if tsz_solver::type_queries::is_tuple_type(self.ctx.types, type_id) {
                let mut changed = false;
                let normalized_elements: Vec<_> = elements
                    .iter()
                    .map(|elem| {
                        let normalized = self
                            .normalize_nested_type_for_assignability_inner(elem.type_id, visited);
                        if normalized != elem.type_id {
                            changed = true;
                        }
                        tsz_solver::TupleElement {
                            type_id: normalized,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.factory().tuple(normalized_elements)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().union(normalized_members)
            } else {
                type_id
            }
        } else if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().intersection(normalized_members)
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    fn normalize_function_shape_for_assignability(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> Option<tsz_solver::FunctionShape> {
        // When a generic function has type params, check if a type references any of them.
        // If so, skip normalization to preserve Application types that use the function's
        // own type params. Eagerly evaluating these to Objects destroys type param identity
        // needed for alpha-renaming during generic function subtype comparison.
        // Example: `<B>(f: (t: A) => B): IList<B>` — the return type `IList<B>` must stay
        // as Application(IList, [B]), not be expanded to an Object containing methods with
        // their own type param B that would shadow the outer B.
        // Collect names of the function's own type params to detect references.
        let own_tp_names: Vec<_> = shape.type_params.iter().map(|tp| tp.name).collect();

        let mut changed = false;
        let params = shape
            .params
            .iter()
            .map(|param| {
                // Skip normalization for params that reference the function's own type
                // params — eagerly evaluating them destroys Application types needed
                // for alpha-renaming during generic function subtype comparison.
                let skip = !own_tp_names.is_empty()
                    && own_tp_names.iter().any(|&name| {
                        tsz_solver::visitor::contains_type_parameter_named(
                            self.ctx.types,
                            param.type_id,
                            name,
                        )
                    });
                let evaluated = if skip {
                    param.type_id
                } else {
                    self.normalize_nested_type_for_assignability(param.type_id)
                };
                if evaluated != param.type_id {
                    changed = true;
                }
                tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: evaluated,
                    optional: param.optional,
                    rest: param.rest,
                }
            })
            .collect();
        let this_type = shape.this_type.map(|this_type| {
            let evaluated = self.normalize_nested_type_for_assignability(this_type);
            if evaluated != this_type {
                changed = true;
            }
            evaluated
        });
        let return_type = {
            // Skip normalization for return types that reference the function's own
            // type params. This preserves Application types (e.g., IList<B>) so that
            // alpha-renaming substitution in check_function_subtype works correctly.
            // Without this, the Application gets expanded to an Object whose inner
            // methods may have type params with the same name, causing shadowing
            // that blocks the alpha-renaming substitution.
            let skip = !own_tp_names.is_empty()
                && own_tp_names.iter().any(|&name| {
                    tsz_solver::visitor::contains_type_parameter_named(
                        self.ctx.types,
                        shape.return_type,
                        name,
                    )
                });
            let evaluated = if skip {
                shape.return_type
            } else {
                self.normalize_nested_type_for_assignability(shape.return_type)
            };
            if evaluated != shape.return_type {
                changed = true;
            }
            evaluated
        };
        let type_predicate = shape.type_predicate.as_ref().map(|predicate| {
            let type_id = predicate.type_id.map(|type_id| {
                let evaluated = self.normalize_nested_type_for_assignability(type_id);
                if evaluated != type_id {
                    changed = true;
                }
                evaluated
            });
            tsz_solver::TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id,
                parameter_index: predicate.parameter_index,
            }
        });

        changed.then_some(tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    fn normalize_callable_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id) {
            return self
                .normalize_function_shape_for_assignability(&shape)
                .map(|shape| self.ctx.types.factory().function(shape))
                .unwrap_or(type_id);
        }
        if let Some(shape) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id) {
            let mut changed = false;
            let call_signatures: Vec<_> = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: false,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();
            let construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: true,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();

            if changed {
                self.ctx
                    .types
                    .factory()
                    .callable(tsz_solver::CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties: shape.properties.clone(),
                        string_index: shape.string_index,
                        number_index: shape.number_index,
                        symbol: shape.symbol,
                        is_abstract: shape.is_abstract,
                    })
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    pub(crate) fn get_keyof_type_keys(
        &mut self,
        type_id: TypeId,
        db: &dyn tsz_solver::TypeDatabase,
    ) -> FxHashSet<Atom> {
        if let Some(keyof_type) = get_keyof_type(db, type_id)
            && let Some(key_type) = keyof_object_properties(db, keyof_type)
            && let Some(members) = get_union_members(db, key_type)
        {
            return members
                .into_iter()
                .filter_map(|m| {
                    if let Some(str_lit) = get_string_literal_value(db, m) {
                        return Some(str_lit);
                    }
                    None
                })
                .collect();
        }
        FxHashSet::default()
    }

    fn typeof_this_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        this_ref: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_this_target(left, this_ref) {
            return self.string_literal_text(right);
        }
        if self.is_typeof_this_target(right, this_ref) {
            return self.string_literal_text(left);
        }
        None
    }

    fn is_typeof_this_target(&self, expr: NodeIndex, this_ref: NodeIndex) -> bool {
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }
        let operand = self.ctx.arena.skip_parenthesized(unary.operand);
        if operand == this_ref {
            return true;
        }
        self.ctx
            .arena
            .get(operand)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
    }

    fn string_literal_text(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self
                .ctx
                .arena
                .get_literal(node)
                .map(|lit| lit.text.as_str());
        }
        None
    }

    pub(crate) fn narrow_this_from_enclosing_typeof_guard(
        &self,
        source_idx: NodeIndex,
        source: TypeId,
    ) -> TypeId {
        let is_this_source = self
            .ctx
            .arena
            .get(source_idx)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
        if !is_this_source {
            return source;
        }

        let mut current = source_idx;
        let mut depth = 0usize;
        while depth < 256 {
            depth += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(current) else {
                break;
            };
            if parent_node.kind != syntax_kind_ext::IF_STATEMENT {
                continue;
            }
            let Some(if_stmt) = self.ctx.arena.get_if_statement(parent_node) else {
                continue;
            };
            if !self.is_node_within(source_idx, if_stmt.then_statement) {
                continue;
            }
            let Some(cond_node) = self.ctx.arena.get(if_stmt.expression) else {
                continue;
            };
            if cond_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(cond_node) else {
                continue;
            };
            let is_equality = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16
                || bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
            if !is_equality {
                continue;
            }
            if let Some(type_name) =
                self.typeof_this_comparison_literal(bin.left, bin.right, source_idx)
            {
                return NarrowingContext::new(self.ctx.types).narrow_by_typeof(source, type_name);
            }
        }

        source
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for one type.
    pub(crate) fn ensure_relation_input_ready(&mut self, type_id: TypeId) {
        // Global fuel guard: bail when total resolution work across all top-level
        // calls has exceeded the budget. Prevents OOM on DOM-heavy React code
        // where many top-level calls each reset per-call fuel.
        if crate::state_domain::type_environment::lazy::global_resolution_fuel_exhausted() {
            return;
        }
        self.ensure_refs_resolved(type_id);
        self.ensure_application_symbols_resolved(type_id);
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for multiple types.
    pub(crate) fn ensure_relation_inputs_ready(&mut self, type_ids: &[TypeId]) {
        for &type_id in type_ids {
            self.ensure_relation_input_ready(type_id);
        }
    }

    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) fn should_suppress_assignability_diagnostic(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // Check if a type contains an error application (e.g., error<any>)
        // This happens when type resolution fails for qualified names like React.ReactElement
        // in function return type positions. Suppress the false positive TS2322.
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let is_callable_or_function = |type_id: TypeId| {
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id).is_some()
                || tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id).is_some()
                || tsz_solver::type_queries::get_type_application(self.ctx.types, type_id)
                    .is_some_and(|app| {
                        tsz_solver::type_queries::get_callable_shape(self.ctx.types, app.base)
                            .is_some()
                            || tsz_solver::type_queries::get_function_shape(
                                self.ctx.types,
                                app.base,
                            )
                            .is_some()
                    })
        };

        let has_own_signature_type_params = |type_id: TypeId| -> bool {
            if let Some(shape) =
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id)
            {
                return shape
                    .call_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty());
            }
            if let Some(shape) =
                tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id)
            {
                return !shape.type_params.is_empty();
            }
            false
        };

        let contains_type_parameters =
            |type_id: TypeId| tsz_solver::contains_type_parameters(self.ctx.types, type_id);

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            // Inference placeholders are transient solver state. Emitting TS2322/TS2345
            // while they are still present creates contextual false positives.
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(target))
            // Suppress TS2322 for callable types where the source contains generic type
            // parameters that may not have been fully inferred from context. Skip when
            // both source and target have their own signature-level type parameters —
            // the solver handles generic-to-generic comparison correctly via alpha-renaming.
            || (is_callable_or_function(source)
                && is_callable_or_function(target)
                && contains_type_parameters(source)
                && !(has_own_signature_type_params(source)
                    && has_own_signature_type_params(target)))
    }

    /// Check if a type contains an error application (recursively).
    fn type_contains_error_application(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
        // Check if it's a direct error application
        if let Some(app) = tsz_solver::type_queries::get_type_application(db, type_id) {
            if app.base == TypeId::ERROR {
                return true;
            }
        }

        // Check if it's a union type containing an error application
        if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's an intersection type containing an error application
        if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's a function type with error return
        if let Some(fn_shape) = tsz_solver::type_queries::get_function_shape(db, type_id) {
            if Self::type_contains_error_application(db, fn_shape.return_type) {
                return true;
            }
        }

        // Check if it's a callable type with error return
        if let Some(callable) = tsz_solver::type_queries::get_callable_shape(db, type_id) {
            for sig in &callable.call_signatures {
                if Self::type_contains_error_application(db, sig.return_type) {
                    return true;
                }
            }
        }

        false
    }

    /// Suppress assignability diagnostics for parser-recovery artifacts.
    pub(crate) fn should_suppress_assignability_for_parse_recovery(
        &self,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if !self.has_syntax_parse_errors() {
            return false;
        }

        if self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }

        self.is_parse_recovery_anchor_node(source_idx)
            || self.is_parse_recovery_anchor_node(diag_idx)
    }

    /// Detect nodes that look like parser-recovery artifacts (empty text, near errors).
    fn is_parse_recovery_anchor_node(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Missing-expression placeholders used by parser recovery.
        if self
            .ctx
            .arena
            .get_identifier_text(idx)
            .is_some_and(str::is_empty)
        {
            return true;
        }

        // Also suppress diagnostics anchored very near a syntax parse error.
        const DIAG_PARSE_DISTANCE: u32 = 16;
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            let before = err_pos.saturating_sub(DIAG_PARSE_DISTANCE);
            let after = err_pos.saturating_add(DIAG_PARSE_DISTANCE);
            if (node.pos >= before && node.pos <= after)
                || (node.end >= before && node.end <= after)
            {
                return true;
            }
        }

        let mut current = idx;
        let mut walk_guard = 0;
        while current.is_some() {
            walk_guard += 1;
            if walk_guard > 512 {
                break;
            }

            if let Some(current_node) = self.ctx.arena.get(current) {
                let flags = current_node.flags as u32;
                if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                    || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                {
                    return true;
                }
            } else {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Ensure all Lazy/Ref types in a type are resolved into the type environment.
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::state_domain::type_environment::lazy::{
            enter_refs_resolution_scope, exit_refs_resolution_scope,
            global_resolution_fuel_exhausted, increment_global_resolution_fuel,
            increment_refs_resolution_fuel, refs_resolution_fuel_exhausted,
        };

        if self.ctx.refs_resolved.contains(&type_id) {
            return;
        }

        let is_outermost = enter_refs_resolution_scope();

        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            if refs_resolution_fuel_exhausted() {
                break;
            }

            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
                // Populate type_env with the VALUE type (constructor for classes) so that
                // TypeEvaluator::visit_type_query can resolve via TypeEnvironment::resolve_ref.
                // Without this, resolve_ref returns None and the fallback resolve_lazy returns
                // the INSTANCE type for classes, causing false TS2345 on `typeof ClassName` args.
                if let Some(&value_type) = self.ctx.symbol_types.get(&sym_id)
                    && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                {
                    env.insert(tsz_solver::SymbolRef(sym_id.0), value_type);
                }
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if refs_resolution_fuel_exhausted() {
                    break;
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                increment_refs_resolution_fuel();
                increment_global_resolution_fuel();
                if global_resolution_fuel_exhausted() {
                    break;
                }
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                {
                    worklist.push(result);
                }
            }
        }
        self.ctx.refs_resolved.insert(type_id);

        if is_outermost {
            exit_refs_resolution_scope();
        }
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        let kind = classify_for_assignability_eval(self.ctx.types, type_id);
        let mut evaluated = match kind {
            AssignabilityEvalKind::Application => {
                let result = self.evaluate_type_with_resolution(type_id);
                // Guard: if evaluation degraded a valid type to ERROR (e.g., due to
                // stack overflow protection tripping during deep recursive type
                // resolution), preserve the original type. ERROR is treated as
                // assignable to/from everything by the subtype checker, which would
                // silently suppress real type errors like TS2322. Keeping the original
                // Lazy type allows the compat checker's resolver to resolve it from the
                // type environment (populated during earlier successful resolution).
                if result == TypeId::ERROR && type_id != TypeId::ERROR {
                    return type_id;
                }
                result
            }
            AssignabilityEvalKind::NeedsEnvEval => {
                // For TypeQuery (typeof), resolve the value type directly from
                // get_type_of_symbol. The TypeEnvironment's types map may contain
                // the instance type for class symbols (stored by type-position
                // resolution paths like resolve_lazy_def_for_type_env), but
                // TypeQuery needs the value-position type (constructor for classes).
                if let Some(symbol_ref) = tsz_solver::visitor::type_query_symbol(
                    self.ctx.types.as_type_database(),
                    type_id,
                ) {
                    let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                    // For merged TYPE_ALIAS + VARIABLE symbols (e.g.,
                    // `type Input = Static<typeof Input>` + `const Input = ...`),
                    // get_type_of_symbol may return the type alias's circular
                    // Lazy(DefId) instead of the value's concrete type. Since
                    // TypeQuery always refers to the value side, resolve directly
                    // from the value declaration to avoid TS2344 false positives.
                    let flags = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                        && (flags & tsz_binder::symbol_flags::VARIABLE) != 0
                    {
                        let value_decl = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .map(|s| s.value_declaration)
                            .unwrap_or(tsz_parser::NodeIndex::NONE);
                        self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    }
                } else {
                    self.evaluate_type_with_env(type_id)
                }
            }
            AssignabilityEvalKind::Resolved => type_id,
        };

        // Distribution pass: normalize compound types so mixed representations do not
        // leak into relation checks (for example, `Lazy(Class)` + resolved class object).
        if let Some(distributed) = map_compound_members(self.ctx.types, evaluated, |member| {
            self.evaluate_type_for_assignability(member)
        }) {
            evaluated = distributed;
        }

        evaluated = self.normalize_callable_type_for_assignability(evaluated);

        evaluated
    }

    /// Resolve a deferred Mapped type by pre-resolving its constraint's Applications.
    ///
    /// When evaluation produces a deferred Mapped type (e.g., from Omit/Pick where
    /// the constraint contains Application types like `Exclude<keyof T, K>`), the
    /// solver's `TypeEvaluator` may have failed because lib type `DefIds` weren't
    /// registered in the `TypeEnvironment`. This method resolves the constraint through
    /// the checker's evaluation path and retries the Mapped type evaluation.
    pub(crate) fn resolve_deferred_mapped_type(&mut self, type_id: TypeId) -> TypeId {
        let Some(mapped_id) = crate::query_boundaries::state::type_environment::mapped_type_id(
            self.ctx.types.as_type_database(),
            type_id,
        ) else {
            return type_id;
        };
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let constraint = mapped.constraint;
        let resolved_constraint = self.evaluate_mapped_constraint_with_resolution(constraint);
        if resolved_constraint != constraint {
            self.ctx
                .env_eval_cache
                .borrow_mut()
                .entry(constraint)
                .or_insert(crate::context::EnvEvalCacheEntry {
                    result: resolved_constraint,
                    depth_exceeded: false,
                });
            let retry = self.evaluate_type_with_env_uncached(type_id);
            if retry != type_id {
                return retry;
            }
        }
        type_id
    }

    // =========================================================================
    // Main Assignability Check
    // =========================================================================

    /// Substitute `ThisType` in a type with the enclosing class instance type.
    ///
    /// When inside a class body, `ThisType` represents the polymorphic `this` type
    /// (a type parameter bounded by the class). Since the `this` expression evaluates
    /// to the concrete class instance type, we must substitute `ThisType` → class
    /// instance type before assignability checks. This matches tsc's behavior where
    /// `return this`, `f(this)`, etc. succeed when the target type is `this`.
    fn substitute_this_type_if_needed(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types can't contain ThisType
        if type_id.is_intrinsic() {
            return type_id;
        }

        let needs_substitution = tsz_solver::contains_this_type(self.ctx.types, type_id);

        if !needs_substitution {
            return type_id;
        }

        let Some(class_info) = &self.ctx.enclosing_class else {
            return type_id;
        };
        let class_idx = class_info.class_idx;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return type_id;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return type_id;
        };

        let instance_type = self.get_class_instance_type(class_idx, class_data);

        if tsz_solver::is_this_type(self.ctx.types, type_id) {
            instance_type
        } else {
            tsz_solver::substitute_this_type(self.ctx.types, type_id, instance_type)
        }
    }

    /// Shared assignability core: cache lookup → compute → cache insert → trace.
    ///
    /// Callers prepare evaluated source/target and supply `extra_flags` to OR
    /// into the base relation flags. This eliminates the duplicated
    /// cache+compute+trace sandwich from `is_assignable_to`, `_strict`, and
    /// `_strict_null`.
    fn check_assignability_cached(
        &mut self,
        source: TypeId,
        target: TypeId,
        extra_flags: u16,
        label: &str,
    ) -> bool {
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);
        let flags = self.ctx.pack_relation_flags() | extra_flags;

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);
            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let overrides = CheckerOverrideProvider::new(self, None);
        let relation_result = is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        );
        let result = relation_result.is_related();

        // TS2859: propagate depth-exceeded flag so callers can emit
        // "Excessive complexity comparing types" diagnostic.
        if relation_result.depth_exceeded {
            self.ctx.relation_depth_exceeded.set(true);
        }

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);
            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(source = source.0, target = target.0, result, "{label}");
        result
    }

    /// Prepare inputs common to all non-bivariant assignability checks:
    /// resolve lazy refs, substitute `ThisType`, and evaluate both sides.
    pub(crate) fn prepare_assignability_inputs(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (TypeId, TypeId) {
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);
        (source, target)
    }

    /// Execute a `RelationRequest` through the canonical boundary, returning
    /// a structured `RelationOutcome`.
    ///
    /// This is the single authoritative checker-level entry point for relation
    /// queries that need both the assignability result AND structured failure
    /// information. It replaces the pattern of calling `is_assignable_to` +
    /// `analyze_assignability_failure` + `is_weak_union_violation` separately.
    ///
    /// The request must contain **prepared** (evaluated) source/target types.
    pub(crate) fn execute_relation_request(
        &mut self,
        request: &crate::query_boundaries::assignability::RelationRequest,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        use crate::query_boundaries::assignability::execute_relation;

        let flags = self.ctx.pack_relation_flags();
        let overrides = CheckerOverrideProvider::new(self, None);

        let mut outcome = execute_relation(
            request,
            self.ctx.types,
            &self.ctx,
            flags,
            &self.ctx.inheritance_graph,
            &overrides,
            Some(&self.ctx),
            self.ctx.sound_mode(),
        );

        // Propagate relation depth exceeded to checker context for TS2859.
        if outcome.depth_exceeded {
            self.ctx.relation_depth_exceeded.set(true);
        }

        // Checker-only post-check: the solver may say "related" but the checker
        // can downgrade via deferred conditional types or other checker-specific
        // semantic rules.
        if outcome.related
            && self
                .checker_only_assignability_failure_reason(request.source, request.target)
                .is_some()
        {
            outcome.related = false;
        }

        outcome
    }

    /// Check if source type is assignable to target type.
    ///
    /// This is the main entry point for assignability checking, used throughout
    /// the type system to validate assignments, function calls, returns, etc.
    /// Assignability is more permissive than subtyping.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return false;
        }

        // Variance-aware fast path: when both source and target are Application
        // types with the same base (e.g., Covariant<A> vs Covariant<B>), check
        // type arguments using computed variance BEFORE structural expansion.
        // This must run before evaluate_type_for_assignability which would
        // expand Application types to structural objects, losing variance info.
        {
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        // Pre-evaluation IndexAccess identity check: when both source and target are
        // IndexAccess types whose object types are type parameters with the same name,
        // accept the relationship before evaluation can destroy type parameter identity.
        // Example: `T_229[K] <: T_420[K]` where T_229 (unconstrained, from type alias)
        // and T_420 (constrained `extends object`, from function) share name "T".
        // Without this, evaluation resolves T_420 to `object`, losing the name match.
        if let Some((s_obj, s_idx)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                source,
            )
            && let Some((t_obj, t_idx)) =
                crate::query_boundaries::checkers::generic::index_access_components(
                    self.ctx.types,
                    target,
                )
            && let Some(s_param) = tsz_solver::visitor::type_param_info(self.ctx.types, s_obj)
            && let Some(t_param) = tsz_solver::visitor::type_param_info(self.ctx.types, t_obj)
            && s_param.name == t_param.name
            && self.is_assignable_to(s_idx, t_idx)
        {
            return true;
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);

        // Guard: if evaluation degraded a valid type to ERROR (e.g., due to the
        // stack overflow protection tripping during deep recursive type resolution),
        // preserve the pre-evaluation type. ERROR is treated as assignable to/from
        // everything by the subtype checker, which would silently suppress real type
        // errors (like TS2322 for property mismatches in object literals with
        // recursive interface targets). Keeping the original Lazy type allows the
        // compat checker's resolver to resolve it from the type environment, which
        // was populated during earlier successful resolution.
        let source = if source_eval == TypeId::ERROR && source != TypeId::ERROR {
            source
        } else {
            source_eval
        };
        let target = if target_eval == TypeId::ERROR && target != TypeId::ERROR {
            target
        } else {
            target_eval
        };

        let result = self.check_assignability_cached(source, target, 0, "is_assignable_to");

        if result
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }

        // Post-check: keyof type checking logic
        if let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            if !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    fn is_concrete_source_to_deferred_keyof_index_access(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((object_type, index_type)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                target,
            )
        else {
            return false;
        };

        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
        {
            return false;
        }

        if !self.is_deferred_generic_index_for_object(index_type, object_type) {
            return false;
        }

        let mut candidate_types = Vec::new();
        self.collect_deferred_index_access_candidate_types(object_type, &mut candidate_types);

        if candidate_types.is_empty() {
            return crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                object_type,
            );
        }

        candidate_types
            .into_iter()
            .any(|candidate| !self.ctx.types.is_assignable_to(source, candidate))
    }

    fn is_deferred_generic_index_for_object(
        &self,
        index_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members
                .iter()
                .copied()
                .any(|member| self.is_deferred_generic_index_for_object(member, object_type));
        }

        if let Some(keyof_operand) = get_keyof_type(self.ctx.types, index_type) {
            return keyof_operand == object_type;
        }

        if let Some(param_info) = tsz_solver::visitor::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_operand) = get_keyof_type(self.ctx.types, constraint)
        {
            return keyof_operand == object_type;
        }

        false
    }

    fn collect_deferred_index_access_candidate_types(
        &mut self,
        object_type: TypeId,
        candidate_types: &mut Vec<TypeId>,
    ) {
        if let Some(param_info) = tsz_solver::visitor::type_param_info(self.ctx.types, object_type)
            && let Some(constraint) = param_info.constraint
        {
            self.collect_deferred_index_access_candidate_types(constraint, candidate_types);
            return;
        }

        self.ensure_relation_input_ready(object_type);
        let evaluated = self.evaluate_type_for_assignability(object_type);
        if evaluated != object_type && evaluated != TypeId::ERROR {
            self.collect_deferred_index_access_candidate_types(evaluated, candidate_types);
            if !candidate_types.is_empty() {
                return;
            }
        }

        if let Some(members) = crate::query_boundaries::common::union_members(
            self.ctx.types,
            object_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::intersection_members(self.ctx.types, object_type)
        }) {
            for member in members.iter().copied() {
                self.collect_deferred_index_access_candidate_types(member, candidate_types);
            }
            return;
        }

        let shape_id =
            tsz_solver::visitor::object_shape_id(self.ctx.types, object_type).or_else(|| {
                tsz_solver::visitor::object_with_index_shape_id(self.ctx.types, object_type)
            });

        if let Some(shape_id) = shape_id {
            let shape = self.ctx.types.object_shape(shape_id);
            candidate_types.extend(shape.properties.iter().map(|prop| {
                if prop.optional {
                    self.ctx.types.union2(prop.type_id, TypeId::UNDEFINED)
                } else {
                    prop.type_id
                }
            }));
        }

        let index_info = self.ctx.types.get_index_signatures(object_type);
        if let Some(string_index) = index_info.string_index {
            candidate_types.push(string_index.value_type);
        }
        if let Some(number_index) = index_info.number_index {
            candidate_types.push(number_index.value_type);
        }
    }

    /// Like `is_assignable_to`, but skips weak type checks (TS2559).
    ///
    /// This matches tsc's `isTypeAssignableTo` behavior, which does NOT
    /// include the weak type check. Used by the flow narrowing guard to
    /// avoid rejecting valid type-guard narrowing (e.g., instanceof).
    pub fn is_assignable_to_no_weak_checks(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let overrides = CheckerOverrideProvider::new(self, None);
        crate::query_boundaries::assignability::is_assignable_no_weak_checks(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags: self.ctx.pack_relation_flags(),
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        )
    }

    /// Like `is_assignable_to`, but disables generic type parameter erasure.
    ///
    /// Used for implements/extends member type checking (TS2416) where tsc's
    /// `compareSignaturesRelated` does NOT erase target type parameters.
    /// A non-generic `(x: string) => string` is NOT assignable to a generic
    /// `<T>(x: T) => T` under this mode.
    pub fn is_assignable_to_no_erase_generics(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::NO_ERASE_GENERICS,
            "is_assignable_to_no_erase_generics",
        )
    }

    /// Like `is_assignable_to`, but forces the strict-function-types relation flag.
    pub fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES,
            "is_assignable_to_strict",
        )
    }

    /// Check assignability while forcing strict null checks in relation flags.
    ///
    /// This keeps the regular checker/solver assignability gateway (resolver,
    /// overrides, caching, and precondition setup) while pinning nullability
    /// semantics to strict mode for localized checks.
    pub fn is_assignable_to_strict_null(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::STRICT_NULL_CHECKS,
            "is_assignable_to_strict_null",
        )
    }

    /// Check assignability with the current `TypeEnvironment` but without
    /// consulting the checker's relation caches.
    ///
    /// Generic call/new inference uses this after instantiation to avoid stale
    /// relation answers while still going through the same input preparation as
    /// the normal assignability gateway.
    pub fn is_assignable_to_with_env(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let target = self.substitute_this_type_if_needed(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return false;
        }

        {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &*env,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let result = {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let overrides = CheckerOverrideProvider::new(self, Some(&*env));
            let relation_result = is_assignable_with_overrides(
                &AssignabilityQueryInputs {
                    db: self.ctx.types,
                    resolver: &*env,
                    source,
                    target,
                    flags,
                    inheritance_graph: &self.ctx.inheritance_graph,
                    sound_mode: self.ctx.sound_mode(),
                },
                &overrides,
            );
            if relation_result.depth_exceeded {
                self.ctx.relation_depth_exceeded.set(true);
            }
            relation_result.is_related()
        };

        if result
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }

        if let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            if !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables `strict_function_types` for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_relation_inputs_ready(&[source, target]);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);

        // For bivariant checks, we strip the strict_function_types flag
        // so the cache key is distinct from regular assignability checks.
        let flags = self.ctx.pack_relation_flags()
            & !crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES;

        if is_cacheable {
            // Note: For assignability checks, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        // Preserve existing behavior: bivariant path does not use checker overrides.
        let result = is_assignable_bivariant_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );

        // Cache the result for non-inference types
        // Use ORIGINAL types for cache key (not evaluated types)
        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to_bivariant"
        );
        result
    }

    /// Check if two types have any overlap (can ever be equal).
    ///
    /// Used for TS2367: "This condition will always return 'false'/'true' since
    /// the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if the types can potentially be equal, false if they can never
    /// have any common value.
    pub fn are_types_overlapping(&mut self, left: TypeId, right: TypeId) -> bool {
        // Ensure centralized relation preconditions before overlap check.
        self.ensure_relation_input_ready(left);
        self.ensure_relation_input_ready(right);

        let env = self.ctx.type_env.borrow();
        are_types_overlapping_with_env(
            self.ctx.types,
            &env,
            left,
            right,
            self.ctx.strict_null_checks(),
        )
    }
}
