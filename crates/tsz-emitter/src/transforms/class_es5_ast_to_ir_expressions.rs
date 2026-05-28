//! Expression conversion: calls (including dynamic `import()` and `super.x()`),
//! property and element access, binary/unary operators, parenthesized, and
//! conditional expressions.
//!
//! Extracted from `class_es5_ast_to_ir.rs` so the central AST→IR conversion
//! file stays under the §19 2000-line cap. Behavior is unchanged.

use super::{AstToIr, IRNode, IRPrinter, get_identifier_text};
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> AstToIr<'a> {
    pub(super) fn convert_call_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(call) = self.arena.get_call_expr(node) {
            let args: Vec<IRNode> = if let Some(ref args) = call.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };

            if matches!(
                self.module_kind,
                ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System
            ) && let Some(callee_node) = self.arena.get(call.expression)
                && callee_node.kind == SyntaxKind::ImportKeyword as u16
            {
                return self.convert_wrapped_dynamic_import(call.arguments.as_ref());
            }

            // Check for bare super(args) → _this = _super.call(this, args) || this
            // This handles super() in expression contexts (e.g. computed property names).
            if self.has_super
                && let Some(cn) = self.arena.get(call.expression)
                && cn.kind == SyntaxKind::SuperKeyword as u16
            {
                let mut call_args = vec![IRNode::this()];
                call_args.extend(args);
                // _this = _super.call(this, args...) || this
                return IRNode::assign(
                    IRNode::id("_this"),
                    IRNode::logical_or(
                        IRNode::call(
                            IRNode::prop(IRNode::id(self.super_name.clone()), "call"),
                            call_args,
                        ),
                        IRNode::this(),
                    ),
                );
            }

            // Check for super.method(args) or super[expr](args) → _super.prototype.method.call(this, args)
            if let Some(super_call) = self.try_convert_super_method_call(
                call.expression,
                args.clone(),
                node.is_optional_chain(),
            ) {
                return super_call;
            }

            let callee = self.convert_expression(call.expression);
            IRNode::CallExpr {
                callee: Box::new(callee),
                arguments: args,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_wrapped_dynamic_import(&self, args: Option<&NodeList>) -> IRNode {
        let first_arg = self.first_dynamic_import_argument(args);
        let first_arg_is_string_like = first_arg.is_none_or(|arg| {
            crate::transforms::emit_utils::dynamic_import_arg_is_string_like(self.arena, arg)
        });

        let mut specifier = first_arg
            .map(|arg| self.emit_ir_fragment_to_string(&self.convert_expression(arg)))
            .unwrap_or_default();
        let mut prefix = String::new();

        if first_arg.is_some() && !first_arg_is_string_like {
            let temp = self.generate_hoisted_temp();
            prefix = format!("{temp} = {specifier}, ");
            specifier = temp;
        }

        if matches!(self.module_kind, ModuleKind::System) {
            return IRNode::Raw(format!("context_1.import({specifier})").into());
        }

        let amd_branch = self.dynamic_import_amd_branch(&specifier);
        if matches!(self.module_kind, ModuleKind::UMD) {
            return IRNode::Raw(
                format!(
                    "{prefix}__syncRequire ? {} : {amd_branch}",
                    self.dynamic_import_commonjs_branch(&specifier)
                )
                .into(),
            );
        }

        IRNode::Raw(format!("{prefix}{amd_branch}").into())
    }

    fn first_dynamic_import_argument(&self, args: Option<&NodeList>) -> Option<NodeIndex> {
        args?
            .nodes
            .iter()
            .copied()
            .find(|&idx| crate::transforms::emit_utils::call_argument_should_emit(self.arena, idx))
    }

    pub(super) fn emit_ir_fragment_to_string(&self, ir: &IRNode) -> String {
        let mut printer = if let Some(source_text) = self.source_text {
            IRPrinter::with_arena_and_source(self.arena, source_text)
        } else {
            IRPrinter::with_arena(self.arena)
        };
        if let Some(transforms) = self.transforms.as_ref() {
            printer.set_transforms(transforms.clone());
        }
        printer.emit(ir).to_string()
    }

    fn dynamic_import_commonjs_branch(&self, specifier: &str) -> String {
        crate::transforms::emit_utils::dynamic_import_cjs_form(specifier)
    }

    fn dynamic_import_amd_branch(&self, specifier: &str) -> String {
        let id = self.dynamic_import_promise_counter.get();
        self.dynamic_import_promise_counter.set(id + 1);
        format!(
            "new Promise(function (resolve_{id}, reject_{id}) {{ require([{specifier}], resolve_{id}, reject_{id}); }}).then(__importStar)"
        )
    }

    /// Check if a call expression callee is super.method or super[expr] and transform to
    /// _super.prototype.method.call(this, args) or _super.prototype[expr].call(this, args)
    fn try_convert_super_method_call(
        &self,
        callee_idx: NodeIndex,
        args: Vec<IRNode>,
        is_optional_call: bool,
    ) -> Option<IRNode> {
        let callee_node = self.arena.get(callee_idx)?;

        // Check for super.method(args) → _super.prototype.method.call(this, args)
        // In static context: super.method(args) → _super.method.call(this, args)
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let obj_node = self.arena.get(access.expression)?;
            if obj_node.kind == SyntaxKind::SuperKeyword as u16 {
                let method_name = get_identifier_text(self.arena, access.name_or_argument)?;
                let super_proto_method = if self.is_static.get() {
                    // Static: _super.method
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::id(self.super_name.clone())),
                        property: method_name.into(),
                    }
                } else {
                    // Instance: _super.prototype.method
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::PropertyAccess {
                            object: Box::new(IRNode::id(self.super_name.clone())),
                            property: "prototype".to_string().into(),
                        }),
                        property: method_name.into(),
                    }
                };
                if is_optional_call {
                    return Some(self.convert_optional_super_method_call(super_proto_method, args));
                }
                let call_method = IRNode::PropertyAccess {
                    object: Box::new(super_proto_method),
                    property: "call".to_string().into(),
                };
                let mut call_args = vec![self.current_this_ir()];
                call_args.extend(args);
                return Some(IRNode::CallExpr {
                    callee: Box::new(call_method),
                    arguments: call_args,
                });
            }
        }

        // Check for super[expr](args) → _super.prototype[expr].call(this, args)
        // In static context: super[expr](args) → _super[expr].call(this, args)
        if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let obj_node = self.arena.get(access.expression)?;
            if obj_node.kind == SyntaxKind::SuperKeyword as u16 {
                let index_expr = self.convert_expression(access.name_or_argument);
                let super_base = if self.is_static.get() {
                    IRNode::id(self.super_name.clone())
                } else {
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::id(self.super_name.clone())),
                        property: "prototype".to_string().into(),
                    }
                };
                let super_proto_elem = IRNode::ElementAccess {
                    object: Box::new(super_base),
                    index: Box::new(index_expr),
                };
                if is_optional_call {
                    return Some(self.convert_optional_super_method_call(super_proto_elem, args));
                }
                let call_method = IRNode::PropertyAccess {
                    object: Box::new(super_proto_elem),
                    property: "call".to_string().into(),
                };
                let mut call_args = vec![self.current_this_ir()];
                call_args.extend(args);
                return Some(IRNode::CallExpr {
                    callee: Box::new(call_method),
                    arguments: call_args,
                });
            }
        }

        None
    }

    fn convert_optional_super_method_call(&self, receiver: IRNode, args: Vec<IRNode>) -> IRNode {
        let temp = self.generate_hoisted_temp();
        let temp_ref = || IRNode::id(temp.clone());

        let mut call_args = vec![self.current_this_ir()];
        call_args.extend(args);

        IRNode::ConditionalExpr {
            condition: Box::new(IRNode::logical_or(
                IRNode::binary(
                    IRNode::assign(temp_ref(), receiver).paren(),
                    "===",
                    IRNode::NullLiteral,
                ),
                IRNode::binary(temp_ref(), "===", IRNode::Undefined),
            )),
            when_true: Box::new(IRNode::Undefined),
            when_false: Box::new(IRNode::CallExpr {
                callee: Box::new(IRNode::PropertyAccess {
                    object: Box::new(temp_ref()),
                    property: "call".to_string().into(),
                }),
                arguments: call_args,
            }),
        }
    }

    pub(super) fn convert_new_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // NewExpression uses CallExprData (same as CallExpression)
        if let Some(call_data) = self.arena.get_call_expr(node) {
            let callee = self.convert_expression(call_data.expression);
            let args = if let Some(ref args) = call_data.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };
            IRNode::NewExpr {
                callee: Box::new(callee),
                arguments: args,
                explicit_arguments: call_data.arguments.is_some(),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_property_access(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // PropertyAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            // Check for super.property → _super.prototype.property (instance) or _super.property (static)
            if let Some(obj_node) = self.arena.get(access.expression)
                && obj_node.kind == SyntaxKind::SuperKeyword as u16
                && let Some(name) = get_identifier_text(self.arena, access.name_or_argument)
            {
                return if self.is_static.get() {
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::id(self.super_name.clone())),
                        property: name.into(),
                    }
                } else {
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::PropertyAccess {
                            object: Box::new(IRNode::id(self.super_name.clone())),
                            property: "prototype".to_string().into(),
                        }),
                        property: name.into(),
                    }
                };
            }

            let object = self.convert_expression(access.expression);
            if let Some(name) = get_identifier_text(self.arena, access.name_or_argument) {
                return IRNode::PropertyAccess {
                    object: Box::new(object),
                    property: name.into(),
                };
            }
        }
        IRNode::ASTRef(idx)
    }

    pub(super) fn convert_element_access(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // ElementAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            // Check for super[expr] → _super.prototype[expr] (instance) or _super[expr] (static)
            if let Some(obj_node) = self.arena.get(access.expression)
                && obj_node.kind == SyntaxKind::SuperKeyword as u16
            {
                let index = self.convert_expression(access.name_or_argument);
                let super_base = if self.is_static.get() {
                    IRNode::id(self.super_name.clone())
                } else {
                    IRNode::PropertyAccess {
                        object: Box::new(IRNode::id(self.super_name.clone())),
                        property: "prototype".to_string().into(),
                    }
                };
                return IRNode::ElementAccess {
                    object: Box::new(super_base),
                    index: Box::new(index),
                };
            }

            let object = self.convert_expression(access.expression);
            let index = self.convert_expression(access.name_or_argument);
            IRNode::ElementAccess {
                object: Box::new(object),
                index: Box::new(index),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_binary_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(bin) = self.arena.get_binary_expr(node) {
            let left = self.convert_expression(bin.left);
            let right = self.convert_expression(bin.right);
            let op = self.get_binary_operator(bin.operator_token);

            // Handle logical operators specially
            if op == "||" {
                return IRNode::LogicalOr {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            if op == "&&" {
                return IRNode::LogicalAnd {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }

            IRNode::BinaryExpr {
                left: Box::new(left),
                operator: op.into(),
                right: Box::new(right),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_binary_operator(&self, token: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(token).to_string()
    }

    pub(super) fn convert_prefix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // PrefixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = self.get_prefix_operator(unary.operator);
            IRNode::PrefixUnaryExpr {
                operator: op.into(),
                operand: Box::new(operand),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_prefix_operator(&self, token: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(token).to_string()
    }

    pub(super) fn convert_postfix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // PostfixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = match unary.operator {
                k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
                k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
                _ => "".to_string(),
            };
            IRNode::PostfixUnaryExpr {
                operand: Box::new(operand),
                operator: op.into(),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_parenthesized(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(paren) = self.arena.get_parenthesized(node) {
            IRNode::Parenthesized(Box::new(self.convert_expression(paren.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_conditional(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // ConditionalExpression uses ConditionalExprData
        if let Some(cond) = self.arena.get_conditional_expr(node) {
            IRNode::ConditionalExpr {
                condition: Box::new(self.convert_expression(cond.condition)),
                when_true: Box::new(self.convert_expression(cond.when_true)),
                when_false: Box::new(self.convert_expression(cond.when_false)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }
}
