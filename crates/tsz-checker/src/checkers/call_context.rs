//! Contextual typing analysis for call expressions.
//!
//! Helpers that determine whether and how contextual types from callee signatures
//! should be applied to call arguments. Covers callback sensitivity analysis,
//! generic return context suppression, and argument-level contextual typing
//! decisions.

use crate::computation::complex::is_contextually_sensitive;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn collect_callback_function_indices(
        &self,
        idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
        depth: usize,
    ) {
        if idx.is_none() || depth > 32 {
            return;
        }

        let current = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(current) else {
            return;
        };

        match node.kind {
            k if (k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION)
                && !out.contains(&current) =>
            {
                out.push(current);
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_callback_function_indices(cond.when_true, out, depth + 1);
                    self.collect_callback_function_indices(cond.when_false, out, depth + 1);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn callback_function_indices(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        let mut callbacks = Vec::new();
        self.collect_callback_function_indices(idx, &mut callbacks, 0);
        callbacks
    }

    pub(crate) fn callback_function_index(&self, idx: NodeIndex) -> Option<NodeIndex> {
        self.callback_function_indices(idx).into_iter().next()
    }

    pub(crate) fn is_callback_like_argument(&self, idx: NodeIndex) -> bool {
        !self.callback_function_indices(idx).is_empty()
    }

    pub(crate) fn callback_argument_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        self.is_callback_like_argument(idx)
            .then(|| self.ctx.arena.get(idx))
            .flatten()
            .map(|node| (node.pos, node.end))
    }

    pub(crate) fn contextual_type_is_unresolved_for_argument_refresh(
        &self,
        type_id: TypeId,
    ) -> bool {
        type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, type_id)
            || common::contains_type_parameters(self.ctx.types, type_id)
    }

    pub(crate) fn is_immediate_call_or_new_callee(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..100 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .is_some_and(|call| call.expression == current);
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn is_immediate_call_or_new_argument(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..100 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .and_then(|call| call.arguments.as_ref())
                        .is_some_and(|args| args.nodes.contains(&current));
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn object_literal_function_like_param_spans(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        fn collect<'a>(checker: &CheckerState<'a>, idx: NodeIndex, spans: &mut Vec<(u32, u32)>) {
            let Some(node) = checker.ctx.arena.get(idx) else {
                return;
            };

            match node.kind {
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    spans.extend(checker.function_like_param_spans_for_node(idx));
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = checker.ctx.arena.get_parenthesized(node) {
                        collect(checker, paren.expression, spans);
                    }
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    if let Some(assertion) = checker.ctx.arena.get_type_assertion(node) {
                        collect(checker, assertion.expression, spans);
                    }
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                {
                    let Some(literal) = checker.ctx.arena.get_literal_expr(node) else {
                        return;
                    };

                    for &element_idx in &literal.elements.nodes {
                        let Some(element) = checker.ctx.arena.get(element_idx) else {
                            continue;
                        };

                        match element.kind {
                            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                let Some(prop) = checker.ctx.arena.get_property_assignment(element)
                                else {
                                    continue;
                                };
                                collect(checker, prop.initializer, spans);
                            }
                            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                                if let Some(spread) = checker.ctx.arena.get_spread(element) {
                                    collect(checker, spread.expression, spans);
                                }
                            }
                            _ => collect(checker, element_idx, spans),
                        }
                    }
                }
                _ => {}
            }
        }

        let mut spans = Vec::new();
        collect(self, arg_idx, &mut spans);
        spans
    }

    pub(crate) fn object_literal_noncontextual_function_param_spans(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        fn collect<'a>(checker: &CheckerState<'a>, idx: NodeIndex, spans: &mut Vec<(u32, u32)>) {
            let Some(node) = checker.ctx.arena.get(idx) else {
                return;
            };
            let Some(obj) = checker.ctx.arena.get_literal_expr(node) else {
                return;
            };

            for &element_idx in &obj.elements.nodes {
                let Some(element) = checker.ctx.arena.get(element_idx) else {
                    continue;
                };

                let collect_function_spans =
                    |checker: &CheckerState<'a>,
                     function_idx: NodeIndex,
                     spans: &mut Vec<(u32, u32)>| {
                        if checker
                            .ctx
                            .implicit_any_contextual_closures
                            .contains(&function_idx)
                        {
                            return;
                        }
                        spans.extend(checker.function_like_param_spans_for_node(function_idx));
                    };

                match element.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        collect_function_spans(checker, element_idx, spans);
                    }
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = checker.ctx.arena.get_property_assignment(element) else {
                            continue;
                        };
                        let init_idx = prop.initializer;
                        if checker.is_callback_like_argument(init_idx) {
                            collect_function_spans(checker, init_idx, spans);
                        } else {
                            collect(checker, init_idx, spans);
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut spans = Vec::new();
        collect(self, arg_idx, &mut spans);
        spans
    }

    /// Nested calls/new expressions should infer from their own callee shapes during
    /// Round 1 generic inference. Applying the outer call's contextual parameter type
    /// at this stage can erase useful inference from the inner call.
    pub(crate) fn round1_should_skip_outer_contextual_type(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                _ => return false,
            }
        }
    }

    pub(crate) fn callback_body_span(&self, arg_idx: NodeIndex) -> Option<(u32, u32)> {
        self.callback_body_spans(arg_idx).into_iter().next()
    }

    pub(crate) fn callback_body_spans(&self, arg_idx: NodeIndex) -> Vec<(u32, u32)> {
        fn collect<'a>(checker: &CheckerState<'a>, idx: NodeIndex, spans: &mut Vec<(u32, u32)>) {
            if idx.is_none() {
                return;
            }
            let current = checker.ctx.arena.skip_parenthesized_and_assertions(idx);
            let Some(node) = checker.ctx.arena.get(current) else {
                return;
            };

            match node.kind {
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
                {
                    if let Some(func) = checker.ctx.arena.get_function(node)
                        && let Some(body_node) = checker.ctx.arena.get(func.body)
                    {
                        spans.push((body_node.pos, body_node.end));
                    }
                }
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = checker.ctx.arena.get_conditional_expr(node) {
                        collect(checker, cond.when_true, spans);
                        collect(checker, cond.when_false, spans);
                    }
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    if let Some(literal) = checker.ctx.arena.get_literal_expr(node) {
                        for &element_idx in &literal.elements.nodes {
                            collect(checker, element_idx, spans);
                        }
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                    if let Some(spread) = checker.ctx.arena.get_spread(node) {
                        collect(checker, spread.expression, spans);
                    }
                }
                _ => {}
            }
        }

        let mut spans = Vec::new();
        collect(self, arg_idx, &mut spans);
        spans
    }

    pub(crate) fn callback_function_param_spans(&self, arg_idx: NodeIndex) -> Vec<(u32, u32)> {
        self.callback_function_indices(arg_idx)
            .into_iter()
            .flat_map(|callback_idx| self.function_like_param_spans_for_node(callback_idx))
            .collect()
    }

    pub(crate) fn contextual_callback_function_indices(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<NodeIndex> {
        self.callback_function_indices(arg_idx)
            .into_iter()
            .filter(|callback_idx| {
                self.ctx
                    .implicit_any_contextual_closures
                    .contains(callback_idx)
            })
            .collect()
    }

    pub(crate) fn contextual_callback_function_param_spans(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        self.contextual_callback_function_indices(arg_idx)
            .into_iter()
            .flat_map(|callback_idx| self.function_like_param_spans_for_node(callback_idx))
            .collect()
    }

    /// Returns true when the callback argument has at least one explicitly-typed parameter
    /// whose type is incompatible with the corresponding expected parameter type.
    ///
    /// TSC reports TS2345 at the call site when the *parameter* types conflict
    /// (e.g. `(n: string) => n` where `(b: number) => number` is expected). In that case
    /// an inner TS2322 from the callback body should not suppress TS2345 --- the outer
    /// diagnostic is more informative. When only the return type differs (e.g.
    /// `(x: number) => ''` where `(x: number) => number` is expected), TSC still reports
    /// the inner TS2322.
    ///
    /// This uses the inferred `actual` type of the callback (not AST node resolution)
    /// to get reliable parameter types.
    pub(crate) fn callback_has_explicit_param_type_conflict(
        &mut self,
        arg_idx: NodeIndex,
        expected: tsz_solver::TypeId,
    ) -> bool {
        let Some(node) = self
            .callback_function_index(arg_idx)
            .and_then(|idx| self.ctx.arena.get(idx))
        else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        // Check if ANY parameter has an explicit type annotation.
        // If none do, this is a fully-contextual callback and we keep the inner error.
        let has_any_explicit_annotation = func.parameters.nodes.iter().any(|param_idx| {
            self.ctx
                .arena
                .get(*param_idx)
                .and_then(|n| self.ctx.arena.get_parameter(n))
                .is_some_and(|p| p.type_annotation.is_some())
        });
        if !has_any_explicit_annotation {
            return false;
        }

        // Resolve the expected function type to extract expected parameter types.
        let resolved_expected = self.evaluate_type_with_env(expected);
        let resolved_expected = self.resolve_type_for_property_access(resolved_expected);
        let resolved_expected = self.resolve_lazy_type(resolved_expected);
        let expected_params =
            call_checker::get_function_parameter_types(self.ctx.types, resolved_expected);
        if expected_params.is_empty() {
            return false;
        }

        // Get the inferred type of the callback to extract its parameter types.
        // Using the inferred type is more reliable than walking the AST param nodes.
        let actual_type = self.get_type_of_node(arg_idx);
        let resolved_actual = self.evaluate_type_with_env(actual_type);
        let resolved_actual = self.resolve_type_for_property_access(resolved_actual);
        let resolved_actual = self.resolve_lazy_type(resolved_actual);
        let actual_params =
            call_checker::get_function_parameter_types(self.ctx.types, resolved_actual);

        // For each explicitly-annotated parameter, check if its type conflicts with
        // the expected parameter type.
        func.parameters
            .nodes
            .iter()
            .enumerate()
            .any(|(i, param_idx)| {
                let has_annotation = self
                    .ctx
                    .arena
                    .get(*param_idx)
                    .and_then(|n| self.ctx.arena.get_parameter(n))
                    .is_some_and(|p| p.type_annotation.is_some());
                if !has_annotation {
                    return false;
                }
                let actual_param_type = actual_params
                    .get(i)
                    .copied()
                    .unwrap_or(tsz_solver::TypeId::UNKNOWN);
                let expected_param_type = expected_params.get(i).copied().unwrap_or_else(|| {
                    *expected_params
                        .last()
                        .expect("expected_params should not be empty")
                });
                // Parameter types conflict if the actual is NOT assignable to expected.
                !self.is_assignable_to(actual_param_type, expected_param_type)
            })
    }

    pub(crate) fn suppress_generic_return_context_for_arg(&self, idx: NodeIndex) -> bool {
        let Some(node) = self
            .callback_function_index(idx)
            .and_then(|callback_idx| self.ctx.arena.get(callback_idx))
        else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        // Do not suppress for generator functions: a generator with no parameters
        // has no parameter-based sensitivity. The outer contextual type should flow
        // into the generator's return type to correctly constrain generic inference.
        func.parameters.nodes.is_empty()
            && func.type_annotation.is_none()
            && !func.asterisk_token
            && is_contextually_sensitive(self, idx)
    }

    fn object_literal_contains_function_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };

        obj.elements.nodes.iter().any(|&element_idx| {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                return false;
            };
            if element.kind == syntax_kind_ext::METHOD_DECLARATION
                || element.kind == syntax_kind_ext::GET_ACCESSOR
                || element.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(element) else {
                return false;
            };
            self.is_callback_like_argument(prop.initializer)
        })
    }

    pub(crate) fn suppress_generic_return_context_for_direct_arg_overlap(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        args: &[NodeIndex],
        contextual_type: Option<TypeId>,
    ) -> bool {
        if contextual_type.is_none() {
            return false;
        }

        let return_type_params =
            self.collect_type_param_names_for_context_overlap(shape.return_type);

        if return_type_params.is_empty() {
            return false;
        }

        let return_is_bare_type_param = common::type_param_info(self.ctx.types, shape.return_type)
            .is_some_and(|info| return_type_params.contains(&info.name));
        let has_bare_return_param_argument = shape.params.iter().any(|param| {
            common::type_param_info(self.ctx.types, param.type_id)
                .is_some_and(|info| return_type_params.contains(&info.name))
        });
        if !return_is_bare_type_param
            && has_bare_return_param_argument
            && let Some(contextual_type) = contextual_type
        {
            let specializes = self.contextual_return_type_specializes_wrapped_params(
                shape.return_type,
                contextual_type,
                &return_type_params,
                &mut FxHashSet::default(),
            );
            if specializes
                && !self.wrapped_return_context_has_stable_overlap_arg(
                    shape,
                    args,
                    &return_type_params,
                )
            {
                return false;
            }
        }

        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                shape
                    .params
                    .last()
                    .and_then(|p| p.rest.then_some(p.type_id))
            }) else {
                break;
            };

            if self.expression_needs_contextual_signature_instantiation(arg_idx, Some(param_type)) {
                continue;
            }

            if self.ctx.arena.get(arg_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION
            }) && call_checker::get_contextual_signature(self.ctx.types, param_type).is_some()
            {
                continue;
            }

            // Skip function expressions: they are deferred arguments whose return
            // types contribute to inference in Round 2. Including them here would
            // incorrectly suppress the contextual return type even when the callback
            // is the sole inference source for the return type's type params.
            // Example: `invoke(() => 1)` where `invoke<T>(f: () => T): T` — the
            // callback `() => 1` references T in its return position, but T has no
            // other inference source; suppressing the contextual type loses the
            // literal preservation.
            if self.is_callback_like_argument(arg_idx) {
                continue;
            }

            if is_contextually_sensitive(self, arg_idx)
                || self.object_literal_contains_function_member(arg_idx)
            {
                continue;
            }

            if self.ctx.arena.get(arg_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && self
                        .ctx
                        .arena
                        .get_literal_expr(node)
                        .is_some_and(|literal| literal.elements.nodes.is_empty())
            }) {
                continue;
            }

            let param_type_params = self.collect_type_param_names_for_context_overlap(param_type);
            if param_type_params
                .iter()
                .any(|name| return_type_params.contains(name))
            {
                return true;
            }
        }

        false
    }

    fn wrapped_return_context_has_stable_overlap_arg(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        args: &[NodeIndex],
        return_type_params: &FxHashSet<Atom>,
    ) -> bool {
        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                shape
                    .params
                    .last()
                    .and_then(|p| p.rest.then_some(p.type_id))
            }) else {
                break;
            };

            let param_type_params = self.collect_type_param_names_for_context_overlap(param_type);
            if !param_type_params
                .iter()
                .any(|name| return_type_params.contains(name))
            {
                continue;
            }

            if self.argument_needs_contextual_type(arg_idx)
                || self
                    .expression_needs_contextual_signature_instantiation(arg_idx, Some(param_type))
                || self.object_literal_contains_function_member(arg_idx)
            {
                continue;
            }

            return true;
        }

        false
    }

    fn collect_type_param_names_for_context_overlap(&mut self, type_id: TypeId) -> FxHashSet<Atom> {
        let mut names = FxHashSet::default();
        self.extend_type_param_names(type_id, &mut names);

        let resolved = self.resolve_lazy_type(type_id);
        if resolved != type_id {
            self.extend_type_param_names(resolved, &mut names);
        }

        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != type_id && evaluated != resolved {
            self.extend_type_param_names(evaluated, &mut names);
        }

        let evaluated_resolved = self.evaluate_type_with_env(resolved);
        if evaluated_resolved != type_id
            && evaluated_resolved != resolved
            && evaluated_resolved != evaluated
        {
            self.extend_type_param_names(evaluated_resolved, &mut names);
        }

        names
    }

    fn extend_type_param_names(&self, type_id: TypeId, names: &mut FxHashSet<Atom>) {
        names.extend(
            common::collect_referenced_types(self.ctx.types, type_id)
                .into_iter()
                .filter_map(|ty| common::type_param_info(self.ctx.types, ty).map(|info| info.name)),
        );
    }

    fn contextual_signature_after_evaluation(
        &mut self,
        type_id: TypeId,
    ) -> Option<tsz_solver::FunctionShape> {
        if let Some(shape) = call_checker::get_contextual_signature(self.ctx.types, type_id) {
            return Some(shape);
        }

        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != type_id {
            if let Some(shape) = call_checker::get_contextual_signature(self.ctx.types, evaluated) {
                return Some(shape);
            }

            let evaluated_application = self.evaluate_application_type(evaluated);
            if evaluated_application != evaluated
                && let Some(shape) =
                    call_checker::get_contextual_signature(self.ctx.types, evaluated_application)
            {
                return Some(shape);
            }
        }

        let evaluated_application = self.evaluate_application_type(type_id);
        if evaluated_application != type_id {
            return call_checker::get_contextual_signature(self.ctx.types, evaluated_application);
        }

        None
    }

    pub(crate) fn contextual_return_type_specializes_wrapped_params(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::interner::Atom>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) -> bool {
        if matches!(target, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
            || !visited.insert((source, target))
        {
            return false;
        }

        if let Some(info) = common::type_param_info(self.ctx.types, source) {
            return tracked_type_params.contains(&info.name);
        }

        if self
            .awaited_application_args_in_type(source)
            .into_iter()
            .any(|awaited_arg| {
                common::collect_referenced_types(self.ctx.types, awaited_arg)
                    .into_iter()
                    .any(|referenced| {
                        common::type_param_info(self.ctx.types, referenced)
                            .is_some_and(|info| tracked_type_params.contains(&info.name))
                    })
            })
        {
            return true;
        }

        let awaited_source = self.evaluate_awaited_application_for_assignability(source);
        if awaited_source != source {
            return self.contextual_return_type_specializes_wrapped_params(
                awaited_source,
                target,
                tracked_type_params,
                visited,
            );
        }

        if let Some(inner) = common::unwrap_readonly_or_noinfer(self.ctx.types, source) {
            return self.contextual_return_type_specializes_wrapped_params(
                inner,
                target,
                tracked_type_params,
                visited,
            );
        }
        if let Some((base, args)) = common::application_info(self.ctx.types, source)
            && args.len() == 1
            && self
                .return_context_application_base_has_name(base, &["Readonly", "NoInfer", "Awaited"])
        {
            return self.contextual_return_type_specializes_wrapped_params(
                args[0],
                target,
                tracked_type_params,
                visited,
            );
        }
        if let Some((base, args)) = common::application_info(self.ctx.types, source)
            && args.len() == 1
            && common::application_info(self.ctx.types, target).is_none()
            && self.return_context_application_base_has_name(base, &["Promise", "PromiseLike"])
        {
            return self.contextual_return_type_specializes_wrapped_params(
                args[0],
                target,
                tracked_type_params,
                visited,
            );
        }
        let source_evaluated = self.evaluate_type_with_env(source);
        if source_evaluated != source
            && let Some((base, args)) = common::application_info(self.ctx.types, source_evaluated)
            && args.len() == 1
            && common::application_info(self.ctx.types, target).is_none()
            && self.return_context_application_base_has_name(base, &["Promise", "PromiseLike"])
        {
            return self.contextual_return_type_specializes_wrapped_params(
                args[0],
                target,
                tracked_type_params,
                visited,
            );
        }
        if source_evaluated != source
            && let Some(inner) =
                common::unwrap_readonly_or_noinfer(self.ctx.types, source_evaluated)
        {
            return self.contextual_return_type_specializes_wrapped_params(
                inner,
                target,
                tracked_type_params,
                visited,
            );
        }

        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            common::application_info(self.ctx.types, source),
            common::application_info(self.ctx.types, target),
        ) && source_base == target_base
            && source_args.len() == target_args.len()
        {
            return source_args
                .iter()
                .zip(target_args.iter())
                .any(|(&source_arg, &target_arg)| {
                    self.contextual_return_type_specializes_wrapped_params(
                        source_arg,
                        target_arg,
                        tracked_type_params,
                        visited,
                    )
                });
        }

        let source_signature = self.contextual_signature_after_evaluation(source);
        let target_signature = self.contextual_signature_after_evaluation(target);
        if let (Some(source_shape), Some(target_shape)) = (source_signature, target_signature)
            && source_shape.params.len() <= target_shape.params.len()
        {
            let params_specialize = source_shape
                .params
                .iter()
                .zip(target_shape.params.iter())
                .any(|(source_param, target_param)| {
                    self.contextual_return_type_specializes_wrapped_params(
                        source_param.type_id,
                        target_param.type_id,
                        tracked_type_params,
                        visited,
                    )
                });
            if params_specialize
                || self.contextual_return_type_specializes_wrapped_params(
                    source_shape.return_type,
                    target_shape.return_type,
                    tracked_type_params,
                    visited,
                )
            {
                return true;
            }
        }

        if let (Some(source_elem), Some(target_elem)) = (
            common::array_element_type(self.ctx.types, source),
            common::array_element_type(self.ctx.types, target),
        ) {
            return self.contextual_return_type_specializes_wrapped_params(
                source_elem,
                target_elem,
                tracked_type_params,
                visited,
            );
        }

        if let (Some(source_elems), Some(target_elems)) = (
            common::tuple_elements(self.ctx.types, source),
            common::tuple_elements(self.ctx.types, target),
        ) && source_elems.len() == target_elems.len()
        {
            return source_elems.iter().zip(target_elems.iter()).any(
                |(source_elem, target_elem)| {
                    self.contextual_return_type_specializes_wrapped_params(
                        source_elem.type_id,
                        target_elem.type_id,
                        tracked_type_params,
                        visited,
                    )
                },
            );
        }

        if let (Some(source_shape), Some(target_shape)) = (
            common::object_shape_for_type(self.ctx.types, source),
            common::object_shape_for_type(self.ctx.types, target),
        ) {
            for source_prop in &source_shape.properties {
                let Some(target_prop) = target_shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == source_prop.name)
                else {
                    continue;
                };
                if self.contextual_return_type_specializes_wrapped_params(
                    source_prop.type_id,
                    target_prop.type_id,
                    tracked_type_params,
                    visited,
                ) {
                    return true;
                }
            }
        }

        let source_eval = self.evaluate_application_type(source);
        let target_eval = self.evaluate_application_type(target);
        if source_eval != source || target_eval != target {
            return self.contextual_return_type_specializes_wrapped_params(
                source_eval,
                target_eval,
                tracked_type_params,
                visited,
            );
        }

        false
    }

    pub(crate) fn return_context_application_base_has_name(
        &self,
        base: TypeId,
        names: &[&str],
    ) -> bool {
        self.ctx
            .resolve_type_to_symbol_id(base)
            .or_else(|| {
                common::lazy_def_id(self.ctx.types, base)
                    .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
            })
            .or_else(|| {
                common::type_query_symbol(self.ctx.types, base)
                    .map(|symbol_ref| tsz_binder::SymbolId(symbol_ref.0))
            })
            .and_then(|symbol_id| self.ctx.binder.get_symbol(symbol_id))
            .is_some_and(|symbol| names.contains(&symbol.escaped_name.as_str()))
    }

    pub(crate) fn sensitive_callback_placeholder_should_skip_round1_inference(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
    ) -> bool {
        !self
            .sensitive_callback_return_only_type_params(callee_shape, callback_param_type)
            .is_empty()
    }

    pub(crate) fn sensitive_callback_return_only_type_params(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
    ) -> Vec<tsz_common::interner::Atom> {
        let tracked_type_params: FxHashSet<_> =
            callee_shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return Vec::new();
        }

        let callback_shape =
            call_checker::get_contextual_signature(self.ctx.types, callback_param_type)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                })
                .or_else(|| {
                    let evaluated = self.evaluate_application_type(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                });
        let Some(callback_shape) = callback_shape else {
            return Vec::new();
        };

        let return_mentions: FxHashSet<_> =
            common::collect_referenced_types(self.ctx.types, callback_shape.return_type)
                .into_iter()
                .filter_map(|ty| {
                    common::type_param_info(self.ctx.types, ty).and_then(|info| {
                        tracked_type_params
                            .contains(&info.name)
                            .then_some(info.name)
                    })
                })
                .collect();
        if return_mentions.is_empty() {
            return Vec::new();
        }

        let param_mentions: FxHashSet<_> = callback_shape
            .params
            .iter()
            .flat_map(|param| {
                common::collect_referenced_types(self.ctx.types, param.type_id).into_iter()
            })
            .filter_map(|ty| {
                common::type_param_info(self.ctx.types, ty).and_then(|info| {
                    tracked_type_params
                        .contains(&info.name)
                        .then_some(info.name)
                })
            })
            .collect();

        return_mentions
            .into_iter()
            .filter(|name| !param_mentions.contains(name))
            .collect()
    }

    pub(crate) fn sensitive_callback_nested_parameter_type_params(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
    ) -> Vec<tsz_common::interner::Atom> {
        let tracked_type_params: FxHashSet<_> =
            callee_shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return Vec::new();
        }

        let callback_shape =
            call_checker::get_contextual_signature(self.ctx.types, callback_param_type)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                })
                .or_else(|| {
                    let evaluated = self.evaluate_application_type(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                });
        let Some(callback_shape) = callback_shape else {
            return Vec::new();
        };

        let mut nested_mentions = FxHashSet::default();
        for param in &callback_shape.params {
            let nested_shape =
                call_checker::get_contextual_signature(self.ctx.types, param.type_id)
                    .or_else(|| {
                        let evaluated = self.evaluate_type_with_env(param.type_id);
                        (evaluated != param.type_id).then(|| {
                            call_checker::get_contextual_signature(self.ctx.types, evaluated)
                        })?
                    })
                    .or_else(|| {
                        let evaluated = self.evaluate_application_type(param.type_id);
                        (evaluated != param.type_id).then(|| {
                            call_checker::get_contextual_signature(self.ctx.types, evaluated)
                        })?
                    });
            let Some(nested_shape) = nested_shape else {
                continue;
            };

            nested_mentions.extend(
                nested_shape
                    .params
                    .iter()
                    .flat_map(|nested_param| {
                        common::collect_referenced_types(self.ctx.types, nested_param.type_id)
                            .into_iter()
                    })
                    .filter_map(|ty| {
                        common::type_param_info(self.ctx.types, ty).and_then(|info| {
                            tracked_type_params
                                .contains(&info.name)
                                .then_some(info.name)
                        })
                    }),
            );
        }

        nested_mentions.into_iter().collect()
    }

    pub(crate) fn function_like_return_parameter_type_params(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
    ) -> Vec<tsz_common::interner::Atom> {
        let tracked_type_params: FxHashSet<_> =
            callee_shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return Vec::new();
        }

        let return_shape =
            call_checker::get_contextual_signature(self.ctx.types, callee_shape.return_type)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(callee_shape.return_type);
                    (evaluated != callee_shape.return_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                })
                .or_else(|| {
                    let evaluated = self.evaluate_application_type(callee_shape.return_type);
                    (evaluated != callee_shape.return_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                });
        let Some(return_shape) = return_shape else {
            return Vec::new();
        };

        return_shape
            .params
            .iter()
            .flat_map(|param| common::collect_referenced_types(self.ctx.types, param.type_id))
            .filter_map(|ty| {
                common::type_param_info(self.ctx.types, ty).and_then(|info| {
                    tracked_type_params
                        .contains(&info.name)
                        .then_some(info.name)
                })
            })
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect()
    }

    pub(crate) fn should_strip_sensitive_placeholder_substitution(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
        type_param_name: tsz_common::interner::Atom,
        inferred: TypeId,
    ) -> bool {
        if !self
            .sensitive_callback_return_only_type_params(callee_shape, callback_param_type)
            .into_iter()
            .any(|name| name == type_param_name)
        {
            return false;
        }

        inferred == TypeId::ANY
            || inferred == TypeId::UNKNOWN
            || inferred == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, inferred)
    }

    pub(crate) fn suppress_initializer_contextual_type_for_generic_call(
        &mut self,
        idx: NodeIndex,
        contextual_type: TypeId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return false;
        }

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };

        let mut callee_idx = call.expression;
        while let Some(callee_node) = self.ctx.arena.get(callee_idx) {
            if callee_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(callee_node)
            {
                callee_idx = paren.expression;
                continue;
            }
            break;
        }
        if self.is_callback_like_argument(callee_idx) {
            return false;
        }

        if call.type_arguments.is_some() {
            return false;
        }

        let callee_type = self.get_type_of_node(call.expression);
        if matches!(
            callee_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return false;
        }

        let callee_type_for_context = self.evaluate_application_type(callee_type);
        let callee_type_for_context = self.resolve_lazy_type(callee_type_for_context);
        let args = match call.arguments.as_ref() {
            Some(args) => args.nodes.as_slice(),
            None => &[],
        };
        let Some(shape) =
            crate::query_boundaries::checkers::call::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_context,
                args.len(),
            )
        else {
            return false;
        };

        if crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            callee_type_for_context,
        )
        .is_some_and(|callable| callable.call_signatures.len() > 1)
        {
            return false;
        }

        if shape.type_params.is_empty() {
            return false;
        }
        // When the return type is a bare type parameter that also appears in
        // parameter position, suppressing the contextual type loses the upper
        // bound that prevents literal widening. For `identity<T>(x: T): T`
        // called as `let v: DooDad = identity('ELSE')`, the contextual type
        // `DooDad` is needed so the solver preserves `"ELSE"` instead of
        // widening to `string`. The solver's `return_type_bare_var` logic
        // already handles priority correctly — it only adds the contextual type
        // as a candidate when no direct argument candidates exist.
        let return_is_bare_type_param = shape.type_params.iter().any(|tp| {
            crate::query_boundaries::type_computation::complex::type_parameter_info(
                self.ctx.types,
                shape.return_type,
            )
            .is_some_and(|info| info.name == tp.name)
        });
        if return_is_bare_type_param {
            return false;
        }
        self.suppress_generic_return_context_for_direct_arg_overlap(
            &shape,
            args,
            Some(contextual_type),
        )
    }

    /// Whether a node is a direct literal expression (numeric/string/boolean/bigint/null
    /// keyword or no-substitution template). Used by `satisfies` handling to avoid
    /// widening leaf literals via contextual typing, matching tsc's
    /// `checkSatisfiesExpressionWorker` which returns fresh literal types from
    /// `checkNumericLiteral`/`checkStringLiteral` etc. regardless of contextual type.
    pub(crate) fn is_direct_literal_expression(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Whether an argument node needs contextual typing from the callee signature.
    ///
    /// Literal expressions need contextual typing to preserve literal types when
    /// the expected parameter type is a literal union (e.g., `"A"` should remain
    /// `"A"` when passed to a parameter of type `"A" | "B"`).
    ///
    /// Other expressions like arrow functions, object literals, etc. also need
    /// contextual typing for their internal structure.
    pub(crate) fn argument_needs_contextual_type(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Literal expressions need contextual typing to preserve literal types
        // when the expected type is a literal union or specific literal type.
        let is_literal = matches!(
            node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        );

        if is_literal {
            return true;
        }

        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                is_contextually_sensitive(self, idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                true
            }
            // Parenthesized expressions: recurse into inner expression.
            // A parenthesized identifier like `(identity)` should NOT get contextual
            // typing — identifiers have fixed declared types. Only parenthesized
            // context-sensitive expressions (arrows, object literals, etc.) need it.
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.argument_needs_contextual_type(paren.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(crate) fn explicit_generic_function_has_fully_annotated_signature(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        if func
            .type_parameters
            .as_ref()
            .is_none_or(|params| params.nodes.is_empty())
            || func.type_annotation.is_none()
        {
            return false;
        }
        func.parameters.nodes.iter().all(|param_idx| {
            self.ctx
                .arena
                .get(*param_idx)
                .and_then(|param| self.ctx.arena.get_parameter(param))
                .is_some_and(|param| param.type_annotation.is_some())
        })
    }

    pub(crate) fn expression_needs_contextual_signature_instantiation(
        &mut self,
        idx: NodeIndex,
        expected_type: Option<TypeId>,
    ) -> bool {
        let Some(expected_type) = expected_type else {
            return false;
        };

        let expected_type = self
            .contextual_type_option_for_expression(Some(expected_type))
            .unwrap_or(expected_type);
        let expected_eval = self.evaluate_type_with_env(expected_type);
        let expected_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected_type,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected_eval,
            )
        });
        if expected_shape.is_none() {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {}
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if self
                    .ctx
                    .arena
                    .get_function(node)
                    .and_then(|func| func.type_parameters.as_ref())
                    .is_none_or(|params| params.nodes.is_empty())
                {
                    return false;
                }
            }
            _ => return false,
        }

        let source_type = self.get_type_of_node(expr_idx);
        let source_eval = self.evaluate_type_with_env(source_type);
        crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            source_type,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                source_eval,
            )
        })
        .is_some_and(|shape| !shape.type_params.is_empty())
    }

    fn contextual_callable_has_parameter_return_feedback(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> bool {
        let return_type = shape.return_type;
        if matches!(return_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
            || common::contains_infer_types(self.ctx.types, return_type)
        {
            return false;
        }

        let type_param_names = |this: &Self, type_id: TypeId| -> FxHashSet<_> {
            common::collect_referenced_types(this.ctx.types, type_id)
                .into_iter()
                .filter_map(|referenced| {
                    common::type_param_info(this.ctx.types, referenced).map(|info| info.name)
                })
                .collect()
        };

        let return_params = type_param_names(self, return_type);
        if return_params.is_empty() {
            return false;
        }
        if shape.params.is_empty() {
            return true;
        }

        let param_params: FxHashSet<_> = shape
            .params
            .iter()
            .flat_map(|param| type_param_names(self, param.type_id).into_iter())
            .collect();
        return_params.iter().any(|name| param_params.contains(name))
    }

    pub(crate) fn call_expression_needs_contextual_generic_instantiation(
        &mut self,
        idx: NodeIndex,
        expected_type: Option<TypeId>,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return false;
        }

        let Some(expected_type) = expected_type else {
            return false;
        };
        if matches!(expected_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
            || common::contains_infer_types(self.ctx.types, expected_type)
        {
            return false;
        }

        let expected_type = self
            .contextual_type_option_for_expression(Some(expected_type))
            .unwrap_or(expected_type);
        let Some(expected_shape) = self.contextual_signature_after_evaluation(expected_type) else {
            return false;
        };
        if !self.contextual_callable_has_parameter_return_feedback(&expected_shape) {
            return false;
        }

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let arg_count = call
            .arguments
            .as_ref()
            .map(|args| args.nodes.len())
            .unwrap_or(0);
        let callee_type = self.get_type_of_node_with_request(call.expression, &TypingRequest::NONE);
        let callee_type = self.evaluate_application_type(callee_type);
        let callee_type = self.resolve_lazy_type(callee_type);
        let callee_type = self.evaluate_contextual_type(callee_type);

        call_checker::get_contextual_signature_for_arity(self.ctx.types, callee_type, arg_count)
            .or_else(|| call_checker::get_call_signature(self.ctx.types, callee_type, arg_count))
            .is_some_and(|shape| !shape.type_params.is_empty())
    }

    pub(crate) fn argument_needs_refresh_for_contextual_call(
        &mut self,
        idx: NodeIndex,
        expected_type: Option<TypeId>,
    ) -> bool {
        self.argument_needs_contextual_type(idx)
            || self.expression_needs_contextual_signature_instantiation(idx, expected_type)
            || self.call_expression_needs_contextual_generic_instantiation(idx, expected_type)
    }

    pub(crate) fn instantiate_callable_result_from_request(
        &mut self,
        idx: NodeIndex,
        result_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(expected_type) = request.contextual_type else {
            return result_type;
        };
        if matches!(result_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return result_type;
        }

        let result_eval = self.evaluate_type_with_env(result_type);
        let has_generic_signature =
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                result_type,
            )
            .or_else(|| {
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    result_eval,
                )
            })
            .is_some_and(|shape| !shape.type_params.is_empty());
        if !has_generic_signature {
            return result_type;
        }

        if self.is_immediate_call_or_new_callee(idx) || !self.is_immediate_call_or_new_argument(idx)
        {
            return result_type;
        }

        let expected_type = self.contextual_type_option_for_expression(Some(expected_type));
        let Some(expected_type) = expected_type else {
            return result_type;
        };

        let instantiated =
            if self.target_has_concrete_return_context_for_generic_refinement(expected_type) {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    result_type,
                    expected_type,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(
                    result_type,
                    expected_type,
                )
            };

        if instantiated == TypeId::ERROR {
            result_type
        } else {
            instantiated
        }
    }
}
