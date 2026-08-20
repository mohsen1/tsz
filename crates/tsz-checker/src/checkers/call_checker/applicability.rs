//! Adapter methods for routing call/new resolution through the solver.

use super::CheckerCallAssignabilityAdapter;
use crate::query_boundaries::checkers::call::{
    CallArgSourceOptions, resolve_call, resolve_call_with_arg_sources, resolve_new,
};
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Per-argument source markers threaded into generic-call resolution. Bundled
/// so the adapter methods stay within the argument-count budget.
pub(crate) struct CallArgSourceMarkers<'a> {
    /// Per-argument: source came from a type annotation/assertion.
    pub(crate) source_is_type_annotation: &'a [bool],
    /// Per-argument: source came from a readonly array/tuple annotation.
    pub(crate) source_is_readonly_annotation: &'a [bool],
    /// Per-argument, per-parameter: unannotated (context-sensitive) callback
    /// parameter (issue #17282).
    pub(crate) callback_param_unannotated: &'a [Vec<bool>],
}

/// Owned counterpart of [`CallArgSourceMarkers`] returned by
/// `CheckerState::call_arg_source_markers`; borrow it with [`Self::as_borrowed`].
pub(crate) struct OwnedCallArgSourceMarkers {
    pub(crate) source_is_type_annotation: Vec<bool>,
    pub(crate) source_is_readonly_annotation: Vec<bool>,
    pub(crate) callback_param_unannotated: Vec<Vec<bool>>,
}

impl OwnedCallArgSourceMarkers {
    pub(crate) fn as_borrowed(&self) -> CallArgSourceMarkers<'_> {
        CallArgSourceMarkers {
            source_is_type_annotation: &self.source_is_type_annotation,
            source_is_readonly_annotation: &self.source_is_readonly_annotation,
            callback_param_unannotated: &self.callback_param_unannotated,
        }
    }
}

impl CallArgSourceMarkers<'_> {
    pub(crate) fn any_set(&self) -> bool {
        self.source_is_type_annotation.iter().any(|&m| m)
            || self.source_is_readonly_annotation.iter().any(|&m| m)
            || self
                .callback_param_unannotated
                .iter()
                .any(|mask| mask.iter().any(|&u| u))
    }

    /// Whether generic call resolution must take the arg-source-aware path.
    ///
    /// It runs when any marker is set, and additionally whenever a
    /// callback-shaped argument is present — even a fully annotated one, whose
    /// per-parameter mask is non-empty but all-`false`. Forwarding those aligned
    /// masks lets the solver distinguish "reliably no context-sensitive
    /// argument" from "no information" when applying tsc's `isFixed` literal-
    /// widen gate (a fresh literal for an *unfixed* top-level-in-return type
    /// parameter is not widened). The plain adapter is the arg-source adapter
    /// with empty slices, so forwarding an all-`false` mask is behavior-neutral
    /// apart from enabling that gate. Issue #17710.
    pub(crate) fn requires_arg_source_path(&self) -> bool {
        self.any_set()
            || self
                .callback_param_unannotated
                .iter()
                .any(|mask| !mask.is_empty())
    }
}

impl<'a> CheckerState<'a> {
    pub(super) fn callable_context_can_type_function_argument_despite_unresolved(
        &self,
        arg_idx: NodeIndex,
        expected_context_type: Option<TypeId>,
    ) -> bool {
        let Some(expected_context_type) = expected_context_type else {
            return false;
        };
        if !self.is_callback_like_argument(arg_idx) {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape
                .params
                .iter()
                .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR);
        }

        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape.call_signatures.iter().all(|sig| {
                sig.params
                    .iter()
                    .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR)
            });
        }

        false
    }

    pub(super) fn normalized_spread_argument_type(&mut self, expr: NodeIndex) -> TypeId {
        let spread_type = self.get_type_of_node(expr);
        if crate::query_boundaries::checkers::call::tuple_elements_for_type(
            self.ctx.types,
            spread_type,
        )
        .is_some()
        {
            return spread_type;
        }
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        let spread_type = self.evaluate_type_with_env(spread_type);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        let spread_type = self.evaluate_application_type(spread_type);
        self.apparent_type_of_deferred_conditional_spread(spread_type)
    }

    /// A value typed as a deferred (uninstantiated generic) conditional has no
    /// tuple/array/iterator shape of its own, so the spread element-extraction
    /// paths in `candidate_collection` would relate the whole conditional to the
    /// rest-parameter element and emit a false `TS2345`.
    ///
    /// `tsc` iterates such a spread through the conditional's *apparent* type —
    /// the union of its branch constraints with the true branch's check
    /// parameter narrowed to `check & extends` (`getApparentType` ->
    /// `getDefaultConstraintOfConditionalType`). Adopt that apparent type here
    /// when it is itself iterable, so the existing array/tuple/iterable branches
    /// recover the real element type. This is scoped to the spread normalization
    /// path: a non-conditional type, a conditional with no reducible constraint,
    /// or one whose apparent type is not iterable is left untouched so unrelated
    /// diagnostics are unchanged.
    fn apparent_type_of_deferred_conditional_spread(&mut self, spread_type: TypeId) -> TypeId {
        if !crate::query_boundaries::common::is_conditional_type(self.ctx.types, spread_type) {
            return spread_type;
        }
        let Some(apparent) =
            crate::query_boundaries::conditional_constraints::conditional_apparent_value_constraint(
                self.ctx.types,
                spread_type,
            )
        else {
            return spread_type;
        };
        // Only adopt the apparent type when it actually yields an iterable shape;
        // otherwise the conditional was not standing in for a spreadable value and
        // the original (deferred) type drives the unchanged diagnostic path.
        if self.is_iterable_type(apparent) {
            apparent
        } else {
            spread_type
        }
    }

    /// Const object/array literal bindings do not benefit from flow narrowing at
    /// call sites. Skipping flow narrowing for these stable identifiers avoids
    /// repeated CFG traversals on large argument objects.
    pub(super) fn can_skip_flow_narrowing_for_argument(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
            return false;
        }

        let Some(decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return false;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
    }

    pub(crate) fn resolve_call_with_checker_adapter(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> tsz_solver::operations::CallWithCheckerResult {
        self.ensure_callee_relation_inputs_ready(func_type);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter {
            state: self,
            overload_subtype_pass: false,
        };
        resolve_call(
            db,
            &mut checker,
            func_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
        )
    }

    pub(crate) fn resolve_call_with_checker_adapter_and_arg_sources(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
        markers: &CallArgSourceMarkers<'_>,
    ) -> tsz_solver::operations::CallWithCheckerResult {
        self.ensure_callee_relation_inputs_ready(func_type);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter {
            state: self,
            overload_subtype_pass: false,
        };
        resolve_call_with_arg_sources(
            db,
            &mut checker,
            func_type,
            arg_types,
            &CallArgSourceOptions {
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
                arg_source_is_type_annotation: markers.source_is_type_annotation,
                arg_source_is_readonly_annotation: markers.source_is_readonly_annotation,
                arg_callback_param_unannotated: markers.callback_param_unannotated,
            },
        )
    }

    /// Resolve a call, routing through the arg-source-aware path when any
    /// argument is a type-annotated source (typed identifier, `as`/`satisfies`
    /// assertion, or `as const`) or an unannotated callback parameter (#17282).
    /// Falls back to the plain adapter when no marker is set, preserving its
    /// behavior exactly.
    pub(crate) fn resolve_call_with_checker_adapter_maybe_arg_sources(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
        markers: &CallArgSourceMarkers<'_>,
    ) -> tsz_solver::operations::CallWithCheckerResult {
        if markers.any_set() {
            self.resolve_call_with_checker_adapter_and_arg_sources(
                func_type,
                arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
                markers,
            )
        } else {
            self.resolve_call_with_checker_adapter(
                func_type,
                arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            )
        }
    }

    /// Pass-1 ("subtype pass") variant of
    /// `resolve_call_with_checker_adapter_maybe_arg_sources` for overload
    /// resolution: identical argument handling, but the adapter's relation
    /// probes treat an `any` source as not related to concrete targets at
    /// every nesting level (tsc `chooseOverload` with `subtypeRelation`).
    pub(crate) fn resolve_call_with_checker_adapter_subtype_pass(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
        markers: &CallArgSourceMarkers<'_>,
    ) -> tsz_solver::operations::CallWithCheckerResult {
        self.ensure_callee_relation_inputs_ready(func_type);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter {
            state: self,
            overload_subtype_pass: true,
        };
        if markers.any_set() {
            resolve_call_with_arg_sources(
                db,
                &mut checker,
                func_type,
                arg_types,
                &CallArgSourceOptions {
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                    arg_source_is_type_annotation: markers.source_is_type_annotation,
                    arg_source_is_readonly_annotation: markers.source_is_readonly_annotation,
                    arg_callback_param_unannotated: markers.callback_param_unannotated,
                },
            )
        } else {
            resolve_call(
                db,
                &mut checker,
                func_type,
                arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            )
        }
    }

    pub(crate) fn resolve_new_with_checker_adapter(
        &mut self,
        type_id: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
    ) -> CallResult {
        self.ensure_relation_input_ready(type_id);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter {
            state: self,
            overload_subtype_pass: false,
        };
        resolve_new(
            db,
            &mut checker,
            type_id,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
        )
    }
}
