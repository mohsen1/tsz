//! ES5 destructuring lowering for the async->generator IR pipeline.
//!
//! Inside an `async` function lowered to ES5, a destructuring `var`/`const`
//! declaration (or a destructuring `catch` binding) cannot be emitted as a
//! native binding pattern -- ES5 has no destructuring syntax. `tsc` rewrites
//! each pattern into a comma sequence of plain assignments that reads the
//! source once (via a temp when it is not a reusable identifier) and extracts
//! every bound name, e.g.
//!
//! ```js
//! // const { a, b } = obj;   -->   a = obj.a, b = obj.b;
//! // const { a, b } = g();   -->   _a = g(), a = _a.a, b = _a.b;
//! // const [x, ...r] = arr;  -->   x = arr[0], r = arr.slice(1);
//! ```
//!
//! The bound names (and any temps) are hoisted to the top of the `__generator`
//! wrapper exactly like every other generator-local `var`, so this module emits
//! hoist-only `VarDecl { initializer: None }` nodes for them and a single
//! `ExpressionStatement` holding the comma-joined assignment chain. The shared
//! `extract_and_remove_var_decl_groups` pass then collects the hoist names and
//! leaves the assignment statement in place.
//!
//! This mirrors the synchronous printer-path destructuring lowering
//! (`emitter::es5::bindings_patterns`) but produces `IRNode`s instead of writing
//! directly, since the async pipeline works on IR.

use super::AsyncES5Transformer;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl AsyncES5Transformer<'_> {
    /// Whether a variable statement declares at least one binding pattern, so it
    /// must route through the destructuring lowering rather than the plain
    /// identifier path.
    pub(in crate::transforms) fn variable_statement_has_binding_pattern(
        &self,
        var_node: &Node,
    ) -> bool {
        self.variable_statement_declarations(var_node)
            .into_iter()
            .any(|decl_idx| {
                self.arena
                    .get(decl_idx)
                    .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                    .is_some_and(|decl| self.is_binding_pattern_name(decl.name))
            })
    }

    /// Flatten a `VARIABLE_STATEMENT` into its `VARIABLE_DECLARATION` indices,
    /// transparently descending through the `VARIABLE_DECLARATION_LIST` level.
    fn variable_statement_declarations(&self, var_node: &Node) -> Vec<NodeIndex> {
        let mut out = Vec::new();
        let Some(var_data) = self.arena.get_variable(var_node) else {
            return out;
        };
        for &child_idx in &var_data.declarations.nodes {
            let Some(child_node) = self.arena.get(child_idx) else {
                continue;
            };
            if child_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                out.push(child_idx);
            } else if child_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                if let Some(list) = self.arena.get_variable(child_node) {
                    out.extend(list.declarations.nodes.iter().copied());
                }
            }
        }
        out
    }

    /// If a `catch` clause's variable declaration binds a destructuring
    /// pattern, return the pattern node; otherwise `None` (plain identifier or
    /// no binding).
    pub(in crate::transforms) fn catch_binding_pattern(
        &self,
        var_decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let var_node = self.arena.get(var_decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(var_node)?;
        self.is_binding_pattern_name(var_decl.name)
            .then_some(var_decl.name)
    }

    /// Lower a destructuring variable declaration whose initializer is read
    /// synchronously (no `await`) into the hoist-only declarations + comma
    /// assignment chain, pushing them into `out`. Used for binding-pattern
    /// declarations that share a statement with an awaited declaration.
    pub(in crate::transforms) fn lower_destructuring_declaration_value(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        out: &mut Vec<IRNode>,
    ) {
        let reusable = matches!(value, IRNode::Identifier(_));
        let mut hoist = Vec::new();
        let mut assigns = Vec::new();
        self.lower_binding_pattern(pattern_idx, value, reusable, &mut assigns, &mut hoist);
        Self::push_hoist_and_assignments(hoist, assigns, out);
    }

    /// Lower a destructuring variable declaration whose initializer is a direct
    /// `await`. The bound names + temps must be hoisted *before* the yield, and
    /// the extraction (reading the resumed `_x.sent()` value) emitted *after*
    /// the resume; the caller emits the yield between the two returned lists.
    pub(in crate::transforms) fn split_suspended_destructuring_declaration(
        &self,
        pattern_idx: NodeIndex,
    ) -> (Vec<IRNode>, Option<IRNode>) {
        let mut hoist = Vec::new();
        let mut assigns = Vec::new();
        // The resumed value (`_x.sent()`) is not a reusable identifier, so a
        // multi-element pattern captures it into a temp (`_a = _x.sent()`) while
        // a single-element pattern reads it inline (`a = (_x.sent()).a`).
        self.lower_binding_pattern(
            pattern_idx,
            IRNode::GeneratorSent,
            false,
            &mut assigns,
            &mut hoist,
        );
        let decls = hoist
            .into_iter()
            .map(|name| IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            })
            .collect();
        (decls, Self::comma_assignment_statement(assigns))
    }

    /// True when `name_idx` is an object/array binding pattern (not an
    /// identifier).
    pub(in crate::transforms) fn is_binding_pattern_name(&self, name_idx: NodeIndex) -> bool {
        self.arena.get(name_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        })
    }

    /// Lower a variable statement containing at least one binding pattern into a
    /// hoist + comma-assignment `Sequence`, matching `tsc`'s generator emit.
    pub(in crate::transforms) fn lower_destructuring_variable_statement(
        &self,
        var_node: &Node,
    ) -> IRNode {
        let mut hoist = Vec::new();
        let mut assigns = Vec::new();

        for decl_idx in self.variable_statement_declarations(var_node) {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if self.is_binding_pattern_name(decl.name) {
                if decl.initializer.is_none() {
                    // A binding pattern without an initializer is not valid
                    // TypeScript; nothing to extract.
                    continue;
                }
                let value = self.expression_to_ir(decl.initializer);
                let reusable = matches!(value, IRNode::Identifier(_));
                self.lower_binding_pattern(decl.name, value, reusable, &mut assigns, &mut hoist);
            } else {
                // Plain identifier declaration in the same statement: `tsc`
                // joins it into the same comma chain (e.g.
                // `const a = 1, { b } = obj;` -> `a = 1, b = obj.b`).
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
                if name.is_empty() {
                    continue;
                }
                Self::push_hoist(&mut hoist, &name);
                if decl.initializer.is_some() {
                    assigns.push(IRNode::assign(
                        IRNode::id(name),
                        self.expression_to_ir(decl.initializer),
                    ));
                }
            }
        }

        Self::assemble_destructuring_sequence(hoist, assigns)
    }

    /// Lower a destructuring `catch` binding into the assignment statements that
    /// read each bound name out of `catch_temp` (the temp holding the caught
    /// value). The caller has already emitted `catch_temp = _x.sent()`.
    pub(in crate::transforms) fn lower_catch_binding_destructuring(
        &self,
        pattern_idx: NodeIndex,
        catch_temp: &str,
        out: &mut Vec<IRNode>,
    ) {
        let mut hoist = Vec::new();
        let mut assigns = Vec::new();
        // The catch temp is an identifier, so it is reused directly (no extra
        // temp), matching `tsc`'s `_a = _b.sent(); a = _a.a, b = _a.b;`.
        self.lower_binding_pattern(
            pattern_idx,
            IRNode::id(catch_temp.to_string()),
            true,
            &mut assigns,
            &mut hoist,
        );
        Self::push_hoist_and_assignments(hoist, assigns, out);
    }

    /// Push hoist-only `var` declarations for each name, followed by the
    /// comma-joined assignment chain, into `out`.
    fn push_hoist_and_assignments(hoist: Vec<String>, assigns: Vec<IRNode>, out: &mut Vec<IRNode>) {
        for name in hoist {
            out.push(IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            });
        }
        if let Some(stmt) = Self::comma_assignment_statement(assigns) {
            out.push(stmt);
        }
    }

    /// Assemble the hoist-only declarations and the comma-joined assignment
    /// statement into a single `Sequence` returned from `statement_to_ir`.
    fn assemble_destructuring_sequence(hoist: Vec<String>, assigns: Vec<IRNode>) -> IRNode {
        let mut stmts = Vec::with_capacity(hoist.len() + 1);
        Self::push_hoist_and_assignments(hoist, assigns, &mut stmts);
        if stmts.len() == 1 {
            stmts.into_iter().next().expect("len checked")
        } else {
            IRNode::Sequence(stmts)
        }
    }

    /// Build the `ExpressionStatement` holding the comma-joined assignment
    /// chain (`a = obj.a, b = obj.b`). A `CommaExpr` directly under an
    /// `ExpressionStatement` is rendered without the surrounding parentheses.
    fn comma_assignment_statement(mut assigns: Vec<IRNode>) -> Option<IRNode> {
        match assigns.len() {
            0 => None,
            1 => Some(IRNode::ExpressionStatement(Box::new(
                assigns.pop().expect("len checked"),
            ))),
            _ => Some(IRNode::ExpressionStatement(Box::new(IRNode::CommaExpr(
                assigns,
            )))),
        }
    }

    fn push_hoist(hoist: &mut Vec<String>, name: &str) {
        if !hoist.iter().any(|existing| existing == name) {
            hoist.push(name.to_string());
        }
    }

    /// Recursively lower a binding pattern, reading bound names out of `value`.
    ///
    /// `value_is_reusable` indicates `value` is a plain identifier that can be
    /// re-read for each element without a temp (matching `tsc`'s
    /// `ensureIdentifier(reuseIdentifierExpressions)`).
    fn lower_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        value_is_reusable: bool,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        match pattern_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                self.lower_object_binding_pattern(
                    pattern_idx,
                    value,
                    value_is_reusable,
                    assigns,
                    hoist,
                );
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.lower_array_binding_pattern(
                    pattern_idx,
                    value,
                    value_is_reusable,
                    assigns,
                    hoist,
                );
            }
            _ => {}
        }
    }

    fn lower_object_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        value_is_reusable: bool,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let elements = self.binding_pattern_elements(pattern_idx);
        let element_count = elements.len();
        let has_computed_key = elements.iter().any(|&e| self.element_has_computed_key(e));
        // A single non-computed element reads `value` exactly once, so it is
        // inlined; otherwise the value is captured (a temp when not a reusable
        // identifier) so it is read only once.
        let needs_temp = element_count != 1 || has_computed_key;
        // A computed property name forces the value into a temp even when it is
        // a reusable identifier, matching `tsc` (`_a = obj, _b = k, v = _a[_b]`).
        let reusable = value_is_reusable && !has_computed_key;
        let src = self.ensure_value_source(value, reusable, needs_temp, assigns, hoist);

        let mut excluded_keys: Vec<IRNode> = Vec::new();
        for &elem_idx in &elements {
            let Some(elem) = self
                .arena
                .get(elem_idx)
                .and_then(|n| self.arena.get_binding_element(n))
            else {
                continue;
            };
            if elem.dot_dot_dot_token {
                // Object rest: `rest = __rest(src, ["a", "b"])`.
                let rhs = IRNode::call(
                    IRNode::RuntimeHelper("__rest".into()),
                    vec![src.clone(), IRNode::ArrayLiteral(excluded_keys.clone())],
                );
                self.bind_target(elem.name, rhs, elem.initializer, assigns, hoist);
                continue;
            }
            let key_idx = Self::binding_element_key(elem.property_name, elem.name);
            let (access, exclude_key) =
                self.object_member_access(src.clone(), key_idx, assigns, hoist);
            excluded_keys.push(exclude_key);
            self.bind_target(elem.name, access, elem.initializer, assigns, hoist);
        }
    }

    fn lower_array_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        value: IRNode,
        value_is_reusable: bool,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let elements = self.binding_pattern_elements_with_holes(pattern_idx);
        let non_omitted = elements.iter().filter(|&&e| !e.is_none()).count();
        let has_rest = elements.iter().any(|&e| {
            self.arena
                .get(e)
                .and_then(|n| self.arena.get_binding_element(n))
                .is_some_and(|elem| elem.dot_dot_dot_token)
        });

        let src = if self.downlevel_iteration {
            // Under downlevelIteration the array is read through the iterator
            // protocol: `_a = __read(value, n)` (or `__read(value)` with a rest).
            let read_args = if has_rest {
                vec![value]
            } else {
                vec![value, IRNode::number(non_omitted.to_string())]
            };
            let temp = self.generate_hoisted_temp();
            Self::push_hoist(hoist, &temp);
            assigns.push(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::call(IRNode::RuntimeHelper("__read".into()), read_args),
            ));
            IRNode::id(temp)
        } else {
            let needs_temp = non_omitted != 1;
            self.ensure_value_source(value, value_is_reusable, needs_temp, assigns, hoist)
        };

        for (index, &elem_idx) in elements.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem) = self
                .arena
                .get(elem_idx)
                .and_then(|n| self.arena.get_binding_element(n))
            else {
                continue;
            };
            if elem.dot_dot_dot_token {
                // Array rest: `rest = src.slice(index)`.
                let rhs = IRNode::call(
                    IRNode::prop(src.clone(), "slice"),
                    vec![IRNode::number(index.to_string())],
                );
                self.bind_target(elem.name, rhs, elem.initializer, assigns, hoist);
                continue;
            }
            let access = IRNode::elem(src.clone(), IRNode::number(index.to_string()));
            self.bind_target(elem.name, access, elem.initializer, assigns, hoist);
        }
    }

    /// Bind `name_idx` (identifier or nested pattern) to `access`, applying an
    /// optional `default_idx` initializer.
    fn bind_target(
        &self,
        name_idx: NodeIndex,
        access: IRNode,
        default_idx: NodeIndex,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) {
        let is_pattern = self.is_binding_pattern_name(name_idx);

        if default_idx.is_some() {
            // `_t = access` then `_t === void 0 ? default : _t`.
            let temp = self.generate_hoisted_temp();
            Self::push_hoist(hoist, &temp);
            assigns.push(IRNode::assign(IRNode::id(temp.clone()), access));
            let defaulted = IRNode::ConditionalExpr {
                condition: Box::new(IRNode::binary(
                    IRNode::id(temp.clone()),
                    "===",
                    IRNode::Undefined,
                )),
                when_true: Box::new(self.expression_to_ir(default_idx)),
                when_false: Box::new(IRNode::id(temp)),
            };
            if is_pattern {
                // A second temp holds the defaulted value before destructuring it.
                let temp2 = self.generate_hoisted_temp();
                Self::push_hoist(hoist, &temp2);
                assigns.push(IRNode::assign(IRNode::id(temp2.clone()), defaulted));
                self.lower_binding_pattern(name_idx, IRNode::id(temp2), true, assigns, hoist);
            } else {
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
                if !name.is_empty() {
                    Self::push_hoist(hoist, &name);
                    assigns.push(IRNode::assign(IRNode::id(name), defaulted));
                }
            }
            return;
        }

        if is_pattern {
            let reusable = matches!(access, IRNode::Identifier(_));
            self.lower_binding_pattern(name_idx, access, reusable, assigns, hoist);
        } else {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
            if !name.is_empty() {
                Self::push_hoist(hoist, &name);
                assigns.push(IRNode::assign(IRNode::id(name), access));
            }
        }
    }

    /// Capture `value` into a temp when it is read more than once and is not a
    /// reusable identifier; otherwise return it unchanged.
    fn ensure_value_source(
        &self,
        value: IRNode,
        value_is_reusable: bool,
        needs_temp: bool,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) -> IRNode {
        if !needs_temp || value_is_reusable {
            return value;
        }
        let temp = self.generate_hoisted_temp();
        Self::push_hoist(hoist, &temp);
        assigns.push(IRNode::assign(IRNode::id(temp.clone()), value));
        IRNode::id(temp)
    }

    /// Produce the property access for an object binding element key, plus the
    /// key form used in a sibling rest element's exclusion list.
    ///
    /// Returns `(access, exclude_key)`. For a computed key the key expression is
    /// captured into a temp first so it is evaluated once.
    fn object_member_access(
        &self,
        src: IRNode,
        key_idx: NodeIndex,
        assigns: &mut Vec<IRNode>,
        hoist: &mut Vec<String>,
    ) -> (IRNode, IRNode) {
        let Some(key_node) = self.arena.get(key_idx) else {
            return (src, IRNode::StringLiteral("".into()));
        };
        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(key_node) {
                let temp = self.generate_hoisted_temp();
                Self::push_hoist(hoist, &temp);
                assigns.push(IRNode::assign(
                    IRNode::id(temp.clone()),
                    self.expression_to_ir(computed.expression),
                ));
                let access = IRNode::elem(src, IRNode::id(temp.clone()));
                // Excluded key for a computed property: `typeof _t === "symbol"
                // ? _t : _t + ""` (matching the synchronous `__rest` lowering).
                let exclude = IRNode::ConditionalExpr {
                    condition: Box::new(IRNode::binary(
                        IRNode::Raw(format!("typeof {temp}").into()),
                        "===",
                        IRNode::StringLiteral("symbol".into()),
                    )),
                    when_true: Box::new(IRNode::id(temp.clone())),
                    when_false: Box::new(IRNode::binary(
                        IRNode::id(temp),
                        "+",
                        IRNode::StringLiteral("".into()),
                    )),
                };
                return (access, exclude);
            }
            return (src, IRNode::StringLiteral("".into()));
        }
        if key_node.is_identifier() {
            let text = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, key_idx);
            let access = IRNode::prop(src, text.clone());
            return (access, IRNode::RawStringLiteral(text.into()));
        }
        if key_node.kind == SyntaxKind::StringLiteral as u16 {
            let text = self
                .arena
                .get_literal(key_node)
                .map(|lit| lit.text.clone())
                .unwrap_or_default();
            let access = IRNode::elem(src, IRNode::RawStringLiteral(text.clone().into()));
            return (access, IRNode::RawStringLiteral(text.into()));
        }
        if key_node.kind == SyntaxKind::NumericLiteral as u16 {
            let text = self
                .arena
                .get_literal(key_node)
                .map(|lit| lit.text.clone())
                .unwrap_or_default();
            let access = IRNode::elem(src, IRNode::number(text.clone()));
            // Numeric keys are excluded as quoted strings, mirroring `tsc`.
            return (access, IRNode::RawStringLiteral(text.into()));
        }
        (src, IRNode::StringLiteral("".into()))
    }

    const fn binding_element_key(property_name: NodeIndex, name: NodeIndex) -> NodeIndex {
        if property_name.is_some() {
            property_name
        } else {
            name
        }
    }

    fn binding_pattern_elements(&self, pattern_idx: NodeIndex) -> Vec<NodeIndex> {
        self.arena
            .get(pattern_idx)
            .and_then(|n| self.arena.get_binding_pattern(n))
            .map(|p| {
                p.elements
                    .nodes
                    .iter()
                    .copied()
                    .filter(|idx| !idx.is_none())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Array elements preserving holes (`OMITTED_EXPRESSION`) so element indices
    /// line up with source positions.
    fn binding_pattern_elements_with_holes(&self, pattern_idx: NodeIndex) -> Vec<NodeIndex> {
        self.arena
            .get(pattern_idx)
            .and_then(|n| self.arena.get_binding_pattern(n))
            .map(|p| p.elements.nodes.clone())
            .unwrap_or_default()
    }

    fn element_has_computed_key(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem) = self
            .arena
            .get(elem_idx)
            .and_then(|n| self.arena.get_binding_element(n))
        else {
            return false;
        };
        if elem.dot_dot_dot_token {
            return false;
        }
        let key_idx = Self::binding_element_key(elem.property_name, elem.name);
        self.arena
            .get(key_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }
}
