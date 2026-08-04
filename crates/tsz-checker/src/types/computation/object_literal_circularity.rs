use super::object_literal::LITERAL_DISPLAY_ORDER_BASE;
use crate::query_boundaries::object_literal_context as object_context_query;
use crate::query_boundaries::signature_building as signature_building_boundary;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether an object-literal member, when its body is checked, binds the
    /// synthetic object-literal `this` type — i.e. a member whose `this` refers
    /// to the surrounding object literal rather than the enclosing scope.
    ///
    /// Methods, get/set accessors, and `function`-expression property
    /// initializers all capture the object as `this`. Arrow-function property
    /// initializers do NOT: an arrow inherits `this` lexically from the
    /// enclosing scope, so a data member declared after an arrow is irrelevant
    /// to that arrow's `this`.
    fn object_literal_member_captures_synthetic_this(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return false;
        };
        if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
            || elem_node.kind == syntax_kind_ext::GET_ACCESSOR
            || elem_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            return true;
        }
        let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
            return false;
        };
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        self.ctx
            .arena
            .get(initializer)
            .is_some_and(|init_node| init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
    }

    /// Whether a function-like node (method shorthand, `function`-expression
    /// property initializer, or accessor) declares an explicit `this:`
    /// parameter. tsc's `getThisTypeOfSignature` makes such a `this` bind to
    /// exactly that declared type, so the enclosing object literal's synthetic
    /// `this` must not be pushed for the member body. Centralizing the check
    /// keeps every object-literal callable-member path in sync (see #14843).
    pub(super) fn function_like_has_explicit_this_parameter(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        let params: &[NodeIndex] = if let Some(method) = self.ctx.arena.get_method_decl(node) {
            &method.parameters.nodes
        } else if let Some(func) = self.ctx.arena.get_function(node) {
            &func.parameters.nodes
        } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
            &accessor.parameters.nodes
        } else {
            return false;
        };
        self.get_explicit_this_type_annotation(params).is_some()
    }

    /// Best-effort `PropertyInfo` entries for the object literal's non-method
    /// members (data properties, shorthands, and accessors) declared *after* the
    /// first `this`-capturing callable member.
    ///
    /// `tsc` types `this` inside an object-literal method/accessor/function
    /// property as the *complete* object literal type. tsz instead builds the
    /// synthetic `this` incrementally, so a member declared after the callable
    /// is invisible to it, producing spurious `TS2339` (and the consequent
    /// `TS7023` circular-return diagnostic) on `this.<laterMember>`. Members
    /// declared *before* the first callable are already present in the
    /// incremental `properties` map by the time the callable body is checked,
    /// and method / `function` / arrow-function members are already represented
    /// in `obj_all_method_names`, so only the trailing non-method members need a
    /// prescan.
    ///
    /// The prescan is deliberately free of expression-checker side effects: it
    /// never invokes `get_type_of_node` (which would populate `node_types` and
    /// suppress the main loop's diagnostics on the subsequent authoritative
    /// pass). Trailing data properties with a literal initializer get their
    /// precise widened type via `literal_type_from_initializer`; every other
    /// trailing member is recorded as `any`, which is enough to make the member
    /// *exist* on `this` (clearing the spurious error) without ever introducing
    /// a new diagnostic.
    pub(super) fn object_literal_trailing_member_props(
        &mut self,
        elements: &[NodeIndex],
    ) -> FxHashMap<Atom, tsz_solver::PropertyInfo> {
        let mut result: FxHashMap<Atom, tsz_solver::PropertyInfo> = FxHashMap::default();
        if self.ctx.in_destructuring_target {
            return result;
        }
        let Some(first_callable_pos) = elements
            .iter()
            .position(|&elem_idx| self.object_literal_member_captures_synthetic_this(elem_idx))
        else {
            return result;
        };
        // Common arrangement ("data first, methods last"): nothing is declared
        // after the only/last callable, so the incremental `properties` map is
        // already complete and no prescan work is needed.
        if first_callable_pos + 1 >= elements.len() {
            return result;
        }

        // Names declared with a `set` accessor anywhere in the literal — a
        // trailing get-accessor is only `readonly` when it has no paired setter.
        let setter_names: FxHashSet<Atom> = elements
            .iter()
            .filter_map(|&elem_idx| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                if elem_node.kind != syntax_kind_ext::SET_ACCESSOR {
                    return None;
                }
                let accessor = self.ctx.arena.get_accessor(elem_node)?;
                self.get_property_name_resolved(accessor.name)
                    .map(|name| self.ctx.types.intern_string(&name))
            })
            .collect();

        let in_const = self.ctx.in_const_assertion;
        for (pos, &elem_idx) in elements.iter().enumerate().skip(first_callable_pos + 1) {
            let Some((name_atom, type_id, readonly)) =
                self.object_literal_trailing_member_prop_entry(elem_idx, &setter_names)
            else {
                continue;
            };
            result.insert(
                name_atom,
                object_context_query::synthetic_this_property(
                    name_atom,
                    type_id,
                    type_id,
                    readonly || in_const,
                    false,
                    // Trailing members share the direct-member display range with
                    // the incrementally-built `properties` (which start at
                    // `LITERAL_DISPLAY_ORDER_BASE`), so the synthetic `this` type
                    // renders every member in source order (tsc parity).
                    LITERAL_DISPLAY_ORDER_BASE.saturating_add(pos as u32),
                ),
            );
        }
        result
    }

    /// Compute `(name, read_type, readonly)` for a single trailing non-method
    /// member, or `None` when the member is already represented elsewhere
    /// (methods / `function` / arrow properties) or carries no statically
    /// resolvable name.
    fn object_literal_trailing_member_prop_entry(
        &mut self,
        elem_idx: NodeIndex,
        setter_names: &FxHashSet<Atom>,
    ) -> Option<(Atom, TypeId, bool)> {
        let elem_node = self.ctx.arena.get(elem_idx)?;

        // Accessors contribute a data-shaped property read through `this`.
        if elem_node.kind == syntax_kind_ext::GET_ACCESSOR
            || elem_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            let accessor_name = self.ctx.arena.get_accessor(elem_node)?.name;
            if self.computed_member_key_is_wide_symbol(accessor_name) {
                return None;
            }
            let name = self.get_property_name_resolved(accessor_name)?;
            let name_atom = self.ctx.types.intern_string(&name);
            // Read type is left as `any`: the member only needs to *exist* on
            // the synthetic `this` to clear the spurious error, and inferring an
            // un-annotated accessor body here would re-run the checker.
            let readonly = elem_node.kind == syntax_kind_ext::GET_ACCESSOR
                && !setter_names.contains(&name_atom);
            return Some((name_atom, TypeId::ANY, readonly));
        }

        // Shorthand property: { x } — the value is an outer binding; record the
        // member as existing without resolving its symbol (which would touch the
        // node cache / flow diagnostics).
        if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            let shorthand = self.ctx.arena.get_shorthand_property(elem_node)?;
            let ident = self
                .ctx
                .arena
                .get(shorthand.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))?;
            let name_atom = self.ctx.types.intern_string(&ident.escaped_text);
            return Some((name_atom, TypeId::ANY, false));
        }

        // Data property assignment. `function`/arrow initializers are already
        // represented in `obj_all_method_names`, so they are skipped here.
        let prop = self.ctx.arena.get_property_assignment(elem_node)?;
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        if self.ctx.arena.get(initializer).is_some_and(|init_node| {
            init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        }) {
            return None;
        }
        if self.computed_member_key_is_wide_symbol(prop.name) {
            return None;
        }
        let name = self.get_property_name_resolved(prop.name)?;
        let name_atom = self.ctx.types.intern_string(&name);
        // A literal initializer has a statically known type; a regular
        // object-literal data property widens it (e.g. `v: 1` -> `number`),
        // while an enclosing `as const` keeps the literal for synthetic `this`.
        // Anything else stays `any` (member exists, no spurious error).
        let type_id = match self.literal_type_from_initializer(prop.initializer) {
            Some(literal) if self.ctx.in_const_assertion => literal,
            Some(literal) => self.widen_literal_type(literal),
            None => TypeId::ANY,
        };
        Some((name_atom, type_id, false))
    }

    /// Merge trailing-member entries into a synthetic-`this` property vector,
    /// skipping any name already present (earlier members and methods take
    /// precedence). The entries already carry their final `readonly` flag (the
    /// `const`-assertion case is baked in by `object_literal_trailing_member_props`).
    pub(crate) fn merge_trailing_member_props(
        this_props: &mut Vec<tsz_solver::PropertyInfo>,
        trailing_member_props: &FxHashMap<Atom, tsz_solver::PropertyInfo>,
    ) {
        for (&name_atom, info) in trailing_member_props {
            if this_props.iter().any(|p| p.name == name_atom) {
                continue;
            }
            this_props.push(info.clone());
        }
    }

    /// Map each callable member (method shorthand or `function`/arrow property
    /// initializer) to its declaration node and a display-order slot. The slot
    /// is `LITERAL_DISPLAY_ORDER_BASE + <source index>` so that when a
    /// synthetic-`this` type splices these methods next to the incrementally
    /// built `properties` (which occupy the same direct-member range), every
    /// member sorts in source order for diagnostic display.
    pub(super) fn object_literal_callable_member_names(
        &mut self,
        elements: &[NodeIndex],
    ) -> FxHashMap<Atom, (NodeIndex, u32)> {
        elements
            .iter()
            .enumerate()
            .filter_map(|(pos, &elem_idx)| {
                let elem_node = self.ctx.arena.get(elem_idx)?;

                if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                    let name = self.get_property_name(method.name)?;
                    return Some((
                        self.ctx.types.intern_string(&name),
                        (
                            elem_idx,
                            LITERAL_DISPLAY_ORDER_BASE.saturating_add(pos as u32),
                        ),
                    ));
                }

                let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                let initializer = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(prop.initializer);
                let init_node = self.ctx.arena.get(initializer)?;
                if !matches!(
                    init_node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                ) {
                    return None;
                }
                let name = self.get_property_name_resolved(prop.name)?;
                Some((
                    self.ctx.types.intern_string(&name),
                    (
                        elem_idx,
                        LITERAL_DISPLAY_ORDER_BASE.saturating_add(pos as u32),
                    ),
                ))
            })
            .collect()
    }

    pub(super) fn object_literal_circular_return_method_sites(
        &self,
        obj_all_method_names: &FxHashMap<Atom, (NodeIndex, u32)>,
    ) -> FxHashSet<NodeIndex> {
        let unannotated_methods: FxHashMap<Atom, NodeIndex> = obj_all_method_names
            .iter()
            .filter_map(|(&name, &(elem_idx, _))| {
                self.object_literal_callable_member_has_inferred_return(elem_idx)
                    .then_some((name, elem_idx))
            })
            .collect();
        if unannotated_methods.is_empty() {
            return FxHashSet::default();
        }

        let mut graph: FxHashMap<NodeIndex, Vec<NodeIndex>> = FxHashMap::default();
        for &elem_idx in unannotated_methods.values() {
            let Some(body_idx) = self.object_literal_callable_member_body(elem_idx) else {
                continue;
            };
            let mut callees = FxHashSet::default();
            self.collect_this_member_calls_in_returns(body_idx, &unannotated_methods, &mut callees);
            if !callees.is_empty() {
                graph.insert(elem_idx, callees.into_iter().collect());
            }
        }

        let mut circular_sites = FxHashSet::default();
        let mut visited = FxHashSet::default();
        let mut stack = Vec::new();
        for &elem_idx in unannotated_methods.values() {
            Self::collect_circular_return_graph_sites(
                elem_idx,
                &graph,
                &mut visited,
                &mut stack,
                &mut circular_sites,
            );
        }
        circular_sites
    }

    fn object_literal_callable_member_body(&self, elem_idx: NodeIndex) -> Option<NodeIndex> {
        let elem_node = self.ctx.arena.get(elem_idx)?;
        if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
            return method.body.into_option();
        }

        let prop = self.ctx.arena.get_property_assignment(elem_node)?;
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return None;
        }
        self.ctx.arena.get_function(init_node)?.body.into_option()
    }

    fn object_literal_callable_member_has_inferred_return(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return false;
        };
        if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
            return method.type_annotation.is_none() && method.body.is_some();
        }

        let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
            return false;
        };
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        let Some(init_node) = self.ctx.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_function(init_node)
            .is_some_and(|func| func.type_annotation.is_none() && func.body.is_some())
    }

    fn collect_circular_return_graph_sites(
        elem_idx: NodeIndex,
        graph: &FxHashMap<NodeIndex, Vec<NodeIndex>>,
        visited: &mut FxHashSet<NodeIndex>,
        stack: &mut Vec<NodeIndex>,
        circular_sites: &mut FxHashSet<NodeIndex>,
    ) {
        if let Some(cycle_start) = stack.iter().position(|&stacked| stacked == elem_idx) {
            circular_sites.extend(stack[cycle_start..].iter().copied());
            return;
        }
        if visited.contains(&elem_idx) {
            return;
        }

        stack.push(elem_idx);
        if let Some(targets) = graph.get(&elem_idx) {
            for &target in targets {
                Self::collect_circular_return_graph_sites(
                    target,
                    graph,
                    visited,
                    stack,
                    circular_sites,
                );
            }
        }
        stack.pop();
        visited.insert(elem_idx);
    }

    fn collect_this_member_calls_in_returns(
        &self,
        body_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    self.collect_this_member_calls_in_return_statement(
                        stmt_idx,
                        unannotated_methods,
                        callees,
                    );
                }
            }
        } else {
            self.collect_this_member_calls_in_return_expression(
                body_idx,
                unannotated_methods,
                callees,
            );
        }
    }

    fn collect_this_member_calls_in_return_statement(
        &self,
        stmt_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.collect_this_member_calls_in_return_expression(
                        ret.expression,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_this_member_calls_in_return_statement(
                            stmt,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_this_member_calls_in_return_statement(
                        if_data.then_statement,
                        unannotated_methods,
                        callees,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            if_data.else_statement,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_this_member_calls_in_return_statement(
                                    stmt,
                                    unannotated_methods,
                                    callees,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_this_member_calls_in_return_statement(
                        try_data.try_block,
                        unannotated_methods,
                        callees,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            try_data.catch_clause,
                            unannotated_methods,
                            callees,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            try_data.finally_block,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_this_member_calls_in_return_statement(
                        catch_data.block,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_this_member_calls_in_return_statement(
                        loop_data.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_this_member_calls_in_return_statement(
                        loop_data.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_this_member_calls_in_return_statement(
                        labeled.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_this_member_calls_in_return_expression(
        &self,
        expr_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        if self.object_literal_expression_is_void_prefix_unary(expr_idx) {
            return;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return;
        }

        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
        {
            let callee = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(call.expression);
            if let Some(callee_node) = self.ctx.arena.get(callee)
                && callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
            {
                let receiver = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression);
                if self.ctx.arena.get(receiver).is_some_and(|receiver_node| {
                    receiver_node.kind == SyntaxKind::ThisKeyword as u16
                }) && let Some(name) = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.as_str())
                {
                    let atom = self.ctx.types.intern_string(name);
                    if let Some(&target_idx) = unannotated_methods.get(&atom) {
                        callees.insert(target_idx);
                    }
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_this_member_calls_in_return_expression(
                child_idx,
                unannotated_methods,
                callees,
            );
        }
    }

    fn object_literal_expression_is_void_prefix_unary(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_unary_expr(node)
                    .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
        })
    }

    pub(super) fn build_object_literal_method_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        trailing_member_props: &rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        current_method_idx: NodeIndex,
        current_method_name: &str,
        current_method_type_override: Option<TypeId>,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        // Members declared after the current method are not yet in `properties`;
        // splice them in so `this.<laterMember>` resolves like the full object.
        Self::merge_trailing_member_props(&mut this_props, trailing_member_props);

        let current_method_name_atom = self.ctx.types.intern_string(current_method_name);
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let method_type = if method_name_atom == current_method_name_atom {
                if let Some(override_type) = current_method_type_override {
                    override_type
                } else {
                    let Some(current_method_node) = self.ctx.arena.get(current_method_idx) else {
                        continue;
                    };
                    let Some(current_method) = self.ctx.arena.get_method_decl(current_method_node)
                    else {
                        continue;
                    };
                    let (_, tp_updates) =
                        self.push_type_parameters(&current_method.type_parameters);
                    let params = current_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param =
                                self.ctx.arena.get(param_idx).and_then(|param_node| {
                                    self.ctx.arena.get_parameter(param_node)
                                })?;
                            Some(signature_building_boundary::param_info(
                                self.ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                param.question_token || param.initializer.is_some(),
                                param.dot_dot_dot_token,
                            ))
                        })
                        .collect();
                    let placeholder = object_context_query::synthetic_this_method_callable(
                        self.ctx.types,
                        params,
                        TypeId::VOID,
                    );
                    self.pop_type_parameters(tp_updates);
                    placeholder
                }
            } else {
                let (other_params, other_return_type) = self
                    .ctx
                    .arena
                    .get(other_elem_idx)
                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                    .map(|other_method| {
                        let params: Vec<tsz_solver::ParamInfo> = other_method
                            .parameters
                            .nodes
                            .iter()
                            .filter_map(|&param_idx| {
                                let param = self
                                    .ctx
                                    .arena
                                    .get(param_idx)
                                    .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                                if let Some(name_node) = self.ctx.arena.get(param.name)
                                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    && ident.escaped_text == "this"
                                {
                                    return None;
                                }
                                Some(signature_building_boundary::param_info(
                                    self.ctx
                                        .arena
                                        .get(param.name)
                                        .and_then(|name_node| {
                                            self.ctx.arena.get_identifier(name_node)
                                        })
                                        .map(|ident| {
                                            self.ctx.types.intern_string(&ident.escaped_text)
                                        }),
                                    if param.type_annotation.is_some() {
                                        self.get_type_from_type_node(param.type_annotation)
                                    } else {
                                        TypeId::ANY
                                    },
                                    param.question_token || param.initializer.is_some(),
                                    param.dot_dot_dot_token,
                                ))
                            })
                            .collect();
                        let return_type = if other_method.type_annotation.is_some() {
                            self.get_type_from_type_node(other_method.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        (params, return_type)
                    })
                    .unwrap_or_else(|| {
                        (
                            vec![signature_building_boundary::param_info(
                                None,
                                TypeId::ANY,
                                false,
                                true,
                            )],
                            TypeId::ANY,
                        )
                    });

                object_context_query::synthetic_this_method_callable(
                    self.ctx.types,
                    other_params,
                    other_return_type,
                )
            };

            this_props.push(object_context_query::synthetic_this_method_property(
                method_name_atom,
                method_type,
                method_type,
                self.ctx.in_const_assertion,
                decl_order,
            ));
        }

        object_context_query::synthetic_this_object(self.ctx.types, this_props)
    }

    /// Build a synthetic `this` type for a function expression that is a property
    /// initializer in an object literal. Similar to `build_object_literal_method_synthetic_this_type`
    /// but for property assignments like `{ prop: function() { this.n } }`.
    ///
    /// The synthetic type includes:
    /// - All already-processed properties from the object literal
    /// - Placeholder signatures for pre-scanned method declarations
    pub(super) fn build_object_literal_fn_property_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        trailing_member_props: &rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        _current_property_name: &str,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        // Members declared after this function-expression property are not yet
        // in `properties`; splice them in so `this.<laterMember>` resolves.
        Self::merge_trailing_member_props(&mut this_props, trailing_member_props);

        // Add placeholder callable types for pre-scanned method declarations
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let (other_params, other_return_type) = self
                .ctx
                .arena
                .get(other_elem_idx)
                .and_then(|n| self.ctx.arena.get_method_decl(n))
                .map(|other_method| {
                    let params: Vec<tsz_solver::ParamInfo> = other_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param = self
                                .ctx
                                .arena
                                .get(param_idx)
                                .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                            if let Some(name_node) = self.ctx.arena.get(param.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && ident.escaped_text == "this"
                            {
                                return None;
                            }
                            Some(signature_building_boundary::param_info(
                                self.ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                param.question_token || param.initializer.is_some(),
                                param.dot_dot_dot_token,
                            ))
                        })
                        .collect();
                    let return_type = if other_method.type_annotation.is_some() {
                        self.get_type_from_type_node(other_method.type_annotation)
                    } else {
                        TypeId::ANY
                    };
                    (params, return_type)
                })
                .unwrap_or_else(|| {
                    (
                        vec![signature_building_boundary::param_info(
                            None,
                            TypeId::ANY,
                            false,
                            true,
                        )],
                        TypeId::ANY,
                    )
                });

            let method_type = object_context_query::synthetic_this_method_callable(
                self.ctx.types,
                other_params,
                other_return_type,
            );
            this_props.push(object_context_query::synthetic_this_method_property(
                method_name_atom,
                method_type,
                TypeId::ANY,
                self.ctx.in_const_assertion,
                decl_order,
            ));
        }

        object_context_query::synthetic_this_object(self.ctx.types, this_props)
    }

    /// Name of the variable binding an object literal initializes, if it is the
    /// initializer of a `const`/`let`/`var` declaration (`const o = { … }`).
    fn object_literal_variable_initializer_name(&self, obj_idx: NodeIndex) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(obj_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        if var_decl.initializer != obj_idx {
            return None;
        }
        let name_node = self.ctx.arena.get(var_decl.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.escaped_text.to_string())
    }

    /// Whether the expression subtree contains an identifier with the given
    /// name (used to detect self-referential data-property initializers).
    fn object_literal_initializer_references_name(&self, idx: NodeIndex, name: &str) -> bool {
        if idx.is_none() {
            return false;
        }
        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && ident.escaped_text == name
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(idx)
            .into_iter()
            .any(|child| self.object_literal_initializer_references_name(child, name))
    }
}
