//! Type assignability and excess property checking.
//!
//! Subtype, identity, and redeclaration compatibility live in
//! `subtype_identity_checker`.

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, AssignabilityQueryInputs, ExcessPropertiesKind,
    are_types_overlapping_with_env, check_application_variance_assignability,
    check_assignable_gate_with_overrides, classify_for_assignability_eval,
    classify_for_excess_properties, contains_infer_types, get_allowed_keys, get_keyof_type,
    get_string_literal_value, get_union_members, is_assignable_bivariant_with_resolver,
    is_assignable_with_overrides, is_keyof_type, is_relation_cacheable, is_type_parameter_like,
    keyof_object_properties, map_compound_members, object_shape_for_type,
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
    fn callable_has_own_generic_signatures(&self, type_id: TypeId) -> bool {
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
            tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
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
        let mut changed = false;
        let params = shape
            .params
            .iter()
            .map(|param| {
                let evaluated = self.normalize_nested_type_for_assignability(param.type_id);
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
            let evaluated = self.normalize_nested_type_for_assignability(shape.return_type);
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
                target: predicate.target.clone(),
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
                            type_predicate: sig.type_predicate.clone(),
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
                            type_predicate: sig.type_predicate.clone(),
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
                        string_index: shape.string_index.clone(),
                        number_index: shape.number_index.clone(),
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

    fn get_keyof_type_keys(
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

    fn narrow_this_from_enclosing_typeof_guard(
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
        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            // Inference placeholders are transient solver state. Emitting TS2322/TS2345
            // while they are still present creates contextual false positives.
            || contains_infer_types(self.ctx.types, source)
            || contains_infer_types(self.ctx.types, target)
    }

    /// Suppress assignability diagnostics for parser-recovery artifacts.
    fn should_suppress_assignability_for_parse_recovery(
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
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if refs_resolution_fuel_exhausted() {
                    break;
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                increment_refs_resolution_fuel();
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
        let mut evaluated = match classify_for_assignability_eval(self.ctx.types, type_id) {
            AssignabilityEvalKind::Application => self.evaluate_type_with_resolution(type_id),
            AssignabilityEvalKind::NeedsEnvEval => self.evaluate_type_with_env(type_id),
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
        let result = is_assignable_with_overrides(
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

        if is_cacheable {
            let cache_key = RelationCacheKey::assignability(source, target, flags, 0);
            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(source = source.0, target = target.0, result, "{label}");
        result
    }

    /// Prepare inputs common to all non-bivariant assignability checks:
    /// resolve lazy refs, substitute `ThisType`, and evaluate both sides.
    fn prepare_assignability_inputs(&mut self, source: TypeId, target: TypeId) -> (TypeId, TypeId) {
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);
        (source, target)
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

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

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
        &self,
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

        if !tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, object_type) {
            return false;
        }

        let Some(keyof_operand) = get_keyof_type(self.ctx.types, index_type) else {
            return false;
        };

        if keyof_operand != object_type {
            return false;
        }

        !crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
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
            is_assignable_with_overrides(
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
            )
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

    // =========================================================================
    // Weak Union and Excess Property Checking
    // =========================================================================

    /// Check if we should skip the general assignability error for an object literal.
    /// Returns true if:
    /// 1. It's a weak union violation (TypeScript shows excess property error instead)
    /// 2. OR if the object literal has excess properties (TypeScript prioritizes TS2353 over TS2345/TS2322)
    pub(crate) fn should_skip_weak_union_error(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation first (using scoped borrow)
        if self.is_weak_union_violation(source, target) {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target, source_idx) {
            return false;
        }

        // There are excess properties. Check if all matching properties have compatible types.
        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return true;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(target_shape) = object_shape_for_type(self.ctx.types, resolved_target) else {
            // If we can't extract a simple object shape from the target (e.g., it's
            // an intersection with a deferred conditional type), we should NOT skip
            // the assignability error. The solver already determined the types are
            // incompatible, and inability to extract properties for excess-property
            // analysis doesn't mean the assignment is valid.
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        let target_props = target_shape.properties.as_slice();

        // Check if any source property that exists in target has a wrong type.
        // Also collect the matching properties so we can verify structural assignability.
        let mut matching_props = Vec::new();
        for source_prop in source_props {
            if let Some(target_prop) = target_props.iter().find(|p| p.name == source_prop.name) {
                let source_prop_type = source_prop.type_id;
                let target_prop_type = target_prop.type_id;

                let effective_target_type = if target_prop.optional {
                    self.ctx
                        .types
                        .union(vec![target_prop_type, TypeId::UNDEFINED])
                } else {
                    target_prop_type
                };

                let is_assignable =
                    { self.is_assignable_to(source_prop_type, effective_target_type) };

                if !is_assignable {
                    return false;
                }
                matching_props.push(source_prop.clone());
            }
        }

        // All matching properties are compatible. Verify that the failure is truly
        // caused by excess properties alone by checking if an object with only the
        // matching properties would be assignable. If not, the failure is structural
        // (e.g., target contains a deferred conditional type) and we should NOT
        // suppress TS2322.
        let trimmed_source = self.ctx.types.object(matching_props);
        if !self.is_assignable_to(trimmed_source, target) {
            return false;
        }

        true
    }

    /// Check assignability and emit the standard TS2322/TS2345-style diagnostic when needed.
    /// `keyword_pos` is the source position of the `satisfies` keyword for accurate TS1360 spans.
    pub(crate) fn check_satisfies_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) -> bool {
        let diag_idx = source_idx;
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }

        if is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) = get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type = get_keyof_type(self.ctx.types, target)
                .expect("is_keyof_type guard ensures this succeeds");
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            if !allowed_keys.contains(&str_lit) {
                self.error_type_does_not_satisfy_the_expected_type(
                    source,
                    target,
                    diag_idx,
                    keyword_pos,
                );
                return false;
            }
        }

        // Track whether excess property checking emits diagnostics.
        // When TS2353 is emitted for excess properties, tsc does NOT also emit TS1360.
        let mut had_excess_property_error = false;
        if let Some(node) = self.ctx.arena.get(source_idx)
            && node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let diags_before = self.ctx.diagnostics.len();
            self.check_object_literal_excess_properties(source, target, source_idx);
            had_excess_property_error = self.ctx.diagnostics.len() > diags_before;
        }

        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }

        // tsc 6.0: `satisfies` ignores readonly-to-mutable mismatches.
        // `[1,2,3] as const satisfies unknown[]` is accepted because `satisfies`
        // checks structural shape, not mutability. If the source is Readonly<T>,
        // try checking T against the target.
        if let Some(inner) = tsz_solver::readonly_inner_type(self.ctx.types, source)
            && self.is_assignable_to(inner, target)
        {
            return true;
        }

        // If excess property errors were already emitted, skip the general TS1360.
        // This matches tsc: when TS2353 is reported, the "does not satisfy" error
        // is suppressed to avoid redundant diagnostics.
        if had_excess_property_error {
            return false;
        }

        // Elaborate: for object literal sources, drill into property-level errors
        // instead of reporting the generic TS1360. This matches tsc behavior where
        // `{ s: "false" } satisfies { [key: string]: boolean }` reports TS2322 at
        // the specific mismatching property rather than TS1360 on the whole expression.
        if let Some(node) = self.ctx.arena.get(source_idx)
            && node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let elaborated =
                self.elaborate_satisfies_object_literal(source, target, source_idx, keyword_pos);
            if elaborated {
                return false;
            }
        }

        self.error_type_does_not_satisfy_the_expected_type(source, target, diag_idx, keyword_pos);
        false
    }

    /// Elaborate a `satisfies` failure for object literal expressions by checking
    /// each property against the target type's index signature or named properties.
    /// Returns true if elaboration produced property-level diagnostics.
    fn elaborate_satisfies_object_literal(
        &mut self,
        _source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        _keyword_pos: Option<u32>,
    ) -> bool {
        let resolved_target = self.resolve_type_for_property_access(target);
        let target_shape = match object_shape_for_type(self.ctx.types, resolved_target) {
            Some(shape) => shape,
            None => return false,
        };

        // Get the index signature value type from the target
        let index_value_type = target_shape.string_index.as_ref().map(|sig| sig.value_type);

        let Some(index_value_type) = index_value_type else {
            // No string index signature — try elaborating against named target properties.
            // For targets with named properties (like interfaces), check if there are
            // missing required properties (TS2741 elaboration) — handled elsewhere.
            return false;
        };

        // Iterate over the object literal's AST properties and check each value
        let Some(lit_data) = self.ctx.arena.get_literal_expr_at(source_idx) else {
            return false;
        };
        let elements: Vec<NodeIndex> = lit_data.elements.nodes.to_vec();

        let diag_count_before = self.ctx.diagnostics.len();

        for &elem_idx in &elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop_data) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };

            // Get the type of the property value (the initializer)
            let prop_value_type = self.get_type_of_node(prop_data.initializer);
            self.ensure_relation_input_ready(prop_value_type);
            self.ensure_relation_input_ready(index_value_type);

            // Check nested object literal excess properties FIRST — tsc prioritizes
            // excess property errors (TS2353) over assignability errors (TS2322).
            // e.g., `{ r: 0, g: 0, d: 0 }` vs `Color` reports "d does not exist" (TS2353)
            // rather than "missing b" (TS2322).
            if let Some(val_node) = self.ctx.arena.get(prop_data.initializer)
                && val_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                let diags_before = self.ctx.diagnostics.len();
                self.check_object_literal_excess_properties(
                    prop_value_type,
                    index_value_type,
                    prop_data.initializer,
                );
                if self.ctx.diagnostics.len() > diags_before {
                    // Excess property errors were reported — skip assignability check
                    continue;
                }
            }

            if !self.is_assignable_to(prop_value_type, index_value_type) {
                // Report TS2322 at the property name (use _with_anchor to avoid
                // assignment_diagnostic_anchor_idx walking up to the variable declaration)
                self.error_type_not_assignable_at_with_anchor(
                    prop_value_type,
                    index_value_type,
                    prop_data.name,
                );
            }
        }

        self.ctx.diagnostics.len() > diag_count_before
    }

    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an assignability diagnostic was emitted.
    pub(crate) fn check_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at(source, target, source_idx, source_idx)
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics with independent
    /// source and diagnostic anchors.
    ///
    /// `source_idx` is used for weak-union/excess-property prioritization.
    /// `diag_idx` is where the assignability diagnostic is anchored.
    pub(crate) fn check_assignable_or_report_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }

        if is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) = get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type = get_keyof_type(self.ctx.types, target)
                .expect("is_keyof_type guard ensures this succeeds");
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            // Only use this pre-check when we could determine concrete keys.
            // An empty set means the inner type couldn't be resolved (e.g., it's
            // an Application, Mapped type with as-clause, or Lazy reference).
            // In that case, fall through to the solver's assignability check which
            // correctly evaluates keyof through the full type evaluation pipeline.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&str_lit) {
                self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
                return false;
            }
        }

        if let Some(allowed) =
            self.numeric_enum_assignment_override_from_source(source, target, source_idx)
        {
            if allowed {
                return true;
            }
            if self.try_elaborate_assignment_source_error(source_idx, target) {
                return false;
            }
            self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
            return false;
        }

        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        if self.try_elaborate_assignment_source_error(source_idx, target) {
            return false;
        }
        self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
        false
    }

    fn numeric_enum_assignment_override_from_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> Option<bool> {
        use tsz_solver::TypeResolver;
        let target = self.evaluate_type_for_assignability(target);
        let target_def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target)?;
        if !self.ctx.is_numeric_enum(target_def_id) {
            return None;
        }

        let source_literal = self.literal_type_from_initializer(source_idx);
        let source_is_number_like = source == TypeId::NUMBER
            || source_literal.is_some_and(|lit| {
                tsz_solver::type_queries::extended::is_number_literal(self.ctx.types, lit)
            });
        if !source_is_number_like {
            return None;
        }

        if self.ctx.is_enum_type(target, self.ctx.types) {
            if let Some(source_literal) = source_literal {
                let structural_target =
                    tsz_solver::type_queries::data::get_enum_member_type(self.ctx.types, target)
                        .unwrap_or(target);
                return Some(self.is_assignable_to(source_literal, structural_target));
            }
            return None;
        }

        let target_member =
            tsz_solver::type_queries::data::get_enum_member_type(self.ctx.types, target);
        let target_literal =
            target_member.and_then(|member| tsz_solver::literal_value(self.ctx.types, member));

        target_member?;

        match source_literal {
            Some(source_literal) => {
                let source_val = tsz_solver::literal_value(self.ctx.types, source_literal);
                match (source_val, target_literal) {
                    (
                        Some(tsz_solver::LiteralValue::Number(source_num)),
                        Some(tsz_solver::LiteralValue::Number(target_num)),
                    ) => Some(source_num == target_num),
                    _ => Some(false),
                }
            }
            None => (source == TypeId::NUMBER).then_some(true),
        }
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics anchored
    /// exactly at `diag_idx`, without assignment-anchor rewriting.
    pub(crate) fn check_assignable_or_report_at_exact_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        if self.try_elaborate_assignment_source_error(source_idx, target) {
            return false;
        }
        self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
        false
    }

    /// Check assignability and emit a generic TS2322 diagnostic at `diag_idx`.
    ///
    /// This is used for call sites that intentionally avoid detailed reason rendering
    /// but still share centralized mismatch/suppression behavior.
    pub(crate) fn check_assignable_or_report_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(arg_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(arg_idx, arg_idx) {
            return true;
        }
        if self.is_assignable_to(source, target) {
            return true;
        }
        if self.should_skip_weak_union_error(source, target, arg_idx) {
            return true;
        }
        // Conditional/generic callback contexts can narrow argument callback parameter
        // types to intersections involving type parameters (e.g. `number & T`).
        // In these cases, strict contravariant checking reports TS2345 even when the
        // concrete expected callback type is assignable to the narrowed callback.
        // tsc defers this mismatch.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            && !crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
            && tsz_solver::type_queries::is_callable_type(self.ctx.types, source)
            && tsz_solver::type_queries::is_callable_type(self.ctx.types, target)
            && !self.callable_has_own_generic_signatures(source)
            && self.ctx.types.is_assignable_to(target, source)
        {
            return true;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }

    /// Returns true when an assignability mismatch should produce a diagnostic.
    ///
    /// This centralizes the standard "not assignable + not weak-union/excess-property
    /// suppression" decision so call sites emitting different diagnostics can share it.
    pub(crate) fn should_report_assignability_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Mirrors `should_report_assignability_mismatch` but uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to_bivariant(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable_to(left, right) && self.is_assignable_to(right, left)
    }

    /// Check if two types are comparable (overlap).
    ///
    /// Corresponds to TypeScript's `areTypesComparable`: returns true if the types
    /// have any overlap. TSC's comparableRelation differs from assignability:
    /// - For union sources: uses `someTypeRelatedToType` (ANY member suffices)
    /// - For union targets: also checks per-member overlap
    /// - For `TypeParameter` sources: uses apparent type (constraint or `unknown`)
    /// - Special carve-out: two unrelated type params are NOT comparable
    ///
    /// Used for switch/case comparability (TS2678), equality narrowing,
    /// relational operator checks (TS2365), etc.
    pub(crate) fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        // Identity: any type is trivially comparable to itself
        if source == target {
            return true;
        }

        // Resolve type parameters to their apparent types for comparison.
        // In tsc, `isTypeComparableTo` uses `getReducedApparentType` for TypeParam sources,
        // and has a carve-out when BOTH source and target are type parameters (only comparable
        // if one constrains to the other). See tsc checker.ts:23671-23684.
        let source_is_tp = is_type_parameter_like(self.ctx.types, source);
        let target_is_tp = is_type_parameter_like(self.ctx.types, target);

        if source_is_tp && target_is_tp {
            // Both are type parameters: only comparable if one constrains to the other.
            // Unconstrained T is NOT comparable to unconstrained U.
            return self.type_params_are_comparable(source, target);
        }

        // Resolve type parameter to apparent type (constraint or `unknown`)
        let source_apparent = if source_is_tp {
            self.get_type_param_apparent_type(source)
        } else {
            source
        };
        let target_apparent = if target_is_tp {
            self.get_type_param_apparent_type(target)
        } else {
            target
        };

        // Fast path: direct bidirectional assignability (with apparent types)
        if self.is_assignable_to(source_apparent, target_apparent)
            || self.is_assignable_to(target_apparent, source_apparent)
        {
            return true;
        }

        // TSC's comparable relation decomposes unions and checks if ANY member
        // is related to the other type. This handles cases like:
        // - `User.A | User.B` comparable to `User.A` (User.A member matches)
        // - `string & Brand` comparable to `"a"` (string member of intersection)

        // Decompose source union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self.is_assignable_to(*member, target_apparent)
                    || self.is_assignable_to(target_apparent, *member)
                {
                    return true;
                }
            }
        }

        // Decompose target union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.is_assignable_to(source_apparent, *member)
                    || self.is_assignable_to(*member, source_apparent)
                {
                    return true;
                }
            }
        }

        // Decompose intersection: `"a"` is comparable to `string & Brand` because
        // `"a"` is assignable to `string` (one constituent). tsc's comparable relation
        // treats intersections as comparable if the source overlaps with ANY member.
        if let Some(members) = query::intersection_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self.is_assignable_to(*member, target_apparent)
                    || self.is_assignable_to(target_apparent, *member)
                {
                    return true;
                }
            }
        }
        if let Some(members) = query::intersection_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.is_assignable_to(source_apparent, *member)
                    || self.is_assignable_to(*member, source_apparent)
                {
                    return true;
                }
            }
        }

        // Additional check: Two object types where ALL properties are optional always
        // overlap at `{}`, making them comparable even if property types differ.
        // Example: `{ b?: number }` vs `{ b?: string }` are comparable because both
        // include `{}` as a valid value.
        if self.objects_with_all_optional_common_props_overlap(source_apparent, target_apparent) {
            return true;
        }

        false
    }

    /// Get the apparent type for a type parameter (constraint or `unknown` for unconstrained).
    /// This matches tsc's `getReducedApparentType` behavior for type parameters.
    fn get_type_param_apparent_type(&self, type_id: TypeId) -> TypeId {
        tsz_solver::type_param_info(self.ctx.types, type_id)
            .and_then(|info| info.constraint)
            .unwrap_or(TypeId::UNKNOWN)
    }

    /// Check if two type parameters are comparable (one constrains to the other).
    /// In tsc, two unconstrained type parameters are NOT comparable (tsc checker.ts:23671-23684).
    fn type_params_are_comparable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Check if source's constraint chain reaches target.
        // For union constraints (e.g., U extends T | string), check if any member
        // of the union is assignable to the target. This handles cases like
        // `U extends T | string` being comparable to `T`.
        if let Some(info) = tsz_solver::type_param_info(self.ctx.types, source)
            && let Some(constraint) = info.constraint
        {
            if self.is_assignable_to(constraint, target) {
                return true;
            }
            // Decompose union constraints: if any member is comparable/assignable to target
            if let Some(members) =
                crate::query_boundaries::dispatch::union_members(self.ctx.types, constraint)
            {
                for member in &members {
                    if *member == target || self.is_assignable_to(*member, target) {
                        return true;
                    }
                }
            }
        }
        // Check if target's constraint chain reaches source
        if let Some(info) = tsz_solver::type_param_info(self.ctx.types, target)
            && let Some(constraint) = info.constraint
        {
            if self.is_assignable_to(source, constraint) {
                return true;
            }
            // Decompose union constraints for target
            if let Some(members) =
                crate::query_boundaries::dispatch::union_members(self.ctx.types, constraint)
            {
                for member in &members {
                    if *member == source || self.is_assignable_to(source, *member) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if source object literal has properties that don't exist in target.
    ///
    /// Uses TypeId-based freshness tracking (fresh object literals only).
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        _source_idx: NodeIndex,
    ) -> bool {
        use tsz_solver::relations::freshness;
        // Only fresh object literals trigger excess property checking.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        if source_props.is_empty() {
            return false;
        }

        let resolved_target = self.resolve_type_for_property_access(target);

        match classify_for_excess_properties(self.ctx.types, resolved_target) {
            ExcessPropertiesKind::Object(shape_id) => {
                let target_shape = self.ctx.types.object_shape(shape_id);
                let target_props = target_shape.properties.as_slice();

                if target_props.is_empty() {
                    return false;
                }

                if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                    return false;
                }

                source_props
                    .iter()
                    .any(|source_prop| !target_props.iter().any(|p| p.name == source_prop.name))
            }
            ExcessPropertiesKind::ObjectWithIndex(_shape_id) => false,
            ExcessPropertiesKind::Union(members) => {
                let mut target_shapes = Vec::new();
                let mut matched_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        // If a union member has no object shape and is a type parameter
                        // or the `object` intrinsic, it accepts any properties, so EPC
                        // should not apply.
                        if is_type_parameter_like(self.ctx.types, resolved_member)
                            || resolved_member == TypeId::OBJECT
                        {
                            return false;
                        }
                        continue;
                    };

                    if shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                    {
                        return false;
                    }

                    target_shapes.push(shape.clone());

                    if self.is_subtype_of(source, resolved_member) {
                        matched_shapes.push(shape);
                    }
                }

                if target_shapes.is_empty() {
                    return false;
                }

                let effective_shapes = if matched_shapes.is_empty() {
                    target_shapes
                } else {
                    matched_shapes
                };

                source_props.iter().any(|source_prop| {
                    !effective_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::Intersection(members) => {
                let mut target_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        // If an intersection member is a type parameter, it could accept
                        // any properties, so EPC should not apply (same logic as union case).
                        if is_type_parameter_like(self.ctx.types, resolved_member)
                            || resolved_member == TypeId::OBJECT
                        {
                            return false;
                        }
                        continue;
                    };

                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        return false;
                    }

                    target_shapes.push(shape);
                }

                if target_shapes.is_empty() {
                    return false;
                }

                source_props.iter().any(|source_prop| {
                    !target_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::NotObject => false,
        }
    }

    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        // Keep failure analysis on the same relation boundary as `is_assignable_to`
        // (CheckerContext resolver + checker overrides) so mismatch suppression and
        // diagnostic rendering observe identical compatibility semantics.
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source,
            target,
            flags: self.ctx.pack_relation_flags(),
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
        };
        let gate = check_assignable_gate_with_overrides(&inputs, &overrides, Some(&self.ctx), true);
        if gate.related
            && let Some(reason) = self.checker_only_assignability_failure_reason(source, target)
        {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }
        if gate.related {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            };
        }
        let mut result = gate.analysis.unwrap_or(
            crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            },
        );

        // When the failure is ExcessProperty but the target contains a deferred
        // conditional type, the real issue is structural (the deferred conditional
        // makes the assignment incompatible regardless of excess properties).
        // tsc emits TS2322 rather than TS2353 in this case. Evaluate the target
        // to check for conditional members and downgrade to a generic TS2322.
        if matches!(
            &result.failure_reason,
            Some(tsz_solver::SubtypeFailureReason::ExcessProperty { .. })
        ) {
            let evaluated_target = self.evaluate_type_for_assignability(target);
            if tsz_solver::has_deferred_conditional_member(self.ctx.types, evaluated_target) {
                result.failure_reason = None;
            }
        }

        result
    }

    pub(crate) fn is_weak_union_violation(&mut self, source: TypeId, target: TypeId) -> bool {
        self.analyze_assignability_failure(source, target)
            .weak_union_violation
    }

    const fn checker_only_assignability_failure_reason(
        &mut self,
        _source: TypeId,
        _target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        None
    }
}
