impl<'a> CheckerState<'a> {
    /// Check if a `new` expression target has a declared type with generic
    /// construct signatures.
    ///
    /// For `new this.Map_<K, V>()` in a property initializer, `this.Map_` may
    /// resolve to `any` because the class is being constructed. But the member's
    /// DECLARED type (`{ new<K, V>(): any }`) has type parameters on its
    /// construct signature, so TS2347 should NOT fire.
    pub(crate) fn new_target_has_declared_generic_construct(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        // Must be a property access expression (e.g., this.Map_)
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };

        // The object must be `this`
        let Some(obj_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if obj_node.kind != SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let prop_name = &ident.escaped_text;

        // Find the enclosing class and check if the member has a declared
        // type with generic construct signatures
        // Walk up parents to find enclosing class
        let class_idx = {
            let mut current = expr_idx;
            let mut found = None;
            for _ in 0..100 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                let Some(parent) = self.ctx.arena.get(ext.parent) else {
                    break;
                };
                if parent.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    found = Some(ext.parent);
                    break;
                }
                current = ext.parent;
            }
            found
        };
        let Some(class_idx) = class_idx else {
            return false;
        };
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        // Check constructor parameters (parameter properties)
        if let Some(ctor_idx) = class_data.members.nodes.iter().find(|&&m| {
            self.ctx
                .arena
                .get(m)
                .is_some_and(|n| n.kind == syntax_kind_ext::CONSTRUCTOR)
        }) && let Some(ctor_node) = self.ctx.arena.get(*ctor_idx)
            && let Some(ctor) = self.ctx.arena.get_constructor(ctor_node)
        {
            for &param_idx in &ctor.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    // Check if this parameter has the matching name
                    let param_name = self
                        .ctx
                        .arena
                        .get(param.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());
                    if param_name == Some(prop_name.as_str()) {
                        // Check if the type annotation has generic construct sigs
                        if param.type_annotation.is_some() {
                            let param_type = self.get_type_from_type_node(param.type_annotation);
                            return self.type_has_generic_construct_signatures(param_type);
                        }
                    }
                }
            }
        }

        // Check regular property declarations
        for &member_idx in &class_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                && let Some(name) = self
                    .ctx
                    .arena
                    .get(prop.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                && name.escaped_text == *prop_name
                && prop.type_annotation.is_some()
            {
                let prop_type = self.get_type_from_type_node(prop.type_annotation);
                return self.type_has_generic_construct_signatures(prop_type);
            }
        }

        false
    }

    /// Check if a type has construct signatures with type parameters.
    fn type_has_generic_construct_signatures(&self, type_id: TypeId) -> bool {
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
        {
            return sigs.iter().any(|sig| !sig.type_params.is_empty());
        }
        // Also check callable shapes
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            return shape
                .construct_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
        }
        false
    }
}
