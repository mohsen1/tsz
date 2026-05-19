//! Implicit `any` parameter diagnostic checks (TS7006, TS7019, TS7051).
//!
//! Detects parameters that implicitly have type `any` under `--noImplicitAny`
//! and emits the appropriate diagnostic for regular params, rest params, and
//! destructuring patterns.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn enclosing_function_for_parameter_name(&self, param_name: NodeIndex) -> Option<NodeIndex> {
        let param_idx = self.ctx.arena.get_extended(param_name)?.parent;
        let func_idx = self.ctx.arena.get_extended(param_idx)?.parent;
        let func_node = self.ctx.arena.get(func_idx)?;
        (func_node.kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
            || func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION)
            .then_some(func_idx)
    }

    fn parameter_has_deferred_explicit_context(&self, param_name: NodeIndex) -> bool {
        let Some(func_idx) = self.enclosing_function_for_parameter_name(param_name) else {
            return false;
        };
        let mut current = self.ctx.arena.get_extended(func_idx).map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
                    return false;
                };
                return var_decl.type_annotation.is_some()
                    && self.explicit_annotation_can_defer_implicit_any_context(
                        var_decl.type_annotation,
                    );
            }
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
            {
                return false;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        false
    }

    fn annotation_is_array_like_type_node(&self, annotation_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::ARRAY_TYPE
                || k == tsz_parser::parser::syntax_kind_ext::TUPLE_TYPE =>
            {
                true
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE => self
                .ctx
                .arena
                .get_wrapped_type(node)
                .is_some_and(|wrapped| self.annotation_is_array_like_type_node(wrapped.type_node)),
            k if k == tsz_parser::parser::syntax_kind_ext::TYPE_OPERATOR => self
                .ctx
                .arena
                .get_type_operator(node)
                .is_some_and(|op| self.annotation_is_array_like_type_node(op.type_node)),
            k if k == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE => self
                .ctx
                .arena
                .get_type_ref(node)
                .and_then(|type_ref| self.property_name_for_error(type_ref.type_name))
                .is_some_and(|name| name == "Array" || name == "ReadonlyArray"),
            k if k == tsz_parser::parser::syntax_kind_ext::UNION_TYPE => self
                .ctx
                .arena
                .get_composite_type(node)
                .is_some_and(|union| {
                    !union.types.nodes.is_empty()
                        && union
                            .types
                            .nodes
                            .iter()
                            .all(|&member| self.annotation_is_array_like_type_node(member))
                }),
            k if k == tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE => self
                .ctx
                .arena
                .get_composite_type(node)
                .is_some_and(|intersection| {
                    intersection
                        .types
                        .nodes
                        .iter()
                        .any(|&member| self.annotation_is_array_like_type_node(member))
                }),
            _ => false,
        }
    }

    fn annotation_is_object_constructor_keys(&self, annotation_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE {
            return self
                .ctx
                .arena
                .get_wrapped_type(node)
                .is_some_and(|wrapped| {
                    self.annotation_is_object_constructor_keys(wrapped.type_node)
                });
        }
        if node.kind != tsz_parser::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE {
            return false;
        }
        let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
            return false;
        };
        let object_is_object_constructor = self
            .ctx
            .arena
            .get(indexed.object_type)
            .filter(|object_node| {
                object_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
            })
            .and_then(|object_node| self.ctx.arena.get_type_ref(object_node))
            .and_then(|type_ref| self.property_name_for_error(type_ref.type_name))
            .is_some_and(|name| name == "ObjectConstructor");
        if !object_is_object_constructor {
            return false;
        }
        let Some(index_node) = self.ctx.arena.get(indexed.index_type) else {
            return false;
        };
        if index_node.kind != tsz_parser::parser::syntax_kind_ext::LITERAL_TYPE {
            return false;
        }
        let Some(literal_type) = self.ctx.arena.get_literal_type(index_node) else {
            return false;
        };
        self.ctx
            .arena
            .get(literal_type.literal)
            .and_then(|literal_node| self.ctx.arena.get_literal(literal_node))
            .is_some_and(|literal| literal.text.as_str() == "keys")
    }

    fn callable_type_returns_array_like(&mut self, callable_type: TypeId) -> bool {
        if matches!(callable_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return false;
        }
        let evaluated = self.evaluate_application_type(callable_type);
        let resolved = self.resolve_lazy_type(evaluated);
        match crate::query_boundaries::type_computation::complex::classify_for_call_signatures(
            self.ctx.types,
            resolved,
        ) {
            crate::query_boundaries::type_computation::complex::CallSignaturesKind::Callable(
                shape_id,
            ) => self
                .ctx
                .types
                .callable_shape(shape_id)
                .call_signatures
                .iter()
                .any(|signature| {
                    let return_type = self.evaluate_application_type(signature.return_type);
                    let return_type = self.resolve_lazy_type(return_type);
                    self.is_array_like_type(return_type)
                }),
            crate::query_boundaries::type_computation::complex::CallSignaturesKind::MultipleSignatures(signatures) => signatures.iter().any(|signature| {
                let return_type = self.evaluate_application_type(signature.return_type);
                let return_type = self.resolve_lazy_type(return_type);
                self.is_array_like_type(return_type)
            }),
            crate::query_boundaries::type_computation::complex::CallSignaturesKind::NoSignatures => false,
        }
    }

    fn symbol_has_object_keys_annotation(&self, initial_sym_id: tsz_binder::SymbolId) -> bool {
        let sym_id = self
            .ctx
            .alias_partner_for(self.ctx.binder, initial_sym_id)
            .unwrap_or(initial_sym_id);
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());
        declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let var_decl_idx =
                if decl_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
                    Some(decl_idx)
                } else if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                    self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
                } else {
                    None
                };
            let Some(var_decl_idx) = var_decl_idx else {
                return false;
            };
            let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
                return false;
            };
            if var_decl_node.kind != tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
                return false;
            }
            self.ctx
                .arena
                .get_variable_declaration(var_decl_node)
                .is_some_and(|var_decl| {
                    var_decl.type_annotation.is_some()
                        && self.annotation_is_object_constructor_keys(var_decl.type_annotation)
                })
        })
    }

    fn callee_declaration_has_object_keys_annotation(&self, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        let symbol_node = if callee_node.kind
            == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            self.ctx
                .arena
                .get_access_expr(callee_node)
                .map(|access| access.name_or_argument)
        } else if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            Some(callee_idx)
        } else {
            None
        };
        let Some(symbol_node) = symbol_node else {
            return false;
        };
        if let Some(initial_sym_id) = self.ctx.binder.get_node_symbol(symbol_node).or_else(|| {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, symbol_node)
        }) && self.symbol_has_object_keys_annotation(initial_sym_id)
        {
            return true;
        }

        if callee_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(property_name) = self.property_name_for_error(access.name_or_argument) else {
            return false;
        };
        let Some(container_sym_id) =
            self.ctx
                .binder
                .get_node_symbol(access.expression)
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, access.expression)
                })
        else {
            return false;
        };
        let container_sym_id = self
            .ctx
            .alias_partner_for(self.ctx.binder, container_sym_id)
            .unwrap_or(container_sym_id);
        let Some(container_symbol) = self.ctx.binder.get_symbol(container_sym_id) else {
            return false;
        };
        container_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(&property_name))
            .or_else(|| {
                container_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(&property_name))
            })
            .is_some_and(|member_sym_id| self.symbol_has_object_keys_annotation(member_sym_id))
    }

    fn callee_is_unresolved_object_keys_like(&self, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(property_name) = self.property_name_for_error(access.name_or_argument) else {
            return false;
        };
        let container_name = self.property_name_for_error(access.expression).or_else(|| {
            self.node_text(access.expression)
                .map(|text| text.trim().to_string())
        });
        matches!(
            (container_name.as_deref(), property_name.as_str()),
            (Some("Object"), "keys") | (Some("util"), "objectKeys")
        )
    }

    fn initializer_returns_array_like(&mut self, initializer_idx: NodeIndex) -> bool {
        let initializer_type = self.get_type_of_node(initializer_idx);
        if initializer_type != TypeId::ANY
            && initializer_type != TypeId::ERROR
            && self.is_array_like_type(initializer_type)
        {
            return true;
        }
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return false;
        };
        if init_node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(init_node) else {
            return false;
        };
        let callee_type = self.get_type_of_node(call.expression);
        let callable_returns_array_like = self.callable_type_returns_array_like(callee_type);
        let callee_has_object_keys_annotation =
            self.callee_declaration_has_object_keys_annotation(call.expression);
        callable_returns_array_like
            || callee_has_object_keys_annotation
            || (callee_type == TypeId::ERROR
                && self.callee_is_unresolved_object_keys_like(call.expression))
    }

    fn type_literal_property_annotation_is_array_like(
        &self,
        type_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        match type_node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::TYPE_LITERAL => {
                let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                    return false;
                };
                type_lit.members.nodes.iter().any(|&member_idx| {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        return false;
                    };
                    if member_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE {
                        return false;
                    }
                    let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                        return false;
                    };
                    signature.type_annotation.is_some()
                        && self
                            .property_name_for_error(signature.name)
                            .as_deref()
                            .is_some_and(|name| name == property_name)
                        && self.annotation_is_array_like_type_node(signature.type_annotation)
                })
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE => self
                .ctx
                .arena
                .get_wrapped_type(type_node)
                .is_some_and(|wrapped| {
                    self.type_literal_property_annotation_is_array_like(
                        wrapped.type_node,
                        property_name,
                    )
                }),
            k if k == tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE => self
                .ctx
                .arena
                .get_composite_type(type_node)
                .is_some_and(|intersection| {
                    intersection.types.nodes.iter().any(|&member| {
                        self.type_literal_property_annotation_is_array_like(member, property_name)
                    })
                }),
            _ => false,
        }
    }

    fn declaration_has_array_like_annotation(
        &mut self,
        decl_idx: NodeIndex,
        property_name: Option<&str>,
    ) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                return false;
            };
            if var_decl.initializer.is_some()
                && self.initializer_returns_array_like(var_decl.initializer)
            {
                return true;
            }
            return var_decl.type_annotation.is_some()
                && self.annotation_is_array_like_type_node(var_decl.type_annotation);
        }
        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::PARAMETER {
            let Some(param) = self.ctx.arena.get_parameter(decl_node) else {
                return false;
            };
            return if let Some(name) = property_name {
                param.type_annotation.is_some()
                    && self
                        .type_literal_property_annotation_is_array_like(param.type_annotation, name)
            } else {
                param.type_annotation.is_some()
                    && self.annotation_is_array_like_type_node(param.type_annotation)
            };
        }
        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::BINDING_ELEMENT {
            return self.binding_element_source_property_is_array_like(decl_idx);
        }
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(parent_idx) = self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
        {
            return self.declaration_has_array_like_annotation(parent_idx, property_name);
        }
        false
    }

    fn binding_element_source_property_is_array_like(
        &mut self,
        binding_element_idx: NodeIndex,
    ) -> bool {
        let Some(binding_node) = self.ctx.arena.get(binding_element_idx) else {
            return false;
        };
        let Some(binding) = self.ctx.arena.get_binding_element(binding_node) else {
            return false;
        };
        let property_name_idx = if binding.property_name.is_some() {
            binding.property_name
        } else {
            binding.name
        };
        let Some(property_name) = self.property_name_for_error(property_name_idx) else {
            return false;
        };
        let Some(pattern_idx) = self
            .ctx
            .arena
            .get_extended(binding_element_idx)
            .map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(parent_idx) = self
            .ctx
            .arena
            .get_extended(pattern_idx)
            .map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        if parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
                return false;
            };
            if var_decl.initializer.is_none() {
                return false;
            }
            let source_type = self.get_type_of_node(var_decl.initializer);
            if source_type != TypeId::ANY
                && source_type != TypeId::ERROR
                && let Some(prop_type) = self
                    .resolve_property_access_with_env(source_type, &property_name)
                    .success_type()
                && self.is_array_like_type(prop_type)
            {
                return true;
            }

            let Some(init_symbol) = self
                .ctx
                .binder
                .get_node_symbol(var_decl.initializer)
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, var_decl.initializer)
                })
            else {
                return false;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(init_symbol) else {
                return false;
            };
            let mut declarations = Vec::new();
            if symbol.value_declaration.is_some() {
                declarations.push(symbol.value_declaration);
            }
            declarations.extend(symbol.declarations.iter().copied());
            return declarations.into_iter().any(|decl| {
                self.declaration_has_array_like_annotation(decl, Some(&property_name))
            });
        }

        if parent_node.kind == tsz_parser::parser::syntax_kind_ext::PARAMETER {
            let Some(param) = self.ctx.arena.get_parameter(parent_node) else {
                return false;
            };
            return param.type_annotation.is_some()
                && self.type_literal_property_annotation_is_array_like(
                    param.type_annotation,
                    &property_name,
                );
        }

        false
    }

    fn expression_has_array_like_declaration(&mut self, expression: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expression) else {
            return false;
        };
        if expr_node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .ctx
                .arena
                .get_parenthesized(expr_node)
                .is_some_and(|paren| self.expression_has_array_like_declaration(paren.expression));
        }
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.ctx.binder.get_node_symbol(expression).or_else(|| {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, expression)
        }) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());
        declarations
            .iter()
            .copied()
            .any(|decl| self.declaration_has_array_like_annotation(decl, None))
    }

    fn filter_receiver_is_array_like_for_implicit_any(&mut self, receiver: NodeIndex) -> bool {
        let receiver_type = self.get_type_of_node(receiver);
        if receiver_type != TypeId::ANY
            && receiver_type != TypeId::ERROR
            && self.is_array_like_type(receiver_type)
        {
            return true;
        }
        self.expression_has_array_like_declaration(receiver)
    }

    fn parameter_is_error_typed_filter_callback(&mut self, param_name: NodeIndex) -> bool {
        let Some(func_idx) = self.enclosing_function_for_parameter_name(param_name) else {
            return false;
        };
        let Some(call_idx) = self.ctx.arena.get_extended(func_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        if call_node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
            return false;
        };
        if call
            .arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
            != Some(func_idx)
        {
            return false;
        }
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        if self
            .property_name_for_error(access.name_or_argument)
            .as_deref()
            != Some("filter")
        {
            return false;
        }
        self.filter_receiver_is_array_like_for_implicit_any(access.expression)
    }

    /// Returns `true` when `node_idx` has already been processed with a contextual
    /// callable type by `get_type_of_function_impl`, meaning its parameters were
    /// typed (or suppressed) there and must not be re-checked for TS7006.
    ///
    /// Example: `fn: (x: number) => number = x => x * 2` — the arrow function is
    /// processed with contextual type `(x: number) => number`, so `x` is inferred
    /// as `number` and no TS7006 is emitted.
    pub(crate) fn closure_has_contextual_type(&self, node_idx: NodeIndex) -> bool {
        self.ctx.implicit_any_checked_closures.contains(&node_idx)
            || self
                .ctx
                .implicit_any_contextual_closures
                .contains(&node_idx)
    }

    pub(crate) fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
        param_index: usize,
    ) {
        self.maybe_report_implicit_any_parameter_with_type_hint(
            param,
            has_contextual_type,
            param_index,
            None,
        );
    }

    pub(crate) fn maybe_report_implicit_any_parameter_with_type_hint(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
        param_index: usize,
        implicit_type_hint: Option<&'static str>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Implicit-any diagnostics (TS7006/TS7019/TS7031) are governed by
        // `noImplicitAny`. `no_implicit_any()` already accounts for the
        // checked-JS case (it returns true under `--checkJs --strict` in
        // `.js` files). When the user explicitly sets `noImplicitAny: false`,
        // tsc suppresses these — forcing JS emission here regresses conformance
        // tests like jsxDeclarationsWithEsModuleInteropNoCrash.tsx.
        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // JSDoc-declared function-type parameters can be represented as
        // synthetic Parameter nodes inside comment spans. They are not runtime
        // function parameters and should not trigger TS7006.
        if self.is_node_inside_jsdoc_comment(param.name) {
            return;
        }
        // Skip rest parameters named 'arguments' — tsc emits TS1100 instead of TS7019
        // for `...arguments` because 'arguments' is a reserved identifier in strict mode.
        if param.dot_dot_dot_token
            && let Some(name_node) = self.ctx.arena.get(param.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.as_str() == "arguments"
        {
            return;
        }
        // Skip parameters that have explicit type annotations
        if param.type_annotation.is_some() {
            return;
        }
        // Skip parameters in contexts where types are inferred from usage:
        // - IIFE parameters get types from the call arguments
        // - JSX attribute callback parameters get types from the JSX context
        // - Promise executor parameters get types from Promise<T>
        // These must be checked BEFORE the destructuring pattern check below,
        // which would otherwise report TS7031 for binding elements.
        if self.is_this_parameter_name(param.name) {
            return;
        }
        if self.is_parameter_in_promise_executor(param.name) {
            return;
        }
        if self.is_parameter_in_iife(param.name) {
            return;
        }
        if self.is_parameter_in_jsx_attribute_callback(param.name) {
            return;
        }
        if self.parameter_has_deferred_explicit_context(param.name)
            || self.parameter_is_error_typed_filter_callback(param.name)
        {
            return;
        }
        // Destructuring parameters need recursive implicit-any checking, but only
        // when the outer initializer doesn't provide type info for the bindings.
        // E.g., `({ json = [] } = {})` — the `{}` default is empty, so `json` is
        // implicitly `any[]`. But `({x} = { x: new Class() })` has a non-empty
        // initializer that types `x` as `Class`, so no TS7031.
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            use tsz_parser::parser::syntax_kind_ext;
            let kind = name_node.kind;
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                let outer_init_provides_types = param.initializer.is_some()
                    && !Self::is_empty_object_literal_init(self.ctx.arena, param.initializer)
                    && !Self::is_empty_array_literal_init(self.ctx.arena, param.initializer);
                if !outer_init_provides_types {
                    self.emit_implicit_any_parameter_for_pattern(
                        param.name,
                        param.dot_dot_dot_token,
                    );
                } else if kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    && let Some(init_len) =
                        Self::array_literal_init_len(self.ctx.arena, param.initializer)
                {
                    // Array-literal default `[v0, v1, ...]` is non-empty but may
                    // be shorter than the binding pattern. Emit TS7031 for
                    // binding leaves at indices the literal does not cover.
                    self.emit_implicit_any_for_array_pattern_beyond_default(
                        param.name,
                        param.dot_dot_dot_token,
                        init_len,
                    );
                }
                return;
            }
        }
        // Check if parameter has an initializer — any initializer (including null/undefined)
        // provides a type for the parameter. tsc infers `null` or `undefined` as the type,
        // so these do NOT trigger TS7006.
        if param.initializer.is_some() && implicit_type_hint.is_none() {
            return;
        }

        let reserved_word_param = self.ctx.arena.get(param.name).and_then(|name_node| {
            self.ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.as_str())
        });
        let preserve_on_strict_mode_parse_error = reserved_word_param.is_some_and(|name| {
            crate::state_checking::is_strict_mode_reserved_name(name)
                || crate::state_checking::is_eval_or_arguments(name)
        });

        // Skip TS7006 for parameters on nodes with parse errors.
        // This prevents cascading "implicitly has any type" errors on malformed AST nodes.
        // The parse error itself should already be emitted (e.g., TS1005, TS2390).
        // Check both the parameter name AND the enclosing function/arrow for errors,
        // since parse errors like `(a): => {}` set flags on the parent, not on `a`.
        //
        // EXCEPTION: Rest parameters (dot_dot_dot_token) are NOT suppressed by parse errors.
        // tsc always emits TS7019 for rest parameters even when related parse errors exist
        // (e.g., TS1047 "rest can't be optional" for `...arg?`, TS1014 "rest not last"
        // for `...x, y`). The empty-name check below still catches truly malformed rest params.
        if !param.dot_dot_dot_token {
            if let Some(name_node) = self.ctx.arena.get(param.name)
                && (name_node.this_node_has_error() || name_node.this_or_subtree_has_error())
                && !preserve_on_strict_mode_parse_error
            {
                return;
            }
            // Also check parent chain (parameter → function/arrow) for parse errors
            if let Some(ext) = self.ctx.arena.get_extended(param.name) {
                // param.name's parent is ParameterDeclaration; its parent is the function/arrow
                let param_decl = ext.parent;
                if let Some(param_node) = self.ctx.arena.get(param_decl)
                    && param_node.this_or_subtree_has_error()
                    && !preserve_on_strict_mode_parse_error
                {
                    return;
                }
                if let Some(param_ext) = self.ctx.arena.get_extended(param_decl)
                    && let Some(func_node) = self.ctx.arena.get(param_ext.parent)
                    && func_node.this_or_subtree_has_error()
                    && !preserve_on_strict_mode_parse_error
                {
                    return;
                }
            }

            // Suppress TS7006 when a scanner-level parse error (e.g. TS1127 invalid character)
            // exists near the parameter. This handles cases like `function f(a,¬) {}`
            // where the sibling token is invalid but the param node itself has no error flag.
            if self.has_syntax_parse_errors()
                && self.node_has_nearby_parse_error(param.name)
                && !preserve_on_strict_mode_parse_error
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Skip if the parameter name is empty (parse recovery artifact)
        if param_name.is_empty() {
            return;
        }
        let implicit_type = implicit_type_hint.unwrap_or("any");

        // Rest parameters use TS7019, regular parameters use TS7006
        let report_node = self.param_node_for_implicit_any_diagnostic(param);

        // TS7051 only applies to parameters WITHOUT modifiers (public/private/protected/readonly).
        // When a parameter has a modifier, the name is clearly a parameter name, not a type.
        let has_parameter_modifiers = param
            .modifiers
            .as_ref()
            .is_some_and(|m| !m.nodes.is_empty());
        let in_class_context = self.ctx.enclosing_class.is_some()
            || self.nearest_enclosing_class(param.name).is_some();

        if param.dot_dot_dot_token {
            // TS7019/TS7051 for rest parameters: tsc anchors the span at the `...`
            // token, covering `...name`.  `normalized_anchor_span` would collapse the
            // Parameter node to just the name identifier, so we bypass it and emit with
            // the raw Parameter-node span (which starts at `...`).
            let rest_report_node = self
                .ctx
                .arena
                .get_extended(param.name)
                .map(|ext| ext.parent)
                .unwrap_or(report_node);
            // Get the span from the Parameter node directly (starts at `...`).
            // Use name end as the span end so modifiers/type annotations are excluded.
            let (rest_start, rest_len) = self
                .get_node_span(rest_report_node)
                .and_then(|(param_start, _)| {
                    let name_end = self.ctx.arena.get(param.name)?.end;
                    Some((param_start, name_end.saturating_sub(param_start)))
                })
                .unwrap_or_else(|| self.get_node_span(report_node).unwrap_or((0, 0)));

            if !has_parameter_modifiers && Self::is_type_keyword_name(&param_name) {
                let suggested_name = format!("arg{param_index}");
                let suggested_type = format!("{param_name}[]");
                let template = tsz_common::diagnostics::get_message_template(
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                )
                .unwrap_or("");
                let message = crate::diagnostics::format_message(
                    template,
                    &[&suggested_name, &suggested_type],
                );
                self.error_at_position(
                    rest_start,
                    rest_len,
                    &message,
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                );
            } else {
                let template = tsz_common::diagnostics::get_message_template(
                    diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                )
                .unwrap_or("");
                let message = crate::diagnostics::format_message(template, &[&param_name]);
                self.error_at_position(
                    rest_start,
                    rest_len,
                    &message,
                    diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                );
            }
        } else {
            // TS7051: Detect parameters whose name looks like a type keyword or type name
            // e.g., `(string, number)` where the user likely meant `(arg0: string, arg1: number)`
            // TypeScript emits TS7051 for type keyword names and uppercase-starting names
            // (which conventionally refer to classes/interfaces).
            // Only when the parameter has NO modifiers (public A is clearly a parameter name).
            // tsc emits TS7051 ("Parameter has a name but no type. Did you mean
            // 'arg0: <type>'?") instead of TS7006 ("implicitly has 'any' type")
            // in two scenarios:
            //   1. The parameter name is a TypeScript primitive type keyword
            //      (`string`, `number`, …) or starts with an uppercase letter
            //      (suggesting a type/class reference). Always upgraded.
            //   2. The parameter name is a non-modifier strict-mode reserved
            //      word (`package`, `let`, `interface`, …) AND the enclosing
            //      function-like is a signature without a body (method or call
            //      signature inside an interface/type literal). Modifier-style
            //      reserved words (`public`, `private`, `protected`) are
            //      clearly parameter names even in a signature, so they keep
            //      TS7006.
            // Top-level functions and arrow/function expressions get TS7006
            // unless rule (1) fires.
            let in_signature_only_context = self.parameter_in_signature_only_context(param.name);
            let always_upgrade = Self::is_type_keyword_name(&param_name)
                || param_name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase());
            let signature_with_non_modifier_reserved =
                in_signature_only_context && Self::is_non_modifier_reserved_name(&param_name);
            if !has_parameter_modifiers
                && !in_class_context
                && (always_upgrade || signature_with_non_modifier_reserved)
            {
                let suggested_name = format!("arg{param_index}");
                self.error_at_node_msg(
                    report_node,
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                    &[&suggested_name, &param_name],
                );
            } else {
                self.error_at_node_msg(
                    report_node,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                    &[&param_name, implicit_type],
                );
            }
        }
    }

    /// Check whether the parameter's enclosing function-like is a signature
    /// without a body — `MethodSignature` or `CallSignature`/`ConstructSignature`
    /// inside an interface or type literal. Those slots conventionally require
    /// type annotations, so tsc emits the more helpful TS7051 ("Did you mean
    /// 'arg0: <Type>'?") instead of TS7006 when a parameter has no type.
    fn parameter_in_signature_only_context(&self, param_name_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(param_ext) = self.ctx.arena.get_extended(param_name_idx) else {
            return false;
        };
        // param_name_idx -> Parameter -> function-like
        let parameter_idx = param_ext.parent;
        let Some(parameter_ext) = self.ctx.arena.get_extended(parameter_idx) else {
            return false;
        };
        let function_like_idx = parameter_ext.parent;
        let Some(function_like_node) = self.ctx.arena.get(function_like_idx) else {
            return false;
        };
        matches!(
            function_like_node.kind,
            syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::CALL_SIGNATURE
                | syntax_kind_ext::CONSTRUCT_SIGNATURE
        )
    }

    /// Check if a parameter name is a strict-mode reserved word that is NOT
    /// a parameter modifier. These are reserved words like `package`, `let`,
    /// `interface`, `static`, `implements` — words that *could* plausibly be
    /// type names if the user forgot the `:`. Modifier-style reserved words
    /// (`public`, `private`, `protected`) and `yield` are clearly parameter
    /// names even when used in signature positions and so are excluded here.
    ///
    /// Used together with `parameter_in_signature_only_context` to decide
    /// when to upgrade an implicit-any TS7006 to the more helpful TS7051
    /// "Did you mean 'arg0: <Name>'?" diagnostic.
    fn is_non_modifier_reserved_name(name: &str) -> bool {
        matches!(
            name,
            "implements" | "interface" | "let" | "package" | "static"
        )
    }

    /// Check if a parameter name is a TypeScript type keyword.
    /// These keywords when used as parameter names strongly suggest the user
    /// intended them as type annotations, not parameter names.
    fn is_type_keyword_name(name: &str) -> bool {
        matches!(
            name,
            "string"
                | "number"
                | "boolean"
                | "symbol"
                | "void"
                | "object"
                | "undefined"
                | "bigint"
                | "never"
                | "any"
                | "unknown"
        )
    }

    fn is_node_inside_jsdoc_comment(&self, node_idx: NodeIndex) -> bool {
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        comments.iter().any(|comment| {
            is_jsdoc_comment(comment, source_text)
                && node.pos >= comment.pos
                && node.pos < comment.end
        })
    }

    fn param_node_for_implicit_any_diagnostic(
        &self,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> NodeIndex {
        let Some(modifiers) = param.modifiers.as_ref() else {
            return param.name;
        };
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;
        for &mod_idx in &modifiers.nodes {
            let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                continue;
            };
            // tsc anchors TS7006 at the first modifier — whether it's an
            // access modifier keyword or a decorator. Return the first
            // modifier found so the position matches tsc's output.
            if mod_node.kind == SyntaxKind::PublicKeyword as u16
                || mod_node.kind == SyntaxKind::PrivateKeyword as u16
                || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                || mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
                || mod_node.kind == syntax_kind_ext::DECORATOR
            {
                return mod_idx;
            }
        }
        param.name
    }

    fn is_parameter_in_jsx_attribute_callback(&self, name_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = Some(name_idx);
        let mut function_idx = None;
        while let Some(idx) = current {
            let Some(node) = self.ctx.arena.get(idx) else {
                break;
            };
            if matches!(
                node.kind,
                syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
            ) {
                function_idx = Some(idx);
                break;
            }
            current = self.ctx.arena.parent_of(idx);
        }

        let Some(function_idx) = function_idx else {
            return false;
        };
        let Some(function_parent) = self
            .ctx
            .arena
            .get_extended(function_idx)
            .map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(function_parent) else {
            return false;
        };

        // Case 1: Direct JSX attribute callback: <Comp onClick={(k) => {}} />
        //   ArrowFunction → JsxExpression → JsxAttribute
        if parent_node.kind == syntax_kind_ext::JSX_EXPRESSION {
            let Some(jsx_parent) = self
                .ctx
                .arena
                .get_extended(function_parent)
                .map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(jsx_parent_node) = self.ctx.arena.get(jsx_parent) else {
                return false;
            };
            if jsx_parent_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                return false;
            }
            // Do not suppress for data-* and aria-* attributes -- these are untyped
            // HTML custom data attributes that don't provide contextual types.
            if let Some(attr_data) = self.ctx.arena.get_jsx_attribute(jsx_parent_node)
                && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();
                if name.starts_with("data-") || name.starts_with("aria-") {
                    return false;
                }
            }
        } else if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            // Case 2: Spread attribute callback: <Comp {...{onClick: (k) => {}}} />
            //   ArrowFunction → PropertyAssignment → ObjectLiteralExpression → JsxSpreadAttribute
            let Some(obj_parent) = self
                .ctx
                .arena
                .get_extended(function_parent)
                .map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(obj_node) = self.ctx.arena.get(obj_parent) else {
                return false;
            };
            if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return false;
            }
            let Some(spread_parent) = self
                .ctx
                .arena
                .get_extended(obj_parent)
                .map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(spread_node) = self.ctx.arena.get(spread_parent) else {
                return false;
            };
            if spread_node.kind != syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                return false;
            }
        } else {
            return false;
        }

        true
    }

    /// Emit TS7006 errors for nested binding elements in destructuring parameters.
    /// TypeScript reports implicit 'any' for individual bindings in patterns like:
    ///   function foo({ x, y }: any) {}  // no error on x, y with type annotation
    ///   function bar({ x, y }) {}        // errors on x and y
    ///
    pub(crate) fn emit_implicit_any_parameter_for_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        // Handle object binding patterns: { x, y, z }
        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    let Some(element_node) = self.ctx.arena.get(element_idx) else {
                        continue;
                    };
                    // Skip omitted expressions
                    if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                    else {
                        continue;
                    };
                    // Check if name is a nested pattern - if so, only recurse, don't report
                    // TS7031 for intermediate patterns. tsc only reports for leaf identifiers.
                    let name_is_pattern = self
                        .ctx
                        .arena
                        .get(binding_elem.name)
                        .map(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        })
                        .unwrap_or(false);

                    if name_is_pattern {
                        // Recursively check nested patterns only
                        self.emit_implicit_any_parameter_for_pattern(
                            binding_elem.name,
                            is_rest_parameter,
                        );
                    } else {
                        // Leaf binding — report when no initializer or empty array default
                        let implicit_type = if binding_elem.initializer.is_none() {
                            Some(if is_rest_parameter { "any[]" } else { "any" })
                        } else if Self::is_empty_array_literal_init(
                            self.ctx.arena,
                            binding_elem.initializer,
                        ) {
                            Some("any[]")
                        } else {
                            None
                        };
                        if let Some(implicit_type) = implicit_type {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);
                            self.error_at_node_msg(
                                binding_elem.name,
                                diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                &[&binding_name, implicit_type],
                            );
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
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                let element_kind = element_node.kind;

                // Skip omitted expressions (holes in array patterns)
                if element_kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }

                // Check if this element is a binding element with initializer
                let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) else {
                    continue;
                };
                // Check if name is a nested pattern - if so, only recurse, don't report
                // TS7031 for intermediate patterns. tsc only reports for leaf identifiers.
                let name_is_pattern = self
                    .ctx
                    .arena
                    .get(binding_elem.name)
                    .map(|n| {
                        n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    })
                    .unwrap_or(false);

                if name_is_pattern {
                    // Recursively check nested patterns only
                    self.emit_implicit_any_parameter_for_pattern(
                        binding_elem.name,
                        is_rest_parameter,
                    );
                } else {
                    // Leaf binding — report when no initializer or empty array default
                    let implicit_type = if binding_elem.initializer.is_none() {
                        Some(if is_rest_parameter { "any[]" } else { "any" })
                    } else if Self::is_empty_array_literal_init(
                        self.ctx.arena,
                        binding_elem.initializer,
                    ) {
                        Some("any[]")
                    } else {
                        None
                    };
                    if let Some(implicit_type) = implicit_type {
                        let binding_name = self.parameter_name_for_error(binding_elem.name);
                        self.error_at_node_msg(
                            binding_elem.name,
                            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                            &[&binding_name, implicit_type],
                        );
                    }
                }
            }
        }
    }

    /// Emit TS7031 for array binding leaves at indices the array-literal default
    /// does not cover. Used when a parameter has the form `[a, b, ...] = [v0, v1]`
    /// and the literal is shorter than the pattern. Leaves with their own
    /// initializer (e.g. `b = 'x'`) are not reported; nested patterns are
    /// recursed into via `emit_implicit_any_parameter_for_pattern` (which
    /// assumes no outer initializer covers them, since the outer literal does
    /// not extend to this index).
    pub(crate) fn emit_implicit_any_for_array_pattern_beyond_default(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
        default_len: usize,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (logical_index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
            // Indices within the literal get their type from the corresponding
            // literal element; nothing to report.
            if logical_index < default_len {
                continue;
            }

            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            // Skip omitted expressions (holes in array patterns).
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            let is_rest_element = binding_elem.dot_dot_dot_token;

            // Nested pattern: recurse without a default — the outer literal
            // does not cover this index at all.
            let name_is_pattern = self
                .ctx
                .arena
                .get(binding_elem.name)
                .map(|n| {
                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                })
                .unwrap_or(false);

            if name_is_pattern {
                self.emit_implicit_any_parameter_for_pattern(
                    binding_elem.name,
                    is_rest_parameter || is_rest_element,
                );
                continue;
            }

            // Leaf binding: emit TS7031 only when it has no own initializer
            // (or an empty array literal default).
            let implicit_type = if binding_elem.initializer.is_none() {
                Some(if is_rest_parameter || is_rest_element {
                    "any[]"
                } else {
                    "any"
                })
            } else if Self::is_empty_array_literal_init(self.ctx.arena, binding_elem.initializer) {
                Some("any[]")
            } else {
                None
            };
            if let Some(implicit_type) = implicit_type {
                let binding_name = self.parameter_name_for_error(binding_elem.name);
                self.error_at_node_msg(
                    binding_elem.name,
                    diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                    &[&binding_name, implicit_type],
                );
            }
        }
    }

    /// Returns true if `init` points to an empty object literal `{}`.
    fn is_empty_object_literal_init(
        arena: &tsz_parser::parser::NodeArena,
        init: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        arena.get(init).is_some_and(|node| {
            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && arena
                    .get_literal_expr(node)
                    .is_some_and(|obj| obj.elements.nodes.is_empty())
        })
    }

    /// Returns true if `init` points to an empty array literal `[]`.
    fn is_empty_array_literal_init(arena: &tsz_parser::parser::NodeArena, init: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        arena.get(init).is_some_and(|node| {
            node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && arena
                    .get_literal_expr(node)
                    .is_some_and(|arr| arr.elements.nodes.is_empty())
        })
    }

    /// Returns the element count of `init` if it is an array literal, else `None`.
    ///
    /// Spread elements (`...rest`) make the literal's effective length not
    /// statically known, so we conservatively return `None` and let the
    /// existing path skip TS7031 emission entirely.
    fn array_literal_init_len(
        arena: &tsz_parser::parser::NodeArena,
        init: NodeIndex,
    ) -> Option<usize> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = arena.get(init)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let arr = arena.get_literal_expr(node)?;
        for &el_idx in &arr.elements.nodes {
            if let Some(el) = arena.get(el_idx)
                && el.kind == syntax_kind_ext::SPREAD_ELEMENT
            {
                return None;
            }
        }
        Some(arr.elements.nodes.len())
    }

    /// Emit TS7031 errors for binding elements in destructuring variable declarations
    /// without type annotations or initializers (`var [a], {b};` under noImplicitAny).
    pub(crate) fn emit_implicit_any_for_var_destructuring(&mut self, pattern_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    let Some(element_node) = self.ctx.arena.get(element_idx) else {
                        continue;
                    };
                    if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }
                    let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                    else {
                        continue;
                    };
                    let name_is_pattern = self
                        .ctx
                        .arena
                        .get(binding_elem.name)
                        .map(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        })
                        .unwrap_or(false);

                    if name_is_pattern {
                        self.emit_implicit_any_for_var_destructuring(binding_elem.name);
                    } else if binding_elem.initializer.is_none() {
                        let binding_name = self.parameter_name_for_error(binding_elem.name);
                        self.error_at_node_msg(
                            binding_elem.name,
                            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                            &[&binding_name, "any"],
                        );
                    }
                }
            }
        } else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) else {
                    continue;
                };
                let name_is_pattern = self
                    .ctx
                    .arena
                    .get(binding_elem.name)
                    .map(|n| {
                        n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    })
                    .unwrap_or(false);

                if name_is_pattern {
                    self.emit_implicit_any_for_var_destructuring(binding_elem.name);
                } else if binding_elem.initializer.is_none() {
                    let binding_name = self.parameter_name_for_error(binding_elem.name);
                    self.error_at_node_msg(
                        binding_elem.name,
                        diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                        &[&binding_name, "any"],
                    );
                }
            }
        }
    }
    /// Re-check closures that deferred TS7006 during type env building.
    /// Called after `is_checking_statements` is set to true. These closures were
    /// processed before statement-checking mode, so their `skip_implicit_any` was
    /// true. Their cached types prevent `get_type_of_function` from re-running,
    /// so we explicitly walk their parameters and emit TS7006 here.
    pub(crate) fn recheck_deferred_implicit_any_closures(&mut self) {
        let deferred = std::mem::take(&mut self.ctx.deferred_implicit_any_closures);
        for func_idx in deferred {
            if self.closure_has_contextual_type(func_idx) {
                continue;
            }
            // Skip closures with JSDoc annotations — JSDoc @param, @type, @template
            // etc. can provide type information that suppresses TS7006. The normal
            // get_type_of_function path handles this; we conservatively skip here.
            if self.find_jsdoc_for_function(func_idx).is_some() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(func_idx) else {
                continue;
            };
            let parameters = if let Some(func) = self.ctx.arena.get_function(node) {
                &func.parameters
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                &method.parameters
            } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                &accessor.parameters
            } else {
                continue;
            };
            let param_nodes: Vec<_> = parameters.nodes.clone();
            let mut param_index = 0;
            for &param_idx in &param_nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                // Skip `this` parameter
                if let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text.as_str() == "this"
                {
                    continue;
                }
                // Skip parameters with type annotations
                if param.type_annotation.is_some() {
                    param_index += 1;
                    continue;
                }
                self.maybe_report_implicit_any_parameter(param, false, param_index);
                param_index += 1;
            }
            self.ctx.implicit_any_checked_closures.insert(func_idx);
        }

        // Re-check closures whose TS7006 was emitted during return-type inference
        // speculation and then rolled back. These closures had genuinely untyped
        // parameters at the time of first processing (inside infer_return_type_from_body).
        // Even if a later call inference retry provided contextual types (adding the
        // closure to implicit_any_contextual_closures), tsc would have kept the TS7006
        // from the initial inference pass. So we unconditionally re-emit here.
        let speculative = std::mem::take(&mut self.ctx.speculative_implicit_any_closures);
        for func_idx in speculative {
            if self.find_jsdoc_for_function(func_idx).is_some() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(func_idx) else {
                continue;
            };
            let parameters = if let Some(func) = self.ctx.arena.get_function(node) {
                &func.parameters
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                &method.parameters
            } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                &accessor.parameters
            } else {
                continue;
            };
            let param_nodes: Vec<_> = parameters.nodes.clone();
            let mut param_index = 0;
            for &param_idx in &param_nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text.as_str() == "this"
                {
                    continue;
                }
                if param.type_annotation.is_some() {
                    param_index += 1;
                    continue;
                }
                self.maybe_report_implicit_any_parameter(param, false, param_index);
                param_index += 1;
            }
        }
    }

    /// Walk a type annotation looking for `FunctionType`/`ConstructorType` nodes and
    /// emit TS7006/TS7019 for any parameters that lack explicit type annotations when
    /// `--noImplicitAny` is enabled.
    ///
    /// Called for class property type annotations in ambient (declare) classes, where
    /// `check_type_for_missing_names` is not invoked because there is no initializer.
    /// Example: `pub_f10: (x) => string` — tsc emits TS7006 for `x`.
    pub(crate) fn check_type_annotation_for_implicit_any_params(&mut self, type_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    for (pi, &param_idx) in func_type.parameters.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                    // Recurse into return type for nested function types like `() => (x) => void`
                    if func_type.type_annotation.is_some() {
                        self.check_type_annotation_for_implicit_any_params(
                            func_type.type_annotation,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in composite.types.nodes.clone().iter() {
                        self.check_type_annotation_for_implicit_any_params(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_annotation_for_implicit_any_params(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_annotation_for_implicit_any_params(wrapped.type_node);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    fn check_codes_no_implicit_any(source: &str) -> Vec<u32> {
        crate::test_utils::check_source(
            source,
            "test.ts",
            crate::context::CheckerOptions {
                no_implicit_any: true,
                ..crate::context::CheckerOptions::default()
            },
        )
        .iter()
        .map(|d| d.code)
        .collect()
    }

    fn count_code(codes: &[u32], code: u32) -> usize {
        codes.iter().filter(|c| **c == code).count()
    }

    #[test]
    fn filter_callback_on_any_receiver_still_reports_ts7006() {
        let codes =
            check_codes_no_implicit_any("declare const values: any; values.filter(x => x);");
        assert_eq!(
            count_code(&codes, 7006),
            1,
            "expected TS7006 for callback parameter on any.filter, got {codes:?}"
        );
    }

    #[test]
    fn filter_callback_on_any_object_keys_result_still_reports_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
declare const util: { objectKeys: any };
const keys = util.objectKeys({});
keys.filter(k => k);
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            1,
            "expected TS7006 when objectKeys itself is any, got {codes:?}"
        );
    }

    #[test]
    fn filter_callback_on_unresolved_array_receiver_suppresses_ts7006() {
        let codes =
            check_codes_no_implicit_any("declare const values: Missing[]; values.filter(x => x);");
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 when the receiver is explicitly array-like, got {codes:?}"
        );
    }

    #[test]
    fn filter_callback_on_destructured_array_property_suppresses_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
function makeIssue(params: { errorMaps: (Missing | undefined)[] }) {
    const { errorMaps } = params;
    errorMaps.filter(m => !!m);
}
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 for destructured array-like property filter, got {codes:?}"
        );
    }

    #[test]
    fn unresolved_explicit_callable_annotation_defers_arrow_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
type ErrorMap = (iss: { code: string }, ctx: { defaultError: string }) => { message: string };
const customMap: ErrorMap = (iss, ctx) => {
    return { message: ctx.defaultError };
};

const isInteger: NumberConstructor["isInteger"] =
    typeof Number.isInteger === "function" ? val => true : val => false;

const objectKeys: ObjectConstructor["keys"] = (obj: any) => [];
const firstKeys = objectKeys({});
firstKeys.filter(k => k);
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 for arrows with explicit callable annotations, got {codes:?}"
        );
    }

    #[test]
    fn ts7031_emitted_for_array_pattern_index_beyond_array_default() {
        // `[x, y] = [1]` — the default literal `[1]` covers index 0 only, so
        // `y` at index 1 must still report TS7031 (implicit any).
        let codes = check_codes_no_implicit_any("function f02([x, y] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            1,
            "expected exactly one TS7031 (for `y`) in `[x, y] = [1]`, got {codes:?}"
        );
    }

    #[test]
    fn ts7031_emitted_for_array_pattern_index_beyond_array_default_with_inner_default() {
        // `[x = 0, y] = [1]` — `x` has its own default, so no TS7031 for x.
        // `y` at index 1 is still uncovered by the literal and has no own
        // default, so TS7031 must fire for `y`.
        let codes = check_codes_no_implicit_any("function f12([x = 0, y] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            1,
            "expected exactly one TS7031 (for `y`) in `[x = 0, y] = [1]`, got {codes:?}"
        );
    }

    #[test]
    fn no_ts7031_when_array_default_covers_pattern() {
        // `[x, y] = [1, 'foo']` — both indices are covered by the literal,
        // so the bindings are implicitly typed `number` / `string`. No TS7031.
        let codes = check_codes_no_implicit_any("function f03([x, y] = [1, 'foo']) {}");
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when literal default covers all binding indices, got {codes:?}"
        );
    }

    #[test]
    fn no_ts7031_when_inner_default_present_beyond_array_default() {
        // `[x = 0, y = 'bar'] = [1]` — `y` has an own default `'bar'` so it
        // is typed `string`. Even though the literal does not cover index 1,
        // no TS7031 should fire.
        let codes = check_codes_no_implicit_any("function f22([x = 0, y = 'bar'] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when leaves carry their own default, got {codes:?}"
        );
    }

    #[test]
    fn ts7031_for_each_uncovered_index_in_longer_pattern() {
        // `[x, y, z] = [1]` — only index 0 is covered. y and z must each
        // report TS7031.
        let codes = check_codes_no_implicit_any("function fN([x, y, z] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            2,
            "expected TS7031 for both `y` and `z`, got {codes:?}"
        );
    }

    #[test]
    fn no_ts7031_for_array_pattern_with_spread_default() {
        // `[x, y] = [...rest]` — spread makes the literal's effective length
        // not statically known. We conservatively skip TS7031 (matching tsc,
        // which infers a tuple type from the spread context).
        let codes = check_codes_no_implicit_any(
            "declare const rest: number[]; function f([x, y] = [...rest]) {}",
        );
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when default contains a spread, got {codes:?}"
        );
    }

    #[test]
    fn ts7019_emitted_with_rest_not_last_parse_error() {
        // tsc emits TS7019 for rest params even when TS1014 (rest not last) is present.
        // TS1014 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("function f(...x, y) { }");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
        // TS7006 should also be emitted for the regular parameter `y`
        assert!(
            codes.contains(&7006),
            "Should have TS7006 for regular param y, got {codes:?}"
        );
    }

    #[test]
    fn ts7019_emitted_with_syntax_parse_errors_flag() {
        // When has_syntax_parse_errors is set (as in the CLI driver path),
        // rest params should still get TS7019.
        let source = "function f(...x, y) { }";
        let options = crate::context::CheckerOptions::default();
        let mut parser =
            tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
        let sf = parser.parse_source_file();
        let mut binder = tsz_binder::BinderState::new();
        binder.bind_source_file(parser.get_arena(), sf);
        let types = crate::query_boundaries::type_construction::TypeInterner::new();
        let mut checker = crate::state::CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        // Simulate the CLI driver setting has_syntax_parse_errors
        checker.ctx.has_syntax_parse_errors = true;
        checker.check_source_file(sf);
        let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param with has_syntax_parse_errors, got {codes:?}"
        );
    }

    #[test]
    fn ts7019_emitted_with_optional_rest_parse_error() {
        // tsc emits TS7019 for rest params even when TS1047 (rest can't be optional) is present.
        // TS1047 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("(...arg?) => 102;");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
    }
}
