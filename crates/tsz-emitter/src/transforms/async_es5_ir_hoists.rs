use super::*;

impl<'a> AsyncES5Transformer<'a> {
    pub(super) const fn async_statements_end_control_flow(statements: &[IRNode]) -> bool {
        matches!(
            statements.last(),
            Some(
                IRNode::ReturnStatement(_) | IRNode::ThrowStatement(_) | IRNode::BreakStatement(_)
            )
        )
    }

    /// Extract `VarDecl` names from a `GeneratorBody` IR node and remove them
    /// from the case statements. Returns variable groups to hoist.
    ///
    /// tsc hoists `var` declarations to before the `return __generator(...)` call,
    /// so they appear at the top of the `__awaiter` wrapper function body.
    /// Extract leading directive prologues (e.g. `"use strict"`) from the first
    /// case of a generator body and return them as raw string values (without quotes).
    ///
    /// When a directive appears at the top of an async function body, `tsc` places
    /// it inside the `__awaiter` callback - before any `var` declarations and
    /// before `__generator` - not inside the switch/case statements. This helper
    /// removes those nodes from case 0 and returns their string content so that
    /// the `AwaiterCall` printer can emit them in the correct position.
    ///
    /// Handles `StringLiteral`, `RawStringLiteral`, and `Raw` nodes (the last form
    /// is emitted when the source text is available and the value is a quoted token).
    pub fn extract_and_remove_directive_prologue(generator_body: &mut IRNode) -> Vec<String> {
        let IRNode::GeneratorBody { cases, .. } = generator_body else {
            return Vec::new();
        };
        let Some(first_case) = cases.first_mut() else {
            return Vec::new();
        };
        let mut directives = Vec::new();
        while let Some(IRNode::ExpressionStatement(expr)) = first_case.statements.first() {
            let directive = match expr.as_ref() {
                IRNode::StringLiteral(text) | IRNode::RawStringLiteral(text) => text.to_string(),
                IRNode::Raw(raw) => {
                    // Raw nodes produced from source tokens include surrounding quotes.
                    let trimmed = raw.trim();
                    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
                        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
                    {
                        trimmed[1..trimmed.len() - 1].to_string()
                    } else {
                        break;
                    }
                }
                _ => break,
            };
            directives.push(directive);
            first_case.statements.remove(0);
        }
        directives
    }

    pub fn extract_and_remove_var_decl_groups(generator_body: &mut IRNode) -> Vec<Vec<String>> {
        let IRNode::GeneratorBody { cases, .. } = generator_body else {
            return Vec::new();
        };

        let mut hoisted = Vec::new();
        let mut current_group = Vec::new();
        for case in cases.iter_mut() {
            Self::extract_and_remove_var_decl_groups_from_statements(
                &mut case.statements,
                &mut hoisted,
                &mut current_group,
            );
        }

        if !current_group.is_empty() {
            hoisted.push(current_group);
        }

        hoisted
    }

    fn extract_and_remove_var_decl_groups_from_statements(
        statements: &mut Vec<IRNode>,
        hoisted: &mut Vec<Vec<String>>,
        current_group: &mut Vec<String>,
    ) {
        let mut i = 0;
        while i < statements.len() {
            match &mut statements[i] {
                IRNode::HoistedVarGroupBreak => {
                    if !current_group.is_empty() {
                        hoisted.push(std::mem::take(current_group));
                    }
                    statements.remove(i);
                    continue;
                }
                IRNode::VarDecl { name, initializer } if initializer.is_none() => {
                    current_group.push(name.to_string());
                    statements.remove(i);
                    continue;
                }
                IRNode::VarDecl { name, initializer } => {
                    let var_name = name.clone();
                    current_group.push(var_name.to_string());
                    let init = initializer
                        .clone()
                        .expect("VarDecl match without guard guarantees initializer is Some");
                    statements[i] = IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(var_name)),
                        operator: "=".to_string().into(),
                        right: init,
                    }));
                }
                IRNode::IfStatement {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::extract_and_remove_var_decl_groups_from_node(
                        then_branch,
                        hoisted,
                        current_group,
                    );
                    if let Some(else_branch) = else_branch {
                        Self::extract_and_remove_var_decl_groups_from_node(
                            else_branch,
                            hoisted,
                            current_group,
                        );
                    }
                }
                IRNode::WithStatement { body, .. }
                | IRNode::ForStatement { body, .. }
                | IRNode::ForInOfStatement { body, .. } => {
                    Self::extract_and_remove_var_decl_groups_from_node(
                        body,
                        hoisted,
                        current_group,
                    );
                }
                IRNode::Block(body) | IRNode::Sequence(body) => {
                    Self::extract_and_remove_var_decl_groups_from_statements(
                        body,
                        hoisted,
                        current_group,
                    );
                }
                IRNode::SwitchStatement { cases, .. } => {
                    for case in cases {
                        Self::extract_and_remove_var_decl_groups_from_statements(
                            &mut case.statements,
                            hoisted,
                            current_group,
                        );
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn extract_and_remove_var_decl_groups_from_node(
        node: &mut IRNode,
        hoisted: &mut Vec<Vec<String>>,
        current_group: &mut Vec<String>,
    ) {
        match node {
            IRNode::Block(statements) | IRNode::Sequence(statements) => {
                Self::extract_and_remove_var_decl_groups_from_statements(
                    statements,
                    hoisted,
                    current_group,
                );
            }
            IRNode::IfStatement {
                then_branch,
                else_branch,
                ..
            } => {
                Self::extract_and_remove_var_decl_groups_from_node(
                    then_branch,
                    hoisted,
                    current_group,
                );
                if let Some(else_branch) = else_branch {
                    Self::extract_and_remove_var_decl_groups_from_node(
                        else_branch,
                        hoisted,
                        current_group,
                    );
                }
            }
            IRNode::WithStatement { body, .. }
            | IRNode::ForStatement { body, .. }
            | IRNode::ForInOfStatement { body, .. } => {
                Self::extract_and_remove_var_decl_groups_from_node(body, hoisted, current_group);
            }
            IRNode::SwitchStatement { cases, .. } => {
                for case in cases {
                    Self::extract_and_remove_var_decl_groups_from_statements(
                        &mut case.statements,
                        hoisted,
                        current_group,
                    );
                }
            }
            _ => {}
        }
    }

    pub fn extract_and_remove_var_decls(generator_body: &mut IRNode) -> Vec<String> {
        Self::extract_and_remove_var_decl_groups(generator_body)
            .into_iter()
            .flatten()
            .collect()
    }
}
