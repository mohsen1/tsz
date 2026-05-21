use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::state::SuspendedAssignmentTarget;
use super::{AsyncES5Transformer, opcodes};

impl<'a> AsyncES5Transformer<'a> {
    pub(super) fn lower_assignment_target_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        if !self.contains_await_recursive(bin.right) || self.contains_await_recursive(bin.left) {
            return false;
        }
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };

        let Some((target, object)) = self.suspended_assignment_target(left_node) else {
            return false;
        };
        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            object,
        ))));

        // For element access targets (obj[idx] = await rhs), tsc also saves the index
        // expression to a temp before yielding, so that left-to-right evaluation order
        // is preserved across the suspension boundary.
        let lowered_target = match target {
            SuspendedAssignmentTarget::Property(property) => {
                // obj.prop = await rhs → _a = obj; yield rhs; _a.prop = sent
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                IRNode::prop(IRNode::id(temp), property)
            }
            SuspendedAssignmentTarget::Element(index) => {
                // obj[idx] = await rhs → _a = obj; _b = idx; yield rhs; _a[_b] = sent
                let index_temp = self.generate_hoisted_temp();
                current_statements.push(IRNode::VarDecl {
                    name: index_temp.clone().into(),
                    initializer: None,
                });
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(index_temp.clone()),
                    *index,
                ))));
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                IRNode::elem(IRNode::id(temp), IRNode::id(index_temp))
            }
        };
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            lowered_target,
            self.expression_to_ir(bin.right),
        ))));
        true
    }

    /// Lower an assignment (or compound assignment) `obj[await idx] OP rhs` where the
    /// await is in the LHS element-access index.
    ///
    /// Structural rule: When the index of an element-access assignment target contains an
    /// await and the base does not, tsc saves the base to a temp before yielding the index.
    /// For plain assignment (`=`), the RHS must be await-free too.
    /// For compound assignment (`+=`, etc. with no await in RHS), the operator is kept.
    ///
    /// - `x[await z] = y`   → `_a = x; yield z; _a[_b.sent()] = y`
    /// - `x[await z] += y`  → `_a = x; yield z; _a[_b.sent()] += y`
    pub(super) fn lower_lhs_element_access_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        let op_text = self.get_operator_text(bin.operator_token);
        // Must be a plain assignment or compound assignment (not comparison, etc.)
        let is_plain_assign = op_text == "=";
        let is_compound_assign = op_text.ends_with('=')
            && op_text.len() >= 2
            && op_text != "=="
            && op_text != "!="
            && op_text != "<="
            && op_text != ">="
            && op_text != "==="
            && op_text != "!=="
            && !is_plain_assign;
        if !is_plain_assign && !is_compound_assign {
            return false;
        }
        // RHS must NOT have await (if it did, lower_compound_assignment_before_suspension
        // would handle compound-assignment cases with await in both sides)
        if self.contains_await_recursive(bin.right) {
            return false;
        }
        // LHS must be an element access with await in the index, object has no await
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(left_node) else {
            return false;
        };
        if !self.contains_await_recursive(access.name_or_argument)
            || self.contains_await_recursive(access.expression)
        {
            return false;
        }

        // Save the object before the yield
        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(access.expression),
        ))));

        // Yield the index expression (which contains the await)
        self.emit_nested_suspension(bin.left, cases, current_statements, current_label);

        // After yield: temp[_a.sent()] OP rhs
        let rhs = self.expression_to_ir(bin.right);
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
            left: Box::new(IRNode::elem(IRNode::id(temp), IRNode::GeneratorSent)),
            operator: op_text.into(),
            right: Box::new(rhs),
        })));
        true
    }

    /// Lower an assignment `(obj[await idx]).prop = rhs` where the LHS is a property
    /// access whose base object is an element access with await in the index.
    ///
    /// Structural rule: When the LHS of an assignment is a property access and its object
    /// is an element access with await in the index (but the element-access base has no
    /// await), tsc saves the element-access base to a temp, yields the index, then assigns
    /// `temp[_a.sent()].prop = rhs`.
    ///
    /// Pattern: `x[await z].b = y` → `_a = x; yield z; _a[_b.sent()].b = y`
    pub(super) fn lower_lhs_chained_element_access_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        // RHS must NOT have await
        if self.contains_await_recursive(bin.right) {
            return false;
        }
        // LHS must be a property access
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(prop_access) = self.arena.get_access_expr(left_node) else {
            return false;
        };
        // The object of the property access must be an element access with await in index
        let Some(obj_node) = self.arena.get(prop_access.expression) else {
            return false;
        };
        if obj_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(elem_access) = self.arena.get_access_expr(obj_node) else {
            return false;
        };
        if !self.contains_await_recursive(elem_access.name_or_argument)
            || self.contains_await_recursive(elem_access.expression)
        {
            return false;
        }

        // Save the element-access base before the yield
        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(elem_access.expression),
        ))));

        // Yield the index (which contains the await); use the elem-access node as the
        // suspension source so the correct yield value is captured
        self.emit_nested_suspension(
            prop_access.expression,
            cases,
            current_statements,
            current_label,
        );

        // After yield: temp[_a.sent()].prop = rhs
        let property = crate::transforms::emit_utils::identifier_text_or_empty(
            self.arena,
            prop_access.name_or_argument,
        );
        let rhs = self.expression_to_ir(bin.right);
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(
                IRNode::elem(IRNode::id(temp), IRNode::GeneratorSent),
                property,
            ),
            rhs,
        ))));
        true
    }

    /// Lower a compound assignment where BOTH sides have await (double-suspension).
    ///
    /// Structural rule: When both the LHS (via an await expression at its root) and the RHS
    /// of a compound assignment contain await, tsc generates a two-yield state machine:
    /// first yielding the LHS await, saving the object and current value, then yielding the
    /// RHS await, and writing back `saved_obj.prop = saved_val OP received`.
    ///
    /// Patterns handled:
    /// - `(await x).prop OP= await y` → yield x; `_a = sent`; `_b = _a.prop`; yield y; `_a.prop = _b OP sent`
    /// - `(await x)[idx] OP= await y` → yield x; `_a = sent`; `_b = idx`; `_c = _a[_b]`; yield y; `_a[_b] = _c OP sent`
    /// - `x[await idx] OP= await y` → `_a = x`; yield idx; `_b = sent`; `_c = _a[_b]`; yield y; `_a[_b] = _c OP sent`
    pub(super) fn lower_compound_assignment_double_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        let op_text = self.get_operator_text(bin.operator_token);
        // Must be a compound assignment
        let is_compound_assign = op_text.ends_with('=')
            && op_text.len() >= 2
            && op_text != "=="
            && op_text != "!="
            && op_text != "<="
            && op_text != ">="
            && op_text != "==="
            && op_text != "!=="
            && op_text != "=";
        if !is_compound_assign {
            return false;
        }
        // RHS must have await
        if !self.contains_await_recursive(bin.right) {
            return false;
        }
        // LHS must have await
        if !self.contains_await_recursive(bin.left) {
            return false;
        }

        let binary_op: String = op_text[..op_text.len() - 1].to_string();

        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };

        // Case A: `(await x).prop OP= await y`
        // LHS is property access whose object is (possibly parenthesized) await
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(prop_access) = self.arena.get_access_expr(left_node) else {
                return false;
            };
            // The object of the property access must have await
            if !self.contains_await_recursive(prop_access.expression) {
                return false;
            }
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                prop_access.name_or_argument,
            );
            let obj_temp = self.generate_hoisted_temp();
            let val_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: obj_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: val_temp.clone().into(),
                initializer: None,
            });
            // First yield: the LHS object expression
            self.emit_nested_suspension(
                prop_access.expression,
                cases,
                current_statements,
                current_label,
            );
            // _a = (_c.sent())
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(obj_temp.clone()),
                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
            ))));
            // _b = _a.prop
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(val_temp.clone()),
                IRNode::prop(IRNode::id(obj_temp.clone()), property.clone()),
            ))));
            // Second yield: the RHS expression
            self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
            // _a.prop = _b OP sent
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(obj_temp), property),
                IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(val_temp)),
                    operator: binary_op.into(),
                    right: Box::new(IRNode::GeneratorSent),
                },
            ))));
            return true;
        }

        // Case B: `(await x)[idx] OP= await y`
        // LHS is element access whose object has await, index has no await
        if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let Some(elem_access) = self.arena.get_access_expr(left_node) else {
                return false;
            };

            if self.contains_await_recursive(elem_access.expression)
                && !self.contains_await_recursive(elem_access.name_or_argument)
            {
                // `(await x)[idx] OP= await y`
                let obj_temp = self.generate_hoisted_temp();
                let idx_temp = self.generate_hoisted_temp();
                let val_temp = self.generate_hoisted_temp();
                current_statements.push(IRNode::VarDecl {
                    name: obj_temp.clone().into(),
                    initializer: None,
                });
                current_statements.push(IRNode::VarDecl {
                    name: idx_temp.clone().into(),
                    initializer: None,
                });
                current_statements.push(IRNode::VarDecl {
                    name: val_temp.clone().into(),
                    initializer: None,
                });
                // First yield: the object expression
                self.emit_nested_suspension(
                    elem_access.expression,
                    cases,
                    current_statements,
                    current_label,
                );
                // _a = (_d.sent())
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(obj_temp.clone()),
                    IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                ))));
                // _b = idx
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(idx_temp.clone()),
                    self.expression_to_ir(elem_access.name_or_argument),
                ))));
                // _c = _a[_b]
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(val_temp.clone()),
                    IRNode::elem(IRNode::id(obj_temp.clone()), IRNode::id(idx_temp.clone())),
                ))));
                // Second yield: the RHS
                self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
                // _a[_b] = _c OP sent
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::elem(IRNode::id(obj_temp), IRNode::id(idx_temp)),
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::id(val_temp)),
                        operator: binary_op.into(),
                        right: Box::new(IRNode::GeneratorSent),
                    },
                ))));
                return true;
            }

            if !self.contains_await_recursive(elem_access.expression)
                && self.contains_await_recursive(elem_access.name_or_argument)
            {
                // `x[await idx] OP= await y`
                let base_temp = self.generate_hoisted_temp();
                let idx_temp = self.generate_hoisted_temp();
                let val_temp = self.generate_hoisted_temp();
                current_statements.push(IRNode::VarDecl {
                    name: base_temp.clone().into(),
                    initializer: None,
                });
                current_statements.push(IRNode::VarDecl {
                    name: idx_temp.clone().into(),
                    initializer: None,
                });
                current_statements.push(IRNode::VarDecl {
                    name: val_temp.clone().into(),
                    initializer: None,
                });
                // _a = x
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(base_temp.clone()),
                    self.expression_to_ir(elem_access.expression),
                ))));
                // First yield: the index expression
                self.emit_nested_suspension(
                    elem_access.name_or_argument,
                    cases,
                    current_statements,
                    current_label,
                );
                // _b = sent
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(idx_temp.clone()),
                    IRNode::GeneratorSent,
                ))));
                // _c = _a[_b]
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(val_temp.clone()),
                    IRNode::elem(IRNode::id(base_temp.clone()), IRNode::id(idx_temp.clone())),
                ))));
                // Second yield: the RHS
                self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
                // _a[_b] = _c OP sent
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::elem(IRNode::id(base_temp), IRNode::id(idx_temp)),
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::id(val_temp)),
                        operator: binary_op.into(),
                        right: Box::new(IRNode::GeneratorSent),
                    },
                ))));
                return true;
            }
        }

        false
    }

    /// Lower a compound assignment `lhs op= await rhs` to preserve left-to-right evaluation.
    ///
    /// Structural rule: When a compound-assignment operator (+=, -=, *=, etc.) has an
    /// await in the RHS, tsc saves the current value of the LHS target to a temp before
    /// yielding, then writes back `saved op sent` after the yield.
    ///
    /// - `x += await y` → `_a = x; yield y; x = _a + sent`
    /// - `x.a += await y` → `_a = x; _b = _a.a; yield y; _a.a = _b + sent`
    /// - `x[k] += await y` → `_a = x; _b = k; _c = _a[_b]; yield y; _a[_b] = _c + sent`
    pub(super) fn lower_compound_assignment_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        let op_text = self.get_operator_text(bin.operator_token);
        // Must be a compound assignment: +=, -=, *=, /=, %=, **=, &=, |=, ^=, <<=, >>=, >>>=, ||=, &&=, ??=
        let is_compound_assign = op_text.ends_with('=')
            && op_text.len() >= 2
            && op_text != "=="
            && op_text != "!="
            && op_text != "<="
            && op_text != ">="
            && op_text != "==="
            && op_text != "!=="
            && op_text != "="; // plain = is not compound
        if !is_compound_assign {
            return false;
        }
        // RHS must have await
        if !self.contains_await_recursive(bin.right) {
            return false;
        }
        // LHS must NOT have await
        if self.contains_await_recursive(bin.left) {
            return false;
        }

        // Derive the underlying binary operator from the compound assignment operator
        // e.g., "+=" → "+", "-=" → "-", "**=" → "**", "&&=" → "&&", "||=" → "||", "??=" → "??"
        let binary_op: String = op_text[..op_text.len() - 1].to_string();

        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };

        // Case 1: Simple identifier `x op= await rhs`
        if left_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let val_temp = self.generate_hoisted_temp();
            // _a = x;
            current_statements.push(IRNode::VarDecl {
                name: val_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(val_temp.clone()),
                self.expression_to_ir(bin.left),
            ))));
            self.emit_nested_suspension(idx, cases, current_statements, current_label);
            // x = _a op sent
            let lhs_ir = self.expression_to_ir(bin.left);
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                lhs_ir,
                IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(val_temp)),
                    operator: binary_op.into(),
                    right: Box::new(IRNode::GeneratorSent),
                },
            ))));
            return true;
        }

        // Case 2: Property access `x.a op= await rhs`
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.arena.get_access_expr(left_node) else {
                return false;
            };
            let obj_temp = self.generate_hoisted_temp();
            let val_temp = self.generate_hoisted_temp();
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            // _a = x; _b = _a.a;
            current_statements.push(IRNode::VarDecl {
                name: obj_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: val_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(obj_temp.clone()),
                self.expression_to_ir(access.expression),
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(val_temp.clone()),
                IRNode::prop(IRNode::id(obj_temp.clone()), property.clone()),
            ))));
            self.emit_nested_suspension(idx, cases, current_statements, current_label);
            // _a.a = _b op sent
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(obj_temp), property),
                IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(val_temp)),
                    operator: binary_op.into(),
                    right: Box::new(IRNode::GeneratorSent),
                },
            ))));
            return true;
        }

        // Case 3: Element access `x[k] op= await rhs`
        if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let Some(access) = self.arena.get_access_expr(left_node) else {
                return false;
            };
            let obj_temp = self.generate_hoisted_temp();
            let idx_temp = self.generate_hoisted_temp();
            let val_temp = self.generate_hoisted_temp();
            // _a = x; _b = k; _c = _a[_b];
            current_statements.push(IRNode::VarDecl {
                name: obj_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: idx_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: val_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(obj_temp.clone()),
                self.expression_to_ir(access.expression),
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(idx_temp.clone()),
                self.expression_to_ir(access.name_or_argument),
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(val_temp.clone()),
                IRNode::elem(IRNode::id(obj_temp.clone()), IRNode::id(idx_temp.clone())),
            ))));
            self.emit_nested_suspension(idx, cases, current_statements, current_label);
            // _a[_b] = _c op sent
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::elem(IRNode::id(obj_temp), IRNode::id(idx_temp)),
                IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(val_temp)),
                    operator: binary_op.into(),
                    right: Box::new(IRNode::GeneratorSent),
                },
            ))));
            return true;
        }

        false
    }

    fn suspended_assignment_target(
        &self,
        left_node: &tsz_parser::parser::node::Node,
    ) -> Option<(SuspendedAssignmentTarget, IRNode)> {
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(left_node)?;
            let object = self.expression_to_ir(access.expression);
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            return Some((SuspendedAssignmentTarget::Property(property), object));
        }

        if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(left_node)?;
            let object = self.expression_to_ir(access.expression);
            let index = self.expression_to_ir(access.name_or_argument);
            return Some((SuspendedAssignmentTarget::Element(Box::new(index)), object));
        }

        None
    }

    pub(super) fn lower_call_callee_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(node)?;
        if self.contains_await_recursive(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        let suspension_arg_index = args
            .nodes
            .iter()
            .position(|&arg| self.contains_await_recursive(arg))?;

        let (callee_temp, this_arg) =
            self.capture_call_callee_before_suspension(call.expression, current_statements)?;
        let arg_array = self.lower_suspended_call_arguments(
            &args.nodes,
            suspension_arg_index,
            current_statements,
        );

        self.emit_nested_suspension(idx, cases, current_statements, current_label);

        Some(IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
            arguments: vec![this_arg, arg_array],
        })
    }

    fn capture_call_callee_before_suspension(
        &self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> Option<(String, IRNode)> {
        let callee_node = self.arena.get(callee)?;

        if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let callee_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: callee_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(callee_temp.clone()),
                self.expression_to_ir(callee),
            ))));
            return Some((callee_temp, IRNode::Undefined));
        }

        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(callee_node)?;
        let this_temp = self.generate_hoisted_temp();
        let callee_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: this_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: callee_temp.clone().into(),
            initializer: None,
        });
        let property = crate::transforms::emit_utils::identifier_text_or_empty(
            self.arena,
            access.name_or_argument,
        );
        let captured_receiver = IRNode::Parenthesized(Box::new(IRNode::assign(
            IRNode::id(this_temp.clone()),
            self.expression_to_ir(access.expression),
        )));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(callee_temp.clone()),
            IRNode::prop(captured_receiver, property),
        ))));

        Some((callee_temp, IRNode::id(this_temp)))
    }

    fn lower_suspended_call_arguments(
        &self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        if suspension_arg_index == 0 {
            let lowered_args = args.iter().map(|&arg| self.expression_to_ir(arg)).collect();
            return IRNode::ArrayLiteral(lowered_args);
        }

        let prefix_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: prefix_temp.clone().into(),
            initializer: None,
        });
        let prefix_args = args[..suspension_arg_index]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(prefix_temp.clone()),
            IRNode::ArrayLiteral(prefix_args),
        ))));

        let suffix_args = args[suspension_arg_index..]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
            arguments: vec![IRNode::ArrayLiteral(suffix_args)],
        }
    }

    /// Lower exponentiation with one suspended operand in an async ES5 body.
    ///
    /// Structural rule: after exponentiation is downleveled, `Math.pow` is the
    /// call target. If one argument suspends, tsc captures the callee and any
    /// already-evaluated arguments before yielding, then resumes through
    /// `callee.apply(thisArg, args)`.
    pub(super) fn lower_exponentiation_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != tsz_scanner::SyntaxKind::AsteriskAsteriskToken as u16 {
            return None;
        }

        let left_suspends = self.contains_await_recursive(bin.left);
        let right_suspends = self.contains_await_recursive(bin.right);
        if left_suspends == right_suspends {
            return None;
        }

        let (callee_temp, this_temp) =
            self.capture_math_pow_callee_before_suspension(current_statements);

        if left_suspends {
            self.emit_nested_suspension(bin.left, cases, current_statements, current_label);
            return Some(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
                arguments: vec![
                    IRNode::id(this_temp),
                    IRNode::ArrayLiteral(vec![
                        IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                        self.expression_to_ir(bin.right),
                    ]),
                ],
            });
        }

        let args_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: args_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(args_temp.clone()),
            IRNode::ArrayLiteral(vec![self.expression_to_ir(bin.left)]),
        ))));
        self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
        Some(IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
            arguments: vec![
                IRNode::id(this_temp),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(args_temp), "concat")),
                    arguments: vec![IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])],
                },
            ],
        })
    }

    fn capture_math_pow_callee_before_suspension(
        &self,
        current_statements: &mut Vec<IRNode>,
    ) -> (String, String) {
        let this_temp = self.generate_hoisted_temp();
        let callee_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: this_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: callee_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(callee_temp.clone()),
            IRNode::prop(
                IRNode::Parenthesized(Box::new(IRNode::assign(
                    IRNode::id(this_temp.clone()),
                    IRNode::id("Math"),
                ))),
                "pow",
            ),
        ))));
        (callee_temp, this_temp)
    }

    pub(super) fn lower_return_comma_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != tsz_scanner::SyntaxKind::CommaToken as u16 {
            return None;
        }

        let left_suspends = self.contains_await_recursive(bin.left);
        let right_suspends = self.contains_await_recursive(bin.right);
        if left_suspends == right_suspends {
            return None;
        }

        if left_suspends {
            self.emit_nested_suspension(bin.left, cases, current_statements, current_label);
            return Some(IRNode::Parenthesized(Box::new(IRNode::BinaryExpr {
                left: Box::new(self.expression_to_ir(bin.left)),
                operator: ",".into(),
                right: Box::new(self.expression_to_ir(bin.right)),
            })));
        }

        current_statements.push(IRNode::ExpressionStatement(Box::new(
            self.expression_to_ir(bin.left),
        )));
        self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
        Some(self.expression_to_ir(bin.right))
    }

    /// Lower a binary expression `L OP await R` where OP is not a short-circuit
    /// operator and only the right operand contains a suspension.
    ///
    /// Structural rule: When a non-assignment, non-short-circuit binary expression
    /// has its left operand free of await and its right operand containing an await,
    /// tsc saves the left operand to a temp before yielding to preserve left-to-right
    /// evaluation order.
    ///
    /// Returns `Some(lowered_ir)` if this pattern applies, `None` otherwise.
    pub(super) fn lower_binary_non_short_circuit_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        let op = bin.operator_token;

        // Short-circuit operators need special branching logic, handled separately
        if op == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
            || op == tsz_scanner::SyntaxKind::BarBarToken as u16
            || op == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16
        {
            return None;
        }
        // Assignment operators (=, +=, -=, etc.) are handled elsewhere; skip them.
        // Any operator whose text contains '=' (but is not '!=' or '<=', '>=', '===', '!==')
        // is an assignment. Use SyntaxKind ranges: assignment ops are 63..=78 in the enum.
        {
            let op_text = self.get_operator_text(op);
            let is_assignment = op_text.ends_with('=')
                && op_text != "!="
                && op_text != "<="
                && op_text != ">="
                && op_text != "==="
                && op_text != "!==";
            if is_assignment {
                return None;
            }
        }

        // Only handle: right has await, left does NOT have await
        if !self.contains_await_recursive(bin.right) || self.contains_await_recursive(bin.left) {
            return None;
        }

        let temp = self.generate_hoisted_temp();
        // var temp;
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        // temp = L;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(bin.left),
        ))));
        // yield R (and split cases)
        self.emit_nested_suspension(idx, cases, current_statements, current_label);
        // Reconstruct: temp OP _a.sent()
        let op_text = self.get_operator_text(op);
        // tsc wraps `_a.sent()` in parens when it is the right operand of a
        // non-assignment binary expression used as a statement, e.g.
        // `_a + (_b.sent())`. Use Parenthesized to reproduce this.
        Some(IRNode::BinaryExpr {
            left: Box::new(IRNode::id(temp)),
            operator: op_text.into(),
            right: Box::new(IRNode::Parenthesized(Box::new(IRNode::GeneratorSent))),
        })
    }

    /// Lower an element access `base[await index]` inside an assignment `target = base[await index]`.
    ///
    /// Structural rule: When the index of an element access contains an await and the
    /// base does not, tsc saves the base to a temp before yielding the index to preserve
    /// left-to-right evaluation order.
    ///
    /// Returns `Some(lowered_ir)` if this pattern applies, `None` otherwise.
    pub(super) fn lower_element_access_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        // Must be an assignment: target = base[await index]
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if self.get_operator_text(bin.operator_token) != "=" {
            return None;
        }
        // Right side must be an element access expression with await in index
        let rhs_node = self.arena.get(bin.right)?;
        if rhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(rhs_node)?;
        if !self.contains_await_recursive(access.name_or_argument)
            || self.contains_await_recursive(access.expression)
        {
            return None;
        }
        // Left side must NOT have await
        if self.contains_await_recursive(bin.left) {
            return None;
        }

        let temp = self.generate_hoisted_temp();
        // var temp;
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        // temp = base;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(access.expression),
        ))));
        // yield the index expression (which contains the await)
        self.emit_nested_suspension(bin.right, cases, current_statements, current_label);
        // Reconstruct: target = temp[_a.sent()]
        let target = self.expression_to_ir(bin.left);
        Some(IRNode::assign(
            target,
            IRNode::ElementAccess {
                object: Box::new(IRNode::id(temp)),
                index: Box::new(IRNode::GeneratorSent),
            },
        ))
    }

    /// Lower a logical short-circuit binary expression `L && await R`, `L || await R`,
    /// or `L ?? await R` where only the right operand contains a suspension.
    ///
    /// Structural rule: When a short-circuit binary expression has an await in only the
    /// right operand, tsc saves L to a temp, conditionally jumps past the yield, yields R
    /// if the condition is met, and stores the result in the temp.
    ///
    /// - `&&`: if `!_a` (L is falsy), skip yield; result is L
    /// - `||`: if `_a` (L is truthy), skip yield; result is L
    /// - `??`: if `_a !== null && _a !== void 0`, skip yield; result is L
    ///
    /// Returns `Some(lowered_ir)` if this pattern applies, `None` otherwise.
    pub(super) fn lower_logical_short_circuit_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        let op = bin.operator_token;

        let is_and = op == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16;
        let is_or = op == tsz_scanner::SyntaxKind::BarBarToken as u16;
        let is_nullish = op == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16;

        if !is_and && !is_or && !is_nullish {
            return None;
        }

        // Only handle: right has await, left does NOT
        if !self.contains_await_recursive(bin.right) || self.contains_await_recursive(bin.left) {
            return None;
        }

        let temp = self.generate_hoisted_temp();
        // var temp;
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        // temp = L;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(bin.left),
        ))));

        // Use a placeholder so the IfBreak target is patched after the yield allocates its
        // resume label.  Allocating end_label first would give it a lower number than the
        // resume case, producing out-of-order switch cases.
        let end_placeholder = self.next_loop_exit_placeholder();

        // Emit the short-circuit condition check: if condition, skip yield
        let skip_condition = if is_and {
            // && : if (!_a) skip
            IRNode::PrefixUnaryExpr {
                operator: "!".to_string().into(),
                operand: Box::new(IRNode::id(temp.clone())),
            }
        } else if is_or {
            // || : if (_a) skip
            IRNode::id(temp.clone())
        } else {
            // ?? : if (_a !== null && _a !== void 0) skip
            IRNode::BinaryExpr {
                left: Box::new(IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(temp.clone())),
                    operator: "!==".to_string().into(),
                    right: Box::new(IRNode::NullLiteral),
                }),
                operator: "&&".to_string().into(),
                right: Box::new(IRNode::BinaryExpr {
                    left: Box::new(IRNode::id(temp.clone())),
                    operator: "!==".to_string().into(),
                    right: Box::new(IRNode::Undefined),
                }),
            }
        };

        current_statements.push(IRNode::IfBreak {
            condition: Box::new(skip_condition),
            target_label: end_placeholder,
        });

        // Yield the right operand; this flushes current_statements into the cases vec
        // and advances *current_label to the resume-after-yield label.
        self.emit_nested_suspension(idx, cases, current_statements, current_label);

        // Allocate end_label AFTER the yield so it is numerically greater than the resume
        // label — labels must appear in increasing order in the switch.
        let end_label = self.state.next_label();
        // Patch the placeholder in the case that was just flushed.
        Self::patch_if_break_target(cases, end_placeholder, end_label);

        // After yield: temp = _a.sent();  _b.label = end_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(end_label.to_string()),
        ))));
        // Push this as a new case and start the end case
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = end_label;

        // The value of the whole expression is the temp
        Some(IRNode::id(temp))
    }

    /// Lower a conditional expression `cond ? await T : F` or `cond ? T : await F`
    /// inside a larger expression statement (e.g. an assignment).
    ///
    /// Structural rule: When a conditional expression has await in `when_true` or `when_false`,
    /// tsc saves the result to a temp, branches to avoid the non-taken yield, and continues
    /// after both branches with the temp value.
    ///
    /// Returns `Some(lowered_ir)` if this pattern applies, `None` otherwise.
    fn lower_conditional_expression_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        // Only handle conditional expressions directly
        if node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return None;
        }
        // Also handle when wrapped in an assignment binary expression
        // (the caller may peel the assignment off and call us on the conditional)
        let cond = self.arena.get_conditional_expr(node)?;

        let true_has_await = self.contains_await_recursive(cond.when_true);
        let false_has_await = self.contains_await_recursive(cond.when_false);

        if !true_has_await && !false_has_await {
            return None;
        }

        let temp = self.generate_hoisted_temp();
        // var temp;
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });

        if true_has_await && !false_has_await {
            // cond ? await T : F
            // if (!cond) goto false_label
            // yield T; case N: temp = sent(); goto end
            // false_label: temp = F
            // end_label: <temp is the result>
            let false_label = self.state.next_label();
            let end_label = self.state.next_label();

            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".to_string().into(),
                    operand: Box::new(self.expression_to_ir(cond.condition)),
                }),
                target_label: false_label,
            });
            // yield when_true
            self.emit_nested_suspension(cond.when_true, cases, current_statements, current_label);
            // after yield: temp = sent()
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::GeneratorSent,
            ))));
            // goto end
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::number(end_label.to_string()))),
                    comment: Some("break".to_string().into()),
                },
            ))));
            // false_label: temp = F
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = false_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                self.expression_to_ir(cond.when_false),
            ))));
            // end_label:
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = end_label;
        } else if !true_has_await && false_has_await {
            // cond ? T : await F
            // if (!cond) goto false_label
            // temp = T; goto end
            // false_label: yield F; case N: temp = sent()
            // end_label: <temp is the result>
            let false_label = self.state.next_label();
            let end_label = self.state.next_label();

            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".to_string().into(),
                    operand: Box::new(self.expression_to_ir(cond.condition)),
                }),
                target_label: false_label,
            });
            // true branch: temp = T
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                self.expression_to_ir(cond.when_true),
            ))));
            // goto end
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::number(end_label.to_string()))),
                    comment: Some("break".to_string().into()),
                },
            ))));
            // false_label:
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = false_label;
            // yield when_false
            self.emit_nested_suspension(cond.when_false, cases, current_statements, current_label);
            // temp = sent()
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::GeneratorSent,
            ))));
            // end_label:
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = end_label;
        } else {
            // Both branches have await — use the generic path
            return None;
        }

        Some(IRNode::id(temp))
    }

    /// Lower an assignment expression `target = conditional_with_await` where the right
    /// side is a conditional expression containing await.
    ///
    /// Returns true if handled, false otherwise.
    pub(super) fn lower_assignment_with_conditional_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        if self.contains_await_recursive(bin.left) {
            return false;
        }
        // Right side must be a conditional expression containing await
        let Some(rhs_node) = self.arena.get(bin.right) else {
            return false;
        };
        if rhs_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return false;
        }
        if !self.contains_await_recursive(bin.right) {
            return false;
        }

        if let Some(cond_ir) = self.lower_conditional_expression_before_suspension(
            bin.right,
            cases,
            current_statements,
            current_label,
        ) {
            // target = temp
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                self.expression_to_ir(bin.left),
                cond_ir,
            ))));
            return true;
        }
        false
    }

    pub(super) fn lower_class_extends_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some((class_name, extends_expr, suspension_idx)) = self.class_extends_suspension(idx)
        else {
            return false;
        };
        let Some(factory_parts) = self.es5_class_factory(idx, &class_name) else {
            return false;
        };

        let factory_temp = self.generate_hoisted_temp();

        // Emit weakmap declarations alongside the other class-related vars.
        // These would otherwise be silently dropped by destructuring just the
        // factory body out of the ES5ClassIIFE IR node.
        for weakmap_name in &factory_parts.weakmap_decls {
            current_statements.push(IRNode::VarDecl {
                name: weakmap_name.clone().into(),
                initializer: None,
            });
        }

        current_statements.push(IRNode::VarDecl {
            name: class_name.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: factory_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(factory_temp.clone()),
            factory_parts.factory,
        ))));

        self.process_await_expression(suspension_idx, cases, current_statements, current_label);

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(class_name),
            IRNode::ES5ClassApply {
                factory: Box::new(IRNode::id(factory_temp)),
                base_class: Box::new(self.extends_value_after_suspension(extends_expr)),
            },
        ))));

        // Emit weakmap initializers and deferred static blocks after the class
        // is assigned, mirroring the ordering used by IRPrinter for
        // ES5ClassIIFE (see `ir_printer.rs` ES5ClassIIFE arm: weakmap_inits
        // appended after the IIFE, then deferred_static_blocks).
        for weakmap_init in factory_parts.weakmap_inits {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
                weakmap_init.into(),
            ))));
        }
        for deferred in factory_parts.deferred_static_blocks {
            current_statements.push(deferred);
        }

        true
    }
}
