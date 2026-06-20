//! ES5 destructuring lowering for the async/generator IR pipeline.
//!
//! The async→generator transform hoists every binding name to the top of the
//! `__awaiter` callback and runs the body as a state machine, so a destructuring
//! *declaration* (`const { a, b } = obj;`) must be lowered the same way `tsc`
//! lowers it inside a generator: the declared names are hoisted (`var a, b;`)
//! and the pattern is flattened into a bare comma-sequence of *assignments*
//! (`a = obj.a, b = obj.b;`). This mirrors `tsc`'s `flattenDestructuringAssignment`
//! for the generator/async target, which is the reason its output differs from
//! the synchronous printer path (which keeps the `var` keyword inline).
//!
//! The flattening rules implemented here match `tsc` exactly:
//! - A temporary is introduced for the source value only when the pattern has
//!   more than one element *and* the source is not a simple inlineable
//!   expression (identifier/literal/`this`); single-element patterns inline the
//!   source directly (`a = obj.a.b`, `a = h().a`).
//! - Nested patterns chain property/element access through the same source.
//! - Defaults capture the access in a temp and compare `=== void 0`.
//! - Object rest uses `__rest(source, [excluded])`; array rest uses
//!   `source.slice(index)`.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl AsyncES5Transformer<'_> {
    /// Lower a destructuring `const`/`let`/`var` declaration inside an async body.
    ///
    /// Handles the three initializer shapes the surrounding state machine can
    /// produce: a direct `await` (value arrives via `_a.sent()`), an initializer
    /// containing a nested `await`, and an await-free initializer. In every case
    /// the binding names are hoisted and the pattern is flattened into a
    /// comma-sequence of assignments.
    pub(in crate::transforms) fn process_destructuring_declaration_in_async(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        trailing_comment: &mut Option<String>,
    ) {
        if initializer_idx.is_none() {
            return;
        }

        if self.is_suspension_expression(initializer_idx) {
            let trailing = trailing_comment.take();
            self.process_await_expression_with_trailing_comment(
                initializer_idx,
                cases,
                current_statements,
                current_label,
                trailing.as_deref(),
            );
            // The awaited result is `_a.sent()`; tsc parenthesizes it when it is
            // used directly as a member-access base.
            self.emit_destructuring_extraction(
                pattern_idx,
                IRNode::GeneratorSent,
                false,
                true,
                current_statements,
            );
            return;
        }

        if self.contains_await_recursive(initializer_idx) {
            self.emit_nested_suspension(initializer_idx, cases, current_statements, current_label);
            let source = self.expression_to_ir(initializer_idx);
            self.emit_destructuring_extraction(
                pattern_idx,
                source,
                false,
                false,
                current_statements,
            );
            return;
        }

        let inlineable = self.initializer_is_simple_inlineable(initializer_idx);
        let source = self.expression_to_ir(initializer_idx);
        self.emit_destructuring_extraction(
            pattern_idx,
            source,
            inlineable,
            false,
            current_statements,
        );
        if let Some(comment) = trailing_comment.take() {
            current_statements.push(IRNode::TrailingComment(comment.into()));
        }
    }

    /// Flatten `pattern_idx` against `source`, hoist its names, and emit the
    /// extraction as a single bare comma-sequence expression statement.
    pub(in crate::transforms) fn emit_destructuring_extraction(
        &mut self,
        pattern_idx: NodeIndex,
        source: IRNode,
        source_inlineable: bool,
        source_paren_as_member_base: bool,
        current_statements: &mut Vec<IRNode>,
    ) {
        let mut assigns = Vec::new();
        let mut hoist = Vec::new();
        self.flatten_es5_destructuring_binding(
            pattern_idx,
            source,
            source_inlineable,
            source_paren_as_member_base,
            &mut assigns,
            &mut hoist,
        );

        for name in hoist {
            current_statements.push(IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            });
        }

        match assigns.len() {
            0 => {}
            1 => current_statements.push(IRNode::ExpressionStatement(Box::new(
                assigns.pop().expect("len checked == 1"),
            ))),
            _ => current_statements.push(IRNode::ExpressionStatement(Box::new(
                IRNode::CommaSequence(assigns),
            ))),
        }
    }
    /// Return the binding pattern of a catch clause's variable declaration when
    /// its name is an object/array pattern (`catch ({ message })`), else `None`.
    pub(in crate::transforms) fn catch_binding_pattern(
        &self,
        var_decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.arena.get(var_decl_idx)?;
        let decl = self.arena.get_variable_declaration(node)?;
        self.is_binding_pattern_node(decl.name).then_some(decl.name)
    }

    /// True when `idx` is an object or array binding pattern.
    pub(in crate::transforms) fn is_binding_pattern_node(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .is_some_and(tsz_parser::parser::node::Node::is_binding_pattern)
    }

    /// True when an expression can be repeated inline without first being
    /// captured into a temporary, mirroring `tsc`'s `isSimpleInlineableExpression`
    /// (identifiers, string/number literals, and the `this`/`true`/`false`/`null`
    /// keywords). Property/element accesses and calls are *not* inlineable.
    pub(in crate::transforms) fn initializer_is_simple_inlineable(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.is_identifier() || node.is_string_literal() || node.is_numeric_literal() {
            return true;
        }
        matches!(
            node.kind,
            k if k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
        )
    }

    /// Lower a destructuring binding `pattern_idx` against an already-lowered
    /// `source` expression, in the async/generator assignment form.
    ///
    /// Every hoisted name (temporaries plus binding identifiers) is appended to
    /// `hoist` in declaration order; the flattened assignment expressions are
    /// appended to `assigns` in evaluation order. Callers hoist the names via
    /// `VarDecl { initializer: None }` and emit the assignments as a single
    /// `CommaSequence` expression statement.
    ///
    /// `source_inlineable` reports whether `source` may be repeated without a
    /// temp. `source_paren_as_member_base` requests that an inlined `source`
    /// used as a member-access base be wrapped in parentheses (needed for an
    /// awaited value such as `_a.sent()`, which `tsc` prints as
    /// `(_a.sent()).prop`).
    pub(in crate::transforms) fn flatten_es5_destructuring_binding(
        &mut self,
        pattern_idx: NodeIndex,
        source: IRNode,
        source_inlineable: bool,
        source_paren_as_member_base: bool,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let is_array = pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let element_indices = pattern.elements.nodes.clone();
        let num_elements = element_indices.len();
        // A dynamic (computed) property key forces the source into a temp even
        // for a single-element pattern, because tsc captures both the source and
        // the key in temporaries before indexing (`_a = obj, _b = k, a = _a[_b]`).
        let has_computed_key = !is_array
            && element_indices
                .iter()
                .any(|&e| self.binding_element_has_dynamic_computed_key(e));

        // tsc captures the source in a temp when the pattern has more than one
        // element and the source is not already inlineable (or whenever a
        // computed key is present). Single-element patterns otherwise reference
        // the source exactly once and inline it directly.
        let base = if (num_elements != 1 && !source_inlineable) || has_computed_key {
            let temp = self.generate_hoisted_temp();
            hoist.push(temp.clone());
            assigns.push(IRNode::assign(IRNode::id(temp.clone()), source));
            IRNode::id(temp)
        } else if source_paren_as_member_base && !source_inlineable {
            IRNode::Parenthesized(Box::new(source))
        } else {
            source
        };

        let mut excluded: Vec<IRNode> = Vec::new();
        for (index, &element_idx) in element_indices.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(binding_elem) = self.arena.get_binding_element(element_node) else {
                continue;
            };

            if binding_elem.dot_dot_dot_token {
                let target = self.binding_name_ir(binding_elem.name, hoist);
                let rest_value = if is_array {
                    IRNode::call(
                        IRNode::prop(base.clone(), "slice"),
                        vec![IRNode::number(index.to_string())],
                    )
                } else {
                    self.helpers_needed.mark_rest();
                    IRNode::call(
                        IRNode::RuntimeHelper("__rest".into()),
                        vec![
                            base.clone(),
                            IRNode::ArrayLiteral(std::mem::take(&mut excluded)),
                        ],
                    )
                };
                if let Some(target) = target {
                    assigns.push(IRNode::assign(target, rest_value));
                }
                continue;
            }

            // Build the property/element access for this binding.
            let access = if is_array {
                IRNode::elem(base.clone(), IRNode::number(index.to_string()))
            } else {
                let (member_access, key_for_rest) = self.object_member_access(
                    &base,
                    binding_elem.property_name,
                    binding_elem.name,
                    assigns,
                    hoist,
                );
                if let Some(key) = key_for_rest {
                    excluded.push(key);
                }
                member_access
            };

            self.flatten_binding_element(
                binding_elem.name,
                binding_elem.initializer,
                access,
                assigns,
                hoist,
            );
        }
    }

    /// True when an object binding element uses a *dynamic* computed property
    /// name. A computed key whose expression is a string/numeric literal
    /// (`["x"]`, `[0]`) is static — tsc lowers it to plain element access and
    /// does not force the source into a temp.
    fn binding_element_has_dynamic_computed_key(&self, element_idx: NodeIndex) -> bool {
        let Some(element_node) = self.arena.get(element_idx) else {
            return false;
        };
        let Some(binding_elem) = self.arena.get_binding_element(element_node) else {
            return false;
        };
        self.dynamic_computed_key_expr(binding_elem.property_name)
            .is_some()
    }

    /// Returns the inner expression of a dynamic computed property key, or `None`
    /// when the property name is absent, not computed, or a static literal.
    fn dynamic_computed_key_expr(&self, property_name_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(property_name_idx)?;
        if node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(node)?;
        let expr = self.arena.get(computed.expression)?;
        let is_literal = expr.is_string_literal() || expr.is_numeric_literal();
        (!is_literal).then_some(computed.expression)
    }

    /// Flatten a single (non-rest) binding element whose value is reached via
    /// `access`, handling an optional default and nested sub-pattern.
    fn flatten_binding_element(
        &mut self,
        name_idx: NodeIndex,
        initializer_idx: NodeIndex,
        access: IRNode,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let has_default = initializer_idx.is_some();
        let is_nested = self.is_binding_pattern_node(name_idx);

        let value = if has_default {
            // Capture the access so the default check evaluates it once:
            // `_t = access, X = _t === void 0 ? default : _t`. A plain
            // identifier access is already single-eval, so tsc reuses it.
            let value_src = if matches!(access, IRNode::Identifier(_)) {
                access
            } else {
                let temp = self.generate_hoisted_temp();
                hoist.push(temp.clone());
                assigns.push(IRNode::assign(IRNode::id(temp.clone()), access));
                IRNode::id(temp)
            };
            let default_ir = self.expression_to_ir(initializer_idx);
            IRNode::ConditionalExpr {
                condition: Box::new(IRNode::binary(value_src.clone(), "===", IRNode::Undefined)),
                when_true: Box::new(default_ir),
                when_false: Box::new(value_src),
            }
        } else {
            access
        };

        if is_nested {
            if has_default {
                // The defaulted value must be materialized before recursing so
                // the nested pattern reads from a stable temp.
                let temp = self.generate_hoisted_temp();
                hoist.push(temp.clone());
                assigns.push(IRNode::assign(IRNode::id(temp.clone()), value));
                self.flatten_es5_destructuring_binding(
                    name_idx,
                    IRNode::id(temp),
                    true,
                    false,
                    assigns,
                    hoist,
                );
            } else {
                self.flatten_es5_destructuring_binding(
                    name_idx, value, false, false, assigns, hoist,
                );
            }
            return;
        }

        if let Some(target) = self.binding_name_ir(name_idx, hoist) {
            assigns.push(IRNode::assign(target, value));
        }
    }

    /// Build the member access for an object binding element and, for non-computed
    /// keys, return the property-key literal that must be threaded into a trailing
    /// `__rest` exclusion list.
    fn object_member_access(
        &mut self,
        base: &IRNode,
        property_name_idx: NodeIndex,
        name_idx: NodeIndex,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) -> (IRNode, Option<IRNode>) {
        // Shorthand `{ a }`: the property key is the binding name.
        if property_name_idx.is_none() {
            let key = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
            return (
                IRNode::prop(base.clone(), key.clone()),
                (!key.is_empty()).then(|| IRNode::string(key)),
            );
        }

        let Some(key_node) = self.arena.get(property_name_idx) else {
            return (base.clone(), None);
        };

        // A dynamic computed key `{ [k]: a }` captures the key in its own temp
        // ahead of the access (`_b = k, a = _a[_b]`) and uses that temp as the
        // rest exclusion entry, matching tsc's single-evaluation of dynamic keys.
        // A computed key wrapping a string/numeric literal (`["x"]`, `[0]`) is
        // static: rebind to the inner literal and fall through to literal access.
        let key_node = if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(key_expr_idx) = self.dynamic_computed_key_expr(property_name_idx) {
                let key_expr = self.expression_to_ir(key_expr_idx);
                let key_temp = self.generate_hoisted_temp();
                hoist.push(key_temp.clone());
                assigns.push(IRNode::assign(IRNode::id(key_temp.clone()), key_expr));
                // A rest sibling excludes a dynamic key by its runtime form,
                // coerced to a string unless it is a symbol (tsc's __rest key
                // form): `typeof _b === "symbol" ? _b : _b + ""`.
                let key_ref = IRNode::id(key_temp);
                let rest_key = IRNode::ConditionalExpr {
                    condition: Box::new(IRNode::binary(
                        IRNode::PrefixUnaryExpr {
                            operator: "typeof ".into(),
                            operand: Box::new(key_ref.clone()),
                        },
                        "===",
                        IRNode::string("symbol"),
                    )),
                    when_true: Box::new(key_ref.clone()),
                    when_false: Box::new(IRNode::binary(key_ref.clone(), "+", IRNode::string(""))),
                };
                return (IRNode::elem(base.clone(), key_ref), Some(rest_key));
            }
            // Static literal computed key: rebind to the inner literal node.
            match self
                .arena
                .get_computed_property(key_node)
                .and_then(|computed| self.arena.get(computed.expression))
            {
                Some(inner) => inner,
                None => return (base.clone(), None),
            }
        } else {
            key_node
        };

        // String / numeric literal key `{ "a-b": x }` / `{ 0: x }`: element access.
        // The rest exclusion entry is always the string form; only the access
        // index distinguishes a numeric key (`x[0]`) from a string key (`x["k"]`).
        if key_node.is_string_literal() || key_node.is_numeric_literal() {
            if let Some(lit) = self.arena.get_literal(key_node) {
                let text = lit.text.clone();
                let access_key = if key_node.is_numeric_literal() {
                    IRNode::number(text.clone())
                } else {
                    IRNode::string(text.clone())
                };
                return (
                    IRNode::elem(base.clone(), access_key),
                    Some(IRNode::string(text)),
                );
            }
        }

        // Identifier key (renamed binding `{ a: b }`).
        let key =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, property_name_idx);
        (
            IRNode::prop(base.clone(), key.clone()),
            (!key.is_empty()).then(|| IRNode::string(key)),
        )
    }

    /// Resolve a binding-element `name` (always an identifier in non-nested
    /// position) to its assignment target, recording it for hoisting.
    fn binding_name_ir(&self, name_idx: NodeIndex, hoist: &mut Vec<String>) -> Option<IRNode> {
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
        if name.is_empty() {
            return None;
        }
        hoist.push(name.clone());
        Some(IRNode::id(name))
    }
}
