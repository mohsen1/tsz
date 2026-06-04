impl<'a> CheckerState<'a> {
    pub(crate) fn type_arg_explicit_constraint_node_in_ast(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        // Get the type argument's name to look up in the type_parameter_scope.
        // Function type params (e.g., `<T extends Foo>(x: Bar<T>)`) are stored
        // in the checker's dynamic scope, not the binder's symbol table.
        let arg_name = self.type_arg_identifier_name(arg_idx);
        // Also check binder symbols for interface/class type params
        let sym_id = if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
            let target = if arg_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
                self.ctx
                    .arena
                    .get_type_ref(arg_node)
                    .map_or(arg_idx, |tr| tr.type_name)
            } else {
                arg_idx
            };
            self.resolve_type_symbol_for_lowering(target)
                .map(tsz_binder::SymbolId)
        } else {
            None
        };

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER != 0
        {
            for &decl_idx in &symbol.declarations {
                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_PARAMETER
                    && let Some(tp_data) = self.ctx.arena.get_type_parameter(decl_node)
                    && tp_data.constraint.is_some()
                {
                    return Some(tp_data.constraint);
                }
            }
        }

        // Walk up the AST to find enclosing function/constructor/signature types whose
        // type parameter list declares this name with a constraint. This
        // handles function type params not in the binder's symbol table.
        if let Some(ref name) = arg_name {
            let mut current = arg_idx;
            for _ in 0..30 {
                let parent = self
                    .ctx
                    .arena
                    .get_extended(current)
                    .map_or(NodeIndex::NONE, |e| e.parent);
                if parent.is_none() {
                    break;
                }
                if let Some(pn) = self.ctx.arena.get(parent) {
                    // Check function types and constructor types
                    let tp_list = if pn.kind
                        == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                    {
                        self.ctx
                            .arena
                            .get_function(pn)
                            .and_then(|func| func.type_parameters.as_ref())
                    } else if pn.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_TYPE
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE
                    {
                        self.ctx
                            .arena
                            .get_function_type(pn)
                            .and_then(|ft| ft.type_parameters.as_ref())
                    } else if pn.kind == tsz_parser::parser::syntax_kind_ext::CALL_SIGNATURE
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCT_SIGNATURE
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::METHOD_SIGNATURE
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION
                    {
                        // Call signatures, construct signatures, and method
                        // signatures/declarations in interfaces and classes can
                        // also declare type parameters with constraints.
                        self.ctx
                            .arena
                            .get_signature(pn)
                            .and_then(|sig| sig.type_parameters.as_ref())
                    } else if pn.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    {
                        self.ctx
                            .arena
                            .get_type_alias(pn)
                            .and_then(|alias| alias.type_parameters.as_ref())
                    } else {
                        None
                    };
                    if let Some(tp_list) = tp_list {
                        for &tp_idx in &tp_list.nodes {
                            if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                                && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                                && let Some(nm) = self.ctx.arena.get(tp_data.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(nm)
                                && ident.escaped_text == *name
                                && tp_data.constraint.is_some()
                            {
                                return Some(tp_data.constraint);
                            }
                        }
                    }
                    // Stop at declaration boundaries
                    if pn.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                        || pn.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    {
                        break;
                    }
                    if pn.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION {
                        // For merged interfaces, check if any OTHER declaration of the same
                        // interface has a constraint on the type parameter at the same position.
                        // e.g., `interface B<T extends number> { ... }` merged with
                        // `interface B<T> { ... }` — `T` is effectively constrained.
                        if let Some(iface) = self.ctx.arena.get_interface(pn)
                            && let Some(ref tp_list) = iface.type_parameters
                        {
                            // Find the position index of this type parameter in the current declaration
                            if let Some(tp_pos) = tp_list.nodes.iter().position(|&tp_idx| {
                                self.ctx
                                    .arena
                                    .get(tp_idx)
                                    .and_then(|tp_node| self.ctx.arena.get_type_parameter(tp_node))
                                    .and_then(|tp_data| self.ctx.arena.get(tp_data.name))
                                    .and_then(|nm| self.ctx.arena.get_identifier(nm))
                                    .is_some_and(|ident| &ident.escaped_text == name)
                            }) {
                                // Look up the interface symbol and check other declarations
                                let iface_name_idx = iface.name;
                                if let Some(iface_sym_id) =
                                    self.ctx.binder.get_node_symbol(iface_name_idx).or_else(|| {
                                        self.ctx
                                            .arena
                                            .get(iface_name_idx)
                                            .and_then(|n| self.ctx.arena.get_identifier(n))
                                            .and_then(|ident| {
                                                self.ctx.binder.file_locals.get(&ident.escaped_text)
                                            })
                                    })
                                    && let Some(iface_symbol) =
                                        self.ctx.binder.get_symbol(iface_sym_id)
                                {
                                    for &decl_idx in &iface_symbol.declarations {
                                        if decl_idx == parent {
                                            continue; // Skip current declaration
                                        }
                                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                            && decl_node.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                                            && let Some(other_iface) = self.ctx.arena.get_interface(decl_node)
                                            && let Some(ref other_tp_list) = other_iface.type_parameters
                                            && let Some(&other_tp_idx) = other_tp_list.nodes.get(tp_pos)
                                            && let Some(other_tp_node) = self.ctx.arena.get(other_tp_idx)
                                            && let Some(other_tp_data) = self.ctx.arena.get_type_parameter(other_tp_node)
                                            && other_tp_data.constraint.is_some()
                                        {
                                            return Some(other_tp_data.constraint);
                                        }
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
                current = parent;
            }
        }

        None
    }

    /// Extract the identifier name from a type argument AST node.
    pub(crate) fn type_arg_identifier_name(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_node = self.ctx.arena.get(arg_idx)?;
        if arg_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
            let tr = self.ctx.arena.get_type_ref(arg_node)?;
            let name_node = self.ctx.arena.get(tr.type_name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        } else if arg_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(arg_node)?;
            Some(ident.escaped_text.clone())
        } else {
            None
        }
    }

    /// Check if a type argument references an `infer` variable declared in a
    /// position with an implicit constraint within a conditional type's extends
    /// clause. In TSC, such infer variables get implicit constraints from their
    /// structural position:
    /// - Rest position (`...infer X`): implicit array constraint
    /// - Template literal position (`` `${infer X}` ``): implicit `string` constraint
    ///
    /// We should skip TS2344 constraint checking for these.
    pub(crate) fn is_infer_with_implicit_constraint_in_conditional(
        &self,
        arg_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Get the name of the type argument (e.g., "Tail" from `ExpandSmallerTuples<Tail>`)
        let arg_name = self.type_arg_identifier_name(arg_idx);
        let Some(ref name) = arg_name else {
            return false;
        };

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        // Walk up to find an enclosing conditional type
        let mut current = arg_idx;
        for _ in 0..30 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |e| e.parent);
            if parent.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node) {
                    // Check if arg_idx is in the true branch of this conditional
                    // (use position-based containment)
                    if let Some(true_node) = self.ctx.arena.get(cond.true_type)
                        && arg_node.pos >= true_node.pos
                        && arg_node.end <= true_node.end
                    {
                        // Search the extends clause for `...infer <name>`
                        if self.extends_clause_has_constrained_infer_named(cond.extends_type, name)
                        {
                            return true;
                        }
                    }
                }
                // Stop at declaration-level nodes
                if parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    return false;
                }
            }
            current = parent;
        }
        false
    }

    /// Recursively search a type node for `infer <name>` patterns in positions
    /// with implicit or explicit constraints:
    /// - `infer <name> extends <constraint>` (explicit extends constraint)
    /// - `...infer <name>` (rest position → implicit array constraint)
    /// - `` `...${infer <name>}...` `` (template literal → implicit `string` constraint)
    ///
    /// Returns true if a matching infer with a constraint is found.
    fn extends_clause_has_constrained_infer_named(&self, node_idx: NodeIndex, name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this is an INFER_TYPE with an explicit `extends` constraint
        // e.g., `infer Head extends DistributedKeyOf<ObjT>`
        if node.kind == syntax_kind_ext::INFER_TYPE
            && let Some(infer_data) = self.ctx.arena.get_infer_type(node)
            && self.infer_type_param_has_name(infer_data, name)
            && let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
            && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
            && tp_data.constraint != NodeIndex::NONE
        {
            return true;
        }

        // Check if this is a REST_TYPE wrapping an INFER_TYPE
        if node.kind == syntax_kind_ext::REST_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            && let Some(inner_node) = self.ctx.arena.get(wrapped.type_node)
            && inner_node.kind == syntax_kind_ext::INFER_TYPE
            && let Some(infer_data) = self.ctx.arena.get_infer_type(inner_node)
            && self.infer_type_param_has_name(infer_data, name)
        {
            return true;
        }

        // Check if this is a TEMPLATE_LITERAL_TYPE containing `infer <name>` in a span.
        // Template literal type spans constrain infer variables to `string`.
        if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE
            && let Some(tlt) = self.ctx.arena.get_template_literal_type(node)
        {
            for &span_idx in &tlt.template_spans.nodes {
                if let Some(span_node) = self.ctx.arena.get(span_idx)
                    && let Some(span_data) = self.ctx.arena.get_template_span(span_node)
                {
                    // The expression/type in the span is at span_data.expression
                    if let Some(type_node) = self.ctx.arena.get(span_data.expression)
                        && type_node.kind == syntax_kind_ext::INFER_TYPE
                        && let Some(infer_data) = self.ctx.arena.get_infer_type(type_node)
                        && self.infer_type_param_has_name(infer_data, name)
                    {
                        return true;
                    }
                }
            }
        }

        // Recurse into tuple type elements
        if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
            for &elem_idx in &tuple.elements.nodes {
                if self.extends_clause_has_constrained_infer_named(elem_idx, name) {
                    return true;
                }
            }
        }

        // Recurse into named tuple members
        if let Some(named_member) = self.ctx.arena.get_named_tuple_member(node)
            && self.extends_clause_has_constrained_infer_named(named_member.type_node, name)
        {
            return true;
        }

        // Recurse into wrapped types (parenthesized, optional, rest)
        if (node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || node.kind == syntax_kind_ext::OPTIONAL_TYPE
            || node.kind == syntax_kind_ext::REST_TYPE)
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            && self.extends_clause_has_constrained_infer_named(wrapped.type_node, name)
        {
            return true;
        }

        // Recurse into type operators (readonly T)
        if node.kind == syntax_kind_ext::TYPE_OPERATOR
            && let Some(op) = self.ctx.arena.get_type_operator(node)
            && self.extends_clause_has_constrained_infer_named(op.type_node, name)
        {
            return true;
        }

        // Recurse into type reference type arguments (Foo<infer T extends X>)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && let Some(ref args) = type_ref.type_arguments
        {
            for &arg_idx in &args.nodes {
                if self.extends_clause_has_constrained_infer_named(arg_idx, name) {
                    return true;
                }
            }
        }

        // Recurse into union/intersection types
        if (node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE)
            && let Some(composite) = self.ctx.arena.get_composite_type(node)
        {
            for &member_idx in &composite.types.nodes {
                if self.extends_clause_has_constrained_infer_named(member_idx, name) {
                    return true;
                }
            }
        }

        // Recurse into function/constructor types (parameters and return type)
        if (node.kind == syntax_kind_ext::FUNCTION_TYPE
            || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE)
            && let Some(func_type) = self.ctx.arena.get_function_type(node)
        {
            for &param_idx in &func_type.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    && param.type_annotation != NodeIndex::NONE
                {
                    // Rest parameters (...args: infer A) have the annotation as bare
                    // INFER_TYPE (no REST_TYPE wrapper). The rest position implies an
                    // implicit `unknown[]` constraint — treat it as constrained.
                    if param.dot_dot_dot_token
                        && let Some(annotation_node) = self.ctx.arena.get(param.type_annotation)
                        && annotation_node.kind == syntax_kind_ext::INFER_TYPE
                        && let Some(infer_data) = self.ctx.arena.get_infer_type(annotation_node)
                        && self.infer_type_param_has_name(infer_data, name)
                    {
                        return true;
                    }
                    if self.extends_clause_has_constrained_infer_named(param.type_annotation, name)
                    {
                        return true;
                    }
                }
            }
            if func_type.type_annotation.is_some()
                && self.extends_clause_has_constrained_infer_named(func_type.type_annotation, name)
            {
                return true;
            }
        }

        // Recurse into object/type literal members
        if node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_lit) = self.ctx.arena.get_type_literal(node)
        {
            for &member_idx in &type_lit.members.nodes {
                if self.extends_clause_has_constrained_infer_named(member_idx, name) {
                    return true;
                }
            }
        }

        // Recurse into array types
        if node.kind == syntax_kind_ext::ARRAY_TYPE
            && let Some(array_type) = self.ctx.arena.get_array_type(node)
            && self.extends_clause_has_constrained_infer_named(array_type.element_type, name)
        {
            return true;
        }

        // Recurse into conditional types
        if node.kind == syntax_kind_ext::CONDITIONAL_TYPE
            && let Some(cond) = self.ctx.arena.get_conditional_type(node)
            && (self.extends_clause_has_constrained_infer_named(cond.check_type, name)
                || self.extends_clause_has_constrained_infer_named(cond.extends_type, name)
                || self.extends_clause_has_constrained_infer_named(cond.true_type, name)
                || self.extends_clause_has_constrained_infer_named(cond.false_type, name))
        {
            return true;
        }

        false
    }

    /// Check if an infer type's type parameter has the given name.
    fn infer_type_param_has_name(
        &self,
        infer_data: &tsz_parser::parser::node::InferTypeData,
        name: &str,
    ) -> bool {
        if let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
            && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
            && let Some(name_node) = self.ctx.arena.get(tp_data.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            ident.escaped_text == name
        } else {
            false
        }
    }

    /// Returns the type parameter a class extends if the class adds no new members at all.
    pub(crate) fn get_extends_type_parameter_if_transparent(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<TypeId> {
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        let mut extends_type_param = None;
        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;

            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;

            // Handle ExpressionWithTypeArguments
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            let base_type = self.get_type_of_node(expr_idx);

            if query::is_type_parameter_like(self.ctx.types, base_type) {
                extends_type_param = Some(base_type);
                break;
            }
        }

        let base_type_param = extends_type_param?;

        // Class is transparent only if it adds no new members at all (no instance, no static).
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::CONSTRUCTOR => continue,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    return None;
                }
                // Index signatures, abstract members, and other node kinds are
                // conservatively skipped; if they prove non-transparent in practice,
                // add them to the return-None arm above.
                _ => continue,
            }
        }

        // Class is transparent - return the type parameter
        Some(base_type_param)
    }
}
