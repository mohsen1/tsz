//! Class member declaration and accessibility validation helpers.

use crate::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if let Some(async_mod_idx) = self.find_async_modifier(modifiers) {
            self.error_at_node(
                async_mod_idx,
                "'async' modifier cannot be used here.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
            );
        }
    }

    pub(crate) fn lookup_member_access_in_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> MemberLookup {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return MemberLookup::NotFound;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return MemberLookup::NotFound;
        };

        let mut accessor_access: Option<MemberAccessLevel> = None;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) != is_static {
                        continue;
                    }
                    let Some(prop_name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    if prop_name == name {
                        let access_level = if self.is_private_identifier_name(prop.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&prop.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) != is_static {
                        continue;
                    }
                    let Some(method_name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if method_name == name {
                        let access_level = if self.is_private_identifier_name(method.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&method.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) != is_static {
                        continue;
                    }
                    let Some(accessor_name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    if accessor_name == name {
                        let access_level = if self.is_private_identifier_name(accessor.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&accessor.modifiers)
                        };
                        // Don't return immediately - a getter/setter pair may have
                        // different visibility. Use the most permissive level (tsc
                        // allows reads when getter is public even if setter is private).
                        match access_level {
                            Some(MemberAccessLevel::Private) | None => return MemberLookup::Public,
                            Some(level) => {
                                accessor_access = Some(match accessor_access {
                                    // First accessor found
                                    None | Some(MemberAccessLevel::Private) => level,
                                    // If either accessor is non-private, use the most permissive level
                                    Some(prev) => prev,
                                });
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    if is_static {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(param_name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        if param_name == name {
                            return match self.member_access_level_from_modifiers(&param.modifiers) {
                                Some(level) => MemberLookup::Restricted(level),
                                None => MemberLookup::Public,
                            };
                        }
                    }
                }
                _ => {}
            }
        }

        // If we found accessor(s) but didn't early-return Public, return
        // the most permissive access level across getter/setter pair.
        if let Some(level) = accessor_access {
            return MemberLookup::Restricted(level);
        }

        MemberLookup::NotFound
    }

    pub(crate) fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self.get_class_name_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let base_idx = self.get_base_class_idx(current)?;
                    current = base_idx;
                }
            }
        }

        None
    }

    pub(crate) fn is_method_member_in_class_hierarchy(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<bool> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            let node = self.ctx.arena.get(current)?;
            let class = self.ctx.arena.get_class(node)?;

            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };

                match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&method.modifiers) != is_static {
                            continue;
                        }
                        if let Some(method_name) = self.get_property_name(method.name)
                            && method_name == name
                        {
                            return Some(true);
                        }
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&prop.modifiers) != is_static {
                            continue;
                        }
                        if let Some(prop_name) = self.get_property_name(prop.name)
                            && prop_name == name
                        {
                            // Auto-accessors (`accessor x`) behave like accessor members
                            // for super-property access and should not trigger TS2855.
                            if self.has_accessor_modifier(&prop.modifiers) {
                                return Some(true);
                            }
                            return Some(false);
                        }
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&accessor.modifiers) != is_static {
                            continue;
                        }
                        if let Some(accessor_name) = self.get_property_name(accessor.name)
                            && accessor_name == name
                        {
                            // In ES2015+, getters/setters are prototype methods accessible via super.
                            // In ES5/ES3, they are property descriptors and not accessible via super.
                            if self.ctx.compiler_options.target.is_es5() {
                                return Some(false);
                            }
                            return Some(true);
                        }
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        if is_static {
                            continue;
                        }
                        let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                            continue;
                        };
                        if ctor.body.is_none() {
                            continue;
                        }
                        for &param_idx in &ctor.parameters.nodes {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                continue;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                continue;
                            };
                            if !self.has_parameter_property_modifier(&param.modifiers) {
                                continue;
                            }
                            if let Some(param_name) = self.get_property_name(param.name)
                                && param_name == name
                            {
                                return Some(false);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let base_idx = self.get_base_class_idx(current)?;
            current = base_idx;
        }

        None
    }

    /// Recursively check a type node for parameter properties in function types.
    /// Function types (like `(x: T) => R` or `new (x: T) => R`) cannot have parameter properties.
    /// Walk a type node and emit TS2304 for unresolved type names inside complex types.
    /// Check type for missing names, but skip top-level `TYPE_REFERENCE` nodes.
    /// This is used when the caller will separately check `TYPE_REFERENCE` nodes
    /// to avoid duplicate error emissions.
    pub(crate) fn check_type_for_missing_names_skip_top_level_ref(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        use tsz_parser::parser::syntax_kind_ext;

        // Skip TYPE_REFERENCE at top level to avoid duplicates
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return;
        }

        // For all other types, use the normal check
        self.check_type_for_missing_names(type_idx);
    }

    pub(crate) fn check_type_for_missing_names(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };
        let factory = self.ctx.types.factory();

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let _ = self.get_type_from_type_reference(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let _ = self.get_type_from_type_query(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let updates =
                        self.push_missing_name_type_parameters(&func_type.type_parameters);
                    self.check_type_parameters_for_missing_names(&func_type.type_parameters);
                    self.check_duplicate_type_parameters(&func_type.type_parameters);
                    for &param_idx in &func_type.parameters.nodes {
                        self.check_parameter_type_for_missing_names(param_idx);
                    }
                    if !func_type.type_annotation.is_none() {
                        self.check_type_for_missing_names(func_type.type_annotation);
                    }
                    self.pop_type_parameters(updates);
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_for_missing_names(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_tuple_element_for_missing_names(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_for_missing_names(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Check check_type and extends_type first (infer type params not in scope yet)
                    self.check_type_for_missing_names(cond.check_type);
                    self.check_type_for_missing_names(cond.extends_type);

                    // Collect infer type parameters from extends_type and add them to scope for true_type
                    let infer_params = self.collect_infer_type_parameters(cond.extends_type);
                    let mut param_bindings = Vec::new();
                    for param_name in &infer_params {
                        let atom = self.ctx.types.intern_string(param_name);
                        let type_id = factory.type_param(tsz_solver::TypeParamInfo {
                            name: atom,
                            constraint: None,
                            default: None,
                            is_const: false,
                        });
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(param_name.clone(), type_id);
                        param_bindings.push((param_name.clone(), previous));
                    }

                    // Check true_type with infer type parameters in scope
                    self.check_type_for_missing_names(cond.true_type);

                    // Remove infer type parameters from scope
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }

                    // Check false_type (infer type params not in scope)
                    self.check_type_for_missing_names(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.check_type_for_missing_names(op.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_for_missing_names(indexed.object_type);
                    self.check_type_for_missing_names(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    // TS7039: Mapped object type implicitly has an 'any' template type.
                    if self.ctx.no_implicit_any() && mapped.type_node.is_none() {
                        let pos = node.pos;
                        let len = node.end.saturating_sub(node.pos);
                        self.ctx.error(
                            pos,
                            len,
                            "Mapped object type implicitly has an 'any' template type.".to_string(),
                            7039,
                        );
                    }
                    self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    let mut param_binding: Option<(String, Option<TypeId>)> = None;
                    if let Some(param_node) = self.ctx.arena.get(mapped.type_parameter)
                        && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                        && let Some(name_node) = self.ctx.arena.get(param.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        let atom = self.ctx.types.intern_string(&name);
                        let type_id = factory.type_param(tsz_solver::TypeParamInfo {
                            name: atom,
                            constraint: None,
                            default: None,
                            is_const: false,
                        });
                        let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                        param_binding = Some((name, previous));
                    }
                    if !mapped.name_type.is_none() {
                        self.check_type_for_missing_names(mapped.name_type);
                    }
                    if !mapped.type_node.is_none() {
                        self.check_type_for_missing_names(mapped.type_node);
                    } else if self.ctx.no_implicit_any() {
                        // TS7039: Mapped object type implicitly has an 'any' template type
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                            diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                        );
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    if let Some((name, previous)) = param_binding {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node)
                    && !pred.type_node.is_none()
                {
                    self.check_type_for_missing_names(pred.type_node);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_for_missing_names(span.expression);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn push_missing_name_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>)> {
        use tsz_solver::TypeParamInfo;

        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let factory = self.ctx.types.factory();
        let mut updates = Vec::new();
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = ident.escaped_text.clone();
            let atom = self.ctx.types.intern_string(&name);
            let type_id = factory.type_param(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
        }

        updates
    }

    pub(crate) fn check_type_member_for_missing_names(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            self.check_computed_property_name(sig.name);

            let updates = self.push_missing_name_type_parameters(&sig.type_parameters);
            self.check_type_parameters_for_missing_names(&sig.type_parameters);
            self.check_duplicate_type_parameters(&sig.type_parameters);
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
            if !sig.type_annotation.is_none() {
                self.check_type_for_missing_names(sig.type_annotation);
            }
            self.pop_type_parameters(updates);
            return;
        }

        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            for &param_idx in &index_sig.parameters.nodes {
                self.check_parameter_type_for_missing_names(param_idx);
            }
            if !index_sig.type_annotation.is_none() {
                self.check_type_for_missing_names(index_sig.type_annotation);
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    pub(crate) fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Check call signatures and construct signatures for parameter properties
        if node.kind == syntax_kind_ext::CALL_SIGNATURE
            || node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
        {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_strict_mode_reserved_parameter_names(
                        &params.nodes,
                        member_idx,
                        false,
                    );
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(sig.type_annotation);

                // TS7013/TS7020: Check for implicit any return type on construct/call signatures
                if self.ctx.no_implicit_any() && sig.type_annotation.is_none() {
                    use crate::diagnostics::diagnostic_codes;
                    if node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE {
                        self.error_at_node(
                            member_idx,
                            "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type.",
                            diagnostic_codes::CONSTRUCT_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RET,
                        );
                    } else {
                        self.error_at_node(
                            member_idx,
                            "Call signature, which lacks return-type annotation, implicitly has an 'any' return type.",
                            diagnostic_codes::CALL_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_T,
                        );
                    }
                }
            }
        }
        // Check method signatures in type literals
        else if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_strict_mode_reserved_parameter_names(
                        &params.nodes,
                        member_idx,
                        false,
                    );
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                self.check_type_for_parameter_properties(sig.type_annotation);
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(name) = self.property_name_for_error(sig.name)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        &[&name, "any"],
                    );
                }
            }
        }
        // Check property signatures for implicit any (error 7008)
        else if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if !sig.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(sig.type_annotation);
                }
                // Property signature without type annotation implicitly has 'any' type
                // Only emit TS7008 when noImplicitAny is enabled
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(member_name) = self.get_property_name(sig.name)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                        &[&member_name, "any"],
                    );
                }
            }
        }
        // Check accessors in type literals/interfaces - cannot have body (error 1183)
        else if (node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.ctx.arena.get_accessor(node)
        {
            // Accessors in type literals and interfaces cannot have implementations
            if !accessor.body.is_none() {
                use crate::diagnostics::diagnostic_codes;
                // Report error on the body
                self.error_at_node(
                    accessor.body,
                    "An implementation cannot be declared in ambient contexts.",
                    diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
            }
        }
    }

    /// Check that all method/constructor overload signatures have implementations.
    /// Reports errors 2389, 2390, 2391, 1042.
    pub(crate) fn check_class_member_implementations(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                i += 1;
                continue;
            };

            match node.kind {
                // TS1042: 'async' modifier cannot be used on getters/setters
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::CONSTRUCTOR => {
                    if let Some(ctor) = self.ctx.arena.get_constructor(node)
                        && ctor.body.is_none()
                    {
                        // Constructor overload signature - check for implementation
                        let has_impl = self.find_constructor_impl(members, i + 1);
                        if !has_impl {
                            self.error_at_node(
                                member_idx,
                                "Constructor implementation is missing.",
                                diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_IS_MISSING,
                            );
                        }
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        let flags = u32::from(node.flags);
                        if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                            || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                        {
                            continue;
                        }
                        // Abstract methods don't need implementations (they're meant for derived classes)
                        let is_abstract = self.has_abstract_modifier(&method.modifiers);
                        if method.body.is_none() && !is_abstract {
                            // Method overload signature - check for implementation
                            let method_name = self.get_method_name_from_node(member_idx);
                            // TSC reports at the method name node, not the declaration
                            let error_node = if !method.name.is_none() {
                                method.name
                            } else {
                                member_idx
                            };
                            if let Some(name) = method_name {
                                let (has_impl, impl_name, impl_idx) =
                                    self.find_method_impl(members, i + 1, &name);
                                if !has_impl {
                                    self.error_at_node(
                                        error_node,
                                        "Function implementation is missing or not immediately following the declaration.",
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                    );
                                } else if let Some(actual_name) = impl_name
                                    && actual_name != name
                                {
                                    // Implementation has wrong name — report at the
                                    // implementation's name node, and only on the last
                                    // overload (the one immediately preceding the implementation).
                                    let impl_member_idx = impl_idx.unwrap_or(i + 1);
                                    if impl_member_idx == i + 1 {
                                        let impl_node_idx = members[impl_member_idx];
                                        let expected_display = self
                                            .get_method_name_for_diagnostic(member_idx)
                                            .unwrap_or_else(|| name.clone());
                                        let impl_error_node = self
                                            .ctx
                                            .arena
                                            .get(impl_node_idx)
                                            .and_then(|n| self.ctx.arena.get_method_decl(n))
                                            .map(|m| m.name)
                                            .filter(|n| !n.is_none())
                                            .unwrap_or(impl_node_idx);
                                        self.error_at_node(
                                            impl_error_node,
                                            &format!(
                                                "Function implementation name must be '{expected_display}'."
                                            ),
                                            diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    pub(crate) fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // Skip parameters that have explicit type annotations
        if !param.type_annotation.is_none() {
            return;
        }
        // Check if parameter has an initializer
        if !param.initializer.is_none() {
            // TypeScript infers type from initializer, EXCEPT for null and undefined
            // Parameters initialized with null/undefined still trigger TS7006
            use tsz_scanner::SyntaxKind;
            let initializer_is_null_or_undefined =
                if let Some(init_node) = self.ctx.arena.get(param.initializer) {
                    init_node.kind == SyntaxKind::NullKeyword as u16
                        || init_node.kind == SyntaxKind::UndefinedKeyword as u16
                } else {
                    false
                };

            // Skip only if initializer is NOT null or undefined
            if !initializer_is_null_or_undefined {
                return;
            }
            // Otherwise continue to emit TS7006 for null/undefined initializers
        }
        if self.is_this_parameter_name(param.name) {
            return;
        }
        if self.is_parameter_in_promise_executor(param.name) {
            return;
        }
        if self.is_parameter_in_iife(param.name) {
            return;
        }

        // Enhanced destructuring parameter detection
        // Check if the parameter name is a destructuring pattern (object/array binding)
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let kind = name_node.kind;

            // Direct destructuring patterns
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // For destructuring parameters, recursively check nested binding elements
                self.emit_implicit_any_parameter_for_pattern(param.name, param.dot_dot_dot_token);
                return;
            }
        }

        // Skip TS7006 for parameters on nodes with parse errors.
        // This prevents cascading "implicitly has any type" errors on malformed AST nodes.
        // The parse error itself should already be emitted (e.g., TS1005, TS2390).
        use tsz_parser::parser::node_flags;
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let flags = name_node.flags as u32;
            if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Skip if the parameter name is empty (parse recovery artifact)
        if param_name.is_empty() {
            return;
        }

        // Rest parameters use TS7019, regular parameters use TS7006
        let report_node = self.param_node_for_implicit_any_diagnostic(param);
        if param.dot_dot_dot_token {
            self.error_at_node_msg(
                report_node,
                diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                &[&param_name],
            );
        } else {
            self.error_at_node_msg(
                report_node,
                diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                &[&param_name, "any"],
            );
        }
    }

    fn param_node_for_implicit_any_diagnostic(
        &self,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> NodeIndex {
        let Some(modifiers) = param.modifiers.as_ref() else {
            return param.name;
        };
        use tsz_scanner::SyntaxKind;
        for &mod_idx in &modifiers.nodes {
            let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::PublicKeyword as u16
                || mod_node.kind == SyntaxKind::PrivateKeyword as u16
                || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                || mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
            {
                return mod_idx;
            }
        }
        param.name
    }

    /// Emit TS7006 errors for nested binding elements in destructuring parameters.
    /// TypeScript reports implicit 'any' for individual bindings in patterns like:
    ///   function foo({ x, y }: any) {}  // no error on x, y with type annotation
    ///   function bar({ x, y }) {}        // errors on x and y
    pub(crate) fn emit_implicit_any_parameter_for_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        // Handle object binding patterns: { x, y, z }
        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    if let Some(element_node) = self.ctx.arena.get(element_idx) {
                        // Skip omitted expressions
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            continue;
                        }

                        if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                        {
                            // Check if this binding element has an initializer
                            let has_initializer = !binding_elem.initializer.is_none();

                            // If no initializer, report error for implicit any
                            if !has_initializer {
                                // Get the property name (could be identifier or string literal)
                                let binding_name = if !binding_elem.property_name.is_none() {
                                    self.parameter_name_for_error(binding_elem.property_name)
                                } else {
                                    self.parameter_name_for_error(binding_elem.name)
                                };

                                let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                                self.error_at_node_msg(
                                    binding_elem.name,
                                    diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                    &[&binding_name, implicit_type],
                                );
                            }

                            // Recursively check nested patterns
                            if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                                let name_kind = name_node.kind;
                                if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                {
                                    self.emit_implicit_any_parameter_for_pattern(
                                        binding_elem.name,
                                        is_rest_parameter,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // Handle array binding patterns: [ x, y, z ]
        else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    let element_kind = element_node.kind;

                    // Skip omitted expressions (holes in array patterns)
                    if element_kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    // Check if this element is a binding element with initializer
                    if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) {
                        let has_initializer = !binding_elem.initializer.is_none();

                        if !has_initializer {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);

                            let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                            self.error_at_node_msg(
                                binding_elem.name,
                                diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                &[&binding_name, implicit_type],
                            );
                        }

                        // Recursively check nested patterns
                        if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                            let name_kind = name_node.kind;
                            if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            {
                                self.emit_implicit_any_parameter_for_pattern(
                                    binding_elem.name,
                                    is_rest_parameter,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Report an error at a specific node.
    /// Check an expression node for TS1359: await outside async function.
    /// Recursively checks the expression tree for await expressions.
    /// Report an error with context about a related symbol.
    /// Check a class member (property, method, constructor, accessor).
    pub(crate) fn check_class_member(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let mut pushed_this = false;
        if let Some(this_type) = self.class_member_this_type(member_idx) {
            self.ctx.this_type_stack.push(this_type);
            pushed_this = true;
        }

        self.check_class_member_name(member_idx);
        self.check_class_member_decorator_expressions(member_idx);

        // TS2302: Static members cannot reference class type parameters
        self.check_static_member_for_class_type_param_refs(member_idx);

        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.check_property_declaration(member_idx);
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.check_method_declaration(member_idx);
            }
            syntax_kind_ext::CONSTRUCTOR => {
                self.check_constructor_declaration(member_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.check_accessor_declaration(member_idx);
            }
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                // Static blocks contain statements that must be type-checked
                if let Some(block) = self.ctx.arena.get_block(node) {
                    // Check for unreachable code in the static block
                    self.check_unreachable_code_in_block(&block.statements.nodes);

                    // Check each statement in the block
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement(stmt_idx);
                    }
                }
            }
            syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are metadata used during type resolution, not members
                // with their own types. They're handled separately by get_index_signatures.
                // Nothing to check here.
            }
            _ => {
                // Other class member types (semicolons, etc.)
                self.get_type_of_node(member_idx);
            }
        }

        if pushed_this {
            self.ctx.this_type_stack.pop();
        }
    }

    fn check_class_member_decorator_expressions(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let modifiers = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .and_then(|p| p.modifiers.as_ref()),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .and_then(|m| m.modifiers.as_ref()),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .and_then(|a| a.modifiers.as_ref()),
            k if k == syntax_kind_ext::CONSTRUCTOR => self
                .ctx
                .arena
                .get_constructor(node)
                .and_then(|c| c.modifiers.as_ref()),
            _ => None,
        };

        let Some(modifiers) = modifiers else {
            return;
        };

        for &modifier_idx in &modifiers.nodes {
            let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                continue;
            };
            if modifier_node.kind != syntax_kind_ext::DECORATOR {
                continue;
            }

            let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) else {
                continue;
            };
            self.get_type_of_node(decorator.expression);
        }
    }

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

        match node.kind {
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

    /// Check a static class member for references to class type parameters (TS2302).
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

    /// Check a static class member for references to class type parameters (TS2302).
    fn check_static_member_for_class_type_param_refs(&mut self, member_idx: NodeIndex) {
        let class_type_param_names: Vec<String> = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.type_param_names.clone())
            .unwrap_or_default();

        if class_type_param_names.is_empty() {
            return;
        }

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
                        &class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node)
                    && self.has_static_modifier(&method.modifiers)
                {
                    // Exclude the method's own type parameters (they shadow class ones)
                    let own_params = self.collect_type_param_names(&method.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|n| !own_params.contains(n))
                        .cloned()
                        .collect();
                    if filtered.is_empty() {
                        return;
                    }
                    for &param_idx in &method.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.check_type_node_for_class_type_param_refs(
                                param.type_annotation,
                                &filtered,
                            );
                        }
                    }
                    self.check_type_node_for_class_type_param_refs(
                        method.type_annotation,
                        &filtered,
                    );
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && self.has_static_modifier(&accessor.modifiers)
                {
                    // Exclude the accessor's own type parameters (they shadow class ones)
                    let own_params = self.collect_type_param_names(&accessor.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|n| !own_params.contains(n))
                        .cloned()
                        .collect();
                    if filtered.is_empty() {
                        return;
                    }
                    for &param_idx in &accessor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.check_type_node_for_class_type_param_refs(
                                param.type_annotation,
                                &filtered,
                            );
                        }
                    }
                    self.check_type_node_for_class_type_param_refs(
                        accessor.type_annotation,
                        &filtered,
                    );
                }
            }
            _ => {}
        }
    }
}
