//! Definite assignment analysis and flow-based type narrowing.
//!
//! Extracted from `flow_analysis.rs`: control flow narrowing at identifier usage,
//! definite assignment checking (TS2454), TDZ analysis, and typeof-based narrowing.

use crate::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::{
    are_types_mutually_subtype_with_env, tuple_elements_for_type, union_members_for_type,
};
use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use std::rc::Rc;
use tsz_binder::{SymbolId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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
        // Skip flow narrowing when getting assignment target types.
        // For assignments like `foo[x] = 1` after `if (foo[x] === undefined)`,
        // we need the declared type (e.g., `number | undefined`) not the narrowed type (`undefined`).
        if self.ctx.skip_flow_narrowing {
            return declared_type;
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
                None => return declared_type, // No flow info - use declared type
            }
        };

        // Fast path: `any` and `error` types cannot be meaningfully narrowed.
        // NOTE: We only skip for direct `any`/`error`, NOT for compound types that
        // contain `any` (e.g. unions of classes with `any`-returning methods).
        // TypeScript narrows such compound types normally via instanceof/typeof.
        if declared_type == TypeId::ANY || declared_type == TypeId::ERROR {
            return declared_type;
        }

        // Rule #42 only applies inside closures. Avoid symbol resolution work
        // on the common non-closure path.
        if self.is_inside_closure()
            && let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && self.is_captured_variable(sym_id, idx)
            && self.is_mutable_binding(sym_id)
        {
            // Rule #42: Reset narrowing for captured mutable bindings in closures
            // (const variables preserve narrowing, let/var reset to declared type)
            return declared_type;
        }

        // Skip narrowing for `never` — it's the bottom type, nothing to narrow.
        // All other types (unions, objects, callables, type params, primitives, etc.)
        // can benefit from flow narrowing (instanceof, typeof, truthiness, etc.).
        if declared_type == TypeId::NEVER {
            return declared_type;
        }

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
            ) || tsz_solver::visitor::is_literal_type_db(self.ctx.types, declared_type))
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
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        );

        let narrowed = analyzer.get_flow_type(idx, declared_type, flow_node);

        // Correlated narrowing for destructured bindings.
        // When `const { data, isSuccess } = useQuery()` and we check `isSuccess`,
        // narrowing of `isSuccess` should also narrow `data`.
        if let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && let Some(info) = self.ctx.destructured_bindings.get(&sym_id).cloned()
            && info.is_const
        {
            return self.apply_correlated_narrowing(&analyzer, sym_id, &info, narrowed, flow_node);
        }

        narrowed
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
        let member_binding_type =
            |member: TypeId, binding: &crate::context::DestructuredBindingInfo| -> Option<TypeId> {
                if !binding.property_name.is_empty() {
                    let mut current = member;
                    for segment in binding.property_name.split('.') {
                        let prop = tsz_solver::type_queries::find_property_in_object_by_str(
                            self.ctx.types,
                            current,
                            segment,
                        )?;
                        current = prop.type_id;
                    }
                    Some(current)
                } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, member) {
                    elems.get(binding.element_index as usize).map(|e| e.type_id)
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
            let mut candidate = flow_node;
            let mut found = None;
            // Walk a short antecedent chain to recover switch-clause context for
            // nodes immediately after a clause (e.g. statements in default block).
            for _ in 0..4 {
                let Some(flow) = self.ctx.binder.flow_nodes.get(candidate) else {
                    break;
                };
                if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                    found = Some(candidate);
                    break;
                }
                let Some(&ant) = flow.antecedent.first() else {
                    break;
                };
                if ant.is_none() {
                    break;
                }
                candidate = ant;
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
                            self.ctx.node_types.get(&clause.expression.0).copied()
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
                    resolved = tsz_solver::type_queries::find_property_in_object_by_str(
                        self.ctx.types,
                        current,
                        segment,
                    )
                    .map(|p| p.type_id);
                    if let Some(next) = resolved {
                        current = next;
                    } else {
                        break;
                    }
                }
                resolved
            } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, *member) {
                elems.get(info.element_index as usize).map(|e| e.type_id)
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
        tsz_solver::utils::union_or_single(self.ctx.types, result_types)
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

    /// Check if we're currently inside a closure (function expression or arrow function).
    ///
    /// This is used to apply Rule #42: CFA Invalidation in Closures.
    ///
    /// Returns true if inside a function expression, arrow function, or method expression.
    const fn is_inside_closure(&self) -> bool {
        self.ctx.inside_closure_depth > 0
    }

    /// Check if a symbol is a mutable binding (let or var) vs immutable (const).
    ///
    /// This is used to implement TypeScript's Rule #42 for type narrowing in closures:
    /// - const variables preserve narrowing through closures (immutable)
    /// - let/var variables lose narrowing when accessed from closures (mutable)
    ///
    /// Implementation checks:
    /// 1. Get the symbol's value declaration
    /// 2. Check if it's a `VariableDeclaration`
    /// 3. Look at the parent `VariableDeclarationList`'s `NodeFlags`
    /// 4. If CONST flag is set → const (immutable)
    /// 5. Otherwise → let/var (mutable)
    ///
    /// Returns true for let/var (mutable), false for const (immutable).
    fn is_mutable_binding(&self, sym_id: SymbolId) -> bool {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return true, // Assume mutable if we can't determine
        };

        // Check the value declaration
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return true; // Assume mutable if no declaration
        }

        let decl_node = match self.ctx.arena.get(decl_idx) {
            Some(node) => node,
            None => return true,
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        // The value_declaration points to VARIABLE_DECLARATION, we need to check its parent's flags
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            // Get the parent (VARIABLE_DECLARATION_LIST) via extended info
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                let flags = parent_node.flags as u32;
                let is_const = (flags & node_flags::CONST) != 0;
                return !is_const; // Return true if NOT const (i.e., let or var)
            }
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;
        !is_const // Return true if NOT const (i.e., let or var)
    }

    /// Check if a variable is captured from an outer scope (vs declared locally).
    ///
    /// Bug #1.2: Rule #42 should only apply to captured variables, not local variables.
    /// - Variables declared INSIDE the closure should narrow normally
    /// - Variables captured from OUTER scope reset narrowing (for let/var)
    ///
    /// This is determined by checking if the variable's declaration is in an ancestor scope.
    fn is_captured_variable(&self, sym_id: SymbolId, reference: NodeIndex) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false,
        };

        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false;
        }

        // Find the enclosing scope of the declaration
        let decl_scope_id = match self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, decl_idx)
        {
            Some(scope_id) => scope_id,
            None => return false,
        };

        // Find the enclosing scope of the usage site (where the variable is accessed).
        let usage_scope_id = match self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, reference)
        {
            Some(scope_id) => scope_id,
            None => return false,
        };

        // If declared and used in the same scope, not captured
        if decl_scope_id == usage_scope_id {
            return false;
        }

        // A variable is "captured" only if it crosses a function boundary.
        // Block scopes (if, while, for) within the same function don't count.
        // We walk up from the declaration scope and usage scope to find
        // their enclosing function/source-file scopes, then compare those.
        let decl_fn_scope = self.find_enclosing_function_scope(decl_scope_id);
        let usage_fn_scope = self.find_enclosing_function_scope(usage_scope_id);

        // If both are in the same function scope, the variable is NOT captured
        if decl_fn_scope == usage_fn_scope {
            return false;
        }

        // The declaration's function scope must be an ancestor of the usage's function scope
        // for the variable to be considered captured
        let mut scope_id = usage_fn_scope;
        let mut iterations = 0;
        while scope_id.is_some() && iterations < MAX_TREE_WALK_ITERATIONS {
            if scope_id == decl_fn_scope {
                return true;
            }

            scope_id = self
                .ctx
                .binder
                .scopes
                .get(scope_id.0 as usize)
                .map_or(tsz_binder::ScopeId::NONE, |scope| scope.parent);

            iterations += 1;
        }

        false
    }

    /// Walk up the scope chain to find the nearest function/source-file/module scope.
    /// Block scopes are skipped.
    fn find_enclosing_function_scope(&self, scope_id: tsz_binder::ScopeId) -> tsz_binder::ScopeId {
        use tsz_binder::ContainerKind;

        let mut current = scope_id;
        let mut iterations = 0;
        while current.is_some() && iterations < MAX_TREE_WALK_ITERATIONS {
            if let Some(scope) = self.ctx.binder.scopes.get(current.0 as usize) {
                match scope.kind {
                    ContainerKind::Function | ContainerKind::SourceFile | ContainerKind::Module => {
                        return current;
                    }
                    _ => {
                        current = scope.parent;
                    }
                }
            } else {
                break;
            }
            iterations += 1;
        }
        current
    }
}
