impl<'a> CheckerState<'a> {
    pub(super) fn check_jsdoc_enum_initializer_values(
        &mut self,
        initializer_idx: NodeIndex,
        enum_element_type: TypeId,
    ) {
        if enum_element_type == TypeId::ERROR || enum_element_type == TypeId::ANY {
            return;
        }
        // tsc per-member validates `@enum {T}` only when the initializer is a
        // direct object literal. `Object.freeze({...})` and other call/wrapper
        // expressions opt out: the value type stays as the inferred object
        // literal type and any downstream `T`-typed use site emits its own
        // diagnostic at the use site.
        let object_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);
        let object_node = match self.ctx.arena.get(object_idx) {
            Some(n) if n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => n,
            _ => return,
        };
        let Some(literal) = self.ctx.arena.get_literal_expr(object_node) else {
            return;
        };
        // Property assignments only: methods, shorthand, accessors, and
        // spread elements aren't value-positioned literals participating in
        // tsc's per-member enum-element validation.
        let property_pairs: Vec<(NodeIndex, NodeIndex)> = literal
            .elements
            .nodes
            .iter()
            .filter_map(|&element_idx| {
                let element_node = self.ctx.arena.get(element_idx)?;
                let property = self.ctx.arena.get_property_assignment(element_node)?;
                Some((property.name, property.initializer))
            })
            .collect();

        let request = TypingRequest::with_contextual_type(enum_element_type);
        for (prop_name_idx, prop_value_idx) in property_pairs {
            let value_type = self.get_type_of_node_with_request(prop_value_idx, &request);
            let value_type = self.resolve_lazy_type(value_type);
            if value_type == TypeId::ERROR || value_type == TypeId::ANY {
                continue;
            }
            // Match tsc's `elaborateElementwise`: anchor TS2322 at the property
            // name with the offending value's type vs the enum element type.
            // The default `check_assignable_or_report` path uses
            // `DiagnosticAnchorKind::RewriteAssignment`, which walks up to the
            // variable declaration and reformats the source as the whole
            // object-literal type — emitting one merged `{ … }` vs `T` error
            // at the binding name. tsc instead reports per-member.
            self.check_assignable_or_report_at_exact_anchor_without_source_elaboration(
                value_type,
                enum_element_type,
                prop_value_idx,
                prop_name_idx,
            );
        }
    }
}
