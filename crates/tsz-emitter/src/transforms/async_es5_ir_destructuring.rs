//! ES5 destructuring lowering for binding patterns inside async generator
//! bodies.
//!
//! The async-to-generator IR pipeline lowers statements into generator
//! opcodes. Binding patterns in `var`/`const`/`let` declarations and in
//! `catch` clauses must be down-leveled to plain member-access assignments,
//! because ES5/ES3 has no destructuring syntax. This module mirrors `tsc`'s
//! `flattenBindingOrAssignmentElement`
//! (`src/compiler/transformers/destructuring.ts`): it walks a binding pattern
//! and produces, for a given source value,
//!
//! - a list of hoisted variable names (leaf bindings plus any temporaries), and
//! - a single comma-joined assignment expression that extracts each binding.
//!
//! The generator hoist pass (`async_es5_ir_hoists`) lifts the names into the
//! `__awaiter` wrapper's `var` list; the comma expression stays inline at the
//! point of declaration, matching `tsc`'s
//! `transformAndEmitVariableDeclarationList`, which joins all initialized
//! declarations of one list with `inlineExpressions`.
//!
//! The `__rest`/`__read` helper *definitions* are registered by the Phase-1
//! lowering pass (`crate::lowering`), which scans every binding pattern in the
//! tree regardless of async context; this module only emits the call sites via
//! [`IRNode::RuntimeHelper`].

use super::AsyncES5Transformer;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Accumulator for a destructuring lowering: hoisted variable names (in
/// declaration order) and the assignment expressions to comma-join.
#[derive(Default)]
struct DestructuringLowering {
    /// Names to hoist into the surrounding `var` list, in the order `tsc`
    /// would hoist them (temporaries appear where they are created).
    names: Vec<String>,
    /// Assignment expressions, in evaluation order, to comma-join into a
    /// single expression statement.
    assigns: Vec<IRNode>,
}

impl DestructuringLowering {
    /// Record a hoisted name, de-duplicating so the `var` list stays clean.
    fn push_name(&mut self, name: String) {
        if !self.names.contains(&name) {
            self.names.push(name);
        }
    }

    /// Fold the collected assignments into a single comma expression. A
    /// left-folded binary `,` chain prints without the wrapping parentheses a
    /// `CommaExpr` would add, matching `tsc`'s statement-level comma list.
    fn into_comma_expression(self) -> Option<IRNode> {
        self.assigns
            .into_iter()
            .reduce(|acc, next| IRNode::binary(acc, ",", next))
    }
}

impl AsyncES5Transformer<'_> {
    /// Lower a binding-pattern declaration whose name is an object or array
    /// binding pattern. Returns the statements to splice into the current
    /// generator case: an initializer-less `var` declaration for every
    /// extracted name (lifted by the hoist pass) followed by one comma-joined
    /// extraction statement.
    ///
    /// `source` is the value being destructured. Callers pass the initializer
    /// identifier directly (when it is a plain identifier the pattern does not
    /// rebind) or a temp that already holds the value. When `force_source_temp`
    /// is set the source is captured into a fresh temp first, even if it is a
    /// plain identifier — required when the pattern rebinds that identifier
    /// (`var { foo, baz } = foo`), so later bindings read the original value.
    pub(in crate::transforms) fn lower_binding_pattern_statements(
        &self,
        pattern_idx: NodeIndex,
        source: IRNode,
        force_source_temp: bool,
    ) -> Vec<IRNode> {
        let mut lowering = DestructuringLowering::default();
        let source = if force_source_temp && matches!(source, IRNode::Identifier(_)) {
            self.ensure_identifier(source, false, &mut lowering)
        } else {
            source
        };
        self.flatten_binding_pattern(pattern_idx, source, &mut lowering);
        let mut statements = Vec::new();
        for name in &lowering.names {
            statements.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
        }
        if let Some(expr) = lowering.into_comma_expression() {
            statements.push(IRNode::ExpressionStatement(Box::new(expr)));
        }
        statements
    }

    /// Down-level a synchronous destructuring declaration (`{ a } = init`)
    /// where `init` does not suspend: extract from the initializer value,
    /// capturing it into a temp first only when the pattern rebinds the source
    /// identifier (`var { foo } = foo`). Shared by the async-statement and
    /// `statement_to_ir` declaration paths.
    pub(in crate::transforms) fn lower_sync_destructuring_declaration(
        &self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> Vec<IRNode> {
        let force_source_temp = self.destructuring_source_needs_temp(pattern_idx, initializer_idx);
        let source = self.expression_to_ir(initializer_idx);
        self.lower_binding_pattern_statements(pattern_idx, source, force_source_temp)
    }

    /// Whether the initializer is a plain identifier that the pattern also
    /// rebinds as a leaf — the `var { foo } = foo` hazard. Such a source must
    /// be captured into a temp before extraction.
    pub(in crate::transforms) fn destructuring_source_needs_temp(
        &self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer_idx) else {
            return false;
        };
        if !init_node.is_identifier() {
            return false;
        }
        let ident =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, initializer_idx);
        if ident.is_empty() {
            return false;
        }
        let mut names = Vec::new();
        self.collect_binding_name(pattern_idx, &mut names);
        names.iter().any(|name| name == &ident)
    }

    /// If a `catch` clause binds a destructuring pattern, return the pattern
    /// node so the caller can extract it from the caught value.
    pub(in crate::transforms) fn catch_binding_pattern(
        &self,
        var_decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let var_node = self.arena.get(var_decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(var_node)?;
        if self.is_binding_pattern_name(var_decl.name) {
            Some(var_decl.name)
        } else {
            None
        }
    }

    /// Whether the declaration name is a destructuring binding pattern.
    pub(in crate::transforms) fn is_binding_pattern_name(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(tsz_parser::parser::node::Node::is_binding_pattern)
    }

    /// `tsc`'s `flattenObjectBindingOrAssignmentPattern` /
    /// `flattenArrayBindingOrAssignmentPattern` dispatch.
    fn flatten_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        out: &mut DestructuringLowering,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            self.flatten_object_pattern(pattern_idx, value, out);
        } else if node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            self.flatten_array_pattern(pattern_idx, value, out);
        }
    }

    fn flatten_object_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        out: &mut DestructuringLowering,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let elements: Vec<NodeIndex> = pattern.elements.nodes.clone();
        let num_elements = elements.len();

        // For anything other than a single-element pattern `tsc` evaluates the
        // value exactly once via a temp (reusing an identifier source). Empty
        // patterns still force a temp so a value with side effects is evaluated.
        let value = if num_elements != 1 {
            self.ensure_identifier(value, num_elements != 0, out)
        } else {
            value
        };

        // Computed-key temps in source order, consumed by `__rest`'s exclusion
        // list to mirror `tsc`'s `computedTempVariables` threading.
        let mut computed_temps: Vec<IRNode> = Vec::new();
        for (i, &element_idx) in elements.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(element) = self.arena.get_binding_element(element_node).cloned() else {
                continue;
            };

            if element.dot_dot_dot_token {
                if i != num_elements - 1 {
                    continue;
                }
                let rest_value = self.object_rest_value(value.clone(), &elements, &computed_temps);
                self.flatten_binding_element(element.name, rest_value, NodeIndex::NONE, out);
                continue;
            }

            let property_name = if element.property_name.is_some() {
                element.property_name
            } else {
                element.name
            };
            let (access, computed_temp) =
                self.destructuring_property_access(value.clone(), property_name, out);
            if let Some(temp) = computed_temp {
                computed_temps.push(temp);
            }
            self.flatten_binding_element(element.name, access, element.initializer, out);
        }
    }

    fn flatten_array_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        out: &mut DestructuringLowering,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let elements: Vec<NodeIndex> = pattern.elements.nodes.clone();
        let num_elements = elements.len();

        let all_omitted = !elements.is_empty()
            && elements.iter().all(|&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::OMITTED_EXPRESSION)
            });

        let has_trailing_rest = elements.last().is_some_and(|&idx| {
            self.arena
                .get(idx)
                .and_then(|n| self.arena.get_binding_element(n))
                .is_some_and(|e| e.dot_dot_dot_token)
        });

        let value = if self.downlevel_iteration {
            // Read the elements of the iterable into an array, then bind.
            let read_count = if has_trailing_rest {
                None
            } else {
                Some(num_elements)
            };
            let read_value = Self::array_read_helper(value, read_count);
            self.ensure_identifier(read_value, false, out)
        } else if num_elements != 1 || all_omitted {
            self.ensure_identifier(value, num_elements != 0, out)
        } else {
            value
        };

        for (i, &element_idx) in elements.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(element) = self.arena.get_binding_element(element_node).cloned() else {
                continue;
            };

            if element.dot_dot_dot_token {
                if i != num_elements - 1 {
                    continue;
                }
                let slice = IRNode::call(
                    IRNode::prop(value.clone(), "slice"),
                    vec![IRNode::number(i.to_string())],
                );
                self.flatten_binding_element(element.name, slice, NodeIndex::NONE, out);
                continue;
            }

            let access = IRNode::elem(value.clone(), IRNode::number(i.to_string()));
            self.flatten_binding_element(element.name, access, element.initializer, out);
        }
    }

    /// `tsc`'s `flattenBindingOrAssignmentElement`: apply a default-value
    /// check, then either recurse into a nested pattern or emit the leaf
    /// assignment.
    fn flatten_binding_element(
        &self,
        name_idx: NodeIndex,
        value: IRNode,
        initializer_idx: NodeIndex,
        out: &mut DestructuringLowering,
    ) {
        let value = if initializer_idx.is_some() {
            let checked = self.default_value_check(value, initializer_idx, out);
            // When the default is not a simple inlineable expression and the
            // target is itself a pattern, `tsc` binds the checked value to a
            // temp so side effects run before the nested destructuring.
            if self.is_binding_pattern_name(name_idx)
                && !self.is_simple_inlineable_expression(initializer_idx)
            {
                self.ensure_identifier(checked, true, out)
            } else {
                checked
            }
        } else {
            value
        };

        if self.is_binding_pattern_name(name_idx) {
            self.flatten_binding_pattern(name_idx, value, out);
            return;
        }

        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
        if name.is_empty() {
            return;
        }
        out.push_name(name.clone());
        out.assigns.push(IRNode::assign(IRNode::id(name), value));
    }

    /// `tsc`'s `createDefaultValueCheck`: bind `value` to a temp (reusing an
    /// identifier) and return `temp === void 0 ? default : temp`.
    fn default_value_check(
        &self,
        value: IRNode,
        initializer_idx: NodeIndex,
        out: &mut DestructuringLowering,
    ) -> IRNode {
        let value = self.ensure_identifier(value, true, out);
        let default_value = self.expression_to_ir(initializer_idx);
        IRNode::ConditionalExpr {
            condition: Box::new(IRNode::binary(value.clone(), "===", IRNode::Undefined)),
            when_true: Box::new(default_value),
            when_false: Box::new(value),
        }
    }

    /// `tsc`'s `createDestructuringPropertyAccess`: `value.name`,
    /// `value["literal"]`, or `value[_t]` for a computed key (cached in a temp).
    /// Returns the access expression and, for computed keys, the key temp so
    /// the caller can thread it into a trailing `__rest` exclusion list.
    fn destructuring_property_access(
        &self,
        value: IRNode,
        property_name_idx: NodeIndex,
        out: &mut DestructuringLowering,
    ) -> (IRNode, Option<IRNode>) {
        let Some(name_node) = self.arena.get(property_name_idx) else {
            return (value, None);
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                let key = self.expression_to_ir(computed.expression);
                let key = self.ensure_identifier(key, false, out);
                return (IRNode::elem(value, key.clone()), Some(key));
            }
            return (value, None);
        }
        if name_node.is_string_literal() {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return (IRNode::elem(value, IRNode::string(lit.text.clone())), None);
            }
        }
        if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return (IRNode::elem(value, IRNode::number(lit.text.clone())), None);
            }
        }
        let name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, property_name_idx);
        (IRNode::prop(value, name), None)
    }

    /// `tsc`'s `createRestHelper`: `__rest(value, ["a", "b", ...])` excluding
    /// every non-rest property name that precedes the rest element. Computed
    /// keys reuse their cached temp via `typeof _t === "symbol" ? _t : _t + ""`.
    fn object_rest_value(
        &self,
        value: IRNode,
        elements: &[NodeIndex],
        computed_temps: &[IRNode],
    ) -> IRNode {
        let mut excluded = Vec::new();
        let mut computed_offset = 0usize;
        for &element_idx in &elements[..elements.len().saturating_sub(1)] {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.arena.get_binding_element(element_node) else {
                continue;
            };
            if element.dot_dot_dot_token {
                continue;
            }
            let property_name = if element.property_name.is_some() {
                element.property_name
            } else {
                element.name
            };
            let Some(name_node) = self.arena.get(property_name) else {
                continue;
            };
            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                if let Some(temp) = computed_temps.get(computed_offset) {
                    computed_offset += 1;
                    excluded.push(IRNode::ConditionalExpr {
                        condition: Box::new(IRNode::binary(
                            IRNode::PrefixUnaryExpr {
                                operator: "typeof ".into(),
                                operand: Box::new(temp.clone()),
                            },
                            "===",
                            IRNode::string("symbol"),
                        )),
                        when_true: Box::new(temp.clone()),
                        when_false: Box::new(IRNode::binary(temp.clone(), "+", IRNode::string(""))),
                    });
                }
                continue;
            }
            if name_node.is_string_literal() || name_node.kind == SyntaxKind::NumericLiteral as u16
            {
                if let Some(lit) = self.arena.get_literal(name_node) {
                    excluded.push(IRNode::string(lit.text.clone()));
                }
                continue;
            }
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, property_name);
            if !name.is_empty() {
                excluded.push(IRNode::string(name));
            }
        }
        IRNode::call(
            IRNode::RuntimeHelper("__rest".into()),
            vec![value, IRNode::ArrayLiteral(excluded)],
        )
    }

    /// `tsc`'s array `createReadHelper`: `__read(value, count)`.
    fn array_read_helper(value: IRNode, count: Option<usize>) -> IRNode {
        let mut args = vec![value];
        if let Some(count) = count {
            args.push(IRNode::number(count.to_string()));
        }
        IRNode::call(IRNode::RuntimeHelper("__read".into()), args)
    }

    /// `tsc`'s `ensureIdentifier`: return `value` unchanged when it is already
    /// an identifier and reuse is allowed; otherwise allocate a hoisted temp,
    /// emit `temp = value`, and return the temp.
    fn ensure_identifier(
        &self,
        value: IRNode,
        reuse_identifier: bool,
        out: &mut DestructuringLowering,
    ) -> IRNode {
        if reuse_identifier && matches!(value, IRNode::Identifier(_)) {
            return value;
        }
        let temp = self.generate_hoisted_temp();
        out.push_name(temp.clone());
        out.assigns
            .push(IRNode::assign(IRNode::id(temp.clone()), value));
        IRNode::id(temp)
    }

    /// Whether the initializer expression is a simple inlineable expression
    /// (`tsc`'s `isSimpleInlineableExpression`): literals and identifiers that
    /// can be duplicated without changing evaluation semantics.
    fn is_simple_inlineable_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        node.is_identifier()
            || node.is_string_literal()
            || node.kind == SyntaxKind::NumericLiteral as u16
            || node.kind == SyntaxKind::BigIntLiteral as u16
            || node.kind == SyntaxKind::TrueKeyword as u16
            || node.kind == SyntaxKind::FalseKeyword as u16
            || node.kind == SyntaxKind::NullKeyword as u16
    }
}
