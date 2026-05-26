//! Contextual literal types and circular reference detection.

use crate::call_checker::CallableContext;
use crate::class_inheritance::ClassInheritanceChecker;
use crate::query_boundaries::common::{
    self as common, ContextualLiteralAllowKind, TypeTraversalKind, are_same_base_literal_kind,
    classify_for_contextual_literal, classify_for_traversal, contains_type_parameters,
    index_access_types, is_conditional_type, is_evaluable_meta_type, is_index_access_type,
    is_this_type, keyof_inner_type, lazy_def_id, type_application, type_parameter_constraint,
    union_members,
};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::computation::ContextualTypeContext;

impl<'a> CheckerState<'a> {
    pub(crate) fn raw_contextual_signature_available(&self, type_id: TypeId) -> bool {
        let helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            type_id,
            self.ctx.compiler_options.no_implicit_any,
        );
        helper.get_this_type_from_marker().is_some()
            || helper.get_this_type().is_some()
            || helper.get_return_type().is_some()
            || helper.get_parameter_type(0).is_some()
            || helper.get_rest_parameter_type(0).is_some()
            // Preserve callable aliases whose raw signature can be extracted even
            // when ContextualTypeContext is obscured by return-position queries
            // like `typeof x`. Evaluating these too early erases callback param context.
            || crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                type_id,
            )
            .is_some()
    }

    pub(crate) fn contextual_type_for_expression(&mut self, type_id: TypeId) -> TypeId {
        // Evaluate union members individually without subtype reduction to
        // preserve literal types (e.g., avoid `string | 'done'` → `string`).
        if let Some(members) = union_members(self.ctx.types, type_id) {
            let mut evaluated_members = Vec::with_capacity(members.len());
            let mut any_changed = false;
            for &member in &members {
                let evaluated = self.contextual_type_for_expression(member);
                if evaluated != member {
                    any_changed = true;
                }
                evaluated_members.push(evaluated);
            }
            if any_changed {
                return self
                    .ctx
                    .types
                    .factory()
                    .union_preserve_members(evaluated_members);
            }
            return type_id;
        }

        // Resolve TypeQuery (typeof) types to their concrete types before
        // contextual typing. Without this, `var x: typeof F = (a) => ...`
        // leaves the contextual type as an unresolved TypeQuery, which the
        // solver's ContextualTypeContext cannot extract parameter types from,
        // causing false TS7006 ("Parameter implicitly has 'any' type").
        {
            use crate::query_boundaries::type_checking_utilities::{
                TypeQueryKind, classify_type_query,
            };
            if matches!(
                classify_type_query(self.ctx.types, type_id),
                TypeQueryKind::TypeQuery(_) | TypeQueryKind::ApplicationWithTypeQuery { .. }
            ) {
                let resolved = self.resolve_type_query_type(type_id);
                if resolved != type_id && resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    return self.contextual_type_for_expression(resolved);
                }
            }
        }

        // Don't resolve type parameters to their constraints here.
        // In tsc, the contextual type preserves the type parameter (e.g., `T`),
        // and constraint resolution happens deeper in the system when structural
        // information is needed. Resolving early causes compute_return_context_substitution
        // to map inner generic type params to the constraint instead of the outer type
        // parameter, destroying inference placeholders (e.g., `new Proxy(obj, handler)`
        // returning `object` instead of `T`).
        if type_parameter_constraint(self.ctx.types, type_id).is_some() {
            return type_id;
        }

        // Preserve deferred indexed-access contextual types like `T[K]` and
        // `Type[K]`. Evaluating them here collapses the generic key space to a
        // union of property types, which suppresses TS2322 on return/assignment
        // sites that should still be checked against the unresolved indexed access.
        if is_index_access_type(self.ctx.types, type_id)
            && contains_type_parameters(self.ctx.types, type_id)
        {
            return type_id;
        }

        // Preserve direct callable shapes as contextual types. Re-evaluating them
        // can simplify contravariant parameter unions inside callback types, e.g.
        // `(value: A | B | C) => U` collapsing to `(value: A) => any` during
        // generic call argument collection for `Array.prototype.map`.
        {
            let db = self.ctx.types.as_type_database();
            if crate::query_boundaries::common::is_function_type(db, type_id)
                || crate::query_boundaries::common::callable_shape_id(db, type_id).is_some()
            {
                return type_id;
            }
        }

        if crate::query_boundaries::state::should_evaluate_contextual_declared_type(
            self.ctx.types,
            type_id,
        ) {
            self.evaluate_type_with_env(type_id)
        } else {
            type_id
        }
    }

    pub(crate) fn contextual_type_option_for_expression(
        &mut self,
        type_id: Option<TypeId>,
    ) -> Option<TypeId> {
        type_id.map(|type_id| self.contextual_type_for_expression(type_id))
    }

    pub(crate) fn contextual_type_option_for_call_argument(
        &mut self,
        type_id: Option<TypeId>,
        arg_idx: NodeIndex,
        callable_ctx: CallableContext,
    ) -> Option<TypeId> {
        self.contextual_type_option_for_call_argument_at(type_id, arg_idx, None, None, callable_ctx)
    }

    pub(crate) fn contextual_type_option_for_call_argument_at(
        &mut self,
        type_id: Option<TypeId>,
        arg_idx: NodeIndex,
        arg_index: Option<usize>,
        arg_count: Option<usize>,
        callable_ctx: CallableContext,
    ) -> Option<TypeId> {
        let type_id = type_id?;
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return Some(self.contextual_type_for_expression(type_id));
        };

        let preserve_raw = matches!(
            arg_node.kind,
            k if k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
        );
        let preserve_raw_object_context = matches!(
            arg_node.kind,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) && (contains_type_parameters(self.ctx.types, type_id)
            || crate::computation::call_inference::should_preserve_contextual_application_shape(
                self.ctx.types,
                type_id,
            ));
        let needs_resolved_callable_context = common::type_param_info(self.ctx.types, type_id)
            .is_some()
            || index_access_types(self.ctx.types, type_id).is_some()
            || is_conditional_type(self.ctx.types, type_id)
            || type_application(self.ctx.types, type_id).is_some();

        // Outer wrappers should not block raw contextual typing for inner callbacks.
        if arg_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(arg_node)
        {
            return self.contextual_type_option_for_call_argument_at(
                Some(type_id),
                paren.expression,
                arg_index,
                arg_count,
                callable_ctx,
            );
        }
        if (arg_node.kind == syntax_kind_ext::AS_EXPRESSION
            || arg_node.kind == syntax_kind_ext::TYPE_ASSERTION
            || arg_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.ctx.arena.get_type_assertion(arg_node)
        {
            return self.contextual_type_option_for_call_argument_at(
                Some(type_id),
                assertion.expression,
                arg_index,
                arg_count,
                callable_ctx,
            );
        }
        if arg_node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.ctx.arena.get_unary_expr_ex(arg_node)
        {
            return self.contextual_type_option_for_call_argument_at(
                Some(type_id),
                unary.expression,
                arg_index,
                arg_count,
                callable_ctx,
            );
        }

        if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && arg_index.is_some()
            && arg_count.is_some()
            && self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                arg_index.is_some_and(|index| index < skip.len() && skip[index])
            })
            && let Some(arg_index) = arg_index
            && let Some(callable_type) = callable_ctx.callable_type
            && let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                callable_type,
            )
            && let Some(raw_contextual_type) = shape
                .params
                .get(arg_index)
                .or_else(|| shape.params.last().filter(|param| param.rest))
                .map(|param| param.type_id)
        {
            return Some(raw_contextual_type);
        }

        if preserve_raw_object_context
            || (preserve_raw
                && (!needs_resolved_callable_context
                    || matches!(
                        arg_node.kind,
                        k if k == syntax_kind_ext::ARROW_FUNCTION
                            || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    ))
                && self.raw_contextual_signature_available(type_id))
        {
            Some(type_id)
        } else {
            Some(self.contextual_type_for_expression(type_id))
        }
    }

    pub(crate) fn contextual_type_allows_literal(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    pub(crate) fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if ctx_type == literal_type {
            return true;
        }
        // tsc: literal contextual type allows ALL literals of the same base type.
        if are_same_base_literal_kind(self.ctx.types, ctx_type, literal_type) {
            return true;
        }
        // tsz's BOOLEAN is intrinsic (not true|false union), so explicit check needed.
        if is_boolean_context_for_boolean_literal(ctx_type, literal_type) {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        // Resolve Lazy(DefId) types before classification.
        if let Some(def_id) = lazy_def_id(self.ctx.types, ctx_type) {
            // Try type_env first
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            // If not resolved, use centralized relation precondition setup to populate type_env.
            self.ensure_relation_input_ready(ctx_type);
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            return false;
        }

        // Evaluate KeyOf/IndexAccess to concrete form before classification.
        if is_evaluable_meta_type(self.ctx.types, ctx_type) {
            let evaluated = self.evaluate_type_with_env(ctx_type);
            if evaluated != ctx_type && evaluated != TypeId::ERROR {
                return self.contextual_type_allows_literal_inner(evaluated, literal_type, visited);
            }
            // Fallback: when `evaluate_type_with_env` could not make progress
            // on a `keyof Lazy(LibType)` because the lib def has not been
            // registered in `TypeEnvironment` yet, force a stronger Lazy
            // resolution and retry the evaluation. Without this, fresh
            // string literals like `'currency'` get widened to `string`
            // even though the keyof target accepts the literal.
            if let Some(keyof_inner) = keyof_inner_type(self.ctx.types, ctx_type)
                && lazy_def_id(self.ctx.types, keyof_inner).is_some()
            {
                self.ensure_relation_input_ready(keyof_inner);
                let resolved_inner = self.evaluate_type_with_env(keyof_inner);
                if resolved_inner != keyof_inner {
                    let new_keyof = self.ctx.types.factory().keyof(resolved_inner);
                    let evaluated2 = self.evaluate_type_with_env(new_keyof);
                    if evaluated2 != new_keyof && evaluated2 != TypeId::ERROR {
                        return self.contextual_type_allows_literal_inner(
                            evaluated2,
                            literal_type,
                            visited,
                        );
                    }
                }
            }
        }
        // IndexAccess fallback: when `evaluate_type_with_env` could not make
        // progress on `Object[Key]` (typically because `Object` is a `Lazy`
        // ref to a lib-namespace interface like `Intl.NumberFormatOptions`
        // whose def has not been registered in `TypeEnvironment` yet), look
        // up the property type directly via the contextual property API.
        // Without this, fresh literals like `'currency'` get widened to
        // `string` even though the indexed-access target accepts the literal.
        if let Some((object_type, index_type)) = index_access_types(self.ctx.types, ctx_type)
            && let Some(prop_name_atom) = common::string_literal_value(self.ctx.types, index_type)
        {
            self.ensure_relation_input_ready(object_type);
            let lookup_object = self.evaluate_type_with_env(object_type);
            let prop_name = self.ctx.types.resolve_atom(prop_name_atom);
            if let Some(prop_type) = self
                .ctx
                .types
                .contextual_property_type(lookup_object, &prop_name)
                && visited.insert(prop_type)
            {
                return self.contextual_type_allows_literal_inner(prop_type, literal_type, visited);
            }
        }
        // Generic `keyof` contexts preserve literal arguments.
        if let Some(keyof_inner) = keyof_inner_type(self.ctx.types, ctx_type)
            && (common::type_param_info(self.ctx.types, keyof_inner).is_some()
                || is_this_type(self.ctx.types, keyof_inner))
        {
            return true;
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            // Type parameters always allow literal types.
            ContextualLiteralAllowKind::TypeParameter { .. }
            | ContextualLiteralAllowKind::TemplateLiteral => true,
            ContextualLiteralAllowKind::Application => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Mapped => {
                let expanded = self.evaluate_type_with_env(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            // Deferred conditional: check both branches. Matches tsc's
            // `isLiteralOfContextualType` recursing through the conditional's
            // default constraint (approximately `true_type | false_type`).
            // Reached when earlier evaluation couldn't resolve the conditional.
            ContextualLiteralAllowKind::Conditional {
                true_type,
                false_type,
            } => {
                self.contextual_type_allows_literal_inner(true_type, literal_type, visited)
                    || self.contextual_type_allows_literal_inner(false_type, literal_type, visited)
            }
            ContextualLiteralAllowKind::NotAllowed => false,
        }
    }

    /// Report TS2310 for a class/interface whose instantiated base type needs
    /// the current symbol's structure to resolve.
    pub(crate) fn report_recursive_base_type_for_symbol(&mut self, sym_id: SymbolId) {
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        checker.error_recursive_base_type_for_symbol(sym_id);
    }

    /// True when resolving `type_id` requires `target_sym`'s structure (meta-type ops).
    pub(crate) fn type_requires_structure_of_symbol(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
    ) -> bool {
        use tsz_solver::recursion::RecursionProfile;
        let mut guard =
            tsz_solver::recursion::RecursionGuard::with_profile(RecursionProfile::ShallowTraversal);
        self.type_requires_structure_of_symbol_inner(type_id, target_sym, false, false, &mut guard)
    }

    /// Like above but skips member types — for TS2310 base-type cycle detection.
    pub(crate) fn type_requires_structure_of_symbol_for_base_type(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
    ) -> bool {
        use tsz_solver::recursion::RecursionProfile;
        let mut guard =
            tsz_solver::recursion::RecursionGuard::with_profile(RecursionProfile::ShallowTraversal);
        self.type_requires_structure_of_symbol_inner(type_id, target_sym, false, true, &mut guard)
    }

    fn type_requires_structure_of_symbol_inner(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
        requires_structure: bool,
        skip_members: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<(TypeId, bool)>,
    ) -> bool {
        // Fast path: intrinsic kinds (any / unknown / never / void / null /
        // undefined plus the reserved PrimitiveX kinds) cannot reference any
        // user-declared symbol. The body below would walk through them and
        // return `false` after multiple `lazy_def_id` / `def_to_symbol_id`
        // probes; skip the guard round-trip and the body call entirely.
        // is_intrinsic() is a free TypeId-range check (no TypeData lookup).
        if type_id.is_intrinsic() {
            return false;
        }
        let key = (type_id, requires_structure);
        if !guard.enter(key).is_entered() {
            return false;
        }
        let result = self.type_requires_structure_of_symbol_body(
            type_id,
            target_sym,
            requires_structure,
            skip_members,
            guard,
        );
        guard.leave(key);
        result
    }

    fn type_requires_structure_of_symbol_body(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
        requires_structure: bool,
        skip_members: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<(TypeId, bool)>,
    ) -> bool {
        if let Some(def_id) = lazy_def_id(self.ctx.types, type_id)
            && self.ctx.def_to_symbol_id_with_fallback(def_id) == Some(target_sym)
        {
            return requires_structure;
        }
        if let Some(def_id) = lazy_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
        {
            let mut resolved_alias = self
                .resolve_and_insert_def_type(def_id)
                .or_else(|| Some(self.get_type_of_symbol(sym_id)))
                .unwrap_or(type_id);
            if resolved_alias == type_id
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                for &decl_idx in &symbol.declarations {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                        continue;
                    }
                    let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                        continue;
                    };
                    let (_params, updates) = self.push_type_parameters(&alias.type_parameters);
                    resolved_alias = self.get_type_from_type_node(alias.type_node);
                    self.pop_type_parameters(updates);
                    break;
                }
            }
            if resolved_alias != type_id
                && self.type_requires_structure_of_symbol_inner(
                    resolved_alias,
                    target_sym,
                    requires_structure,
                    skip_members,
                    guard,
                )
            {
                return true;
            }
        }
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
            && let Some(body) = self.ctx.definition_store.get_body(def_id)
            && body != type_id
            && self.type_requires_structure_of_symbol_inner(
                body,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }
        if requires_structure
            && self
                .ctx
                .definition_store
                .find_def_for_type(type_id)
                .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                == Some(target_sym)
        {
            return true;
        }

        let resolved_lazy = self.resolve_lazy_type(type_id);
        if resolved_lazy != type_id
            && self.type_requires_structure_of_symbol_inner(
                resolved_lazy,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }

        // Performance guard: skip expensive evaluation after 200 unique type visits.
        // Deep generic/mapped-type chains (styled-components) cause hangs otherwise.
        let evaluated = if guard.iterations() <= 200 {
            match classify_for_traversal(self.ctx.types, type_id) {
                TypeTraversalKind::Application { .. }
                | TypeTraversalKind::Conditional(_)
                | TypeTraversalKind::Mapped(_)
                | TypeTraversalKind::IndexAccess { .. }
                | TypeTraversalKind::KeyOf(_) => self.evaluate_type_with_resolution(type_id),
                _ => type_id,
            }
        } else {
            type_id
        };
        if evaluated != type_id
            && self.type_requires_structure_of_symbol_inner(
                evaluated,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }

        match classify_for_traversal(self.ctx.types, type_id) {
            TypeTraversalKind::Terminal
            | TypeTraversalKind::Lazy(_)
            | TypeTraversalKind::TypeQuery(_)
            | TypeTraversalKind::SymbolRef(_) => false,
            TypeTraversalKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if requires_structure && shape.symbol == Some(target_sym) {
                    return true;
                }
                // skip_members: don't walk into properties (TS2310 base-type check)
                if skip_members {
                    return false;
                }
                shape.properties.iter().any(|prop| {
                    self.type_requires_structure_of_symbol_inner(
                        prop.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        prop.write_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.string_index.as_ref().is_some_and(|sig| {
                    self.type_requires_structure_of_symbol_inner(
                        sig.key_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        sig.value_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.number_index.as_ref().is_some_and(|sig| {
                    self.type_requires_structure_of_symbol_inner(
                        sig.key_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        sig.value_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Members(members) => members.into_iter().any(|member| {
                self.type_requires_structure_of_symbol_inner(
                    member,
                    target_sym,
                    requires_structure,
                    skip_members,
                    guard,
                )
            }),
            TypeTraversalKind::Array(elem) => self.type_requires_structure_of_symbol_inner(
                elem,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            ),
            TypeTraversalKind::Tuple(list_id) => {
                self.ctx.types.tuple_list(list_id).iter().any(|elem| {
                    self.type_requires_structure_of_symbol_inner(
                        elem.type_id,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Function(shape_id) => {
                if skip_members {
                    return false;
                }
                let shape = self.ctx.types.function_shape(shape_id);
                shape.params.iter().any(|param| {
                    self.type_requires_structure_of_symbol_inner(
                        param.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || self.type_requires_structure_of_symbol_inner(
                    shape.return_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || shape.this_type.is_some_and(|this_type| {
                    self.type_requires_structure_of_symbol_inner(
                        this_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Callable(shape_id) => {
                if skip_members {
                    return false;
                }
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.call_signatures.iter().any(|sig| {
                    sig.params.iter().any(|param| {
                        self.type_requires_structure_of_symbol_inner(
                            param.type_id,
                            target_sym,
                            false,
                            skip_members,
                            guard,
                        )
                    }) || self.type_requires_structure_of_symbol_inner(
                        sig.return_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.construct_signatures.iter().any(|sig| {
                    sig.params.iter().any(|param| {
                        self.type_requires_structure_of_symbol_inner(
                            param.type_id,
                            target_sym,
                            false,
                            skip_members,
                            guard,
                        )
                    }) || self.type_requires_structure_of_symbol_inner(
                        sig.return_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.properties.iter().any(|prop| {
                    self.type_requires_structure_of_symbol_inner(
                        prop.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::TypeParameter {
                constraint,
                default,
            } => {
                constraint.is_some_and(|constraint| {
                    self.type_requires_structure_of_symbol_inner(
                        constraint,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                }) || default.is_some_and(|default| {
                    self.type_requires_structure_of_symbol_inner(
                        default,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Application { base, args, .. } => {
                self.type_requires_structure_of_symbol_inner(
                    base,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || args.iter().any(|&arg| {
                    self.type_requires_structure_of_symbol_inner(
                        arg,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.type_requires_structure_of_symbol_inner(
                    cond.check_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.extends_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.true_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.false_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                )
            }
            TypeTraversalKind::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_requires_structure_of_symbol_inner(
                        constraint,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_requires_structure_of_symbol_inner(
                        default,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                }) || self.type_requires_structure_of_symbol_inner(
                    mapped.constraint,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    mapped.template,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || mapped.name_type.is_some_and(|name_type| {
                    self.type_requires_structure_of_symbol_inner(
                        name_type,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::IndexAccess {
                object: object_type,
                index: index_type,
            } => {
                self.type_requires_structure_of_symbol_inner(
                    object_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    index_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                )
            }
            TypeTraversalKind::TemplateLiteral(types) => types.into_iter().any(|type_id| {
                self.type_requires_structure_of_symbol_inner(
                    type_id,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                )
            }),
            TypeTraversalKind::KeyOf(inner) | TypeTraversalKind::Readonly(inner) => self
                .type_requires_structure_of_symbol_inner(
                    inner,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ),
            TypeTraversalKind::StringIntrinsic(type_arg) => self
                .type_requires_structure_of_symbol_inner(
                    type_arg,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ),
        }
    }

    /// Report TS2313 for mapped constraints in instantiated type alias cycles.
    pub(crate) fn report_instantiated_type_alias_mapped_constraint_cycles(
        &mut self,
        alias_sym_id: SymbolId,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args: &[TypeId],
        current_sym: SymbolId,
    ) {
        let Some(symbol) = self.get_symbol_globally(alias_sym_id) else {
            return;
        };
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return;
        }

        let declarations = symbol.declarations.clone();
        let bindings: FxHashMap<String, TypeId> = type_params
            .iter()
            .zip(type_args.iter().copied())
            .map(|(param, arg)| (self.ctx.types.resolve_atom(param.name), arg))
            .collect();
        let updates: Vec<(String, Option<TypeId>)> = bindings
            .iter()
            .map(|(name, &arg)| {
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), arg);
                (name.clone(), previous)
            })
            .collect();

        for &decl_idx in &declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(alias) = self.ctx.arena.get_type_alias(node) else {
                continue;
            };
            self.report_instantiated_mapped_constraint_cycles_in_node(
                alias.type_node,
                current_sym,
                &bindings,
            );
        }

        for (name, previous) in updates.into_iter().rev() {
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            }
        }
    }

    fn report_instantiated_mapped_constraint_cycles_in_node(
        &mut self,
        node_idx: NodeIndex,
        current_sym: SymbolId,
        bindings: &FxHashMap<String, TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MAPPED_TYPE {
            let Some(mapped) = self.ctx.arena.get_mapped_type(node) else {
                return;
            };
            let Some(type_param_node) = self.ctx.arena.get(mapped.type_parameter) else {
                return;
            };
            let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node) else {
                return;
            };

            let mut local_update = None;
            if let Some(name_node) = self.ctx.arena.get(type_param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let atom = self.ctx.types.intern_string(&name);
                let provisional = self
                    .ctx
                    .types
                    .factory()
                    .type_param(tsz_solver::TypeParamInfo {
                        name: atom,
                        constraint: None,
                        default: None,
                        is_const: false,
                    });
                let previous = self
                    .ctx
                    .type_parameter_scope
                    .insert(name.clone(), provisional);
                local_update = Some((name, previous));
            }

            if type_param.constraint != NodeIndex::NONE {
                let constraint_type = self.get_type_from_type_node(type_param.constraint);
                let syntactic_cycle = self.constraint_mentions_instantiated_cycle_target(
                    type_param.constraint,
                    current_sym,
                    bindings,
                    &mut FxHashSet::default(),
                );
                if (self.type_requires_structure_of_symbol(constraint_type, current_sym)
                    || syntactic_cycle)
                    && let Some(name_node) = self.ctx.arena.get(type_param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    self.error_at_node_msg(
                        type_param.constraint,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                        &[ident.escaped_text.as_str()],
                    );
                }
            }

            for child_idx in self.ctx.arena.get_children(node_idx) {
                self.report_instantiated_mapped_constraint_cycles_in_node(
                    child_idx,
                    current_sym,
                    bindings,
                );
            }

            if let Some((name, previous)) = local_update {
                self.ctx.type_parameter_scope.remove(&name);
                if let Some(previous) = previous {
                    self.ctx.type_parameter_scope.insert(name, previous);
                }
            }
            return;
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.report_instantiated_mapped_constraint_cycles_in_node(
                child_idx,
                current_sym,
                bindings,
            );
        }
    }

    fn constraint_mentions_instantiated_cycle_target(
        &mut self,
        node_idx: NodeIndex,
        current_sym: SymbolId,
        bindings: &FxHashMap<String, TypeId>,
        shadowed_params: &mut FxHashSet<String>,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && !shadowed_params.contains(ident.escaped_text.as_str())
            && let Some(&arg) = bindings.get(ident.escaped_text.as_str())
        {
            use tsz_solver::recursion::RecursionProfile;
            let mut guard = tsz_solver::recursion::RecursionGuard::with_profile(
                RecursionProfile::ShallowTraversal,
            );
            return self.type_requires_structure_of_symbol_inner(
                arg,
                current_sym,
                true,
                false,
                &mut guard,
            );
        }

        let mut inserted_shadow = None;
        if node.kind == syntax_kind_ext::MAPPED_TYPE
            && let Some(mapped) = self.ctx.arena.get_mapped_type(node)
            && let Some(type_param_node) = self.ctx.arena.get(mapped.type_parameter)
            && let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node)
            && let Some(name_node) = self.ctx.arena.get(type_param.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.clone();
            if shadowed_params.insert(name.clone()) {
                inserted_shadow = Some(name);
            }
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.constraint_mentions_instantiated_cycle_target(
                child_idx,
                current_sym,
                bindings,
                shadowed_params,
            ) {
                if let Some(name) = inserted_shadow {
                    shadowed_params.remove(&name);
                }
                return true;
            }
        }

        if let Some(name) = inserted_shadow {
            shadowed_params.remove(&name);
        }

        false
    }
}

/// Check if contextual type is `boolean` and the literal is a boolean literal.
fn is_boolean_context_for_boolean_literal(ctx_type: TypeId, literal_type: TypeId) -> bool {
    if ctx_type != TypeId::BOOLEAN
        && ctx_type != TypeId::BOOLEAN_TRUE
        && ctx_type != TypeId::BOOLEAN_FALSE
    {
        return false;
    }
    literal_type == TypeId::BOOLEAN_TRUE || literal_type == TypeId::BOOLEAN_FALSE
}

#[cfg(test)]
mod deferred_conditional_literal_tests {
    use crate::test_utils::check_source_codes;

    /// Assigning a string literal to a deferred conditional whose false branch
    /// is that exact literal must NOT widen the source to `string`. Matches
    /// tsc's `isLiteralOfContextualType` recursing through conditional types.
    ///
    /// ```ts
    /// type Foo<T> = T extends true ? string : "a";
    /// function test<T>(x: Foo<T>) {
    ///   x = "a"; // ok — both branches accept "a"
    /// }
    /// ```
    #[test]
    fn literal_preserved_when_target_is_deferred_conditional_with_literal_branch() {
        let codes = check_source_codes(
            r#"type Foo<T> = T extends true ? string : "a";
               function test<T>(x: Foo<T>) {
                 x = "a";
               }"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 when assigning matching literal to deferred conditional: {codes:?}"
        );
    }

    /// Sanity check: assigning a non-matching `string` value still errors.
    #[test]
    fn deferred_conditional_still_errors_on_widened_source() {
        let codes = check_source_codes(
            r#"type Foo<T> = T extends true ? "b" : "a";
               function test<T>(x: Foo<T>, s: string) {
                 x = s;
               }"#,
        );
        assert!(
            codes.contains(&2322),
            "Should emit TS2322 when assigning a `string` value to a literal-only deferred conditional: {codes:?}"
        );
    }
}
