impl<'a> CheckerState<'a> {
    fn check_class_member_decorator_expressions(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Fast path: skip all decorator-related work when the member has no decorators.
        // This avoids expensive AST extraction and modifier analysis for the common case.
        {
            let has_any_decorator = match node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|d| d.modifiers.as_ref())
                        .is_some_and(|m| {
                            m.nodes.iter().any(|&idx| {
                                self.ctx
                                    .arena
                                    .get(idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                _ => false,
            };

            // Also need to check constructor parameter decorators
            let has_param_decorator = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes)),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes))
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(node)
                    .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes)),
                _ => false,
            };

            if !has_any_decorator && !has_param_decorator {
                return;
            }
        }

        let (modifiers, parameters, member_name_idx) = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), None, decl.name)
                }),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), Some(&decl.parameters), decl.name)
                }),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), Some(&decl.parameters), decl.name)
                }),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.ctx
                    .arena
                    .get_constructor(node)
                    .map_or((None, None, NodeIndex::NONE), |decl| {
                        (
                            decl.modifiers.as_ref(),
                            Some(&decl.parameters),
                            NodeIndex::NONE,
                        )
                    })
            }
            _ => (None, None, NodeIndex::NONE),
        };

        let is_abstract = modifiers.is_some_and(|m| {
            m.nodes.iter().any(|&mod_idx| {
                self.ctx
                    .arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == SyntaxKind::AbstractKeyword as u16)
            })
        });

        let is_ambient = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            || modifiers.is_some_and(|m| {
                m.nodes.iter().any(|&n| {
                    self.ctx
                        .arena
                        .get(n)
                        .is_some_and(|n| n.kind == SyntaxKind::DeclareKeyword as u16)
                })
            });

        let is_ambient_field = is_ambient && node.kind == syntax_kind_ext::PROPERTY_DECLARATION;

        // With --experimentalDecorators, decorators on private-named members
        // and members of class expressions are not valid (TS1206).
        let is_private_member =
            member_name_idx != NodeIndex::NONE && self.is_private_identifier_name(member_name_idx);
        let is_class_expression_member = self.ctx.enclosing_class.as_ref().is_some_and(|c| {
            self.ctx
                .arena
                .get(c.class_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION)
        });
        let legacy_decorator_not_valid = self.ctx.compiler_options.experimental_decorators
            && (is_private_member || is_class_expression_member);

        // ES (TC39) decorator first-argument shape per member kind. Computed once
        // before the per-decorator loop because the member kind and modifiers do
        // not vary across decorators on the same declaration.
        //
        // - Plain field: runtime invokes `decorator(undefined, context)`.
        // - Auto-accessor (`accessor x = …`): runtime invokes
        //   `decorator(target, context)` where `target` is a
        //   `ClassAccessorDecoratorTarget<This, Value>` object. We resolve the
        //   global type and instantiate it with `<any, any>`; the decorator's
        //   `This`/`Value` type parameters are inferred from this shape.
        //
        // If `ClassAccessorDecoratorTarget` is unavailable (e.g. `--noLib`) we
        // fall back to `ANY` so the absence of the lib type cannot itself
        // produce a TS1240 false positive.
        let es_member_first_arg: Option<TypeId> =
            if !self.ctx.compiler_options.experimental_decorators
                && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && !is_ambient_field
            {
                Some(if self.has_accessor_modifier_ref(modifiers) {
                    self.resolve_class_accessor_decorator_target_any()
                        .unwrap_or(TypeId::ANY)
                } else {
                    TypeId::UNDEFINED
                })
            } else {
                None
            };

        if let Some(modifiers) = modifiers {
            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                if modifier_node.kind != syntax_kind_ext::DECORATOR {
                    continue;
                }

                if is_abstract
                    || (!self.ctx.compiler_options.experimental_decorators && is_ambient_field)
                    || legacy_decorator_not_valid
                {
                    use crate::diagnostics::diagnostic_codes;
                    if is_abstract && node.kind == syntax_kind_ext::METHOD_DECLARATION {
                        self.error_at_node(
                            modifier_idx,
                            "A decorator can only decorate a method implementation, not an overload.",
                            diagnostic_codes::A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD,
                        );
                    } else {
                        self.error_at_node(
                            modifier_idx,
                            "Decorators are not valid here.",
                            diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                        );
                    }
                }

                let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) else {
                    continue;
                };

                // TS1497: Check decorator expression grammar
                self.check_grammar_decorator(decorator.expression);

                let decorator_type = self.compute_type_of_node(decorator.expression);
                let actual_this_type =
                    self.call_site_receiver_type(decorator_type, decorator.expression);

                if let Some(first_arg) = es_member_first_arg {
                    self.check_es_member_decorator_call_signature(
                        modifier_idx,
                        decorator_type,
                        first_arg,
                        actual_this_type,
                    );
                }

                if self.ctx.compiler_options.experimental_decorators
                    && !is_abstract
                    && !legacy_decorator_not_valid
                    && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                {
                    self.check_legacy_property_decorator_call_signature(
                        modifier_idx,
                        decorator_type,
                        self.has_accessor_modifier_ref(Some(modifiers)),
                        actual_this_type,
                    );
                }

                if !is_abstract
                    && !legacy_decorator_not_valid
                    && (node.kind == syntax_kind_ext::METHOD_DECLARATION
                        || node.kind == syntax_kind_ext::GET_ACCESSOR
                        || node.kind == syntax_kind_ext::SET_ACCESSOR)
                {
                    self.check_method_or_accessor_decorator_call_signature(
                        decorator.expression,
                        decorator_type,
                        modifier_idx,
                        member_idx,
                        self.ctx.compiler_options.experimental_decorators,
                        actual_this_type,
                    );
                }
            }
        }

        if let Some(parameters) = parameters {
            for &param_idx in &parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(param_modifiers) = &param.modifiers {
                    for &modifier_idx in &param_modifiers.nodes {
                        let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                            continue;
                        };
                        if modifier_node.kind != syntax_kind_ext::DECORATOR {
                            continue;
                        }

                        if !self.ctx.compiler_options.experimental_decorators {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                modifier_idx,
                                "Decorators are not valid here.",
                                diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                            );
                        }

                        if let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) {
                            // TS1497: Check decorator expression grammar
                            self.check_grammar_decorator(decorator.expression);

                            let decorator_type = self.compute_type_of_node(decorator.expression);

                            // TS1308: Check for await expressions in decorator arguments.
                            // Decorator arguments are evaluated in the enclosing scope,
                            // not the decorated method's scope. An await in a non-async
                            // enclosing function should trigger TS1308.
                            self.check_await_expression(decorator.expression);

                            // TS1239: Validate parameter decorator call signature.
                            // The runtime invokes parameter decorators as
                            // `decorator(target, key, parameterIndex)`. For
                            // constructor parameters tsc passes `undefined` for
                            // `key`; for method/accessor parameters tsc passes a
                            // string (the method name). Decorators whose `key`
                            // parameter type disagrees with the position are
                            // rejected with TS1239. Only check under
                            // `experimentalDecorators` since stage-3 decorators
                            // (which use a different runtime ABI) are not yet a
                            // supported configuration.
                            if self.ctx.compiler_options.experimental_decorators {
                                let is_constructor_parameter =
                                    node.kind == syntax_kind_ext::CONSTRUCTOR;
                                let actual_this_type = self
                                    .call_site_receiver_type(decorator_type, decorator.expression);
                                self.check_parameter_decorator_call_signature(
                                    modifier_idx,
                                    decorator_type,
                                    is_constructor_parameter,
                                    actual_this_type,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Quick scan to check if any parameter in a parameter list has a decorator modifier.
    fn any_parameter_has_decorator(&self, params: &[NodeIndex]) -> bool {
        for &param_idx in params {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if let Some(ref mods) = param.modifiers {
                for &mod_idx in &mods.nodes {
                    if self
                        .ctx
                        .arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}
