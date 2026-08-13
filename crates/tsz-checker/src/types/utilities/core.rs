//! Parameter type utilities, type construction, and type resolution methods
//! for `CheckerState`.

use super::heritage_walk_state::HeritageSymbolWalkState;
use crate::query_boundaries::enum_analysis as enum_query;
use crate::query_boundaries::type_checking_utilities as query;
use crate::state::{CheckerState, EnumKind};
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Result from resolving literal string keys against an object type.
pub(crate) struct LiteralKeysResult {
    /// The computed result type (union/intersection of found key types).
    /// `None` only when the lookup itself failed (e.g., object was unknown).
    pub result_type: Option<TypeId>,
    /// Keys that were not found as properties on the object type.
    /// When non-empty, the caller should emit TS2339 for each.
    pub missing_keys: Vec<String>,
}

impl<'a> CheckerState<'a> {
    // ============================================================================
    // Section 52: Parameter Type Utilities
    // ============================================================================

    /// Assign contextual types to destructuring parameters (binding patterns).
    ///
    /// When a function has a contextual type (e.g., from a callback position),
    /// destructuring parameters need to have their bindings inferred from
    /// the contextual parameter type.
    ///
    /// This function only processes parameters without explicit type annotations,
    /// as TypeScript respects explicit annotations over contextual inference.
    ///
    /// ## Examples:
    /// ```typescript
    /// declare function map<T, U>(arr: T[], fn: (item: T) => U): U[];
    ///
    /// // x and y types come from contextual type T
    /// map(arr, ({ x, y }) => x + y);
    ///
    /// // Explicit annotation takes precedence
    /// map(arr, ({ x, y }: { x: string; y: number }) => x + y);
    /// ```
    pub(crate) fn assign_contextual_types_to_destructuring_params(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            if param.type_annotation.is_some() {
                continue;
            }

            // Only process binding patterns (destructuring)
            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

            if !is_binding_pattern {
                continue;
            }

            // Get the contextual type for this parameter position
            let contextual_type = param_types
                .get(i)
                .and_then(|t| *t)
                .filter(|&t| t != TypeId::UNKNOWN && t != TypeId::ERROR);

            if let Some(mut ctx_type) = contextual_type {
                if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ctx_type)
                    && crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        ctx_type,
                    )
                    .is_none()
                {
                    continue;
                }
                // When the parameter has a default value (e.g., `{ x } = {}`),
                // strip `undefined` from the contextual type since the default
                // guarantees the destructured value is not undefined. Without
                // this, `T | undefined` causes false TS2339 on destructured
                // property access.
                if param.initializer.is_some() {
                    ctx_type =
                        crate::query_boundaries::common::remove_undefined(self.ctx.types, ctx_type);
                }
                // Assign the contextual type to the binding pattern elements
                let request = crate::context::TypingRequest::with_contextual_type(ctx_type);
                self.assign_binding_pattern_symbol_types_with_request(
                    param.name, ctx_type, &request,
                );
            }
        }
    }

    /// Validate top-level destructuring patterns of closure parameters against
    /// their externally-derived contextual types.
    ///
    /// For function declarations and methods, `check_parameter_binding_pattern_defaults`
    /// already runs these checks. Closures (arrow/function expressions) skip that path,
    /// so this helper covers the closure-specific gap when the parameter type is
    /// derived from a contextual signature or an IIFE call argument.
    ///
    /// Emits:
    /// - TS2488 (and its ES5 variants) for an array binding pattern destructuring
    ///   a non-iterable contextual type, e.g. `(([]) => 0)({})`.
    /// - TS2532 for an empty object binding pattern destructuring a possibly-undefined
    ///   contextual type, e.g. `(({}) => 0)(undefined)`.
    pub(crate) fn check_closure_destructuring_top_level_diagnostics(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.type_annotation.is_some() {
                continue;
            }
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let is_array = name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
            let is_object = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;
            if !is_array && !is_object {
                continue;
            }
            let Some(ctx_type) = param_types
                .get(i)
                .and_then(|t| *t)
                .filter(|&t| t != TypeId::ANY && t != TypeId::UNKNOWN && t != TypeId::ERROR)
            else {
                continue;
            };
            if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ctx_type)
                && crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    ctx_type,
                )
                .is_none()
            {
                continue;
            }
            // When the parameter has a default value, the destructure happens
            // against the type with `undefined` removed, matching tsc.
            let effective_type = if param.initializer.is_some() {
                crate::query_boundaries::common::remove_undefined(self.ctx.types, ctx_type)
            } else {
                ctx_type
            };
            if is_array {
                self.check_destructuring_iterability(param.name, effective_type, param.initializer);
            } else if is_object {
                let pattern_data = self
                    .ctx
                    .arena
                    .get(param.name)
                    .and_then(|n| self.ctx.arena.get_binding_pattern(n));
                let elements_empty = pattern_data
                    .map(|p| p.elements.nodes.is_empty())
                    .unwrap_or(false);
                // Same `checkNonNullNonVoidType` arm as the binding-pattern
                // statement path: strict-only in tsc, so the gate stays here.
                if elements_empty && self.ctx.compiler_options.strict_null_checks {
                    let (non_nullish_type, nullish_cause) = self.split_nullish_type(effective_type);
                    if let Some(cause) = nullish_cause {
                        self.report_nullish_object(param.name, cause, non_nullish_type.is_none());
                    }
                }
            }
        }
    }

    /// Record destructured parameter binding groups for correlated narrowing.
    ///
    /// This enables cases like:
    /// `function f({ data, isSuccess }: Result) { if (isSuccess) data... }`
    /// where narrowing one binding should narrow sibling bindings from the same source union.
    pub(crate) fn record_destructured_parameter_binding_groups(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        use crate::query_boundaries::state::checking as state_query;

        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
            if !is_binding_pattern {
                continue;
            }
            let Some(pattern_data) = self.ctx.arena.get_binding_pattern(name_node) else {
                continue;
            };
            let direct_identifier_count = pattern_data
                .elements
                .nodes
                .iter()
                .filter_map(|&element_idx| self.ctx.arena.get(element_idx))
                .filter_map(|element_node| self.ctx.arena.get_binding_element(element_node))
                .filter(|element| {
                    self.ctx
                        .arena
                        .get(element.name)
                        .is_some_and(|name| name.kind == SyntaxKind::Identifier as u16)
                })
                .take(2)
                .count();
            if direct_identifier_count < 2 {
                continue;
            }

            let Some(param_type) = param_types.get(i).and_then(|t| *t) else {
                continue;
            };
            if param_type == TypeId::UNKNOWN || param_type == TypeId::ERROR {
                continue;
            }
            if !self.destructured_param_type_may_resolve_to_union(param_type, 0) {
                continue;
            }

            let mut resolved_for_union = self.evaluate_type_with_env(param_type);
            if state_query::union_members(self.ctx.types, resolved_for_union).is_none()
                && let Some(constraint) =
                    state_query::type_parameter_constraint(self.ctx.types, resolved_for_union)
            {
                resolved_for_union = self.evaluate_type_with_env(constraint);
            }
            if state_query::union_members(self.ctx.types, resolved_for_union).is_none() {
                continue;
            }

            // Parameters with binding patterns are treated as stable for correlated
            // narrowing, matching TypeScript's alias-aware flow behavior.
            self.record_destructured_binding_group(
                param.name,
                resolved_for_union,
                true,
                name_node.kind,
            );
        }
    }

    fn destructured_param_type_may_resolve_to_union(&self, type_id: TypeId, depth: u8) -> bool {
        if crate::query_boundaries::common::union_members(self.ctx.types, type_id).is_some() {
            return true;
        }
        if depth >= 4 {
            return true;
        }

        let cached_eval = self.ctx.lookup_env_eval_cache(type_id);
        if let Some(cached) = cached_eval
            && cached.result != type_id
        {
            return self.destructured_param_type_may_resolve_to_union(cached.result, depth + 1);
        }

        if let Some(def_id) = query::lazy_def_id(self.ctx.types, type_id) {
            return self.lazy_def_may_resolve_to_union(def_id, depth + 1);
        }

        match query::classify_for_evaluation(self.ctx.types, type_id) {
            query::EvaluationNeeded::Application { .. } => {
                let Some(app) = query::type_application(self.ctx.types, type_id) else {
                    return true;
                };
                if let Some(def_id) = query::lazy_def_id(self.ctx.types, app.base) {
                    return self.lazy_def_may_resolve_to_union(def_id, depth + 1);
                }
                true
            }
            query::EvaluationNeeded::TypeParameter {
                constraint: Some(constraint),
            } => self.destructured_param_type_may_resolve_to_union(constraint, depth + 1),
            query::EvaluationNeeded::Readonly(inner) => {
                self.destructured_param_type_may_resolve_to_union(inner, depth + 1)
            }
            query::EvaluationNeeded::Intersection(members) => members.iter().any(|&member| {
                self.destructured_param_type_may_resolve_to_union(member, depth + 1)
            }),
            query::EvaluationNeeded::Union(_)
            | query::EvaluationNeeded::IndexAccess { .. }
            | query::EvaluationNeeded::KeyOf(_)
            | query::EvaluationNeeded::Mapped { .. }
            | query::EvaluationNeeded::Conditional { .. }
            | query::EvaluationNeeded::TypeQuery(_)
            | query::EvaluationNeeded::SymbolRef(_) => true,
            query::EvaluationNeeded::TypeParameter { constraint: None }
            | query::EvaluationNeeded::Resolved(_)
            | query::EvaluationNeeded::Callable(_)
            | query::EvaluationNeeded::Function(_) => false,
        }
    }

    fn lazy_def_may_resolve_to_union(&self, def_id: tsz_solver::DefId, depth: u8) -> bool {
        match self.ctx.definition_store.get_kind(def_id) {
            Some(tsz_solver::def::DefKind::TypeAlias) => self
                .ctx
                .definition_store
                .get_body(def_id)
                .is_none_or(|body| self.destructured_param_type_may_resolve_to_union(body, depth)),
            Some(
                tsz_solver::def::DefKind::Interface
                | tsz_solver::def::DefKind::Class
                | tsz_solver::def::DefKind::ClassConstructor
                | tsz_solver::def::DefKind::Enum
                | tsz_solver::def::DefKind::Namespace
                | tsz_solver::def::DefKind::Function
                | tsz_solver::def::DefKind::Variable,
            ) => false,
            None => true,
        }
    }

    pub(crate) fn record_contextual_tuple_parameter_groups(
        &mut self,
        params: &[NodeIndex],
        contextual_type: Option<TypeId>,
    ) {
        use crate::context::DestructuredBindingInfo;
        use crate::query_boundaries::state::checking as state_query;

        let Some(expected) = contextual_type else {
            return;
        };
        let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        ) else {
            return;
        };
        let Some(rest_param) = shape.params.last().filter(|param| param.rest) else {
            return;
        };

        let mut source_type = self.evaluate_type_with_env(rest_param.type_id);
        if state_query::union_members(self.ctx.types, source_type).is_none()
            && let Some(constraint) =
                state_query::type_parameter_constraint(self.ctx.types, source_type)
        {
            source_type = self.evaluate_type_with_env(constraint);
        }

        let has_tuple_shape =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_type).is_some()
                || state_query::union_members(self.ctx.types, source_type).is_some_and(|members| {
                    members.iter().all(|&member| {
                        crate::query_boundaries::common::tuple_elements(self.ctx.types, member)
                            .is_some()
                    })
                });
        if !has_tuple_shape {
            return;
        }

        let group_id = self.ctx.next_binding_group_id;
        self.ctx.next_binding_group_id += 1;

        for (index, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                continue;
            }

            for sym_id in self
                .parameter_symbol_ids(param_idx, param.name)
                .into_iter()
                .flatten()
            {
                self.ctx.destructured_bindings.insert(
                    sym_id,
                    DestructuredBindingInfo {
                        source_type,
                        property_name: String::new(),
                        element_index: index as u32,
                        group_id,
                        is_const: true,
                        is_rest: false,
                    },
                );
            }
        }
    }

    // ============================================================================
    // Section 53: Type and Symbol Utilities
    // ============================================================================

    /// Check if an expression produces a "fresh" literal type that should be widened.
    ///
    /// In TypeScript, literal types created from literal expressions are "fresh" and get
    /// widened when assigned to mutable bindings (let/var). Literal types from other
    /// sources (variable references, type annotations, narrowing) are "non-fresh" and
    /// should NOT be widened.
    ///
    /// An identifier referring to an unannotated `const` declaration whose initializer
    /// is itself a fresh literal expression is also treated as fresh: tsc tracks
    /// such bindings as widening literal types and widens them when copied into a
    /// mutable binding.
    ///
    /// ## Examples:
    /// ```typescript
    /// let x = "foo";          // "foo" is fresh → widened to string
    /// let a: "foo" = "foo";
    /// let y = a;              // a's type is non-fresh → y: "foo" (not widened)
    /// let z = a || "bar";     // result from || is non-fresh → z: "foo" (not widened)
    ///
    /// const tag = "start";    // unannotated const literal → widening literal type
    /// let m = tag;            // tag is fresh-by-reference → widened to string
    /// ```
    pub(crate) fn is_fresh_literal_expression(&self, idx: NodeIndex) -> bool {
        self.is_fresh_literal_expression_inner(idx, 0)
    }

    fn is_fresh_literal_expression_inner(&self, idx: NodeIndex, depth: u8) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Cycle / runaway-recursion guard. Identifier-following can in principle
        // reach itself through pathological forward references like
        // `const a = a;`. Bound the chain so the structural recursion always
        // terminates.
        const MAX_FRESH_LITERAL_DEPTH: u8 = 16;
        if depth > MAX_FRESH_LITERAL_DEPTH {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        let kind = node.kind;

        // Direct literal tokens are always fresh
        if kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return true;
        }

        // Parenthesized expressions inherit freshness from inner expression
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.is_fresh_literal_expression_inner(paren.expression, depth + 1);
        }

        // A plain assignment expression (`x = y`) evaluates to its RHS value,
        // so its freshness (and the widening that follows from it) is the
        // RHS's, not the assignment node's own — `check_assignment_expression`
        // already returns `right_type` for the same reason. Without this,
        // `var b = a = [undefined, null]` never widens `b`'s inferred tuple at
        // all, since the initializer node itself (the assignment) never
        // reaches the direct object/array-literal or parenthesized-unwrap
        // cases below. Compound assignments (`+=` and friends) are excluded:
        // their value type is not simply the RHS's.
        if kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.is_fresh_literal_expression_inner(binary.right, depth + 1);
        }

        // Prefix unary (+/-) on numeric/bigint literals are fresh
        if kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(prefix) = self.ctx.arena.get_unary_expr(node)
        {
            let op = prefix.operator;
            if op == SyntaxKind::PlusToken as u16 || op == SyntaxKind::MinusToken as u16 {
                return self.is_fresh_literal_expression_inner(prefix.operand, depth + 1);
            }
        }

        // Conditional expressions: fresh if either branch produces a fresh type.
        // E.g., `cond ? true : undefined` has a fresh `true` branch, so the
        // result type `true | undefined` should be widened to `boolean | undefined`.
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            return self.is_fresh_literal_expression_inner(cond.when_true, depth + 1)
                || self.is_fresh_literal_expression_inner(cond.when_false, depth + 1);
        }

        // Object and array literals need widening (property types get widened)
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        {
            return true;
        }

        // Template expressions (with substitutions) produce string, which doesn't need widening
        // but we mark them fresh for consistency
        if kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            return true;
        }

        // Identifier referencing an unannotated `const` declaration whose
        // initializer is itself a fresh literal expression. tsc tracks these
        // bindings as widening literal types, so copying them into a `let`/`var`
        // binding must still widen.
        if kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(decl_idx) = symbol.primary_declaration()
            && self.ctx.arena.is_const_variable_declaration(decl_idx)
            && let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_none()
            && var_decl.initializer.is_some()
        {
            return self.is_fresh_literal_expression_inner(var_decl.initializer, depth + 1);
        }

        // A direct enum-member access `E.A` (or namespace-qualified `NS.E.A`)
        // mints a *fresh* enum literal that widens to the parent enum `E` at a
        // mutable binding, exactly as a primitive literal token widens to its
        // base. tsc gives enum literal types the same fresh/regular duality as
        // string/number literals. A property *read* of an enum-member-typed
        // value (`o.p` where `o: { p: E.A }`) is non-fresh: it keeps `E.A`.
        // Distinguish the two by resolving the access to its symbol — a genuine
        // enum-member access resolves to an `ENUM_MEMBER` symbol, a property
        // read does not.
        //
        // The symbol resolution walks the entity-name chain, so it is gated
        // behind the node's cached type: an access whose already-computed type is
        // not an enum-member type can never be a fresh enum-member access, which
        // fast-rejects the common `obj.prop` initializer before the walk. When
        // the type is not yet cached, fall through to the resolve so freshness is
        // never under-reported.
        if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && self
                .ctx
                .node_types
                .get(&idx.0)
                .copied()
                .is_none_or(|t| enum_query::is_enum_member_for_widening(&self.ctx, t))
        {
            if self
                .resolve_qualified_symbol(idx)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|sym| sym.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER))
            {
                return true;
            }
            // The entity-name walk cannot follow a *value* receiver
            // (`m.Color.Blue` where `m: typeof NS`). The access is still a
            // genuine enum-member read — and therefore fresh — when the
            // receiver's checked type is the parent enum's namespace-object
            // surface (`typeof Color`). A property read off a plain object
            // that merely *declares* an enum-member type (`o.p` with
            // `o: { p: E.A }`) fails this receiver probe and stays non-fresh.
            if let Some(member_type) = self.ctx.node_types.get(&idx.0).copied()
                && let Some(access) = self.ctx.arena.get_access_expr_at(idx)
                && let Some(receiver_type) = self.ctx.node_types.get(&access.expression.0).copied()
                && let Some(parent_sym) =
                    enum_query::enum_member_parent_symbol_for_widening(&self.ctx, member_type)
            {
                return self.ctx.enum_namespace_types.get(&parent_sym).copied()
                    == Some(receiver_type);
            }
        }

        // Everything else (identifiers, call expressions, binary expressions, etc.)
        // produces non-fresh types that should NOT be widened
        false
    }

    /// Map an expanded argument index back to the original argument node index.
    ///
    /// This handles spread arguments that expand to multiple elements.
    /// When a spread argument has a tuple type, it expands to multiple positional
    /// arguments. This function maps from the expanded index back to the original
    /// argument node for error reporting purposes.
    ///
    /// ## Parameters:
    /// - `args`: Slice of argument node indices
    /// - `expanded_index`: Index in the expanded argument list
    ///
    /// ## Returns:
    /// - `Some(NodeIndex)`: The original argument node index
    /// - `None`: If the index doesn't map to a valid argument
    ///
    /// ## Examples:
    /// ```typescript
    /// function foo(a: string, b: number, c: boolean) {}
    /// const tuple = ["hello", 42, true] as const;
    /// // Spread expands to 3 arguments: foo(...tuple)
    /// // expanded_index 0, 1, 2 all map to the spread argument node
    /// ```
    pub(crate) fn map_expanded_arg_index_to_original(
        &self,
        args: &[NodeIndex],
        expanded_index: usize,
    ) -> Option<NodeIndex> {
        let mut current_expanded_index = 0;

        for &arg_idx in args {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                // Check if this is a spread element
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
                {
                    // Try to get the cached type, fall back to looking up directly
                    let spread_type = self
                        .ctx
                        .node_types
                        .get(&spread_data.expression.0)
                        .copied()
                        .unwrap_or(TypeId::ANY);
                    let spread_type = self.resolve_type_for_property_access_simple(spread_type);

                    // If it's a tuple type, it expands to multiple elements
                    if let Some(elems_id) = query::tuple_list_id(self.ctx.types, spread_type) {
                        let elems = self.ctx.types.tuple_list(elems_id);
                        let end_index = current_expanded_index + elems.len();
                        if expanded_index >= current_expanded_index && expanded_index < end_index {
                            // The error is within this spread - report at the spread node
                            return Some(arg_idx);
                        }
                        current_expanded_index = end_index;
                        continue;
                    }
                }
            }

            // Non-spread or non-tuple spread: takes one slot
            if expanded_index == current_expanded_index {
                return Some(arg_idx);
            }
            current_expanded_index += 1;
        }

        None
    }

    /// Simple type resolution for property access - doesn't trigger new type computation.
    ///
    /// This function resolves type applications to their base type without
    /// triggering expensive type computation. It's used in contexts where we
    /// just need the base type for inspection, not full type resolution.
    ///
    /// ## Examples:
    /// ```typescript
    /// type Box<T> = { value: T };
    /// // Box<string> resolves to Box for property access inspection
    /// ```
    fn resolve_type_for_property_access_simple(&self, type_id: TypeId) -> TypeId {
        query::application_base(self.ctx.types, type_id).unwrap_or(type_id)
    }

    pub(crate) fn lookup_symbol_with_name(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> Option<(&tsz_binder::Symbol, &tsz_parser::parser::node::NodeArena)> {
        let name_hint = name_hint.map(str::trim).filter(|name| !name.is_empty());

        if let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
            && name_hint.is_none_or(|name| symbol.escaped_name == name)
        {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        if let Some(name) = name_hint {
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id)
                    && symbol.escaped_name == name
                {
                    return Some((symbol, lib_ctx.arena.as_ref()));
                }
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == name
            {
                let arena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());
                return Some((symbol, arena));
            }
            return None;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id) {
                return Some((symbol, lib_ctx.arena.as_ref()));
            }
        }

        None
    }

    /// Check if a symbol is value-only (has value but not type).
    ///
    /// This function distinguishes between symbols that can only be used as values
    /// vs. symbols that can be used as types. This is important for:
    /// - Import/export checking
    /// - Type position validation
    /// - Value expression validation
    ///
    /// ## Examples:
    /// ```typescript
    /// // Value-only symbols:
    /// const x = 42;  // x is value-only
    ///
    /// // Not value-only:
    /// type T = string;  // T is type-only
    /// interface Box {}  // Box is both type and value
    /// class Foo {}  // Foo is both type and value
    /// ```
    pub(crate) fn symbol_is_value_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        let (symbol, arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        // Fast path using symbol flags: if symbol has TYPE flag, it's not value-only
        // This handles classes, interfaces, enums, type aliases, etc.
        // TYPE flag includes: CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS
        let has_type_flag = symbol.has_any_flags(symbol_flags::TYPE);
        if has_type_flag {
            return false;
        }

        // Modules/namespaces can be used as types in some contexts, but not if they're
        // merged with functions or other values (e.g., function+namespace declaration merging)
        // In such cases, the function/value takes precedence and TS2749 should be emitted
        let has_module = symbol.has_any_flags(symbol_flags::MODULE);
        let has_function = symbol.has_any_flags(symbol_flags::FUNCTION);
        // Exclude both FUNCTION and MODULE flags when checking for "other" value flags.
        // VALUE_MODULE is part of VALUE, but a symbol that only has module flags
        // (VALUE_MODULE | NAMESPACE_MODULE) should be treated as a pure namespace.
        let has_other_value = symbol
            .has_any_flags(symbol_flags::VALUE & !symbol_flags::FUNCTION & !symbol_flags::MODULE);

        // Pure namespace (MODULE only, no function/value flags) is not value-only
        if has_module && !has_function && !has_other_value {
            return false;
        }

        // Check declarations as a secondary source of truth (for cases where flags might not be set correctly)
        if self.symbol_has_type_declaration(symbol, arena) {
            return false;
        }

        // If the symbol is type-only (from `import type`), it's not value-only
        // In type positions, type-only imports should be allowed
        if symbol.is_type_only {
            return false;
        }

        // Finally, check if this is purely a value symbol (has VALUE but not TYPE)
        let has_value = symbol.has_any_flags(symbol_flags::VALUE);
        let has_type = symbol.has_any_flags(symbol_flags::TYPE);
        has_value && !has_type
    }

    /// Check if an alias resolves to a value-only symbol.
    ///
    /// This function follows alias chains to determine if the ultimate target
    /// is a value-only symbol. This is used for validating import/export aliases
    /// and type position checks.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Original declarations
    /// const x = 42;
    /// type T = string;
    ///
    /// // Aliases
    /// import { x as xAlias } from "./mod";  // xAlias resolves to value-only
    /// import { type T as TAlias } from "./mod";  // TAlias is type-only
    /// ```
    pub(crate) fn alias_resolves_to_value_only(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> bool {
        let (symbol, _arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }

        // If the alias symbol itself is type-only, it doesn't resolve to value-only
        if symbol.is_type_only {
            return false;
        }

        let mut visited = AliasCycleTracker::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        // symbol_is_value_only already checks TYPE flags and declarations
        // No need for redundant declaration check here
        let target_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());
        self.symbol_is_value_only(target, Some(target_name))
    }

    fn symbol_has_type_declaration(
        &self,
        symbol: &tsz_binder::Symbol,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        for &decl in &symbol.declarations {
            if decl.is_none() {
                continue;
            }
            let Some(node) = arena.get(decl) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => return true,
                k if k == syntax_kind_ext::CLASS_DECLARATION => return true,
                k if k == syntax_kind_ext::ENUM_DECLARATION => return true,
                _ => {}
            }
        }

        false
    }

    // ============================================================================
    // Section 54: Literal Key and Element Access Utilities
    // ============================================================================

    /// Extract literal keys from a type as string and number atom vectors.
    ///
    /// This function is used for element access type inference when the index
    /// type contains literal types. It extracts string and number literal values
    /// from single literals or unions of literals.
    ///
    /// ## Parameters:
    /// - `index_type`: The type to extract literal keys from
    ///
    /// ## Returns:
    /// - `Some((string_keys, number_keys))`: Tuple of string and number literal keys
    /// - `None`: If the type is not a literal or union of literals
    ///
    /// ## Examples:
    /// ```typescript
    /// // Single literal:
    /// type T1 = "foo";  // Returns: (["foo"], [])
    ///
    /// // Union of literals:
    /// type T2 = "a" | "b" | 1 | 2;  // Returns: (["a", "b"], [1.0, 2.0])
    ///
    /// // Non-literal type:
    /// type T3 = string;  // Returns: None
    /// ```
    pub(crate) fn get_literal_key_union_from_type(
        &self,
        index_type: TypeId,
    ) -> Option<(Vec<tsz_common::interner::Atom>, Vec<f64>)> {
        match query::literal_key_kind(self.ctx.types, index_type) {
            query::LiteralKeyKind::StringLiteral(atom) => Some((vec![atom], Vec::new())),
            query::LiteralKeyKind::NumberLiteral(num) => Some((Vec::new(), vec![num])),
            query::LiteralKeyKind::Union(members) => {
                let mut string_keys = Vec::with_capacity(members.len());
                let mut number_keys = Vec::new();
                for &member in &members {
                    match query::literal_key_kind(self.ctx.types, member) {
                        query::LiteralKeyKind::StringLiteral(atom) => string_keys.push(atom),
                        query::LiteralKeyKind::NumberLiteral(num) => number_keys.push(num),
                        _ => return None,
                    }
                }
                Some((string_keys, number_keys))
            }
            query::LiteralKeyKind::Other => {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .and_then(|constraint| {
                    (constraint != index_type)
                        .then(|| self.get_literal_key_union_from_type(constraint))
                        .flatten()
                })
            }
        }
    }

    /// Get element access type for literal string keys.
    ///
    /// This function computes the type of element access when the index is a
    /// string literal or union of string literals. It handles both property
    /// access and numeric array indexing (when strings represent numeric indices).
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of string literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all property/element types
    /// - `None`: If any property is not found or if keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: "hello" };
    /// type T = obj["a" | "b"];  // number | string
    ///
    /// const arr = [1, 2, 3];
    /// type U = arr["0" | "1"];  // number (treated as numeric index)
    /// ```
    pub(crate) fn get_element_access_type_for_literal_keys(
        &mut self,
        object_type: TypeId,
        keys: &[tsz_common::interner::Atom],
        is_write_context: bool,
    ) -> LiteralKeysResult {
        use crate::query_boundaries::common::PropertyAccessResult;

        if keys.is_empty() {
            return LiteralKeysResult {
                result_type: None,
                missing_keys: Vec::new(),
            };
        }

        // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
        let resolved_type = self.resolve_type_for_property_access(object_type);
        if resolved_type == TypeId::ANY {
            return LiteralKeysResult {
                result_type: Some(TypeId::ANY),
                missing_keys: Vec::new(),
            };
        }
        if resolved_type == TypeId::ERROR {
            return LiteralKeysResult {
                result_type: None,
                missing_keys: Vec::new(),
            };
        }

        let numeric_as_index = self.is_array_like_type(resolved_type);
        let mut types = Vec::with_capacity(keys.len());
        let mut missing_keys = Vec::new();

        for &key in keys {
            let name = self.ctx.types.resolve_atom(key);
            if numeric_as_index && let Some(index) = self.get_numeric_index_from_string(&name) {
                let element_type =
                    self.get_element_access_type(resolved_type, TypeId::NUMBER, Some(index));
                types.push(element_type);
                continue;
            }

            match self.ctx.types.property_access_type(resolved_type, &name) {
                PropertyAccessResult::Success {
                    type_id,
                    write_type,
                    ..
                } => {
                    // In write context (assignment target), use the write/setter type.
                    let effective = if is_write_context {
                        write_type.unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    types.push(effective);
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    types.push(property_type.unwrap_or(TypeId::UNKNOWN));
                }
                // IsUnknown: Return immediately — the caller has node context and
                // will report TS2571 error.
                PropertyAccessResult::IsUnknown => {
                    return LiteralKeysResult {
                        result_type: None,
                        missing_keys: Vec::new(),
                    };
                }
                // PropertyNotFound: Track the missing key instead of bailing out.
                // tsc emits TS2339 per missing key, not TS7053 for the whole union.
                PropertyAccessResult::PropertyNotFound { .. } => {
                    missing_keys.push(name.to_string());
                }
            }
        }

        // In write context, the value must be assignable to ALL possible property types
        // (intersection), since we don't know which key will be used at runtime.
        // In read context, the result is ANY of the property types (union).
        let result_type = if types.is_empty() {
            None
        } else if is_write_context {
            let intersection = tsz_solver::utils::intersection_or_single(self.ctx.types, types);
            Some(self.evaluate_type_with_env(intersection))
        } else {
            Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
        };

        LiteralKeysResult {
            result_type,
            missing_keys,
        }
    }

    /// Get element access type for literal number keys.
    ///
    /// This function computes the type of element access when the index is a
    /// number literal or union of number literals. It handles array/tuple
    /// indexing with literal numeric values.
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of numeric literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all element types
    /// - `None`: If keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const arr = [1, "hello", true];
    /// type T = arr[0 | 1];  // number | string
    ///
    /// const tuple = [1, 2] as const;
    /// type U = tuple[0 | 1];  // 1 | 2
    /// ```
    pub(crate) fn get_element_access_type_for_literal_number_keys(
        &mut self,
        object_type: TypeId,
        keys: &[f64],
        is_write_context: bool,
    ) -> Option<TypeId> {
        if keys.is_empty() {
            return None;
        }

        let mut types = Vec::with_capacity(keys.len());
        for &value in keys {
            if let Some(index) = self.get_numeric_index_from_number(value) {
                let ty = self.get_element_access_type(object_type, TypeId::NUMBER, Some(index));
                if (ty == TypeId::ERROR || ty == TypeId::UNDEFINED)
                    && !self.is_array_like_type(object_type)
                {
                    let index_signature_ty =
                        self.get_element_access_type(object_type, TypeId::NUMBER, None);
                    if index_signature_ty != TypeId::ERROR
                        && index_signature_ty != TypeId::UNDEFINED
                    {
                        types.push(index_signature_ty);
                        continue;
                    }
                    return None;
                }
                types.push(ty);
            } else {
                let ty = self.get_element_access_type(object_type, TypeId::NUMBER, None);
                if (ty == TypeId::ERROR || ty == TypeId::UNDEFINED)
                    && !self.is_array_like_type(object_type)
                {
                    return None;
                }
                return Some(ty);
            }
        }

        // In write context, intersect (value must satisfy all possible indices).
        if is_write_context {
            let intersection = tsz_solver::utils::intersection_or_single(self.ctx.types, types);
            Some(self.evaluate_type_with_env(intersection))
        } else {
            Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
        }
    }

    /// Check if a type is array-like (supports numeric indexing).
    ///
    /// This function determines if a type supports numeric element access,
    /// including arrays, tuples, and unions/intersections of array-like types.
    ///
    /// ## Array-like Types:
    /// - Array types: `T[]`, `Array<T>`
    /// - Tuple types: `[T1, T2, ...]`
    /// - Readonly arrays: `readonly T[]`, `ReadonlyArray<T>`
    /// - Unions where all members are array-like
    /// - Intersections where any member is array-like
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array-like types:
    /// type A = number[];
    /// type B = [string, number];
    /// type C = readonly boolean[];
    /// type D = A | B;  // Union of array-like types
    ///
    /// // Not array-like:
    /// type E = { [key: string]: number };  // Index signature, not array-like
    /// ```
    pub(crate) fn is_array_like_type(&self, object_type: TypeId) -> bool {
        let object_type = self.ctx.types.evaluate_type(object_type);
        // Check for array/tuple types directly
        if crate::query_boundaries::checkers::iterable::is_array_type(self.ctx.types, object_type) {
            return true;
        }

        match query::classify_array_like(self.ctx.types, object_type) {
            query::ArrayLikeKind::Array(_) | query::ArrayLikeKind::Tuple => true,
            query::ArrayLikeKind::Readonly(inner) => self.is_array_like_type(inner),
            query::ArrayLikeKind::Union(members) => members
                .iter()
                .all(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Other => self.type_has_array_like_heritage(object_type),
        }
    }

    fn type_has_array_like_heritage(&self, type_id: TypeId) -> bool {
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            // Delegate to solver query for object symbol extraction
            crate::query_boundaries::common::object_symbol(self.ctx.types, type_id)
        });
        let Some(sym_id) = sym_id else {
            return false;
        };
        let mut walk_state = HeritageSymbolWalkState::new();
        self.symbol_has_array_like_heritage(sym_id, &mut walk_state)
    }

    fn symbol_has_array_like_heritage(
        &self,
        sym_id: SymbolId,
        walk_state: &mut HeritageSymbolWalkState,
    ) -> bool {
        if !walk_state.enter_path(sym_id) {
            return false;
        }

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            walk_state.leave_path(sym_id);
            return false;
        };

        if Self::is_builtin_array_like_name(symbol.escaped_name.as_str()) {
            walk_state.leave_path(sym_id);
            return true;
        }

        let mut decls = symbol.declarations.clone();
        let value_decl = symbol.value_declaration;
        if value_decl != NodeIndex::NONE && !decls.contains(&value_decl) {
            decls.push(value_decl);
        }

        for decl_idx in decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let heritage_clauses = if let Some(interface) = self.ctx.arena.get_interface(node) {
                interface.heritage_clauses.as_ref()
            } else if let Some(class_decl) = self.ctx.arena.get_class(node) {
                class_decl.heritage_clauses.as_ref()
            } else {
                None
            };

            let Some(heritage_clauses) = heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        self.ctx
                            .arena
                            .get_type_ref(type_node)
                            .map(|type_ref| type_ref.type_name)
                            .unwrap_or(type_idx)
                    } else {
                        type_idx
                    };

                    if let Some(base_name) = self.heritage_name_text(expr_idx)
                        && Self::is_builtin_array_like_name(base_name.as_str())
                    {
                        walk_state.leave_path(sym_id);
                        return true;
                    }

                    if let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx)
                        && self.symbol_has_array_like_heritage(base_sym_id, walk_state)
                    {
                        walk_state.leave_path(sym_id);
                        return true;
                    }
                }
            }
        }

        walk_state.leave_path(sym_id);
        false
    }

    fn is_builtin_array_like_name(name: &str) -> bool {
        matches!(
            name.rsplit('.').next().unwrap_or(name),
            "Array" | "ReadonlyArray" | "ConcatArray"
        )
    }

    /// Check if an index signature error should be reported for element access.
    ///
    /// This function determines whether a "No index signature" error should be
    /// emitted for element access on an object type. This happens when:
    /// - The object type doesn't have an appropriate index signature
    /// - The index type is a literal or union of literals
    /// - The access is not valid property access
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `index_type`: The type of the index expression
    /// - `literal_index`: Optional explicit numeric index
    ///
    /// ## Returns:
    /// - `true`: Report "No index signature" error
    /// - `false`: Don't report (has index signature, or any/unknown type)
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: 2 };
    /// obj["c"];  // Error: No index signature with parameter of type '"c"'
    ///
    /// const obj2: { [key: string]: number } = { a: 1 };
    /// obj2["c"];  // OK: Has string index signature
    /// ```
    pub(crate) fn should_report_no_index_signature(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        if object_type == TypeId::ANY
            || object_type == TypeId::ERROR
            || (self.ctx.compiler_options.strict_null_checks && object_type == TypeId::UNKNOWN)
        {
            return false;
        }

        // `unknown` index type can't trigger TS7053 — it's not usable as an index.
        if index_type == TypeId::UNKNOWN {
            return false;
        }

        // For a type parameter, indexability is governed by its base constraint:
        // the declared `extends` type, or `unknown` when there is none. tsc
        // reports TS7053 whenever a concrete key can't index that base
        // constraint — both for a constraint that lacks the needed index
        // signature (`T extends Item`) and for an unconstrained `T`, whose base
        // constraint `unknown` has no index signatures at all.
        let is_type_param =
            crate::query_boundaries::common::is_type_parameter(self.ctx.types, object_type);
        let check_type = if is_type_param {
            if crate::query_boundaries::common::is_type_parameter(self.ctx.types, index_type) {
                return false;
            }
            match crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                object_type,
            ) {
                // A constraint that still mentions type parameters (e.g.
                // `Record<K, number>`) can't be resolved until instantiation.
                Some(constraint)
                    if crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        constraint,
                    ) =>
                {
                    return false;
                }
                // Keep the raw constraint; it is resolved to its apparent type
                // below (intersections are handled member-by-member there).
                Some(constraint) => constraint,
                // Unconstrained: the base constraint is `unknown`, which has
                // no index signatures, so a concrete key can't index it.
                None => TypeId::UNKNOWN,
            }
        } else {
            object_type
        };

        if check_type == TypeId::ANY || check_type == TypeId::ERROR {
            return false;
        }

        // `any` index type: tsc reports TS7053 when noImplicitAny is on and the
        // object lacks an index signature. Treat `any` as wanting both string and
        // number indexing — if the object supports neither, a diagnostic should fire.
        let (wants_string, wants_number) = if index_type == TypeId::ANY {
            (true, true)
        } else {
            let index_key_kind = self.get_index_key_kind(index_type);
            let wants_number = literal_index.is_some()
                || index_key_kind
                    .as_ref()
                    .is_some_and(|(_, wants_number)| *wants_number);
            let wants_string = index_key_kind
                .as_ref()
                .is_some_and(|(wants_string, _)| *wants_string);
            (wants_string, wants_number)
        };
        if !wants_number && !wants_string {
            return false;
        }

        // A type parameter is indexed against the *apparent type* of its
        // constraint. The constraint may be an unevaluated application/alias/
        // mapped type whose head is a `Lazy(DefId)` (e.g. `Record<string, V>`,
        // `Partial<Record<…>>`, a user mapped alias). The downstream
        // index-signature queries evaluate with a resolver-less pass that cannot
        // expand a `Lazy(DefId)`, so resolve through the checker environment here —
        // matching tsc's `getApparentType` of the constraint.
        if is_type_param {
            // An intersection constraint is indexable when *any* member supplies a
            // usable index signature (tsc's apparent-type lookup over the
            // constituents). Resolve each member separately: merging the whole
            // intersection drops member index signatures, which would re-introduce
            // the false positive.
            if let Some(members) = query::get_intersection_members(self.ctx.types, check_type) {
                return members.iter().all(|&member| {
                    self.constraint_member_reports_no_index_signature(
                        member,
                        index_type,
                        wants_string,
                        wants_number,
                    )
                });
            }
            return self.constraint_member_reports_no_index_signature(
                check_type,
                index_type,
                wants_string,
                wants_number,
            );
        }

        self.object_reports_no_index_signature(check_type, index_type, wants_string, wants_number)
    }

    /// Resolve a single type-parameter-constraint member to its apparent type
    /// through the checker environment, then decide whether it lacks the needed
    /// index signature. A member that resolves to `any`/error is treated as
    /// indexable (no report), matching tsc.
    fn constraint_member_reports_no_index_signature(
        &self,
        member: TypeId,
        index_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        let resolved =
            crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                self.ctx.types,
                &self.ctx,
                member,
            );
        if resolved == TypeId::ANY || resolved == TypeId::ERROR {
            return false;
        }
        self.object_reports_no_index_signature(resolved, index_type, wants_string, wants_number)
    }

    /// Decide whether a concrete (non-type-parameter, non-intersection) object
    /// type lacks the index signature needed for the requested key kind. Returns
    /// `true` when tsc would report TS7053 for this receiver.
    fn object_reports_no_index_signature(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        let unwrapped_type = query::unwrap_readonly_for_lookup(self.ctx.types, object_type);

        // An intersection's applicable index infos are the union of every
        // member's infos (tsc collects them across constituents): a read is
        // accepted when ANY member's signature applies. The merged surface
        // from `get_index_info` deliberately keeps only same-key composites
        // (distinct template-pattern keys stay per-member), so consulting it
        // here would test the key against one member's pattern and wrongly
        // report TS7053 for `` {[x:`foo-${string}`]: ..} & {[x:`${string}-bar`]: ..} ``
        // reads that match the other member.
        if let Some(members) = query::get_intersection_members(self.ctx.types, unwrapped_type) {
            return members.iter().all(|&member| {
                self.object_reports_no_index_signature(
                    member,
                    index_type,
                    wants_string,
                    wants_number,
                )
            });
        }

        if wants_string
            && let Some(string_index) =
                crate::query_boundaries::common::IndexSignatureResolver::new(self.ctx.types)
                    .get_index_info(unwrapped_type)
                    .string_index
                    .as_ref()
            && !crate::query_boundaries::index_signature::index_key_type_satisfies_index_signature(
                self.ctx.types,
                index_type,
                string_index.key_type,
            )
        {
            return true;
        }

        !self.is_element_indexable(unwrapped_type, wants_string, wants_number)
    }

    /// Determine what kind of index key a type represents.
    ///
    /// This function analyzes a type to determine if it can be used for string
    /// or numeric indexing. Returns a tuple of (`wants_string`, `wants_number`).
    ///
    /// ## Returns:
    /// - `Some((true, false))`: String index (e.g., `"foo"`, `string`)
    /// - `Some((false, true))`: Number index (e.g., `42`, `number`)
    /// - `Some((true, true))`: Both string and number (e.g., `"a" | 1 | 2`)
    /// - `None`: Not an index type
    ///
    /// ## Examples:
    /// ```typescript
    /// type A = "foo";        // (true, false) - string literal
    /// type B = 42;           // (false, true) - number literal
    /// type C = string;       // (true, false) - string type
    /// type D = "a" | "b";    // (true, false) - union of strings
    /// type E = "a" | 1;      // (true, true) - mixed literals
    /// ```
    pub(crate) fn get_index_key_kind(&self, index_type: TypeId) -> Option<(bool, bool)> {
        if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            return Some((false, true));
        }

        match query::classify_index_key(self.ctx.types, index_type) {
            query::IndexKeyKind::String
            | query::IndexKeyKind::StringLiteral
            | query::IndexKeyKind::TemplateLiteralString => Some((true, false)),
            query::IndexKeyKind::Number | query::IndexKeyKind::NumberLiteral => Some((false, true)),
            // `${number}` is a numeric string type — valid for both string and number
            // index signatures. Arrays have number index signatures, and objects may
            // have string index signatures, so this type can index both.
            query::IndexKeyKind::NumericStringLike => Some((true, true)),
            query::IndexKeyKind::Union(members) => {
                let mut wants_string = false;
                let mut wants_number = false;
                for member in members {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            query::IndexKeyKind::Other => {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .and_then(|constraint| {
                    (constraint != index_type).then(|| self.get_index_key_kind(constraint))
                })
                .flatten()
            }
        }
    }
}
