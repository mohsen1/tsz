//! Definite assignment analysis (TS2454), TDZ analysis, and flow-based type narrowing.

use crate::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::{
    are_types_mutually_subtype_with_env, tuple_elements_for_type, union_members_for_type,
};
use crate::query_boundaries::state::checking::find_property_in_object_by_str;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;
use std::rc::Rc;
use tsz_binder::{SymbolId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Definite Assignment Analysis
    // =========================================================================

    /// Apply control flow narrowing to a type at a specific identifier usage.
    ///
    /// This walks backwards through the control flow graph to determine what
    /// type guards (typeof, null checks, etc.) have been applied.
    ///
    /// ## Rule #42: CFA Invalidation in Closures
    ///
    /// When accessing a variable inside a closure (function expression or arrow function):
    /// - If the variable is `let` or `var` (mutable): Reset to declared type (ignore outer narrowing)
    /// - If the variable is `const` (immutable): Maintain narrowing (safe)
    ///
    /// This prevents unsound assumptions where a mutable variable's type is narrowed
    /// in the outer scope but the closure captures the variable and might execute
    /// after the variable has been reassigned to a different type.
    pub(crate) fn apply_flow_narrowing(&self, idx: NodeIndex, declared_type: TypeId) -> TypeId {
        self.apply_flow_narrowing_with_initial_type(idx, declared_type, None)
    }

    pub(crate) fn apply_flow_narrowing_with_initial_type(
        &self,
        idx: NodeIndex,
        declared_type: TypeId,
        initial_type_override: Option<TypeId>,
    ) -> TypeId {
        // Skip flow narrowing when getting assignment target types.
        // For assignments like `foo[x] = 1` after `if (foo[x] === undefined)`,
        // we need the declared type (e.g., `number | undefined`) not the narrowed type (`undefined`).
        if self.ctx.skip_flow_narrowing {
            return declared_type;
        }

        // Optional-chain access results (`obj?.prop`, `obj?.[k]`) already encode
        // nullish behavior during property/element access typing. Re-running flow
        // analysis at the access node is redundant and expensive on repeated chains.
        if let Some(node) = self.ctx.arena.get(idx)
            && (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && access.question_dot_token
        {
            return declared_type;
        }

        // Generic callable property reads should keep their declared read type.
        // A prior write like `obj.fn = otherFn` can be assignable while still being
        // a narrower generic signature than the property's declared type. Replaying
        // that write through access-node flow narrowing rewrites later reads to the
        // last assigned subtype, which is not how tsc treats declared callable
        // properties on interfaces/classes.
        if let Some(node) = self.ctx.arena.get(idx)
            && (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        {
            let function_shape = crate::query_boundaries::flow_analysis::function_shape_for_type(
                self.ctx.types,
                declared_type,
            );
            let has_generic_call_signatures =
                crate::query_boundaries::flow_analysis::call_signatures_for_type(
                    self.ctx.types,
                    declared_type,
                )
                .is_some_and(|sigs| sigs.iter().any(|sig| !sig.type_params.is_empty()));
            let construct_signatures =
                crate::query_boundaries::flow_analysis::construct_signatures_for_type(
                    self.ctx.types,
                    declared_type,
                );
            let has_any_construct_signatures = construct_signatures
                .as_ref()
                .is_some_and(|sigs| !sigs.is_empty());
            let has_generic_construct_signatures = construct_signatures
                .as_ref()
                .is_some_and(|sigs| sigs.iter().any(|sig| !sig.type_params.is_empty()));
            let is_generic_function_shape = function_shape
                .as_ref()
                .is_some_and(|shape| !shape.type_params.is_empty());
            let is_constructor_function_shape = function_shape
                .as_ref()
                .is_some_and(|shape| shape.is_constructor);

            if is_generic_function_shape
                || has_generic_call_signatures
                || has_generic_construct_signatures
                || is_constructor_function_shape
                || has_any_construct_signatures
            {
                return declared_type;
            }
        }

        // Get the flow node for this expression usage FIRST
        // If there's no flow info, no narrowing is possible regardless of node type
        let flow_node = if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            flow
        } else {
            // Some nodes in type positions (e.g. `typeof x` inside a type alias)
            // don't carry direct flow links. Fall back to the nearest parent that
            // has flow information so narrowing can still apply at that site.
            let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
            let mut found = None;
            while let Some(parent) = current {
                if parent.is_none() {
                    break;
                }
                if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                    found = Some(flow);
                    break;
                }
                current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
            }
            match found {
                Some(flow) => flow,
                None => return declared_type,
            }
        };

        // Fast path: `any` and `error` types cannot be meaningfully narrowed.
        // NOTE: We only skip for direct `any`/`error`, NOT for compound types that
        // contain `any` (e.g. unions of classes with `any`-returning methods).
        // TypeScript narrows such compound types normally via instanceof/typeof.
        if declared_type == TypeId::ERROR {
            return declared_type;
        }

        // Rule #42 for captured mutable variables in closures is handled at
        // the flow-graph level: check_flow() returns initial_type when it
        // reaches a START node for a captured mutable variable (core.rs).
        // We must NOT bail out here because local narrowing within the
        // closure (e.g. `typeof x === "string" && x.length`) still applies —
        // the flow walk will encounter CONDITION nodes before hitting START.

        // Skip narrowing for `never` — it's the bottom type, nothing to narrow.
        // All other types (unions, objects, callables, type params, primitives, etc.)
        // can benefit from flow narrowing (instanceof, typeof, truthiness, etc.).
        if declared_type == TypeId::NEVER {
            return declared_type;
        }

        // Note: we intentionally do NOT skip flow narrowing for primitive types.
        // User-defined type predicates can narrow primitives to literal unions
        // or branded intersections (e.g., `value is "foo" | "bar"`, `value is string & Tag`).

        // Hot-path optimization: for property/element access expressions with an already
        // concrete primitive/literal result type, flow re-analysis at the access node is
        // typically redundant. The object expression has already been flow-narrowed before
        // property lookup; re-walking flow for the access itself is high-cost in long
        // discriminant-if chains (e.g. repeated `if (e.kind === "...") return e.dataN`).
        //
        // Keep full flow narrowing for unions/objects/type-parameters, where access-level
        // narrowing may still materially change the type.
        if let Some(node) = self.ctx.arena.get(idx)
            && (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && (matches!(
                declared_type,
                TypeId::STRING
                    | TypeId::NUMBER
                    | TypeId::BOOLEAN
                    | TypeId::BIGINT
                    | TypeId::SYMBOL
                    | TypeId::UNDEFINED
                    | TypeId::NULL
                    | TypeId::VOID
            ) || tsz_solver::visitor::is_literal_type_through_type_constraints(
                self.ctx.types,
                declared_type,
            ))
        {
            return declared_type;
        }

        // Optional-chain intermediates (`obj?.a` in `obj?.a?.b`) are transient
        // receiver values used only to continue the chain. Re-running flow
        // narrowing at each intermediate segment is redundant and expensive.
        if let Some(node) = self.ctx.arena.get(idx)
            && (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && access.question_dot_token
            && let Some(ext) = self.ctx.arena.get_extended(idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && let Some(parent_access) = self.ctx.arena.get_access_expr(parent_node)
            && parent_access.question_dot_token
            && parent_access.expression == idx
        {
            return declared_type;
        }

        // Create a flow analyzer and apply narrowing
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
        .with_numeric_atom_cache(&self.ctx.flow_numeric_atom_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(Rc::clone(&self.ctx.type_environment))
        .with_narrowing_cache(&self.ctx.narrowing_cache)
        .with_call_type_predicates(&self.ctx.call_type_predicates)
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        )
        .with_symbol_last_assignment_pos(&self.ctx.symbol_last_assignment_pos)
        .with_destructured_bindings(&self.ctx.destructured_bindings);

        // Strip `undefined` from the initial type for parameters with default values.
        // Matches tsc's getInitialType: a parameter like `x: string | undefined = "val"`
        // starts as `string` (not `string | undefined`) because the default guarantees it.
        //
        // PERF: Check cheapest conditions first to short-circuit early.
        // 1. Check if override provided (instant)
        // 2. Resolve symbol and check if it has an initializer (cheap hash lookups)
        // 3. Only walk the AST tree for is_in_default_parameter when we know the
        //    symbol IS a parameter with a default — this saves ~5 parent lookups
        //    per call for the vast majority of identifiers that aren't defaulted params.
        let initial_type = if let Some(initial_type) = initial_type_override {
            initial_type
        } else if let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
            && sym.value_declaration.is_some()
            && let Some(decl_node) = self.ctx.arena.get(sym.value_declaration)
            && let Some(param) = self.ctx.arena.get_parameter(decl_node)
            && param.initializer.is_some()
            && !self.is_in_default_parameter(idx)
        {
            crate::query_boundaries::flow::narrow_destructuring_default(
                self.ctx.types,
                declared_type,
                true,
            )
        } else {
            declared_type
        };
        let narrowed = analyzer.get_flow_type(idx, initial_type, flow_node);

        // Correlated narrowing for destructured bindings.
        // When `const { data, isSuccess } = useQuery()` and we check `isSuccess`,
        // narrowing of `isSuccess` should also narrow `data`.
        if let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && let Some(info) = self.ctx.destructured_bindings.get(&sym_id).cloned()
            && info.is_const
        {
            let correlated =
                self.apply_correlated_narrowing(&analyzer, sym_id, &info, narrowed, flow_node);
            // Also apply source-based property narrowing if available.
            // Example: `if (action.payload) { const { kind, payload } = action; ... }`
            // Correlated narrows payload to `number | undefined` (from kind === 'A'),
            // source-based strips `undefined` (from action.payload truthiness).
            // The combined result intersects both narrowings.
            if let Some((source_expr, prop_name)) =
                self.ctx.destructured_binding_sources.get(&sym_id).cloned()
            {
                let narrowed_via_source = self.narrow_destructured_binding_via_source(
                    &analyzer,
                    source_expr,
                    &prop_name,
                    declared_type,
                    flow_node,
                );
                if narrowed_via_source != declared_type && narrowed_via_source != correlated {
                    // Intersect correlated and source-based narrowing results.
                    // Keep only types that appear in both narrowed sets.
                    let c_members = union_members_for_type(self.ctx.types, correlated);
                    let s_members = union_members_for_type(self.ctx.types, narrowed_via_source);
                    match (c_members, s_members) {
                        (Some(c), Some(s)) => {
                            let filtered: Vec<TypeId> =
                                c.iter().filter(|t| s.contains(t)).copied().collect();
                            if !filtered.is_empty() {
                                return tsz_solver::utils::union_or_single(
                                    self.ctx.types,
                                    filtered,
                                );
                            }
                        }
                        (Some(c), None) => {
                            let filtered: Vec<TypeId> = c
                                .iter()
                                .filter(|&&t| t == narrowed_via_source)
                                .copied()
                                .collect();
                            if !filtered.is_empty() {
                                return tsz_solver::utils::union_or_single(
                                    self.ctx.types,
                                    filtered,
                                );
                            }
                        }
                        (None, Some(s)) => {
                            if s.contains(&correlated) {
                                return correlated;
                            }
                        }
                        (None, None) => {
                            if correlated == narrowed_via_source {
                                return correlated;
                            }
                        }
                    }
                }
            }
            return correlated;
        }

        // Flow-based property narrowing for destructured bindings.
        // When `const { bar } = aFoo` and `aFoo.bar` was narrowed by a prior condition
        // (e.g., `if (aFoo.bar)`), the binding element `bar` should use the narrowed type.
        // This works by finding a property access expression `source.prop` in the flow
        // conditions and running the flow analyzer on it.
        if narrowed == initial_type
            && let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && let Some((source_expr, prop_name)) =
                self.ctx.destructured_binding_sources.get(&sym_id).cloned()
        {
            let narrowed_via_source = self.narrow_destructured_binding_via_source(
                &analyzer,
                source_expr,
                &prop_name,
                declared_type,
                flow_node,
            );
            if narrowed_via_source != declared_type {
                return narrowed_via_source;
            }
        }

        narrowed
    }

    /// Narrow a destructured binding element's type using flow conditions on the source property.
    ///
    /// When `const { bar } = aFoo` and `aFoo.bar` was narrowed by `if (aFoo.bar)`, this
    /// finds the `aFoo.bar` property access expression in the flow condition chain and
    /// runs the flow analyzer on it to get the narrowed type.
    fn narrow_destructured_binding_via_source(
        &self,
        analyzer: &FlowAnalyzer<'_>,
        source_expr: NodeIndex,
        prop_name: &str,
        declared_type: TypeId,
        flow_node: tsz_binder::FlowNodeId,
    ) -> TypeId {
        // Find a property access expression `source.prop` by walking flow condition
        // antecedents. We look for CONDITION flow nodes whose associated AST node
        // contains a property access on the source expression with the matching property name.
        let prop_access_ref =
            self.find_property_access_in_flow_conditions(source_expr, prop_name, flow_node);

        let Some(prop_access_node) = prop_access_ref else {
            return declared_type;
        };

        // Use the property access node as the reference for flow analysis.
        // The flow analyzer will walk back through conditions and find the narrowing.
        analyzer.get_flow_type(prop_access_node, declared_type, flow_node)
    }

    /// Walk the flow condition chain to find a property access expression `source.prop`
    /// that matches the given source expression and property name.
    fn find_property_access_in_flow_conditions(
        &self,
        source_expr: NodeIndex,
        prop_name: &str,
        flow_node: tsz_binder::FlowNodeId,
    ) -> Option<NodeIndex> {
        use tsz_binder::flow_flags;

        let mut current = flow_node;
        let mut visited = 0u32;

        while visited < 64 {
            visited += 1;
            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                break;
            };

            if flow.has_any_flags(
                flow_flags::CONDITION | flow_flags::TRUE_CONDITION | flow_flags::FALSE_CONDITION,
            ) {
                // Check if this condition's AST node contains a property access on source
                if let Some(prop_access) =
                    self.find_matching_property_access(flow.node, source_expr, prop_name)
                {
                    return Some(prop_access);
                }
            }

            // Follow antecedents
            if let Some(&ant) = flow.antecedent.first() {
                if ant.is_none() {
                    break;
                }
                current = ant;
            } else {
                break;
            }
        }

        None
    }

    /// Check if an AST node contains a property access expression matching `source.prop_path`.
    /// The `prop_name` can be a dotted path like `"nested.b"` for nested destructuring,
    /// which matches a chained access like `aFoo.nested.b`.
    /// Walks into binary expressions, call expressions, and unary expressions to find it.
    fn find_matching_property_access(
        &self,
        node_idx: NodeIndex,
        source_expr: NodeIndex,
        prop_name: &str,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(node_idx)?;

        // Direct property access: `aFoo.bar` or chained `aFoo.nested.b`
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            if !access.question_dot_token {
                // For dotted paths like "nested.b", we need to match a chain of property accesses.
                // The outermost access is the last segment ("b"), and we walk inward to match
                // earlier segments ("nested") until we reach the source expression.
                if self.matches_property_chain(node_idx, source_expr, prop_name) {
                    return Some(node_idx);
                }
            }
        }

        // Binary expression: `aFoo.bar && ...` or `aFoo.bar !== undefined`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
        {
            if let Some(found) =
                self.find_matching_property_access(binary.left, source_expr, prop_name)
            {
                return Some(found);
            }
            if let Some(found) =
                self.find_matching_property_access(binary.right, source_expr, prop_name)
            {
                return Some(found);
            }
        }

        // Prefix unary: `!aFoo.bar`
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.ctx.arena.get_unary_expr_ex(node)
        {
            return self.find_matching_property_access(unary.expression, source_expr, prop_name);
        }

        // Call expression: `isNonNull(aFoo.bar)`
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
            && let Some(args) = &call.arguments
        {
            for &arg in &args.nodes {
                if let Some(found) = self.find_matching_property_access(arg, source_expr, prop_name)
                {
                    return Some(found);
                }
            }
        }

        None
    }

    /// Check if a property access chain matches `source.seg1.seg2...segN`.
    ///
    /// Given `prop_path = "nested.b"` and `source_expr` pointing to `aFoo`,
    /// this matches `aFoo.nested.b` by walking the chain from the outermost access inward:
    /// 1. Outermost access property name must be "b" (last segment)
    /// 2. Its base must be a property access with name "nested" (first segment)
    /// 3. That base's expression must match `source_expr` (i.e., `aFoo`)
    fn matches_property_chain(
        &self,
        access_node: NodeIndex,
        source_expr: NodeIndex,
        prop_path: &str,
    ) -> bool {
        // Split the dotted path into segments
        let segments: Vec<&str> = prop_path.split('.').collect();
        self.matches_property_chain_segments(access_node, source_expr, &segments)
    }

    fn matches_property_chain_segments(
        &self,
        node_idx: NodeIndex,
        source_expr: NodeIndex,
        segments: &[&str],
    ) -> bool {
        if segments.is_empty() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }

        // Check that the property name matches the last segment
        let last_segment = segments[segments.len() - 1];
        let name_matches = self
            .ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text == last_segment)
            .unwrap_or(false);

        if !name_matches {
            return false;
        }

        if segments.len() == 1 {
            // Single segment: base must match source_expr
            self.is_same_reference(access.expression, source_expr)
        } else {
            // Multiple segments: recurse into the base with remaining segments
            self.matches_property_chain_segments(
                access.expression,
                source_expr,
                &segments[..segments.len() - 1],
            )
        }
    }

    /// Check if two expression nodes refer to the same variable/reference.
    fn is_same_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let (Some(node_a), Some(node_b)) = (self.ctx.arena.get(a), self.ctx.arena.get(b)) else {
            return a == b;
        };

        // Both are identifiers: compare symbols or names
        if node_a.kind == SyntaxKind::Identifier as u16
            && node_b.kind == SyntaxKind::Identifier as u16
        {
            if let Some(sym_a) = self.ctx.binder.get_node_symbol(a)
                && let Some(sym_b) = self.ctx.binder.get_node_symbol(b)
            {
                return sym_a == sym_b;
            }
            if let Some(ident_a) = self.ctx.arena.get_identifier(node_a)
                && let Some(ident_b) = self.ctx.arena.get_identifier(node_b)
            {
                return ident_a.escaped_text == ident_b.escaped_text;
            }
        }

        // Both are property accesses: compare recursively
        if node_a.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node_b.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access_a) = self.ctx.arena.get_access_expr(node_a)
            && let Some(access_b) = self.ctx.arena.get_access_expr(node_b)
        {
            // Both must have same property name and same base expression
            let names_match = self
                .ctx
                .arena
                .get(access_a.name_or_argument)
                .zip(self.ctx.arena.get(access_b.name_or_argument))
                .and_then(|(na, nb)| {
                    let ia = self.ctx.arena.get_identifier(na)?;
                    let ib = self.ctx.arena.get_identifier(nb)?;
                    Some(ia.escaped_text == ib.escaped_text)
                })
                .unwrap_or(false);
            if names_match {
                return self.is_same_reference(access_a.expression, access_b.expression);
            }
        }

        a == b
    }

    /// Apply correlated narrowing for destructured bindings.
    ///
    /// When `const { data, isSuccess } = useQuery()` returns a union type,
    /// and `isSuccess` is narrowed (e.g. via truthiness check in `if (isSuccess)`),
    /// this function narrows the source union type and re-derives `data`'s type.
    fn apply_correlated_narrowing(
        &self,
        analyzer: &FlowAnalyzer<'_>,
        sym_id: SymbolId,
        info: &crate::context::DestructuredBindingInfo,
        declared_type: TypeId,
        flow_node: tsz_binder::FlowNodeId,
    ) -> TypeId {
        let Some(source_members) = union_members_for_type(self.ctx.types, info.source_type) else {
            return declared_type;
        };

        // Find all siblings in the same binding group
        let siblings: Vec<_> = self
            .ctx
            .destructured_bindings
            .iter()
            .filter(|(s, i)| **s != sym_id && i.group_id == info.group_id && i.is_const)
            .map(|(s, i)| (*s, i.clone()))
            .collect();

        if siblings.is_empty() {
            return declared_type;
        }

        // Start with the full source type members
        let source_member_count = source_members.len();
        let mut remaining_members = source_members;
        let member_binding_type = |member: TypeId,
                                   binding: &crate::context::DestructuredBindingInfo|
         -> Option<TypeId> {
            if !binding.property_name.is_empty() {
                let mut current = member;
                for segment in binding.property_name.split('.') {
                    let prop = find_property_in_object_by_str(self.ctx.types, current, segment)?;
                    current = prop.type_id;
                }
                Some(current)
            } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, member) {
                resolve_tuple_binding_type(
                    self.ctx.types,
                    &elems,
                    binding.element_index as usize,
                    binding.is_rest,
                )
            } else {
                None
            }
        };
        let symbol_identifier_ref = |sym: SymbolId| -> Option<NodeIndex> {
            let mut declaration_ident: Option<NodeIndex> = None;
            for (&node_id, &node_sym) in &self.ctx.binder.node_symbols {
                if node_sym != sym {
                    continue;
                }
                let idx = NodeIndex(node_id);
                let Some(node) = self.ctx.arena.get(idx) else {
                    continue;
                };
                if node.kind != SyntaxKind::Identifier as u16 {
                    continue;
                }

                // Prefer a usage site over declaration identifier nodes in binding/variable/parameter
                // declarations, because usage nodes carry richer flow facts (e.g. switch discriminants).
                let is_declaration_ident = self
                    .ctx
                    .arena
                    .get_extended(idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        parent.kind == syntax_kind_ext::BINDING_ELEMENT
                            || parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            || parent.kind == syntax_kind_ext::PARAMETER
                    });

                if !is_declaration_ident {
                    return Some(idx);
                }
                declaration_ident = Some(idx);
            }
            declaration_ident
        };
        let switch_flow_node = {
            let mut found = None;
            let mut visited = FxHashSet::default();
            let mut worklist = VecDeque::from([flow_node]);

            // Statement-level flow nodes inside a clause can sit several hops away
            // from the clause marker and may have multiple antecedents. Walk the
            // local graph slice instead of assuming a single short chain.
            while let Some(candidate) = worklist.pop_front() {
                if candidate.is_none() || !visited.insert(candidate) {
                    continue;
                }
                let Some(flow) = self.ctx.binder.flow_nodes.get(candidate) else {
                    continue;
                };
                if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                    found = Some(candidate);
                    break;
                }
                for &antecedent in &flow.antecedent {
                    if antecedent.is_some() {
                        worklist.push_back(antecedent);
                    }
                }
            }
            found
        };
        let switch_clause_context = switch_flow_node
            .and_then(|switch_flow_id| self.ctx.binder.flow_nodes.get(switch_flow_id))
            .filter(|flow| flow.has_any_flags(flow_flags::SWITCH_CLAUSE))
            .and_then(|flow| {
                let clause_idx = flow.node;
                let is_implicit_default = self
                    .ctx
                    .arena
                    .get(clause_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CASE_BLOCK);
                let switch_idx = if is_implicit_default {
                    self.ctx
                        .arena
                        .get_extended(clause_idx)
                        .and_then(|ext| (ext.parent.is_some()).then_some(ext.parent))
                } else {
                    self.ctx.binder.get_switch_for_clause(clause_idx)
                }?;
                let switch_node = self.ctx.arena.get(switch_idx)?;
                let switch_data = self.ctx.arena.get_switch(switch_node)?;
                let switch_sym = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, switch_data.expression)?;

                let collect_case_types = |case_block: NodeIndex| -> Vec<TypeId> {
                    let Some(case_block_node) = self.ctx.arena.get(case_block) else {
                        return Vec::new();
                    };
                    let Some(block) = self.ctx.arena.get_block(case_block_node) else {
                        return Vec::new();
                    };
                    block
                        .statements
                        .nodes
                        .iter()
                        .filter_map(|&case_clause_idx| {
                            let clause_node = self.ctx.arena.get(case_clause_idx)?;
                            let clause = self.ctx.arena.get_case_clause(clause_node)?;
                            if clause.expression.is_none() {
                                return None;
                            }
                            self.ctx
                                .node_types
                                .get(&clause.expression.0)
                                .copied()
                                .or_else(|| self.literal_type_from_initializer(clause.expression))
                        })
                        .collect()
                };

                if is_implicit_default {
                    Some((switch_sym, None, collect_case_types(switch_data.case_block)))
                } else {
                    let clause_node = self.ctx.arena.get(clause_idx)?;
                    let clause = self.ctx.arena.get_case_clause(clause_node)?;
                    if clause.expression.is_none() {
                        Some((switch_sym, None, collect_case_types(switch_data.case_block)))
                    } else {
                        Some((
                            switch_sym,
                            self.ctx.node_types.get(&clause.expression.0).copied(),
                            Vec::new(),
                        ))
                    }
                }
            });

        // For each sibling, check if it's been narrowed
        for (sib_sym, sib_info) in &siblings {
            if let Some((switch_sym, case_type, default_case_types)) = &switch_clause_context
                && *switch_sym == *sib_sym
            {
                if let Some(case_ty) = *case_type {
                    remaining_members.retain(|&member| {
                        if let Some(prop_type) = member_binding_type(member, sib_info) {
                            prop_type == case_ty || {
                                let env = self.ctx.type_env.borrow();
                                are_types_mutually_subtype_with_env(
                                    self.ctx.types,
                                    &env,
                                    case_ty,
                                    prop_type,
                                    self.ctx.strict_null_checks(),
                                )
                            }
                        } else {
                            true
                        }
                    });
                } else if !default_case_types.is_empty() {
                    remaining_members.retain(|&member| {
                        let Some(prop_type) = member_binding_type(member, sib_info) else {
                            return true;
                        };
                        !default_case_types.iter().any(|&case_ty| {
                            prop_type == case_ty || {
                                let env = self.ctx.type_env.borrow();
                                are_types_mutually_subtype_with_env(
                                    self.ctx.types,
                                    &env,
                                    case_ty,
                                    prop_type,
                                    self.ctx.strict_null_checks(),
                                )
                            }
                        })
                    });
                }
                continue;
            }

            // Get the sibling's initial type (from the union source)
            let sib_initial = if let Some(&cached) = self.ctx.symbol_types.get(sib_sym) {
                cached
            } else {
                continue;
            };

            // Get the sibling's reference node (value_declaration)
            let Some(sib_sym_data) = self.ctx.binder.symbols.get(*sib_sym) else {
                continue;
            };
            let mut sib_ref = sib_sym_data.value_declaration;
            if sib_ref.is_none() {
                continue;
            }
            // Flow analysis expects an expression/identifier reference node. For destructured
            // symbols the declaration is often a BindingElement; use its identifier name node.
            if let Some(decl_node) = self.ctx.arena.get(sib_ref)
                && decl_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(decl_node)
                && let Some(name_node) = self.ctx.arena.get(binding.name)
                && name_node.kind == SyntaxKind::Identifier as u16
            {
                sib_ref = binding.name;
            }

            // Get the sibling's narrowed type at this flow node
            let mut sib_narrowed = analyzer.get_flow_type(sib_ref, sib_initial, flow_node);
            if sib_narrowed == sib_initial
                && let Some(identifier_ref) = symbol_identifier_ref(*sib_sym)
                && identifier_ref != sib_ref
            {
                sib_narrowed = analyzer.get_flow_type(identifier_ref, sib_initial, flow_node);
            }

            // If the sibling wasn't narrowed, skip
            if sib_narrowed == sib_initial {
                continue;
            }

            remaining_members.retain(|&member| {
                let member_prop_type = member_binding_type(member, sib_info);

                if let Some(prop_type) = member_prop_type {
                    // Keep this member if the sibling's narrowed type overlaps
                    // with the member's property type
                    prop_type == sib_narrowed || {
                        let env = self.ctx.type_env.borrow();
                        are_types_mutually_subtype_with_env(
                            self.ctx.types,
                            &env,
                            sib_narrowed,
                            prop_type,
                            self.ctx.strict_null_checks(),
                        )
                    }
                } else {
                    true // Keep if we can't determine
                }
            });
        }

        // If no members were filtered, no correlated narrowing happened
        if remaining_members.len() == source_member_count {
            return declared_type;
        }

        // If all members were filtered, return never
        if remaining_members.is_empty() {
            return TypeId::NEVER;
        }

        // Re-derive this symbol's property type from the remaining source members
        let mut result_types = Vec::new();
        for member in &remaining_members {
            let member_prop_type = if !info.property_name.is_empty() {
                let mut current = *member;
                let mut resolved = Some(current);
                for segment in info.property_name.split('.') {
                    resolved = find_property_in_object_by_str(self.ctx.types, current, segment)
                        .map(|p| p.type_id);
                    if let Some(next) = resolved {
                        current = next;
                    } else {
                        break;
                    }
                }
                resolved
            } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, *member) {
                resolve_tuple_binding_type(
                    self.ctx.types,
                    &elems,
                    info.element_index as usize,
                    info.is_rest,
                )
            } else {
                None
            };

            if let Some(ty) = member_prop_type {
                result_types.push(ty);
            }
        }

        if result_types.is_empty() {
            return declared_type;
        }
        let correlated = tsz_solver::utils::union_or_single(self.ctx.types, result_types);

        // Combine correlated narrowing with flow narrowing (e.g., truthiness check).
        // When `declared_type` (the flow-narrowed type) differs from the original
        // symbol type, both narrowings are active and we need their intersection.
        // Example: `if (payload) { if (kind === 'A') { ... } }` where:
        //   - flow narrows payload: number|undefined|string|undefined → number|string
        //   - correlated narrows payload: → number|undefined
        //   - combined: → number
        if correlated != declared_type
            && let Some(c_members) = union_members_for_type(self.ctx.types, correlated)
        {
            let n_members = union_members_for_type(self.ctx.types, declared_type);
            let filtered: Vec<_> = c_members
                .iter()
                .filter(|&&c| {
                    if let Some(ref n_members) = n_members {
                        n_members.contains(&c)
                    } else {
                        c == declared_type
                    }
                })
                .copied()
                .collect();
            if !filtered.is_empty() && filtered.len() < c_members.len() {
                return tsz_solver::utils::union_or_single(self.ctx.types, filtered);
            }
        }
        correlated
    }

    /// Get the symbol for an identifier node.
    ///
    /// Returns None if the node is not an identifier or has no symbol.
    fn get_symbol_for_identifier(&self, idx: NodeIndex) -> Option<SymbolId> {
        use tsz_scanner::SyntaxKind;

        let node = self.ctx.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // First try get_node_symbol, then fall back to resolve_identifier
        self.ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
    }

    // NOTE: Rule #42 captured-variable methods (is_inside_closure, is_mutable_binding,
    // is_captured_variable, find_enclosing_function_scope) were removed. Rule #42 is now
    // enforced at the flow-graph level in check_flow() (core.rs START node handling).
}

/// Resolve a tuple binding's type from its elements, handling rest bindings.
///
/// For rest bindings (`...rest`), finds the rest element from `element_index` onward
/// and wraps its inner type as an array. For non-rest bindings, returns the element
/// at the given index directly.
fn resolve_tuple_binding_type(
    db: &dyn tsz_solver::QueryDatabase,
    elems: &[TupleElement],
    element_index: usize,
    is_rest: bool,
) -> Option<TypeId> {
    if is_rest {
        let rest_elem = elems
            .iter()
            .skip(element_index)
            .find(|e| e.rest)
            .or_else(|| elems.get(element_index))?;
        Some(db.factory().array(rest_elem.type_id))
    } else {
        elems.get(element_index).map(|e| e.type_id)
    }
}
