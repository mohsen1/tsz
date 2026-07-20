//! Recovery helpers for property access during class construction.

use crate::classes_domain::class_summary::ClassChainSummary;
use crate::query_boundaries::class_type as class_type_boundary;
use crate::state::CheckerState;
use std::rc::Rc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns the actual declared callable surface for a named instance method
    /// on the class currently being built.
    ///
    /// Phase-2 class publication intentionally gives deferred methods a cheap
    /// rest-`any` placeholder. Semantic consumers that need a method's parameter
    /// surface before final publication can use this bounded symbol-table lookup
    /// instead of scanning the provisional object or the whole class body.
    pub(crate) fn direct_enclosing_class_method_declared_type(
        &mut self,
        property_name: &str,
    ) -> Option<TypeId> {
        let (class_idx, member_sym_id, declarations) = {
            let class_idx = self.ctx.enclosing_class.as_ref()?.class_idx;
            let class_sym_id = self.ctx.binder.get_node_symbol(class_idx)?;
            let class_symbol = self.ctx.binder.get_symbol(class_sym_id)?;
            let member_sym_id = class_symbol.members.as_ref()?.get(property_name)?;
            let declarations = self
                .ctx
                .binder
                .get_symbol(member_sym_id)?
                .declarations
                .clone();
            (class_idx, member_sym_id, declarations)
        };

        let mut overload_signatures = Vec::new();
        let mut implementation_signatures = Vec::new();
        let mut overload_optional = false;
        let mut implementation_optional = false;
        for member_idx in declarations {
            if !self
                .ctx
                .declaration_is_local_to_current_arena(member_sym_id, member_idx)
                || self.nearest_enclosing_class(member_idx) != Some(class_idx)
            {
                continue;
            }
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                continue;
            };
            if self.has_static_modifier(&method.modifiers) {
                continue;
            }
            let signature = self.call_signature_parameter_surface_from_method(method, member_idx);
            if method.body.is_none() {
                overload_optional |= method.question_token;
                overload_signatures.push(signature);
            } else {
                implementation_optional |= method.question_token;
                implementation_signatures.push(signature);
            }
        }
        let (signatures, optional) = if overload_signatures.is_empty() {
            (implementation_signatures, implementation_optional)
        } else {
            (overload_signatures, overload_optional)
        };
        if signatures.is_empty() {
            return None;
        }

        let method_type =
            class_type_boundary::class_method_callable_type(self.ctx.types, signatures);
        Some(class_type_boundary::optional_class_member_type(
            self.ctx.types,
            method_type,
            optional,
        ))
    }

    /// Recover a bare-`this` member whose syntactically nearest class is only a
    /// class-header evaluation boundary, not the owner of lexical `this`.
    ///
    /// Decorators, heritage expressions, and computed names on a nested class
    /// execute in the enclosing lexical scope. During early class construction
    /// that enclosing instance may not have merged inherited members into its
    /// partial object shape yet. The ordinary recovery path starts from the
    /// syntactically nearest class and therefore cannot find those members.
    /// Keep this parent walk on the `PropertyNotFound` edge only; the common
    /// class-`this` lookup continues to use `ctx.enclosing_class` in O(1).
    pub(super) fn recover_bare_this_lexical_class_header_member(
        &mut self,
        receiver_expr: NodeIndex,
        property_name: &str,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        let receiver_expr = self.ctx.arena.skip_parenthesized(receiver_expr);
        if !self.is_this_expression(receiver_expr) {
            return None;
        }

        let syntactic_class = self.nearest_enclosing_class(receiver_expr)?;
        let lexical_class =
            tsz_parser::syntax::transform_utils::nearest_enclosing_lexical_this_class(
                self.ctx.arena,
                receiver_expr,
            )?;
        if lexical_class == syntactic_class {
            return None;
        }

        let is_static_access = self
            .direct_child_below_ancestor(receiver_expr, lexical_class)
            .is_some_and(|member_idx| self.class_member_is_static(member_idx));
        let lexical_summary = self.summarize_class_chain(lexical_class);
        let bound_type_params = self
            .active_class_summary_root_type_params(lexical_class, &lexical_summary, true)
            .unwrap_or_else(|| lexical_summary.root_type_params().to_vec());
        let mut summary = Some(lexical_summary);
        let recovered = self.recover_property_from_class_chain_summary(
            true,
            Some((lexical_class, is_static_access)),
            &mut summary,
            property_name,
        )?;
        Some((recovered, bound_type_params))
    }

    fn direct_child_below_ancestor(
        &self,
        node_idx: NodeIndex,
        ancestor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 1024 {
                return None;
            }
            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent == ancestor_idx {
                return Some(current);
            }
            current = parent;
        }
        None
    }

    fn active_class_summary_root_type_params(
        &self,
        class_idx: NodeIndex,
        summary: &ClassChainSummary,
        allow_construction_scope: bool,
    ) -> Option<Vec<TypeId>> {
        if let Some(info) = self
            .ctx
            .enclosing_class
            .as_ref()
            .filter(|info| info.class_idx == class_idx)
        {
            return Some(info.class_type_parameter_ids.clone());
        }
        allow_construction_scope.then(|| {
            summary
                .root_type_params_from_active_scope(self.ctx.types, &self.ctx.type_parameter_scope)
        })?
    }

    pub(crate) fn resolve_class_access_with_current_member_initializer_recovery(
        &mut self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> (Option<(NodeIndex, bool)>, bool) {
        let mut resolved = self.resolve_class_for_access(expression, receiver_type);
        let recovery = self.resolve_current_class_member_initializer_access_for_recovery(
            expression,
            receiver_type,
        );
        let recovery_is_lexical_this = recovery.is_some()
            && self.is_this_expression(self.ctx.arena.skip_parenthesized(expression));
        if recovery_is_lexical_this || resolved.is_none() {
            resolved = recovery;
        }
        let is_current = recovery.is_some()
            || self.property_access_is_current_class_member_initializer_receiver(
                expression,
                receiver_type,
            );
        (resolved, is_current)
    }

    pub(crate) fn resolve_current_class_member_initializer_access_for_recovery(
        &mut self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        let (class_idx, class_sym) = self.current_class_member_initializer_context(expression)?;
        let receiver_is_current_class_identifier = self
            .resolve_identifier_symbol_without_tracking(expression)
            .is_some_and(|receiver_sym| receiver_sym == class_sym);
        if !self.property_access_receiver_targets_class(receiver_type, class_sym)
            && !self.asserted_this_receiver_targets_current_class(expression, class_sym)
            && !receiver_is_current_class_identifier
        {
            return None;
        }

        let is_static_access = self.find_enclosing_static_block(expression).is_some()
            || self.is_in_static_class_member_context(expression)
            || receiver_is_current_class_identifier
            || self.is_constructor_type(receiver_type);
        Some((class_idx, is_static_access))
    }

    pub(crate) fn property_access_is_current_class_construction_recovery(
        &self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> bool {
        if !self
            .property_access_is_current_class_member_initializer_receiver(expression, receiver_type)
        {
            return false;
        }
        let Some(class_idx) = self.nearest_enclosing_class(expression) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_node_symbol(class_idx) else {
            return false;
        };
        self.ctx.class_instance_resolution_set.contains(&class_sym)
    }

    pub(crate) fn property_access_is_current_class_member_initializer_receiver(
        &self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> bool {
        let Some(class_idx) = self.nearest_enclosing_class(expression) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_node_symbol(class_idx) else {
            return false;
        };
        if !self.ctx.class_instance_resolution_set.contains(&class_sym) {
            return false;
        }
        if self.ctx.checking_computed_property_name.is_none()
            && !self.property_access_is_in_class_property_initializer(expression)
        {
            return false;
        }

        self.property_access_receiver_targets_class(receiver_type, class_sym)
    }

    pub(crate) fn property_access_receiver_symbol(
        &self,
        type_id: TypeId,
    ) -> Option<tsz_binder::SymbolId> {
        self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
                .and_then(|(base, _)| self.ctx.resolve_type_to_symbol_id(base))
        })
    }

    fn property_access_receiver_targets_class(
        &self,
        receiver_type: TypeId,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(receiver_sym) = self.property_access_receiver_symbol(receiver_type) else {
            return false;
        };
        receiver_sym == class_sym
            || self
                .ctx
                .inheritance_graph
                .is_derived_from(receiver_sym, class_sym)
            || self.property_access_receiver_declared_extends_class(receiver_sym, class_sym)
    }

    fn property_access_receiver_declared_extends_class(
        &self,
        receiver_sym: tsz_binder::SymbolId,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(receiver_idx) = self.get_class_declaration_from_symbol(receiver_sym) else {
            return false;
        };
        let Some(class_idx) = self.get_class_declaration_from_symbol(class_sym) else {
            return false;
        };
        self.is_class_derived_from(receiver_idx, class_idx)
    }

    fn current_class_member_initializer_context(
        &self,
        expression: NodeIndex,
    ) -> Option<(NodeIndex, tsz_binder::SymbolId)> {
        let class_idx = self.nearest_enclosing_class(expression)?;
        let class_sym = self.ctx.binder.get_node_symbol(class_idx)?;
        Some((class_idx, class_sym))
    }

    fn asserted_this_receiver_targets_current_class(
        &mut self,
        expression: NodeIndex,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let mut current = expression;
        let mut saw_assertion = false;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 64 {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::TYPE_ASSERTION => {
                    let Some(assertion) = self.ctx.arena.get_type_assertion(node) else {
                        return false;
                    };
                    let assertion_expression = assertion.expression;
                    let assertion_type_node = assertion.type_node;
                    let assertion_type = self.get_type_from_type_node(assertion_type_node);
                    if !self.property_access_receiver_targets_class(assertion_type, class_sym) {
                        return false;
                    }
                    saw_assertion = true;
                    current = assertion_expression;
                }
                _ => return saw_assertion && self.is_this_expression(current),
            }
        }
        false
    }

    pub(super) fn recover_property_from_class_chain_summary(
        &mut self,
        is_current_class_member_initializer_receiver: bool,
        resolved_class_access: Option<(NodeIndex, bool)>,
        summary: &mut Option<Rc<ClassChainSummary>>,
        property_name: &str,
    ) -> Option<TypeId> {
        if !is_current_class_member_initializer_receiver {
            return None;
        }
        let (class_idx, is_static_access) = resolved_class_access?;
        if summary.is_none() {
            *summary = Some(self.summarize_class_chain(class_idx));
        }
        let own_member_type = self.own_class_member_type_for_recovery(
            class_idx,
            property_name,
            is_static_access,
            true,
        );
        let summary = summary.as_ref()?;
        if let Some(member_type) = own_member_type {
            if let Some(active_root_type_params) =
                self.active_class_summary_root_type_params(class_idx, summary, true)
            {
                return Some(summary.rebind_root_type_params(
                    self.ctx.types,
                    &active_root_type_params,
                    member_type,
                ));
            }
            return Some(member_type);
        }
        let member_type = summary
            .member_info(property_name, is_static_access, true)?
            .type_id;
        let active_root_type_params =
            self.active_class_summary_root_type_params(class_idx, summary, true);
        Some(summary.rebind_root_type_params(
            self.ctx.types,
            active_root_type_params.as_deref().unwrap_or(&[]),
            member_type,
        ))
    }

    pub(super) fn recover_direct_this_class_chain_member(
        &mut self,
        direct_class_this_receiver: bool,
        used_class_chain_method_type: bool,
        receiver_expr: NodeIndex,
        property_name: &str,
        prop_type: TypeId,
        object_type_for_access: TypeId,
        original_object_type: TypeId,
    ) -> Option<(TypeId, bool)> {
        if !direct_class_this_receiver
            || object_type_for_access != original_object_type
            || (!used_class_chain_method_type
                && self.enclosing_class_declares_member(property_name))
        {
            return None;
        }

        let class_idx = self.nearest_enclosing_class(receiver_expr)?;
        let summary = self.summarize_class_chain(class_idx);
        let member = summary.member_info(property_name, false, true)?;
        let member_is_method_like = member.is_method || member.is_accessor;
        if !used_class_chain_method_type
            && (member.from_interface
                || (member_is_method_like
                    && !matches!(prop_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR))
                || matches!(
                    member.type_id,
                    TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                )
                || member.type_id == prop_type)
        {
            return None;
        }

        let in_construction_scope = self.ctx.checking_computed_property_name.is_some()
            || self.property_access_is_in_class_property_initializer(receiver_expr);
        let active_root_type_params =
            self.active_class_summary_root_type_params(class_idx, &summary, in_construction_scope);
        let member_type = summary.rebind_root_type_params(
            self.ctx.types,
            active_root_type_params.as_deref().unwrap_or(&[]),
            member.type_id,
        );
        Some((member_type, member_is_method_like))
    }

    fn enclosing_class_declares_member(&self, property_name: &str) -> bool {
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };
        // Per-file memo: the membership scan is `O(members)` per access and runs
        // once per `this.x` property access, so on an `N`-member class it is
        // `O(N^2)`. The answer is a pure function of the immutable AST, keyed by
        // `(class node, member-name Atom)`. Interning the name to its canonical
        // `Atom` (`O(1)` amortized) gives an identity key.
        let name_atom = self.ctx.types.intern_string(property_name);
        let cache_key = (class_info.class_idx, name_atom);
        if let Some(&cached) = self
            .ctx
            .enclosing_class_declares_member_cache
            .borrow()
            .get(&cache_key)
        {
            return cached;
        }
        let declares = class_info
            .member_nodes
            .iter()
            .any(|&member_idx| self.get_member_name(member_idx).as_deref() == Some(property_name));
        self.ctx
            .enclosing_class_declares_member_cache
            .borrow_mut()
            .insert(cache_key, declares);
        declares
    }

    pub(super) fn substitute_direct_this_property_shape_type(
        &self,
        direct_class_this_receiver: bool,
        used_class_chain_method_type: bool,
        object_type_for_access: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if used_class_chain_method_type || !direct_class_this_receiver {
            return None;
        }

        // Accelerated by-name member lookup: the canonical `find_property_in_object`
        // query uses the shape's per-`ObjectShapeId` property index (O(1) for large
        // shapes, sorted-`Atom` binary search otherwise). This is byte-identical to
        // the previous linear `resolve_atom_ref` string scan because shape property
        // names are interned, so matching the receiver's interned property `Atom`
        // resolves the same property. Routing through it removes the O(N) per-access
        // scan that made N `this.x` accesses on an N-property class O(N^2).
        let name_atom = self.ctx.types.intern_string(property_name);
        let raw_prop = crate::query_boundaries::common::find_property_in_object(
            self.ctx.types,
            object_type_for_access,
            name_atom,
        )?;
        crate::query_boundaries::common::contains_this_type(self.ctx.types, raw_prop.type_id).then(
            || {
                crate::query_boundaries::common::substitute_this_type(
                    self.ctx.types,
                    raw_prop.type_id,
                    self.ctx.types.this_type(),
                )
            },
        )
    }

    pub(super) fn has_recoverable_current_class_member(
        &mut self,
        is_current_class_member_initializer_receiver: bool,
        resolved_class_access: Option<(NodeIndex, bool)>,
        summary: &mut Option<Rc<ClassChainSummary>>,
        property_name: &str,
    ) -> bool {
        if !is_current_class_member_initializer_receiver {
            return false;
        }
        let Some((class_idx, is_static_access)) = resolved_class_access else {
            return false;
        };
        if summary.is_none() {
            *summary = Some(self.summarize_class_chain(class_idx));
        }
        if self
            .own_class_member_type_for_recovery(class_idx, property_name, is_static_access, true)
            .is_some()
        {
            return true;
        }
        summary.as_ref().is_some_and(|summary| {
            summary
                .member_info(property_name, is_static_access, true)
                .is_some()
        })
    }

    fn property_access_is_in_class_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut child = NodeIndex::NONE;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 1024 {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(property) = self.ctx.arena.get_property_decl(node) else {
                        return false;
                    };
                    let is_active_class_member = self.ctx.enclosing_class.as_ref().is_some_and(
                        |class_info| {
                            self.ctx
                                .arena
                                .get_extended(current)
                                .is_some_and(|ext| ext.parent == class_info.class_idx)
                        },
                    );
                    if is_active_class_member && property.initializer == child {
                        return true;
                    }
                    if !child.is_some()
                        || !tsz_parser::syntax::transform_utils::child_is_in_enclosing_lexical_this_scope(
                            self.ctx.arena,
                            current,
                            child,
                        )
                    {
                        return false;
                    }
                }
                // A regular nested function owns a distinct `this`; an arrow is
                // deliberately not a boundary because it retains the field
                // initializer's lexical class receiver.
                syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::FUNCTION_DECLARATION => {
                    return false;
                }
                syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION => {
                    if !child.is_some()
                        || !tsz_parser::syntax::transform_utils::child_is_in_enclosing_lexical_this_scope(
                            self.ctx.arena,
                            current,
                            child,
                        )
                    {
                        return false;
                    }
                }
                syntax_kind_ext::CONSTRUCTOR => return false,
                _ => {}
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            child = current;
            current = ext.parent;
        }
        false
    }

    /// Returns the declared type of the named instance member from the directly
    /// enclosing class body. Used during `this`-receiver property access recovery
    /// when the enclosing class type is not yet fully resolved.
    pub(super) fn direct_this_class_member_declared_type(
        &mut self,
        property_name: &str,
    ) -> Option<TypeId> {
        let member_nodes = self.ctx.enclosing_class.as_ref()?.member_nodes.clone();

        for member_idx in member_nodes {
            if self.get_member_name(member_idx).as_deref() != Some(property_name) {
                continue;
            }

            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    let signature = self.call_signature_from_method(method, member_idx);
                    let method_type = class_type_boundary::class_method_callable_type(
                        self.ctx.types,
                        vec![signature],
                    );
                    return Some(class_type_boundary::optional_class_member_type(
                        self.ctx.types,
                        method_type,
                        method.question_token,
                    ));
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if let Some(type_id) =
                        self.effective_class_property_declared_type(member_idx, prop)
                    {
                        return Some(type_id);
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    let accessor_type = self.get_type_of_node(member_idx);
                    if accessor_type != TypeId::ERROR {
                        return Some(accessor_type);
                    }
                }
                _ => {}
            }
        }

        None
    }
}
