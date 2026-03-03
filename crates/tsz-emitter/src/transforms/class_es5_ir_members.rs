//! ES5 class member emission — prototype methods, static members, and accessors.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.
//! Contains `emit_methods_ir`, `emit_static_members_ir`, and related helpers.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRMethodName, IRNode, IRParam, IRPropertyDescriptor};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

use super::{ES5ClassTransformer, PropertyNameIR, collect_accessor_pairs, get_identifier_text};

impl<'a> ES5ClassTransformer<'a> {
    /// Emit prototype methods as IR
    pub(super) fn emit_methods_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // First pass: collect instance accessors by name to combine getter/setter pairs
        let accessor_map = collect_accessor_pairs(self.arena, &class_data.members, false);

        // Track which accessor names we've emitted
        let mut emitted_accessors: FxHashSet<String> = FxHashSet::default();

        // Second pass: emit methods and accessors in source order
        for (member_i, &member_idx) in class_data.members.nodes.iter().enumerate() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Skip static methods
                if self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Check if async method (not generator)
                let is_async = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                    && !method_data.asterisk_token;

                // Capture body source range for single-line detection
                let body_source_range = self
                    .arena
                    .get(method_data.body)
                    .map(|body_node| (body_node.pos, body_node.end));

                let method_body = if is_async {
                    // Async method: use async transformer to build proper generator body
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                        hoisted_vars: Vec::new(),
                    }]
                } else {
                    let mut method_body = self.convert_block_body(method_data.body);

                    // Check if method needs `var _this = this;` capture
                    let needs_this_capture = self.constructor_needs_this_capture(method_data.body);
                    if needs_this_capture {
                        // Insert `var _this = this;` at the start of method body
                        method_body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
                    }

                    method_body
                };

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                // ClassName.prototype.methodName = function () { body };
                body.push(IRNode::PrototypeMethod {
                    class_name: self.class_name.clone(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    }),
                    leading_comment,
                    trailing_comment,
                });
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle accessor (getter/setter) - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    // Skip static/abstract/private (already filtered in first pass)
                    if self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword)
                        || self
                            .arena
                            .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();

                    // Skip if already emitted
                    if emitted_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = accessor_map.get(&accessor_name) {
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
                                IRNode::id(&self.class_name),
                                "prototype",
                            )),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                enumerable: false,
                                configurable: true,
                                trailing_comment: None,
                            },
                        });

                        let has_explicit_semicolon_member = class_data
                            .members
                            .nodes
                            .get(member_i + 1)
                            .and_then(|&idx| self.arena.get(idx))
                            .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
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

                        emitted_accessors.insert(accessor_name);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(accessor) = self.find_auto_accessor(member_idx) else {
                    continue;
                };
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                    || self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                    || is_private_identifier(self.arena, prop_data.name)
                {
                    continue;
                }

                let property_name = self.get_method_name_ir(prop_data.name);
                body.push(IRNode::DefineProperty {
                    target: Box::new(IRNode::prop(IRNode::id(&self.class_name), "prototype")),
                    property_name,
                    descriptor: IRPropertyDescriptor {
                        get: Some(Box::new(
                            self.build_auto_accessor_getter_function(&accessor.weakmap_name),
                        )),
                        set: Some(Box::new(
                            self.build_auto_accessor_setter_function(&accessor.weakmap_name),
                        )),
                        enumerable: false,
                        configurable: true,
                        trailing_comment: self.extract_trailing_comment_for_node(member_node),
                    },
                });
            } else if member_node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT {
                body.push(IRNode::EmptyStatement);
            }
        }
    }

    /// Build a getter function IR from an accessor node
    fn build_getter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let params = self.extract_parameters(&accessor_data.parameters);

        let body_source_range = self.arena.get(accessor_data.body).map(|n| (n.pos, n.end));

        let body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = self.convert_block_body(accessor_data.body);

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
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let mut params = self.extract_parameters(&accessor_data.parameters);

        let body_source_range = self.arena.get(accessor_data.body).map(|n| (n.pos, n.end));

        let mut body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = self.convert_block_body(accessor_data.body);

            // Check if setter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                // Insert `var _this = this;` at the start of setter body
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
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
            initializer: Some(Box::new(IRNode::Raw(format!(
                "var {loop_var} = {start_index}"
            )))),
            condition: Some(Box::new(IRNode::binary(
                IRNode::id(loop_var),
                "<",
                IRNode::prop(IRNode::id("arguments"), "length"),
            ))),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(loop_var)),
                operator: "++".to_string(),
            })),
            body: Box::new(IRNode::block(vec![assignment])),
        };

        body.insert(0, collect_rest);
        body.insert(0, IRNode::var_decl(rest_name, Some(IRNode::empty_array())));
    }

    /// Emit static members as IR.
    /// Returns deferred static block IIFEs (for classes with no non-block static members).
    pub(super) fn emit_static_members_ir(
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
                        && prop_data.initializer.is_some();
                }
            } else if (m_node.kind == syntax_kind_ext::GET_ACCESSOR
                || m_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(m_node)
            {
                return self
                    .arena
                    .has_modifier(&acc_data.modifiers, SyntaxKind::StaticKeyword)
                    && !self
                        .arena
                        .has_modifier(&acc_data.modifiers, SyntaxKind::AbstractKeyword)
                    && !is_private_identifier(self.arena, acc_data.name);
            }
            false
        });

        let mut deferred_static_blocks = Vec::new();

        // First pass: collect static accessors by name to combine getter/setter pairs
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        // Track which static accessor names we've emitted
        let mut emitted_static_accessors: FxHashSet<String> = FxHashSet::default();

        // Second pass: emit static members in source order
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Only static methods
                if !self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Check if async method (not generator)
                let is_async = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                    && !method_data.asterisk_token;

                let method_body = if is_async {
                    // Async method: use async transformer to build proper generator body
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                        hoisted_vars: Vec::new(),
                    }]
                } else {
                    // Check if this static method has arrow functions with class_alias
                    let class_alias = self.get_class_alias_for_static_method(method_data.body);
                    self.convert_block_body_with_alias(method_data.body, class_alias)
                };

                // Capture body source range for single-line detection
                let body_source_range = self
                    .arena
                    .get(method_data.body)
                    .map(|body_node| (body_node.pos, body_node.end));

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                // ClassName.methodName = function () { body };
                body.push(IRNode::StaticMethod {
                    class_name: self.class_name.clone(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    }),
                    leading_comment,
                    trailing_comment,
                });
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                // Only static properties
                if !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip abstract properties (they don't exist at runtime)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }

                // Skip `declare` properties — ambient/type-only declarations have no runtime representation
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }

                // Skip private
                if is_private_identifier(self.arena, prop_data.name) {
                    continue;
                }
                // Skip accessor fields - currently emitted via auto-accessor lowering
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                {
                    continue;
                }

                // Skip if no initializer
                if prop_data.initializer.is_none() {
                    continue;
                }

                if let Some(prop_name) = self.get_property_name_ir(prop_data.name) {
                    let target = match &prop_name {
                        PropertyNameIR::Identifier(n) => {
                            IRNode::prop(IRNode::id(&self.class_name), n)
                        }
                        PropertyNameIR::StringLiteral(s) => {
                            IRNode::elem(IRNode::id(&self.class_name), IRNode::string(s))
                        }
                        PropertyNameIR::NumericLiteral(n) => {
                            IRNode::elem(IRNode::id(&self.class_name), IRNode::number(n))
                        }
                        PropertyNameIR::Computed(expr_idx) => IRNode::elem(
                            IRNode::id(&self.class_name),
                            self.convert_expression(*expr_idx),
                        ),
                    };

                    // ClassName.prop = value;
                    body.push(IRNode::expr_stmt(IRNode::assign(
                        target,
                        self.convert_expression(prop_data.initializer),
                    )));
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Static block: wrap in IIFE to preserve block scoping
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| self.convert_statement(stmt_idx))
                        .collect();

                    let iife = IRNode::StaticBlockIIFE { statements };
                    if has_static_props {
                        // Inline: maintain initialization order with other static members
                        body.push(iife);
                    } else {
                        // Deferred: emit after the class IIFE
                        deferred_static_blocks.push(iife);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle static accessor - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node)
                    && self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    // Skip abstract/private
                    if self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();

                    // Skip if already emitted
                    if emitted_static_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = static_accessor_map.get(&accessor_name)
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
                            target: Box::new(IRNode::id(&self.class_name)),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                enumerable: false,
                                configurable: true,
                                trailing_comment: None,
                            },
                        });

                        emitted_static_accessors.insert(accessor_name);
                    }
                }
            }
        }

        deferred_static_blocks
    }

    /// Get method name as IR representation
    pub(super) fn get_method_name_ir(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return IRMethodName::Identifier(String::new());
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return IRMethodName::Computed(Box::new(
                    self.convert_expression(computed.expression),
                ));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return IRMethodName::Identifier(ident.escaped_text.clone());
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::StringLiteral(lit.text.clone());
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return IRMethodName::NumericLiteral(lit.text.clone());
        }

        IRMethodName::Identifier(String::new())
    }
}
