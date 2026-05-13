use super::DeclarationEmitter;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::{CallExprData, ClassData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn generic_new_expression_inferred_type_text(
        &self,
        base_text: &str,
        new_expr: &CallExprData,
    ) -> Option<String> {
        let args = new_expr.arguments.as_ref()?;
        if args.nodes.is_empty() {
            return None;
        }

        let ident = self.get_identifier_text(new_expr.expression)?;
        let sym_id = self.resolve_identifier_symbol(new_expr.expression, &ident)?;
        let symbol = self.binder.and_then(|binder| binder.symbols.get(sym_id))?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }

        for &decl_idx in &symbol.declarations {
            let Some(class_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_data) = self.arena.get_class(class_node) else {
                continue;
            };
            let Some(type_parameters) = class_data.type_parameters.as_ref() else {
                continue;
            };
            if type_parameters.nodes.is_empty() {
                continue;
            }

            let type_param_names = type_parameters
                .nodes
                .iter()
                .filter_map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_type_parameter(param_node)?;
                    self.get_identifier_text(param.name)
                })
                .collect::<Vec<_>>();
            if type_param_names.is_empty() {
                continue;
            }

            let mut inferred_args =
                self.generic_new_expression_default_type_args(type_parameters)?;
            if inferred_args.len() != type_param_names.len() {
                continue;
            }

            let mut inferred_any = false;
            self.infer_generic_new_expression_args_from_constructor(
                class_data,
                &type_param_names,
                &args.nodes,
                &mut inferred_args,
                &mut inferred_any,
            );
            self.infer_generic_new_expression_args_from_base(
                class_data,
                &type_param_names,
                &args.nodes,
                &mut inferred_args,
                &mut inferred_any,
            );

            if inferred_any {
                return Some(format!("{base_text}<{}>", inferred_args.join(", ")));
            }
        }

        None
    }

    fn generic_new_expression_default_type_args(
        &self,
        type_parameters: &NodeList,
    ) -> Option<Vec<String>> {
        type_parameters
            .nodes
            .iter()
            .map(|&param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_type_parameter(param_node)?;
                if param.default.is_some() {
                    let default_node = self.arena.get(param.default)?;
                    self.get_source_slice_no_semi(default_node.pos, default_node.end)
                } else {
                    Some("unknown".to_string())
                }
            })
            .collect()
    }

    fn infer_generic_new_expression_args_from_constructor(
        &self,
        class_data: &ClassData,
        type_param_names: &[String],
        arg_nodes: &[NodeIndex],
        inferred_args: &mut [String],
        inferred_any: &mut bool,
    ) {
        let Some(constructor) = class_data.members.nodes.iter().find_map(|&member_idx| {
            let member_node = self.arena.get(member_idx)?;
            self.arena.get_constructor(member_node)
        }) else {
            return;
        };

        for (param_idx, &arg_idx) in constructor.parameters.nodes.iter().zip(arg_nodes.iter()) {
            let Some(param_node) = self.arena.get(*param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(type_text) = self.source_slice_from_arena(self.arena, param.type_annotation)
            else {
                continue;
            };
            let type_text = type_text.trim();
            if let Some(type_param_index) = type_param_names
                .iter()
                .position(|name| name.as_str() == type_text)
                && let Some(arg_type_text) = self.generic_new_expression_arg_type_text(arg_idx)
            {
                inferred_args[type_param_index] = arg_type_text;
                *inferred_any = true;
            }
        }
    }

    fn infer_generic_new_expression_args_from_base(
        &self,
        class_data: &ClassData,
        type_param_names: &[String],
        arg_nodes: &[NodeIndex],
        inferred_args: &mut [String],
        inferred_any: &mut bool,
    ) {
        if class_data.members.nodes.iter().any(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|member_node| self.arena.get_constructor(member_node).is_some())
        }) {
            return;
        }
        let Some(first_arg) = arg_nodes.first().copied() else {
            return;
        };
        let Some(heritage_clauses) = class_data.heritage_clauses.as_ref() else {
            return;
        };
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&heritage_type_idx) = heritage.types.nodes.first() else {
                return;
            };
            let Some(heritage_type_node) = self.arena.get(heritage_type_idx) else {
                return;
            };
            let Some(expr_type_args) = self.arena.get_expr_type_args(heritage_type_node) else {
                return;
            };
            if expr_type_args
                .type_arguments
                .as_ref()
                .map_or(0, |args| args.nodes.len())
                != 1
                || type_param_names.len() != 1
            {
                return;
            }
            let Some(type_arg_idx) = expr_type_args
                .type_arguments
                .as_ref()
                .and_then(|args| args.nodes.first().copied())
            else {
                return;
            };
            let Some(type_arg_text) = self
                .get_source_slice_no_semi(
                    self.arena.get(type_arg_idx).map_or(0, |node| node.pos),
                    self.arena.get(type_arg_idx).map_or(0, |node| node.end),
                )
                .map(|text| text.trim().to_string())
            else {
                return;
            };
            if type_arg_text != type_param_names[0] {
                return;
            }
            if let Some(arg_type_text) = self.generic_new_expression_arg_type_text(first_arg) {
                inferred_args[0] = arg_type_text;
                *inferred_any = true;
            }
            return;
        }
    }

    fn generic_new_expression_arg_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        if let Some(interner) = self.type_interner
            && let Some(type_id) = self.get_node_type_or_names(&[arg_idx])
        {
            let widened = tsz_solver::operations::widening::widen_literal_type(interner, type_id);
            return Some(self.print_type_id_for_inferred_declaration(widened));
        }

        self.widened_primitive_literal_type_text(arg_idx)
    }

    fn widened_primitive_literal_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let unary = self.arena.get_unary_expr(expr_node)?;
                let operand = self.arena.get(unary.operand)?;
                match operand.kind {
                    k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
                    k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
