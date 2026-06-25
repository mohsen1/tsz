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

            // Private member call `recv.#m(args)`: a private method, a private
            // field holding a function, or a private getter invoked in call
            // position. The member is read through `__classPrivateFieldGet(...)`
            // and invoked with `.call(recv, args)` so the original receiver is
            // preserved as `this` (mirroring tsc and the main, non-ES5 emitter).
            // Without `.call`, a private method read (`kind: "m"`) would emit the
            // bare callee `recv.()` because a private identifier has no plain
            // property name; the `.call` form is what makes the lowering valid.
            if let Some(private_call) = self.try_convert_private_member_call(
                call.expression,
                &args,
                node.is_optional_chain(),
            ) {
                return private_call;
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

    /// Lower `recv.#name(args)` where `#name` is a private member with a read
    /// slot (method, function-valued field, or getter) to
    /// `__classPrivateFieldGet(recv, brand, kind[, fn]).call(recv, args)`.
    ///
    /// The receiver is referenced twice (once to read the member, once as the
    /// `.call` `this`), so a side-effecting receiver is captured once into a
    /// hoisted temp: `(_a = side()).….call(_a, args)`. Returns `None` for a
    /// non-private callee, an optional chain (handled separately), or a private
    /// name with no read slot (e.g. a static private method, left to the
    /// fallthrough).
    fn try_convert_private_member_call(
        &self,
        callee_idx: NodeIndex,
        args: &[IRNode],
        is_optional_chain: bool,
    ) -> Option<IRNode> {
        if is_optional_chain {
            return None;
        }
        let (receiver_idx, clean) = self.private_access_target(callee_idx)?;
        // A private name with no read slot (e.g. a static private method) is
        // left to the fallthrough. Checked before any temp is allocated so the
        // fallthrough cannot leak a hoisted `var`.
        if !self.has_private_read_slot(&clean) {
            return None;
        }

        // Capture a side-effecting receiver once so it is evaluated a single
        // time and shared between the member read and the `.call` `this`.
        let (get_ir, call_receiver) =
            if crate::transforms::emit_utils::is_simple_copiable_expression(
                self.arena,
                receiver_idx,
            ) {
                (
                    self.private_field_get_ir(receiver_idx, &clean)?,
                    self.convert_expression(receiver_idx),
                )
            } else {
                let temp = self.generate_hoisted_temp();
                let captured = IRNode::Parenthesized(Box::new(IRNode::assign(
                    IRNode::id(temp.clone()),
                    self.convert_expression(receiver_idx),
                )));
                (
                    self.private_field_get_ir_with_receiver(captured, &clean)?,
                    IRNode::id(temp),
                )
            };

        let mut call_args = Vec::with_capacity(args.len() + 1);
        call_args.push(call_receiver);
        call_args.extend(args.iter().cloned());
        Some(IRNode::call(IRNode::prop(get_ir, "call"), call_args))
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

            // Private field/accessor read: `this.#x` → `__classPrivateFieldGet(this, _C_x, "f")`
            if let Some(name_node) = self.arena.get(access.name_or_argument)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                if let Some(ident) = self.arena.get_identifier(name_node) {
                    let raw = &ident.escaped_text;
                    let clean = raw.strip_prefix('#').unwrap_or(raw.as_str());
                    if let Some(get_ir) = self.private_field_get_ir(access.expression, clean) {
                        return get_ir;
                    }
                }
                // Unknown private name — fall through to ASTRef
                return IRNode::ASTRef(idx);
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

    /// Decompose a `recv.#name` property access into `(receiver_idx,
    /// clean_name)`, or `None` when `idx` is not a private-identifier property
    /// access. Pure AST shape — it does not consult the storage maps.
    fn private_access_target(&self, idx: NodeIndex) -> Option<(NodeIndex, String)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let name_node = self.arena.get(access.name_or_argument)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        let ident = self.arena.get_identifier(name_node)?;
        let raw = &ident.escaped_text;
        let clean = raw.strip_prefix('#').unwrap_or(raw.as_str()).to_string();
        Some((access.expression, clean))
    }

    /// If `idx` is a `recv.#name` property access whose member is a private
    /// field or accessor with **both** a read and a write slot, and whose
    /// receiver is a simple, side-effect-free expression safe to evaluate more
    /// than once, return `(receiver_idx, clean_name)`.
    ///
    /// Compound assignment (`this.#x += v`) and `++`/`--` mutation read the slot
    /// and then write it, so they reference the receiver twice. A non-simple
    /// receiver (which must be evaluated exactly once) and a private *method*
    /// (no field slot) are intentionally rejected here and left to the existing
    /// fallthrough. The rule keys on the member being a `PrivateIdentifier` with
    /// a storage entry, never on its spelling.
    fn private_mutation_target(&self, idx: NodeIndex) -> Option<(NodeIndex, String)> {
        let (receiver_idx, clean) = self.private_access_target(idx)?;
        // Read-modify-write needs both a get slot and a set slot.
        self.private_read_info(&clean)?;
        self.private_write_info(&clean)?;
        if !crate::transforms::emit_utils::is_simple_copiable_expression(self.arena, receiver_idx) {
            return None;
        }
        Some((receiver_idx, clean))
    }

    /// `__classPrivateFieldGet(receiver, <brand>, "<kind>"[, <fn>])` for a known
    /// private field/accessor/method read. `None` when the name has no read slot.
    fn private_field_get_ir(&self, receiver_idx: NodeIndex, clean_name: &str) -> Option<IRNode> {
        self.private_field_get_ir_with_receiver(self.convert_expression(receiver_idx), clean_name)
    }

    /// Like [`Self::private_field_get_ir`] but with a pre-built receiver node,
    /// so a side-effecting receiver can be captured once (e.g.
    /// `(_a = side())`) before it is reused by a `.call`.
    fn private_field_get_ir_with_receiver(
        &self,
        receiver: IRNode,
        clean_name: &str,
    ) -> Option<IRNode> {
        let (brand_var, kind, member_ref) = self.private_read_info(clean_name)?;
        let mut args = vec![
            receiver,
            IRNode::id(brand_var),
            IRNode::StringLiteral(kind.into()),
        ];
        if let Some(member_ref) = member_ref {
            args.push(IRNode::id(member_ref));
        }
        Some(IRNode::call(
            IRNode::RuntimeHelper(std::borrow::Cow::Borrowed("__classPrivateFieldGet")),
            args,
        ))
    }

    /// `__classPrivateFieldSet(receiver, <brand>, value, "<kind>"[, <fn>])` for
    /// a known private field/accessor write. `None` when the name has no write
    /// slot (e.g. a getter-only accessor).
    fn private_field_set_ir(
        &self,
        receiver_idx: NodeIndex,
        clean_name: &str,
        value: IRNode,
    ) -> Option<IRNode> {
        let (brand_var, kind, member_ref) = self.private_write_info(clean_name)?;
        let mut args = vec![
            self.convert_expression(receiver_idx),
            IRNode::id(brand_var),
            value,
            IRNode::StringLiteral(kind.into()),
        ];
        if let Some(member_ref) = member_ref {
            args.push(IRNode::id(member_ref));
        }
        Some(IRNode::call(
            IRNode::RuntimeHelper(std::borrow::Cow::Borrowed("__classPrivateFieldSet")),
            args,
        ))
    }

    /// Base operator for an ES5-lowerable private-field compound assignment
    /// (`+=` -> `+`). Returns `None` for `=`, exponent (`**=`), and logical
    /// (`&&= ||= ??=`) assignments: `**=`/`&&=`/`||=` are handled separately by
    /// `private_exp_or_logical_compound_ir`, and `??=` stays on the fallthrough.
    const fn es5_private_compound_base_op(token: u16) -> Option<&'static str> {
        Some(match token {
            t if t == SyntaxKind::PlusEqualsToken as u16 => "+",
            t if t == SyntaxKind::MinusEqualsToken as u16 => "-",
            t if t == SyntaxKind::AsteriskEqualsToken as u16 => "*",
            t if t == SyntaxKind::SlashEqualsToken as u16 => "/",
            t if t == SyntaxKind::PercentEqualsToken as u16 => "%",
            t if t == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<",
            t if t == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>",
            t if t == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => ">>>",
            t if t == SyntaxKind::AmpersandEqualsToken as u16 => "&",
            t if t == SyntaxKind::CaretEqualsToken as u16 => "^",
            t if t == SyntaxKind::BarEqualsToken as u16 => "|",
            _ => return None,
        })
    }

    /// Lower `this.#x **= v` / `this.#x &&= v` / `this.#x ||= v` for a known
    /// private slot, or `None` to fall through.
    ///
    /// - `**=` is an *unconditional* write, so it folds to
    ///   `__classPrivateFieldSet(this, _C_x, Math.pow(__classPrivateFieldGet(this, _C_x, "f"), v), "f")`
    ///   for both fields and accessors. ES5/ES3 — this transform's only targets
    ///   — have no `**` operator, so `Math.pow` is always required.
    /// - `&&=` / `||=` are *conditional* writes. The always-write fold
    ///   `set(this, _C_x, get() && v, "f")` is observably equivalent only when
    ///   the slot has no write side effect, i.e. a plain field (kind `"f"`): for
    ///   a field, `a &&= b` ≡ `a = (a && b)` because the short-circuit value is
    ///   stored idempotently. For an accessor the setter would run when the short
    ///   circuit says skip, so accessor short-circuit assignment falls through.
    ///   `??=` also falls through: at ES5 the `??` itself needs nullish lowering.
    ///
    /// A conditional-expression rhs is parenthesized because `&&`/`||` bind
    /// tighter than `?:`, mirroring the main emitter's private-field policy.
    fn private_exp_or_logical_compound_ir(
        &self,
        operator_token: u16,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<IRNode> {
        let (receiver_idx, clean) = self.private_mutation_target(left_idx)?;

        if operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16 {
            let get_ir = self.private_field_get_ir(receiver_idx, &clean)?;
            let rhs = self.convert_expression(right_idx);
            let powed = IRNode::call(
                IRNode::PropertyAccess {
                    object: Box::new(IRNode::id("Math")),
                    property: "pow".into(),
                },
                vec![get_ir, rhs],
            );
            return self.private_field_set_ir(receiver_idx, &clean, powed);
        }

        let short_op = if operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16 {
            "&&"
        } else if operator_token == SyntaxKind::BarBarEqualsToken as u16 {
            "||"
        } else {
            return None;
        };
        // Short-circuit assignment may only be folded into an unconditional set
        // for a plain field; accessors keep their conditional-write semantics.
        if self.private_write_info(&clean).map(|(_, kind, _)| kind) != Some("f") {
            return None;
        }
        let get_ir = self.private_field_get_ir(receiver_idx, &clean)?;
        let mut rhs = self.convert_expression(right_idx);
        if self
            .arena
            .get(right_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
        {
            rhs = IRNode::Parenthesized(Box::new(rhs));
        }
        let folded = IRNode::binary(get_ir, short_op, rhs);
        self.private_field_set_ir(receiver_idx, &clean, folded)
    }

    /// Lower `recv.#x++` / `recv.#x--` for a known private field/accessor slot.
    ///
    /// `is_statement` selects tsc's two forms (`_a` = value temp, `_b` = old
    /// value temp, allocated old-value-first to match tsc's `var _a, _b` order):
    /// - statement (result discarded):
    ///   `__classPrivateFieldSet(this, _C_x, (_a = get, _a++, _a), "f")`
    /// - value position:
    ///   `(__classPrivateFieldSet(this, _C_x, (_b = get, _a = _b++, _b), "f"), _a)`
    fn private_postfix_mutation_ir(
        &self,
        receiver_idx: NodeIndex,
        clean_name: &str,
        operator: u16,
        is_statement: bool,
    ) -> Option<IRNode> {
        let op: std::borrow::Cow<'static, str> = if operator == SyntaxKind::PlusPlusToken as u16 {
            "++".into()
        } else if operator == SyntaxKind::MinusMinusToken as u16 {
            "--".into()
        } else {
            return None;
        };
        // Confirm the write slot before allocating any hoisted temp so a missing
        // setter cannot leak a `var` declaration through the fallthrough path.
        self.private_write_info(clean_name)?;

        if is_statement {
            let temp = self.generate_hoisted_temp();
            let get_ir = self.private_field_get_ir(receiver_idx, clean_name)?;
            let inner = IRNode::CommaExpr(vec![
                IRNode::assign(IRNode::id(temp.clone()), get_ir),
                IRNode::PostfixUnaryExpr {
                    operand: Box::new(IRNode::id(temp.clone())),
                    operator: op,
                },
                IRNode::id(temp),
            ]);
            self.private_field_set_ir(receiver_idx, clean_name, inner)
        } else {
            // old-value temp first (`_a`), then value temp (`_b`).
            let old_val = self.generate_hoisted_temp();
            let val = self.generate_hoisted_temp();
            let get_ir = self.private_field_get_ir(receiver_idx, clean_name)?;
            let inner = IRNode::CommaExpr(vec![
                IRNode::assign(IRNode::id(val.clone()), get_ir),
                IRNode::assign(
                    IRNode::id(old_val.clone()),
                    IRNode::PostfixUnaryExpr {
                        operand: Box::new(IRNode::id(val.clone())),
                        operator: op,
                    },
                ),
                IRNode::id(val),
            ]);
            let set_ir = self.private_field_set_ir(receiver_idx, clean_name, inner)?;
            Some(IRNode::CommaExpr(vec![set_ir, IRNode::id(old_val)]))
        }
    }

    /// When `expr_idx` is `recv.#x++` / `recv.#x--` in statement (result-
    /// discarded) position, build tsc's leaner statement form. `None` leaves the
    /// expression to the generic statement conversion.
    pub(super) fn try_private_postfix_statement(&self, expr_idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::POSTFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.arena.get_unary_expr(node)?;
        let (receiver_idx, clean) = self.private_mutation_target(unary.operand)?;
        self.private_postfix_mutation_ir(receiver_idx, &clean, unary.operator, true)
    }

    pub(super) fn convert_binary_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(bin) = self.arena.get_binary_expr(node) {
            // Private field write: `this.#x = value` → `__classPrivateFieldSet(this, _C_x, value, "f")`
            if bin.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16 {
                if let Some(lhs_node) = self.arena.get(bin.left)
                    && lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(lhs_access) = self.arena.get_access_expr(lhs_node)
                    && let Some(name_node) = self.arena.get(lhs_access.name_or_argument)
                    && name_node.kind == SyntaxKind::PrivateIdentifier as u16
                {
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        let raw = &ident.escaped_text;
                        let clean = raw.strip_prefix('#').unwrap_or(raw.as_str());
                        if let Some((brand_var, kind, member_ref)) = self.private_write_info(clean)
                        {
                            let receiver = self.convert_expression(lhs_access.expression);
                            let value = self.convert_expression(bin.right);
                            let mut set_args = vec![
                                receiver,
                                IRNode::id(brand_var),
                                value,
                                IRNode::StringLiteral(kind.into()),
                            ];
                            if let Some(member_ref) = member_ref {
                                set_args.push(IRNode::id(member_ref));
                            }
                            return IRNode::call(
                                IRNode::RuntimeHelper(std::borrow::Cow::Borrowed(
                                    "__classPrivateFieldSet",
                                )),
                                set_args,
                            );
                        }
                    }
                }
            }

            // Private field compound assignment: `this.#x += v` →
            // `__classPrivateFieldSet(this, _C_x, __classPrivateFieldGet(this, _C_x, "f") + v, "f")`.
            // Only the simple-base-operator compounds are lowered here; `**=`
            // (needs `Math.pow`) and logical (`&&= ||= ??=`) assignments stay on
            // the fallthrough as documented follow-ups.
            if let Some(base_op) = Self::es5_private_compound_base_op(bin.operator_token)
                && let Some((receiver_idx, clean)) = self.private_mutation_target(bin.left)
                && let Some(get_ir) = self.private_field_get_ir(receiver_idx, &clean)
            {
                let rhs = self.convert_expression(bin.right);
                let new_value = IRNode::binary(get_ir, base_op, rhs);
                if let Some(set_ir) = self.private_field_set_ir(receiver_idx, &clean, new_value) {
                    return set_ir;
                }
            }

            // Private field exponent (`**=`) and short-circuit (`&&=`/`||=`)
            // compound assignment. See `private_exp_or_logical_compound_ir`.
            if let Some(lowered) =
                self.private_exp_or_logical_compound_ir(bin.operator_token, bin.left, bin.right)
            {
                return lowered;
            }

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
            // Private field prefix mutation: `++this.#x` →
            // `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), ++_a), "f")`.
            // tsc uses one form for both statement and value position because a
            // prefix mutation already evaluates to the new value.
            if (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
                && let Some((receiver_idx, clean)) = self.private_mutation_target(unary.operand)
                && self.private_write_info(&clean).is_some()
            {
                let temp = self.generate_hoisted_temp();
                let op = self.get_prefix_operator(unary.operator);
                if let Some(get_ir) = self.private_field_get_ir(receiver_idx, &clean) {
                    let bumped = IRNode::CommaExpr(vec![
                        IRNode::assign(IRNode::id(temp.clone()), get_ir),
                        IRNode::PrefixUnaryExpr {
                            operator: op.into(),
                            operand: Box::new(IRNode::id(temp)),
                        },
                    ]);
                    if let Some(set_ir) = self.private_field_set_ir(receiver_idx, &clean, bumped) {
                        return set_ir;
                    }
                }
            }

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
            // Private field postfix mutation in value position: `f(this.#x++)` →
            // `(__classPrivateFieldSet(this, _C_x, (_b = get, _a = _b++, _b), "f"), _a)`.
            // Statement position (`this.#x++;`) uses the leaner form routed via
            // `try_private_postfix_statement` from `convert_expression_statement`.
            if let Some((receiver_idx, clean)) = self.private_mutation_target(unary.operand)
                && let Some(lowered) =
                    self.private_postfix_mutation_ir(receiver_idx, &clean, unary.operator, false)
            {
                return lowered;
            }

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
            let expression =
                IRNode::Parenthesized(Box::new(self.convert_expression(paren.expression)));
            if let Some(comment) = self
                .leading_block_comment_before_node(node)
                .or_else(|| self.leading_block_comments_before_expression(node, paren.expression))
            {
                IRNode::LeadingCommentExpr {
                    comment: comment.into(),
                    expression: Box::new(expression),
                }
            } else {
                expression
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_type_assertion(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // Both TYPE_ASSERTION and AS_EXPRESSION use TypeAssertionData
        if let Some(assertion) = self.arena.get_type_assertion(node) {
            let expression = self.convert_expression(assertion.expression);
            if let Some(comment) = self.leading_block_comment_before_node(node).or_else(|| {
                self.leading_block_comments_before_expression(node, assertion.expression)
            }) {
                IRNode::LeadingCommentExpr {
                    comment: comment.into(),
                    expression: Box::new(expression),
                }
            } else {
                expression
            }
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

    // Structural rule: at ES5 a `recv.#name(args)` call where `#name` is a
    // private member with a read slot (method, function-valued field, or
    // getter) lowers to `__classPrivateFieldGet(recv, brand, kind[, fn])
    // .call(recv, args)`. The brand for an instance method/getter is the
    // class's `_instances` WeakSet (never the function var), and the call is
    // routed through `.call` so the receiver is preserved as `this`. Without
    // this the private method read had no brand entry and emitted the invalid
    // bare callee `recv.()`. The rule keys on the member's read slot and kind,
    // not on its spelling — these tests vary class/member/binder names.

    #[test]
    fn instance_private_method_call_uses_instances_brand_and_call() {
        let output = emit_es5(
            "class Counter {\n    #step(n: number) { return n; }\n    bump() { return this.#step(2); }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Counter_instances, \"m\", _Counter_step).call(this, 2)"
            ),
            "Instance `this.#step(2)` must read through the `_instances` brand and invoke via `.call`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("this.()") && !output.contains(".()"),
            "Lowered output must not contain the invalid bare private callee.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_private_method_reference_without_call_reads_function_value() {
        // A private method read in value position (not called) lowers to the
        // bare 4-arg get with no `.call`.
        let output = emit_es5(
            "class Registry {\n    #lookup() { return 1; }\n    handle() { const f = this.#lookup; return f; }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Registry_instances, \"m\", _Registry_lookup)"
            ),
            "A private-method reference must read the 4-arg function value.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("_Registry_lookup).call"),
            "A bare reference must not synthesize a `.call`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_private_getter_in_call_position_preserves_receiver() {
        // Distinct binder names (anti-hardcoding): the call lowering keys on the
        // read slot, not the member spelling.
        let output = emit_es5(
            "class Service {\n    get #handler() { return () => 1; }\n    run() { return this.#handler(); }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Service_instances, \"a\", _Service_handler_get).call(this)"
            ),
            "A private getter invoked in call position must brand against `_instances` and `.call`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_private_method_call_captures_side_effecting_receiver_once() {
        // The receiver is referenced twice (read + `.call` this), so a
        // side-effecting receiver must be captured into a single hoisted temp.
        let output = emit_es5(
            "class Node {\n    #weight() { return 1; }\n    total(make: () => Node) { return make().#weight(); }\n}\n",
        );
        assert!(
            output.contains("(_a = make())")
                && output.contains(
                    "__classPrivateFieldGet((_a = make()), _Node_instances, \"m\", _Node_weight).call(_a)"
                ),
            "A side-effecting receiver must be captured once and reused by the `.call`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_private_accessor_read_uses_instances_brand_and_getter_ref() {
        let output = emit_es5(
            "class Cell {\n    get #value() { return 7; }\n    peek() { return this.#value; }\n}\n",
        );
        assert!(
            output
                .contains("__classPrivateFieldGet(this, _Cell_instances, \"a\", _Cell_value_get)"),
            "An instance accessor read must brand against `_instances` and pass the getter ref.\nOutput:\n{output}"
        );
    }

    #[test]
    fn instance_private_accessor_write_uses_instances_brand_and_setter_ref() {
        let output = emit_es5(
            "class Slot {\n    set #value(v: number) {}\n    fill() { this.#value = 9; }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Slot_instances, 9, \"a\", _Slot_value_set)"
            ),
            "An instance accessor write must brand against `_instances` and pass the setter ref.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_function_call_routes_through_call_to_preserve_this() {
        // A private field holding a function is still read with kind "f", but a
        // call must use `.call(this)` like tsc (preserving the receiver).
        let output =
            emit_es5("class Box {\n    #run = () => 1;\n    go() { return this.#run(); }\n}\n");
        assert!(
            output.contains("__classPrivateFieldGet(this, _Box_run, \"f\").call(this)"),
            "A private field-function call must route through `.call(this)`.\nOutput:\n{output}"
        );
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
    fn accessor_return_preserves_jsdoc_type_cast_comment() {
        let output =
            emit_es5("class Casts {\n    get value() { return /** @type {*} */(null); }\n}\n");
        assert!(
            output.contains("return /** @type {*} */ (null);"),
            "ES5 class IR must preserve erased JSDoc type-cast comments.\nOutput:\n{output}"
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

    // Structural rule: when the ES5 class-IR converter lowers a read-modify-write
    // on a private field/accessor (`this.#x op= v`, `++this.#x`, `this.#x++`), it
    // must route the read through `__classPrivateFieldGet` and the write through
    // `__classPrivateFieldSet` rather than emitting an un-assignable
    // `__classPrivateFieldGet(...) op= v`. The rule keys on the member being a
    // `PrivateIdentifier` with a known storage slot — so these tests vary class,
    // member, operator, and member-kind (field vs accessor).

    #[test]
    fn private_field_compound_add_lowers_to_get_op_set() {
        let output = emit_es5(
            "class Acc {\n    #count = 0;\n    bump() {\n        this.#count += 2;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Acc_count, __classPrivateFieldGet(this, _Acc_count, \"f\") + 2, \"f\")"
            ),
            "Private `#count += 2` must lower to get-op-set.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("\"f\") += "),
            "Must not emit an un-assignable `get(...) += v`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_compound_bitor_uses_base_operator() {
        // Different class/member/operator: `|=` lowers with base `|`.
        let output = emit_es5(
            "class Flags {\n    #mask = 0;\n    set(b: number) {\n        this.#mask |= b;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Flags_mask, __classPrivateFieldGet(this, _Flags_mask, \"f\") | b, \"f\")"
            ),
            "Private `#mask |= b` must lower to get-`|`-set.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_prefix_increment_uses_single_form() {
        let output =
            emit_es5("class Pre {\n    #n = 0;\n    up() {\n        ++this.#n;\n    }\n}\n");
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Pre_n, (_a = __classPrivateFieldGet(this, _Pre_n, \"f\"), ++_a), \"f\")"
            ),
            "Prefix `++this.#n` must use the new-value comma form.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _a;"),
            "Prefix mutation must hoist its temp.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_postfix_decrement_statement_uses_lean_form() {
        // Statement position discards the result → no old-value temp.
        let output =
            emit_es5("class Pst {\n    #v = 5;\n    step() {\n        this.#v--;\n    }\n}\n");
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Pst_v, (_a = __classPrivateFieldGet(this, _Pst_v, \"f\"), _a--, _a), \"f\")"
            ),
            "Statement `this.#v--` must use the lean single-temp form.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("var _a, _b"),
            "Statement postfix must not allocate an old-value temp.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_postfix_increment_value_keeps_old_value() {
        // Value position (`return ...`) must yield the pre-mutation value.
        let output = emit_es5(
            "class Val {\n    #w = 0;\n    take() {\n        return this.#w++;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "return (__classPrivateFieldSet(this, _Val_w, (_b = __classPrivateFieldGet(this, _Val_w, \"f\"), _a = _b++, _b), \"f\"), _a)"
            ),
            "Value `return this.#w++` must return the old value via the two-temp form.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _a, _b;"),
            "Value postfix must hoist both temps in tsc order (`_a`, `_b`).\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_accessor_compound_uses_instances_brand_and_get_set_refs() {
        // An instance accessor brands against `_Box_instances` and threads the
        // getter as the trailing read argument and the setter as the trailing
        // write argument (tsc's 4-arg get / 5-arg set forms).
        let output = emit_es5(
            "class Box {\n    get #val() { return 1; }\n    set #val(v: number) {}\n    add() {\n        this.#val += 3;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Box_instances, __classPrivateFieldGet(this, _Box_instances, \"a\", _Box_val_get) + 3, \"a\", _Box_val_set)"
            ),
            "Accessor `#val += 3` must brand against `_Box_instances` and pass the getter/setter refs.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_plain_assignment_still_lowers() {
        // Regression guard: the plain `=` write path (from #12180) is unchanged.
        let output =
            emit_es5("class Plain {\n    #p = 0;\n    reset() {\n        this.#p = 9;\n    }\n}\n");
        assert!(
            output.contains("__classPrivateFieldSet(this, _Plain_p, 9, \"f\")"),
            "Plain `this.#p = 9` must still lower to a single set.\nOutput:\n{output}"
        );
    }

    // Structural rule: the ES5 class-IR converter must lower private-field
    // exponent (`**=`) and short-circuit (`&&=`/`||=`) compound assignment to a
    // get-fold-set form instead of an un-assignable `__classPrivateFieldGet(...)
    // **= v`. `**=` is an unconditional write (fields + accessors); `&&=`/`||=`
    // fold to an always-write set only for plain fields, where it is observably
    // equivalent. `??=` and accessor short-circuit stay on the fallthrough. The
    // rule keys on the member's storage slot and kind, never on its spelling.

    #[test]
    fn private_field_exponent_assign_lowers_through_math_pow() {
        let output =
            emit_es5("class E {\n    #x = 2;\n    m() {\n        this.#x **= 3;\n    }\n}\n");
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _E_x, Math.pow(__classPrivateFieldGet(this, _E_x, \"f\"), 3), \"f\")"
            ),
            "Private `#x **= 3` must lower through `Math.pow`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("**="),
            "ES5 output must not retain the `**=` operator.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_accessor_exponent_assign_threads_get_and_set_storage() {
        // `**=` is unconditional, so it is also correct for accessors: read
        // through the getter and write through the setter, both branded against
        // the `_instances` `WeakSet` with kind "a".
        let output = emit_es5(
            "class Box {\n    get #v() { return 2; }\n    set #v(x: number) {}\n    grow() {\n        this.#v **= 4;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Box_instances, Math.pow(__classPrivateFieldGet(this, _Box_instances, \"a\", _Box_v_get), 4), \"a\", _Box_v_set)"
            ),
            "Accessor `#v **= 4` must brand against `_Box_instances` and thread the getter/setter refs.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_logical_and_assign_folds_to_set_get_and_rhs() {
        let output = emit_es5(
            "class A {\n    #flag = true;\n    m() {\n        this.#flag &&= false;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _A_flag, __classPrivateFieldGet(this, _A_flag, \"f\") && false, \"f\")"
            ),
            "Private `#flag &&= false` must fold to set(get() && rhs).\nOutput:\n{output}"
        );
        assert!(
            !output.contains("&&="),
            "Lowered output must not retain `&&=`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_logical_or_assign_folds_to_set_get_or_rhs() {
        let output = emit_es5(
            "class O {\n    #cache = 0;\n    m(v: number) {\n        this.#cache ||= v;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _O_cache, __classPrivateFieldGet(this, _O_cache, \"f\") || v, \"f\")"
            ),
            "Private `#cache ||= v` must fold to set(get() || rhs).\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_logical_assign_parenthesizes_conditional_rhs() {
        // `||` binds tighter than `?:`, so a conditional rhs must be parenthesized
        // or the assignment silently reparses as `(get() || a) ? b : c`.
        let output = emit_es5(
            "declare const a: any;\nclass C {\n    #x = 0;\n    m() {\n        this.#x ||= a ? 1 : 2;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _C_x, __classPrivateFieldGet(this, _C_x, \"f\") || (a ? 1 : 2), \"f\")"
            ),
            "Conditional rhs of `||=` must be parenthesized.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_field_nullish_assign_stays_on_fallthrough() {
        // Out of scope: `??=` needs ES5 nullish lowering of the folded `??`.
        // This guards the documented scope boundary (no accidental partial fold).
        let output =
            emit_es5("class N {\n    #x = 0;\n    m() {\n        this.#x ??= 9;\n    }\n}\n");
        assert!(
            !output.contains("Math.pow") && !output.contains("\"f\") ?? "),
            "`??=` must not be partially folded; it stays on the fallthrough.\nOutput:\n{output}"
        );
    }

    #[test]
    fn private_accessor_short_circuit_assign_stays_on_fallthrough() {
        // Out of scope: an accessor `&&=` would call the setter even when the
        // short circuit says skip, so the always-write fold is unsafe here.
        let output = emit_es5(
            "class Acc {\n    get #v() { return 1; }\n    set #v(x: number) {}\n    m() {\n        this.#v &&= 3;\n    }\n}\n",
        );
        assert!(
            !output.contains("__classPrivateFieldSet(this, _Acc_v_set, __classPrivateFieldGet"),
            "Accessor `&&=` must not be folded to an always-write set.\nOutput:\n{output}"
        );
    }
}
