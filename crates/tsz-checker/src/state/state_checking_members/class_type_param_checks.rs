//! Validation of class type parameter references in static members (TS2302),
//! abstract overload consistency (TS2512), and abstract declaration consecutiveness (TS2516).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a> CheckerState<'a> {
    /// Check if a type node references class type parameters (TS2302).
    /// Called for static members to ensure they don't reference the enclosing class's type params.
    fn check_type_node_for_class_type_param_refs(
        &mut self,
        type_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        use crate::diagnostics::diagnostic_codes;

        if type_idx.is_none() || class_type_param_names.is_empty() {
            return;
        }
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.ctx.arena.get_identifier(node)
                    && class_type_param_names.contains(&ident.escaped_text)
                {
                    self.error_at_node(
                        type_idx,
                        "Static members cannot reference class type parameters.",
                        diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                    );
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    // Check if type_name is an identifier matching a class type param
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && class_type_param_names.contains(&ident.escaped_text)
                    {
                        self.error_at_node(
                            type_idx,
                            "Static members cannot reference class type parameters.",
                            diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                        );
                    }
                    // Also check type arguments
                    if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                        && let Some(ref type_args) = type_ref.type_arguments
                    {
                        for &arg_idx in &type_args.nodes {
                            self.check_type_node_for_class_type_param_refs(
                                arg_idx,
                                class_type_param_names,
                            );
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_node_for_class_type_param_refs(
                        arr.element_type,
                        class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_type_node_for_class_type_param_refs(
                            elem_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_node_for_class_type_param_refs(
                            member_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    // Exclude function type's own type parameters (they shadow class ones)
                    let own_params = self.collect_type_param_names(&func_type.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|n| !own_params.contains(n))
                        .cloned()
                        .collect();
                    let names_to_check = if own_params.is_empty() {
                        class_type_param_names
                    } else if filtered.is_empty() {
                        return;
                    } else {
                        &filtered
                    };
                    for &param_idx in &func_type.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.check_type_node_for_class_type_param_refs(
                                param.type_annotation,
                                names_to_check,
                            );
                        }
                    }
                    self.check_type_node_for_class_type_param_refs(
                        func_type.type_annotation,
                        names_to_check,
                    );
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_node_for_class_type_param_refs(
                        wrapped.type_node,
                        class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_class_type_param_refs(
                            member_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Check a type literal member for class type parameter references.
    fn check_type_member_for_class_type_param_refs(
        &mut self,
        member_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        if let Some(sig) = self.ctx.arena.get_signature(node) {
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        self.check_type_node_for_class_type_param_refs(
                            param.type_annotation,
                            class_type_param_names,
                        );
                    }
                }
            }
            self.check_type_node_for_class_type_param_refs(
                sig.type_annotation,
                class_type_param_names,
            );
        }
    }

    fn check_static_member_body_for_class_type_param_refs(
        &mut self,
        root_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        if root_idx.is_none() || class_type_param_names.is_empty() {
            return;
        }

        let mut stack = vec![root_idx];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            let parent_is_type_node = self
                .ctx
                .arena
                .get_extended(node_idx)
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .is_some_and(|parent| parent.is_type_node());
            if node.is_type_node() && !parent_is_type_node {
                self.check_type_node_for_class_type_param_refs(node_idx, class_type_param_names);
                continue;
            }
            stack.extend(self.ctx.arena.get_children(node_idx));
        }
    }

    /// Collect type parameter names from a type parameter list.
    fn collect_type_param_names(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_parameters else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for &param_idx in &list.nodes {
            if let Some(node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_type_parameter(node)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                names.push(ident.escaped_text.clone());
            }
        }
        names
    }

    fn check_static_member_for_class_type_param_refs_with_names(
        &mut self,
        member_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        if class_type_param_names.is_empty() {
            return;
        }

        self.ctx.rebuild_emitted_diagnostics_from_current();

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node)
                    && self.has_static_modifier(&prop.modifiers)
                {
                    self.check_type_node_for_class_type_param_refs(
                        prop.type_annotation,
                        class_type_param_names,
                    );
                    self.check_static_member_body_for_class_type_param_refs(
                        prop.initializer,
                        class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node)
                    && self.has_static_modifier(&method.modifiers)
                {
                    let own_params = self.collect_type_param_names(&method.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|name| !own_params.contains(name))
                        .cloned()
                        .collect();
                    self.check_callable_for_class_type_param_refs(
                        class_type_param_names,
                        &method.type_parameters,
                        &method.parameters,
                        method.type_annotation,
                    );
                    self.check_static_member_body_for_class_type_param_refs(method.body, &filtered);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && self.has_static_modifier(&accessor.modifiers)
                {
                    let own_params = self.collect_type_param_names(&accessor.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|name| !own_params.contains(name))
                        .cloned()
                        .collect();
                    self.check_callable_for_class_type_param_refs(
                        class_type_param_names,
                        &accessor.type_parameters,
                        &accessor.parameters,
                        accessor.type_annotation,
                    );
                    self.check_static_member_body_for_class_type_param_refs(
                        accessor.body,
                        &filtered,
                    );
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(idx_sig) = self.ctx.arena.get_index_signature(node)
                    && self.has_static_modifier(&idx_sig.modifiers)
                {
                    self.check_type_node_for_class_type_param_refs(
                        idx_sig.type_annotation,
                        class_type_param_names,
                    );
                    for &param_idx in &idx_sig.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.check_type_node_for_class_type_param_refs(
                                param.type_annotation,
                                class_type_param_names,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn recheck_static_member_class_type_param_refs_in_node(&mut self, node_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, MODULE_BLOCK, MODULE_DECLARATION,
        };

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => {
                if let Some(class_data) = self.ctx.arena.get_class(node) {
                    let class_type_param_names =
                        self.collect_type_param_names(&class_data.type_parameters);
                    for &member_idx in &class_data.members.nodes {
                        self.check_static_member_for_class_type_param_refs_with_names(
                            member_idx,
                            &class_type_param_names,
                        );
                    }
                }
            }
            k if k == MODULE_DECLARATION => {
                if let Some(module) = self.ctx.arena.get_module(node) {
                    self.recheck_static_member_class_type_param_refs_in_node(module.body);
                }
            }
            k if k == MODULE_BLOCK => {
                if let Some(block) = self.ctx.arena.get_module_block(node)
                    && let Some(ref statements) = block.statements
                {
                    for &stmt_idx in &statements.nodes {
                        self.recheck_static_member_class_type_param_refs_in_node(stmt_idx);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn recheck_static_member_class_type_param_refs_in_source_file(
        &mut self,
        statements: &[NodeIndex],
    ) {
        for &stmt_idx in statements {
            self.recheck_static_member_class_type_param_refs_in_node(stmt_idx);
        }
    }

    /// Check a static class member for references to class type parameters (TS2302).
    pub(crate) fn check_static_member_for_class_type_param_refs(&mut self, member_idx: NodeIndex) {
        // Fast path: skip clone when there are no type params (common case)
        let has_type_params = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| !c.type_param_names.is_empty());
        if !has_type_params {
            return;
        }

        let class_type_param_names: Vec<String> = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.type_param_names.clone())
            .unwrap_or_default();

        self.check_static_member_for_class_type_param_refs_with_names(
            member_idx,
            &class_type_param_names,
        );
    }

    pub(crate) fn check_type_node_for_static_member_class_type_param_refs(
        &mut self,
        type_idx: NodeIndex,
    ) {
        // Fast path: skip clone when there are no type params (common case)
        let has_type_params = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| !c.type_param_names.is_empty());
        if !has_type_params {
            return;
        }

        let class_type_param_names: Vec<String> = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.type_param_names.clone())
            .unwrap_or_default();

        if class_type_param_names.is_empty() {
            return;
        }

        // Use the AST tree-walk to determine static context, not the
        // `in_static_member` flag. The flag can be stale when lazy type
        // resolution for instance properties is triggered from a static
        // method context.
        if !self.is_in_static_class_member_context(type_idx) {
            return;
        }

        self.check_type_node_for_class_type_param_refs(type_idx, &class_type_param_names);
    }

    /// Shared logic for checking a callable member (method/accessor) for class
    /// type parameter references in its parameters and return type (TS2302).
    fn check_callable_for_class_type_param_refs(
        &mut self,
        class_type_param_names: &[String],
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        parameters: &tsz_parser::parser::NodeList,
        type_annotation: NodeIndex,
    ) {
        // Exclude the member's own type parameters (they shadow class ones)
        let own_params = self.collect_type_param_names(type_parameters);
        let filtered: Vec<String> = class_type_param_names
            .iter()
            .filter(|n| !own_params.contains(n))
            .cloned()
            .collect();
        if filtered.is_empty() {
            return;
        }
        for &param_idx in &parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                self.check_type_node_for_class_type_param_refs(param.type_annotation, &filtered);
            }
        }
        self.check_type_node_for_class_type_param_refs(type_annotation, &filtered);
    }

    /// Check that all method overload signatures in a group share the same abstract modifier
    /// (TS2512: Overload signatures must all be abstract or non-abstract).
    pub(crate) fn check_abstract_overload_consistency(
        &mut self,
        members: &[tsz_parser::parser::NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let mut i = 0;
        while i < members.len() {
            let start_idx = i;
            let start_member_idx = members[start_idx];

            let Some(node) = self.ctx.arena.get(start_member_idx) else {
                i += 1;
                continue;
            };

            if node.kind != syntax_kind_ext::METHOD_DECLARATION {
                i += 1;
                continue;
            }

            let start_name = self.get_method_name_from_node(start_member_idx);
            if start_name.is_none() {
                i += 1;
                continue;
            }

            // Collect the group of methods with the same name
            let mut group = Vec::new();
            let mut impl_index_in_group = None;

            for &member_idx in members.iter().skip(start_idx) {
                let Some(cur_node) = self.ctx.arena.get(member_idx) else {
                    break;
                };
                if cur_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                    break;
                }

                let cur_name = self.get_method_name_from_node(member_idx);
                if cur_name != start_name {
                    break;
                }

                let Some(method) = self.ctx.arena.get_method_decl(cur_node) else {
                    break;
                };

                let is_abstract = self.has_abstract_modifier(&method.modifiers);
                let has_body = method.body.is_some();
                let error_node = method.name;

                if has_body && impl_index_in_group.is_none() {
                    impl_index_in_group = Some(group.len());
                }

                group.push((member_idx, is_abstract, has_body, error_node));
            }

            // Determine the "truth" abstractness for the group
            if group.len() > 1 {
                let truth_abstract = if let Some(idx) = impl_index_in_group {
                    group[idx].1
                } else {
                    group[0].1
                };

                // Report TS2512 for any signature that differs
                for &(_member_idx, is_abstract, _has_body, error_node) in &group {
                    if is_abstract != truth_abstract {
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_ABSTRACT_OR_NON_ABSTRACT,
                            diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_ABSTRACT_OR_NON_ABSTRACT,
                        );
                    }
                }
            }

            i += group.len();
        }
    }

    /// Check that all declarations of an abstract method are consecutive (TS2516).
    pub(crate) fn check_abstract_method_consecutive_declarations(
        &mut self,
        members: &[tsz_parser::parser::NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use rustc_hash::{FxHashMap, FxHashSet};
        use tsz_parser::parser::syntax_kind_ext;

        // Map from (method_name, is_static) to the node index of its FIRST abstract declaration.
        let mut first_abstract_decl: FxHashMap<(String, bool), tsz_parser::parser::NodeIndex> =
            FxHashMap::default();
        // Track the methods we've already emitted TS2516 for, to avoid duplicate errors.
        let mut reported_methods: FxHashSet<(String, bool)> = FxHashSet::default();

        let mut last_seen_method: Option<(String, bool)> = None;

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                last_seen_method = None;
                continue;
            };

            // Only care about method declarations.
            if node.kind != syntax_kind_ext::METHOD_DECLARATION {
                last_seen_method = None;
                continue;
            }

            let Some(method) = self.ctx.arena.get_method_decl(node) else {
                last_seen_method = None;
                continue;
            };

            let name = self.get_method_name_from_node(member_idx);
            let is_static = self.has_static_modifier(&method.modifiers);
            let is_abstract = self.has_abstract_modifier(&method.modifiers);

            if let Some(name_str) = name {
                let method_key = (name_str, is_static);

                if is_abstract {
                    if let Some(&first_decl_node) = first_abstract_decl.get(&method_key) {
                        // We have seen an abstract declaration of this method before.
                        // If the last seen method wasn't this one, we have a discontinuity!
                        if last_seen_method.as_ref() != Some(&method_key)
                            && reported_methods.insert(method_key.clone())
                        {
                            self.error_at_node(
                                        first_decl_node,
                                        diagnostic_messages::ALL_DECLARATIONS_OF_AN_ABSTRACT_METHOD_MUST_BE_CONSECUTIVE,
                                        diagnostic_codes::ALL_DECLARATIONS_OF_AN_ABSTRACT_METHOD_MUST_BE_CONSECUTIVE,
                                    );
                        }
                    } else {
                        // First time seeing an abstract declaration for this method key.
                        first_abstract_decl.insert(method_key.clone(), method.name);
                    }
                }

                // Update the last seen method key (even if it's non-abstract, as long as it's the same method group).
                last_seen_method = Some(method_key);
            } else {
                last_seen_method = None;
            }
        }
    }

    pub(crate) fn check_for_static_member_class_type_param_reference(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        error_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_binder::symbol_flags;

        // Must be in a class and inside a static member
        let Some(enclosing_class) = self.ctx.enclosing_class.as_ref() else {
            return;
        };

        // Use the AST tree-walk, not the in_static_member flag, to avoid
        // false TS2302 when lazy resolution is triggered from static context.
        if !self.is_in_static_class_member_context(error_node) {
            return;
        }

        // Must be a type parameter
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.flags & symbol_flags::TYPE_PARAMETER == 0 {
            return;
        }

        // Must be a type parameter of the enclosing class
        let class_sym_id = self
            .ctx
            .binder
            .node_symbols
            .get(&enclosing_class.class_idx.0)
            .copied();

        // Is sym_id a type parameter of class_sym?
        if let Some(class_sym) = class_sym_id
            && symbol.parent == class_sym
        {
            self.error_at_node(
                error_node,
                "Static members cannot reference class type parameters.",
                diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
            );
        }
    }
}
