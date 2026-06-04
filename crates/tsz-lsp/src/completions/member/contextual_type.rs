use super::*;

impl<'a> Completions<'a> {
    /// Get the set of property names already defined in an object literal.
    pub(super) fn get_defined_properties(
        &self,
        object_literal_idx: NodeIndex,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let node = self
            .arena
            .get(object_literal_idx)
            .expect("object_literal_idx must be valid in arena");

        if let Some(lit) = self.arena.get_literal_expr(node) {
            for &prop_idx in &lit.elements.nodes {
                if let Some(name) = self.get_property_name(prop_idx) {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// Extract the property name from a property assignment or shorthand.
    fn get_property_name(&self, prop_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(prop_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                self.arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)
            }
            _ => None,
        }
    }

    /// Walk up the AST to find the expected/contextual type for a node.
    pub(in crate::completions) fn get_contextual_type(
        &self,
        node_idx: NodeIndex,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        let parent = self.arena.get(parent_idx)?;

        match parent.kind {
            // const x: Type = { ... }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(parent)?;
                if decl.initializer == node_idx && decl.type_annotation.is_some() {
                    return Some(checker.get_type_of_node(decl.type_annotation));
                }
            }
            // { prop: { ... } } -> Recurse to parent object
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(parent)?;
                if prop.initializer == node_idx {
                    let grand_parent_ext = self.arena.get_extended(parent_idx)?;
                    let grand_parent_idx = grand_parent_ext.parent;

                    // Get context of the parent object
                    let parent_context = self.get_contextual_type(grand_parent_idx, checker)?;

                    // Look up this property in the parent context
                    let prop_name = self.arena.get_identifier_text(prop.name)?;
                    return self.lookup_property_type(parent_context, prop_name, checker);
                }
            }
            // return { ... }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let func_idx = self.find_enclosing_function(parent_idx)?;
                let func_node = self.arena.get(func_idx)?;

                // Check return type annotation
                if let Some(func) = self.arena.get_function(func_node)
                    && func.type_annotation.is_some()
                {
                    return Some(checker.get_type_of_node(func.type_annotation));
                }
            }
            // function call argument: foo({ ... })
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(parent)?;
                // Find which argument position this node is at
                let arg_index = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().position(|&arg| arg == node_idx));

                if let Some(arg_idx) = arg_index {
                    // Get the function signature type
                    let func_type = checker.get_type_of_node(call.expression);
                    if let Some(param_type) =
                        self.get_parameter_type_at(func_type, arg_idx, checker)
                    {
                        return Some(param_type);
                    }

                    if let Some(sym_id) = self.resolve_member_target_symbol(call.expression) {
                        let symbol_type = checker.get_type_of_symbol(sym_id);
                        if let Some(param_type) =
                            self.get_parameter_type_at(symbol_type, arg_idx, checker)
                        {
                            return Some(param_type);
                        }
                    }
                }
            }
            // assignment expression: target = { ... }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(parent)?;
                if binary.right == node_idx
                    && binary.operator_token == SyntaxKind::EqualsToken as u16
                {
                    return Some(checker.get_type_of_node(binary.left));
                }
            }
            _ => {}
        }
        None
    }

    /// Find the type of a property from an object type.
    fn lookup_property_type(
        &self,
        type_id: TypeId,
        name: &str,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let interner = self.interner?;

        self.collect_properties_for_type(type_id, interner, checker, &mut visited, &mut props);
        props.get(name).map(|p| p.type_id)
    }

    /// Find the enclosing function for a node (for return type lookup).
    fn find_enclosing_function(&self, start_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = start_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Get the type of the Nth parameter of a function type.
    fn get_parameter_type_at(
        &self,
        func_type: TypeId,
        param_index: usize,
        _checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let interner = self.interner?;

        // Look up the callable signature
        if let Some(callable_id) = visitor::callable_shape_id(interner, func_type) {
            let callable = interner.callable_shape(callable_id);
            // Use the first call signature
            if let Some(first_sig) = callable.call_signatures.first()
                && param_index < first_sig.params.len()
            {
                return Some(first_sig.params[param_index].type_id);
            }
        }
        None
    }
}
