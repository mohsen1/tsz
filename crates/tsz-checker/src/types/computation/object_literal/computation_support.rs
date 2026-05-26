//! Support helpers for object literal type computation.

use crate::state::CheckerState;
use crate::symbols_domain::name_text::{
    is_zero_arg_call_like_expr_in_arena, simple_computed_name_expr_text_in_arena,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId};

pub(super) const SPREAD_DISPLAY_ORDER_OFFSET: u32 = 1_000_000;
pub(super) const SPREAD_DISPLAY_ORDER_STRIDE: u32 = 10_000;

pub(super) fn rebase_spread_display_property_order(
    props: &[PropertyInfo],
    base: u32,
) -> Vec<PropertyInfo> {
    let mut props = props.to_vec();
    props.sort_by_key(|prop| prop.declaration_order);
    for (index, prop) in props.iter_mut().enumerate() {
        prop.declaration_order = base.saturating_add(index as u32);
    }
    props
}

pub(super) fn remove_synthetic_missing_union_spread_props(member_props: &mut [Vec<PropertyInfo>]) {
    let mut required_names = rustc_hash::FxHashSet::default();
    for props in member_props.iter() {
        for prop in props {
            if !prop.optional {
                required_names.insert(prop.name);
            }
        }
    }
    if required_names.is_empty() {
        return;
    }

    // Conditional object literal unions are completed with `p?: undefined`
    // placeholders for display/type-union balance. In an object spread, that
    // placeholder means the branch omits `p`; it should not materialize as a
    // spread property when another branch supplies a required `p`.
    for props in member_props {
        props.retain(|prop| {
            !(prop.optional
                && prop.type_id == TypeId::UNDEFINED
                && required_names.contains(&prop.name))
        });
    }
}

impl<'a> CheckerState<'a> {
    pub(super) fn object_literal_property_is_typed_variable_initializer(
        &self,
        property_elem_idx: NodeIndex,
    ) -> bool {
        let Some(object_idx) = self.ctx.arena.parent_of(property_elem_idx) else {
            return false;
        };
        let Some(parent_idx) = self.ctx.arena.parent_of(object_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        self.ctx
            .arena
            .get_variable_declaration(parent_node)
            .is_some_and(|var_decl| {
                var_decl.initializer == object_idx && var_decl.type_annotation.is_some()
            })
    }

    /// Decide whether an object-literal property value's literal type should
    /// be widened to its primitive. Inside a `satisfies T` operand we apply
    /// tsc's exact `isLiteralOfContextualType` per-property gate (via
    /// `contextual_type_allows_literal`) and ignore `preserve_literal_types` —
    /// the user-written `T` is a finer specification than the wrapping
    /// generic-call's contextual broadening. Outside, the coarser legacy
    /// policy (preserve whenever the outer object context is non-permissive)
    /// is kept and the downstream deep-widen compensates.
    pub(super) fn should_widen_object_property_literal(
        &mut self,
        value_type: TypeId,
        property_context_type: Option<TypeId>,
        had_object_context: bool,
        has_non_widening_source: bool,
    ) -> bool {
        if self.ctx.in_const_assertion || has_non_widening_source {
            return false;
        }
        if self.ctx.in_satisfies_operand {
            // Unconstrained properties (not covered by the satisfies type) preserve their
            // literal types — tsc's `isLiteralOfContextualType` returns false for absent
            // properties.
            let Some(ctx_type) = property_context_type else {
                return false;
            };
            // Skip the recursive `isLiteralOfContextualType` walk for values the widener
            // cannot transform anyway — functions, plain objects, `any`, etc.
            let value_is_widenable =
                crate::query_boundaries::common::is_literal_type(self.ctx.types, value_type)
                    || crate::query_boundaries::common::is_union_type(self.ctx.types, value_type);
            if !value_is_widenable {
                return false;
            }
            return !self.contextual_type_allows_literal(ctx_type, value_type);
        }
        if self.ctx.preserve_literal_types {
            return false;
        }
        let property_context_preserves_literal = property_context_type.is_some_and(|ct| {
            !crate::query_boundaries::type_computation::core::is_literal_permissive_object_context(
                ct,
            )
        });
        !property_context_preserves_literal && !had_object_context
    }

    pub(super) fn spread_source_is_unannotated_object_literal_binding(
        &self,
        expression: NodeIndex,
    ) -> bool {
        let expression = self.ctx.arena.skip_parenthesized_and_assertions(expression);
        let Some(node) = self.ctx.arena.get(expression) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expression)
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if var_decl.type_annotation.is_some() {
                return false;
            }
            let initializer = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(var_decl.initializer);
            let Some(init_node) = self.ctx.arena.get(initializer) else {
                return false;
            };
            return init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
        }
        false
    }

    /// Merge a single spread-contributed property into the running
    /// `properties` map.
    ///
    /// tsc's spread merge rule is asymmetric on the *later* property's
    /// optionality:
    /// - When the later property is **required**, it fully overrides the
    ///   earlier one (the runtime always sees the later value).
    /// - When the later property is **optional**, the runtime may skip
    ///   it, so the earlier contribution still applies. The merged read
    ///   type is the union of both, the merged write type is the union
    ///   of both write types, and the merged property is required iff
    ///   *some* contributor was required. `readonly` is intersected.
    ///
    /// The unconditional-override path that this replaces broke the
    /// optional-later case in
    /// `compiler/conformance/types/spread/objectSpreadStrictNull.ts`,
    /// where `{ ...definiteString, ...optionalNumber }` should produce
    /// `{ sn: string | number }`, not `{ sn?: number }`.
    pub(super) fn merge_spread_property(
        &self,
        properties: &mut rustc_hash::FxHashMap<tsz_common::interner::Atom, PropertyInfo>,
        prop: &PropertyInfo,
    ) {
        use std::collections::hash_map::Entry;
        match properties.entry(prop.name) {
            Entry::Vacant(slot) => {
                slot.insert(prop.clone());
            }
            Entry::Occupied(mut slot) => {
                let merged =
                    crate::query_boundaries::type_computation::core::merge_object_spread_property(
                        self.ctx.types,
                        self.ctx.exact_optional_property_types(),
                        Some(slot.get()),
                        prop,
                    );
                slot.insert(merged);
            }
        }
    }

    pub(super) fn variable_declaration_for_symbol_decl(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::VariableDeclarationData> {
        let mut current = decl_idx;
        for _ in 0..4 {
            let node = self.ctx.arena.get(current)?;
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                return Some(var_decl);
            }
            current = self.ctx.arena.get_extended(current)?.parent;
        }
        None
    }

    pub(super) fn expression_is_type_assertion(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION
        })
    }

    /// Returns `true` when the symbol's value type comes from a non-fresh source:
    /// either an explicit variable type annotation or a const-asserted initializer.
    pub(super) fn sym_has_non_widening_declared_value_type(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        std::iter::once(symbol.value_declaration)
            .chain(symbol.declarations.iter().copied())
            .any(|decl_idx| {
                self.variable_declaration_for_symbol_decl(decl_idx)
                    .is_some_and(|var_decl| {
                        self.ctx.arena.get(var_decl.type_annotation).is_some()
                            || self.expression_is_const_assertion(var_decl.initializer)
                    })
            })
    }

    pub(super) fn identifier_refers_to_non_widening_declared_value_type(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
        self.ctx
            .binder
            .resolve_identifier(self.ctx.arena, node_idx)
            .is_some_and(|sym_id| self.sym_has_non_widening_declared_value_type(sym_id))
    }

    pub(super) fn object_literal_property_access_literal_type(
        &mut self,
        node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node_idx = self.ctx.arena.skip_parenthesized_and_assertions(node_idx);
        let node = self.ctx.arena.get(node_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        if let Some(literal_type) = self.const_array_to_enum_member_literal_type_query(node_idx) {
            return Some(literal_type);
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        self.imported_array_to_enum_member_literal_type(access.expression, access.name_or_argument)
    }

    pub(super) fn object_literal_variable_initializer_symbol(
        &self,
        idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let parent_idx = self.ctx.arena.get_extended(idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        if var_decl.initializer != idx {
            return None;
        }
        self.ctx
            .binder
            .get_node_symbol(var_decl.name)
            .or_else(|| self.resolve_identifier_symbol_without_tracking(var_decl.name))
    }

    pub(super) fn record_partial_object_literal_property(
        &mut self,
        stack_index: Option<usize>,
        prop: &PropertyInfo,
    ) {
        let Some(stack_index) = stack_index else {
            return;
        };
        if let Some(active) = self
            .ctx
            .object_literal_tracking
            .partial_initializers
            .get_mut(stack_index)
        {
            active.properties.insert(prop.name, prop.clone());
        }
    }

    pub(super) fn pop_partial_object_literal_initializer(&mut self, stack_index: Option<usize>) {
        let Some(stack_index) = stack_index else {
            return;
        };
        if stack_index + 1 == self.ctx.object_literal_tracking.partial_initializers.len() {
            self.ctx.object_literal_tracking.partial_initializers.pop();
        } else if stack_index < self.ctx.object_literal_tracking.partial_initializers.len() {
            self.ctx
                .object_literal_tracking
                .partial_initializers
                .remove(stack_index);
        }
    }

    pub(super) fn pop_object_literal_contexts(
        &mut self,
        marker_this_type: Option<TypeId>,
        partial_initializer_stack_index: Option<usize>,
    ) {
        if marker_this_type.is_some() {
            self.ctx.this_type_stack.pop();
        }
        self.pop_partial_object_literal_initializer(partial_initializer_stack_index);
    }

    pub(super) fn contextual_this_type_from_marker(&self, ctx_type: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::ContextualTypeContext;

        let env = self.ctx.type_env.borrow();
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            ctx_type,
            self.ctx.compiler_options.no_implicit_any,
        );
        if let Some(this_type) = ctx_helper.get_this_type_from_marker_with_resolver(&*env) {
            return Some(this_type);
        }

        let def_id = self.ctx.definition_store.find_def_for_type(ctx_type)?;
        let body = self.ctx.definition_store.get_body(def_id)?;
        if body == ctx_type {
            return None;
        }

        let body_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            body,
            self.ctx.compiler_options.no_implicit_any,
        );
        body_helper.get_this_type_from_marker_with_resolver(&*env)
    }

    pub(super) fn function_like_has_explicit_signature_annotations(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match expr_node.kind {
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                let Some(func) = self.ctx.arena.get_function(expr_node) else {
                    return false;
                };
                if func.type_annotation.is_some() {
                    return true;
                }
                func.parameters.nodes.iter().any(|&param_node| {
                    self.ctx
                        .arena
                        .get(param_node)
                        .and_then(|node| self.ctx.arena.get_parameter(node))
                        .is_some_and(|param| param.type_annotation.is_some())
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .ctx
                .arena
                .get_parenthesized(expr_node)
                .is_some_and(|paren| {
                    self.function_like_has_explicit_signature_annotations(paren.expression)
                }),
            _ => false,
        }
    }

    pub(super) fn simple_computed_name_expr_text_for_duplicates(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        simple_computed_name_expr_text_in_arena(self.ctx.arena, expr_idx)
    }

    pub(super) fn is_zero_arg_call_like_expr_for_duplicates(&self, expr_idx: NodeIndex) -> bool {
        is_zero_arg_call_like_expr_in_arena(self.ctx.arena, expr_idx)
    }

    pub(super) fn simple_computed_call_name_for_duplicates(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        if !self.is_zero_arg_call_like_expr_for_duplicates(computed.expression) {
            return None;
        }
        let expr_text = self.simple_computed_name_expr_text_for_duplicates(computed.expression)?;
        Some(format!("[{expr_text}]"))
    }

    pub(super) fn this_options_property_access_receiver(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(expr_node)?;

        let base_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let base_node = self.ctx.arena.get(base_idx)?;
        if base_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let base_access = self.ctx.arena.get_access_expr(base_node)?;
        let base_name_is_options = self
            .ctx
            .arena
            .get(base_access.name_or_argument)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some_and(|ident| ident.escaped_text == "options");
        if !base_name_is_options {
            return None;
        }

        let receiver_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(base_access.expression);
        self.ctx
            .arena
            .get(receiver_idx)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            .then_some(receiver_idx)
    }
}
