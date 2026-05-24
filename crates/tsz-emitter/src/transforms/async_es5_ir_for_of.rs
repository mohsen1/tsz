//! Async ES5 `for..of` state-machine lowering.

use super::AsyncES5Transformer;
use super::state::{ForInAssignmentTarget, ForInSuspendedElementIndex, ForInSuspendedObject};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
enum ForOfAssignmentTarget {
    Simple {
        target: ForInAssignmentTarget,
        declared_name: Option<String>,
    },
    Destructuring {
        pattern: NodeIndex,
        declared_names: Vec<String>,
    },
}

impl<'a> AsyncES5Transformer<'a> {
    fn for_of_iterable_temp_name_for_statement(
        &self,
        statement: NodeIndex,
        expression: NodeIndex,
    ) -> String {
        let Some(expr_node) = self.arena.get(expression) else {
            return self.generate_hoisted_temp();
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return self.generate_hoisted_temp();
        }
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, expression);
        if name.is_empty() {
            return self.generate_hoisted_temp();
        }

        let current_pos = self.arena.get(statement).map_or(u32::MAX, |node| node.pos);
        let ordinal = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.kind == syntax_kind_ext::FOR_OF_STATEMENT && node.pos <= current_pos
            })
            .filter_map(|(idx, node)| {
                let for_of = self.arena.get_for_in_of(node)?;
                let expr_node = self.arena.get(for_of.expression)?;
                if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                    return None;
                }
                let expr_name = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    for_of.expression,
                );
                (expr_name == name).then_some(NodeIndex(idx as u32))
            })
            .count()
            .max(1);

        self.fresh_reserved_name(format!("{name}_{ordinal}"))
    }

    pub(super) fn process_for_of_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_of) = self.arena.get_for_in_of(node) else {
            return false;
        };
        if for_of.await_modifier
            || crate::transforms::emit_utils::for_of_using_info(self.arena, for_of.initializer)
                .is_some()
        {
            return false;
        }

        let Some(assignment_target) = self.for_of_assignment_target(for_of.initializer) else {
            return false;
        };
        let target_object_temp = if matches!(
            assignment_target,
            ForOfAssignmentTarget::Simple {
                target: ForInAssignmentTarget::SuspendedElement {
                    index: ForInSuspendedElementIndex::Suspended(_),
                    ..
                },
                ..
            }
        ) {
            Some(self.generate_hoisted_temp())
        } else {
            None
        };

        let index_name = self.fresh_reserved_name("_i");
        let iterable_name = self.for_of_iterable_temp_name_for_statement(idx, for_of.expression);
        for name in [&index_name, &iterable_name] {
            current_statements.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
        }
        match &assignment_target {
            ForOfAssignmentTarget::Simple {
                declared_name: Some(iteration_name),
                ..
            } => {
                current_statements.push(IRNode::VarDecl {
                    name: iteration_name.clone().into(),
                    initializer: None,
                });
            }
            ForOfAssignmentTarget::Destructuring { declared_names, .. } => {
                for iteration_name in declared_names {
                    current_statements.push(IRNode::VarDecl {
                        name: iteration_name.clone().into(),
                        initializer: None,
                    });
                }
            }
            _ => {}
        }
        if let Some(temp) = &target_object_temp {
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
        }

        let expression_has_suspension = self.contains_await_recursive(for_of.expression);
        let body_has_suspension = self.contains_await_recursive(for_of.statement);
        let mut index_initialized_before_iterable = false;
        let iterable =
            if let Some(suspension) = self.direct_suspension_expression(for_of.expression) {
                current_statements.push(Self::expression_statement(IRNode::assign(
                    IRNode::id(index_name.clone()),
                    IRNode::number("0"),
                )));
                self.process_await_expression(suspension, cases, current_statements, current_label);
                index_initialized_before_iterable = true;
                IRNode::GeneratorSent
            } else if expression_has_suspension {
                return false;
            } else {
                self.for_of_iterable_to_ir_with_es5_computed_temps(
                    for_of.expression,
                    current_statements,
                )
            };

        let assignment_has_suspension = match &assignment_target {
            ForOfAssignmentTarget::Simple { .. } => {
                self.contains_await_recursive(for_of.initializer)
            }
            ForOfAssignmentTarget::Destructuring { pattern, .. } => {
                self.contains_await_recursive(*pattern)
            }
        };

        if !assignment_has_suspension && !expression_has_suspension && !body_has_suspension {
            let iteration_value = IRNode::elem(
                IRNode::id(iterable_name.clone()),
                IRNode::id(index_name.clone()),
            );
            let mut body = match &assignment_target {
                ForOfAssignmentTarget::Simple {
                    target: ForInAssignmentTarget::Direct(target),
                    ..
                } => vec![Self::expression_statement(IRNode::assign(
                    (**target).clone(),
                    iteration_value,
                ))],
                ForOfAssignmentTarget::Destructuring { pattern, .. } => {
                    let mut assignments = Vec::new();
                    let mut group_break_started = false;
                    if !self.push_for_of_destructuring_assignments(
                        *pattern,
                        iteration_value,
                        cases,
                        &mut assignments,
                        current_label,
                        &mut group_break_started,
                    ) {
                        return false;
                    }
                    assignments
                }
                _ => return false,
            };
            if let Some(body_node) = self.arena.get(for_of.statement) {
                if body_node.kind == syntax_kind_ext::BLOCK {
                    if let Some(block) = self.arena.get_block(body_node) {
                        body.extend(
                            block
                                .statements
                                .nodes
                                .iter()
                                .map(|&stmt| self.statement_to_ir(stmt)),
                        );
                    }
                } else {
                    body.push(self.statement_to_ir(for_of.statement));
                }
            }
            current_statements.push(IRNode::ForStatement {
                initializer: Some(Box::new(IRNode::binary(
                    IRNode::assign(IRNode::id(index_name.clone()), IRNode::number("0")),
                    ",",
                    IRNode::assign(IRNode::id(iterable_name.clone()), iterable),
                ))),
                condition: Some(Box::new(IRNode::binary(
                    IRNode::id(index_name.clone()),
                    "<",
                    IRNode::prop(IRNode::id(iterable_name), "length"),
                ))),
                incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                    operand: Box::new(IRNode::id(index_name)),
                    operator: "++".into(),
                })),
                body: Box::new(IRNode::Block(body)),
            });
            return true;
        }

        let loop_label = self.state.next_label();
        let end_placeholder = self.next_loop_exit_placeholder();
        let init_expression = if index_initialized_before_iterable {
            IRNode::assign(IRNode::id(iterable_name.clone()), iterable)
        } else {
            IRNode::binary(
                IRNode::assign(IRNode::id(index_name.clone()), IRNode::number("0")),
                ",",
                IRNode::assign(IRNode::id(iterable_name.clone()), iterable),
            )
        };
        current_statements.push(Self::expression_statement(init_expression));
        current_statements.push(Self::generator_label_assignment(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(Self::negated_condition(IRNode::binary(
                IRNode::id(index_name.clone()),
                "<",
                IRNode::prop(IRNode::id(iterable_name.clone()), "length"),
            ))),
            target_label: end_placeholder,
        });
        let iteration_value =
            IRNode::elem(IRNode::id(iterable_name), IRNode::id(index_name.clone()));
        match assignment_target {
            ForOfAssignmentTarget::Simple {
                target: ForInAssignmentTarget::Direct(target),
                ..
            } => {
                current_statements.push(Self::expression_statement(IRNode::assign(
                    *target,
                    iteration_value,
                )));
            }
            ForOfAssignmentTarget::Simple {
                target:
                    ForInAssignmentTarget::SuspendedProperty {
                        object_suspension,
                        property,
                    },
                ..
            } => {
                self.process_await_expression(
                    object_suspension,
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements.push(Self::expression_statement(IRNode::assign(
                    IRNode::prop(
                        IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                        property,
                    ),
                    iteration_value,
                )));
            }
            ForOfAssignmentTarget::Simple {
                target: ForInAssignmentTarget::SuspendedElement { object, index },
                ..
            } => match index {
                ForInSuspendedElementIndex::Direct(index) => {
                    let ForInSuspendedObject::Suspended(object_suspension) = object else {
                        return false;
                    };
                    self.process_await_expression(
                        object_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(
                            IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            *index,
                        ),
                        iteration_value,
                    )));
                }
                ForInSuspendedElementIndex::Suspended(index_suspension) => {
                    let Some(temp) = target_object_temp else {
                        return false;
                    };
                    match object {
                        ForInSuspendedObject::Direct(object) => {
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                *object,
                            )));
                        }
                        ForInSuspendedObject::Suspended(object_suspension) => {
                            self.process_await_expression(
                                object_suspension,
                                cases,
                                current_statements,
                                current_label,
                            );
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            )));
                        }
                    }
                    self.process_await_expression(
                        index_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(IRNode::id(temp), IRNode::GeneratorSent),
                        iteration_value,
                    )));
                }
            },
            ForOfAssignmentTarget::Destructuring { pattern, .. } => {
                let mut group_break_started = false;
                if !self.push_for_of_destructuring_assignments(
                    pattern,
                    iteration_value,
                    cases,
                    current_statements,
                    current_label,
                    &mut group_break_started,
                ) {
                    return false;
                }
            }
        }

        self.process_block_or_statement_in_async(
            for_of.statement,
            cases,
            current_statements,
            current_label,
        );

        let increment_label = self.state.next_label();
        let end_label = self.state.next_label();
        current_statements.push(Self::generator_label_assignment(increment_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        current_statements.push(Self::expression_statement(IRNode::PostfixUnaryExpr {
            operand: Box::new(IRNode::id(index_name)),
            operator: "++".into(),
        }));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: increment_label,
            statements: std::mem::take(current_statements),
        });

        Self::patch_if_break_target(cases, end_placeholder, end_label);
        *current_label = end_label;
        true
    }

    fn for_of_assignment_target(&self, initializer: NodeIndex) -> Option<ForOfAssignmentTarget> {
        if let Some((target, declared_name)) = self.for_in_assignment_target(initializer) {
            return Some(ForOfAssignmentTarget::Simple {
                target,
                declared_name,
            });
        }

        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let decl_list = self.arena.get_variable(init_node)?;
            if decl_list.declarations.nodes.len() != 1 {
                return None;
            }
            let decl_idx = *decl_list.declarations.nodes.first()?;
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            if decl.initializer.is_some() {
                return None;
            }
            let name_node = self.arena.get(decl.name)?;
            if !Self::is_for_of_destructuring_pattern_kind(name_node.kind) {
                return None;
            }
            let mut declared_names = Vec::new();
            self.collect_binding_name(decl.name, &mut declared_names);
            return Some(ForOfAssignmentTarget::Destructuring {
                pattern: decl.name,
                declared_names,
            });
        }

        if Self::is_for_of_destructuring_pattern_kind(init_node.kind) {
            return Some(ForOfAssignmentTarget::Destructuring {
                pattern: initializer,
                declared_names: Vec::new(),
            });
        }

        None
    }

    const fn is_for_of_destructuring_pattern_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            || kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
    }

    fn push_for_of_destructuring_assignments(
        &mut self,
        pattern: NodeIndex,
        source: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> bool {
        let Some(pattern_node) = self.arena.get(pattern) else {
            return false;
        };
        match pattern_node.kind {
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let Some(binding) = self.arena.get_binding_pattern(pattern_node) else {
                    return false;
                };
                let elements: Vec<NodeIndex> = binding.elements.nodes.clone();
                self.push_for_of_array_binding_assignments(
                    &elements,
                    source,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                let Some(literal) = self.arena.get_literal_expr(pattern_node) else {
                    return false;
                };
                let elements: Vec<NodeIndex> = literal.elements.nodes.clone();
                self.push_for_of_array_literal_assignments(
                    &elements,
                    source,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let Some(binding) = self.arena.get_binding_pattern(pattern_node) else {
                    return false;
                };
                let elements: Vec<NodeIndex> = binding.elements.nodes.clone();
                self.push_for_of_object_binding_assignments(
                    &elements,
                    source,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let Some(literal) = self.arena.get_literal_expr(pattern_node) else {
                    return false;
                };
                let elements: Vec<NodeIndex> = literal.elements.nodes.clone();
                self.push_for_of_object_literal_assignments(
                    &elements,
                    source,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
            }
            _ => false,
        }
    }

    fn push_for_of_array_binding_assignments(
        &mut self,
        elements: &[NodeIndex],
        source: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> bool {
        for (index, &element_idx) in elements.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                return false;
            }
            let Some(element) = self.arena.get_binding_element(element_node).cloned() else {
                return false;
            };
            let value = if element.dot_dot_dot_token {
                IRNode::call(
                    IRNode::prop(source.clone(), "slice"),
                    vec![IRNode::number(index.to_string())],
                )
            } else {
                IRNode::elem(source.clone(), IRNode::number(index.to_string()))
            };
            if self
                .push_for_of_binding_element_assignment(
                    element.name,
                    element.initializer,
                    value,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
                .is_none()
            {
                return false;
            }
        }
        true
    }

    fn push_for_of_array_literal_assignments(
        &mut self,
        elements: &[NodeIndex],
        source: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> bool {
        for (index, &element_idx) in elements.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let Some(spread) = self.arena.get_unary_expr_ex(element_node) else {
                    return false;
                };
                let Some(target) = self.for_of_destructuring_target_ir(spread.expression) else {
                    return false;
                };
                current_statements.push(Self::expression_statement(IRNode::assign(
                    target,
                    IRNode::call(
                        IRNode::prop(source.clone(), "slice"),
                        vec![IRNode::number(index.to_string())],
                    ),
                )));
                continue;
            }

            let value = IRNode::elem(source.clone(), IRNode::number(index.to_string()));
            let Some(handled) = self.push_for_of_assignment_pattern_element(
                element_idx,
                value.clone(),
                cases,
                current_statements,
                current_label,
                group_break_started,
            ) else {
                return false;
            };
            if handled {
                continue;
            }
            let Some(target) = self.for_of_destructuring_target_ir(element_idx) else {
                return false;
            };
            current_statements.push(Self::expression_statement(IRNode::assign(target, value)));
        }
        true
    }

    fn push_for_of_object_binding_assignments(
        &mut self,
        elements: &[NodeIndex],
        source: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> bool {
        for &element_idx in elements {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                return false;
            }
            let Some(element) = self.arena.get_binding_element(element_node).cloned() else {
                return false;
            };
            if element.dot_dot_dot_token {
                return false;
            }
            let property_name = if element.property_name.is_some() {
                element.property_name
            } else {
                element.name
            };
            let Some(value) =
                self.destructuring_object_property_value(source.clone(), property_name)
            else {
                return false;
            };
            if self
                .push_for_of_binding_element_assignment(
                    element.name,
                    element.initializer,
                    value,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
                .is_none()
            {
                return false;
            }
        }
        true
    }

    fn push_for_of_object_literal_assignments(
        &mut self,
        elements: &[NodeIndex],
        source: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> bool {
        for &element_idx in elements {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            match element_node.kind {
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_shorthand_property(element_node).cloned()
                    else {
                        return false;
                    };
                    let name = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, prop.name,
                    );
                    if name.is_empty() {
                        return false;
                    }
                    let value = IRNode::prop(source.clone(), name);
                    if self
                        .push_for_of_defaulted_assignment(
                            IRNode::id(crate::transforms::emit_utils::identifier_text_or_empty(
                                self.arena, prop.name,
                            )),
                            value,
                            prop.object_assignment_initializer,
                            cases,
                            current_statements,
                            current_label,
                            group_break_started,
                        )
                        .is_none()
                    {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(element_node).cloned()
                    else {
                        return false;
                    };
                    let Some(value) =
                        self.destructuring_object_property_value(source.clone(), prop.name)
                    else {
                        return false;
                    };
                    let Some(handled) = self.push_for_of_assignment_pattern_element(
                        prop.initializer,
                        value.clone(),
                        cases,
                        current_statements,
                        current_label,
                        group_break_started,
                    ) else {
                        return false;
                    };
                    if handled {
                        continue;
                    }
                    let Some(target) = self.for_of_destructuring_target_ir(prop.initializer) else {
                        return false;
                    };
                    current_statements
                        .push(Self::expression_statement(IRNode::assign(target, value)));
                }
                _ => return false,
            }
        }
        true
    }

    fn push_for_of_assignment_pattern_element(
        &mut self,
        element_idx: NodeIndex,
        value: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> Option<bool> {
        let element_node = self.arena.get(element_idx)?;
        if Self::is_for_of_destructuring_pattern_kind(element_node.kind) {
            return Some(self.push_for_of_destructuring_assignments(
                element_idx,
                value,
                cases,
                current_statements,
                current_label,
                group_break_started,
            ));
        }
        if element_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return Some(false);
        }
        let binary = self.arena.get_binary_expr(element_node)?;
        if self.get_operator_text(binary.operator_token) != "=" {
            return Some(false);
        }
        let target = self.for_of_destructuring_target_ir(binary.left)?;
        self.push_for_of_defaulted_assignment(
            target,
            value,
            binary.right,
            cases,
            current_statements,
            current_label,
            group_break_started,
        )
        .map(|()| true)
    }

    fn push_for_of_binding_element_assignment(
        &mut self,
        name: NodeIndex,
        initializer: NodeIndex,
        value: IRNode,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> Option<()> {
        let name_node = self.arena.get(name)?;
        if Self::is_for_of_destructuring_pattern_kind(name_node.kind) {
            return self
                .push_for_of_destructuring_assignments(
                    name,
                    value,
                    cases,
                    current_statements,
                    current_label,
                    group_break_started,
                )
                .then_some(());
        }
        let target = self.for_of_destructuring_target_ir(name)?;
        self.push_for_of_defaulted_assignment(
            target,
            value,
            initializer,
            cases,
            current_statements,
            current_label,
            group_break_started,
        )
    }

    fn push_for_of_defaulted_assignment(
        &mut self,
        target: IRNode,
        value: IRNode,
        initializer: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        group_break_started: &mut bool,
    ) -> Option<()> {
        if initializer.is_none() {
            current_statements.push(Self::expression_statement(IRNode::assign(target, value)));
            return Some(());
        }

        if let Some(default_suspension) = self.direct_suspension_expression(initializer) {
            let selected_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: selected_temp.clone().into(),
                initializer: None,
            });
            if !*group_break_started {
                current_statements.push(IRNode::HoistedVarGroupBreak);
                *group_break_started = true;
            }
            let value_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: value_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(value_temp.clone()),
                value,
            )));

            let fallback_placeholder = self.next_loop_exit_placeholder();
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(Self::negated_condition(IRNode::binary(
                    IRNode::id(value_temp.clone()),
                    "===",
                    IRNode::Undefined,
                ))),
                target_label: fallback_placeholder,
            });
            self.process_await_expression(
                default_suspension,
                cases,
                current_statements,
                current_label,
            );

            let after_placeholder = self.next_loop_exit_placeholder();
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(selected_temp.clone()),
                IRNode::GeneratorSent,
            )));
            current_statements.push(Self::generator_break_statement(after_placeholder));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            let fallback_label = self.state.next_label();
            Self::patch_if_break_target(cases, fallback_placeholder, fallback_label);
            *current_label = fallback_label;
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(selected_temp.clone()),
                IRNode::id(value_temp),
            )));
            let after_label = self.state.next_label();
            current_statements.push(Self::generator_label_assignment(after_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            Self::patch_if_break_target(cases, after_placeholder, after_label);
            *current_label = after_label;
            current_statements.push(Self::expression_statement(IRNode::assign(
                target,
                IRNode::id(selected_temp),
            )));
            return Some(());
        }

        if self.contains_await_recursive(initializer) {
            return None;
        }
        if !*group_break_started {
            current_statements.push(IRNode::HoistedVarGroupBreak);
            *group_break_started = true;
        }
        let value_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: value_temp.clone().into(),
            initializer: None,
        });
        let default_value = self.expression_to_ir(initializer);
        let selected_value = IRNode::ConditionalExpr {
            condition: Box::new(IRNode::binary(
                IRNode::id(value_temp.clone()),
                "===",
                IRNode::Undefined,
            )),
            when_true: Box::new(default_value),
            when_false: Box::new(IRNode::id(value_temp.clone())),
        };
        current_statements.push(Self::expression_statement(IRNode::binary(
            IRNode::assign(IRNode::id(value_temp), value),
            ",",
            IRNode::assign(target, selected_value),
        )));
        Some(())
    }

    fn for_of_destructuring_target_ir(&self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return Some(self.expression_to_ir(idx));
        }
        None
    }
}
