use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn infer_type_from_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> TypeId {
        self.infer_type_from_binding_pattern_with_request(
            pattern_idx,
            parent_type,
            &TypingRequest::NONE,
        )
    }

    pub(crate) fn infer_type_from_binding_pattern_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return TypeId::ANY;
        };

        let factory = self.ctx.types.factory();

        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
                return TypeId::ANY;
            };

            let mut properties = Vec::new();

            for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
                if let Some(element_node) = self.ctx.arena.get(element_idx)
                    && let Some(element_data) = self.ctx.arena.get_binding_element(element_node)
                {
                    // Skip rest elements — `...rest` in `{a, ...rest}` is not a named property;
                    // it represents remaining properties and should not appear in the contextual type.
                    if element_data.dot_dot_dot_token {
                        continue;
                    }

                    // Compute property name
                    let name_str = if element_data.property_name.is_some() {
                        let prop_name_idx = element_data.property_name;
                        if let Some(prop_name_node) = self.ctx.arena.get(prop_name_idx) {
                            if let Some(ident) = self.ctx.arena.get_identifier(prop_name_node) {
                                ident.escaped_text.clone()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            ident.escaped_text.clone()
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    let atom = self.ctx.types.intern_string(&name_str);

                    // When the parent type is `unknown` (e.g. from a rest type
                    // parameter constraint `Args extends unknown[]`), skip
                    // property lookup to avoid a false TS2339.  The binding
                    // pattern is synthesising a contextual type here, not
                    // validating existing properties.
                    let mut element_type = if parent_type == TypeId::UNKNOWN {
                        TypeId::ANY
                    } else {
                        self.get_binding_element_type_with_request(
                            pattern_idx,
                            i,
                            parent_type,
                            element_data,
                            request,
                        )
                    };

                    if element_data.initializer.is_some() {
                        // Set contextual type for initializers so that:
                        // - Arrow/function parameters get inferred from the element type
                        // - Literal defaults preserve their literal type (e.g., "foo"
                        //   stays "foo" for assignability checks against union types)
                        // The first evaluation caches the type, so contextual typing
                        // must be set here to ensure the cached type is correct.
                        let request =
                            if element_type != TypeId::ANY && element_type != TypeId::UNKNOWN {
                                request.read().contextual(element_type)
                            } else {
                                request.read().contextual_opt(None)
                            };
                        let init_type =
                            self.get_type_of_node_with_request(element_data.initializer, &request);
                        if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                            element_type = init_type;
                        } else if !self.is_assignable_to(init_type, element_type) {
                            element_type = self.ctx.types.factory().union2(element_type, init_type);
                        }
                    } else if element_type == TypeId::ANY
                        && let Some(name_node) = self.ctx.arena.get(element_data.name)
                        && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                    {
                        element_type = self.infer_type_from_binding_pattern_with_request(
                            element_data.name,
                            element_type,
                            request,
                        );
                    }

                    let is_optional =
                        element_data.initializer.is_some() || element_data.dot_dot_dot_token;

                    let mut prop_info = tsz_solver::PropertyInfo::new(atom, element_type);
                    prop_info.optional = is_optional;
                    properties.push(prop_info);
                }
            }
            // An empty object binding pattern `{}` provides no structural constraints.
            // Return ANY so callers (e.g. function_type.rs) fall back to the initializer
            // type (`any`, `{a: number}`, etc.) instead of synthesising a useless `{}`.
            if properties.is_empty() {
                return TypeId::ANY;
            }
            return factory.object(properties);
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
                return TypeId::ANY;
            };

            let mut elements = Vec::new();

            for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };

                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    elements.push(tsz_solver::TupleElement {
                        type_id: TypeId::ANY,
                        optional: true,
                        rest: false,
                        name: None,
                    });
                    continue;
                }

                if let Some(element_data) = self.ctx.arena.get_binding_element(element_node) {
                    let mut element_type = if parent_type == TypeId::UNKNOWN {
                        TypeId::ANY
                    } else {
                        self.get_binding_element_type_with_request(
                            pattern_idx,
                            i,
                            parent_type,
                            element_data,
                            request,
                        )
                    };

                    if element_data.initializer.is_some() {
                        // Set contextual type for function-like initializers
                        let request = if element_type != TypeId::ANY
                            && element_type != TypeId::UNKNOWN
                            && let Some(init_node) = self.ctx.arena.get(element_data.initializer)
                            && (init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                        {
                            request.read().contextual(element_type)
                        } else {
                            request.read().contextual_opt(None)
                        };
                        let init_type =
                            self.get_type_of_node_with_request(element_data.initializer, &request);
                        if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                            element_type = init_type;
                        } else if !self.is_assignable_to(init_type, element_type) {
                            element_type = self.ctx.types.factory().union2(element_type, init_type);
                        }
                    } else if element_type == TypeId::ANY
                        && let Some(name_node) = self.ctx.arena.get(element_data.name)
                        && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                    {
                        element_type = self.infer_type_from_binding_pattern_with_request(
                            element_data.name,
                            element_type,
                            request,
                        );
                    }

                    let is_rest = element_data.dot_dot_dot_token;
                    // Rest elements are never optional — `get_binding_element_type_with_request`
                    // already returns the array type (e.g. `any[]`) for rest elements.
                    // Only non-rest elements with a default initializer are optional.
                    let is_optional = element_data.initializer.is_some() && !is_rest;

                    elements.push(tsz_solver::TupleElement {
                        type_id: element_type,
                        optional: is_optional,
                        rest: is_rest,
                        name: None,
                    });
                }
            }

            // An empty array binding pattern `[]` provides no structural constraints.
            // Return ANY so callers fall back to the initializer type instead of `[]`.
            if elements.is_empty() {
                return TypeId::ANY;
            }
            return factory.tuple(elements);
        }
        TypeId::ANY
    }
}

#[cfg(test)]
mod binding_contextual_type_tests {
    use crate::test_utils::check_source_codes;

    /// The contextual type fix ensures arrow function initializers in binding
    /// patterns get their parameter types inferred from the element type.
    /// Without this fix, `v => v.toString()` would be typed as `(v: any) => any`
    /// instead of `(v: number) => string`.
    #[test]
    fn arrow_in_binding_pattern_gets_contextual_type() {
        // This should not produce TS7006 (implicit any) because the arrow
        // parameter `v` should be contextually typed as `number`.
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             function f({ show = v => v.toString() }: Show) {}",
        );
        assert!(
            !codes.contains(&7006),
            "Arrow param should not be implicit any: {codes:?}"
        );
    }

    /// Variable declaration with arrow function default in binding pattern.
    #[test]
    fn var_decl_arrow_binding_gets_contextual_type() {
        let codes = check_source_codes(
            "interface SI { stringIdentity(s: string): string; }
             let { stringIdentity: id = arg => arg }: SI = { stringIdentity: x => x };",
        );
        assert!(
            !codes.contains(&7006),
            "Arrow param in var decl binding should not be implicit any: {codes:?}"
        );
    }

    /// Function expression default in binding pattern gets contextual type.
    #[test]
    fn function_expr_binding_gets_contextual_type() {
        let codes = check_source_codes(
            "interface Fn { handler: (x: number) => number; }
             function f({ handler = function(x) { return x; } }: Fn) {}",
        );
        assert!(
            !codes.contains(&7006),
            "Function expr param in binding should not be implicit any: {codes:?}"
        );
    }

    /// Destructuring from `unknown` parent type (e.g. rest type param
    /// constraint) must not emit false TS2339.
    #[test]
    fn inferred_rest_type_no_false_ts2339() {
        let codes = check_source_codes(
            "function wrap<Args extends unknown[]>(_: (...args: Args) => void) {}
             wrap(({ cancelable } = {}) => {});",
        );
        assert!(
            !codes.contains(&2339),
            "Should not emit TS2339 for destructured param with default from unknown rest type: {codes:?}"
        );
    }

    /// Nested destructuring of an optional property in an annotated parameter
    /// must include `| undefined` in the inner binding's type. The single-level
    /// resolver only handles `{ x }: T` patterns; nested patterns like
    /// `{ a: { b } }: T` previously fell through to `any` and silently dropped
    /// `| undefined`, masking real assignability errors in the function body.
    #[test]
    fn nested_destructured_optional_property_propagates_undefined() {
        let codes = check_source_codes(
            "// @strict: true
             function f({ a: { b } }: { a: { b?: number } } = { a: {} }) {
                 const x: number = b;
             }",
        );
        assert!(
            codes.contains(&2322),
            "Nested destructured optional property `b` should be `number | undefined` and emit TS2322 when assigned to `number`: {codes:?}"
        );
    }

    /// Same rule with different identifier names (`outer/inner` instead of
    /// `a/b`) — confirms the fix is keyed on the *structure* (nested binding
    /// pattern with optional property), not on any specific identifier name
    /// (per CLAUDE.md §25 anti-hardcoding review checklist).
    #[test]
    fn nested_destructured_optional_property_propagates_undefined_alt_names() {
        let codes = check_source_codes(
            "// @strict: true
             function g({ outer: { inner } }: { outer: { inner?: string } } = { outer: {} }) {
                 const x: string = inner;
             }",
        );
        assert!(
            codes.contains(&2322),
            "Nested destructured optional `inner` should propagate `| undefined`: {codes:?}"
        );
    }
}
