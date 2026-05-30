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

/// Continuation applied to the (non-nullish) receiver of a downlevel
/// optional-chain access. Built once and applied to the guarded receiver in
/// the ternary's false branch.
pub(super) enum OptionalChainTail {
    /// `<recv>.name`
    Property(std::borrow::Cow<'static, str>),
    /// `<recv>[index]`
    Element(Box<IRNode>),
    /// `<recv>.name(args)` — the access carried `?.`, the call did not.
    MethodCall {
        property: std::borrow::Cow<'static, str>,
        arguments: Vec<IRNode>,
    },
    /// `<recv>[index](args)` — the access carried `?.`, the call did not.
    ElementMethodCall {
        index: Box<IRNode>,
        arguments: Vec<IRNode>,
    },
}

impl OptionalChainTail {
    fn apply(self, receiver: IRNode) -> IRNode {
        match self {
            Self::Property(name) => IRNode::PropertyAccess {
                object: Box::new(receiver),
                property: name,
            },
            Self::Element(index) => IRNode::ElementAccess {
                object: Box::new(receiver),
                index,
            },
            Self::MethodCall {
                property,
                arguments,
            } => IRNode::CallExpr {
                callee: Box::new(IRNode::PropertyAccess {
                    object: Box::new(receiver),
                    property,
                }),
                arguments,
            },
            Self::ElementMethodCall { index, arguments } => IRNode::CallExpr {
                callee: Box::new(IRNode::ElementAccess {
                    object: Box::new(receiver),
                    index,
                }),
                arguments,
            },
        }
    }
}

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

            // Optional method call `R?.m(args)` / `R?.[k](args)`: the access
            // carries `?.` but the call itself does not, so the whole call
            // short-circuits on `R`. Lower the guard with the call in the
            // false branch (the IR has no optional-access node, mirroring the
            // AST printer's non-ES2020 form). `R.m?.()` (an optional *call*
            // token) is a different shape and is intentionally left to the
            // existing path.
            if node.is_optional_chain()
                && let Some(optional_call) =
                    self.try_convert_optional_method_call(call.expression, args.clone())
            {
                return optional_call;
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

    /// The ES5 receiver a `super` keyword lowers to in this member context:
    /// `_super.prototype` for an instance member home, `_super` for a static
    /// one. The choice is keyed on the static/instance context of the enclosing
    /// member, not on the spelling of the property that follows `super`.
    pub(super) fn es5_super_receiver_base(&self) -> IRNode {
        if self.is_static.get() {
            IRNode::id(self.super_name.clone())
        } else {
            IRNode::PropertyAccess {
                object: Box::new(IRNode::id(self.super_name.clone())),
                property: "prototype".to_string().into(),
            }
        }
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
                // Static: `_super.method`; instance: `_super.prototype.method`.
                let super_proto_method = IRNode::PropertyAccess {
                    object: Box::new(self.es5_super_receiver_base()),
                    property: method_name.into(),
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
                let super_proto_elem = IRNode::ElementAccess {
                    object: Box::new(self.es5_super_receiver_base()),
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

    /// Lower `R?.m(args)` / `R?.[k](args)` where the *access* carried `?.` but
    /// the call did not. Returns `None` for any other callee shape (including
    /// `R.m?.()`, where the call token itself is optional) so the caller falls
    /// back to its normal path.
    fn try_convert_optional_method_call(
        &self,
        callee_idx: NodeIndex,
        args: Vec<IRNode>,
    ) -> Option<IRNode> {
        let callee_node = self.arena.get(callee_idx)?;
        let access = self.arena.get_access_expr(callee_node)?;
        if !access.question_dot_token {
            return None;
        }
        // `super?.m()` cannot capture `super`; leave it to the existing path.
        if self
            .arena
            .get(access.expression)
            .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16)
        {
            return None;
        }

        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let name = get_identifier_text(self.arena, access.name_or_argument)?;
            return Some(self.lower_optional_chain_guard(
                access.expression,
                OptionalChainTail::MethodCall {
                    property: name.into(),
                    arguments: args,
                },
            ));
        }
        if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let index = self.convert_expression(access.name_or_argument);
            return Some(self.lower_optional_chain_guard(
                access.expression,
                OptionalChainTail::ElementMethodCall {
                    index: Box::new(index),
                    arguments: args,
                },
            ));
        }
        None
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
            {
                // A bare `super` recovers as `super.<missing>` (TS1034). tsc still
                // substitutes the `super` receiver with `_super.prototype`
                // (instance) / `_super` (static) and emits the dangling member
                // access verbatim, yielding `_super.prototype.` / `_super.`. The
                // receiver substitution is keyed on the base being the `super`
                // keyword, independent of whether a property name is present.
                let property =
                    get_identifier_text(self.arena, access.name_or_argument).unwrap_or_default();
                return IRNode::PropertyAccess {
                    object: Box::new(self.es5_super_receiver_base()),
                    property: property.into(),
                };
            }

            if let Some(name) = get_identifier_text(self.arena, access.name_or_argument) {
                // Optional chain: `R?.prop` short-circuits when `R` is nullish.
                // The IR has no optional-access node, so lower the guard here the
                // same way the AST printer does for non-ES2020 targets.
                if access.question_dot_token {
                    return self.lower_optional_chain_guard(
                        access.expression,
                        OptionalChainTail::Property(name.into()),
                    );
                }
                let object = self.convert_expression(access.expression);
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
                return IRNode::ElementAccess {
                    object: Box::new(self.es5_super_receiver_base()),
                    index: Box::new(index),
                };
            }

            // Optional chain: `R?.[idx]` short-circuits when `R` is nullish.
            if access.question_dot_token {
                let index = self.convert_expression(access.name_or_argument);
                return self.lower_optional_chain_guard(
                    access.expression,
                    OptionalChainTail::Element(Box::new(index)),
                );
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

    /// Lower a downlevel optional-chain access whose head receiver is
    /// `receiver_idx` and whose continuation is `tail`.
    ///
    /// Matches the non-ES2020 AST-printer form:
    /// - simple receiver `R`: `R === null || R === void 0 ? void 0 : R<tail>`
    /// - other receiver `E`:  `(_t = E) === null || _t === void 0 ? void 0 : _t<tail>`
    ///
    /// `receiver_idx` is converted through `convert_expression`, so `this`
    /// substitution (e.g. the static class alias) and nested lowering still
    /// apply. The rule keys on the access node's `?.` token, not on any
    /// identifier name or rendered text.
    pub(super) fn lower_optional_chain_guard(
        &self,
        receiver_idx: NodeIndex,
        tail: OptionalChainTail,
    ) -> IRNode {
        let receiver = self.convert_expression(receiver_idx);
        let receiver_simple =
            crate::transforms::emit_utils::is_simple_copiable_expression(self.arena, receiver_idx);

        // `guard_head` is the left operand of the first `=== null` comparison;
        // `body_receiver` is reused for the second comparison and the access
        // body. For a simple receiver both are the receiver itself; otherwise
        // the receiver is captured once via `(_t = E)` and referenced as `_t`.
        let (guard_head, body_receiver): (IRNode, IRNode) = if receiver_simple {
            (receiver.clone(), receiver)
        } else {
            let temp = self.generate_hoisted_temp();
            (
                IRNode::assign(IRNode::id(temp.clone()), receiver).paren(),
                IRNode::id(temp),
            )
        };

        let condition = IRNode::logical_or(
            IRNode::binary(guard_head, "===", IRNode::NullLiteral),
            IRNode::binary(body_receiver.clone(), "===", IRNode::Undefined),
        );
        let when_false = tail.apply(body_receiver);

        IRNode::ConditionalExpr {
            condition: Box::new(condition),
            when_true: Box::new(IRNode::Undefined),
            when_false: Box::new(when_false),
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
            // Parentheses that exist only to scope a type assertion become
            // redundant once the assertion is erased: `(e as Error).message`
            // emits `e.message`, not `(e).message`. Mirror the normal emitter's
            // policy of dropping such parens when the erased inner expression is
            // a simple primary whose meaning cannot change without parens.
            if self.parenthesized_wraps_erasable_simple_primary(paren.expression) {
                return self.convert_expression(paren.expression);
            }
            IRNode::Parenthesized(Box::new(self.convert_expression(paren.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    /// True when a parenthesized expression directly wraps a type assertion
    /// (`as`/`<T>`/satisfies) whose underlying expression, after erasing the
    /// assertion, is a simple primary that does not require the parentheses.
    fn parenthesized_wraps_erasable_simple_primary(&self, inner_idx: NodeIndex) -> bool {
        let Some(inner) = self.arena.get(inner_idx) else {
            return false;
        };
        // Only the type-assertion forms make the wrapping parens purely
        // syntactic. Everything else keeps its parens.
        if !(inner.kind == syntax_kind_ext::TYPE_ASSERTION
            || inner.kind == syntax_kind_ext::AS_EXPRESSION
            || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
        {
            return false;
        }
        // Peel the type-assertion chain to the underlying expression.
        let mut cur = inner_idx;
        loop {
            let Some(node) = self.arena.get(cur) else {
                return false;
            };
            let is_assertion = node.kind == syntax_kind_ext::TYPE_ASSERTION
                || node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION;
            if is_assertion {
                let Some(assertion) = self.arena.get_type_assertion(node) else {
                    return false;
                };
                cur = assertion.expression;
                continue;
            }
            // `node` is the erased underlying expression. An optional-chain
            // member is load-bearing in access position, so never strip those.
            if node.is_optional_chain()
                || self
                    .arena
                    .get_access_expr(node)
                    .is_some_and(|a| a.question_dot_token)
            {
                return false;
            }
            return matches!(
                node.kind,
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == SyntaxKind::ThisKeyword as u16
                    || k == SyntaxKind::SuperKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::StringLiteral as u16
                    || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
            );
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

#[cfg(test)]
mod optional_chain_in_class_member_tests {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;
    use tsz_common::ScriptTarget;
    use tsz_parser::ParserState;

    /// Emit `source` as ES5 JS through the full class-IR lowering pipeline.
    fn emit_es5(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let ctx = EmitContext::with_options(options.clone());
        let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
        let mut printer =
            EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
        printer.set_source_text(source);
        printer.emit(root);
        printer.get_output().to_string()
    }

    // Structural rule: when the ES5 class-IR converter sees a property/element
    // access (or `recv?.m()` method call) carrying `?.`, it must lower the
    // nullish short-circuit guard rather than dropping the token. The rule keys
    // on the access node's `?.` flag, not on the receiver's spelling — so these
    // tests vary class/member names, member kinds, and access/call shapes.

    #[test]
    fn static_property_initializer_this_optional_property_keeps_guard() {
        // `this` inside a static initializer is substituted with the class
        // alias; the optional-property guard must survive that substitution.
        let output = emit_es5("class Widget {\n    static handle = this?.id;\n}\n");
        assert!(
            output.contains("=== null ||") && output.contains("=== void 0 ? void 0 :"),
            "Static `this?.id` must keep the optional-chain guard.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("Widget.handle = _a.id;"),
            "Optional access must not be dropped to a plain property access.\nOutput:\n{output}"
        );
    }

    #[test]
    fn static_property_initializer_this_optional_method_call_keeps_guard() {
        // Different class/member names; optional method call `this?.compute()`.
        let output = emit_es5("class Engine {\n    static result = this?.compute();\n}\n");
        assert!(
            output.contains("=== null ||")
                && output.contains("=== void 0 ? void 0 :")
                && output.contains(".compute()"),
            "Static `this?.compute()` must guard the call.\nOutput:\n{output}"
        );
    }

    #[test]
    fn static_property_initializer_this_optional_element_call_keeps_guard() {
        // Element-access optional method call inside a static initializer.
        let output = emit_es5("class Store {\n    static v = this?.[\"load\"]();\n}\n");
        assert!(
            output.contains("=== null ||")
                && output.contains("=== void 0 ? void 0 :")
                && output.contains("[\"load\"]()"),
            "Static `this?.[\"load\"]()` must guard the element call.\nOutput:\n{output}"
        );
    }

    #[test]
    fn static_method_body_this_optional_access_keeps_guard() {
        // Static *method* body (not just initializer), different name again.
        let output =
            emit_es5("class Service {\n    static run() {\n        return this?.go();\n    }\n}\n");
        assert!(
            output.contains("=== null ||") && output.contains("=== void 0 ? void 0 :"),
            "Static method `this?.go()` must keep the guard.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_method_body_this_optional_access_keeps_guard() {
        // Instance method body — proves the fix is not static-specific.
        let output = emit_es5("class Cache {\n    m() {\n        return this?.entry;\n    }\n}\n");
        assert!(
            output.contains("this === null || this === void 0 ? void 0 : this.entry"),
            "Instance `this?.entry` must keep the guard.\nOutput:\n{output}"
        );
    }

    #[test]
    fn class_member_identifier_receiver_optional_access_keeps_guard() {
        // Receiver is a plain identifier, not `this` — proves the rule keys on
        // the `?.` token, not on the `this` keyword.
        let output =
            emit_es5("declare const dep: any;\nclass Host {\n    static value = dep?.field;\n}\n");
        assert!(
            output.contains("dep === null || dep === void 0 ? void 0 : dep.field"),
            "Identifier-receiver `dep?.field` must keep the guard.\nOutput:\n{output}"
        );
    }

    #[test]
    fn class_member_non_optional_access_is_unchanged() {
        // Negative case: a non-optional access must NOT gain a guard.
        let output = emit_es5("class Plain {\n    static value = this.field;\n}\n");
        assert!(
            !output.contains("=== void 0 ? void 0 :"),
            "Non-optional `this.field` must not be lowered to a guard.\nOutput:\n{output}"
        );
    }
}
