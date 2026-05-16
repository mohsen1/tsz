//! ES5 class member emission — prototype methods, static members, and accessors.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.
//! Contains `emit_all_members_ir` and related helpers.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRMethodName, IRNode, IRParam, IRPropertyDescriptor};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{contains_async_arrow_function, is_private_identifier};
use tsz_scanner::SyntaxKind;

use super::{
    ES5ClassTransformer, PropertyNameIR, collect_accessor_pairs, get_identifier_text,
    has_effective_static_modifier,
};

impl<'a> ES5ClassTransformer<'a> {
    fn method_has_async_generator_asterisk(
        &self,
        member_idx: NodeIndex,
        method_body: NodeIndex,
        asterisk_token: bool,
    ) -> bool {
        asterisk_token
            || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                self.source_text,
                self.arena.get(member_idx).map_or(0, |node| node.pos),
                self.arena.get(method_body).map_or_else(
                    || self.arena.get(member_idx).map_or(0, |node| node.end),
                    |body| body.pos,
                ),
            )
    }

    fn async_generator_params_need_forwarding(&self, params: &[NodeIndex]) -> bool {
        params.iter().copied().any(|param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };
            if param.initializer.is_some() {
                return true;
            }
            self.arena
                .get(param.name)
                .is_some_and(|name_node| name_node.kind != SyntaxKind::Identifier as u16)
        })
    }

    fn async_generator_outer_params(
        &self,
        ast_params: &[NodeIndex],
        ir_params: &[IRParam],
    ) -> Vec<IRParam> {
        if !self.async_generator_params_need_forwarding(ast_params) {
            return ir_params.to_vec();
        }

        ir_params
            .iter()
            .map(|param| {
                if param.name.starts_with('_') {
                    IRParam::new(param.name.to_string())
                } else {
                    IRParam::new(format!("{}_1", param.name))
                }
            })
            .collect()
    }

    fn async_generator_method_body(
        &self,
        method_name_idx: NodeIndex,
        params: &[NodeIndex],
        body: NodeIndex,
    ) -> Vec<IRNode> {
        let move_params_to_generator = self.async_generator_params_need_forwarding(params);
        let method_name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, method_name_idx);
        let inner_name = (!method_name.is_empty()).then(|| format!("{method_name}_1"));
        let mut transformer = AsyncES5Transformer::new(self.arena);
        if let Some(source_text) = self.source_text {
            transformer.set_source_text(source_text);
        }
        self.configure_async_disposable_context(&mut transformer);
        let inner = transformer.transform_async_generator_inner_function(
            inner_name,
            params,
            body,
            move_params_to_generator,
        );
        self.sync_async_disposable_context(&mut transformer);
        vec![IRNode::ReturnStatement(Some(Box::new(IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__asyncGenerator".into())),
            arguments: vec![
                IRNode::This { captured: false },
                IRNode::id("arguments"),
                inner,
            ],
        })))]
    }

    /// Build a getter function IR from an accessor node
    fn build_getter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_getter_function_ir_impl(accessor_idx, false)
    }

    fn build_getter_function_ir_static(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_getter_function_ir_impl(accessor_idx, true)
    }

    fn build_getter_function_ir_impl(
        &self,
        accessor_idx: NodeIndex,
        is_static: bool,
    ) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let params = self.extract_parameters(&accessor_data.parameters);

        let body_source_range = self.arena.pos_end_at(accessor_data.body);

        let body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = if is_static {
                self.convert_block_body_static(accessor_data.body)
            } else {
                self.convert_block_body(accessor_data.body)
            };

            // Check if getter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                // Insert `var _this = this;` at the start of getter body
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            body
        };

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Build a setter function IR from an accessor node
    fn build_setter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_setter_function_ir_impl(accessor_idx, false)
    }

    fn build_setter_function_ir_static(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_setter_function_ir_impl(accessor_idx, true)
    }

    fn build_setter_function_ir_impl(
        &self,
        accessor_idx: NodeIndex,
        is_static: bool,
    ) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let mut params = self.extract_parameters(&accessor_data.parameters);

        // Generate destructuring prologue for binding-pattern parameters
        let accessor_destructuring =
            self.generate_destructuring_prologue(&accessor_data.parameters, &params);

        let body_source_range = if accessor_destructuring.is_empty() {
            self.arena.pos_end_at(accessor_data.body)
        } else {
            None // Force multi-line when destructuring prologue exists
        };

        let mut body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = if is_static {
                self.convert_block_body_static(accessor_data.body)
            } else {
                self.convert_block_body(accessor_data.body)
            };

            // Check if setter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            // Prepend destructuring prologue
            if !accessor_destructuring.is_empty() {
                let mut full = accessor_destructuring;
                full.append(&mut body);
                body = full;
            }

            body
        };

        self.lower_rest_parameter_for_es5(&mut params, &mut body);

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Lower a rest parameter into ES5 `arguments` collection statements.
    /// Example: `(...v)` -> `() { var v = []; for (var _i = 0; _i < arguments.length; _i++) { ... } }`
    fn lower_rest_parameter_for_es5(&self, params: &mut Vec<IRParam>, body: &mut Vec<IRNode>) {
        let Some(rest_index) = params.iter().position(|param| param.rest) else {
            return;
        };

        let rest_name = params[rest_index].name.clone();
        params.truncate(rest_index);

        let loop_var = "_i";
        let start_index = rest_index.to_string();

        let target_index = if rest_index == 0 {
            IRNode::id(loop_var)
        } else {
            IRNode::binary(
                IRNode::id(loop_var),
                "-",
                IRNode::number(start_index.clone()),
            )
        };

        let assignment = IRNode::expr_stmt(IRNode::assign(
            IRNode::elem(IRNode::id(rest_name.clone()), target_index),
            IRNode::elem(IRNode::id("arguments"), IRNode::id(loop_var)),
        ));

        let collect_rest = IRNode::ForStatement {
            initializer: Some(Box::new(IRNode::Raw(
                format!("var {loop_var} = {start_index}").into(),
            ))),
            condition: Some(Box::new(IRNode::binary(
                IRNode::id(loop_var),
                "<",
                IRNode::prop(IRNode::id("arguments"), "length"),
            ))),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(loop_var)),
                operator: "++".to_string().into(),
            })),
            body: Box::new(IRNode::block(vec![assignment])),
        };

        body.insert(0, collect_rest);
        body.insert(0, IRNode::var_decl(rest_name, Some(IRNode::empty_array())));
    }

    /// Get method name as IR representation.
    /// Computed property names use static-like super access (`_super.X` not `_super.prototype.X`)
    /// because they are evaluated at class definition time in the IIFE body, not inside methods.
    pub(super) fn get_method_name_ir(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return IRMethodName::Identifier(String::new().into());
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return IRMethodName::Computed(Box::new(
                    self.convert_computed_property_expression(computed.expression, true),
                ));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return IRMethodName::Identifier(ident.escaped_text.clone().into());
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::StringLiteral(lit.text.clone().into());
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return IRMethodName::NumericLiteral(lit.text.clone().into());
        }

        IRMethodName::Identifier(String::new().into())
    }

    /// Emit all class members (prototype and static) in source order.
    /// This matches tsc's behavior of interleaving prototype and static members
    /// based on their order in the source code.
    /// Returns deferred static block IIFEs (for classes with no non-block static members).
    pub(super) fn emit_all_members_ir(
        &self,
        body: &mut Vec<IRNode>,
        class_idx: NodeIndex,
    ) -> Vec<IRNode> {
        let Some(class_node) = self.arena.get(class_idx) else {
            return Vec::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return Vec::new();
        };

        // --- Static member preamble ---

        // Check if class has non-block static members (properties, accessors, methods with bodies)
        // This determines whether static blocks go inline or deferred
        let has_static_props = class_data.members.nodes.iter().any(|&m_idx| {
            let Some(m_node) = self.arena.get(m_idx) else {
                return false;
            };
            if m_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop_data) = self.arena.get_property_decl(m_node) {
                    return self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                        && !is_private_identifier(self.arena, prop_data.name)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                        && self.property_initializer_has_equals(m_node, prop_data);
                }
            } else if (m_node.kind == syntax_kind_ext::GET_ACCESSOR
                || m_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(m_node)
            {
                return self
                    .arena
                    .has_modifier(&acc_data.modifiers, SyntaxKind::StaticKeyword)
                    && !(self
                        .arena
                        .has_modifier(&acc_data.modifiers, SyntaxKind::AbstractKeyword)
                        && acc_data.body.is_none())
                    && !is_private_identifier(self.arena, acc_data.name);
            }
            false
        });

        let class_alias = self.current_static_class_alias.clone();

        let mut deferred_static_blocks = Vec::new();

        // Collect accessor pairs for both instance and static
        let instance_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, false);
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        let mut emitted_instance_accessors: FxHashSet<String> = FxHashSet::default();
        let mut emitted_static_accessors: FxHashSet<String> = FxHashSet::default();

        // Collect deferred static property initializers.
        // tsc emits methods/accessors (both instance and static) in source order,
        // but defers static property initializer assignments to after all methods/accessors.
        let mut deferred_static_prop_inits: Vec<IRNode> = Vec::new();

        // Single pass: emit all members in source order
        for (member_i, &member_idx) in class_data.members.nodes.iter().enumerate() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                let is_static = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword);

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                if is_static {
                    // --- Static method ---
                    let has_async_modifier = self
                        .arena
                        .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword);
                    let has_generator_asterisk = self.method_has_async_generator_asterisk(
                        member_idx,
                        method_data.body,
                        method_data.asterisk_token,
                    );
                    let is_async = has_async_modifier && !has_generator_asterisk;
                    let is_async_generator = has_async_modifier && has_generator_asterisk;

                    let static_destructuring =
                        self.generate_destructuring_prologue(&method_data.parameters, &params);

                    let method_body = if is_async {
                        let mut async_transformer = AsyncES5Transformer::new(self.arena)
                            .with_class_super_context(
                                self.has_extends,
                                self.super_name.clone(),
                                true,
                            );
                        if let Some(source_text) = self.source_text {
                            async_transformer.set_source_text(source_text);
                        }
                        self.configure_async_disposable_context(&mut async_transformer);
                        let has_await = async_transformer.body_contains_await(method_data.body);
                        let mut generator_body =
                            async_transformer.transform_generator_body(method_data.body, has_await);
                        self.sync_async_disposable_context(&mut async_transformer);
                        let hoisted_var_groups =
                            AsyncES5Transformer::extract_and_remove_var_decl_groups(
                                &mut generator_body,
                            );
                        vec![IRNode::AwaiterCall {
                            this_arg: Box::new(IRNode::this()),
                            generator_body: Box::new(generator_body),
                            hoisted_var_groups,
                            promise_constructor: self
                                .async_method_promise_constructor(method_data.type_annotation),
                            multiline_callback: false,
                        }]
                    } else if is_async_generator {
                        self.async_generator_method_body(
                            method_data.name,
                            &method_data.parameters.nodes,
                            method_data.body,
                        )
                    } else {
                        let local_class_alias =
                            self.get_class_alias_for_static_method(method_data.body);
                        let mut mbody = self.convert_block_body_with_alias_static(
                            method_data.body,
                            local_class_alias,
                        );
                        if !static_destructuring.is_empty() {
                            let mut full = static_destructuring;
                            full.append(&mut mbody);
                            mbody = full;
                        }
                        mbody
                    };

                    let body_source_range = if is_async
                        || is_async_generator
                        || self.has_destructured_parameters(&method_data.parameters)
                    {
                        None
                    } else {
                        self.arena
                            .get(method_data.body)
                            .map(|body_node| (body_node.pos, body_node.end))
                    };

                    let leading_comment = self.extract_leading_comment(member_node);
                    let trailing_comment =
                        self.extract_trailing_comment_for_method(method_data.body);

                    let function = IRNode::FunctionExpr {
                        name: None,
                        parameters: if is_async_generator {
                            self.async_generator_outer_params(
                                &method_data.parameters.nodes,
                                &params,
                            )
                        } else {
                            params
                        },
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    };

                    if self.use_define_for_class_fields {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::id(self.class_name.clone())),
                            property_name: method_name,
                            descriptor: IRPropertyDescriptor {
                                get: None,
                                set: None,
                                value: Some(Box::new(function)),
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: true,
                                trailing_comment,
                            },
                            leading_comment,
                        });
                    } else {
                        body.push(IRNode::StaticMethod {
                            class_name: self.class_name.clone().into(),
                            method_name,
                            function: Box::new(function),
                            leading_comment,
                            trailing_comment,
                        });
                    }
                } else {
                    // --- Instance method ---
                    let destructuring_prologue =
                        self.generate_destructuring_prologue(&method_data.parameters, &params);

                    let has_async_modifier = self
                        .arena
                        .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword);
                    let has_generator_asterisk = self.method_has_async_generator_asterisk(
                        member_idx,
                        method_data.body,
                        method_data.asterisk_token,
                    );
                    let is_async = has_async_modifier && !has_generator_asterisk;
                    let is_async_generator = has_async_modifier && has_generator_asterisk;

                    let body_source_range = if is_async || is_async_generator {
                        None
                    } else if destructuring_prologue.is_empty() {
                        self.arena
                            .get(method_data.body)
                            .map(|body_node| (body_node.pos, body_node.end))
                    } else {
                        None
                    };

                    let method_body = if is_async {
                        let mut async_transformer = AsyncES5Transformer::new(self.arena)
                            .with_class_super_context(
                                self.has_extends,
                                self.super_name.clone(),
                                false,
                            );
                        if let Some(source_text) = self.source_text {
                            async_transformer.set_source_text(source_text);
                        }
                        self.configure_async_disposable_context(&mut async_transformer);
                        let has_await = async_transformer.body_contains_await(method_data.body);
                        let mut generator_body =
                            async_transformer.transform_generator_body(method_data.body, has_await);
                        self.sync_async_disposable_context(&mut async_transformer);
                        let hoisted_var_groups =
                            AsyncES5Transformer::extract_and_remove_var_decl_groups(
                                &mut generator_body,
                            );
                        vec![IRNode::AwaiterCall {
                            this_arg: Box::new(IRNode::this()),
                            generator_body: Box::new(generator_body),
                            hoisted_var_groups,
                            promise_constructor: self
                                .async_method_promise_constructor(method_data.type_annotation),
                            multiline_callback: false,
                        }]
                    } else if is_async_generator {
                        self.async_generator_method_body(
                            method_data.name,
                            &method_data.parameters.nodes,
                            method_data.body,
                        )
                    } else {
                        let mut method_body = self.convert_block_body(method_data.body);
                        if !destructuring_prologue.is_empty() {
                            let mut full_body = destructuring_prologue;
                            full_body.append(&mut method_body);
                            method_body = full_body;
                        }
                        let needs_this_capture =
                            self.constructor_needs_this_capture(method_data.body);
                        if needs_this_capture {
                            method_body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
                        }
                        method_body
                    };

                    let leading_comment = self.extract_leading_comment(member_node);
                    let trailing_comment =
                        self.extract_trailing_comment_for_method(method_data.body);

                    let function = IRNode::FunctionExpr {
                        name: None,
                        parameters: if is_async_generator {
                            self.async_generator_outer_params(
                                &method_data.parameters.nodes,
                                &params,
                            )
                        } else {
                            params
                        },
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    };

                    if self.use_define_for_class_fields {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: method_name,
                            descriptor: IRPropertyDescriptor {
                                get: None,
                                set: None,
                                value: Some(Box::new(function)),
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: true,
                                trailing_comment,
                            },
                            leading_comment,
                        });
                    } else {
                        body.push(IRNode::PrototypeMethod {
                            class_name: self.class_name.clone().into(),
                            method_name,
                            function: Box::new(function),
                            leading_comment,
                            trailing_comment,
                        });
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    let is_static =
                        has_effective_static_modifier(self.arena, &accessor_data.modifiers);
                    let is_abstract = self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword);
                    let is_private = is_private_identifier(self.arena, accessor_data.name);

                    if (is_abstract && accessor_data.body.is_none()) || is_private {
                        continue;
                    }

                    let accessor_name = match get_identifier_text(self.arena, accessor_data.name) {
                        Some(name) => name,
                        None => format!("__computed_{}", member_idx.0),
                    };

                    if is_static {
                        // --- Static accessor ---
                        if emitted_static_accessors.contains(&accessor_name) {
                            continue;
                        }

                        if let Some(&(getter_idx, setter_idx)) =
                            static_accessor_map.get(&accessor_name)
                        {
                            let get_fn = if let Some(getter_idx) = getter_idx {
                                self.build_getter_function_ir_static(getter_idx)
                            } else {
                                None
                            };
                            let set_fn = if let Some(setter_idx) = setter_idx {
                                self.build_setter_function_ir_static(setter_idx)
                            } else {
                                None
                            };
                            body.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::id(self.class_name.clone())),
                                property_name: self.get_method_name_ir(accessor_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: get_fn.map(Box::new),
                                    set: set_fn.map(Box::new),
                                    value: None,
                                    get_leading_comment: getter_idx
                                        .and_then(|idx| self.arena.get(idx))
                                        .and_then(|node| self.extract_leading_comment(node)),
                                    set_leading_comment: setter_idx
                                        .and_then(|idx| self.arena.get(idx))
                                        .and_then(|node| self.extract_leading_comment(node)),
                                    enumerable: false,
                                    configurable: true,
                                    writable: false,
                                    trailing_comment: None,
                                },
                                leading_comment: None,
                            });
                            emitted_static_accessors.insert(accessor_name);
                        }
                    } else {
                        // --- Instance accessor ---
                        if emitted_instance_accessors.contains(&accessor_name) {
                            continue;
                        }

                        if let Some(&(getter_idx, setter_idx)) =
                            instance_accessor_map.get(&accessor_name)
                        {
                            let get_fn = if let Some(getter_idx) = getter_idx {
                                self.build_getter_function_ir(getter_idx)
                            } else {
                                None
                            };
                            let set_fn = if let Some(setter_idx) = setter_idx {
                                self.build_setter_function_ir(setter_idx)
                            } else {
                                None
                            };
                            body.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::prop(
                                    IRNode::id(self.class_name.clone()),
                                    "prototype",
                                )),
                                property_name: self.get_method_name_ir(accessor_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: get_fn.map(Box::new),
                                    set: set_fn.map(Box::new),
                                    value: None,
                                    get_leading_comment: getter_idx
                                        .and_then(|idx| self.arena.get(idx))
                                        .and_then(|node| self.extract_leading_comment(node)),
                                    set_leading_comment: setter_idx
                                        .and_then(|idx| self.arena.get(idx))
                                        .and_then(|node| self.extract_leading_comment(node)),
                                    enumerable: false,
                                    configurable: true,
                                    writable: false,
                                    trailing_comment: None,
                                },
                                leading_comment: None,
                            });

                            let has_explicit_semicolon_member = class_data
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .is_some_and(|n| {
                                    n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                                });
                            if !has_explicit_semicolon_member {
                                let accessor_end = [getter_idx, setter_idx]
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|idx| self.arena.get(idx))
                                    .map(|n| n.end)
                                    .max()
                                    .unwrap_or(member_node.end);
                                let next_pos = class_data
                                    .members
                                    .nodes
                                    .get(member_i + 1)
                                    .and_then(|&idx| self.arena.get(idx))
                                    .map_or(member_node.end, |n| n.pos);
                                if self.source_has_semicolon_between(accessor_end, next_pos) {
                                    body.push(IRNode::EmptyStatement);
                                }
                            }
                            if self.source_text.is_some_and(|text| {
                                let start = std::cmp::min(member_node.pos as usize, text.len());
                                let end = std::cmp::min(member_node.end as usize, text.len());
                                start < end && text[start..end].trim_end().ends_with(';')
                            }) {
                                body.push(IRNode::EmptyStatement);
                            }

                            emitted_instance_accessors.insert(accessor_name);
                        }
                    }
                }
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                let is_static = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword);
                let is_abstract = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword);
                let is_declare = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword);
                let is_private_field = is_private_identifier(self.arena, prop_data.name);
                let is_accessor_keyword = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword);

                if is_static {
                    // --- Static property ---
                    if self.skip_static_field_initializers {
                        continue;
                    }
                    if is_accessor_keyword {
                        let Some(accessor) = self.find_auto_accessor(member_idx) else {
                            continue;
                        };
                        if is_abstract || is_declare || is_private_field {
                            continue;
                        }
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::id(self.class_name.clone())),
                            property_name: self.auto_accessor_setter_property_name(prop_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: Some(Box::new(
                                    self.build_static_auto_accessor_getter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                set: Some(Box::new(
                                    self.build_static_auto_accessor_setter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                value: None,
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: self
                                    .extract_trailing_comment_for_node(member_node),
                            },
                            leading_comment: self.extract_leading_comment(member_node),
                        });
                        continue;
                    }
                    if is_abstract || is_declare || is_private_field || is_accessor_keyword {
                        continue;
                    }
                    if !self.property_initializer_has_equals(member_node, prop_data) {
                        continue;
                    }
                    // Defer static property initializers to after all methods/accessors.
                    // tsc emits methods/accessors in source order first, then static
                    // property initializer assignments.

                    if let Some(prop_name) = self.get_property_name_ir(prop_data.name) {
                        let target = match &prop_name {
                            PropertyNameIR::Identifier(n) => {
                                IRNode::prop(IRNode::id(self.class_name.clone()), n.clone())
                            }
                            PropertyNameIR::StringLiteral(s) => IRNode::elem(
                                IRNode::id(self.class_name.clone()),
                                IRNode::string(s.clone()),
                            ),
                            PropertyNameIR::NumericLiteral(n) => IRNode::elem(
                                IRNode::id(self.class_name.clone()),
                                IRNode::number(n.clone()),
                            ),
                            PropertyNameIR::Computed(expr_idx) => {
                                // Use hoisted temp if available
                                if let Some(temp) = self.computed_prop_temp_map.get(expr_idx) {
                                    IRNode::elem(
                                        IRNode::id(self.class_name.clone()),
                                        IRNode::id(temp.clone()),
                                    )
                                } else {
                                    IRNode::elem(
                                        IRNode::id(self.class_name.clone()),
                                        self.convert_computed_property_expression(*expr_idx, true),
                                    )
                                }
                            }
                        };
                        let value = if !self.class_decorators.is_empty() {
                            if let Some(alias) = self.class_self_reference_alias.as_ref() {
                                self.convert_expression_static_with_decorator_self_alias(
                                    prop_data.initializer,
                                    alias,
                                )
                            } else {
                                self.convert_expression_static_with_raw_this_substitution(
                                    prop_data.initializer,
                                    "(void 0)",
                                )
                            }
                        } else if let Some(ref alias) = class_alias {
                            self.convert_expression_static_with_class_alias(
                                prop_data.initializer,
                                alias,
                            )
                        } else {
                            self.convert_expression_static(prop_data.initializer)
                        };
                        if self.use_define_for_class_fields {
                            deferred_static_prop_inits.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::id(self.class_name.clone())),
                                property_name: self.get_method_name_ir(prop_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: None,
                                    set: None,
                                    value: Some(Box::new(value)),
                                    get_leading_comment: None,
                                    set_leading_comment: None,
                                    enumerable: true,
                                    configurable: true,
                                    writable: true,
                                    trailing_comment: None,
                                },
                                leading_comment: self.extract_leading_comment(member_node),
                            });
                        } else {
                            if self
                                .expression_contains_static_class_expression(prop_data.initializer)
                            {
                                deferred_static_prop_inits.push(IRNode::VarDecl {
                                    name: self.generate_temp_name().into(),
                                    initializer: None,
                                });
                            }
                            deferred_static_prop_inits
                                .push(IRNode::expr_stmt(IRNode::assign(target, value)));
                        }
                    }
                } else {
                    // --- Instance auto-accessor property ---
                    let Some(accessor) = self.find_auto_accessor(member_idx) else {
                        continue;
                    };
                    if is_abstract || is_private_field {
                        continue;
                    }
                    let storage_inits =
                        self.auto_accessor_instance_storage_inits_for_computed_key(member_idx);
                    let leading_comment = self.extract_leading_comment(member_node);
                    if storage_inits.is_empty() {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: self.auto_accessor_setter_property_name(prop_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: Some(Box::new(
                                    self.build_auto_accessor_getter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                set: Some(Box::new(
                                    self.build_auto_accessor_setter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                value: None,
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: self
                                    .extract_trailing_comment_for_node(member_node),
                            },
                            leading_comment,
                        });
                    } else {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: self
                                .auto_accessor_getter_property_name(prop_data.name, &storage_inits),
                            descriptor: IRPropertyDescriptor {
                                get: Some(Box::new(
                                    self.build_auto_accessor_getter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                set: None,
                                value: None,
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: None,
                            },
                            leading_comment,
                        });
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: self.auto_accessor_setter_property_name(prop_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: None,
                                set: Some(Box::new(
                                    self.build_auto_accessor_setter_function(
                                        &accessor.weakmap_name,
                                    ),
                                )),
                                value: None,
                                get_leading_comment: None,
                                set_leading_comment: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: self
                                    .extract_trailing_comment_for_node(member_node),
                            },
                            leading_comment: None,
                        });
                    }
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // --- Static block ---
                if self.skip_static_field_initializers {
                    continue;
                }
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| {
                            if let Some(ref alias) = class_alias {
                                self.convert_statement_static_with_class_alias(stmt_idx, alias)
                            } else {
                                self.convert_statement_static(stmt_idx)
                            }
                        })
                        .collect();

                    let iife = IRNode::StaticBlockIIFE { statements };
                    if has_static_props {
                        // Defer static blocks to after methods/accessors,
                        // interleaved with static property inits in source order
                        deferred_static_prop_inits.push(iife);
                    } else {
                        deferred_static_blocks.push(iife);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT {
                body.push(IRNode::EmptyStatement);
            }
        }

        // Emit deferred static property initializers and static blocks after
        // all methods/accessors, matching tsc's ES5 class member ordering.
        if !deferred_static_prop_inits.is_empty() {
            if let Some(alias) = self.class_self_reference_alias.as_ref()
                && !self.class_decorators.is_empty()
                && self.has_static_property_initializer(&class_data.members)
            {
                body.push(IRNode::VarDecl {
                    name: alias.clone().into(),
                    initializer: None,
                });
            }
            // Emit class alias preamble before the first static property init
            if let Some(ref alias) = class_alias {
                body.push(IRNode::VarDecl {
                    name: alias.clone().into(),
                    initializer: None,
                });
                body.push(IRNode::expr_stmt(IRNode::assign(
                    IRNode::id(alias.clone()),
                    IRNode::id(self.class_name.clone()),
                )));
            }
            body.append(&mut deferred_static_prop_inits);
        }

        deferred_static_blocks
    }

    pub(super) fn has_static_property_initializer(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                return false;
            };
            self.arena
                .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                && !is_private_identifier(self.arena, prop_data.name)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                && self.property_initializer_has_equals(member_node, prop_data)
        })
    }

    fn expression_contains_static_class_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class_data) = self.arena.get_class(node)
        {
            return self.has_static_property_initializer(&class_data.members);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.expression_contains_static_class_expression(paren.expression);
        }
        if (node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.expression_contains_static_class_expression(assertion.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.expression_contains_static_class_expression(unary.expression);
        }

        false
    }

    /// Check if any static property initializer or static block uses `this`.
    /// Returns true if a class alias is needed (i.e. `var _a; _a = ClassName;`).
    ///
    /// Note: `this` in static methods/getters/setters does NOT need aliasing because
    /// regular functions have their own `this` binding. Only static property initializer
    /// expressions and static block statement bodies need `this` → `_a` substitution.
    pub(super) fn static_members_need_class_alias(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        if !self.class_decorators.is_empty() {
            return false;
        }

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                // Only static properties with initializers
                if !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                    || self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }
                if !self.property_initializer_has_equals(member_node, prop_data) {
                    continue;
                }
                // Async arrows in static initializers also need the class alias:
                // tsc passes it to the downlevel `__generator` call as lexical `this`.
                if self.contains_static_value_this_reference(prop_data.initializer)
                    || contains_async_arrow_function(self.arena, prop_data.initializer)
                {
                    return true;
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Check if the static block body contains `this`
                if let Some(block_data) = self.arena.get_block(member_node) {
                    for &stmt_idx in &block_data.statements.nodes {
                        if self.contains_static_value_this_reference(stmt_idx)
                            || contains_async_arrow_function(self.arena, stmt_idx)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn contains_static_value_this_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }

        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            || node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            let Some(var_data) = self.arena.get_variable(node) else {
                return false;
            };
            return var_data
                .declarations
                .nodes
                .iter()
                .any(|&decl_idx| self.contains_static_value_this_reference(decl_idx));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = self.arena.get_variable_declaration(node) else {
                return false;
            };
            return decl.initializer.is_some()
                && self.contains_static_value_this_reference(decl.initializer);
        }

        self.arena
            .get_children(idx)
            .into_iter()
            .any(|child_idx| self.contains_static_value_this_reference(child_idx))
    }

    fn async_method_promise_constructor(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }

        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return Some(self.qualified_type_name_to_expr(type_ref.type_name));
        }

        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, type_ref.type_name);
        if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            && name != "Promise"
            && name != "PromiseLike"
            && !self.is_type_only_declaration_name(&name)
        {
            self.commonjs_import_substitutions
                .get(&name)
                .cloned()
                .or(Some(name))
        } else {
            None
        }
    }

    fn qualified_type_name_to_expr(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.get_qualified_name(node)
        {
            let left = self.qualified_type_name_to_expr(qn.left);
            let right =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, qn.right);
            return format!("{left}.{right}");
        }
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    fn is_type_only_declaration_name(&self, name: &str) -> bool {
        if self.has_value_declaration_name(name) {
            return false;
        }

        self.arena.nodes.iter().any(|node| {
            if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                self.arena.get_type_alias(node).is_some_and(|alias| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, alias.name)
                        == name
                })
            } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena.get_interface(node).is_some_and(|interface| {
                    crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        interface.name,
                    ) == name
                })
            } else {
                false
            }
        })
    }

    fn has_value_declaration_name(&self, name: &str) -> bool {
        self.arena.nodes.iter().any(|node| match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .arena
                .get_variable(node)
                .is_some_and(|var_stmt| self.variable_statement_declares_name(var_stmt, name)),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(node).is_some_and(|func| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, func.name)
                        == name
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(node).is_some_and(|class| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, class.name)
                        == name
                })
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|enum_decl| {
                    crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        enum_decl.name,
                    ) == name
                })
            }
            _ => false,
        })
    }

    fn variable_statement_declares_name(
        &self,
        var_stmt: &tsz_parser::parser::node::VariableData,
        name: &str,
    ) -> bool {
        var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };
            decl_list.declarations.nodes.iter().any(|&decl_idx| {
                self.arena
                    .get_variable_declaration_at(decl_idx)
                    .is_some_and(|decl| {
                        crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena, decl.name,
                        ) == name
                    })
            })
        })
    }
}
